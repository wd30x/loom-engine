//! `loom-mcp` over its stdio MCP transport, driving a real `loom serve`
//! (`docs/architecture.md` section 13). No GPU: screenshots are covered by
//! smoke (H7).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(60);

/// `loom` next to `loom-mcp` in the same target directory.
fn loom_exe() -> PathBuf {
    let loom = Path::new(env!("CARGO_BIN_EXE_loom-mcp"))
        .parent()
        .expect("the loom-mcp binary is in a directory")
        .join(format!("loom{}", std::env::consts::EXE_SUFFIX));
    assert!(
        loom.is_file(),
        "{} not found: run `cargo build -p loom_engine` first (cargo test --workspace builds it)",
        loom.display()
    );
    loom
}

/// A running `loom-mcp` with its stdout lines and stderr end on channels.
struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    /// Receives once stderr is closed by every process holding it: loom-mcp
    /// and the `loom serve` child, which inherits it.
    stderr_closed: Receiver<()>,
    next_id: u64,
}

impl Mcp {
    fn start(loom: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_loom-mcp"))
            .arg("--loom")
            .arg(loom)
            .env("RUST_LOG", "off")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("loom-mcp can be started");
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let (line_tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let (closed_tx, stderr_closed) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = stderr.read_to_end(&mut Vec::new());
            let _ = closed_tx.send(());
        });
        let mut mcp = Self {
            stdin: child.stdin.take(),
            child,
            lines,
            stderr_closed,
            next_id: 1,
        };
        let init = mcp.request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "loom_mcp tests", "version": "0"}
            }),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "loom-mcp", "{init}");
        mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        mcp
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin is still open");
        writeln!(stdin, "{message}").expect("loom-mcp reads its stdin");
        stdin.flush().expect("loom-mcp reads its stdin");
    }

    /// Sends a request and returns the reply with the same id.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self
                .lines
                .recv_timeout(left)
                .unwrap_or_else(|e| panic!("no reply to {method} ({e})"));
            let reply: Value = serde_json::from_str(&line).expect("loom-mcp writes JSON lines");
            if reply["id"] == id {
                return reply;
            }
        }
    }

    /// Calls a tool; returns (isError, text of the first content).
    fn call(&mut self, tool: &str, arguments: Value) -> (bool, String) {
        let reply = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        let result = &reply["result"];
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("no text content: {reply}"))
            .to_owned();
        (result["isError"] == true, text)
    }

    /// Closes stdin and waits for loom-mcp, and every process sharing its
    /// stderr, to exit. Returns loom-mcp's exit code.
    fn close(mut self) -> i32 {
        drop(self.stdin.take());
        self.stderr_closed
            .recv_timeout(TIMEOUT)
            .expect("loom-mcp and its loom serve child exit after stdin closes");
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("loom-mcp can be waited for") {
                return status.code().expect("loom-mcp exits normally");
            }
            assert!(Instant::now() < deadline, "loom-mcp did not exit");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The raw `result` text of the last reply `loom serve` gives to `requests`
/// (numbered 1, 2, ...).
fn serve_result(loom: &Path, requests: &[Value]) -> String {
    let mut child = Command::new(loom)
        .arg("serve")
        .env("RUST_LOG", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("loom serve can be started");
    let mut stdin = child.stdin.take().expect("stdin is piped");
    for (i, params) in requests.iter().enumerate() {
        let (method, params) = (&params[0], &params[1]);
        let line = json!({"jsonrpc": "2.0", "id": i + 1, "method": method, "params": params});
        writeln!(stdin, "{line}").expect("loom serve reads its stdin");
    }
    drop(stdin);
    let out = child.wait_with_output().expect("loom serve exits");
    let stdout = String::from_utf8(out.stdout).expect("UTF-8");
    let last = stdout.lines().last().expect("loom serve replied");
    last.strip_prefix(&format!(
        r#"{{"jsonrpc":"2.0","id":{},"result":"#,
        requests.len()
    ))
    .and_then(|rest| rest.strip_suffix('}'))
    .unwrap_or_else(|| panic!("unexpected reply: {last}"))
    .to_owned()
}

#[test]
fn tools_forward_to_loom_serve_and_keep_its_numbers_exact() {
    let loom = loom_exe();
    let mut mcp = Mcp::start(&loom);

    let list = mcp.request("tools/list", json!({}));
    let mut names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "describe",
            "input",
            "load_scene",
            "record",
            "reset",
            "screenshot",
            "state",
            "step",
            "trace"
        ]
    );

    assert_eq!(
        mcp.call("input", json!({"axes": {"move_y": 1.0}})),
        (false, r#"{"actions":[],"axes":{"move_y":1.0}}"#.to_owned())
    );
    assert_eq!(
        mcp.call("step", json!({"ticks": 30})),
        (false, r#"{"tick":30,"events":[]}"#.to_owned())
    );
    let (is_error, state) = mcp.call("state", json!({}));
    assert!(!is_error, "{state}");
    let parsed: Value = serde_json::from_str(&state).expect("state is JSON");
    assert_eq!(parsed["tick"], 30);
    let direct = serve_result(
        &loom,
        &[
            json!(["input", {"axes": {"move_y": 1.0}}]),
            json!(["step", {"ticks": 30}]),
            json!(["state", {}]),
        ],
    );
    assert_eq!(state, direct);

    let (is_error, text) = mcp.call("step", json!({"ticks": 0}));
    assert!(is_error, "{text}");
    assert!(text.starts_with("error -32602: "), "{text}");

    assert_eq!(mcp.close(), 0);
}

#[test]
fn missing_loom_executable_is_reported_as_a_tool_error() {
    let missing = std::env::temp_dir()
        .join("no-such-dir-for-loom-mcp")
        .join("loom.exe");
    let mut mcp = Mcp::start(&missing);
    for _ in 0..2 {
        // Each call tries again, and fails the same way.
        let (is_error, text) = mcp.call("describe", json!({}));
        assert!(is_error, "{text}");
        assert!(text.starts_with("starting loom serve ("), "{text}");
        assert!(!text.contains("note: new loom session"), "{text}");
    }
    assert_eq!(mcp.close(), 0);
}

#[test]
fn trace_tool_starts_a_trace_file_and_stops_it_without_a_path() {
    let loom = loom_exe();
    let dir = std::env::temp_dir().join("loom-mcp-tests").join("trace");
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("t.jsonl");
    let quoted = serde_json::to_string(&path.display().to_string()).expect("a string");
    let mut mcp = Mcp::start(&loom);
    assert_eq!(
        mcp.call("trace", json!({"path": path})),
        (false, format!(r#"{{"path":{quoted},"tick":0}}"#))
    );
    assert!(!mcp.call("step", json!({"ticks": 2})).0);
    assert_eq!(
        mcp.call("trace", json!({})),
        (false, format!(r#"{{"path":{quoted},"lines":3}}"#))
    );
    let text = std::fs::read_to_string(&path).expect("the trace was written");
    let ticks: Vec<u64> = text
        .lines()
        .map(|line| {
            let line: Value = serde_json::from_str(line).expect("each line is JSON");
            line["tick"].as_u64().expect("a tick")
        })
        .collect();
    assert_eq!(ticks, [0, 1, 2]);
    // Nothing left to stop.
    assert_eq!(
        mcp.call("trace", json!({})),
        (false, r#"{"path":null,"lines":0}"#.to_owned())
    );
    assert_eq!(mcp.close(), 0);
}

#[test]
fn screenshot_tool_takes_debug_draw_as_true_or_names_and_forwards_it() {
    let loom = loom_exe();
    let mut mcp = Mcp::start(&loom);
    let list = mcp.request("tools/list", json!({}));
    let screenshot = list["result"]["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .find(|t| t["name"] == "screenshot")
        .expect("a screenshot tool");
    let schema = screenshot["inputSchema"].to_string();
    assert!(schema.contains("debug_draw"), "{schema}");
    assert!(
        schema.contains("boolean") && schema.contains("array"),
        "{schema}"
    );
    // A bad name reaches loom serve, which rejects it before rendering:
    // no GPU needed.
    let path = std::env::temp_dir().join("loom-mcp-tests-never.png");
    let (is_error, text) = mcp.call(
        "screenshot",
        json!({"path": path, "debug_draw": ["hitboxes"]}),
    );
    assert!(is_error, "{text}");
    assert!(
        text.starts_with(r#"error -32602: unknown debug draw category "hitboxes""#),
        "{text}"
    );
    assert_eq!(mcp.close(), 0);
}
