//! The `loom serve` child process: started by the first call, one request
//! at a time, and started again on the call after it dies.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::reply::{Reply, parse_reply};

/// How long a child gets to exit by itself (after EOF on its stdin, or
/// after its stdout ended) before it is killed.
const EXIT_GRACE: Duration = Duration::from_secs(5);

/// What one call produced.
pub struct Answer {
    /// This call started a new session because the previous one ended
    /// unexpectedly: its state (tick, held input, scene) is gone.
    pub new_session: bool,
    pub outcome: Outcome,
}

pub enum Outcome {
    /// The raw text of the reply's `result`.
    Result(String),
    /// `loom serve` answered with an error.
    Error { code: i64, message: String },
    /// No answer: the child could not be started, or died, or broke the
    /// protocol.
    Failed(String),
}

pub struct Bridge {
    loom: PathBuf,
    /// `--size`, passed through to `loom serve`.
    size: Option<String>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    session: Option<Session>,
    /// The last session ended unexpectedly; the next one to start says so.
    lost: bool,
}

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Bridge {
    pub fn new(loom: PathBuf, size: Option<String>) -> Self {
        Self {
            loom,
            size,
            state: Mutex::new(State::default()),
        }
    }

    /// Sends one request and waits for its reply. `params` holds only the
    /// fields the caller gave; empty params are left out. Calls queue up:
    /// the child sees one request at a time.
    pub async fn call(&self, method: &str, params: Map<String, Value>) -> Answer {
        let mut state = self.state.lock().await;
        let mut new_session = false;
        if state.session.is_none() {
            match self.start() {
                Ok(session) => {
                    state.session = Some(session);
                    new_session = std::mem::take(&mut state.lost);
                }
                Err(e) => {
                    return Answer {
                        new_session,
                        outcome: Outcome::Failed(format!("{e:#}")),
                    };
                }
            }
        }
        let session = state.session.as_mut().expect("started above");
        let outcome = match session.exchange(method, params).await {
            Ok(outcome) => outcome,
            Err(broken) => {
                let session = state.session.take().expect("checked above");
                state.lost = true;
                let status = session.finish().await;
                Outcome::Failed(match broken {
                    Broken::Gone => format!(
                        "loom serve exited unexpectedly ({status}); the next call starts a new session"
                    ),
                    Broken::Protocol(why) => format!(
                        "loom serve broke the protocol ({why}); it was stopped ({status}) and the next call starts a new session"
                    ),
                })
            }
        };
        Answer {
            new_session,
            outcome,
        }
    }

    /// Ends the session, if any: closes the child's stdin so that it exits
    /// on EOF, killing it if it does not within [`EXIT_GRACE`].
    pub async fn shutdown(&self) {
        if let Some(session) = self.state.lock().await.session.take() {
            let status = session.finish().await;
            log::info!("loom serve ended ({status})");
        }
    }

    fn start(&self) -> Result<Session> {
        let mut command = Command::new(&self.loom);
        command.arg("serve");
        if let Some(size) = &self.size {
            command.args(["--size", size]);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("starting loom serve ({})", self.loom.display()))?;
        log::info!("started loom serve ({})", self.loom.display());
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        Ok(Session {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }
}

/// Why a session cannot go on.
enum Broken {
    /// Its stdin or stdout closed: the process has exited or is exiting.
    Gone,
    /// It wrote something that is not the reply to the request.
    Protocol(String),
}

impl Session {
    async fn exchange(
        &mut self,
        method: &str,
        params: Map<String, Value>,
    ) -> Result<Outcome, Broken> {
        let id = self.next_id;
        self.next_id += 1;
        let mut request = Map::new();
        request.insert("jsonrpc".to_owned(), "2.0".into());
        request.insert("id".to_owned(), id.into());
        request.insert("method".to_owned(), method.into());
        if !params.is_empty() {
            request.insert("params".to_owned(), Value::Object(params));
        }
        let mut line = Value::Object(request).to_string();
        line.push('\n');
        if let Err(e) = self.write(&line).await {
            log::warn!("writing to loom serve: {e}");
            return Err(Broken::Gone);
        }
        let mut reply = String::new();
        match self.stdout.read_line(&mut reply).await {
            Ok(0) => return Err(Broken::Gone),
            Ok(_) => {}
            Err(e) => {
                log::warn!("reading from loom serve: {e}");
                return Err(Broken::Gone);
            }
        }
        let reply = reply.trim_end_matches(['\n', '\r']);
        match parse_reply(reply, id) {
            Ok(Reply::Result(raw)) => Ok(Outcome::Result(raw.to_owned())),
            Ok(Reply::Error { code, message }) => Ok(Outcome::Error { code, message }),
            Err(why) => Err(Broken::Protocol(why)),
        }
    }

    async fn write(&mut self, line: &str) -> std::io::Result<()> {
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await
    }

    /// Closes stdin and waits for the child to exit, killing it after
    /// [`EXIT_GRACE`]. Returns its exit status, for messages.
    async fn finish(self) -> String {
        let Self {
            mut child, stdin, ..
        } = self;
        drop(stdin);
        let status = match tokio::time::timeout(EXIT_GRACE, child.wait()).await {
            Ok(status) => status,
            Err(_) => {
                log::warn!("loom serve did not exit within {EXIT_GRACE:?}; killing it");
                if let Err(e) = child.start_kill() {
                    log::warn!("killing loom serve: {e}");
                }
                child.wait().await
            }
        };
        match status {
            Ok(status) => match status.code() {
                Some(code) => format!("exit code {code}"),
                None => "no exit code".to_owned(),
            },
            Err(e) => format!("exit status unknown: {e}"),
        }
    }
}
