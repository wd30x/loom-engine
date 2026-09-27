//! Traces: a line a tick, the same on every run, matching the state dumps.

use std::fs;

use serde_json::Value;

use super::common::{
    assert_trace_fits, path_str, repo_root, report, run_headless_replay, run_loom, run_loom_in,
    test_dir, trace_events, write_file, write_script,
};
use super::rogue::ROGUE_SCRIPT;
use super::souls_moves::{SOULS_KILL_SCRIPT, SOULS_KILL_STATE, SOULS_STAKE_SCRIPT};

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
    // checks them: staggered on 212, killed on 250 (its 50 souls right
    // after the hit) and let go, revived on 430, locked on again on 440.
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
            serde_json::json!({"type": "souls", "from": "dummy_b", "amount": 50, "total": 50}),
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
