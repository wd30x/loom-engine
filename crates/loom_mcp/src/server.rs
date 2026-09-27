//! The MCP tools: one per `loom serve` method, same name, same params
//! (`docs/architecture.md` section 13). Descriptions (doc comments) are
//! what the AI reads.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::schemars::{self, JsonSchema};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::bridge::{Answer, Bridge, Outcome};

const INSTRUCTIONS: &str = "\
Drives a headless loom engine session (a `loom serve` process, started on the first call). \
Typical use: `describe` to list the input actions and axes, then `input` to set held input \
and `step` to advance time, then `state` (JSON state dump) or `screenshot` (PNG, returned \
inline and saved to a file). Time only advances on `step`, in fixed 1/60 s ticks, so the \
same calls always give the same results. `record` writes the input so far as a script that \
`loom run --headless --frames <tick> --input <script> --dump-state <file>` replays to the \
same state, which makes a regression test. Relative paths are relative to the repository root.";

/// The first line of a result after the previous session died.
const NEW_SESSION_NOTE: &str = "note: new loom session, tick starts at 0";

#[derive(Clone)]
pub struct LoomServer {
    bridge: Arc<Bridge>,
    /// Numbers the screenshots saved without a `path`, from 1.
    shots: Arc<AtomicU64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadSceneArgs {
    /// Scene file (RON) to load, relative to the repository root. Give either this or
    /// `source`; with neither, the built-in demo scene is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Scene text (RON) to load instead of a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputArgs {
    /// Actions to press (true) or release (false), by name. Unmentioned actions keep their state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<BTreeMap<String, bool>>,
    /// Axis values to hold, by name; 0 releases the axis. Unmentioned axes keep their value.
    /// Units are defined by the game (e.g. move_x/move_y in [-1, 1], look_x/look_y in radians
    /// per tick).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<BTreeMap<String, f64>>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepArgs {
    /// Number of 1/60 s ticks to run, at least 1. Default 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticks: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotArgs {
    /// PNG file to write, relative to the repository root; parent directories are created.
    /// Default: out/mcp/shot-<n>.png, numbered from 1 for each MCP server start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// [width, height] in pixels. Default: the session's size (1280x720 unless configured).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[u32; 2]>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordArgs {
    /// Input script file (RON) to write, relative to the repository root.
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TraceArgs {
    /// JSONL file to write the trace to, relative to the repository root; parent directories
    /// are created and an existing file is emptied. Omit it to stop tracing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[tool_router]
impl LoomServer {
    pub fn new(bridge: Bridge) -> Self {
        Self {
            bridge: Arc::new(bridge),
            shots: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Describe the session: tick rate (60 Hz), current tick, scene path (null for the
    /// built-in or inline scene), and the names of the input actions and axes, sorted.
    #[tool]
    async fn describe(&self) -> Result<CallToolResult, McpError> {
        Ok(self.forward("describe", Map::new()).await)
    }

    /// Load a scene from a file (`path`), from RON text (`source`), or the built-in demo
    /// scene (neither). Restarts at tick 0 with no held input and an empty recording. If the
    /// scene is invalid, the current session is kept unchanged.
    #[tool]
    async fn load_scene(
        &self,
        Parameters(args): Parameters<LoadSceneArgs>,
    ) -> Result<CallToolResult, McpError> {
        Ok(self.forward("load_scene", to_params(&args)).await)
    }

    /// Restart the current scene at tick 0, clearing the held input and the recording.
    #[tool]
    async fn reset(&self) -> Result<CallToolResult, McpError> {
        Ok(self.forward("reset", Map::new()).await)
    }

    /// Change the held input. Input is level-triggered: what you set stays in effect for every
    /// following tick until you change it again. Axis units are defined by the game; `describe`
    /// lists the action and axis names. If any name is unknown or any value invalid, nothing
    /// changes. Returns the full held input: pressed actions and non-zero axes.
    #[tool]
    async fn input(
        &self,
        Parameters(args): Parameters<InputArgs>,
    ) -> Result<CallToolResult, McpError> {
        Ok(self.forward("input", to_params(&args)).await)
    }

    /// Advance the simulation by N fixed 1/60 s ticks using the held input. Returns the tick
    /// reached and the events the game emitted during those ticks (hits, dodged swings,
    /// dropped presses, actions starting and ending, lock-on changes, ...), in order, each
    /// with the tick it happened on.
    #[tool]
    async fn step(
        &self,
        Parameters(args): Parameters<StepArgs>,
    ) -> Result<CallToolResult, McpError> {
        Ok(self.forward("step", to_params(&args)).await)
    }

    /// The simulation state as JSON: tick, sim_time, actions and axes of the last tick, the
    /// local transform of every scene node (by path), and the game state. Same content as
    /// `loom run --dump-state`.
    #[tool]
    async fn state(&self) -> Result<CallToolResult, McpError> {
        Ok(self.forward("state", Map::new()).await)
    }

    /// Render the current state and save it as PNG; the image is also returned inline. The
    /// first screenshot of a session creates the renderer (a few hundred ms), later ones are
    /// fast. Returns path, size, GPU adapter and render_ms.
    #[tool]
    async fn screenshot(
        &self,
        Parameters(mut args): Parameters<ScreenshotArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = args
            .path
            .get_or_insert_with(|| {
                let n = self.shots.fetch_add(1, Ordering::Relaxed) + 1;
                format!("out/mcp/shot-{n}.png")
            })
            .clone();
        let answer = self.bridge.call("screenshot", to_params(&args)).await;
        let Answer {
            new_session,
            outcome: Outcome::Result(raw),
        } = answer
        else {
            return Ok(tool_result(answer));
        };
        let text = with_note(new_session, raw);
        Ok(match std::fs::read(&path) {
            Ok(png) => CallToolResult::success(vec![
                ContentBlock::text(text),
                ContentBlock::image(
                    base64::engine::general_purpose::STANDARD.encode(png),
                    "image/png",
                ),
            ]),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(format!(
                "{text}\nthe screenshot was saved, but reading {path} back failed: {e}"
            ))]),
        })
    }

    /// Write the input since the last load_scene/reset (or session start) as an input script
    /// (RON). Replay it with `loom run --headless --frames <tick> --input <path>` to reach the
    /// same state.
    #[tool]
    async fn record(
        &self,
        Parameters(args): Parameters<RecordArgs>,
    ) -> Result<CallToolResult, McpError> {
        Ok(self.forward("record", to_params(&args)).await)
    }

    /// Start writing a trace to `path`: one JSON line per tick, `{"tick", "events", "state"}`
    /// with the full state dump, starting with the current tick; or stop it (no `path`).
    /// load_scene and reset start the file over from tick 0. Returns the path and the tick of
    /// the first line, or on stopping the path (null if none) and the number of lines.
    #[tool]
    async fn trace(
        &self,
        Parameters(args): Parameters<TraceArgs>,
    ) -> Result<CallToolResult, McpError> {
        Ok(self.forward("trace", to_params(&args)).await)
    }
}

impl LoomServer {
    async fn forward(&self, method: &str, params: Map<String, Value>) -> CallToolResult {
        tool_result(self.bridge.call(method, params).await)
    }

    /// Ends the `loom serve` session, if one is running.
    pub async fn shutdown(&self) {
        self.bridge.shutdown().await;
    }
}

#[tool_handler]
impl ServerHandler for LoomServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("loom-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// The params to send: only the fields that were given.
fn to_params(args: &impl Serialize) -> Map<String, Value> {
    match serde_json::to_value(args) {
        Ok(Value::Object(params)) => Some(params),
        _ => None,
    }
    .expect("tool argument structs serialize to a JSON object")
}

fn tool_result(answer: Answer) -> CallToolResult {
    let Answer {
        new_session,
        outcome,
    } = answer;
    match outcome {
        Outcome::Result(raw) => {
            CallToolResult::success(vec![ContentBlock::text(with_note(new_session, raw))])
        }
        Outcome::Error { code, message } => CallToolResult::error(vec![ContentBlock::text(
            with_note(new_session, format!("error {code}: {message}")),
        )]),
        Outcome::Failed(message) => {
            CallToolResult::error(vec![ContentBlock::text(with_note(new_session, message))])
        }
    }
}

fn with_note(new_session: bool, text: String) -> String {
    if new_session {
        format!("{NEW_SESSION_NOTE}\n{text}")
    } else {
        text
    }
}
