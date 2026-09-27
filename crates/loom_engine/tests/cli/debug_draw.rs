//! The debug draw: its report field and categories, and that it changes
//! no state.

use std::fs;

use serde_json::Value;

use super::common::{path_str, repo_root, report, run_loom, run_loom_in, test_dir, write_file};
use super::souls_moves::SOULS_KILL_SCRIPT;

#[test]
fn debug_draw_report_is_null_every_category_or_the_ones_given_in_game_order() {
    for (flag, expected) in [
        (None, Value::Null),
        (
            Some("--debug-draw"),
            serde_json::json!([
                "hurtboxes",
                "swings",
                "colliders",
                "lock",
                "bonfires",
                "bloodstain"
            ]),
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
            r#"unknown debug draw category "hitboxes": souls has "hurtboxes", "swings", "colliders", "lock", "bonfires" and "bloodstain""#,
        ),
        (
            "--debug-draw=",
            r#"unknown debug draw category "": souls has "hurtboxes", "swings", "colliders", "lock", "bonfires" and "bloodstain""#,
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
