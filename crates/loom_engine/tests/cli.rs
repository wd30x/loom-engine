//! CLI contract tests: run the real `loom` binary and check stdout, exit
//! code and output files.
//!
//! Only GPU-free invocations belong here (headless, no `--screenshot`), so
//! that `verify.ps1` keeps working on machines without a GPU.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

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
    assert_eq!(
        fs::read_to_string(&file).expect("state file was written"),
        "{\n  \"tick\": 60,\n  \"sim_time\": 1.0,\n  \"demo\": {\n    \"yaw\": 1.0\n  }\n}\n"
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
