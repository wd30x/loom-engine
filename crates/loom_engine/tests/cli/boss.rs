//! The boss: its fight, its stagger, its second phase.

use std::fs;

use serde_json::Value;

use super::common::{
    assert_trace_fits, path_str, repo_root, report, run_headless_replay_on, run_loom, test_dir,
    trace_events, write_file,
};

/// The boss fight: walk up (ticks 1-39), take the boss's cleave, roll back
/// through its combo's first cut (333, backwards 331-336), lock on (380),
/// walk in (382-427) and attack twice in its rest, the second buffered.
const BOSS_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 333, action: \"roll\", down: true), (tick: 334, action: \"roll\", down: false), ",
    "(tick: 380, action: \"lock\", down: true), (tick: 381, action: \"lock\", down: false), ",
    "(tick: 430, action: \"attack\", down: true), (tick: 431, action: \"attack\", down: false), ",
    "(tick: 448, action: \"attack\", down: true), (tick: 449, action: \"attack\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_y\", value: 1.0), (tick: 40, axis: \"move_y\", value: 0.0), ",
    "(tick: 331, axis: \"move_y\", value: -1.0), (tick: 337, axis: \"move_y\", value: 0.0), ",
    "(tick: 382, axis: \"move_y\", value: 1.0), (tick: 428, axis: \"move_y\", value: 0.0)",
    "])\n",
);

/// The state on tick 520 of [`BOSS_SCRIPT`], the character killed by the combo.
const BOSS_STATE: &str = include_str!("expected/boss_state.json");

/// SplitMix64's step: the boss's `rng` after `n` numbers from seed 7.
fn boss_rng(n: u64) -> u64 {
    7_u64.wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

/// The state of `scene` on tick `frames` of `script`, run in `dir`.
fn boss_at(
    dir: &std::path::Path,
    scene: &std::path::Path,
    script: &std::path::Path,
    frames: &str,
) -> Value {
    let state = dir.join(format!("{frames}.json"));
    run_headless_replay_on(Some(scene), frames, script, &state);
    serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
        .expect("state file is valid JSON")
}

/// The boss's `(state, move, move_tick, phase, chase, rng)`.
fn boss_of(dump: &Value) -> (Value, Value, Value, Value, Value, u64) {
    let e = &dump["game"]["enemies"][0];
    assert_eq!(e["node"], "boss");
    (
        e["state"].clone(),
        e["move"].clone(),
        e["move_tick"].clone(),
        e["phase"].clone(),
        e["chase"].clone(),
        e["rng"].as_u64().expect("rng is a whole number"),
    )
}

/// `(hp, state)` of a health in a state dump.
fn hp_of(health: &Value) -> (Value, Value) {
    (health["hp"].clone(), health["state"].clone())
}

/// The trace of `script` on `scene` for `frames` ticks, its last state
/// checked against `expected`, run twice to the same bytes and checked
/// against the schema; its events.
fn boss_trace(
    dir: &std::path::Path,
    scene: &std::path::Path,
    script: &std::path::Path,
    frames: &str,
    expected: &str,
) -> Vec<(u64, Value)> {
    let trace = |name: &str| {
        let (trace, state) = (
            dir.join(format!("{name}.jsonl")),
            dir.join(format!("{name}.json")),
        );
        let out = run_loom(&[
            "run",
            "--headless",
            "--frames",
            frames,
            "--scene",
            path_str(scene),
            "--input",
            path_str(script),
            "--trace",
            path_str(&trace),
            "--dump-state",
            path_str(&state),
        ]);
        let r = report(&out);
        assert_eq!(out.code, 0, "{r}");
        assert_eq!(
            fs::read_to_string(&state).expect("the state dump was written"),
            expected
        );
        fs::read_to_string(&trace).expect("the trace was written")
    };
    let first = trace("one");
    assert_eq!(first, trace("two"));
    assert_trace_fits(&first, "souls");
    trace_events(&first)
}

/// `(tick, move, candidates)` of every `enemy_move` in `events`.
fn enemy_moves(events: &[(u64, Value)]) -> Vec<(u64, String, Value)> {
    events
        .iter()
        .filter(|(_, e)| e["type"] == "enemy_move")
        .map(|(t, e)| {
            let m = e["move"].as_str().expect("a move").to_owned();
            (*t, m, e["candidates"].clone())
        })
        .collect()
}

/// `(tick, "type by attack")` of every hit, dodge and stage in `events`.
fn fight_events(events: &[(u64, Value)]) -> Vec<(u64, String)> {
    events
        .iter()
        .filter(|(_, e)| ["hit", "dodged", "stage"].contains(&e["type"].as_str().unwrap_or("")))
        .map(|(t, e)| {
            let kind = e["type"].as_str().expect("a type");
            (*t, format!("{kind} {} {}", e["by"], e["attack"]))
        })
        .collect()
}

fn owned(list: &[(u64, &str)]) -> Vec<(u64, String)> {
    list.iter().map(|&(t, s)| (t, s.to_owned())).collect()
}

#[test]
fn boss_fight_replays_its_moves_hits_and_dodges_the_same_every_time() {
    let dir = test_dir("boss_fight_replays_its_moves_hits_and_dodges_the_same_every_time");
    let script = write_file(&dir, "boss.ron", BOSS_SCRIPT);
    let scene = repo_root().join("assets/scenes/boss.ron");
    let at = |frames: &str| boss_at(&dir, &scene, &script, frames);
    let player = |dump: &Value| hp_of(&dump["game"]["player"]["health"]);
    // 11.9 m apart on tick 1, beyond every move: only the approach, of
    // weight min(floor(2 * 9.9), 4) = 4, chosen with the first number;
    // 30 ticks at most, one walked.
    let s = at("1");
    assert_eq!(
        boss_of(&s),
        (
            "chase".into(),
            Value::Null,
            0.into(),
            Value::Null,
            29.into(),
            boss_rng(1)
        )
    );
    assert_eq!(s["game"]["enemies"][0]["stage"], 1);
    // On tick 31, 8.7 m apart, the lunge's 2 against the approach's 4:
    // the second number chose the lunge.
    let s = at("31");
    assert_eq!(
        boss_of(&s),
        (
            "attack".into(),
            "lunge".into(),
            1.into(),
            "startup".into(),
            0.into(),
            boss_rng(2)
        )
    );
    // Another approach (151), then the cleave chosen on tick 179 hits on
    // its first active tick, 179 + 45: 100 - 45 hp, staggered (poise 0).
    let s = at("224");
    assert_eq!(
        boss_of(&s),
        (
            "attack".into(),
            "cleave".into(),
            46.into(),
            "active".into(),
            0.into(),
            boss_rng(4)
        )
    );
    assert_eq!(player(&s), (55.0.into(), "stagger".into()));
    // The combo chosen on tick 308: its first cut (334-338) is rolled
    // through, the others out of reach.
    let s = at("335");
    assert_eq!(
        boss_of(&s),
        (
            "attack".into(),
            "combo".into(),
            28.into(),
            "active".into(),
            0.into(),
            boss_rng(5)
        )
    );
    assert_eq!(player(&s), (55.0.into(), "ok".into()));
    // Resting after the combo, hit by the first attack: 600 - 20.
    let s = at("441");
    assert_eq!(s["game"]["enemies"][0]["state"], "rest");
    assert_eq!(
        hp_of(&s["game"]["hurtboxes"][0]["health"]),
        (580.0.into(), "ok".into())
    );

    // The whole state on the last tick, and the trace of two runs.
    let events = boss_trace(&dir, &scene, &script, "520", BOSS_STATE);
    let close = serde_json::json!(["sweep", "cleave", "combo"]);
    let far = serde_json::json!(["lunge", "approach"]);
    let want: Vec<(u64, String, Value)> = [
        (1, "approach", serde_json::json!(["approach"])),
        (31, "lunge", far),
        (151, "approach", serde_json::json!(["cleave", "approach"])),
        (179, "cleave", close.clone()),
        (308, "combo", close.clone()),
        (457, "combo", close),
    ]
    .into_iter()
    .map(|(t, m, c)| (t, m.to_owned(), c))
    .collect();
    assert_eq!(enemy_moves(&events), want);
    // Hits both ways and the dodge, by tick.
    assert_eq!(
        fight_events(&events),
        owned(&[
            (224, r#"hit "boss" "cleave""#),
            (335, r#"dodged "boss" null"#),
            (441, r#"hit "player" "attack""#),
            (471, r#"hit "player" "attack""#),
            (484, r#"hit "boss" "combo""#),
            (501, r#"hit "boss" "combo""#),
            (518, r#"hit "boss" "combo""#),
        ])
    ); // The health bar's trail: each cut of the combo (484, 501, 518) starts
    // its 30-tick wait over, so on 520 it still stands at 55, the health
    // before the combo, two ticks into the last wait.
    let s: Value = serde_json::from_str(BOSS_STATE).expect("valid JSON");
    assert_eq!(s["game"]["player"]["hp_trail"], 55.0);
    assert_eq!(s["game"]["player"]["hp_trail_wait"], 27);
}

/// Two heavy attacks stagger the boss (poise 50): walk up (1-39), backstep
/// through its cleave (223), lock on (226), step in (244-249), a heavy
/// (251) and another one buffered (286).
const BOSS_STAGGER_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 223, action: \"roll\", down: true), (tick: 224, action: \"roll\", down: false), ",
    "(tick: 226, action: \"lock\", down: true), (tick: 227, action: \"lock\", down: false), ",
    "(tick: 251, action: \"heavy\", down: true), (tick: 252, action: \"heavy\", down: false), ",
    "(tick: 286, action: \"heavy\", down: true), (tick: 287, action: \"heavy\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_y\", value: 1.0), (tick: 40, axis: \"move_y\", value: 0.0), ",
    "(tick: 244, axis: \"move_y\", value: 1.0), (tick: 250, axis: \"move_y\", value: 0.0)",
    "])\n",
);

#[test]
fn two_heavy_attacks_around_the_boss_rest_stagger_it() {
    let dir = test_dir("two_heavy_attacks_around_the_boss_rest_stagger_it");
    let script = write_file(&dir, "stagger.ron", BOSS_STAGGER_SCRIPT);
    let scene = repo_root().join("assets/scenes/boss.ron");
    let at = |frames: &str| boss_at(&dir, &scene, &script, frames);
    let boss = |dump: &Value| hp_of(&dump["game"]["hurtboxes"][0]["health"]);
    // The first heavy lands in its rest (267-306): 30 impact.
    let s = at("274");
    assert_eq!(s["game"]["enemies"][0]["state"], "rest");
    assert_eq!(boss(&s), (565.0.into(), "ok".into()));
    assert_eq!(s["game"]["hurtboxes"][0]["health"]["impact"], 30.0);
    // A heavy takes 50 ticks and the rest 40: the second lands on tick
    // 324, 16 ticks into the combo chosen on 308, past 50 impact.
    let s = at("323");
    assert_eq!(
        boss_of(&s),
        (
            "attack".into(),
            "combo".into(),
            16.into(),
            "startup".into(),
            0.into(),
            boss_rng(5)
        )
    );
    let s = at("324");
    assert_eq!(boss(&s), (530.0.into(), "stagger".into()));
    assert_eq!(
        boss_of(&s),
        (
            "stagger".into(),
            Value::Null,
            0.into(),
            Value::Null,
            0.into(),
            boss_rng(5)
        )
    );
    assert_eq!(s["game"]["enemies"][0]["stage"], 1);
}

/// The state on tick 520 of [`BOSS_STAGGER_SCRIPT`] on the boss with 100 hp.
const BOSS_PHASE2_STATE: &str = include_str!("expected/boss_phase2_state.json");

#[test]
fn boss_brought_to_half_health_roars_into_a_faster_phase_2() {
    let dir = test_dir("boss_brought_to_half_health_roars_into_a_faster_phase_2");
    let script = write_file(&dir, "stagger.ron", BOSS_STAGGER_SCRIPT);
    // The boss scene with 100 hp instead of 600: phase 2 from 50.
    let source = fs::read_to_string(repo_root().join("assets/scenes/boss.ron"))
        .expect("the boss scene is in the repository");
    assert_eq!(source.matches(r#"("hp", 600),"#).count(), 1);
    let scene = write_file(
        &dir,
        "boss.ron",
        &source.replace(r#"("hp", 600),"#, r#"("hp", 100),"#),
    );
    let at = |frames: &str| boss_at(&dir, &scene, &script, frames);
    let boss = |dump: &Value| hp_of(&dump["game"]["hurtboxes"][0]["health"]);
    // The first heavy: 65 hp. The second (324) leaves 30, at most half:
    // phase 2, its stagger dropped for the roar, the combo broken off.
    let s = at("274");
    assert_eq!(boss(&s), (65.0.into(), "ok".into()));
    let s = at("324");
    assert_eq!(boss(&s), (30.0.into(), "ok".into()));
    let e = &s["game"]["enemies"][0];
    assert_eq!(
        (e["stage"].clone(), boss_of(&s)),
        (
            2.into(),
            (
                "roar".into(),
                Value::Null,
                1.into(),
                "startup".into(),
                0.into(),
                boss_rng(5)
            )
        )
    );
    // The roar's 70 ticks end on 393; on 394 the cleave, its startup of
    // 45 cut to round(45 * 0.75) = 34: still startup on its 34th tick,
    // active on its 35th.
    let s = at("427");
    assert_eq!(
        boss_of(&s),
        (
            "attack".into(),
            "cleave".into(),
            34.into(),
            "startup".into(),
            0.into(),
            boss_rng(6)
        )
    );
    let s = at("428");
    assert_eq!(boss_of(&s).3, "active");
    // Its rest of 40 cut to 30.
    let s = at("471");
    assert_eq!(
        (
            s["game"]["enemies"][0]["state"].clone(),
            s["game"]["enemies"][0]["rest"].clone()
        ),
        ("rest".into(), 30.into())
    );

    let events = boss_trace(&dir, &scene, &script, "520", BOSS_PHASE2_STATE);
    // The stage right after the hit that brought it there; the blast on
    // the roar's 20th tick hits the character standing 2 m off.
    assert_eq!(
        fight_events(&events),
        owned(&[
            (224, r#"dodged "boss" null"#),
            (225, r#"dodged "boss" null"#),
            (226, r#"dodged "boss" null"#),
            (227, r#"dodged "boss" null"#),
            (274, r#"hit "player" "heavy""#),
            (324, r#"hit "player" "heavy""#),
            (324, r#"stage "boss" null"#),
            (343, r#"hit "boss" "roar""#),
            (428, r#"hit "boss" "cleave""#),
        ])
    );
    let stage: Vec<&Value> = events
        .iter()
        .filter(|(_, e)| e["type"] == "stage")
        .map(|(_, e)| e)
        .collect();
    assert_eq!(
        stage,
        [&serde_json::json!({"type": "stage", "by": "boss", "stage": 2})]
    );
    let spin = serde_json::json!(["sweep", "cleave", "combo", "spin"]);
    assert_eq!(
        enemy_moves(&events)[5..],
        [
            (394, "cleave".to_owned(), spin.clone()),
            (502, "spin".to_owned(), spin),
        ]
    );
}
