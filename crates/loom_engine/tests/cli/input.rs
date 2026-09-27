//! Input: replaying scripts, recording, axes, and input script errors.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::common::{
    assert_f32_bits, g2_3_demo, path_str, report, run_headless_replay_on, run_loom, test_dir,
    write_script,
};
use super::run::SCENARIO_B;

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
