//! Bonfires: resting, standing up, death and the respawn.

use std::fs;

use serde_json::Value;

use super::common::{
    assert_trace_fits, path_str, report, run_headless_replay, run_loom, test_dir, trace_events,
    write_file,
};

/// The bonfire: walk to it (diagonally 1-99, then along +Z 100-129), sit
/// (E on 140), stand (E on 160), walk to the stake (170-349 along +X,
/// 350-432 along -Z) and stand in front of it until it kills.
const BONFIRE_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 140, action: \"interact\", down: true), (tick: 141, action: \"interact\", down: false), ",
    "(tick: 160, action: \"interact\", down: true), (tick: 161, action: \"interact\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_x\", value: -1.0), (tick: 1, axis: \"move_y\", value: -1.0), ",
    "(tick: 100, axis: \"move_x\", value: 0.0), (tick: 130, axis: \"move_y\", value: 0.0), ",
    "(tick: 170, axis: \"move_x\", value: 1.0), (tick: 350, axis: \"move_x\", value: 0.0), ",
    "(tick: 350, axis: \"move_y\", value: 1.0), (tick: 433, axis: \"move_y\", value: 0.0)",
    "])\n",
);

/// The state on tick 900 of [`BONFIRE_SCRIPT`], the respawn at the bonfire.
const BONFIRE_STATE: &str = include_str!("expected/bonfire_state.json");

#[test]
fn demo_bonfire_rest_stand_death_and_respawn_replay_the_same_every_time() {
    let dir = test_dir("demo_bonfire_rest_stand_death_and_respawn_replay_the_same_every_time");
    let script = write_file(&dir, "bonfire.ron", BONFIRE_SCRIPT);
    let at = |frames: &str| -> Value {
        let state = dir.join(format!("{frames}.json"));
        run_headless_replay(frames, &script, &state);
        serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
            .expect("state file is valid JSON")
    };
    // Sitting down on 140 makes where the character stands the respawn
    // point, and resets the stake: its round starts again that tick.
    let s = at("140");
    let game = &s["game"];
    assert_eq!(game["player"]["action"], "sit");
    assert_eq!(game["respawn"]["position"], game["player"]["position"]);
    assert_eq!(game["respawn"]["facing"], game["player"]["facing"]);
    assert_eq!(game["respawn"]["bonfire"], "bonfire");
    assert_eq!(game["swingers"][0]["swing_tick"], 1);
    let rest = game["player"]["position"].clone();
    // Its round is 120 ticks from 140, active on 41-46: tick 899 is its
    // 40th, and 900 would be its 41st, a swing; the respawn resets it to 1.
    assert_eq!(at("899")["game"]["swingers"][0]["swing_tick"], 40);

    let trace = |name: &str| {
        let (trace, state) = (
            dir.join(format!("{name}.jsonl")),
            dir.join(format!("{name}.json")),
        );
        let out = run_loom(&[
            "run",
            "--headless",
            "--frames",
            "900",
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
            BONFIRE_STATE
        );
        fs::read_to_string(&trace).expect("the trace was written")
    };
    let first = trace("one");
    assert_eq!(first, trace("two"));
    assert_trace_fits(&first, "souls");
    let events: Vec<(u64, String)> = trace_events(&first)
        .iter()
        .filter(|(_, e)| {
            ["rest", "stand", "respawn", "hit"].contains(&e["type"].as_str().unwrap_or(""))
        })
        .map(|(t, e)| {
            let kind = e["type"].as_str().expect("a type");
            let what = match kind {
                "rest" => e["bonfire"].to_string(),
                "hit" => e["result"].to_string(),
                "respawn" => (e["position"] == rest).to_string(),
                _ => String::new(),
            };
            (*t, format!("{kind} {what}"))
        })
        .collect();
    // The stake hits on the 41st tick of its rounds from 380: 70, 40, 10,
    // then 0 hp on 780; the respawn 120 ticks later, where it sat.
    let want: Vec<(u64, String)> = [
        (140, r#"rest "bonfire""#),
        (160, "stand "),
        (420, r#"hit "stagger""#),
        (540, r#"hit "stagger""#),
        (660, r#"hit "stagger""#),
        (780, r#"hit "death""#),
        (900, "respawn true"),
    ]
    .iter()
    .map(|&(t, s)| (t, s.to_owned()))
    .collect();
    assert_eq!(events, want);
    let s: Value = serde_json::from_str(BONFIRE_STATE).expect("valid JSON");
    assert_eq!(s["game"]["player"]["position"], rest);
    assert_eq!(s["game"]["swingers"][0]["swing_tick"], 1);
    assert_eq!(s["game"]["player"]["health"]["hp"], 100.0);
}
