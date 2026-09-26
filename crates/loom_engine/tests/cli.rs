//! CLI contract tests: run the real `loom` binary and check stdout, exit
//! code and output files.
//!
//! Only GPU-free invocations belong here (headless, no `--screenshot`), so
//! that `verify.ps1` keeps working on machines without a GPU.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

/// State after 60 ticks of the built-in scene without input (scenario A).
const SCENARIO_A: &str = concat!(
    "{\n",
    "  \"tick\": 60,\n",
    "  \"sim_time\": 1.0,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {\n",
    "    \"look_x\": 0.0,\n",
    "    \"look_y\": 0.0,\n",
    "    \"move_x\": 0.0,\n",
    "    \"move_y\": 0.0\n",
    "  },\n",
    "  \"nodes\": {\n",
    "    \"ground\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        40.0,\n",
    "        1.0,\n",
    "        40.0\n",
    "      ]\n",
    "    },\n",
    "    \"main_cam\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.5352764,\n",
    "        3.8637035\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        -0.1305262,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.9914449\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"pillars\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"pillars/a\": {\n",
    "      \"position\": [\n",
    "        3.0,\n",
    "        1.5,\n",
    "        -4.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        3.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"pillars/b\": {\n",
    "      \"position\": [\n",
    "        -3.0,\n",
    "        1.5,\n",
    "        -4.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        3.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"pillars/c\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.5,\n",
    "        -9.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        3.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"player\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"player/visor\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.5,\n",
    "        -0.3\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.4,\n",
    "        0.15,\n",
    "        0.3\n",
    "      ]\n",
    "    },\n",
    "    \"sun\": {\n",
    "      \"position\": [\n",
    "        4.0,\n",
    "        8.0,\n",
    "        3.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        -0.43359014,\n",
    "        0.39115202,\n",
    "        0.21679507,\n",
    "        0.78230405\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"wall\": {\n",
    "      \"position\": [\n",
    "        -3.0,\n",
    "        1.0,\n",
    "        3.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.25881904,\n",
    "        0.0,\n",
    "        0.9659258\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        4.0,\n",
    "        2.0,\n",
    "        0.5\n",
    "      ]\n",
    "    }\n",
    "  },\n",
    "  \"game\": {\n",
    "    \"player\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0\n",
    "      ],\n",
    "      \"facing\": 0.0\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    }\n",
    "  }\n",
    "}\n",
);

/// State after 90 ticks, `move_y` at 1.0 for ticks 1..=60 (scenario S1).
const SCENARIO_B: &str = concat!(
    "{\n",
    "  \"tick\": 90,\n",
    "  \"sim_time\": 1.5,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {\n",
    "    \"look_x\": 0.0,\n",
    "    \"look_y\": 0.0,\n",
    "    \"move_x\": 0.0,\n",
    "    \"move_y\": 0.0\n",
    "  },\n",
    "  \"nodes\": {\n",
    "    \"ground\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        40.0,\n",
    "        1.0,\n",
    "        40.0\n",
    "      ]\n",
    "    },\n",
    "    \"main_cam\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.5352764,\n",
    "        -0.13629532\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        -0.1305262,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.9914449\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"pillars\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"pillars/a\": {\n",
    "      \"position\": [\n",
    "        3.0,\n",
    "        1.5,\n",
    "        -4.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        3.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"pillars/b\": {\n",
    "      \"position\": [\n",
    "        -3.0,\n",
    "        1.5,\n",
    "        -4.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        3.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"pillars/c\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.5,\n",
    "        -9.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        3.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"player\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        -3.9999988\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"player/visor\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.5,\n",
    "        -0.3\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.4,\n",
    "        0.15,\n",
    "        0.3\n",
    "      ]\n",
    "    },\n",
    "    \"sun\": {\n",
    "      \"position\": [\n",
    "        4.0,\n",
    "        8.0,\n",
    "        3.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        -0.43359014,\n",
    "        0.39115202,\n",
    "        0.21679507,\n",
    "        0.78230405\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"wall\": {\n",
    "      \"position\": [\n",
    "        -3.0,\n",
    "        1.0,\n",
    "        3.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.25881904,\n",
    "        0.0,\n",
    "        0.9659258\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        4.0,\n",
    "        2.0,\n",
    "        0.5\n",
    "      ]\n",
    "    }\n",
    "  },\n",
    "  \"game\": {\n",
    "    \"player\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        -3.9999988\n",
    "      ],\n",
    "      \"facing\": 0.0\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    }\n",
    "  }\n",
    "}\n",
);

struct Output {
    code: i32,
    stdout: String,
}

fn run_loom(args: &[&str]) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_loom"))
        .args(args)
        .env("RUST_LOG", "off")
        .output()
        .expect("the loom binary can be started");
    Output {
        code: out
            .status
            .code()
            .expect("loom exits normally, not by a signal"),
        stdout: String::from_utf8(out.stdout).expect("stdout is UTF-8"),
    }
}

/// Asserts the protocol: stdout is exactly one line holding one JSON object.
fn report(out: &Output) -> Value {
    assert!(
        out.stdout.ends_with('\n'),
        "stdout must end with a newline: {:?}",
        out.stdout
    );
    assert_eq!(
        out.stdout.lines().count(),
        1,
        "stdout must be one line: {:?}",
        out.stdout
    );
    let value: Value = serde_json::from_str(&out.stdout).expect("stdout is valid JSON");
    assert!(value.is_object(), "stdout must be a JSON object: {value}");
    value
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

/// Compares a JSON `[x, y, z]` with f32 values bit by bit (serde_json prints
/// f32 in its shortest round-trip form, so parsing back as f32 is exact).
fn assert_f32_bits(value: &Value, expected: [f32; 3]) {
    let actual: Vec<f32> = value
        .as_array()
        .expect("value is an array")
        .iter()
        .map(|v| v.as_f64().expect("number") as f32)
        .collect();
    assert_eq!(
        actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        expected.map(f32::to_bits),
        "{actual:?} != {expected:?}"
    );
}

fn assert_f64_bits(value: &Value, expected: f64) {
    let actual = value.as_f64().expect("value is a number");
    assert_eq!(
        actual.to_bits(),
        expected.to_bits(),
        "{actual} != {expected}"
    );
}

#[test]
fn headless_run_without_screenshot_reports_null_adapter() {
    let out = run_loom(&["run", "--headless", "--frames", "60"]);
    let r = report(&out);
    assert_eq!(out.code, 0);
    assert_eq!(r["status"], "ok");
    assert_eq!(r["mode"], "headless");
    assert!(r["adapter"].is_null());
    assert!(r["renderer_init_ms"].is_null());
    assert!(r["screenshot"].is_null());
    assert!(r["state"].is_null());
    assert_eq!(r["frames"], 60);
    assert_eq!(r["ticks"], 60);
    assert_f64_bits(&r["sim_time"], 1.0);
}

#[test]
fn state_dump_after_sixty_ticks_matches_expected_bytes() {
    let file = test_dir("state_dump_after_sixty_ticks_matches_expected_bytes").join("s.json");
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "60",
        "--dump-state",
        path_str(&file),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0);
    assert_eq!(r["state"], path_str(&file));
    assert!(r["record"].is_null());
    assert!(r["input"].is_null());
    assert!(r["scene"].is_null());
    assert_eq!(
        fs::read_to_string(&file).expect("state file was written"),
        SCENARIO_A
    );
}

#[test]
fn repeated_headless_runs_write_identical_state_dumps() {
    let dir = test_dir("repeated_headless_runs_write_identical_state_dumps");
    let files = [dir.join("first.json"), dir.join("second.json")];
    for file in &files {
        let out = run_loom(&[
            "run",
            "--headless",
            "--frames",
            "3600",
            "--dump-state",
            path_str(file),
        ]);
        assert_eq!(out.code, 0);
        report(&out);
    }
    let [a, b] = files.map(|f| fs::read(f).expect("state file was written"));
    assert_eq!(a, b);
}

#[test]
fn state_dump_creates_missing_parent_directories() {
    let file = test_dir("state_dump_creates_missing_parent_directories").join("a/b/s.json");
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--dump-state",
        path_str(&file),
    ]);
    assert_eq!(out.code, 0);
    report(&out);
    assert!(file.is_file());
}

#[test]
fn state_dump_into_a_file_path_is_a_runtime_error() {
    let dir = test_dir("state_dump_into_a_file_path_is_a_runtime_error");
    fs::write(dir.join("blocker"), "not a directory").expect("blocker file can be written");
    let file = dir.join("blocker").join("s.json");
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--dump-state",
        path_str(&file),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1);
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    assert!(
        error.contains("blocker"),
        "error should name the path: {error}"
    );
}

#[test]
fn no_capture_together_with_headless_is_an_argument_error() {
    let out = run_loom(&["run", "--headless", "--frames", "1", "--no-capture"]);
    let r = report(&out);
    assert_eq!(out.code, 2);
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    assert!(error.contains("--no-capture"), "{error}");
}

#[test]
fn headless_without_frames_is_an_argument_error() {
    let out = run_loom(&["run", "--headless"]);
    let r = report(&out);
    assert_eq!(out.code, 2);
    assert_eq!(r["status"], "error");
}

#[test]
fn zero_frames_is_an_argument_error() {
    let out = run_loom(&["run", "--headless", "--frames", "0"]);
    let r = report(&out);
    assert_eq!(out.code, 2);
    assert_eq!(r["status"], "error");
}

#[test]
fn malformed_size_is_an_argument_error() {
    let out = run_loom(&["run", "--headless", "--frames", "1", "--size", "abc"]);
    let r = report(&out);
    assert_eq!(out.code, 2);
    assert_eq!(r["status"], "error");
}

/// Writes an input script with the given events (RON event tuples) and
/// returns its path.
fn write_script(dir: &std::path::Path, name: &str, events: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let body = format!(
        "(version: 1, tick_hz: 60, events: [{}])\n",
        events.join(", ")
    );
    fs::write(&path, body).expect("script file can be written");
    path
}

/// Writes an input script with no action events and the given axis events
/// (RON tuples), and returns its path.
fn write_axis_script(dir: &std::path::Path, name: &str, axes: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let body = format!(
        "(version: 1, tick_hz: 60, events: [], axes: [{}])\n",
        axes.join(", ")
    );
    fs::write(&path, body).expect("script file can be written");
    path
}

fn run_headless_replay(frames: &str, script: &std::path::Path, state: &std::path::Path) -> Value {
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        frames,
        "--input",
        path_str(script),
        "--dump-state",
        path_str(state),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    r
}

#[test]
fn replayed_forward_run_writes_the_expected_state() {
    let dir = test_dir("replayed_forward_run_writes_the_expected_state");
    let script = write_axis_script(
        &dir,
        "s1.ron",
        &[
            r#"(tick: 1, axis: "move_y", value: 1.0)"#,
            r#"(tick: 61, axis: "move_y", value: 0.0)"#,
        ],
    );
    let state = dir.join("s.json");
    let r = run_headless_replay("90", &script, &state);
    assert_eq!(
        r["input"],
        serde_json::json!({"path": path_str(&script), "events": 2, "applied": 2})
    );
    assert_eq!(
        fs::read_to_string(&state).expect("state file was written"),
        SCENARIO_B
    );
}

/// Replays `axes` headless for `frames` ticks and returns the state dump.
fn replay_state(name: &str, frames: &str, axes: &[&str]) -> Value {
    let dir = test_dir(name);
    let script = write_axis_script(&dir, "script.ron", axes);
    let state = dir.join("s.json");
    run_headless_replay(frames, &script, &state);
    serde_json::from_str(&fs::read_to_string(&state).expect("state file was written"))
        .expect("state file is valid JSON")
}

fn assert_f32_value_bits(value: &Value, expected: f32) {
    let actual = value.as_f64().expect("value is a number") as f32;
    assert_eq!(
        actual.to_bits(),
        expected.to_bits(),
        "{actual} != {expected}"
    );
}

fn assert_quat_bits(value: &Value, expected: [f32; 4]) {
    let actual: Vec<u32> = value
        .as_array()
        .expect("value is an array")
        .iter()
        .map(|v| (v.as_f64().expect("number") as f32).to_bits())
        .collect();
    assert_eq!(
        actual,
        expected.map(f32::to_bits),
        "{value} != {expected:?}"
    );
}

/// Checks `game.player` and the `player` node.
fn assert_player(dump: &Value, position: [f32; 3], facing: f32, rotation: [f32; 4]) {
    assert_f32_bits(&dump["game"]["player"]["position"], position);
    assert_f32_value_bits(&dump["game"]["player"]["facing"], facing);
    assert_f32_bits(&dump["nodes"]["player"]["position"], position);
    assert_quat_bits(&dump["nodes"]["player"]["rotation"], rotation);
}

#[test]
fn sideways_run_turns_the_player_to_face_right() {
    let dump = replay_state(
        "sideways_run_turns_the_player_to_face_right",
        "30",
        &[r#"(tick: 1, axis: "move_x", value: 1.0)"#],
    );
    assert_eq!(dump["tick"], 30);
    assert_player(
        &dump,
        [2.0000007, 0.0, 0.0],
        -1.5707964,
        [0.0, -0.70710677, 0.0, 0.70710677],
    );
    assert_f32_bits(
        &dump["nodes"]["main_cam"]["position"],
        [2.0000007, 2.5352764, 3.8637035],
    );
}

#[test]
fn running_while_turning_the_camera_replays_to_the_expected_state() {
    let dump = replay_state(
        "running_while_turning_the_camera_replays_to_the_expected_state",
        "60",
        &[
            r#"(tick: 1, axis: "move_y", value: 1.0)"#,
            r#"(tick: 1, axis: "look_x", value: 0.04)"#,
            r#"(tick: 31, axis: "look_x", value: 0.0)"#,
        ],
    );
    assert_eq!(dump["tick"], 60);
    assert_eq!(
        dump["axes"],
        serde_json::json!({"look_x": 0.0, "look_y": 0.0, "move_x": 0.0, "move_y": 1.0})
    );
    assert_player(
        &dump,
        [2.9577415, 0.0, -2.2566528],
        -1.1999999,
        [0.0, -0.5646424, 0.0, 0.8253356],
    );
    assert_f32_value_bits(&dump["game"]["camera"]["yaw"], -1.1999999);
    assert_f32_value_bits(&dump["game"]["camera"]["distance"], 4.0);
    assert_f32_bits(
        &dump["nodes"]["main_cam"]["position"],
        [-0.6433809, 2.5352762, -0.8566092],
    );
    assert_quat_bits(
        &dump["nodes"]["main_cam"]["rotation"],
        [-0.10772792, -0.55981183, -0.07370063, 0.8182748],
    );
}

#[test]
fn pillar_blocks_a_replayed_forward_run() {
    let dump = replay_state(
        "pillar_blocks_a_replayed_forward_run",
        "150",
        &[r#"(tick: 1, axis: "move_y", value: 1.0)"#],
    );
    assert_eq!(dump["tick"], 150);
    for position in [
        &dump["game"]["player"]["position"],
        &dump["nodes"]["player"]["position"],
    ] {
        assert_f32_bits(position, [0.0, 0.0, -8.15]);
    }
}

#[test]
fn state_dump_camera_yaw_wraps_into_minus_pi_to_pi() {
    let dump = replay_state(
        "state_dump_camera_yaw_wraps_into_minus_pi_to_pi",
        "40",
        &[r#"(tick: 1, axis: "look_x", value: -0.1)"#],
    );
    // 40 * 0.1 = 4.0 without wrapping.
    assert_f32_value_bits(&dump["game"]["camera"]["yaw"], -2.2831872);
    assert_f32_value_bits(&dump["game"]["camera"]["distance"], 4.0);
    assert_f32_bits(
        &dump["nodes"]["main_cam"]["position"],
        [-2.924056, 2.5352764, -2.5254908],
    );
}

/// The canonical recording of a diagonal run plus a one-tick look.
const AXIS_RECORDING: &str = concat!(
    "(\n",
    "    version: 1,\n",
    "    tick_hz: 60,\n",
    "    events: [],\n",
    "    axes: [\n",
    "        (tick: 1, axis: \"move_x\", value: 1.0),\n",
    "        (tick: 1, axis: \"move_y\", value: 0.5),\n",
    "        (tick: 20, axis: \"look_x\", value: 0.0175),\n",
    "        (tick: 21, axis: \"look_x\", value: 0.0),\n",
    "        (tick: 46, axis: \"move_x\", value: 0.0),\n",
    "        (tick: 46, axis: \"move_y\", value: 0.0),\n",
    "    ],\n",
    ")\n",
);

#[test]
fn recording_while_replaying_axes_reproduces_the_sorted_script() {
    let dir = test_dir("recording_while_replaying_axes_reproduces_the_sorted_script");
    // Same-tick events deliberately out of name order.
    let script = write_axis_script(
        &dir,
        "unsorted.ron",
        &[
            r#"(tick: 1, axis: "move_y", value: 0.5)"#,
            r#"(tick: 1, axis: "move_x", value: 1.0)"#,
            r#"(tick: 20, axis: "look_x", value: 0.0175)"#,
            r#"(tick: 21, axis: "look_x", value: 0.0)"#,
            r#"(tick: 46, axis: "move_y", value: 0.0)"#,
            r#"(tick: 46, axis: "move_x", value: 0.0)"#,
        ],
    );
    let record = dir.join("rec/out.ron");
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "60",
        "--input",
        path_str(&script),
        "--record",
        path_str(&record),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["record"], path_str(&record));
    assert_eq!(
        fs::read_to_string(&record).expect("recording was written"),
        AXIS_RECORDING
    );
}

#[test]
fn invalid_input_script_is_a_runtime_error_naming_the_event() {
    let dir = test_dir("invalid_input_script_is_a_runtime_error_naming_the_event");
    // The demo defines no actions, so any action is unknown.
    let script = write_script(
        &dir,
        "bad.ron",
        &[r#"(tick: 42, action: "move_left", down: true)"#],
    );
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "60",
        "--input",
        path_str(&script),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1);
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    for part in [path_str(&script), "events[0]", "tick 42"] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn invalid_axis_event_is_a_runtime_error_naming_it() {
    let dir = test_dir("invalid_axis_event_is_a_runtime_error_naming_it");
    let script = write_axis_script(
        &dir,
        "bad.ron",
        &[
            r#"(tick: 1, axis: "move_x", value: 1.0)"#,
            r#"(tick: 42, axis: "move_x", value: 1.0)"#,
        ],
    );
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "60",
        "--input",
        path_str(&script),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1);
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    for part in [path_str(&script), "axes[1]", "tick 42"] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn input_events_after_the_final_tick_are_counted_as_unapplied() {
    let dir = test_dir("input_events_after_the_final_tick_are_counted_as_unapplied");
    let script = write_axis_script(
        &dir,
        "late.ron",
        &[r#"(tick: 100, axis: "move_y", value: -1.0)"#],
    );
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "60",
        "--input",
        path_str(&script),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["input"]["events"], 1);
    assert_eq!(r["input"]["applied"], 0);
}

/// A minimal valid scene: a character `hero` (line 6) and its orbit camera.
const MINIMAL_SCENE: &str = r#"(
    version: 1,
    camera: "cam",
    ambient: (0.3, 0.3, 0.3),
    nodes: [
        (name: "hero", mesh: Capsule(radius: 0.3, height: 1.6), character: (run_speed: 2.0, turn_speed: 360.0)),
        (name: "cam", camera: (fov_y: 50.0, near: 0.1, far: 50.0),
            orbit_camera: (target: "hero", pivot_height: 1.0, distance: 3.0,
                yaw: 0.0, pitch: -10.0, pitch_min: -60.0, pitch_max: 30.0,
                collision_radius: 0.2, return_speed: 4.0)),
    ],
)
"#;

fn write_file(dir: &std::path::Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("file can be written");
    path
}

#[test]
fn custom_scene_is_loaded_and_reported() {
    let dir = test_dir("custom_scene_is_loaded_and_reported");
    let scene = write_file(&dir, "min.scene.ron", MINIMAL_SCENE);
    let state = dir.join("s.json");
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "10",
        "--scene",
        path_str(&scene),
        "--dump-state",
        path_str(&state),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["scene"], path_str(&scene));
    let dump: Value =
        serde_json::from_str(&fs::read_to_string(&state).expect("state file was written"))
            .expect("state file is valid JSON");
    let nodes: Vec<&String> = dump["nodes"]
        .as_object()
        .expect("nodes is an object")
        .keys()
        .collect();
    assert_eq!(nodes, ["cam", "hero"]);
}

#[test]
fn scene_without_a_character_is_a_runtime_error() {
    let dir = test_dir("scene_without_a_character_is_a_runtime_error");
    // Without the character, the orbit camera has to go too.
    let start = MINIMAL_SCENE
        .find(",\n            orbit_camera")
        .expect("scene has an orbit camera");
    let end = MINIMAL_SCENE
        .find("return_speed: 4.0)")
        .expect("scene has return_speed")
        + "return_speed: 4.0)".len();
    let src = format!("{}{}", &MINIMAL_SCENE[..start], &MINIMAL_SCENE[end..])
        .replace(", character: (run_speed: 2.0, turn_speed: 360.0)", "");
    let scene = write_file(&dir, "nocharacter.scene.ron", &src);
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1);
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    for part in [path_str(&scene), "character"] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn invalid_scene_is_a_runtime_error_naming_the_node() {
    let dir = test_dir("invalid_scene_is_a_runtime_error_naming_the_node");
    let scene = write_file(
        &dir,
        "bad.scene.ron",
        &MINIMAL_SCENE.replace(
            r#"        (name: "cam","#,
            r#"        (name: "arena", children: [(name: "pillar_2", mesh: Cube, scale: (1.0, 0.0, 1.0))]),
        (name: "cam","#,
        ),
    );
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1);
    let error = r["error"].as_str().expect("error is a string");
    for part in [path_str(&scene), "arena/pillar_2"] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn malformed_scene_reports_line_and_column() {
    let dir = test_dir("malformed_scene_reports_line_and_column");
    let scene = write_file(
        &dir,
        "typo.scene.ron",
        &MINIMAL_SCENE.replace(
            "mesh: Capsule(radius: 0.3, height: 1.6)",
            "mesh: Capsule(radius: 0.3, height: 1.6), colour: (1.0, 0.0, 0.0)",
        ),
    );
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1);
    let error = r["error"].as_str().expect("error is a string");
    assert!(error.contains(path_str(&scene)), "{error}");
    assert!(error.contains("colour"), "{error}");
    // ron reports the span as line:column-line:column; the typo is on line 6.
    let span = regex_like_span(error);
    assert!(
        span.is_some_and(|(line, _)| line == 6),
        "no line:column span in {error}"
    );
}

/// Finds the first `L:C` (digits, colon, digits) in `text`.
fn regex_like_span(text: &str) -> Option<(u32, u32)> {
    text.split(|c: char| !(c.is_ascii_digit() || c == ':'))
        .filter_map(|word| {
            let (line, col) = word.split_once(':')?;
            Some((line.parse().ok()?, col.parse().ok()?))
        })
        .next()
}

#[test]
fn headless_run_reports_zero_reloads() {
    let dir = test_dir("headless_run_reports_zero_reloads");
    let scene = write_file(&dir, "min.scene.ron", MINIMAL_SCENE);
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "10",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["reloads"], 0);
    assert_eq!(r["reload_errors"], 0);
}

/// The project's tile sheet as an absolute path with forward slashes, so it
/// can be written into a RON string on any platform.
fn tiles_sheet() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/art/tiles.json")
        .display()
        .to_string()
        .replace('\\', "/")
}

/// A 2D scene (no character) whose tilemap `map` uses `sheet` and draws
/// `'.'` with `floor_tag`.
fn tilemap_scene(sheet: &str, floor_tag: &str) -> String {
    format!(
        r#"(
    version: 1,
    camera: "cam",
    ambient: (1.0, 1.0, 1.0),
    nodes: [
        (name: "cam", position: (0.0, 0.0, 10.0), camera: (ortho: (height: 4.0), near: 0.1, far: 100.0)),
        (name: "map", tilemap: (sheet: "{sheet}", legend: [('.', "{floor_tag}")], rows: [".."])),
    ],
)
"#
    )
}

/// [`MINIMAL_SCENE`] plus a sprite node `badge` showing `tag` of the tile sheet.
fn scene_with_sprite(tag: &str) -> String {
    let sprite = format!(
        "        (name: \"badge\", position: (0.0, 2.0, 0.0), sprite: (sheet: \"{}\", tag: \"{tag}\")),\n    ],",
        tiles_sheet()
    );
    MINIMAL_SCENE.replacen("\n    ],", &format!("\n{sprite}"), 1)
}

#[test]
fn view_without_screenshot_is_an_argument_error() {
    let out = run_loom(&["view"]);
    let r = report(&out);
    assert_eq!(out.code, 2);
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    assert!(error.contains("--screenshot"), "{error}");
}

#[test]
fn view_with_a_misspelled_tag_fails_before_rendering() {
    let dir = test_dir("view_with_a_misspelled_tag_fails_before_rendering");
    let scene = write_file(
        &dir,
        "map.scene.ron",
        &tilemap_scene(&tiles_sheet(), "flor"),
    );
    let shot = dir.join("shot.png");
    let out = run_loom(&[
        "view",
        "--scene",
        path_str(&scene),
        "--screenshot",
        path_str(&shot),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1, "{r}");
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    let expected = format!(
        "node map: tilemap: sheet {} has no tag \"flor\"",
        tiles_sheet()
    );
    assert!(
        error.contains(&expected),
        "error should contain {expected:?}: {error}"
    );
    assert!(!shot.exists(), "no screenshot is written");
}

#[test]
fn view_with_a_missing_sheet_is_a_runtime_error() {
    let dir = test_dir("view_with_a_missing_sheet_is_a_runtime_error");
    let missing = dir
        .join("nowhere.json")
        .display()
        .to_string()
        .replace('\\', "/");
    let scene = write_file(&dir, "map.scene.ron", &tilemap_scene(&missing, "floor"));
    let out = run_loom(&[
        "view",
        "--scene",
        path_str(&scene),
        "--screenshot",
        path_str(&dir.join("shot.png")),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1, "{r}");
    let error = r["error"].as_str().expect("error is a string");
    for part in ["node map: tilemap", missing.as_str()] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn headless_run_without_screenshot_still_checks_sprite_tags() {
    let dir = test_dir("headless_run_without_screenshot_still_checks_sprite_tags");
    let scene = write_file(&dir, "badge.scene.ron", &scene_with_sprite("playr"));
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1, "{r}");
    let error = r["error"].as_str().expect("error is a string");
    for part in [
        path_str(&scene),
        "node badge: sprite",
        "has no tag \"playr\"",
    ] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn headless_run_accepts_a_scene_with_a_valid_sprite() {
    let dir = test_dir("headless_run_accepts_a_scene_with_a_valid_sprite");
    let scene = write_file(&dir, "badge.scene.ron", &scene_with_sprite("player"));
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["adapter"], Value::Null);
}
