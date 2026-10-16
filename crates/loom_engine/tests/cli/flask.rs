//! The flask: healing, interrupted drinks, refilling at a bonfire.

use std::fs;

use serde_json::Value;

use super::common::{
    assert_trace_fits, path_str, report, run_headless_replay, run_loom, test_dir, trace_events,
    write_file,
};

/// The built-in demo scene: hit twice by the stake, back off and drink, go
/// back and drink again only to be interrupted, then sit at the bonfire.
pub(super) const FLASK_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 370, action: \"drink\", down: true), (tick: 371, action: \"drink\", down: false), ",
    "(tick: 500, action: \"drink\", down: true), (tick: 501, action: \"drink\", down: false), ",
    "(tick: 790, action: \"interact\", down: true), (tick: 791, action: \"interact\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_x\", value: 1.0), (tick: 91, axis: \"move_x\", value: 0.0), ",
    "(tick: 320, axis: \"move_x\", value: -1.0), (tick: 360, axis: \"move_x\", value: 0.0), ",
    "(tick: 440, axis: \"move_x\", value: 1.0), (tick: 480, axis: \"move_x\", value: 0.0), ",
    "(tick: 560, axis: \"move_x\", value: -1.0), (tick: 650, axis: \"move_y\", value: -1.0), ",
    "(tick: 750, axis: \"move_x\", value: 0.0), (tick: 780, axis: \"move_y\", value: 0.0)",
    "])\n",
);

/// The state on tick 800 of [`FLASK_SCRIPT`], sitting at the bonfire with a
/// full flask. Determinism only; the hp and flask are checked by hand in the
/// test.
const FLASK_STATE: &str = include_str!("expected/flask_state.json");

#[test]
fn demo_flask_heals_is_interrupted_and_refills_at_the_bonfire_the_same_every_time() {
    let dir =
        test_dir("demo_flask_heals_is_interrupted_and_refills_at_the_bonfire_the_same_every_time");
    let script = write_file(&dir, "flask.ron", FLASK_SCRIPT);
    let at = |frames: &str| -> Value {
        let state = dir.join(format!("{frames}.json"));
        run_headless_replay(frames, &script, &state);
        serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
            .expect("state file is valid JSON")
    };
    let player = |s: &Value| {
        let p = &s["game"]["player"];
        (
            p["health"]["hp"].as_f64().expect("hp"),
            p["flask"].as_u64().expect("flask"),
            p["action"].as_str().expect("action").to_owned(),
        )
    };
    // By hand: the stake (30 damage, active on ticks 41..=46 of each
    // 120-tick round from load) hits the character standing 2 m in front
    // of it on 163 and 283: 100 - 30 - 30 = 40. Backed off out of reach, it
    // drinks from 370; tick 30 of the drink, 399, gives back 40 % of 100.
    assert_eq!(player(&at("360")), (40.0, 3, "idle".to_owned()));
    assert_eq!(player(&at("398")), (40.0, 2, "drink".to_owned()));
    assert_eq!(player(&at("399")), (80.0, 2, "drink".to_owned()));
    // Back in front of the stake, it drinks from 500; the hit on 523, tick
    // 24 of the drink, interrupts it before the heal: the flask is used,
    // 80 - 30 = 50.
    assert_eq!(player(&at("522")), (80.0, 1, "drink".to_owned()));
    assert_eq!(player(&at("523")), (50.0, 1, "idle".to_owned()));
    // Sitting at the bonfire on 790 fills the flask and the health.
    assert_eq!(player(&at("789")), (50.0, 1, "idle".to_owned()));

    let trace = |name: &str| {
        let (trace, state) = (
            dir.join(format!("{name}.jsonl")),
            dir.join(format!("{name}.json")),
        );
        let out = run_loom(&[
            "run",
            "--headless",
            "--frames",
            "800",
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
            FLASK_STATE
        );
        fs::read_to_string(&trace).expect("the trace was written")
    };
    let first = trace("one");
    assert_eq!(first, trace("two"));
    assert_trace_fits(&first, "souls");
    let events: Vec<(u64, String)> = trace_events(&first)
        .iter()
        .filter_map(|(t, e)| {
            let what = match e["type"].as_str().unwrap_or("") {
                "hit" => format!("hit {}", e["hp"]),
                "heal" => format!("heal {} {}", e["amount"], e["hp"]),
                "action_start" if e["action"] == "drink" => "drink".to_owned(),
                "action_end" if e["action"] == "drink" => format!("end {}", e["reason"]),
                "rest" => "rest".to_owned(),
                "input_dropped" => format!("dropped {} {}", e["input"], e["reason"]),
                _ => return None,
            };
            Some((*t, what))
        })
        .collect();
    let want: Vec<(u64, String)> = [
        (163, "hit 70.0"),
        (283, "hit 40.0"),
        (370, "drink"),
        (399, "heal 40.0 80.0"),
        (429, r#"end "finished""#),
        (500, "drink"),
        (523, "hit 50.0"),
        (523, r#"end "interrupted""#),
        (790, "rest"),
    ]
    .iter()
    .map(|&(t, s)| (t, s.to_owned()))
    .collect();
    assert_eq!(events, want);
    let s: Value = serde_json::from_str(FLASK_STATE).expect("valid JSON");
    assert_eq!(player(&s), (100.0, 3, "sit".to_owned()));
}
