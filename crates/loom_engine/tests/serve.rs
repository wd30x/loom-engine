//! `loom serve` over real pipes (`docs/architecture.md` section 13). No GPU:
//! screenshots are checked by smoke (H5, H6).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const DEMO_SCENE: &str = include_str!("../../../games/souls/src/demo.scene.ron");

struct Session {
    code: i32,
    /// stdout, one reply per line.
    lines: Vec<String>,
}

/// Starts `loom serve`, writes `requests` one per line, closes stdin and
/// collects every reply until the process exits.
fn serve_raw(requests: &[String]) -> Session {
    serve_raw_in(&repo_root(), requests)
}

/// The repository root: scenes under `assets/` name their sheets relative
/// to it.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// [`serve_raw`] with `dir` as the working directory.
fn serve_raw_in(dir: &std::path::Path, requests: &[String]) -> Session {
    let mut child = Command::new(env!("CARGO_BIN_EXE_loom"))
        .arg("serve")
        .current_dir(dir)
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
    serve_in(&repo_root(), requests)
}

/// [`serve`] with `dir` as the working directory.
fn serve_in(dir: &std::path::Path, requests: &[String]) -> Vec<Value> {
    let session = serve_raw_in(dir, requests);
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
    let bindings = souls::bindings();
    let mut actions: Vec<&str> = bindings.actions().collect();
    let mut axes: Vec<&str> = bindings.axes().collect();
    actions.sort_unstable();
    axes.sort_unstable();
    assert_eq!(
        *r,
        json!({"tick_hz": 60, "tick": 0, "game": "souls", "scene": null, "edited": false, "actions": actions, "axes": axes})
    );
    assert_eq!(r["axes"], json!(["look_x", "look_y", "move_x", "move_y"]));
    assert_eq!(
        r["actions"],
        json!(["attack", "drink", "heavy", "interact", "lock", "roll"])
    );
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
        .current_dir(repo_root())
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
fn scene_load_with_an_unknown_sprite_tag_keeps_the_running_session() {
    let sheet = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/art/tiles.json")
        .display()
        .to_string()
        .replace('\\', "/");
    let badge = format!("(name: \"badge\", sprite: (sheet: \"{sheet}\", tag: \"playr\")),");
    let source = DEMO_SCENE.replacen(
        "(name: \"ground\",",
        &format!("{badge}\n        (name: \"ground\","),
        1,
    );
    assert_ne!(source, DEMO_SCENE);
    let requests = [
        req(1, "step", json!({"ticks": 5})),
        req(2, "load_scene", json!({"source": source})),
        req(3, "describe", json!({})),
    ];
    let replies = serve(&requests);
    assert_eq!(error_code(&replies[1]), -32000);
    let message = replies[1]["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.starts_with("invalid scene source") && message.contains("node badge: sprite"),
        "{}",
        replies[1]
    );
    assert!(message.contains("has no tag \"playr\""), "{message}");
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
    assert_eq!(
        *result(&replies[1]),
        json!({"tick": 0, "game": "souls", "scene": null})
    );
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

/// `load_scene` of the rogue scene, for a session in the repository root.
fn load_rogue(id: u64) -> String {
    req(id, "load_scene", json!({"path": "assets/scenes/rogue.ron"}))
}

#[test]
fn describe_on_a_rogue_scene_lists_its_nine_actions_and_no_axes() {
    let replies = serve_in(
        &repo_root(),
        &[load_rogue(1), req(2, "describe", json!({}))],
    );
    assert_eq!(
        *result(&replies[0]),
        json!({"tick": 0, "game": "rogue", "scene": "assets/scenes/rogue.ron"})
    );
    let r = result(&replies[1]);
    assert_eq!(r["game"], "rogue");
    assert_eq!(
        r["actions"],
        json!([
            "move_e", "move_n", "move_ne", "move_nw", "move_s", "move_se", "move_sw", "move_w",
            "wait"
        ])
    );
    assert_eq!(r["axes"], json!([]));
}

#[test]
fn held_rogue_action_steps_once_and_replays_from_its_recording() {
    let dir = test_dir("held_rogue_action_steps_once_and_replays_from_its_recording");
    let record = dir.join("rec.ron");
    let dump = dir.join("dump.json");
    let requests = [
        load_rogue(1),
        req(2, "input", json!({"actions": {"move_e": true}})),
        req(3, "step", json!({"ticks": 5})),
        req(4, "state", json!({})),
        req(5, "input", json!({"actions": {"move_e": false}})),
        req(6, "step", json!({})),
        req(7, "input", json!({"actions": {"move_e": true}})),
        req(8, "step", json!({})),
        req(9, "state", json!({})),
        req(10, "record", json!({"path": path_str(&record)})),
    ];
    let session = serve_raw_in(&repo_root(), &requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len(), "{:?}", session.lines);
    let replies: Vec<Value> = session
        .lines
        .iter()
        .map(|l| serde_json::from_str(l).expect("JSON"))
        .collect();
    // Held for 5 ticks: one step.
    assert_eq!(
        result(&replies[3])["game"],
        json!({
            "turn": 1,
            "player": {"cell": [4, 2], "hp": 10},
            // 67 energy each: not enough to act yet.
            "zombies": [
                {"name": "zombie", "cell": [7, 2], "hp": 3, "energy": 67},
                {"name": "zombie_2", "cell": [12, 7], "hp": 3, "energy": 67}
            ],
            "messages": []
        })
    );
    // Released for a tick, pressed again: a second step.
    assert_eq!(result(&replies[7])["tick"], 7);
    assert_eq!(
        result(&replies[8])["game"],
        json!({
            "turn": 2,
            "player": {"cell": [5, 2], "hp": 10},
            // 134 energy: one step each towards the player, 34 left.
            "zombies": [
                {"name": "zombie", "cell": [6, 2], "hp": 3, "energy": 34},
                {"name": "zombie_2", "cell": [11, 6], "hp": 3, "energy": 34}
            ],
            "messages": []
        })
    );
    assert_eq!(
        *result(&replies[9]),
        json!({"path": path_str(&record), "ticks": 7})
    );

    let out = Command::new(env!("CARGO_BIN_EXE_loom"))
        .args([
            "run",
            "--headless",
            "--frames",
            "7",
            "--scene",
            "assets/scenes/rogue.ron",
        ])
        .arg("--input")
        .arg(&record)
        .arg("--dump-state")
        .arg(&dump)
        .current_dir(repo_root())
        .env("RUST_LOG", "off")
        .output()
        .expect("loom run can be started");
    assert!(out.status.success(), "{out:?}");
    let line = &session.lines[8];
    let raw = line
        .strip_prefix(r#"{"jsonrpc":"2.0","id":9,"result":"#)
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
fn load_scene_switches_games_and_their_bindings() {
    let requests = [
        load_rogue(1),
        req(2, "input", json!({"axes": {"move_x": 1.0}})),
        req(3, "load_scene", json!({})),
        req(4, "input", json!({"axes": {"move_x": 1.0}})),
        req(5, "describe", json!({})),
    ];
    let replies = serve_in(&repo_root(), &requests);
    assert_eq!(result(&replies[0])["game"], "rogue");
    assert_eq!(error_code(&replies[1]), -32602);
    assert_eq!(
        *result(&replies[2]),
        json!({"tick": 0, "game": "souls", "scene": null})
    );
    assert_eq!(result(&replies[3])["axes"], json!({"move_x": 1.0}));
    assert_eq!(result(&replies[4])["game"], "souls");
}

#[test]
fn souls_roll_key_sprints_when_held_rolls_when_tapped_and_replays() {
    let dir = test_dir("souls_roll_key_sprints_when_held_rolls_when_tapped_and_replays");
    let record = dir.join("rec.ron");
    let dump = dir.join("dump.json");
    let requests = [
        req(
            1,
            "input",
            json!({"actions": {"roll": true}, "axes": {"move_y": 1.0}}),
        ),
        req(2, "step", json!({"ticks": 20})),
        req(3, "state", json!({})),
        req(4, "input", json!({"actions": {"roll": false}})),
        req(5, "step", json!({})),
        req(6, "input", json!({"actions": {"roll": true}})),
        req(7, "step", json!({})),
        req(8, "input", json!({"actions": {"roll": false}})),
        req(9, "step", json!({})),
        req(10, "state", json!({})),
        req(11, "record", json!({"path": path_str(&record)})),
    ];
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len(), "{:?}", session.lines);
    let replies: Vec<Value> = session
        .lines
        .iter()
        .map(|l| serde_json::from_str(l).expect("JSON"))
        .collect();
    // Held past the tap window (15 ticks) while moving: sprinting.
    let held = &result(&replies[2])["game"]["player"];
    assert_eq!(held["action"], "sprint");
    // Released, pressed for one tick, released: a tap, so a roll.
    let tapped = &result(&replies[9])["game"]["player"];
    assert_eq!(tapped["action"], "roll");
    assert_eq!(tapped["action_tick"], 1);
    assert_eq!(result(&replies[9])["tick"], 23);
    assert_eq!(
        *result(&replies[10]),
        json!({"path": path_str(&record), "ticks": 23})
    );

    let out = Command::new(env!("CARGO_BIN_EXE_loom"))
        .current_dir(repo_root())
        .args(["run", "--headless", "--frames", "23", "--input"])
        .arg(&record)
        .arg("--dump-state")
        .arg(&dump)
        .env("RUST_LOG", "off")
        .output()
        .expect("loom run can be started");
    assert!(out.status.success(), "{out:?}");
    let line = &session.lines[9];
    let raw = line
        .strip_prefix(r#"{"jsonrpc":"2.0","id":10,"result":"#)
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
fn souls_attack_pressed_through_held_input_reaches_its_active_phase_and_replays() {
    let dir =
        test_dir("souls_attack_pressed_through_held_input_reaches_its_active_phase_and_replays");
    let record = dir.join("rec.ron");
    let dump = dir.join("dump.json");
    let requests = [
        req(1, "input", json!({"actions": {"attack": true}})),
        req(2, "step", json!({})),
        req(3, "input", json!({"actions": {"attack": false}})),
        req(4, "step", json!({"ticks": 11})),
        req(5, "state", json!({})),
        req(6, "record", json!({"path": path_str(&record)})),
    ];
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len(), "{:?}", session.lines);
    let replies: Vec<Value> = session
        .lines
        .iter()
        .map(|l| serde_json::from_str(l).expect("JSON"))
        .collect();
    // Default light attack: startup ticks 1-10, active 11-14.
    let player = &result(&replies[4])["game"]["player"];
    assert_eq!(player["action"], "attack");
    assert_eq!(player["action_tick"], 12);
    assert_eq!(player["attack_phase"], "active");
    assert_eq!(player["combo"], 1);
    assert_eq!(
        *result(&replies[5]),
        json!({"path": path_str(&record), "ticks": 12})
    );
    let script = fs::read_to_string(&record).expect("the recording was written");
    assert!(
        script.contains(r#"action: "attack", down: true"#),
        "{script}"
    );

    let out = Command::new(env!("CARGO_BIN_EXE_loom"))
        .current_dir(repo_root())
        .args(["run", "--headless", "--frames", "12", "--input"])
        .arg(&record)
        .arg("--dump-state")
        .arg(&dump)
        .env("RUST_LOG", "off")
        .output()
        .expect("loom run can be started");
    assert!(out.status.success(), "{out:?}");
    let line = &session.lines[4];
    let raw = line
        .strip_prefix(r#"{"jsonrpc":"2.0","id":5,"result":"#)
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
fn souls_lock_pressed_through_held_input_locks_on_to_dummy_b_and_replays() {
    let dir = test_dir("souls_lock_pressed_through_held_input_locks_on_to_dummy_b_and_replays");
    let record = dir.join("rec.ron");
    let dump = dir.join("dump.json");
    let requests = [
        req(1, "input", json!({"actions": {"lock": true}})),
        req(2, "step", json!({})),
        req(3, "input", json!({"actions": {"lock": false}})),
        req(4, "step", json!({"ticks": 9})),
        req(5, "state", json!({})),
        req(6, "record", json!({"path": path_str(&record)})),
    ];
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len(), "{:?}", session.lines);
    let replies: Vec<Value> = session
        .lines
        .iter()
        .map(|l| serde_json::from_str(l).expect("JSON"))
        .collect();
    // dummy_b is straight ahead of the character at the start.
    assert_eq!(result(&replies[4])["game"]["lock"], "dummy_b");
    assert_eq!(
        *result(&replies[5]),
        json!({"path": path_str(&record), "ticks": 10})
    );

    let out = Command::new(env!("CARGO_BIN_EXE_loom"))
        .current_dir(repo_root())
        .args(["run", "--headless", "--frames", "10", "--input"])
        .arg(&record)
        .arg("--dump-state")
        .arg(&dump)
        .env("RUST_LOG", "off")
        .output()
        .expect("loom run can be started");
    assert!(out.status.success(), "{out:?}");
    let line = &session.lines[4];
    let raw = line
        .strip_prefix(r#"{"jsonrpc":"2.0","id":5,"result":"#)
        .and_then(|rest| rest.strip_suffix('}'))
        .unwrap_or_else(|| panic!("unexpected state reply: {line}"));
    let dumped: String = fs::read_to_string(&dump)
        .expect("the state dump was written")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(raw, dumped);
}

// ---------------------------------------------------------------- events and traces

/// The raw text of a trace line's `events` array (the state dump that
/// follows starts with its `tick`; no event holds that).
fn trace_events_text(line: &str) -> &str {
    let key = r#","events":"#;
    let rest = &line[line.find(key).expect("the line has events") + key.len()..];
    &rest[..rest
        .find(r#","state":{"tick":"#)
        .expect("the line has a state")]
}

/// The raw text of a `step` reply's `events` array.
fn step_events_text(line: &str) -> &str {
    let key = r#","events":"#;
    let rest = &line[line.find(key).expect("the reply has events") + key.len()..];
    rest.strip_suffix("}}")
        .expect("the reply ends with the result")
}

/// The events inside an array's text `[…]`, joined by commas: nothing for
/// `[]`.
fn inner(array: &str) -> &str {
    array
        .strip_prefix('[')
        .and_then(|a| a.strip_suffix(']'))
        .expect("an array")
}

/// Requests that play `souls_dummy_killed_while_locked_...`'s script on
/// the built-in scene up to tick 250 (the dummy killed), ids from 1; a
/// trace is started after tick 100, the tick the first attack starts.
/// Returns the requests and the ids of every `step`.
fn kill_requests(trace: &std::path::Path, record: &std::path::Path) -> (Vec<String>, Vec<u64>) {
    let plan: [(Value, u64); 13] = [
        (json!({"actions": {"lock": true}}), 1),
        (
            json!({"actions": {"lock": false}, "axes": {"move_y": 1.0}}),
            94,
        ),
        (json!({"axes": {"move_y": 0.0}}), 4),
        (json!({"actions": {"attack": true}}), 1),
        (json!({"actions": {"attack": false}}), 14),
        (json!({"actions": {"attack": true}}), 1),
        (json!({"actions": {"attack": false}}), 29),
        (json!({"actions": {"attack": true}}), 1),
        (json!({"actions": {"attack": false}}), 44),
        (json!({"actions": {"heavy": true}}), 1),
        (json!({"actions": {"heavy": false}}), 49),
        (json!({"actions": {"attack": true}}), 1),
        (json!({"actions": {"attack": false}}), 10),
    ];
    let mut requests = Vec::new();
    let mut steps = Vec::new();
    let mut tick = 0;
    for (input, ticks) in plan {
        requests.push(req(requests.len() as u64 + 1, "input", input));
        steps.push(requests.len() as u64 + 1);
        requests.push(req(
            requests.len() as u64 + 1,
            "step",
            json!({"ticks": ticks}),
        ));
        tick += ticks;
        if tick == 100 {
            let params = json!({"path": path_str(trace)});
            requests.push(req(requests.len() as u64 + 1, "trace", params));
        }
    }
    assert_eq!(tick, 250);
    let params = json!({"path": path_str(record)});
    requests.push(req(requests.len() as u64 + 1, "record", params));
    (requests, steps)
}

#[test]
fn step_events_carry_their_tick_and_the_text_the_replayed_trace_has() {
    let dir = test_dir("step_events_carry_their_tick_and_the_text_the_replayed_trace_has");
    let (served, record, replayed) = (
        dir.join("served.jsonl"),
        dir.join("rec.ron"),
        dir.join("replayed.jsonl"),
    );
    let (requests, steps) = kill_requests(&served, &record);
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len(), "{:?}", session.lines);
    let out = Command::new(env!("CARGO_BIN_EXE_loom"))
        .current_dir(repo_root())
        .args(["run", "--headless", "--frames", "250", "--input"])
        .arg(&record)
        .arg("--trace")
        .arg(&replayed)
        .env("RUST_LOG", "off")
        .output()
        .expect("loom run can be started");
    assert!(out.status.success(), "{out:?}");
    let replayed = fs::read_to_string(&replayed).expect("the replay traced");

    // Every step's events, joined, against the replayed trace's events with
    // a `tick` put in front of each (every event starts with its `type`).
    let from_steps: Vec<&str> = steps
        .iter()
        .map(|&id| inner(step_events_text(&session.lines[id as usize - 1])))
        .filter(|e| !e.is_empty())
        .collect();
    let from_trace: Vec<String> = replayed
        .lines()
        .enumerate()
        .map(|(t, line)| {
            inner(trace_events_text(line))
                .replace(r#"{"type":"#, &format!(r#"{{"tick":{t},"type":"#))
        })
        .filter(|e| !e.is_empty())
        .collect();
    assert_eq!(from_steps.join(","), from_trace.join(","));
    // Floats, and the hit that kills dummy_b on the last tick.
    assert!(
        from_steps.join(",").contains(
            r#"{"tick":250,"type":"hit","by":"player","target":"dummy_b","attack":"attack","combo":1,"damage":20.0,"hp":0.0,"result":"death"}"#
        ),
        "{from_steps:?}"
    );

    // The trace started after tick 100 is the replayed one from line 100:
    // its first line has that tick's events (the attack starting).
    let served = fs::read_to_string(&served).expect("the session traced");
    let replayed_lines: Vec<&str> = replayed.lines().collect();
    assert_eq!(served.lines().collect::<Vec<_>>(), replayed_lines[100..]);
    let first = served.lines().next().expect("a first line");
    assert!(
        first.starts_with(
            r#"{"tick":100,"events":[{"type":"action_start","action":"attack","combo":1,"buffered":false}],"#
        ),
        "{first}"
    );
}

#[test]
fn trace_starts_stops_and_starts_over_on_reset_and_load_scene() {
    let dir = test_dir("trace_starts_stops_and_starts_over_on_reset_and_load_scene");
    let (a, b) = (dir.join("new/a.jsonl"), dir.join("b.jsonl"));
    let replies = serve(&[
        req(1, "step", json!({"ticks": 3})),
        req(2, "trace", json!({"path": path_str(&a)})),
        req(3, "step", json!({"ticks": 2})),
        req(4, "reset", json!({})),
        req(5, "step", json!({"ticks": 1})),
        // Starting another trace stops this one.
        req(6, "trace", json!({"path": path_str(&b)})),
        req(7, "step", json!({"ticks": 2})),
        req(8, "load_scene", json!({"source": DEMO_SCENE})),
        req(9, "trace", json!({})),
        req(10, "trace", json!({})),
        req(11, "trace", json!({"path": path_str(&a), "extra": 1})),
    ]);
    assert_eq!(*result(&replies[0]), json!({"tick": 3, "events": []}));
    assert_eq!(
        *result(&replies[1]),
        json!({"path": path_str(&a), "tick": 3})
    );
    assert_eq!(
        *result(&replies[5]),
        json!({"path": path_str(&b), "tick": 1})
    );
    // b: ticks 1, 2 and 3, then tick 0 of the loaded scene.
    assert_eq!(
        *result(&replies[8]),
        json!({"path": path_str(&b), "lines": 1})
    );
    assert_eq!(*result(&replies[9]), json!({"path": null, "lines": 0}));
    assert_eq!(error_code(&replies[10]), -32602);
    let ticks = |path: &std::path::Path| -> Vec<u64> {
        fs::read_to_string(path)
            .expect("the trace was written")
            .lines()
            .map(|l| {
                let l: Value = serde_json::from_str(l).expect("JSON");
                l["tick"].as_u64().expect("a tick")
            })
            .collect()
    };
    // a: ticks 3, 4, 5, emptied by the reset, then 0 and 1.
    assert_eq!(ticks(&a), [0, 1]);
    assert_eq!(ticks(&b), [0]);
}

// ---------------------------------------------------------------- debug draw

#[test]
fn screenshot_with_a_bad_debug_draw_is_invalid_params_before_any_rendering() {
    // Rejected before the renderer is created: this needs no GPU. The
    // renders themselves are checked by smoke (H10).
    let dir = test_dir("screenshot_with_a_bad_debug_draw_is_invalid_params_before_any_rendering");
    let shot = dir.join("never.png");
    let path = path_str(&shot);
    let replies = serve_in(
        &repo_root(),
        &[
            req(
                1,
                "screenshot",
                json!({"path": path, "debug_draw": ["lock", "hitboxes"]}),
            ),
            req(
                2,
                "screenshot",
                json!({"path": path, "debug_draw": ["lock", "lock"]}),
            ),
            req(3, "screenshot", json!({"path": path, "debug_draw": "lock"})),
            req(4, "load_scene", json!({"path": "assets/scenes/rogue.ron"})),
            req(
                5,
                "screenshot",
                json!({"path": path, "debug_draw": ["lock"]}),
            ),
        ],
    );
    let message = |reply: &Value| reply["error"]["message"].as_str().unwrap_or("").to_owned();
    for i in [0, 1, 2, 4] {
        assert_eq!(error_code(&replies[i]), -32602, "{}", replies[i]);
    }
    assert_eq!(
        message(&replies[0]),
        r#"unknown debug draw category "hitboxes": souls has "hurtboxes", "swings", "colliders", "lock", "bonfires", "bloodstain" and "skeleton""#
    );
    assert_eq!(
        message(&replies[1]),
        r#"debug draw category "lock" is given twice"#
    );
    assert_eq!(
        message(&replies[4]),
        r#"unknown debug draw category "lock": rogue has "skeleton""#
    );
    assert!(!shot.exists());
}

// ---------------------------------------------------------------- hud

#[test]
fn screenshot_with_a_hud_that_is_not_a_bool_is_invalid_params_before_any_rendering() {
    // Rejected before the renderer is created: this needs no GPU. The
    // renders with and without the HUD are checked by smoke (H11).
    let dir =
        test_dir("screenshot_with_a_hud_that_is_not_a_bool_is_invalid_params_before_any_rendering");
    let shot = dir.join("never.png");
    let path = path_str(&shot);
    let replies = serve_in(
        &repo_root(),
        &[
            req(1, "screenshot", json!({"path": path, "hud": "off"})),
            req(2, "screenshot", json!({"path": path, "hud": 0})),
        ],
    );
    for reply in &replies {
        assert_eq!(error_code(reply), -32602, "{reply}");
    }
    assert!(!shot.exists());
}

// Windows only: elsewhere file locks are advisory and the write succeeds.
#[cfg(windows)]
#[test]
fn trace_that_fails_to_close_after_quit_exits_1_without_another_stdout_line() {
    use std::io::{BufRead, BufReader};

    // Another handle locks the whole trace file while its first line is
    // still buffered: on Windows the lock is mandatory, so writing it out
    // at `quit` fails.
    let dir = test_dir("trace_that_fails_to_close_after_quit_exits_1_without_another_stdout_line");
    let trace = dir.join("t.jsonl");
    let mut child = Command::new(env!("CARGO_BIN_EXE_loom"))
        .current_dir(repo_root())
        .arg("serve")
        .env("RUST_LOG", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the loom binary can be started");
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
    let mut reply = String::new();
    writeln!(
        stdin,
        "{}",
        req(1, "trace", json!({"path": path_str(&trace)}))
    )
    .expect("loom serve reads its stdin");
    stdout.read_line(&mut reply).expect("a reply");
    assert!(reply.contains(r#""result":{"path""#), "{reply}");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&trace)
        .expect("the trace file exists");
    lock.lock().expect("the trace file can be locked");
    writeln!(stdin, "{}", req(2, "quit", json!({}))).expect("loom serve reads its stdin");
    drop(stdin);
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut stdout, &mut rest).expect("stdout is UTF-8");
    let status = child.wait().expect("loom serve can be waited for");
    drop(lock);
    assert_eq!(rest, "{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}\n");
    assert_eq!(status.code(), Some(1));
}

// ---------------------------------------------------------------- editing

/// Pushes requests, ids following on, that play the first 212 ticks of
/// `kill_requests`' script: three light hits on dummy_b, then a heavy one
/// on tick 212. With `trace`, a trace is started after tick 100.
fn play_to_212(requests: &mut Vec<String>, trace: Option<&std::path::Path>) {
    let plan: [(Value, u64); 11] = [
        (json!({"actions": {"lock": true}}), 1),
        (
            json!({"actions": {"lock": false}, "axes": {"move_y": 1.0}}),
            94,
        ),
        (json!({"axes": {"move_y": 0.0}}), 4),
        (json!({"actions": {"attack": true}}), 1),
        (json!({"actions": {"attack": false}}), 14),
        (json!({"actions": {"attack": true}}), 1),
        (json!({"actions": {"attack": false}}), 29),
        (json!({"actions": {"attack": true}}), 1),
        (json!({"actions": {"attack": false}}), 44),
        (json!({"actions": {"heavy": true}}), 1),
        (json!({"actions": {"heavy": false}}), 22),
    ];
    let mut tick = 0;
    for (input, ticks) in plan {
        requests.push(req(requests.len() as u64 + 1, "input", input));
        let params = json!({"ticks": ticks});
        requests.push(req(requests.len() as u64 + 1, "step", params));
        tick += ticks;
        if let (Some(trace), 100) = (trace, tick) {
            let params = json!({"path": path_str(trace)});
            requests.push(req(requests.len() as u64 + 1, "trace", params));
        }
    }
    assert_eq!(tick, 212);
}

/// Pushes a request with the next id; returns its index in the replies.
fn push(requests: &mut Vec<String>, method: &str, params: Value) -> usize {
    requests.push(req(requests.len() as u64 + 1, method, params));
    requests.len() - 1
}

/// The raw text of a reply's result, as written.
fn raw_result(line: &str) -> &str {
    let rest = &line[line.find(r#","result":"#).expect("a result") + 10..];
    rest.strip_suffix('}')
        .expect("the reply ends with the result")
}

/// `loom run --headless --scene <scene> --input <record> --frames <frames>`
/// with `extra` arguments.
fn replay(
    scene: &std::path::Path,
    record: &std::path::Path,
    frames: u64,
    extra: &[&std::path::Path; 2],
) {
    let out = Command::new(env!("CARGO_BIN_EXE_loom"))
        .current_dir(repo_root())
        .args(["run", "--headless", "--scene"])
        .arg(scene)
        .arg("--input")
        .arg(record)
        .args(["--frames", &frames.to_string()])
        .args(extra)
        .env("RUST_LOG", "off")
        .output()
        .expect("loom run can be started");
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn state_after_a_poise_edit_equals_replaying_the_saved_scene_and_recording() {
    let dir = test_dir("state_after_a_poise_edit_equals_replaying_the_saved_scene_and_recording");
    let (scene, record, dump) = (
        dir.join("poise70.scene.ron"),
        dir.join("rec.ron"),
        dir.join("dump.json"),
    );
    let mut requests = Vec::new();
    play_to_212(&mut requests, None);
    let before = push(&mut requests, "state", json!({}));
    let edit = push(
        &mut requests,
        "edit_node",
        json!({"path": "dummy_b", "props": {"poise": 70}}),
    );
    let after = push(&mut requests, "state", json!({}));
    let save = push(
        &mut requests,
        "save_scene",
        json!({"path": path_str(&scene)}),
    );
    let rec = push(&mut requests, "record", json!({"path": path_str(&record)}));
    let source = push(&mut requests, "scene", json!({}));
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len(), "{:?}", session.lines);
    let replies: Vec<Value> = session
        .lines
        .iter()
        .map(|l| serde_json::from_str(l).expect("JSON"))
        .collect();
    assert_eq!(*result(&replies[edit]), json!({"tick": 212}));
    assert_eq!(*result(&replies[save]), json!({"path": path_str(&scene)}));
    assert_eq!(
        *result(&replies[rec]),
        json!({"path": path_str(&record), "ticks": 212})
    );

    // By hand: light hits on ticks 110, 140 and 170 add impacts of 10 each,
    // the heavy one on tick 212 adds 30 (no 60 ticks without a hit between
    // them). With poise 25 the third light hit (30 > 25) staggers and
    // clears the impacts, and the heavy one (30 > 25) staggers again. With
    // poise 70 the impacts add up to 60: no stagger. Hp: 100 - 3 x 20 - 35.
    let dummy_b = |i: usize| result(&replies[i])["game"]["hurtboxes"][1].clone();
    assert_eq!(dummy_b(before)["node"], "dummy_b");
    assert_eq!(
        dummy_b(before)["health"],
        json!({"hp": 5.0, "state": "stagger", "state_ticks": 30, "impact": 0.0, "impact_wait": 60})
    );
    assert_eq!(
        dummy_b(after)["health"],
        json!({"hp": 5.0, "state": "ok", "state_ticks": 0, "impact": 60.0, "impact_wait": 60})
    );

    let saved = fs::read_to_string(&scene).expect("the scene was saved");
    assert_eq!(result(&replies[source])["source"], saved.as_str());
    assert_eq!(result(&replies[source])["edited"], true);
    assert!(saved.contains("(\"poise\", 70.0),"), "{saved}");

    replay(
        &scene,
        &record,
        212,
        &[std::path::Path::new("--dump-state"), &dump],
    );
    let dumped: String = fs::read_to_string(&dump)
        .expect("the state dump was written")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(raw_result(&session.lines[after]), dumped);
}

#[test]
fn failed_edit_replies_the_load_message_and_leaves_state_tick_and_scene_as_they_were() {
    let mut requests = vec![
        req(1, "input", json!({"axes": {"move_y": 1.0}})),
        req(2, "step", json!({"ticks": 20})),
    ];
    let looks = ["state", "describe", "scene"];
    for method in looks {
        push(&mut requests, method, json!({}));
    }
    let edit = push(
        &mut requests,
        "edit_node",
        json!({"path": "dummy_b", "props": {"poise": -1}}),
    );
    for method in looks {
        push(&mut requests, method, json!({}));
    }
    // The same scene loaded as text: its message, after the load's own
    // prefix.
    let at = DEMO_SCENE
        .find("\"dummy_b\"")
        .expect("dummy_b is in the demo");
    let (head, tail) = DEMO_SCENE.split_at(at);
    let source = format!(
        "{head}{}",
        tail.replacen("(\"poise\", 25)", "(\"poise\", -1)", 1)
    );
    let load = push(&mut requests, "load_scene", json!({"source": source}));
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len(), "{:?}", session.lines);
    for i in 0..looks.len() {
        let (was, is) = (&session.lines[2 + i], &session.lines[edit + 1 + i]);
        assert_eq!(raw_result(was), raw_result(is));
    }
    let reply: Value = serde_json::from_str(&session.lines[edit]).expect("JSON");
    assert_eq!(error_code(&reply), -32602);
    let message = reply["error"]["message"].as_str().expect("a message");
    assert_eq!(
        message,
        r#"the node "dummy_b": prop "poise" must be at least 0, got -1"#
    );
    let loaded: Value = serde_json::from_str(&session.lines[load]).expect("JSON");
    assert_eq!(
        loaded["error"]["message"],
        format!("invalid scene source: {message}")
    );
}

#[test]
fn node_ron_keeps_the_spawn_point_while_its_transform_follows_the_player() {
    let replies = serve(&[
        req(1, "input", json!({"axes": {"move_y": 1.0}})),
        req(2, "step", json!({"ticks": 30})),
        req(3, "node", json!({"path": "player"})),
        req(4, "state", json!({})),
        req(5, "node", json!({"path": "dummy_b/body"})),
        req(6, "node", json!({"path": "dummy_b/nope"})),
    ]);
    let node = result(&replies[2]);
    assert_eq!(node["path"], "player");
    assert_eq!(node["children"], json!(["visor", "flask"]));
    let ron = node["ron"].as_str().expect("ron is text");
    assert!(
        ron.starts_with("(\n    name: \"player\",\n    position: (0.0, 0.0, 0.0),\n"),
        "{ron}"
    );
    assert!(!ron.contains("children"), "{ron}");
    let now = &result(&replies[3])["nodes"]["player"];
    assert_eq!(node["transform"], *now);
    assert_ne!(now["position"], json!([0.0, 0.0, 0.0]));
    assert_eq!(
        *result(&replies[4]),
        json!({
            "path": "dummy_b/body",
            "ron": "(\n    name: \"body\",\n    mesh: Capsule(\n        radius: 0.4,\n        height: 1.8,\n    ),\n    color: (0.45, 0.5, 0.6),\n)\n",
            "children": [],
            "transform": {"position": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        })
    );
    assert_eq!(error_code(&replies[5]), -32602);
    assert_eq!(
        replies[5]["error"]["message"],
        r#"no node at "dummy_b/nope""#
    );
}

#[test]
fn added_dummy_with_hp_is_among_the_hurtboxes_after_the_rerun() {
    let dummy = r#"(name: "dummy_d", position: (0.0, 0.0, -3.0),
        props: [("hurt_radius", 0.4), ("hurt_height", 1.8), ("hp", 50)],
        children: [(name: "body", mesh: Capsule(radius: 0.4, height: 1.8))])"#;
    let replies = serve(&[
        req(1, "step", json!({"ticks": 10})),
        req(2, "add_node", json!({"parent": null, "ron": dummy})),
        req(3, "state", json!({})),
        req(4, "node", json!({"path": "dummy_d/body"})),
        req(
            5,
            "add_node",
            json!({"parent": "dummy_d", "index": 2, "ron": "(name: \"x\")"}),
        ),
        req(
            6,
            "add_node",
            json!({"parent": "dummy_d", "ron": "(name: \"body\")"}),
        ),
    ]);
    assert_eq!(*result(&replies[1]), json!({"tick": 10}));
    let state = result(&replies[2]);
    assert_eq!(state["tick"], 10);
    let hurtboxes = state["game"]["hurtboxes"]
        .as_array()
        .expect("hurtboxes is a list");
    let added = hurtboxes.last().expect("a hurtbox");
    assert_eq!(added["node"], "dummy_d");
    assert_eq!(added["health"]["hp"], 50.0);
    assert_eq!(hurtboxes.len(), 5);
    assert_eq!(
        state["nodes"]["dummy_d"]["position"],
        json!([0.0, 0.0, -3.0])
    );
    assert_eq!(result(&replies[3])["children"], json!([]));
    assert_eq!(error_code(&replies[4]), -32602);
    assert_eq!(
        replies[4]["error"]["message"],
        r#"index 2 is past the end: "dummy_d" has 1 children"#
    );
    // Rejected by the load's rules, in the load's words.
    assert_eq!(error_code(&replies[5]), -32602);
    let message = replies[5]["error"]["message"].as_str().expect("a message");
    assert!(message.contains("\"dummy_d/body\""), "{message}");
}

#[test]
fn removing_the_stake_leaves_no_swingers() {
    let replies = serve(&[
        req(1, "step", json!({"ticks": 5})),
        req(2, "state", json!({})),
        req(3, "remove_node", json!({"path": "stake"})),
        req(4, "state", json!({})),
        req(5, "remove_node", json!({"path": "stake"})),
        req(6, "node", json!({"path": "stake/post"})),
    ]);
    let swingers = |i: usize| result(&replies[i])["game"]["swingers"].clone();
    assert_eq!(swingers(1)[0]["node"], "stake");
    assert_eq!(*result(&replies[2]), json!({"tick": 5}));
    assert_eq!(swingers(3), json!([]));
    assert_eq!(result(&replies[3])["tick"], 5);
    assert!(result(&replies[3])["nodes"].get("stake/post").is_none());
    assert_eq!(error_code(&replies[4]), -32602);
    assert_eq!(replies[4]["error"]["message"], r#"no node at "stake""#);
    assert_eq!(error_code(&replies[5]), -32602);
}

#[test]
fn edit_scene_sets_the_ambient_light_and_refuses_to_change_the_game() {
    let replies = serve(&[
        req(1, "scene", json!({})),
        req(2, "edit_scene", json!({"set": "ambient: (0.5, 0.5, 0.5)"})),
        req(3, "scene", json!({})),
        req(4, "edit_scene", json!({"set": "game: \"rogue\""})),
        req(5, "edit_scene", json!({"set": "ambient: (0.5, 0.5)"})),
        req(6, "describe", json!({})),
    ]);
    let before = result(&replies[0]);
    assert_eq!(before["edited"], false);
    let before = before["source"].as_str().expect("source is text");
    assert!(
        before.contains("\n    ambient: (0.2, 0.2, 0.25),\n"),
        "{before}"
    );
    assert_eq!(*result(&replies[1]), json!({"tick": 0}));
    let after = result(&replies[2]);
    assert_eq!(after["edited"], true);
    assert_eq!(
        after["source"],
        before.replace("ambient: (0.2, 0.2, 0.25)", "ambient: (0.5, 0.5, 0.5)")
    );
    assert_eq!(error_code(&replies[3]), -32602);
    assert_eq!(
        replies[3]["error"]["message"],
        "edit_scene cannot change game"
    );
    assert_eq!(error_code(&replies[4]), -32602);
    assert_eq!(
        replies[4]["error"]["message"],
        "set: 1:19: Expected a tuple of size 3 but found 2 elements instead"
    );
    assert_eq!(result(&replies[5])["game"], "souls");
}

#[test]
fn reset_restarts_the_edited_scene_and_load_scene_drops_the_edit() {
    let replies = serve(&[
        req(1, "step", json!({"ticks": 10})),
        req(
            2,
            "edit_node",
            json!({"path": "player", "set": "position: (2.0, 0.0, -1.0)"}),
        ),
        req(3, "reset", json!({})),
        req(4, "state", json!({})),
        req(5, "describe", json!({})),
        req(6, "load_scene", json!({})),
        req(7, "describe", json!({})),
        req(8, "state", json!({})),
        req(9, "scene", json!({})),
    ]);
    assert_eq!(*result(&replies[1]), json!({"tick": 10}));
    assert_eq!(*result(&replies[2]), json!({"tick": 0}));
    let state = result(&replies[3]);
    assert_eq!(state["tick"], 0);
    assert_eq!(state["game"]["player"]["position"], json!([2.0, 0.0, -1.0]));
    assert_eq!(result(&replies[4])["edited"], true);
    assert_eq!(result(&replies[6])["edited"], false);
    let state = result(&replies[7]);
    assert_eq!(state["game"]["player"]["position"], json!([0.0, 0.0, 0.0]));
    assert_eq!(result(&replies[8])["edited"], false);
}

#[test]
fn edit_while_tracing_rewrites_the_trace_from_tick_0_as_the_replay_writes_it() {
    let dir = test_dir("edit_while_tracing_rewrites_the_trace_from_tick_0_as_the_replay_writes_it");
    let (served, scene, record, replayed) = (
        dir.join("served.jsonl"),
        dir.join("scene.ron"),
        dir.join("rec.ron"),
        dir.join("replayed.jsonl"),
    );
    let mut requests = Vec::new();
    play_to_212(&mut requests, Some(&served));
    let edit = push(
        &mut requests,
        "edit_node",
        json!({"path": "dummy_b", "props": {"poise": 70}}),
    );
    push(
        &mut requests,
        "save_scene",
        json!({"path": path_str(&scene)}),
    );
    push(&mut requests, "record", json!({"path": path_str(&record)}));
    let stop = push(&mut requests, "trace", json!({}));
    let session = serve_raw(&requests);
    assert_eq!(session.code, 0);
    assert_eq!(session.lines.len(), requests.len(), "{:?}", session.lines);
    // No events from the rerun in any reply.
    assert_eq!(raw_result(&session.lines[edit]), r#"{"tick":212}"#);
    assert_eq!(
        raw_result(&session.lines[stop]),
        format!(r#"{{"path":{},"lines":213}}"#, json!(path_str(&served)))
    );

    replay(
        &scene,
        &record,
        212,
        &[std::path::Path::new("--trace"), &replayed],
    );
    let served = fs::read_to_string(&served).expect("the session traced");
    let replayed = fs::read_to_string(&replayed).expect("the replay traced");
    assert_eq!(served.lines().count(), 213);
    assert_eq!(served, replayed);
}

#[test]
fn edit_at_tick_0_swaps_the_scene_without_running_a_tick() {
    let dir = test_dir("edit_at_tick_0_swaps_the_scene_without_running_a_tick");
    let trace = dir.join("t.jsonl");
    let replies = serve(&[
        req(1, "trace", json!({"path": path_str(&trace)})),
        req(
            2,
            "edit_node",
            json!({"path": "player", "set": "position: (2.0, 0.0, -1.0)"}),
        ),
        req(3, "state", json!({})),
        req(4, "trace", json!({})),
    ]);
    assert_eq!(*result(&replies[1]), json!({"tick": 0}));
    let state = result(&replies[2]);
    assert_eq!(state["tick"], 0);
    assert_eq!(state["game"]["player"]["position"], json!([2.0, 0.0, -1.0]));
    assert_eq!(
        *result(&replies[3]),
        json!({"path": path_str(&trace), "lines": 1})
    );
    let written = fs::read_to_string(&trace).expect("the trace was written");
    let line: Value = serde_json::from_str(written.trim_end()).expect("one JSON line");
    assert_eq!(line["tick"], 0);
    assert_eq!(
        line["state"]["game"]["player"]["position"],
        json!([2.0, 0.0, -1.0])
    );
}

// ---------------------------------------------------------------- schema

#[test]
fn schema_request_replies_the_committed_schema() {
    let replies = serve(&[
        req(1, "schema", json!({})),
        req(2, "schema", json!({"game": "souls"})),
    ]);
    let committed = fs::read_to_string(repo_root().join("schema/loom.schema.json"))
        .expect("the schema file is committed");
    let committed: Value = serde_json::from_str(&committed).expect("JSON");
    assert_eq!(*result(&replies[0]), committed);
    assert_eq!(error_code(&replies[1]), -32602);
}
