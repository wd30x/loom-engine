//! Souls: won, dropped on death and picked back up.

use std::fs;

use serde_json::Value;

use super::common::{
    assert_trace_fits, path_str, report, run_headless_replay, run_loom, test_dir, trace_events,
    write_file,
};

/// The built-in demo scene: kill dummy_b, die to the stake with its souls,
/// respawn, walk back and pick them up.
const SOULS_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 100, action: \"attack\", down: true), (tick: 101, action: \"attack\", down: false), ",
    "(tick: 115, action: \"attack\", down: true), (tick: 116, action: \"attack\", down: false), ",
    "(tick: 145, action: \"attack\", down: true), (tick: 146, action: \"attack\", down: false), ",
    "(tick: 190, action: \"heavy\", down: true), (tick: 191, action: \"heavy\", down: false), ",
    "(tick: 240, action: \"attack\", down: true), (tick: 241, action: \"attack\", down: false), ",
    "(tick: 975, action: \"interact\", down: true), (tick: 976, action: \"interact\", down: false)",
    "], axes: [",
    "(tick: 2, axis: \"move_y\", value: 1.0), (tick: 96, axis: \"move_y\", value: 0.0), ",
    "(tick: 260, axis: \"move_x\", value: 1.0), (tick: 260, axis: \"move_y\", value: -1.0), ",
    "(tick: 393, axis: \"move_x\", value: 0.0), (tick: 393, axis: \"move_y\", value: 0.0), ",
    "(tick: 890, axis: \"move_x\", value: 1.0), (tick: 970, axis: \"move_x\", value: 0.0)",
    "])\n",
);

/// The state on tick 1000 of [`SOULS_SCRIPT`], the souls picked back up.
/// Determinism only; the souls and the bloodstain are checked by hand in
/// the test.
const SOULS_STATE: &str = include_str!("expected/souls_state.json");

#[test]
fn demo_souls_are_won_dropped_on_death_and_picked_back_up_the_same_every_time() {
    let dir =
        test_dir("demo_souls_are_won_dropped_on_death_and_picked_back_up_the_same_every_time");
    let script = write_file(&dir, "souls.ron", SOULS_SCRIPT);
    let at = |frames: &str| -> Value {
        let state = dir.join(format!("{frames}.json"));
        run_headless_replay(frames, &script, &state);
        serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
            .expect("state file is valid JSON")
    };
    let souls = |s: &Value| {
        (
            s["game"]["player"]["souls"].as_u64().expect("souls"),
            s["game"]["bloodstain"].clone(),
        )
    };
    // By hand: the light attack on 250 kills dummy_b (as in the kill
    // replay, without the lock), which gives its 50 souls.
    assert_eq!(souls(&at("249")), (0, Value::Null));
    assert_eq!(souls(&at("250")), (50, Value::Null));
    // In front of the stake, its fourth hit, on 764, kills the character:
    // the 50 souls drop where it stands.
    let s = at("764");
    let died = s["game"]["player"]["position"].clone();
    assert_eq!(
        souls(&s),
        (0, serde_json::json!({"position": died, "souls": 50}))
    );
    // The respawn at the spawn point on 884 leaves the bloodstain.
    let s = at("884");
    assert_eq!(
        s["game"]["player"]["position"],
        serde_json::json!([0.0, 0.0, 0.0])
    );
    assert_eq!(
        souls(&s),
        (0, serde_json::json!({"position": died, "souls": 50}))
    );
    // Back within 1.5 m of it, the interact key on 975 picks them up.
    assert_eq!(
        souls(&at("974")),
        (0, serde_json::json!({"position": died, "souls": 50}))
    );
    assert_eq!(souls(&at("975")), (50, Value::Null));

    let trace = |name: &str| {
        let (trace, state) = (
            dir.join(format!("{name}.jsonl")),
            dir.join(format!("{name}.json")),
        );
        let out = run_loom(&[
            "run",
            "--headless",
            "--frames",
            "1000",
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
            SOULS_STATE
        );
        fs::read_to_string(&trace).expect("the trace was written")
    };
    let first = trace("one");
    assert_eq!(first, trace("two"));
    assert_trace_fits(&first, "souls");
    let events: Vec<(u64, Value)> = trace_events(&first)
        .into_iter()
        .filter(|(_, e)| {
            ["souls", "souls_dropped", "souls_recovered"]
                .contains(&e["type"].as_str().unwrap_or(""))
        })
        .collect();
    let want: Vec<(u64, Value)> = vec![
        (
            250,
            serde_json::json!({"type": "souls", "from": "dummy_b", "amount": 50, "total": 50}),
        ),
        (
            764,
            serde_json::json!({"type": "souls_dropped", "position": died, "amount": 50, "lost": 0}),
        ),
        (
            975,
            serde_json::json!({"type": "souls_recovered", "amount": 50, "total": 50}),
        ),
    ];
    assert_eq!(events, want);
    let s: Value = serde_json::from_str(SOULS_STATE).expect("valid JSON");
    assert_eq!(souls(&s), (50, Value::Null));
}
