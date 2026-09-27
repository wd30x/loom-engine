//! CLI contract tests: run the real `loom` binary and check stdout, exit
//! code and output files.
//!
//! Only GPU-free invocations belong here (headless, no `--screenshot`), so
//! that `verify.ps1` keeps working on machines without a GPU.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

/// State after 60 ticks of the G2.3 demo scene without input (scenario A).
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
    "      \"facing\": 0.0,\n",
    "      \"stamina\": 100.0,\n",
    "      \"stamina_wait\": 0,\n",
    "      \"action\": \"idle\",\n",
    "      \"action_tick\": 0,\n",
    "      \"invulnerable\": false,\n",
    "      \"attack_phase\": null,\n",
    "      \"combo\": 0,\n",
    "      \"buffered\": null,\n",
    "      \"hits\": 0,\n",
    "      \"last_hit\": null,\n",
    "      \"health\": {\n",
    "        \"hp\": 100.0,\n",
    "        \"state\": \"ok\",\n",
    "        \"state_ticks\": 0,\n",
    "        \"impact\": 0.0,\n",
    "        \"impact_wait\": 0\n",
    "      }\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": [],\n",
    "    \"swingers\": [],\n",
    "    \"enemies\": []\n",
    "  }\n",
    "}\n",
);

/// State after 90 ticks of the G2.3 demo scene, `move_y` at 1.0 for ticks
/// 1..=60 (scenario S1).
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
    "      \"facing\": 0.0,\n",
    "      \"stamina\": 100.0,\n",
    "      \"stamina_wait\": 0,\n",
    "      \"action\": \"idle\",\n",
    "      \"action_tick\": 0,\n",
    "      \"invulnerable\": false,\n",
    "      \"attack_phase\": null,\n",
    "      \"combo\": 0,\n",
    "      \"buffered\": null,\n",
    "      \"hits\": 0,\n",
    "      \"last_hit\": null,\n",
    "      \"health\": {\n",
    "        \"hp\": 100.0,\n",
    "        \"state\": \"ok\",\n",
    "        \"state_ticks\": 0,\n",
    "        \"impact\": 0.0,\n",
    "        \"impact_wait\": 0\n",
    "      }\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": [],\n",
    "    \"swingers\": [],\n",
    "    \"enemies\": []\n",
    "  }\n",
    "}\n",
);

struct Output {
    code: i32,
    stdout: String,
}

fn run_loom(args: &[&str]) -> Output {
    run_loom_in(std::path::Path::new("."), args)
}

/// The repository root: scenes under `assets/` name their sheets relative
/// to it.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// [`run_loom`] with `dir` as the working directory.
fn run_loom_in(dir: &std::path::Path, args: &[&str]) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_loom"))
        .args(args)
        .current_dir(dir)
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
    assert!(r["scene"].is_null());
    assert_eq!(r["frames"], 60);
    assert_eq!(r["ticks"], 60);
    assert_f64_bits(&r["sim_time"], 1.0);
}

#[test]
fn state_dump_after_sixty_ticks_matches_expected_bytes() {
    let file = test_dir("state_dump_after_sixty_ticks_matches_expected_bytes").join("s.json");
    // The G2.3 scene: the dummies would add nodes to the dump.
    let scene = g2_3_demo();
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "60",
        "--dump-state",
        path_str(&file),
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0);
    assert_eq!(r["state"], path_str(&file));
    assert!(r["record"].is_null());
    assert!(r["input"].is_null());
    assert_eq!(r["scene"], path_str(&scene));
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
    run_headless_replay_on(None, frames, script, state)
}

/// [`run_headless_replay`] on `scene`, or the built-in scene without one.
fn run_headless_replay_on(
    scene: Option<&std::path::Path>,
    frames: &str,
    script: &std::path::Path,
    state: &std::path::Path,
) -> Value {
    let mut args = vec![
        "run",
        "--headless",
        "--frames",
        frames,
        "--input",
        path_str(script),
        "--dump-state",
        path_str(state),
    ];
    if let Some(scene) = scene {
        args.extend(["--scene", path_str(scene)]);
    }
    let out = run_loom(&args);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    r
}

/// The built-in scene as it was in G2.3, before the lock-on dummies:
/// tests whose results the dummies would change run on it.
fn g2_3_demo() -> PathBuf {
    repo_root().join("games/souls/src/g2_3_demo.scene.ron")
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
    // The G2.3 scene: the dummies would add nodes to the dump.
    let r = run_headless_replay_on(Some(&g2_3_demo()), "90", &script, &state);
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
    replay_state_on(None, name, frames, axes)
}

/// [`replay_state`] on `scene`, or the built-in scene without one.
fn replay_state_on(
    scene: Option<&std::path::Path>,
    name: &str,
    frames: &str,
    axes: &[&str],
) -> Value {
    let dir = test_dir(name);
    let script = write_axis_script(&dir, "script.ron", axes);
    let state = dir.join("s.json");
    run_headless_replay_on(scene, frames, &script, &state);
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
    // The G2.3 scene: dummy_b now stands in front of pillar c.
    let dump = replay_state_on(
        Some(&g2_3_demo()),
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

/// A minimal valid souls scene: a character `hero` (line 6) and its orbit
/// camera.
const MINIMAL_SCENE: &str = r#"(
    version: 1, game: "souls",
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

/// The planner's prototype run (E4.4): 13 key presses, the k-th pressed on
/// tick 2k - 1 and released on tick 2k: wait x5, move_e x4, move_n, wait x3.
/// `zombie` walks up to the player, bites once and is killed in three hits;
/// the player steps onto its corpse and north, and `zombie_2` comes round
/// the L-shaped wall to stand next to the player.
const ROGUE_SCRIPT: [&str; 26] = [
    r#"(tick: 1, action: "wait", down: true)"#,
    r#"(tick: 2, action: "wait", down: false)"#,
    r#"(tick: 3, action: "wait", down: true)"#,
    r#"(tick: 4, action: "wait", down: false)"#,
    r#"(tick: 5, action: "wait", down: true)"#,
    r#"(tick: 6, action: "wait", down: false)"#,
    r#"(tick: 7, action: "wait", down: true)"#,
    r#"(tick: 8, action: "wait", down: false)"#,
    r#"(tick: 9, action: "wait", down: true)"#,
    r#"(tick: 10, action: "wait", down: false)"#,
    r#"(tick: 11, action: "move_e", down: true)"#,
    r#"(tick: 12, action: "move_e", down: false)"#,
    r#"(tick: 13, action: "move_e", down: true)"#,
    r#"(tick: 14, action: "move_e", down: false)"#,
    r#"(tick: 15, action: "move_e", down: true)"#,
    r#"(tick: 16, action: "move_e", down: false)"#,
    r#"(tick: 17, action: "move_e", down: true)"#,
    r#"(tick: 18, action: "move_e", down: false)"#,
    r#"(tick: 19, action: "move_n", down: true)"#,
    r#"(tick: 20, action: "move_n", down: false)"#,
    r#"(tick: 21, action: "wait", down: true)"#,
    r#"(tick: 22, action: "wait", down: false)"#,
    r#"(tick: 23, action: "wait", down: true)"#,
    r#"(tick: 24, action: "wait", down: false)"#,
    r#"(tick: 25, action: "wait", down: true)"#,
    r#"(tick: 26, action: "wait", down: false)"#,
];

/// State after [`ROGUE_SCRIPT`] and 28 ticks of `assets/scenes/rogue.ron`,
/// as the planner's independent prototype computed it (docs/PLAN.md, E4.4):
/// 13 turns, the player on (4, 1) with 9 hp, `zombie` a corpse on (4, 2)
/// drawn at z (0 + 0.5) / 2 = 0.25, `zombie_2` on (5, 1).
const ROGUE_STATE: &str = concat!(
    "{\n",
    "  \"tick\": 28,\n",
    "  \"sim_time\": 0.4666666666666667,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {},\n",
    "  \"nodes\": {\n",
    "    \"cam\": {\n",
    "      \"position\": [\n",
    "        9.5,\n",
    "        -5.125,\n",
    "        10.0\n",
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
    "    \"map\": {\n",
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
    "    \"player\": {\n",
    "      \"position\": [\n",
    "        4.0,\n",
    "        -1.0,\n",
    "        1.0\n",
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
    "    \"zombie\": {\n",
    "      \"position\": [\n",
    "        4.0,\n",
    "        -2.0,\n",
    "        0.25\n",
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
    "    \"zombie_2\": {\n",
    "      \"position\": [\n",
    "        5.0,\n",
    "        -1.0,\n",
    "        0.5\n",
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
    "    }\n",
    "  },\n",
    "  \"game\": {\n",
    "    \"turn\": 13,\n",
    "    \"player\": {\n",
    "      \"cell\": [\n",
    "        4,\n",
    "        1\n",
    "      ],\n",
    "      \"hp\": 9\n",
    "    },\n",
    "    \"zombies\": [\n",
    "      {\n",
    "        \"name\": \"zombie\",\n",
    "        \"cell\": [\n",
    "          4,\n",
    "          2\n",
    "        ],\n",
    "        \"hp\": 0,\n",
    "        \"energy\": 69\n",
    "      },\n",
    "      {\n",
    "        \"name\": \"zombie_2\",\n",
    "        \"cell\": [\n",
    "          5,\n",
    "          1\n",
    "        ],\n",
    "        \"hp\": 3,\n",
    "        \"energy\": 71\n",
    "      }\n",
    "    ],\n",
    "    \"messages\": [\n",
    "      {\n",
    "        \"turn\": 6,\n",
    "        \"text\": \"You hit zombie.\"\n",
    "      },\n",
    "      {\n",
    "        \"turn\": 6,\n",
    "        \"text\": \"zombie bites you.\"\n",
    "      },\n",
    "      {\n",
    "        \"turn\": 7,\n",
    "        \"text\": \"You hit zombie.\"\n",
    "      },\n",
    "      {\n",
    "        \"turn\": 8,\n",
    "        \"text\": \"You kill zombie.\"\n",
    "      }\n",
    "    ]\n",
    "  }\n",
    "}\n",
);

#[test]
fn rogue_replay_fights_a_zombie_to_the_prototype_state() {
    let dir = test_dir("rogue_replay_fights_a_zombie_to_the_prototype_state");
    let script = write_script(&dir, "rogue.ron", &ROGUE_SCRIPT);
    let state = dir.join("state.json");
    let out = run_loom_in(
        &repo_root(),
        &[
            "run",
            "--headless",
            "--frames",
            "28",
            "--scene",
            "assets/scenes/rogue.ron",
            "--input",
            path_str(&script),
            "--dump-state",
            path_str(&state),
        ],
    );
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["game"], "rogue");
    assert_eq!(r["scene"], "assets/scenes/rogue.ron");
    assert_eq!(r["input"]["applied"], 26);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, ROGUE_STATE);
}

#[test]
fn headless_run_checks_sprite_alternates_against_the_sheet() {
    let dir = test_dir("headless_run_checks_sprite_alternates_against_the_sheet");
    let src = scene_with_sprite("player").replacen(
        "tag: \"player\")",
        "tag: \"player\", alternates: [\"corpes\"])",
        1,
    );
    assert!(src.contains("corpes"));
    let scene = write_file(&dir, "badge.scene.ron", &src);
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
    assert_eq!(
        r["error"],
        format!(
            "invalid scene {}: node badge: sprite: sheet {} has no tag \"corpes\"",
            path_str(&scene),
            tiles_sheet()
        )
    );
}

#[test]
fn souls_run_reports_its_game() {
    let out = run_loom(&["run", "--headless", "--frames", "1"]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["game"], "souls");
}

/// Runs [`MINIMAL_SCENE`] with its `game: "souls",` replaced by `game`, and
/// returns the error of the (failed) run and the scene path.
fn run_with_game(test: &str, game: &str) -> (String, String) {
    let dir = test_dir(test);
    let src = MINIMAL_SCENE.replacen("game: \"souls\",", game, 1);
    assert_ne!(src, MINIMAL_SCENE);
    let scene = write_file(&dir, "game.scene.ron", &src);
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
    let error = r["error"].as_str().expect("error is a string").to_owned();
    (error, path_str(&scene).to_owned())
}

#[test]
fn scene_without_a_game_is_a_runtime_error_naming_both_games() {
    let (error, scene) = run_with_game(
        "scene_without_a_game_is_a_runtime_error_naming_both_games",
        "",
    );
    assert_eq!(
        error,
        format!(
            "invalid scene {scene}: the scene does not say which game runs it: \
             add game: \"rogue\" or game: \"souls\" at the top level"
        )
    );
}

#[test]
fn scene_with_an_unknown_game_is_a_runtime_error_listing_the_games() {
    let (error, scene) = run_with_game(
        "scene_with_an_unknown_game_is_a_runtime_error_listing_the_games",
        "game: \"rouge\",",
    );
    assert_eq!(
        error,
        format!(
            "invalid scene {scene}: unknown game \"rouge\": the games are \"rogue\" and \"souls\""
        )
    );
}

#[test]
fn view_without_scene_is_an_argument_error() {
    let dir = test_dir("view_without_scene_is_an_argument_error");
    let shot = dir.join("shot.png");
    let out = run_loom(&["view", "--screenshot", path_str(&shot)]);
    let r = report(&out);
    assert_eq!(out.code, 2, "{r}");
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    assert!(error.contains("--scene"), "{error}");
    assert!(!shot.exists());
}

/// The souls moves script: tap roll while running forward (a roll from
/// tick 2), stop, tap roll again (a backstep from tick 41), then hold roll
/// while running from tick 70 (a sprint from tick 85, the 16th held tick)
/// until tick 100.
const SOULS_MOVES_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"roll\", down: true), (tick: 2, action: \"roll\", down: false), ",
    "(tick: 40, action: \"roll\", down: true), (tick: 41, action: \"roll\", down: false), ",
    "(tick: 70, action: \"roll\", down: true), (tick: 100, action: \"roll\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_y\", value: 1.0), (tick: 34, axis: \"move_y\", value: 0.0), ",
    "(tick: 70, axis: \"move_y\", value: 1.0)",
    "])\n",
);

/// State after [`SOULS_MOVES_SCRIPT`] and 110 ticks of the G2.3 demo scene.
/// Determinism only: no independent source for every byte. The stamina
/// is checked by hand in the test.
const SOULS_MOVES_STATE: &str = concat!(
    "{\n",
    "  \"tick\": 110,\n",
    "  \"sim_time\": 1.8333333333333333,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {\n",
    "    \"look_x\": 0.0,\n",
    "    \"look_y\": 0.0,\n",
    "    \"move_x\": 0.0,\n",
    "    \"move_y\": 1.0\n",
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
    "        -1.1362922\n",
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
    "        -4.9999957\n",
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
    "        -4.9999957\n",
    "      ],\n",
    "      \"facing\": 0.0,\n",
    "      \"stamina\": 64.25,\n",
    "      \"stamina_wait\": 19,\n",
    "      \"action\": \"idle\",\n",
    "      \"action_tick\": 0,\n",
    "      \"invulnerable\": false,\n",
    "      \"attack_phase\": null,\n",
    "      \"combo\": 0,\n",
    "      \"buffered\": null,\n",
    "      \"hits\": 0,\n",
    "      \"last_hit\": null,\n",
    "      \"health\": {\n",
    "        \"hp\": 100.0,\n",
    "        \"state\": \"ok\",\n",
    "        \"state_ticks\": 0,\n",
    "        \"impact\": 0.0,\n",
    "        \"impact_wait\": 0\n",
    "      }\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": [],\n",
    "    \"swingers\": [],\n",
    "    \"enemies\": []\n",
    "  }\n",
    "}\n",
);

#[test]
fn souls_roll_backstep_and_sprint_replay_to_the_expected_state() {
    let dir = test_dir("souls_roll_backstep_and_sprint_replay_to_the_expected_state");
    let script = write_file(&dir, "moves.ron", SOULS_MOVES_SCRIPT);
    let state = dir.join("state.json");
    // The G2.3 scene: the dummies would add nodes to the dump.
    run_headless_replay_on(Some(&g2_3_demo()), "110", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_MOVES_STATE);
    // By hand, with the default props: the roll costs 20 (80 left, 30
    // ticks to wait); ticks 34..=40 count 7 down; the backstep costs 12
    // (68, wait 30); ticks 61..=84 count 24 down; sprinting on ticks
    // 85..=99 drains 15 x 15/60 = 3.75 (64.25, wait 30 each tick); ticks
    // 100..=110 count 11 down: 19 left, nothing regenerated yet.
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    let player = &dump["game"]["player"];
    assert_eq!(player["stamina"], 64.25);
    assert_eq!(player["stamina_wait"], 19);
    assert_eq!(player["action"], "idle");
    // Forward 1/15 + 3.2 (roll), back 1.5 (backstep), forward 1 + 1.5
    // (15 run, 15 sprint ticks) + 11/15: z = -5, up to rounding.
    let z = player["position"][2].as_f64().expect("z is a number");
    assert!((z + 5.0).abs() < 1e-4, "{z}");
}

/// A light attack on tick 1, two more pressed during the recovery of the
/// one before (ticks 20 and 50) and a heavy one pressed during the third
/// (tick 80), running forward the whole time.
const SOULS_ATTACKS_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"attack\", down: true), (tick: 2, action: \"attack\", down: false), ",
    "(tick: 20, action: \"attack\", down: true), (tick: 21, action: \"attack\", down: false), ",
    "(tick: 50, action: \"attack\", down: true), (tick: 51, action: \"attack\", down: false), ",
    "(tick: 80, action: \"heavy\", down: true), (tick: 81, action: \"heavy\", down: false)",
    "], axes: [(tick: 1, axis: \"move_y\", value: 1.0)])\n",
);

/// State after [`SOULS_ATTACKS_SCRIPT`] and 120 ticks of the G2.3 demo
/// scene. Determinism only: no independent source for every byte. The
/// stamina, the combo and the action are checked by hand in the test.
const SOULS_ATTACKS_STATE: &str = concat!(
    "{\n",
    "  \"tick\": 120,\n",
    "  \"sim_time\": 2.0,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {\n",
    "    \"look_x\": 0.0,\n",
    "    \"look_y\": 0.0,\n",
    "    \"move_x\": 0.0,\n",
    "    \"move_y\": 1.0\n",
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
    "        2.3637035\n",
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
    "        -1.5\n",
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
    "        -1.5\n",
    "      ],\n",
    "      \"facing\": 0.0,\n",
    "      \"stamina\": 30.0,\n",
    "      \"stamina_wait\": 30,\n",
    "      \"action\": \"heavy\",\n",
    "      \"action_tick\": 30,\n",
    "      \"invulnerable\": false,\n",
    "      \"attack_phase\": \"recovery\",\n",
    "      \"combo\": 0,\n",
    "      \"buffered\": null,\n",
    "      \"hits\": 0,\n",
    "      \"last_hit\": null,\n",
    "      \"health\": {\n",
    "        \"hp\": 100.0,\n",
    "        \"state\": \"ok\",\n",
    "        \"state_ticks\": 0,\n",
    "        \"impact\": 0.0,\n",
    "        \"impact_wait\": 0\n",
    "      }\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": [],\n",
    "    \"swingers\": [],\n",
    "    \"enemies\": []\n",
    "  }\n",
    "}\n",
);

#[test]
fn souls_light_combo_then_heavy_replays_to_the_expected_state() {
    let dir = test_dir("souls_light_combo_then_heavy_replays_to_the_expected_state");
    let script = write_file(&dir, "attacks.ron", SOULS_ATTACKS_SCRIPT);
    // By hand, with the default props: light attacks last 10 + 4 + 16 = 30
    // ticks, so each buffered press starts the next one right after the
    // last: light on ticks 1, 31 and 61, heavy on 91.
    let third = dir.join("third.json");
    run_headless_replay_on(Some(&g2_3_demo()), "61", &script, &third);
    let dump: Value =
        serde_json::from_str(&fs::read_to_string(&third).expect("the state dump was written"))
            .expect("state file is valid JSON");
    let player = &dump["game"]["player"];
    assert_eq!(player["action"], "attack");
    assert_eq!(player["action_tick"], 1);
    assert_eq!(player["combo"], 3);

    let state = dir.join("state.json");
    // The G2.3 scene: the dummies would add nodes to the dump.
    run_headless_replay_on(Some(&g2_3_demo()), "120", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_ATTACKS_STATE);
    // Tick 120 is heavy tick 30 of 22 + 5 + 23: recovery. Stamina: 100 -
    // 3 x 15 - 25 = 30, the wait not counting down while attacking. The
    // slides: 3 x 0.3 + 0.6 = 1.5 forward, up to rounding.
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    let player = &dump["game"]["player"];
    assert_eq!(player["action"], "heavy");
    assert_eq!(player["action_tick"], 30);
    assert_eq!(player["attack_phase"], "recovery");
    assert_eq!(player["combo"], 0);
    assert_eq!(player["buffered"], Value::Null);
    assert_eq!(player["stamina"], 30.0);
    assert_eq!(player["stamina_wait"], 30);
    let z = player["position"][2].as_f64().expect("z is a number");
    assert!((z + 1.5).abs() < 1e-4, "{z}");
}

/// Lock on (tick 1: straight ahead is dummy_b), strafe right for a second
/// (ticks 10..=69), flick `look_x` once (tick 80: over the 0.1 threshold,
/// to dummy_c on the right), then let go (tick 100).
const SOULS_LOCK_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"lock\", down: true), (tick: 2, action: \"lock\", down: false), ",
    "(tick: 100, action: \"lock\", down: true), (tick: 101, action: \"lock\", down: false)",
    "], axes: [",
    "(tick: 10, axis: \"move_x\", value: 1.0), (tick: 70, axis: \"move_x\", value: 0.0), ",
    "(tick: 80, axis: \"look_x\", value: 0.12), (tick: 81, axis: \"look_x\", value: 0.0)",
    "])\n",
);

/// State after [`SOULS_LOCK_SCRIPT`] and 120 ticks of the built-in scene.
/// Determinism only: no independent source for every byte. Which target
/// is locked on when is checked in the test.
const SOULS_LOCK_STATE: &str = concat!(
    "{\n",
    "  \"tick\": 120,\n",
    "  \"sim_time\": 2.0,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {\n",
    "    \"look_x\": 0.0,\n",
    "    \"look_y\": 0.0,\n",
    "    \"move_x\": 0.0,\n",
    "    \"move_y\": 0.0\n",
    "  },\n",
    "  \"nodes\": {\n",
    "    \"dummy_a\": {\n",
    "      \"position\": [\n",
    "        -5.0,\n",
    "        0.0,\n",
    "        -6.0\n",
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
    "    \"dummy_a/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_a/body\": {\n",
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
    "    \"dummy_a/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_b\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        -6.5\n",
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
    "    \"dummy_b/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_b/body\": {\n",
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
    "    \"dummy_b/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_c\": {\n",
    "      \"position\": [\n",
    "        5.0,\n",
    "        0.0,\n",
    "        -6.0\n",
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
    "    \"dummy_c/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_c/body\": {\n",
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
    "    \"dummy_c/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
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
    "        40.0,\n",
    "        1.0,\n",
    "        40.0\n",
    "      ]\n",
    "    },\n",
    "    \"main_cam\": {\n",
    "      \"position\": [\n",
    "        2.8141937,\n",
    "        2.7655094,\n",
    "        2.5030198\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        -0.15899347,\n",
    "        -0.123853825,\n",
    "        -0.020108728,\n",
    "        0.97927374\n",
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
    "        3.758911,\n",
    "        0.0,\n",
    "        -1.1720309\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        -0.12547562,\n",
    "        0.0,\n",
    "        0.9920967\n",
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
    "    \"stake\": {\n",
    "      \"position\": [\n",
    "        6.0,\n",
    "        0.0,\n",
    "        2.0\n",
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
    "    \"stake/arm\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.1,\n",
    "        -1.2\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.1,\n",
    "        0.1,\n",
    "        2.0\n",
    "      ]\n",
    "    },\n",
    "    \"stake/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.8,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.3,\n",
    "        1.6,\n",
    "        0.3\n",
    "      ]\n",
    "    },\n",
    "    \"stake/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"stake/post\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.8,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.3,\n",
    "        1.6,\n",
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
    "        3.758911,\n",
    "        0.0,\n",
    "        -1.1720309\n",
    "      ],\n",
    "      \"facing\": -0.25161445,\n",
    "      \"stamina\": 100.0,\n",
    "      \"stamina_wait\": 0,\n",
    "      \"action\": \"idle\",\n",
    "      \"action_tick\": 0,\n",
    "      \"invulnerable\": false,\n",
    "      \"attack_phase\": null,\n",
    "      \"combo\": 0,\n",
    "      \"buffered\": null,\n",
    "      \"hits\": 0,\n",
    "      \"last_hit\": null,\n",
    "      \"health\": {\n",
    "        \"hp\": 100.0,\n",
    "        \"state\": \"ok\",\n",
    "        \"state_ticks\": 0,\n",
    "        \"impact\": 0.0,\n",
    "        \"impact_wait\": 0\n",
    "      }\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": -0.25161445,\n",
    "      \"pitch\": -0.3219082,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": [\n",
    "      {\n",
    "        \"node\": \"dummy_a\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"dummy_b\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"dummy_c\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"stake\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      }\n",
    "    ],\n",
    "    \"swingers\": [\n",
    "      {\n",
    "        \"node\": \"stake\",\n",
    "        \"swing_tick\": 120,\n",
    "        \"phase\": \"recovery\"\n",
    "      }\n",
    "    ],\n",
    "    \"enemies\": []\n",
    "  }\n",
    "}\n",
);

#[test]
fn souls_lock_strafe_switch_and_release_replay_to_the_expected_state() {
    let dir = test_dir("souls_lock_strafe_switch_and_release_replay_to_the_expected_state");
    let script = write_file(&dir, "lock.ron", SOULS_LOCK_SCRIPT);
    let lock_after = |frames: &str| {
        let state = dir.join(format!("{frames}.json"));
        run_headless_replay(frames, &script, &state);
        let dump: Value =
            serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
                .expect("state file is valid JSON");
        dump["game"]["lock"].clone()
    };
    // The middle dummy is the one straight ahead; the flick on tick 80
    // switches to the right one; the press on tick 100 lets go.
    assert_eq!(lock_after("1"), "dummy_b");
    assert_eq!(lock_after("79"), "dummy_b");
    assert_eq!(lock_after("80"), "dummy_c");
    assert_eq!(lock_after("99"), "dummy_c");

    let state = dir.join("state.json");
    run_headless_replay("120", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_LOCK_STATE);
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    assert_eq!(dump["game"]["lock"], Value::Null);
}

/// Lock on (tick 1: dummy_b), walk up to it until its block stops the
/// character (ticks 2..=95), then a light attack on tick 100 and two more
/// pressed during the recovery of the one before (ticks 115 and 145): a
/// three-hit combo on ticks 100, 130 and 160.
const SOULS_COMBO_HITS_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"lock\", down: true), (tick: 2, action: \"lock\", down: false), ",
    "(tick: 100, action: \"attack\", down: true), (tick: 101, action: \"attack\", down: false), ",
    "(tick: 115, action: \"attack\", down: true), (tick: 116, action: \"attack\", down: false), ",
    "(tick: 145, action: \"attack\", down: true), (tick: 146, action: \"attack\", down: false)",
    "], axes: [",
    "(tick: 2, axis: \"move_y\", value: 1.0), (tick: 96, axis: \"move_y\", value: 0.0)",
    "])\n",
);

/// State after [`SOULS_COMBO_HITS_SCRIPT`] and 200 ticks of the built-in
/// scene. Determinism only: no independent source for every byte. The hits
/// are checked by hand in the test.
const SOULS_COMBO_HITS_STATE: &str = concat!(
    "{\n",
    "  \"tick\": 200,\n",
    "  \"sim_time\": 3.3333333333333335,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {\n",
    "    \"look_x\": 0.0,\n",
    "    \"look_y\": 0.0,\n",
    "    \"move_x\": 0.0,\n",
    "    \"move_y\": 0.0\n",
    "  },\n",
    "  \"nodes\": {\n",
    "    \"dummy_a\": {\n",
    "      \"position\": [\n",
    "        -5.0,\n",
    "        0.0,\n",
    "        -6.0\n",
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
    "    \"dummy_a/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_a/body\": {\n",
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
    "    \"dummy_a/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_b\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        -6.5\n",
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
    "    \"dummy_b/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_b/body\": {\n",
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
    "    \"dummy_b/hp_bar\": {\n",
    "      \"position\": [\n",
    "        -0.24000001,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.32000002,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_c\": {\n",
    "      \"position\": [\n",
    "        5.0,\n",
    "        0.0,\n",
    "        -6.0\n",
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
    "    \"dummy_c/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_c/body\": {\n",
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
    "    \"dummy_c/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
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
    "        40.0,\n",
    "        1.0,\n",
    "        40.0\n",
    "      ]\n",
    "    },\n",
    "    \"main_cam\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        3.896173,\n",
    "        -2.5471332\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        -0.3156607,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.94887215\n",
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
    "        -5.75\n",
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
    "    \"stake\": {\n",
    "      \"position\": [\n",
    "        6.0,\n",
    "        0.0,\n",
    "        2.0\n",
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
    "    \"stake/arm\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.1,\n",
    "        -1.2\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.1,\n",
    "        0.1,\n",
    "        2.0\n",
    "      ]\n",
    "    },\n",
    "    \"stake/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.8,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.3,\n",
    "        1.6,\n",
    "        0.3\n",
    "      ]\n",
    "    },\n",
    "    \"stake/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"stake/post\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.8,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.3,\n",
    "        1.6,\n",
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
    "        -5.75\n",
    "      ],\n",
    "      \"facing\": 0.0,\n",
    "      \"stamina\": 55.0,\n",
    "      \"stamina_wait\": 19,\n",
    "      \"action\": \"idle\",\n",
    "      \"action_tick\": 0,\n",
    "      \"invulnerable\": false,\n",
    "      \"attack_phase\": null,\n",
    "      \"combo\": 0,\n",
    "      \"buffered\": null,\n",
    "      \"hits\": 0,\n",
    "      \"last_hit\": null,\n",
    "      \"health\": {\n",
    "        \"hp\": 100.0,\n",
    "        \"state\": \"ok\",\n",
    "        \"state_ticks\": 0,\n",
    "        \"impact\": 0.0,\n",
    "        \"impact_wait\": 0\n",
    "      }\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.64230573,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": \"dummy_b\",\n",
    "    \"hurtboxes\": [\n",
    "      {\n",
    "        \"node\": \"dummy_a\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"dummy_b\",\n",
    "        \"hits\": 3,\n",
    "        \"last_hit\": {\n",
    "          \"tick\": 170,\n",
    "          \"attack\": \"attack\",\n",
    "          \"combo\": 3\n",
    "        },\n",
    "        \"health\": {\n",
    "          \"hp\": 40.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 30\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"dummy_c\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"stake\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      }\n",
    "    ],\n",
    "    \"swingers\": [\n",
    "      {\n",
    "        \"node\": \"stake\",\n",
    "        \"swing_tick\": 80,\n",
    "        \"phase\": \"recovery\"\n",
    "      }\n",
    "    ],\n",
    "    \"enemies\": []\n",
    "  }\n",
    "}\n",
);

#[test]
fn souls_light_combo_hits_the_locked_dummy_three_times_and_replays() {
    let dir = test_dir("souls_light_combo_hits_the_locked_dummy_three_times_and_replays");
    let script = write_file(&dir, "combo.ron", SOULS_COMBO_HITS_SCRIPT);
    let state = dir.join("state.json");
    run_headless_replay("200", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_COMBO_HITS_STATE);
    // By hand: dummy_b's block stops the character at z = -6.1 + 0.35 =
    // -5.75, 0.75 m from dummy_b. On an attack's first active tick the
    // -30° edge of the sweep passes 0.75 * sin 30° = 0.375 m from it,
    // within its radius 0.4: hits on ticks 110, 140 and 170.
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    let hurtboxes = &dump["game"]["hurtboxes"];
    assert_eq!(hurtboxes[1]["node"], "dummy_b");
    assert_eq!(hurtboxes[1]["hits"], 3);
    assert_eq!(
        hurtboxes[1]["last_hit"],
        serde_json::json!({"tick": 170, "attack": "attack", "combo": 3})
    );
    for other in [&hurtboxes[0], &hurtboxes[2]] {
        assert_eq!(other["hits"], 0, "{other}");
    }
    // By hand: three light hits of 20 damage leave 40 of 100 hp. Their
    // impacts add up to 10, 20, then 30 > poise 25: a stagger on tick 170
    // for 30 ticks, over by tick 200, which empties the impact; the reset
    // wait set on tick 170 (60 ticks) has 30 left.
    assert_eq!(
        hurtboxes[1]["health"],
        serde_json::json!({
            "hp": 40.0, "state": "ok", "state_ticks": 0, "impact": 0.0, "impact_wait": 30
        })
    );
    // The bar at 40 %: 0.8 * 0.4 wide, moved left by 0.8 * 0.6 / 2 so its
    // left end stays at x = -0.4 (up to rounding).
    let bar = &dump["nodes"]["dummy_b/hp_bar"];
    let width = bar["scale"][0].as_f64().expect("a number") as f32;
    let x = bar["position"][0].as_f64().expect("a number") as f32;
    assert_eq!(width, 0.8 * 0.4);
    assert_eq!(x, -(0.8 * (1.0 - 0.4)) / 2.0);
    assert!(
        (x - width / 2.0 + 0.4).abs() < 1e-6,
        "left end {}",
        x - width / 2.0
    );
}

/// [`SOULS_COMBO_HITS_SCRIPT`] (lock on dummy_b, walk up, three light hits
/// on ticks 110, 140 and 170: 40 hp left), then a heavy attack pressed on
/// tick 190 and a light one on 240. The lock key on tick 300 finds nothing
/// to lock on to, the one on tick 440 locks on again.
const SOULS_KILL_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"lock\", down: true), (tick: 2, action: \"lock\", down: false), ",
    "(tick: 100, action: \"attack\", down: true), (tick: 101, action: \"attack\", down: false), ",
    "(tick: 115, action: \"attack\", down: true), (tick: 116, action: \"attack\", down: false), ",
    "(tick: 145, action: \"attack\", down: true), (tick: 146, action: \"attack\", down: false), ",
    "(tick: 190, action: \"heavy\", down: true), (tick: 191, action: \"heavy\", down: false), ",
    "(tick: 240, action: \"attack\", down: true), (tick: 241, action: \"attack\", down: false), ",
    "(tick: 300, action: \"lock\", down: true), (tick: 301, action: \"lock\", down: false), ",
    "(tick: 440, action: \"lock\", down: true), (tick: 441, action: \"lock\", down: false)",
    "], axes: [",
    "(tick: 2, axis: \"move_y\", value: 1.0), (tick: 96, axis: \"move_y\", value: 0.0)",
    "])\n",
);

/// State after [`SOULS_KILL_SCRIPT`] and 450 ticks of the built-in scene.
/// Determinism only: no independent source for every byte. The health and
/// the lock along the way are checked by hand in the test.
const SOULS_KILL_STATE: &str = concat!(
    "{\n",
    "  \"tick\": 450,\n",
    "  \"sim_time\": 7.5,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {\n",
    "    \"look_x\": 0.0,\n",
    "    \"look_y\": 0.0,\n",
    "    \"move_x\": 0.0,\n",
    "    \"move_y\": 0.0\n",
    "  },\n",
    "  \"nodes\": {\n",
    "    \"dummy_a\": {\n",
    "      \"position\": [\n",
    "        -5.0,\n",
    "        0.0,\n",
    "        -6.0\n",
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
    "    \"dummy_a/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_a/body\": {\n",
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
    "    \"dummy_a/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_b\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        -6.5\n",
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
    "    \"dummy_b/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_b/body\": {\n",
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
    "    \"dummy_b/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_c\": {\n",
    "      \"position\": [\n",
    "        5.0,\n",
    "        0.0,\n",
    "        -6.0\n",
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
    "    \"dummy_c/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_c/body\": {\n",
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
    "    \"dummy_c/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
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
    "        40.0,\n",
    "        1.0,\n",
    "        40.0\n",
    "      ]\n",
    "    },\n",
    "    \"main_cam\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        3.896173,\n",
    "        -2.5471332\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        -0.3156607,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.94887215\n",
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
    "        -5.75\n",
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
    "    \"stake\": {\n",
    "      \"position\": [\n",
    "        6.0,\n",
    "        0.0,\n",
    "        2.0\n",
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
    "    \"stake/arm\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.1,\n",
    "        -1.2\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.1,\n",
    "        0.1,\n",
    "        2.0\n",
    "      ]\n",
    "    },\n",
    "    \"stake/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.8,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.3,\n",
    "        1.6,\n",
    "        0.3\n",
    "      ]\n",
    "    },\n",
    "    \"stake/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"stake/post\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.8,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.3,\n",
    "        1.6,\n",
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
    "        -5.75\n",
    "      ],\n",
    "      \"facing\": 0.0,\n",
    "      \"stamina\": 100.0,\n",
    "      \"stamina_wait\": 0,\n",
    "      \"action\": \"idle\",\n",
    "      \"action_tick\": 0,\n",
    "      \"invulnerable\": false,\n",
    "      \"attack_phase\": null,\n",
    "      \"combo\": 0,\n",
    "      \"buffered\": null,\n",
    "      \"hits\": 0,\n",
    "      \"last_hit\": null,\n",
    "      \"health\": {\n",
    "        \"hp\": 100.0,\n",
    "        \"state\": \"ok\",\n",
    "        \"state_ticks\": 0,\n",
    "        \"impact\": 0.0,\n",
    "        \"impact_wait\": 0\n",
    "      }\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.64230573,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": \"dummy_b\",\n",
    "    \"hurtboxes\": [\n",
    "      {\n",
    "        \"node\": \"dummy_a\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"dummy_b\",\n",
    "        \"hits\": 5,\n",
    "        \"last_hit\": {\n",
    "          \"tick\": 250,\n",
    "          \"attack\": \"attack\",\n",
    "          \"combo\": 1\n",
    "        },\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"dummy_c\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"stake\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      }\n",
    "    ],\n",
    "    \"swingers\": [\n",
    "      {\n",
    "        \"node\": \"stake\",\n",
    "        \"swing_tick\": 90,\n",
    "        \"phase\": \"recovery\"\n",
    "      }\n",
    "    ],\n",
    "    \"enemies\": []\n",
    "  }\n",
    "}\n",
);

#[test]
fn souls_dummy_killed_while_locked_lets_go_then_revives_and_locks_again() {
    let dir = test_dir("souls_dummy_killed_while_locked_lets_go_then_revives_and_locks_again");
    let script = write_file(&dir, "kill.ron", SOULS_KILL_SCRIPT);
    let after = |frames: &str| {
        let state = dir.join(format!("{frames}.json"));
        run_headless_replay(frames, &script, &state);
        let dump: Value =
            serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
                .expect("state file is valid JSON");
        let dummy_b = &dump["game"]["hurtboxes"][1];
        assert_eq!(dummy_b["node"], "dummy_b");
        (
            dump["game"]["lock"].clone(),
            dummy_b["health"].clone(),
            dummy_b["hits"].clone(),
        )
    };
    let health = |hp: f32, state: &str, state_ticks: u32, impact_wait: u32| {
        serde_json::json!({
            "hp": hp, "state": state, "state_ticks": state_ticks,
            "impact": 0.0, "impact_wait": impact_wait
        })
    };
    // By hand, from 40 hp and 30 impact wait left on tick 200 (see
    // `souls_light_combo_hits_the_locked_dummy_three_times_and_replays`):
    // the heavy attack starts on tick 190 and hits on its first active
    // tick, 212 (-35: 5 left; impact 30 > poise 25: staggered for 30
    // ticks, the reset wait set to 60).
    assert_eq!(
        after("212"),
        ("dummy_b".into(), health(5.0, "stagger", 30, 60), 4.into())
    );
    // The light attack of tick 240 hits on tick 250: 5 - 20 is 0, dead
    // for 180 ticks, the impact and its wait emptied, the lock let go.
    assert_eq!(after("249").0, "dummy_b");
    assert_eq!(
        after("250"),
        (Value::Null, health(0.0, "dead", 180, 0), 5.into())
    );
    // Tick 300: dead dummy_b cannot be locked on to, and the other two
    // are more than lock_angle to the sides.
    assert_eq!(
        after("300"),
        (Value::Null, health(0.0, "dead", 130, 0), 5.into())
    );
    // Back at full health on tick 250 + 180 = 430, and locked on again by
    // the press on tick 440.
    assert_eq!(after("429").1, health(0.0, "dead", 1, 0));
    assert_eq!(
        after("430"),
        (Value::Null, health(100.0, "ok", 0, 0), 5.into())
    );

    let state = dir.join("state.json");
    run_headless_replay("450", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_KILL_STATE);
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    assert_eq!(dump["game"]["lock"], "dummy_b");
    assert_eq!(
        dump["game"]["hurtboxes"][1]["health"],
        health(100.0, "ok", 0, 0)
    );
}

/// Walk right to 2 m in front of the stake (ticks 1..=90) and stand through
/// its second round. Before its third round, tap roll (274-275) to roll left
/// through its sweep; walk back right (307..=354) and stand there until it
/// kills the character.
const SOULS_STAKE_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 274, action: \"roll\", down: true), (tick: 275, action: \"roll\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_x\", value: 1.0), (tick: 91, axis: \"move_x\", value: 0.0), ",
    "(tick: 275, axis: \"move_x\", value: -1.0), (tick: 276, axis: \"move_x\", value: 0.0), ",
    "(tick: 307, axis: \"move_x\", value: 1.0), (tick: 355, axis: \"move_x\", value: 0.0)",
    "])\n",
);

/// State after [`SOULS_STAKE_SCRIPT`] and 763 ticks of the built-in scene:
/// the tick the character respawns. Determinism only: no independent source
/// for every byte. The hits are checked by hand in the test.
const SOULS_STAKE_STATE: &str = concat!(
    "{\n",
    "  \"tick\": 763,\n",
    "  \"sim_time\": 12.716666666666667,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {\n",
    "    \"look_x\": 0.0,\n",
    "    \"look_y\": 0.0,\n",
    "    \"move_x\": 0.0,\n",
    "    \"move_y\": 0.0\n",
    "  },\n",
    "  \"nodes\": {\n",
    "    \"dummy_a\": {\n",
    "      \"position\": [\n",
    "        -5.0,\n",
    "        0.0,\n",
    "        -6.0\n",
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
    "    \"dummy_a/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_a/body\": {\n",
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
    "    \"dummy_a/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_b\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        -6.5\n",
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
    "    \"dummy_b/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_b/body\": {\n",
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
    "    \"dummy_b/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_c\": {\n",
    "      \"position\": [\n",
    "        5.0,\n",
    "        0.0,\n",
    "        -6.0\n",
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
    "    \"dummy_c/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        1.8,\n",
    "        0.8\n",
    "      ]\n",
    "    },\n",
    "    \"dummy_c/body\": {\n",
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
    "    \"dummy_c/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        2.1,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
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
    "    \"stake\": {\n",
    "      \"position\": [\n",
    "        6.0,\n",
    "        0.0,\n",
    "        2.0\n",
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
    "    \"stake/arm\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.1,\n",
    "        -1.2\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.1,\n",
    "        0.1,\n",
    "        2.0\n",
    "      ]\n",
    "    },\n",
    "    \"stake/block\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.8,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.3,\n",
    "        1.6,\n",
    "        0.3\n",
    "      ]\n",
    "    },\n",
    "    \"stake/hp_bar\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        1.9,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.8,\n",
    "        0.08,\n",
    "        0.08\n",
    "      ]\n",
    "    },\n",
    "    \"stake/post\": {\n",
    "      \"position\": [\n",
    "        0.0,\n",
    "        0.8,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.3,\n",
    "        1.6,\n",
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
    "      \"facing\": 0.0,\n",
    "      \"stamina\": 100.0,\n",
    "      \"stamina_wait\": 0,\n",
    "      \"action\": \"idle\",\n",
    "      \"action_tick\": 0,\n",
    "      \"invulnerable\": false,\n",
    "      \"attack_phase\": null,\n",
    "      \"combo\": 0,\n",
    "      \"buffered\": null,\n",
    "      \"hits\": 4,\n",
    "      \"last_hit\": {\n",
    "        \"tick\": 643,\n",
    "        \"by\": \"stake\"\n",
    "      },\n",
    "      \"health\": {\n",
    "        \"hp\": 100.0,\n",
    "        \"state\": \"ok\",\n",
    "        \"state_ticks\": 0,\n",
    "        \"impact\": 0.0,\n",
    "        \"impact_wait\": 0\n",
    "      }\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": [\n",
    "      {\n",
    "        \"node\": \"dummy_a\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"dummy_b\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"dummy_c\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      },\n",
    "      {\n",
    "        \"node\": \"stake\",\n",
    "        \"hits\": 0,\n",
    "        \"last_hit\": null,\n",
    "        \"health\": {\n",
    "          \"hp\": 100.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 0.0,\n",
    "          \"impact_wait\": 0\n",
    "        }\n",
    "      }\n",
    "    ],\n",
    "    \"swingers\": [\n",
    "      {\n",
    "        \"node\": \"stake\",\n",
    "        \"swing_tick\": 43,\n",
    "        \"phase\": \"active\"\n",
    "      }\n",
    "    ],\n",
    "    \"enemies\": []\n",
    "  }\n",
    "}\n",
);

#[test]
fn souls_stake_hits_staggers_is_rolled_through_then_kills_and_the_character_respawns() {
    let dir = test_dir(
        "souls_stake_hits_staggers_is_rolled_through_then_kills_and_the_character_respawns",
    );
    let script = write_file(&dir, "stake.ron", SOULS_STAKE_SCRIPT);
    let player = |frames: &str| {
        let state = dir.join(format!("{frames}.json"));
        run_headless_replay(frames, &script, &state);
        let dump: Value =
            serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
                .expect("state file is valid JSON");
        dump["game"]["player"].clone()
    };
    let health = |hp: f32, state: &str, state_ticks: u32, impact_wait: u32| {
        serde_json::json!({
            "hp": hp, "state": state, "state_ticks": state_ticks,
            "impact": 0.0, "impact_wait": impact_wait
        })
    };
    let hit = |tick: u64| serde_json::json!({"tick": tick, "by": "stake"});
    // By hand: the stake stands at (6, 0, 2) facing -Z and sweeps 150° in
    // six 25° slices from the right, active on ticks 41..=46 of each
    // 120-tick round. The character stops at x = 6 (up to rounding), 2 m
    // in front of it, on the edge between the third and fourth slices: the
    // third active tick of the second round, 120 + 43, hits. 30 damage,
    // impact 20 > poise 0: staggered for 30 ticks.
    let p = player("162");
    assert_eq!(
        (p["hits"].clone(), p["last_hit"].clone()),
        (0.into(), Value::Null)
    );
    let p = player("163");
    assert_f32_bits(&p["position"], [5.999997, 0.0, 0.0]);
    assert_eq!(p["hits"], 1);
    assert_eq!(p["last_hit"], hit(163));
    assert_eq!(p["health"], health(70.0, "stagger", 30, 60));
    // Standing again on 193; the impact's reset wait set on 163 has 30 left.
    assert_eq!(player("193")["health"], health(70.0, "ok", 0, 30));
    // The third round (ticks 281..=286): the roll from tick 275 is
    // invulnerable through tick 294. On tick 285 the character is 1.1 m left
    // of the stake's center line, 2 m in front: 29° to the left, inside the
    // fifth slice (25°-50°) that tick sweeps, and within reach. Not hit.
    let p = player("285");
    assert_eq!(
        (p["action"].clone(), p["invulnerable"].clone()),
        ("roll".into(), true.into())
    );
    assert_f32_bits(&p["position"], [4.899998, 0.0, 0.0]);
    assert_eq!(p["hits"], 1);
    assert_eq!(player("300")["hits"], 1);
    // Back in front of it from tick 354: hit on 403 and 523 (40, then 10
    // hp left), killed on 643: dead for 120 ticks, lying where it stood.
    let p = player("643");
    assert_eq!(p["hits"], 4);
    assert_eq!(p["last_hit"], hit(643));
    assert_eq!(p["health"], health(0.0, "dead", 120, 0));
    let p = player("762");
    assert_eq!(p["health"], health(0.0, "dead", 1, 0));
    assert_f32_bits(&p["position"], [5.999997, 0.0, 0.0]);
    // Tick 643 + 120: back at the start at full health.
    let state = dir.join("state.json");
    run_headless_replay("763", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_STAKE_STATE);
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    let p = &dump["game"]["player"];
    assert_f32_bits(&p["position"], [0.0, 0.0, 0.0]);
    assert_eq!(p["facing"], 0.0);
    assert_eq!(p["health"], health(100.0, "ok", 0, 0));
    assert_eq!(p["hits"], 4);
}

// ---------------------------------------------------------------- traces

/// The raw text of a trace line's `events` and `state`. The state is the
/// state dump, which starts with its `tick`; no event holds that.
fn trace_parts(line: &str) -> (&str, &str) {
    let key = r#","events":"#;
    let events = line
        .find(key)
        .map(|i| &line[i + key.len()..])
        .unwrap_or_else(|| panic!("no events in {line}"));
    let split = events
        .find(r#","state":{"tick":"#)
        .unwrap_or_else(|| panic!("no state in {line}"));
    let state = &events[split + r#","state":"#.len()..];
    (
        &events[..split],
        state
            .strip_suffix('}')
            .unwrap_or_else(|| panic!("the line does not end in a brace: {line}")),
    )
}

/// A state dump file's text without its whitespace: the trace's `state`.
fn compact(pretty: &str) -> String {
    pretty.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Every `(tick, event)` of a trace, in order.
fn trace_events(trace: &str) -> Vec<(u64, Value)> {
    trace
        .lines()
        .flat_map(|line| {
            let line: Value = serde_json::from_str(line).expect("each trace line is JSON");
            let tick = line["tick"].as_u64().expect("a tick");
            let events = line["events"].as_array().expect("events").clone();
            events.into_iter().map(move |e| (tick, e))
        })
        .collect()
}

#[test]
fn trace_of_the_kill_replay_has_a_line_a_tick_matching_the_state_dumps() {
    let dir = test_dir("trace_of_the_kill_replay_has_a_line_a_tick_matching_the_state_dumps");
    let script = write_file(&dir, "kill.ron", SOULS_KILL_SCRIPT);
    let (trace, state) = (dir.join("sub/trace.jsonl"), dir.join("state.json"));
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "450",
        "--input",
        path_str(&script),
        "--trace",
        path_str(&trace),
        "--dump-state",
        path_str(&state),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    let text = fs::read_to_string(&trace).expect("the trace was written");
    assert_trace_fits(&text, "souls");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 451);
    assert!(text.ends_with("}\n") && !text.contains('\r'));
    assert!(lines[0].starts_with(r#"{"tick":0,"events":[],"state":{"tick":0,"#));
    for (t, line) in lines.iter().enumerate() {
        assert!(
            line.starts_with(&format!(r#"{{"tick":{t},"events":["#)),
            "line {t}"
        );
    }
    // The last line is the state dump; line 250 is that of a run of 250.
    assert_eq!(
        trace_parts(lines[450]).1,
        compact(&fs::read_to_string(&state).expect("the state dump was written"))
    );
    assert_eq!(compact(SOULS_KILL_STATE), trace_parts(lines[450]).1);
    let mid = dir.join("250.json");
    run_headless_replay("250", &script, &mid);
    assert_eq!(
        trace_parts(lines[250]).1,
        compact(&fs::read_to_string(&mid).expect("the state dump was written"))
    );
    // The hits and the lock as `souls_dummy_killed_while_locked_...`
    // checks them: staggered on 212, killed on 250 and let go, revived on
    // 430, locked on again on 440.
    let events = trace_events(&text);
    let on = |tick: u64| -> Vec<Value> {
        events
            .iter()
            .filter(|(t, _)| *t == tick)
            .map(|(_, e)| e.clone())
            .collect()
    };
    let hit = |attack: &str, combo: u32, damage: f32, hp: f32, result: &str| {
        serde_json::json!({"type": "hit", "by": "player", "target": "dummy_b",
            "attack": attack, "combo": combo, "damage": damage, "hp": hp, "result": result})
    };
    let lock = |target: Value, reason: &str| serde_json::json!({"type": "lock", "target": target, "reason": reason});
    assert_eq!(on(1), [lock("dummy_b".into(), "pressed")]);
    assert_eq!(on(212), [hit("heavy", 0, 35.0, 5.0, "stagger")]);
    assert_eq!(
        on(250),
        [
            hit("attack", 1, 20.0, 0.0, "death"),
            lock(Value::Null, "target_died")
        ]
    );
    assert_eq!(
        on(430),
        [serde_json::json!({"type": "revive", "node": "dummy_b"})]
    );
    assert_eq!(on(440), [lock("dummy_b".into(), "pressed")]);
    assert_eq!(
        r["trace"],
        serde_json::json!({"path": path_str(&trace), "lines": 451, "events": events.len()})
    );
}

#[test]
fn trace_is_the_same_on_every_run_and_leaves_the_state_dump_as_it_was() {
    let dir = test_dir("trace_is_the_same_on_every_run_and_leaves_the_state_dump_as_it_was");
    let script = write_file(&dir, "stake.ron", SOULS_STAKE_SCRIPT);
    let run = |name: &str, traced: bool| {
        let trace = dir.join(format!("{name}.jsonl"));
        let state = dir.join(format!("{name}.json"));
        let mut args = vec![
            "run",
            "--headless",
            "--frames",
            "400",
            "--input",
            path_str(&script),
            "--dump-state",
            path_str(&state),
        ];
        if traced {
            args.extend(["--trace", path_str(&trace)]);
        }
        let out = run_loom(&args);
        let r = report(&out);
        assert_eq!(out.code, 0, "{r}");
        let trace = traced.then(|| fs::read_to_string(&trace).expect("the trace was written"));
        (
            r,
            trace,
            fs::read_to_string(&state).expect("the state dump was written"),
        )
    };
    let (r1, trace1, state1) = run("one", true);
    let (_, trace2, state2) = run("two", true);
    let (r3, trace3, state3) = run("three", false);
    assert_eq!(trace1, trace2);
    assert_eq!(state1, state2);
    assert_eq!(state1, state3);
    assert_eq!(trace3, None);
    assert!(r3["trace"].is_null());
    assert_eq!(r1["trace"]["lines"], 401);
    // The stake hits on 163 (staggering the character) and misses the
    // roll through its third round, as
    // `souls_stake_hits_staggers_is_rolled_through_then_kills_...` checks.
    let trace1 = trace1.expect("traced");
    assert_trace_fits(&trace1, "souls");
    let events = trace_events(&trace1);
    let hits: Vec<(u64, &Value)> = events
        .iter()
        .filter(|(_, e)| e["type"] == "hit")
        .map(|(t, e)| (*t, &e["result"]))
        .collect();
    assert_eq!(hits, [(163, &Value::from("stagger"))]);
    assert!(
        events
            .iter()
            .any(|(t, e)| *t == 285 && e["type"] == "dodged" && e["action"] == "roll"),
        "{events:?}"
    );
}

#[test]
fn rogue_trace_has_a_hit_for_each_attack_message() {
    let dir = test_dir("rogue_trace_has_a_hit_for_each_attack_message");
    let script = write_script(&dir, "rogue.ron", &ROGUE_SCRIPT);
    let trace = dir.join("trace.jsonl");
    let out = run_loom_in(
        &repo_root(),
        &[
            "run",
            "--headless",
            "--frames",
            "28",
            "--scene",
            "assets/scenes/rogue.ron",
            "--input",
            path_str(&script),
            "--trace",
            path_str(&trace),
        ],
    );
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    let text = fs::read_to_string(&trace).expect("the trace was written");
    assert_trace_fits(&text, "rogue");
    let last: Value =
        serde_json::from_str(text.lines().last().expect("a line")).expect("the line is JSON");
    let messages: Vec<&str> = last["state"]["game"]["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|m| m["text"].as_str().expect("a text"))
        .collect();
    let events = trace_events(&text);
    assert!(!events.is_empty());
    let mut expected = Vec::new();
    for (_, e) in &events {
        assert_eq!(e["type"], "hit", "{e}");
        let by = e["by"].as_str().expect("by");
        let target = e["target"].as_str().expect("target");
        let death = e["result"] == "death";
        assert_eq!(death, e["hp"] == 0, "{e}");
        if by == "player" {
            let verb = if death { "kill" } else { "hit" };
            expected.push(format!("You {verb} {target}."));
        } else {
            assert_eq!(target, "player");
            expected.push(format!("{by} bites you."));
            if death {
                expected.push("You die.".to_owned());
            }
        }
    }
    assert_eq!(messages, expected);
}

// ---------------------------------------------------------------- debug draw

#[test]
fn debug_draw_report_is_null_every_category_or_the_ones_given_in_game_order() {
    for (flag, expected) in [
        (None, Value::Null),
        (
            Some("--debug-draw"),
            serde_json::json!(["hurtboxes", "swings", "colliders", "lock"]),
        ),
        (
            Some("--debug-draw=lock,hurtboxes"),
            serde_json::json!(["hurtboxes", "lock"]),
        ),
    ] {
        let mut args = vec!["run", "--headless", "--frames", "1"];
        args.extend(flag);
        let out = run_loom(&args);
        let r = report(&out);
        assert_eq!(out.code, 0, "{r}");
        assert_eq!(r["debug_draw"], expected, "{flag:?}");
    }
}

#[test]
fn unknown_or_repeated_debug_draw_category_is_an_argument_error() {
    for (flag, message) in [
        (
            "--debug-draw=lock,hitboxes",
            r#"unknown debug draw category "hitboxes": souls has "hurtboxes", "swings", "colliders" and "lock""#,
        ),
        (
            "--debug-draw=",
            r#"unknown debug draw category "": souls has "hurtboxes", "swings", "colliders" and "lock""#,
        ),
        (
            "--debug-draw=swings,swings",
            r#"debug draw category "swings" is given twice"#,
        ),
    ] {
        let out = run_loom(&["run", "--headless", "--frames", "1", flag]);
        let r = report(&out);
        assert_eq!(out.code, 2, "{r}");
        assert_eq!(
            r,
            serde_json::json!({"status": "error", "error": message}),
            "{flag}"
        );
    }
}

#[test]
fn rogue_takes_debug_draw_without_a_value_and_rejects_any_category() {
    let run = |flag: &str| {
        run_loom_in(
            &repo_root(),
            &[
                "run",
                "--headless",
                "--frames",
                "1",
                "--scene",
                "assets/scenes/rogue.ron",
                flag,
            ],
        )
    };
    let out = run("--debug-draw");
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["debug_draw"], serde_json::json!([]));
    let out = run("--debug-draw=lock");
    let r = report(&out);
    assert_eq!(out.code, 2, "{r}");
    assert_eq!(
        r["error"],
        r#"unknown debug draw category "lock": rogue has none"#
    );
}

#[test]
fn debug_draw_leaves_the_state_dump_and_the_trace_byte_for_byte_the_same() {
    let dir = test_dir("debug_draw_leaves_the_state_dump_and_the_trace_byte_for_byte_the_same");
    let script = write_file(&dir, "kill.ron", SOULS_KILL_SCRIPT);
    let run = |name: &str, debug: bool| {
        let (state, trace) = (
            dir.join(format!("{name}.json")),
            dir.join(format!("{name}.jsonl")),
        );
        let mut args = vec![
            "run",
            "--headless",
            "--frames",
            "260",
            "--input",
            path_str(&script),
            "--dump-state",
            path_str(&state),
            "--trace",
            path_str(&trace),
        ];
        if debug {
            args.push("--debug-draw");
        }
        let out = run_loom(&args);
        let r = report(&out);
        assert_eq!(out.code, 0, "{r}");
        (
            fs::read_to_string(&state).expect("the state dump was written"),
            fs::read_to_string(&trace).expect("the trace was written"),
        )
    };
    assert_eq!(run("plain", false), run("drawn", true));
}

// ---------------------------------------------------------------- schema

/// `schema/loom.schema.json`, as committed.
fn committed_schema() -> Value {
    let path = repo_root().join("schema/loom.schema.json");
    let text = fs::read_to_string(&path).expect("the schema file is committed");
    serde_json::from_str(&text).expect("the schema file is JSON")
}

/// The validator of one part of the committed schema: `["scene"]`, or
/// `["games", "souls", "state_dump"]` and so on.
fn validator(keys: &[&str]) -> jsonschema::Validator {
    let schema = keys
        .iter()
        .fold(committed_schema(), |part, key| part[key].clone());
    assert!(schema.is_object(), "no schema at {keys:?}");
    jsonschema::validator_for(&schema).expect("a valid JSON Schema")
}

/// Fails with the first error when `instance` does not fit.
fn assert_fits(validator: &jsonschema::Validator, instance: &Value, what: &str) {
    if let Some(e) = validator.iter_errors(instance).next() {
        panic!("{what}: {e} at {}", e.instance_path());
    }
}

/// Every line of a trace written by `game` fits its `trace_line`.
fn assert_trace_fits(trace: &str, game: &str) {
    let validator = validator(&["games", game, "trace_line"]);
    for (i, line) in trace.lines().enumerate() {
        let line: Value = serde_json::from_str(line).expect("each trace line is JSON");
        assert_fits(&validator, &line, &format!("trace line {i}"));
    }
}

#[test]
fn loom_schema_prints_the_committed_schema_the_same_on_every_run() {
    let first = run_loom(&["schema"]);
    let second = run_loom(&["schema"]);
    assert_eq!(first.code, 0);
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.stdout.lines().count(), 1);
    assert!(first.stdout.ends_with("}\n"));
    let r = report(&first);
    assert_eq!(r["status"], "ok");
    assert_eq!(r.as_object().expect("an object").len(), 2);
    assert_eq!(r["schema"], committed_schema());
}

#[test]
fn every_repository_scene_fits_the_scene_schema_and_broken_ones_do_not() {
    let root = repo_root();
    let mut files: Vec<PathBuf> = ["demo", "g2_3_demo", "g3_3_demo"]
        .iter()
        .map(|name| root.join(format!("games/souls/src/{name}.scene.ron")))
        .collect();
    let mut assets: Vec<PathBuf> = fs::read_dir(root.join("assets/scenes"))
        .expect("assets/scenes exists")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "ron"))
        .collect();
    assets.sort();
    assert_eq!(assets.len(), 3, "{assets:?}");
    files.extend(assets);
    let scene = validator(&["scene"]);
    let as_json = |file: &PathBuf| -> Value {
        let text = fs::read_to_string(file).expect("the scene can be read");
        let doc = loom_scene::SceneDoc::parse(&text).expect("the scene parses");
        serde_json::to_value(&doc).expect("a scene is plain data")
    };
    for file in &files {
        assert_fits(&scene, &as_json(file), &file.display().to_string());
    }

    let demo = as_json(&files[0]);
    assert!(scene.is_valid(&demo));
    let mut misspelled = demo.clone();
    misspelled["nodes"][1]["positon"] = serde_json::json!([0.0, 0.0, 0.0]);
    assert!(!scene.is_valid(&misspelled));
    let mut unknown = demo.clone();
    unknown["gravity"] = serde_json::json!(9.8);
    assert!(!scene.is_valid(&unknown));
    let mut flat = demo.clone();
    assert_eq!(flat["nodes"][1]["name"], "player");
    flat["nodes"][1]["position"] = serde_json::json!([0.0, 0.0]);
    assert!(!scene.is_valid(&flat));
    let mut mesh = demo;
    mesh["nodes"][0]["mesh"] = serde_json::json!("Cone");
    assert!(!scene.is_valid(&mesh));
}

#[test]
fn every_expected_state_dump_fits_its_games_state_dump_schema() {
    let dumps = [
        ("souls", "SCENARIO_A", SCENARIO_A),
        ("souls", "SCENARIO_B", SCENARIO_B),
        ("souls", "SOULS_MOVES_STATE", SOULS_MOVES_STATE),
        ("souls", "SOULS_ATTACKS_STATE", SOULS_ATTACKS_STATE),
        ("souls", "SOULS_LOCK_STATE", SOULS_LOCK_STATE),
        ("souls", "SOULS_COMBO_HITS_STATE", SOULS_COMBO_HITS_STATE),
        ("souls", "SOULS_KILL_STATE", SOULS_KILL_STATE),
        ("souls", "SOULS_STAKE_STATE", SOULS_STAKE_STATE),
        ("rogue", "ROGUE_STATE", ROGUE_STATE),
    ];
    let souls = validator(&["games", "souls", "state_dump"]);
    let rogue = validator(&["games", "rogue", "state_dump"]);
    for (game, name, text) in dumps {
        let dump: Value = serde_json::from_str(text).expect("the expected dump is JSON");
        let (own, other) = match game {
            "souls" => (&souls, &rogue),
            _ => (&rogue, &souls),
        };
        assert_fits(own, &dump, name);
        assert!(!other.is_valid(&dump), "{name} fits the other game too");
    }

    // The schema asks for the fields: one taken out no longer fits.
    let mut dump: Value = serde_json::from_str(SOULS_MOVES_STATE).expect("JSON");
    assert!(souls.is_valid(&dump));
    dump["game"]["player"]
        .as_object_mut()
        .expect("the player is an object")
        .remove("stamina");
    assert!(!souls.is_valid(&dump));
}

// ---------------------------------------------------------------- the boss

/// The boss fight: walk up (ticks 1-39), take the boss's cleave, roll back
/// through its combo's first cut (333, backwards 331-336), lock on (380),
/// walk in (382-427) and attack twice in its rest, the second buffered.
const BOSS_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 333, action: \"roll\", down: true), (tick: 334, action: \"roll\", down: false), ",
    "(tick: 380, action: \"lock\", down: true), (tick: 381, action: \"lock\", down: false), ",
    "(tick: 430, action: \"attack\", down: true), (tick: 431, action: \"attack\", down: false), ",
    "(tick: 448, action: \"attack\", down: true), (tick: 449, action: \"attack\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_y\", value: 1.0), (tick: 40, axis: \"move_y\", value: 0.0), ",
    "(tick: 331, axis: \"move_y\", value: -1.0), (tick: 337, axis: \"move_y\", value: 0.0), ",
    "(tick: 382, axis: \"move_y\", value: 1.0), (tick: 428, axis: \"move_y\", value: 0.0)",
    "])\n",
);

/// The state on tick 520 of [`BOSS_SCRIPT`], the character killed by the combo.
const BOSS_STATE: &str = concat!(
    "{\n",
    "  \"tick\": 520,\n",
    "  \"sim_time\": 8.666666666666666,\n",
    "  \"actions\": [],\n",
    "  \"axes\": {\n",
    "    \"look_x\": 0.0,\n",
    "    \"look_y\": 0.0,\n",
    "    \"move_x\": 0.0,\n",
    "    \"move_y\": 0.0\n",
    "  },\n",
    "  \"nodes\": {\n",
    "    \"boss\": {\n",
    "      \"position\": [\n",
    "        -4.3711393e-7,\n",
    "        0.0,\n",
    "        3.5000033\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        -1.0,\n",
    "        0.0,\n",
    "        7.54979e-8\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.0,\n",
    "        1.0,\n",
    "        1.0\n",
    "      ]\n",
    "    },\n",
    "    \"boss/arm\": {\n",
    "      \"position\": [\n",
    "        -0.45965165,\n",
    "        1.8,\n",
    "        -1.8435618\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.12186936,\n",
    "        0.0,\n",
    "        0.99254614\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        0.15,\n",
    "        0.15,\n",
    "        2.8\n",
    "      ]\n",
    "    },\n",
    "    \"boss/body\": {\n",
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
    "    \"boss/hp_bar\": {\n",
    "      \"position\": [\n",
    "        -0.05333333,\n",
    "        3.4,\n",
    "        0.0\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.0,\n",
    "        0.0,\n",
    "        0.0,\n",
    "        1.0\n",
    "      ],\n",
    "      \"scale\": [\n",
    "        1.4933333,\n",
    "        0.1,\n",
    "        0.1\n",
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
    "        40.0,\n",
    "        1.0,\n",
    "        40.0\n",
    "      ]\n",
    "    },\n",
    "    \"main_cam\": {\n",
    "      \"position\": [\n",
    "        2.7286234e-8,\n",
    "        1.3589728,\n",
    "        9.130854\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.01763114,\n",
    "        4.1230713e-8,\n",
    "        -7.2705747e-10,\n",
    "        0.99984455\n",
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
    "        -7.0,\n",
    "        1.5,\n",
    "        -8.0\n",
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
    "        7.0,\n",
    "        1.5,\n",
    "        -8.0\n",
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
    "        -7.0,\n",
    "        1.5,\n",
    "        4.0\n",
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
    "    \"pillars/d\": {\n",
    "      \"position\": [\n",
    "        7.0,\n",
    "        1.5,\n",
    "        4.0\n",
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
    "        -3.0240565e-7,\n",
    "        0.0,\n",
    "        5.133341\n",
    "      ],\n",
    "      \"rotation\": [\n",
    "        0.70710677,\n",
    "        2.9159054e-8,\n",
    "        -7.76111e-8,\n",
    "        0.70710677\n",
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
    "    }\n",
    "  },\n",
    "  \"game\": {\n",
    "    \"player\": {\n",
    "      \"position\": [\n",
    "        -3.0240565e-7,\n",
    "        0.0,\n",
    "        5.133341\n",
    "      ],\n",
    "      \"facing\": 8.247426e-8,\n",
    "      \"stamina\": 73.0,\n",
    "      \"stamina_wait\": 0,\n",
    "      \"action\": \"idle\",\n",
    "      \"action_tick\": 0,\n",
    "      \"invulnerable\": false,\n",
    "      \"attack_phase\": null,\n",
    "      \"combo\": 0,\n",
    "      \"buffered\": null,\n",
    "      \"hits\": 4,\n",
    "      \"last_hit\": {\n",
    "        \"tick\": 520,\n",
    "        \"by\": \"boss\"\n",
    "      },\n",
    "      \"health\": {\n",
    "        \"hp\": 0.0,\n",
    "        \"state\": \"dead\",\n",
    "        \"state_ticks\": 120,\n",
    "        \"impact\": 0.0,\n",
    "        \"impact_wait\": 0\n",
    "      }\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 8.2474244e-8,\n",
    "      \"pitch\": 0.035264105,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": [\n",
    "      {\n",
    "        \"node\": \"boss\",\n",
    "        \"hits\": 2,\n",
    "        \"last_hit\": {\n",
    "          \"tick\": 471,\n",
    "          \"attack\": \"attack\",\n",
    "          \"combo\": 2\n",
    "        },\n",
    "        \"health\": {\n",
    "          \"hp\": 560.0,\n",
    "          \"state\": \"ok\",\n",
    "          \"state_ticks\": 0,\n",
    "          \"impact\": 20.0,\n",
    "          \"impact_wait\": 11\n",
    "        }\n",
    "      }\n",
    "    ],\n",
    "    \"swingers\": [],\n",
    "    \"enemies\": [\n",
    "      {\n",
    "        \"node\": \"boss\",\n",
    "        \"state\": \"attack\",\n",
    "        \"move\": \"combo\",\n",
    "        \"move_tick\": 62,\n",
    "        \"phase\": \"active\",\n",
    "        \"hit\": 3,\n",
    "        \"facing\": -3.1415925,\n",
    "        \"rest\": 0,\n",
    "        \"chase\": 0,\n",
    "        \"rng\": 13064056694810536069\n",
    "      }\n",
    "    ]\n",
    "  }\n",
    "}\n",
);

/// SplitMix64's step: the boss's `rng` after `n` numbers from seed 7.
fn boss_rng(n: u64) -> u64 {
    7_u64.wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

/// The state of the boss scene on tick `frames` of `script`, run in `dir`.
fn boss_at(dir: &std::path::Path, script: &std::path::Path, frames: &str) -> Value {
    let state = dir.join(format!("{frames}.json"));
    let scene = repo_root().join("assets/scenes/boss.ron");
    run_headless_replay_on(Some(&scene), frames, script, &state);
    serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
        .expect("state file is valid JSON")
}

/// The boss's `(state, move, move_tick, phase, chase, rng)`.
fn boss_of(dump: &Value) -> (Value, Value, Value, Value, Value, u64) {
    let e = &dump["game"]["enemies"][0];
    assert_eq!(e["node"], "boss");
    (
        e["state"].clone(),
        e["move"].clone(),
        e["move_tick"].clone(),
        e["phase"].clone(),
        e["chase"].clone(),
        e["rng"].as_u64().expect("rng is a whole number"),
    )
}

/// `(hp, state)` of a health in a state dump.
fn hp_of(health: &Value) -> (Value, Value) {
    (health["hp"].clone(), health["state"].clone())
}

#[test]
fn boss_fight_replays_its_moves_hits_and_dodges_the_same_every_time() {
    let dir = test_dir("boss_fight_replays_its_moves_hits_and_dodges_the_same_every_time");
    let script = write_file(&dir, "boss.ron", BOSS_SCRIPT);
    let scene = repo_root().join("assets/scenes/boss.ron");
    let at = |frames: &str| boss_at(&dir, &script, frames);
    let player = |dump: &Value| hp_of(&dump["game"]["player"]["health"]);
    // 11.9 m apart on tick 1, beyond every move: only the approach, of
    // weight floor(2 * 9.9) = 19, chosen with the first number; 30 ticks
    // at most, one walked.
    let s = at("1");
    assert_eq!(
        boss_of(&s),
        (
            "chase".into(),
            Value::Null,
            0.into(),
            Value::Null,
            29.into(),
            boss_rng(1)
        )
    );
    // Its second approach (tick 31) ends on tick 60; on 61, 6.9 m apart,
    // the lunge's 2 against the approach's 9: the third number chose the
    // lunge.
    let s = at("61");
    assert_eq!(
        boss_of(&s),
        (
            "attack".into(),
            "lunge".into(),
            1.into(),
            "startup".into(),
            0.into(),
            boss_rng(3)
        )
    );
    // The cleave chosen on tick 181 hits on its first active tick, 181 +
    // 45: 100 - 45 hp, staggered (poise 0).
    let s = at("226");
    assert_eq!(
        boss_of(&s),
        (
            "attack".into(),
            "cleave".into(),
            46.into(),
            "active".into(),
            0.into(),
            boss_rng(4)
        )
    );
    assert_eq!(player(&s), (55.0.into(), "stagger".into()));
    // The combo chosen on tick 310: its first cut (336-340) is rolled
    // through, the others out of reach.
    let s = at("337");
    assert_eq!(
        boss_of(&s),
        (
            "attack".into(),
            "combo".into(),
            28.into(),
            "active".into(),
            0.into(),
            boss_rng(5)
        )
    );
    assert_eq!(player(&s), (55.0.into(), "ok".into()));
    // Resting after the combo, hit by the first attack: 600 - 20.
    let s = at("441");
    assert_eq!(s["game"]["enemies"][0]["state"], "rest");
    assert_eq!(
        hp_of(&s["game"]["hurtboxes"][0]["health"]),
        (580.0.into(), "ok".into())
    );

    // The whole state on the last tick, and the trace of two runs.
    let trace = |name: &str| {
        let (trace, state) = (
            dir.join(format!("{name}.jsonl")),
            dir.join(format!("{name}.json")),
        );
        let out = run_loom(&[
            "run",
            "--headless",
            "--frames",
            "520",
            "--scene",
            path_str(&scene),
            "--input",
            path_str(&script),
            "--trace",
            path_str(&trace),
            "--dump-state",
            path_str(&state),
        ]);
        let r = report(&out);
        assert_eq!(out.code, 0, "{r}");
        assert_eq!(
            fs::read_to_string(&state).expect("the state dump was written"),
            BOSS_STATE
        );
        fs::read_to_string(&trace).expect("the trace was written")
    };
    let first = trace("one");
    assert_eq!(first, trace("two"));
    assert_trace_fits(&first, "souls");
    let events = trace_events(&first);
    let moves: Vec<(u64, String, Value)> = events
        .iter()
        .filter(|(_, e)| e["type"] == "enemy_move")
        .map(|(t, e)| {
            let m = e["move"].as_str().expect("a move").to_owned();
            (*t, m, e["candidates"].clone())
        })
        .collect();
    let close = serde_json::json!(["sweep", "cleave", "combo"]);
    let far = serde_json::json!(["lunge", "approach"]);
    let want: Vec<(u64, String, Value)> = [
        (1, "approach", serde_json::json!(["approach"])),
        (31, "approach", far.clone()),
        (61, "lunge", far),
        (181, "cleave", close.clone()),
        (310, "combo", close.clone()),
        (459, "combo", close),
    ]
    .into_iter()
    .map(|(t, m, c)| (t, m.to_owned(), c))
    .collect();
    assert_eq!(moves, want);
    // Hits both ways and the dodge, by tick.
    let hits: Vec<(u64, String)> = events
        .iter()
        .filter(|(_, e)| e["type"] == "hit" || e["type"] == "dodged")
        .map(|(t, e)| {
            let kind = e["type"].as_str().expect("a type");
            (*t, format!("{kind} {} {}", e["by"], e["attack"]))
        })
        .collect();
    let want: Vec<(u64, String)> = [
        (226, r#"hit "boss" "cleave""#),
        (337, r#"dodged "boss" null"#),
        (441, r#"hit "player" "attack""#),
        (471, r#"hit "player" "attack""#),
        (486, r#"hit "boss" "combo""#),
        (503, r#"hit "boss" "combo""#),
        (520, r#"hit "boss" "combo""#),
    ]
    .iter()
    .map(|&(t, s)| (t, s.to_owned()))
    .collect();
    assert_eq!(hits, want);
}

/// Two heavy attacks stagger the boss (poise 50): walk up (1-39), backstep
/// through its cleave (223), lock on (226), then a heavy as soon as the
/// backstep ends (246) and another one buffered (281).
const BOSS_STAGGER_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 223, action: \"roll\", down: true), (tick: 224, action: \"roll\", down: false), ",
    "(tick: 226, action: \"lock\", down: true), (tick: 227, action: \"lock\", down: false), ",
    "(tick: 246, action: \"heavy\", down: true), (tick: 247, action: \"heavy\", down: false), ",
    "(tick: 281, action: \"heavy\", down: true), (tick: 282, action: \"heavy\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_y\", value: 1.0), (tick: 40, axis: \"move_y\", value: 0.0)",
    "])\n",
);

#[test]
fn two_heavy_attacks_around_the_boss_rest_stagger_it() {
    let dir = test_dir("two_heavy_attacks_around_the_boss_rest_stagger_it");
    let script = write_file(&dir, "stagger.ron", BOSS_STAGGER_SCRIPT);
    let at = |frames: &str| boss_at(&dir, &script, frames);
    let boss = |dump: &Value| hp_of(&dump["game"]["hurtboxes"][0]["health"]);
    // The first heavy lands on the first tick of its rest: 30 impact.
    let s = at("270");
    assert_eq!(s["game"]["enemies"][0]["state"], "rest");
    assert_eq!(boss(&s), (565.0.into(), "ok".into()));
    assert_eq!(s["game"]["hurtboxes"][0]["health"]["impact"], 30.0);
    // A heavy takes 50 ticks and the rest 40: the second lands on tick
    // 319, 9 ticks into the sweep chosen on 310, past 50 impact.
    let s = at("318");
    assert_eq!(
        boss_of(&s),
        (
            "attack".into(),
            "sweep".into(),
            9.into(),
            "startup".into(),
            0.into(),
            boss_rng(5)
        )
    );
    let s = at("319");
    assert_eq!(boss(&s), (530.0.into(), "stagger".into()));
    assert_eq!(
        boss_of(&s),
        (
            "stagger".into(),
            Value::Null,
            0.into(),
            Value::Null,
            0.into(),
            boss_rng(5)
        )
    );
}
