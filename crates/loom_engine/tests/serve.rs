//! `loom serve` over real pipes (`docs/architecture.md` section 13). No GPU:
//! screenshots are checked by smoke (H5, H6).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const DEMO_SCENE: &str = include_str!("../../loom_sim/src/demo.scene.ron");

struct Session {
    code: i32,
    /// stdout, one reply per line.
    lines: Vec<String>,
}

/// Starts `loom serve`, writes `requests` one per line, closes stdin and
/// collects every reply until the process exits.
fn serve_raw(requests: &[String]) -> Session {
    let mut child = Command::new(env!("CARGO_BIN_EXE_loom"))
        .arg("serve")
        .env("RUST_LOG", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the loom binary can be started");
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input: String = requests.iter().map(|r| format!("{r}\n")).collect();
    // A separate thread, so that a full stdout pipe cannot block the writes.
    // After `quit` the pipe may be closed: later writes may fail.
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let out = child
        .wait_with_output()
        .expect("loom serve can be waited for");
    writer.join().expect("the writer thread does not panic");
    Session {
        code: out
            .status
            .code()
            .expect("loom exits normally, not by a signal"),
        lines: String::from_utf8(out.stdout)
            .expect("stdout is UTF-8")
            .lines()
            .map(str::to_owned)
            .collect(),
    }
}

/// [`serve_raw`], checking the exit code 0 and one reply per request.
/// Returns the parsed replies.
fn serve(requests: &[String]) -> Vec<Value> {
    let session = serve_raw(requests);
    assert_eq!(session.code, 0, "{:?}", session.lines);
    assert_eq!(
        session.lines.len(),
        requests.len(),
        "one reply per request: {:?}",
        session.lines
    );
    session
        .lines
        .iter()
        .map(|l| serde_json::from_str(l).expect("every reply is JSON"))
        .collect()
}

/// A request line with a numeric id.
fn req(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn result(reply: &Value) -> &Value {
    assert!(reply.get("error").is_none(), "unexpected error: {reply}");
    &reply["result"]
}

fn error_code(reply: &Value) -> i64 {
    reply["error"]["code"]
        .as_i64()
        .unwrap_or_else(|| panic!("expected an error: {reply}"))
}

/// A fresh, empty per-test directory (tests run in parallel).
fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("old test directory can be removed");
    }
    fs::create_dir_all(&dir).expect("test directory can be created");
    dir
}

fn path_str(path: &std::path::Path) -> &str {
    path.to_str().expect("test paths are UTF-8")
}

#[test]
fn describe_lists_the_demo_bindings_sorted_without_a_scene_path() {
    let replies = serve(&[req(1, "describe", json!({}))]);
    let r = result(&replies[0]);
    let bindings = loom_sim::demo_bindings();
    let mut actions: Vec<&str> = bindings.actions().collect();
    let mut axes: Vec<&str> = bindings.axes().collect();
    actions.sort_unstable();
    axes.sort_unstable();
    assert_eq!(
        *r,
        json!({"tick_hz": 60, "tick": 0, "scene": null, "actions": actions, "axes": axes})
    );
    assert_eq!(r["axes"], json!(["look_x", "look_y", "move_x", "move_y"]));
}

#[test]
fn stepping_with_held_input_matches_replaying_the_recording() {
    let dir = test_dir("stepping_with_held_input_matches_replaying_the_recording");
    let record = dir.join("rec.ron");
    let dump = dir.join("dump.json");
    let requests = [
        req(1, "input", json!({"axes": {"move_y": 1.0}})),
        req(2, "step", json!({"ticks": 30})),
        req(3, "input", json!({"axes": {"look_x": 0.04}})),
        req(4, "step", json!({"ticks": 20})),
        req(5, "input", json!({"axes": {"move_y": 0.0, "look_x": 0.0}})),
        req(6, "step", json!({"ticks": 10})),
        req(7, "state", json!({})),
        req(8, "record", json!({"path": path_str(&record)})),
    ];
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len(), "{:?}", session.lines);
    let replies: Vec<Value> = session
        .lines
        .iter()
        .map(|l| serde_json::from_str(l).expect("JSON"))
        .collect();
    assert_eq!(result(&replies[5])["tick"], 60);
    assert_eq!(
        *result(&replies[7]),
        json!({"path": path_str(&record), "ticks": 60})
    );

    let out = Command::new(env!("CARGO_BIN_EXE_loom"))
        .args(["run", "--headless", "--frames", "60", "--input"])
        .arg(&record)
        .arg("--dump-state")
        .arg(&dump)
        .env("RUST_LOG", "off")
        .output()
        .expect("loom run can be started");
    assert!(out.status.success(), "{out:?}");

    // The raw text of the state reply's result, as written.
    let line = &session.lines[6];
    let raw = line
        .strip_prefix(r#"{"jsonrpc":"2.0","id":7,"result":"#)
        .and_then(|rest| rest.strip_suffix('}'))
        .unwrap_or_else(|| panic!("unexpected state reply: {line}"));
    let dumped: String = fs::read_to_string(&dump)
        .expect("the state dump was written")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(raw, dumped);
}

#[test]
fn replies_come_in_request_order_with_their_ids() {
    let requests = [
        req(1, "describe", json!({})),
        r#"{"jsonrpc":"2.0","id":"second","method":"step"}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":-3,"method":"step","params":{"ticks":2}}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":"4th one","method":"describe"}"#.to_owned(),
    ];
    let replies = serve(&requests);
    let ids: Vec<&Value> = replies.iter().map(|r| &r["id"]).collect();
    assert_eq!(
        ids,
        [&json!(1), &json!("second"), &json!(-3), &json!("4th one")]
    );
    assert_eq!(result(&replies[1])["tick"], 1);
    assert_eq!(result(&replies[2])["tick"], 3);
    assert_eq!(result(&replies[3])["tick"], 3);
}

#[test]
fn each_bad_request_gets_its_error_code_and_the_session_goes_on() {
    let bad: [(&str, i64, Value); 10] = [
        ("{not json", -32700, Value::Null),
        (
            r#"[{"jsonrpc":"2.0","id":1,"method":"describe"}]"#,
            -32600,
            Value::Null,
        ),
        (
            r#"{"jsonrpc":"2.0","method":"describe"}"#,
            -32600,
            Value::Null,
        ),
        (
            r#"{"jsonrpc":"2.0","id":10,"method":"fly"}"#,
            -32601,
            json!(10),
        ),
        (
            r#"{"jsonrpc":"2.0","id":11,"method":"step","params":{"ticks":1,"speed":2}}"#,
            -32602,
            json!(11),
        ),
        (
            r#"{"jsonrpc":"2.0","id":12,"method":"input","params":{"actions":{"jump":true}}}"#,
            -32602,
            json!(12),
        ),
        (
            r#"{"jsonrpc":"2.0","id":13,"method":"step","params":{"ticks":0}}"#,
            -32602,
            json!(13),
        ),
        (
            r#"{"jsonrpc":"2.0","id":14,"method":"load_scene","params":{"path":"a.ron","source":"()"}}"#,
            -32602,
            json!(14),
        ),
        (
            r#"{"jsonrpc":"2.0","id":15,"method":"load_scene","params":{"path":"no/such/scene.ron"}}"#,
            -32000,
            json!(15),
        ),
        (
            r#"{"jsonrpc":"2.0","id":16,"method":"step","param":{"ticks":60}}"#,
            -32600,
            json!(16),
        ),
    ];
    let mut requests = vec![req(1, "step", json!({"ticks": 3}))];
    for (i, (line, _, _)) in bad.iter().enumerate() {
        requests.push((*line).to_owned());
        requests.push(req(100 + i as u64, "describe", json!({})));
    }
    let replies = serve(&requests);
    for (i, (line, code, id)) in bad.iter().enumerate() {
        let error = &replies[1 + 2 * i];
        assert_eq!(error_code(error), *code, "{line}: {error}");
        assert_eq!(error["id"], *id, "{line}: {error}");
        let describe = result(&replies[2 + 2 * i]);
        assert_eq!(describe["tick"], 3, "after {line}");
    }
    let missing = &replies[1 + 2 * 8]["error"]["message"];
    assert!(
        missing
            .as_str()
            .is_some_and(|m| m.starts_with("reading scene no/such/scene.ron")),
        "{missing}"
    );
}

#[test]
fn input_request_with_an_unknown_name_changes_nothing() {
    let requests = [
        req(1, "input", json!({"axes": {"move_y": 1.0, "bogus": 1.0}})),
        req(
            2,
            "input",
            json!({"actions": {"jump": true}, "axes": {"move_y": 1.0}}),
        ),
        req(3, "step", json!({})),
        req(4, "state", json!({})),
    ];
    let replies = serve(&requests);
    assert_eq!(error_code(&replies[0]), -32602);
    assert!(
        replies[0]["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("bogus"))
    );
    assert_eq!(error_code(&replies[1]), -32602);
    let state = result(&replies[3]);
    assert_eq!(state["tick"], 1);
    assert_eq!(state["actions"], json!([]));
    assert_eq!(state["axes"]["move_y"], 0.0);
    assert_eq!(state["game"]["player"]["position"], json!([0.0, 0.0, 0.0]));
}

#[test]
fn failed_scene_load_keeps_the_running_session() {
    let requests = [
        req(1, "step", json!({"ticks": 5})),
        req(2, "load_scene", json!({"source": "(version: 1,"})),
        req(3, "describe", json!({})),
    ];
    let replies = serve(&requests);
    assert_eq!(error_code(&replies[1]), -32000);
    assert!(
        replies[1]["error"]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("invalid scene source")),
        "{}",
        replies[1]
    );
    assert_eq!(result(&replies[2])["tick"], 5);
}

#[test]
fn reset_restarts_the_scene_loaded_from_source() {
    let old = "(name: \"player\", position: (0.0, 0.0, 0.0),";
    assert!(DEMO_SCENE.contains(old));
    let source = DEMO_SCENE.replace(old, "(name: \"player\", position: (2.0, 0.0, -1.0),");
    let requests = [
        req(1, "step", json!({"ticks": 4})),
        req(2, "load_scene", json!({"source": source})),
        req(3, "step", json!({})),
        req(4, "state", json!({})),
        req(5, "input", json!({"axes": {"move_y": 1.0}})),
        req(6, "step", json!({"ticks": 10})),
        req(7, "reset", json!({})),
        req(8, "describe", json!({})),
        req(9, "step", json!({})),
        req(10, "state", json!({})),
    ];
    let replies = serve(&requests);
    assert_eq!(*result(&replies[1]), json!({"tick": 0, "scene": null}));
    let start = json!([2.0, 0.0, -1.0]);
    assert_eq!(result(&replies[3])["game"]["player"]["position"], start);
    assert_eq!(*result(&replies[6]), json!({"tick": 0}));
    assert_eq!(result(&replies[7])["tick"], 0);
    // Back at the loaded scene's start, not the built-in scene's, and the
    // held input was cleared.
    let state = result(&replies[9]);
    assert_eq!(state["tick"], 1);
    assert_eq!(state["game"]["player"]["position"], start);
    assert_eq!(state["axes"]["move_y"], 0.0);
}

#[test]
fn negative_zero_axis_value_is_dropped_from_the_held_input() {
    let requests = [
        req(1, "input", json!({"axes": {"look_x": 0.5, "move_y": 1.0}})),
        r#"{"jsonrpc":"2.0","id":2,"method":"input","params":{"axes":{"look_x":-0.0}}}"#.to_owned(),
    ];
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(
        session.lines,
        [
            r#"{"jsonrpc":"2.0","id":1,"result":{"actions":[],"axes":{"look_x":0.5,"move_y":1.0}}}"#,
            r#"{"jsonrpc":"2.0","id":2,"result":{"actions":[],"axes":{"move_y":1.0}}}"#,
        ]
    );
}

#[test]
fn end_of_stdin_ends_the_session_with_exit_code_zero() {
    let session = serve_raw(&[req(1, "describe", json!({}))]);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), 1, "{:?}", session.lines);
}

#[test]
fn quit_replies_and_then_ends_the_session() {
    let session = serve_raw(&[
        req(1, "describe", json!({})),
        req(2, "quit", json!({})),
        req(3, "describe", json!({})),
    ]);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), 2, "{:?}", session.lines);
    assert_eq!(session.lines[1], r#"{"jsonrpc":"2.0","id":2,"result":{}}"#);
}

#[test]
fn session_without_screenshot_reports_no_adapter() {
    let dir = test_dir("session_without_screenshot_reports_no_adapter");
    let requests = [
        req(1, "describe", json!({})),
        req(2, "step", json!({"ticks": 3})),
        req(3, "state", json!({})),
        req(4, "record", json!({"path": path_str(&dir.join("r.ron"))})),
    ];
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len());
    for line in &session.lines {
        // The key, quoted: the test directory's name contains the word.
        assert!(!line.contains("\"adapter\""), "{line}");
    }
}
