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
    "      \"buffered\": null\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": []\n",
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
    "      \"buffered\": null\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": []\n",
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
    "      \"buffered\": null\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": []\n",
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
    "      \"buffered\": null\n",
    "    },\n",
    "    \"camera\": {\n",
    "      \"yaw\": 0.0,\n",
    "      \"pitch\": -0.2617994,\n",
    "      \"distance\": 4.0\n",
    "    },\n",
    "    \"lock\": null,\n",
    "    \"hurtboxes\": []\n",
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
    "      \"buffered\": null\n",
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
    "      }\n",
    "    ]\n",
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
    "      \"buffered\": null\n",
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
    "      }\n",
    "    ]\n",
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
    "      \"buffered\": null\n",
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
    "      }\n",
    "    ]\n",
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
