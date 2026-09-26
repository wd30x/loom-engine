//! `loom serve`: a headless remote-control session over stdin and stdout,
//! JSON-RPC 2.0 with one message per line (`docs/architecture.md` section
//! 13). Time only advances on `step`, so the same requests always get the
//! same replies, apart from `render_ms` and `adapter`.
//!
//! Replies are serialized from typed structs, never through
//! `serde_json::Value`: `Value` would sort object keys and widen `f32` to
//! `f64`, and `state` must match `--dump-state` number for number.

use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use loom_core::TICK_HZ;
use loom_core::input::ActionSet;
use loom_render::{AdapterSummary, Renderer};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::report::RunAdapter;
use crate::run::save_screenshot;
use crate::sim::{LoadedScene, Sim};
use crate::{ServeArgs, Size};

/// Runs a session until stdin ends or a `quit` request. A bad `--scene` is
/// an error before any request is read; after that, errors are replies and
/// the session goes on. Only failing to read stdin or write stdout ends it
/// with an error.
pub(crate) fn serve(args: &ServeArgs) -> Result<()> {
    let sim = match &args.scene {
        Some(path) => sim_from_file(path)?,
        None => sim_builtin()?,
    };
    let mut session = Session {
        sim,
        renderer: None,
        size: args.size,
        quit: false,
    };
    log::info!("serving requests on stdin");
    let result = session.run();
    if let Some(renderer) = session.renderer.take() {
        renderer.shutdown();
    }
    result
}

struct Session {
    sim: Sim,
    /// Created by the first `screenshot`; a session without one never
    /// touches the GPU.
    renderer: Option<Renderer>,
    /// `--size`: the screenshot size when a request gives none.
    size: Size,
    /// Set by `quit`: stop after sending its reply.
    quit: bool,
}

impl Session {
    fn run(&mut self) -> Result<()> {
        for line in io::stdin().lock().lines() {
            // `lines` drops the `\n` and a `\r` before it.
            let reply = match line {
                Ok(line) => self.respond(&line),
                Err(e) if e.kind() == io::ErrorKind::InvalidData => failure(
                    &Value::Null,
                    &ServeError::Parse(anyhow!(e).context("the line is not UTF-8")),
                ),
                Err(e) => return Err(e).context("reading a request from stdin"),
            };
            send(&reply).context("writing a reply to stdout")?;
            if self.quit {
                break;
            }
        }
        Ok(())
    }

    /// The reply line to one request line.
    fn respond(&mut self, line: &str) -> String {
        let request = match parse_request(line) {
            Ok(request) => request,
            Err((id, e)) => return failure(&id, &e),
        };
        match self.handle(&request.id, &request.method, request.params) {
            Ok(reply) => reply,
            Err(e) => failure(&request.id, &e),
        }
    }

    fn handle(
        &mut self,
        id: &Value,
        method: &str,
        params: Option<Value>,
    ) -> Result<String, ServeError> {
        Ok(match method {
            "describe" => {
                let NoParams {} = parse_params(params)?;
                let bindings = self.sim.bindings();
                let mut actions: Vec<&str> = bindings.actions().collect();
                let mut axes: Vec<&str> = bindings.axes().collect();
                actions.sort_unstable();
                axes.sort_unstable();
                reply(
                    id,
                    Describe {
                        tick_hz: TICK_HZ,
                        tick: self.tick(),
                        scene: self.sim.scene_path(),
                        actions,
                        axes,
                    },
                )
            }
            "load_scene" => {
                let LoadSceneParams { path, source } = parse_params(params)?;
                let sim = match (path, source) {
                    (Some(_), Some(_)) => {
                        return Err(ServeError::InvalidParams(anyhow!(
                            "give either path or source, not both"
                        )));
                    }
                    (Some(path), None) => sim_from_file(Path::new(&path)),
                    (None, Some(source)) => sim_from_source(source),
                    (None, None) => sim_builtin(),
                }
                .map_err(ServeError::Engine)?;
                // Only now: a failed load keeps the session as it was.
                self.sim = sim;
                reply(
                    id,
                    LoadSceneResult {
                        tick: self.tick(),
                        scene: self.sim.scene_path(),
                    },
                )
            }
            "reset" => {
                let NoParams {} = parse_params(params)?;
                self.sim = self.restarted().map_err(ServeError::Engine)?;
                reply(id, TickResult { tick: self.tick() })
            }
            "input" => {
                let InputParams { actions, axes } = parse_params(params)?;
                let held = self
                    .sim
                    .set_input(&actions, &axes)
                    .map_err(ServeError::InvalidParams)?;
                reply(
                    id,
                    HeldInput {
                        actions: &held.actions,
                        axes: &held.axes,
                    },
                )
            }
            "step" => {
                let StepParams { ticks } = parse_params(params)?;
                if ticks == 0 {
                    return Err(ServeError::InvalidParams(anyhow!(
                        "ticks must be at least 1"
                    )));
                }
                for _ in 0..ticks {
                    self.sim.tick();
                }
                reply(id, TickResult { tick: self.tick() })
            }
            "state" => {
                let NoParams {} = parse_params(params)?;
                reply(id, self.sim.state_dump())
            }
            "screenshot" => {
                let start = Instant::now();
                let ScreenshotParams { path, size } = parse_params(params)?;
                let (width, height) = match size {
                    None => (self.size.width, self.size.height),
                    Some([w, h]) if w > 0 && h > 0 => (w, h),
                    Some(_) => {
                        return Err(ServeError::InvalidParams(anyhow!(
                            "size must be two positive integers"
                        )));
                    }
                };
                let (path, adapter) = self
                    .screenshot(&path, width, height)
                    .map_err(ServeError::Engine)?;
                let render_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                reply(
                    id,
                    ScreenshotResult {
                        path,
                        size: [width, height],
                        adapter: adapter.into(),
                        render_ms,
                    },
                )
            }
            "record" => {
                let RecordParams { path } = parse_params(params)?;
                let path = self.sim.write_record(&path).map_err(ServeError::Engine)?;
                reply(
                    id,
                    RecordResult {
                        path,
                        ticks: self.tick(),
                    },
                )
            }
            "quit" => {
                let NoParams {} = parse_params(params)?;
                self.quit = true;
                reply(id, Empty {})
            }
            _ => {
                return Err(ServeError::MethodNotFound(anyhow!(
                    "unknown method {method:?}"
                )));
            }
        })
    }

    fn tick(&self) -> u64 {
        self.sim.game().clock().tick()
    }

    /// A fresh session on the current scene, from the text it was loaded
    /// from (not the file again, which may have changed since).
    fn restarted(&self) -> Result<Sim> {
        let loaded = match self.sim.scene_source() {
            Some(source) => {
                LoadedScene::from_source(source).context("reloading the current scene")?
            }
            None => LoadedScene::builtin()?,
        };
        Ok(Sim::held(
            loaded,
            self.sim.scene_path(),
            self.sim.scene_source().map(str::to_owned),
        ))
    }

    /// Renders the current state at `width` x `height` and saves it as PNG:
    /// the same steps as `loom run --headless --screenshot`, so the same
    /// bytes. Creates the renderer on first use.
    fn screenshot(
        &mut self,
        path: &Path,
        width: u32,
        height: u32,
    ) -> Result<(String, AdapterSummary)> {
        if self.renderer.is_none() {
            self.renderer = Some(Renderer::new_headless(width, height)?);
        }
        let renderer = self.renderer.as_mut().expect("created just above");
        renderer.resize(width, height)?;
        self.sim.render(renderer)?;
        let saved = save_screenshot(renderer, path)?;
        Ok((saved, renderer.adapter_summary()))
    }
}

/// A session on the scene file at `path`, remembering its text for `reset`.
fn sim_from_file(path: &Path) -> Result<Sim> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("reading scene {}", path.display()))?;
    let loaded = LoadedScene::from_source(&source)
        .with_context(|| format!("invalid scene {}", path.display()))?;
    Ok(Sim::held(
        loaded,
        Some(path.display().to_string()),
        Some(source),
    ))
}

/// A session on scene text given inline.
fn sim_from_source(source: String) -> Result<Sim> {
    let loaded = LoadedScene::from_source(&source).context("invalid scene source")?;
    Ok(Sim::held(loaded, None, Some(source)))
}

/// A session on the built-in demo scene.
fn sim_builtin() -> Result<Sim> {
    Ok(Sim::held(LoadedScene::builtin()?, None, None))
}

// ---------------------------------------------------------------- protocol

/// A failed request. The variant gives the JSON-RPC error code; the message
/// is the error chain, formatted like the CLI's error JSON (`{:#}`).
#[derive(Debug)]
enum ServeError {
    /// The line is not JSON.
    Parse(anyhow::Error),
    /// JSON, but not a request.
    InvalidRequest(anyhow::Error),
    MethodNotFound(anyhow::Error),
    InvalidParams(anyhow::Error),
    /// The engine failed: scene, file or GPU.
    Engine(anyhow::Error),
}

impl ServeError {
    fn code(&self) -> i32 {
        match self {
            Self::Parse(_) => -32700,
            Self::InvalidRequest(_) => -32600,
            Self::MethodNotFound(_) => -32601,
            Self::InvalidParams(_) => -32602,
            Self::Engine(_) => -32000,
        }
    }

    fn message(&self) -> String {
        let (Self::Parse(e)
        | Self::InvalidRequest(e)
        | Self::MethodNotFound(e)
        | Self::InvalidParams(e)
        | Self::Engine(e)) = self;
        format!("{e:#}")
    }
}

struct Request {
    /// A number or a string.
    id: Value,
    method: String,
    params: Option<Value>,
}

/// Checks that `line` is a JSON-RPC 2.0 request. On error, also returns the
/// id to reply with: the request's own if it has a valid one, else `null`.
fn parse_request(line: &str) -> Result<Request, (Value, ServeError)> {
    let invalid =
        |id: Value, message: &str| (id, ServeError::InvalidRequest(anyhow!(message.to_owned())));
    let value: Value = serde_json::from_str(line).map_err(|e| {
        (
            Value::Null,
            ServeError::Parse(anyhow!(e).context("the line is not valid JSON")),
        )
    })?;
    let mut request = match value {
        Value::Object(request) => request,
        Value::Array(_) => {
            return Err(invalid(
                Value::Null,
                "the request is an array: batches are not supported",
            ));
        }
        _ => return Err(invalid(Value::Null, "the request is not a JSON object")),
    };
    // The id first, so that the other errors can carry it.
    let id = match request.remove("id") {
        Some(id @ (Value::Number(_) | Value::String(_))) => id,
        Some(_) => return Err(invalid(Value::Null, "id must be a number or a string")),
        None => {
            return Err(invalid(
                Value::Null,
                "the request has no id: notifications are not supported",
            ));
        }
    };
    if request.remove("jsonrpc") != Some(Value::String("2.0".to_owned())) {
        return Err(invalid(id, "jsonrpc must be \"2.0\""));
    }
    let method = match request.remove("method") {
        Some(Value::String(method)) => method,
        Some(_) => return Err(invalid(id, "method must be a string")),
        None => return Err(invalid(id, "the request has no method")),
    };
    let params = request.remove("params");
    // Like `params`, a misspelled member (say `param`) must not be ignored.
    if let Some(member) = request.keys().next() {
        let message = format!("unknown member {member:?} in the request");
        return Err(invalid(id, &message));
    }
    Ok(Request { id, method, params })
}

/// The params of a method; omitted params are `{}`. Unknown fields are
/// errors (every params struct denies them).
fn parse_params<T: DeserializeOwned>(params: Option<Value>) -> Result<T, ServeError> {
    let params = match params {
        None => Value::Object(serde_json::Map::new()),
        Some(params @ Value::Object(_)) => params,
        Some(_) => {
            return Err(ServeError::InvalidParams(anyhow!(
                "params must be a JSON object"
            )));
        }
    };
    serde_json::from_value(params)
        .map_err(|e| ServeError::InvalidParams(anyhow!(e).context("invalid params")))
}

/// A success reply line: `jsonrpc`, `id`, `result`, in that order.
fn reply(id: &Value, result: impl Serialize) -> String {
    serde_json::to_string(&Success {
        jsonrpc: "2.0",
        id,
        result,
    })
    .expect("reply types contain only plain data")
}

/// An error reply line: `jsonrpc`, `id`, `error`, in that order.
fn failure(id: &Value, error: &ServeError) -> String {
    serde_json::to_string(&Failure {
        jsonrpc: "2.0",
        id,
        error: ErrorObject {
            code: error.code(),
            message: error.message(),
        },
    })
    .expect("reply types contain only plain data")
}

/// Writes one reply line to stdout and flushes it. The only function of
/// `loom serve` that writes to stdout (a startup error before the session
/// still goes through `report::emit`).
fn send(line: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(line.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

#[derive(Serialize)]
struct Success<'a, T: Serialize> {
    jsonrpc: &'static str,
    id: &'a Value,
    result: T,
}

#[derive(Serialize)]
struct Failure<'a> {
    jsonrpc: &'static str,
    id: &'a Value,
    error: ErrorObject,
}

#[derive(Serialize)]
struct ErrorObject {
    code: i32,
    message: String,
}

// ---------------------------------------------------------------- params

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoParams {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadSceneParams {
    path: Option<String>,
    source: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputParams {
    #[serde(default)]
    actions: BTreeMap<String, bool>,
    #[serde(default)]
    axes: BTreeMap<String, f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepParams {
    #[serde(default = "one_tick")]
    ticks: u64,
}

fn one_tick() -> u64 {
    1
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotParams {
    path: PathBuf,
    size: Option<[u32; 2]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordParams {
    path: PathBuf,
}

// ---------------------------------------------------------------- results

#[derive(Serialize)]
struct Describe<'a> {
    tick_hz: u32,
    tick: u64,
    scene: Option<String>,
    /// Sorted by name.
    actions: Vec<&'a str>,
    /// Sorted by name.
    axes: Vec<&'a str>,
}

#[derive(Serialize)]
struct LoadSceneResult {
    tick: u64,
    scene: Option<String>,
}

#[derive(Serialize)]
struct TickResult {
    tick: u64,
}

/// The held input: pressed actions (sorted) and the non-zero axes.
#[derive(Serialize)]
struct HeldInput<'a> {
    actions: &'a ActionSet,
    axes: &'a BTreeMap<String, f32>,
}

#[derive(Serialize)]
struct ScreenshotResult {
    path: String,
    size: [u32; 2],
    adapter: RunAdapter,
    /// Wall clock for this request, including creating the renderer the
    /// first time. Diagnosis only.
    render_ms: u64,
}

#[derive(Serialize)]
struct RecordResult {
    path: String,
    ticks: u64,
}

#[derive(Serialize)]
struct Empty {}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_of(line: &str) -> (Value, i32) {
        let (id, e) = parse_request(line).err().expect("the line is rejected");
        (id, e.code())
    }

    #[test]
    fn misspelled_request_member_is_an_invalid_request_carrying_the_id() {
        let (id, code) =
            error_of(r#"{"jsonrpc":"2.0","id":7,"method":"step","param":{"ticks":60}}"#);
        assert_eq!((id, code), (Value::from(7), -32600));
    }

    #[test]
    fn request_without_a_valid_id_is_answered_with_a_null_id() {
        for line in [
            r#"{"jsonrpc":"2.0","method":"describe"}"#,
            r#"{"jsonrpc":"2.0","id":null,"method":"describe"}"#,
            r#"{"jsonrpc":"2.0","id":[1],"method":"describe"}"#,
            r#"[{"jsonrpc":"2.0","id":1,"method":"describe"}]"#,
            r#""describe""#,
        ] {
            assert_eq!(error_of(line), (Value::Null, -32600), "{line}");
        }
        assert_eq!(error_of("{"), (Value::Null, -32700));
    }

    #[test]
    fn params_that_are_not_an_object_are_invalid_params() {
        for params in [Value::from(vec![5]), Value::Null, Value::from(5)] {
            let err = parse_params::<StepParams>(Some(params))
                .err()
                .expect("rejected");
            assert_eq!(err.code(), -32602);
        }
        let StepParams { ticks } = parse_params(None).expect("omitted params are {}");
        assert_eq!(ticks, 1);
    }
}
