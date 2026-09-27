//! Helpers most of the modules use: running `loom`, reading its report,
//! test directories and scripts, comparing floats bit by bit.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

pub(super) struct Output {
    pub(super) code: i32,
    pub(super) stdout: String,
}

pub(super) fn run_loom(args: &[&str]) -> Output {
    run_loom_in(&repo_root(), args)
}

/// The repository root: scenes under `assets/` name their sheets relative
/// to it.
pub(super) fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// [`run_loom`] with `dir` as the working directory.
pub(super) fn run_loom_in(dir: &std::path::Path, args: &[&str]) -> Output {
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
pub(super) fn report(out: &Output) -> Value {
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
pub(super) fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("old test directory can be removed");
    }
    fs::create_dir_all(&dir).expect("test directory can be created");
    dir
}

pub(super) fn path_str(path: &std::path::Path) -> &str {
    path.to_str().expect("test paths are UTF-8")
}

/// Compares a JSON `[x, y, z]` with f32 values bit by bit (serde_json prints
/// f32 in its shortest round-trip form, so parsing back as f32 is exact).
pub(super) fn assert_f32_bits(value: &Value, expected: [f32; 3]) {
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

/// Writes an input script with the given events (RON event tuples) and
/// returns its path.
pub(super) fn write_script(dir: &std::path::Path, name: &str, events: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let body = format!(
        "(version: 1, tick_hz: 60, events: [{}])\n",
        events.join(", ")
    );
    fs::write(&path, body).expect("script file can be written");
    path
}

pub(super) fn run_headless_replay(
    frames: &str,
    script: &std::path::Path,
    state: &std::path::Path,
) -> Value {
    run_headless_replay_on(None, frames, script, state)
}

/// [`run_headless_replay`] on `scene`, or the built-in scene without one.
pub(super) fn run_headless_replay_on(
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
pub(super) fn g2_3_demo() -> PathBuf {
    repo_root().join("games/souls/src/g2_3_demo.scene.ron")
}

pub(super) fn write_file(dir: &std::path::Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("file can be written");
    path
}

/// Every `(tick, event)` of a trace, in order.
pub(super) fn trace_events(trace: &str) -> Vec<(u64, Value)> {
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

/// `schema/loom.schema.json`, as committed.
pub(super) fn committed_schema() -> Value {
    let path = repo_root().join("schema/loom.schema.json");
    let text = fs::read_to_string(&path).expect("the schema file is committed");
    serde_json::from_str(&text).expect("the schema file is JSON")
}

/// The validator of one part of the committed schema: `["scene"]`, or
/// `["games", "souls", "state_dump"]` and so on.
pub(super) fn validator(keys: &[&str]) -> jsonschema::Validator {
    let schema = keys
        .iter()
        .fold(committed_schema(), |part, key| part[key].clone());
    assert!(schema.is_object(), "no schema at {keys:?}");
    jsonschema::validator_for(&schema).expect("a valid JSON Schema")
}

/// Fails with the first error when `instance` does not fit.
pub(super) fn assert_fits(validator: &jsonschema::Validator, instance: &Value, what: &str) {
    if let Some(e) = validator.iter_errors(instance).next() {
        panic!("{what}: {e} at {}", e.instance_path());
    }
}

/// Every line of a trace written by `game` fits its `trace_line`.
pub(super) fn assert_trace_fits(trace: &str, game: &str) {
    let validator = validator(&["games", game, "trace_line"]);
    for (i, line) in trace.lines().enumerate() {
        let line: Value = serde_json::from_str(line).expect("each trace line is JSON");
        assert_fits(&validator, &line, &format!("trace line {i}"));
    }
}
