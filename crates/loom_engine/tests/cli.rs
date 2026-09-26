//! CLI contract tests: run the real `loom` binary and check stdout, exit
//! code and output files.
//!
//! Only GPU-free invocations belong here (headless, no `--screenshot`), so
//! that `verify.ps1` keeps working on machines without a GPU.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

/// State after 60 ticks of the built-in scene without input.
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
    "    \"cube\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.5,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.1311443,\n",
    "        0.47404212,\n",
    "        -0.07164446,\n",
    "        0.86772823\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
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
    "        20.0,\n",
    "        1.0,\n",
    "        20.0\n",
    "      ]\n",
    "    },\n",
    "    \"main_cam\": {\n",
    "      \"position\": [\n",
    "        2.5,\n",
    "        2.0,\n",
    "        3.5\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        -0.15902498,\n",
    "        0.30089226,\n",
    "        0.05096195,\n",
    "        0.9389238\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
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
    "    }\n",
    "  },\n",
    "  \"demo\": {\n",
    "    \"yaw\": 1.0,\n",
    "    \"position\": [\n",
    "      0.0,\n",
    "      0.5,\n",
    "      0.0\n",
    "    ]\n",
    "  }\n",
    "}\n",
);

/// State after 90 ticks, `move_x` at 1.0 for ticks 1..=60.
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
    "    \"cube\": {\n",
    "      \"position\": [\n",
    "        1.9999994,\n",
    "        0.5,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.109342225,\n",
    "        0.6739847,\n",
    "        -0.10186283,\n",
    "        0.7234728\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
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
    "        20.0,\n",
    "        1.0,\n",
    "        20.0\n",
    "      ]\n",
    "    },\n",
    "    \"main_cam\": {\n",
    "      \"position\": [\n",
    "        2.5,\n",
    "        2.0,\n",
    "        3.5\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        -0.15902498,\n",
    "        0.30089226,\n",
    "        0.05096195,\n",
    "        0.9389238\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
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
    "    }\n",
    "  },\n",
    "  \"demo\": {\n",
    "    \"yaw\": 1.5,\n",
    "    \"position\": [\n",
    "      1.9999994,\n",
    "      0.5,\n",
    "      0.0\n",
    "    ]\n",
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
fn state_dump_yaw_wraps_into_zero_to_tau() {
    let file = test_dir("state_dump_yaw_wraps_into_zero_to_tau").join("s.json");
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "400",
        "--dump-state",
        path_str(&file),
    ]);
    assert_eq!(out.code, 0);
    report(&out);
    let state: Value =
        serde_json::from_str(&fs::read_to_string(&file).expect("state file was written"))
            .expect("state file is valid JSON");
    assert_eq!(state["tick"], 400);
    assert_f64_bits(&state["sim_time"], 6.666666666666667);
    assert_f64_bits(&state["demo"]["yaw"], 0.38348135948708073);
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
fn replayed_input_moves_the_cube_to_the_expected_state() {
    let dir = test_dir("replayed_input_moves_the_cube_to_the_expected_state");
    let script = write_axis_script(
        &dir,
        "b.ron",
        &[
            r#"(tick: 1, axis: "move_x", value: 1.0)"#,
            r#"(tick: 61, axis: "move_x", value: 0.0)"#,
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

#[test]
// 0.707107 is the accumulated position, not an approximation of 1/sqrt(2).
#[allow(clippy::approx_constant)]
fn diagonal_replay_matches_the_expected_state() {
    let dir = test_dir("diagonal_replay_matches_the_expected_state");
    let script = write_axis_script(
        &dir,
        "c.ron",
        &[
            r#"(tick: 1, axis: "move_x", value: 1.0)"#,
            r#"(tick: 1, axis: "move_y", value: 1.0)"#,
        ],
    );
    let state = dir.join("s.json");
    run_headless_replay("30", &script, &state);
    let dump: Value =
        serde_json::from_str(&fs::read_to_string(&state).expect("state file was written"))
            .expect("state file is valid JSON");
    assert_eq!(dump["tick"], 30);
    assert_eq!(dump["actions"], serde_json::json!([]));
    assert_eq!(
        dump["axes"],
        serde_json::json!({"look_x": 0.0, "look_y": 0.0, "move_x": 1.0, "move_y": 1.0})
    );
    for position in [
        &dump["demo"]["position"],
        &dump["nodes"]["cube"]["position"],
    ] {
        assert_f32_bits(position, [0.707107, 0.5, -0.707107]);
    }
}

/// The canonical recording of scenario D plus a one-tick look.
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
fn axis_script_moves_the_cube_with_normalized_diagonal() {
    let dir = test_dir("axis_script_moves_the_cube_with_normalized_diagonal");
    let script = write_axis_script(
        &dir,
        "d.ron",
        &[
            r#"(tick: 1, axis: "move_x", value: 1.0)"#,
            r#"(tick: 1, axis: "move_y", value: 0.5)"#,
        ],
    );
    let state = dir.join("s.json");
    run_headless_replay("45", &script, &state);
    let dump: Value =
        serde_json::from_str(&fs::read_to_string(&state).expect("state file was written"))
            .expect("state file is valid JSON");
    assert_eq!(dump["tick"], 45);
    for position in [
        &dump["demo"]["position"],
        &dump["nodes"]["cube"]["position"],
    ] {
        assert_f32_bits(position, [1.3416408, 0.5, -0.6708204]);
    }
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

/// A minimal valid scene: the demo's `cube` plus a camera.
const MINIMAL_SCENE: &str = r#"(
    version: 1,
    camera: "cam",
    ambient: (0.3, 0.3, 0.3),
    nodes: [
        (name: "cube", mesh: Cube),
        (name: "cam", position: (0.0, 1.0, 4.0), look_at: (0.0, 0.0, 0.0),
            camera: (fov_y: 50.0, near: 0.1, far: 50.0)),
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
    assert_eq!(nodes, ["cam", "cube"]);
}

#[test]
fn scene_without_a_cube_is_a_runtime_error() {
    let dir = test_dir("scene_without_a_cube_is_a_runtime_error");
    let scene = write_file(
        &dir,
        "nocube.scene.ron",
        &MINIMAL_SCENE.replace(
            r#"(name: "cube", mesh: Cube),"#,
            r#"(name: "box", mesh: Cube),"#,
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
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    for part in [path_str(&scene), "cube"] {
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
            r#"(name: "cube", mesh: Cube),"#,
            r#"(name: "cube", mesh: Cube),
        (name: "arena", children: [(name: "pillar_2", mesh: Cube, scale: (1.0, 0.0, 1.0))]),"#,
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
        &MINIMAL_SCENE.replace("mesh: Cube", "mesh: Cube, colour: (1.0, 0.0, 0.0)"),
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
