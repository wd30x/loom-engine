//! The basics of `loom run`: argument errors, the report, the state dump
//! and its expected bytes, `--size`.

use std::fs;

use serde_json::Value;

use super::common::{g2_3_demo, path_str, report, run_loom, test_dir};

/// State after 60 ticks of the G2.3 demo scene without input (scenario A).
pub(super) const SCENARIO_A: &str = include_str!("expected/scenario_a.json");

/// State after 90 ticks of the G2.3 demo scene, `move_y` at 1.0 for ticks
/// 1..=60 (scenario S1).
pub(super) const SCENARIO_B: &str = include_str!("expected/scenario_b.json");

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
