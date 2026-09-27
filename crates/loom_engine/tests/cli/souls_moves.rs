//! The souls character: rolls, backsteps, sprints, attacks, lock-on,
//! combos, hits, kills and the swinging stake.

use std::fs;

use serde_json::Value;

use super::common::{
    assert_f32_bits, g2_3_demo, run_headless_replay, run_headless_replay_on, test_dir, write_file,
};

/// The souls moves script: tap roll while running forward (a roll from
/// tick 2), stop, tap roll again (a backstep from tick 41), then hold roll
/// while running from tick 70 (a sprint from tick 85, the 16th held tick)
/// until tick 100.
const SOULS_MOVES_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"roll\", down: true), (tick: 2, action: \"roll\", down: false), ",
    "(tick: 40, action: \"roll\", down: true), (tick: 41, action: \"roll\", down: false), ",
    "(tick: 70, action: \"roll\", down: true), (tick: 100, action: \"roll\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_y\", value: 1.0), (tick: 34, axis: \"move_y\", value: 0.0), ",
    "(tick: 70, axis: \"move_y\", value: 1.0)",
    "])\n",
);

/// State after [`SOULS_MOVES_SCRIPT`] and 110 ticks of the G2.3 demo scene.
/// Determinism only: no independent source for every byte. The stamina
/// is checked by hand in the test.
pub(super) const SOULS_MOVES_STATE: &str = include_str!("expected/souls_moves_state.json");

#[test]
fn souls_roll_backstep_and_sprint_replay_to_the_expected_state() {
    let dir = test_dir("souls_roll_backstep_and_sprint_replay_to_the_expected_state");
    let script = write_file(&dir, "moves.ron", SOULS_MOVES_SCRIPT);
    let state = dir.join("state.json");
    // The G2.3 scene: the dummies would add nodes to the dump.
    run_headless_replay_on(Some(&g2_3_demo()), "110", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_MOVES_STATE);
    // By hand, with the default props: the roll costs 20 (80 left, 30
    // ticks to wait); ticks 34..=40 count 7 down; the backstep costs 12
    // (68, wait 30); ticks 61..=84 count 24 down; sprinting on ticks
    // 85..=99 drains 15 x 15/60 = 3.75 (64.25, wait 30 each tick); ticks
    // 100..=110 count 11 down: 19 left, nothing regenerated yet.
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    let player = &dump["game"]["player"];
    assert_eq!(player["stamina"], 64.25);
    assert_eq!(player["stamina_wait"], 19);
    assert_eq!(player["action"], "idle");
    // Forward 1/15 + 3.2 (roll), back 1.5 (backstep), forward 1 + 1.5
    // (15 run, 15 sprint ticks) + 11/15: z = -5, up to rounding.
    let z = player["position"][2].as_f64().expect("z is a number");
    assert!((z + 5.0).abs() < 1e-4, "{z}");
}

/// A light attack on tick 1, two more pressed during the recovery of the
/// one before (ticks 20 and 50) and a heavy one pressed during the third
/// (tick 80), running forward the whole time.
const SOULS_ATTACKS_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"attack\", down: true), (tick: 2, action: \"attack\", down: false), ",
    "(tick: 20, action: \"attack\", down: true), (tick: 21, action: \"attack\", down: false), ",
    "(tick: 50, action: \"attack\", down: true), (tick: 51, action: \"attack\", down: false), ",
    "(tick: 80, action: \"heavy\", down: true), (tick: 81, action: \"heavy\", down: false)",
    "], axes: [(tick: 1, axis: \"move_y\", value: 1.0)])\n",
);

/// State after [`SOULS_ATTACKS_SCRIPT`] and 120 ticks of the G2.3 demo
/// scene. Determinism only: no independent source for every byte. The
/// stamina, the combo and the action are checked by hand in the test.
pub(super) const SOULS_ATTACKS_STATE: &str = include_str!("expected/souls_attacks_state.json");

#[test]
fn souls_light_combo_then_heavy_replays_to_the_expected_state() {
    let dir = test_dir("souls_light_combo_then_heavy_replays_to_the_expected_state");
    let script = write_file(&dir, "attacks.ron", SOULS_ATTACKS_SCRIPT);
    // By hand, with the default props: light attacks last 10 + 4 + 16 = 30
    // ticks, so each buffered press starts the next one right after the
    // last: light on ticks 1, 31 and 61, heavy on 91.
    let third = dir.join("third.json");
    run_headless_replay_on(Some(&g2_3_demo()), "61", &script, &third);
    let dump: Value =
        serde_json::from_str(&fs::read_to_string(&third).expect("the state dump was written"))
            .expect("state file is valid JSON");
    let player = &dump["game"]["player"];
    assert_eq!(player["action"], "attack");
    assert_eq!(player["action_tick"], 1);
    assert_eq!(player["combo"], 3);

    let state = dir.join("state.json");
    // The G2.3 scene: the dummies would add nodes to the dump.
    run_headless_replay_on(Some(&g2_3_demo()), "120", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_ATTACKS_STATE);
    // Tick 120 is heavy tick 30 of 22 + 5 + 23: recovery. Stamina: 100 -
    // 3 x 15 - 25 = 30, the wait not counting down while attacking. The
    // slides: 3 x 0.3 + 0.6 = 1.5 forward, up to rounding.
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    let player = &dump["game"]["player"];
    assert_eq!(player["action"], "heavy");
    assert_eq!(player["action_tick"], 30);
    assert_eq!(player["attack_phase"], "recovery");
    assert_eq!(player["combo"], 0);
    assert_eq!(player["buffered"], Value::Null);
    assert_eq!(player["stamina"], 30.0);
    assert_eq!(player["stamina_wait"], 30);
    let z = player["position"][2].as_f64().expect("z is a number");
    assert!((z + 1.5).abs() < 1e-4, "{z}");
}

/// Lock on (tick 1: straight ahead is dummy_b), strafe right for a second
/// (ticks 10..=69), flick `look_x` once (tick 80: over the 0.1 threshold,
/// to dummy_c on the right), then let go (tick 100).
const SOULS_LOCK_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"lock\", down: true), (tick: 2, action: \"lock\", down: false), ",
    "(tick: 100, action: \"lock\", down: true), (tick: 101, action: \"lock\", down: false)",
    "], axes: [",
    "(tick: 10, axis: \"move_x\", value: 1.0), (tick: 70, axis: \"move_x\", value: 0.0), ",
    "(tick: 80, axis: \"look_x\", value: 0.12), (tick: 81, axis: \"look_x\", value: 0.0)",
    "])\n",
);

/// State after [`SOULS_LOCK_SCRIPT`] and 120 ticks of the built-in scene.
/// Determinism only: no independent source for every byte. Which target
/// is locked on when is checked in the test.
pub(super) const SOULS_LOCK_STATE: &str = include_str!("expected/souls_lock_state.json");

#[test]
fn souls_lock_strafe_switch_and_release_replay_to_the_expected_state() {
    let dir = test_dir("souls_lock_strafe_switch_and_release_replay_to_the_expected_state");
    let script = write_file(&dir, "lock.ron", SOULS_LOCK_SCRIPT);
    let lock_after = |frames: &str| {
        let state = dir.join(format!("{frames}.json"));
        run_headless_replay(frames, &script, &state);
        let dump: Value =
            serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
                .expect("state file is valid JSON");
        dump["game"]["lock"].clone()
    };
    // The middle dummy is the one straight ahead; the flick on tick 80
    // switches to the right one; the press on tick 100 lets go.
    assert_eq!(lock_after("1"), "dummy_b");
    assert_eq!(lock_after("79"), "dummy_b");
    assert_eq!(lock_after("80"), "dummy_c");
    assert_eq!(lock_after("99"), "dummy_c");

    let state = dir.join("state.json");
    run_headless_replay("120", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_LOCK_STATE);
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    assert_eq!(dump["game"]["lock"], Value::Null);
}

/// Lock on (tick 1: dummy_b), walk up to it until its block stops the
/// character (ticks 2..=95), then a light attack on tick 100 and two more
/// pressed during the recovery of the one before (ticks 115 and 145): a
/// three-hit combo on ticks 100, 130 and 160.
const SOULS_COMBO_HITS_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"lock\", down: true), (tick: 2, action: \"lock\", down: false), ",
    "(tick: 100, action: \"attack\", down: true), (tick: 101, action: \"attack\", down: false), ",
    "(tick: 115, action: \"attack\", down: true), (tick: 116, action: \"attack\", down: false), ",
    "(tick: 145, action: \"attack\", down: true), (tick: 146, action: \"attack\", down: false)",
    "], axes: [",
    "(tick: 2, axis: \"move_y\", value: 1.0), (tick: 96, axis: \"move_y\", value: 0.0)",
    "])\n",
);

/// State after [`SOULS_COMBO_HITS_SCRIPT`] and 200 ticks of the built-in
/// scene. Determinism only: no independent source for every byte. The hits
/// are checked by hand in the test.
pub(super) const SOULS_COMBO_HITS_STATE: &str =
    include_str!("expected/souls_combo_hits_state.json");

#[test]
fn souls_light_combo_hits_the_locked_dummy_three_times_and_replays() {
    let dir = test_dir("souls_light_combo_hits_the_locked_dummy_three_times_and_replays");
    let script = write_file(&dir, "combo.ron", SOULS_COMBO_HITS_SCRIPT);
    let state = dir.join("state.json");
    run_headless_replay("200", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_COMBO_HITS_STATE);
    // By hand: dummy_b's block stops the character at z = -6.1 + 0.35 =
    // -5.75, 0.75 m from dummy_b. On an attack's first active tick the
    // -30° edge of the sweep passes 0.75 * sin 30° = 0.375 m from it,
    // within its radius 0.4: hits on ticks 110, 140 and 170.
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    let hurtboxes = &dump["game"]["hurtboxes"];
    assert_eq!(hurtboxes[1]["node"], "dummy_b");
    assert_eq!(hurtboxes[1]["hits"], 3);
    assert_eq!(
        hurtboxes[1]["last_hit"],
        serde_json::json!({"tick": 170, "attack": "attack", "combo": 3})
    );
    for other in [&hurtboxes[0], &hurtboxes[2]] {
        assert_eq!(other["hits"], 0, "{other}");
    }
    // By hand: three light hits of 20 damage leave 40 of 100 hp. Their
    // impacts add up to 10, 20, then 30 > poise 25: a stagger on tick 170
    // for 30 ticks, over by tick 200, which empties the impact; the reset
    // wait set on tick 170 (60 ticks) has 30 left.
    assert_eq!(
        hurtboxes[1]["health"],
        serde_json::json!({
            "hp": 40.0, "state": "ok", "state_ticks": 0, "impact": 0.0, "impact_wait": 30
        })
    );
    // The bar at 40 %: 0.8 * 0.4 wide, moved left by 0.8 * 0.6 / 2 so its
    // left end stays at x = -0.4 (up to rounding).
    let bar = &dump["nodes"]["dummy_b/hp_bar"];
    let width = bar["scale"][0].as_f64().expect("a number") as f32;
    let x = bar["position"][0].as_f64().expect("a number") as f32;
    assert_eq!(width, 0.8 * 0.4);
    assert_eq!(x, -(0.8 * (1.0 - 0.4)) / 2.0);
    assert!(
        (x - width / 2.0 + 0.4).abs() < 1e-6,
        "left end {}",
        x - width / 2.0
    );
}

/// [`SOULS_COMBO_HITS_SCRIPT`] (lock on dummy_b, walk up, three light hits
/// on ticks 110, 140 and 170: 40 hp left), then a heavy attack pressed on
/// tick 190 and a light one on 240. The lock key on tick 300 finds nothing
/// to lock on to, the one on tick 440 locks on again.
pub(super) const SOULS_KILL_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 1, action: \"lock\", down: true), (tick: 2, action: \"lock\", down: false), ",
    "(tick: 100, action: \"attack\", down: true), (tick: 101, action: \"attack\", down: false), ",
    "(tick: 115, action: \"attack\", down: true), (tick: 116, action: \"attack\", down: false), ",
    "(tick: 145, action: \"attack\", down: true), (tick: 146, action: \"attack\", down: false), ",
    "(tick: 190, action: \"heavy\", down: true), (tick: 191, action: \"heavy\", down: false), ",
    "(tick: 240, action: \"attack\", down: true), (tick: 241, action: \"attack\", down: false), ",
    "(tick: 300, action: \"lock\", down: true), (tick: 301, action: \"lock\", down: false), ",
    "(tick: 440, action: \"lock\", down: true), (tick: 441, action: \"lock\", down: false)",
    "], axes: [",
    "(tick: 2, axis: \"move_y\", value: 1.0), (tick: 96, axis: \"move_y\", value: 0.0)",
    "])\n",
);

/// State after [`SOULS_KILL_SCRIPT`] and 450 ticks of the built-in scene.
/// Determinism only: no independent source for every byte. The health and
/// the lock along the way are checked by hand in the test.
pub(super) const SOULS_KILL_STATE: &str = include_str!("expected/souls_kill_state.json");

#[test]
fn souls_dummy_killed_while_locked_lets_go_then_revives_and_locks_again() {
    let dir = test_dir("souls_dummy_killed_while_locked_lets_go_then_revives_and_locks_again");
    let script = write_file(&dir, "kill.ron", SOULS_KILL_SCRIPT);
    let after = |frames: &str| {
        let state = dir.join(format!("{frames}.json"));
        run_headless_replay(frames, &script, &state);
        let dump: Value =
            serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
                .expect("state file is valid JSON");
        let dummy_b = &dump["game"]["hurtboxes"][1];
        assert_eq!(dummy_b["node"], "dummy_b");
        (
            dump["game"]["lock"].clone(),
            dummy_b["health"].clone(),
            dummy_b["hits"].clone(),
        )
    };
    let health = |hp: f32, state: &str, state_ticks: u32, impact_wait: u32| {
        serde_json::json!({
            "hp": hp, "state": state, "state_ticks": state_ticks,
            "impact": 0.0, "impact_wait": impact_wait
        })
    };
    // By hand, from 40 hp and 30 impact wait left on tick 200 (see
    // `souls_light_combo_hits_the_locked_dummy_three_times_and_replays`):
    // the heavy attack starts on tick 190 and hits on its first active
    // tick, 212 (-35: 5 left; impact 30 > poise 25: staggered for 30
    // ticks, the reset wait set to 60).
    assert_eq!(
        after("212"),
        ("dummy_b".into(), health(5.0, "stagger", 30, 60), 4.into())
    );
    // The light attack of tick 240 hits on tick 250: 5 - 20 is 0, dead
    // for 180 ticks, the impact and its wait emptied, the lock let go.
    assert_eq!(after("249").0, "dummy_b");
    assert_eq!(
        after("250"),
        (Value::Null, health(0.0, "dead", 180, 0), 5.into())
    );
    // Tick 300: dead dummy_b cannot be locked on to, and the other two
    // are more than lock_angle to the sides.
    assert_eq!(
        after("300"),
        (Value::Null, health(0.0, "dead", 130, 0), 5.into())
    );
    // Back at full health on tick 250 + 180 = 430, and locked on again by
    // the press on tick 440.
    assert_eq!(after("429").1, health(0.0, "dead", 1, 0));
    assert_eq!(
        after("430"),
        (Value::Null, health(100.0, "ok", 0, 0), 5.into())
    );

    let state = dir.join("state.json");
    run_headless_replay("450", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_KILL_STATE);
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    assert_eq!(dump["game"]["lock"], "dummy_b");
    assert_eq!(
        dump["game"]["hurtboxes"][1]["health"],
        health(100.0, "ok", 0, 0)
    ); // Killing dummy_b gave its 50 souls; its revival takes none back.
    assert_eq!(dump["game"]["player"]["souls"], 50);
}

/// Walk right to 2 m in front of the stake (ticks 1..=90) and stand through
/// its second round. Before its third round, tap roll (274-275) to roll left
/// through its sweep; walk back right (307..=354) and stand there until it
/// kills the character.
pub(super) const SOULS_STAKE_SCRIPT: &str = concat!(
    "(version: 1, tick_hz: 60, events: [",
    "(tick: 274, action: \"roll\", down: true), (tick: 275, action: \"roll\", down: false)",
    "], axes: [",
    "(tick: 1, axis: \"move_x\", value: 1.0), (tick: 91, axis: \"move_x\", value: 0.0), ",
    "(tick: 275, axis: \"move_x\", value: -1.0), (tick: 276, axis: \"move_x\", value: 0.0), ",
    "(tick: 307, axis: \"move_x\", value: 1.0), (tick: 355, axis: \"move_x\", value: 0.0)",
    "])\n",
);

/// State after [`SOULS_STAKE_SCRIPT`] and 763 ticks of the built-in scene:
/// the tick the character respawns. Determinism only: no independent source
/// for every byte. The hits are checked by hand in the test.
pub(super) const SOULS_STAKE_STATE: &str = include_str!("expected/souls_stake_state.json");

#[test]
fn souls_stake_hits_staggers_is_rolled_through_then_kills_and_the_character_respawns() {
    let dir = test_dir(
        "souls_stake_hits_staggers_is_rolled_through_then_kills_and_the_character_respawns",
    );
    let script = write_file(&dir, "stake.ron", SOULS_STAKE_SCRIPT);
    let player = |frames: &str| {
        let state = dir.join(format!("{frames}.json"));
        run_headless_replay(frames, &script, &state);
        let dump: Value =
            serde_json::from_str(&fs::read_to_string(&state).expect("the state dump was written"))
                .expect("state file is valid JSON");
        dump["game"]["player"].clone()
    };
    let health = |hp: f32, state: &str, state_ticks: u32, impact_wait: u32| {
        serde_json::json!({
            "hp": hp, "state": state, "state_ticks": state_ticks,
            "impact": 0.0, "impact_wait": impact_wait
        })
    };
    let hit = |tick: u64| serde_json::json!({"tick": tick, "by": "stake"});
    // By hand: the stake stands at (6, 0, 2) facing -Z and sweeps 150° in
    // six 25° slices from the right, active on ticks 41..=46 of each
    // 120-tick round. The character stops at x = 6 (up to rounding), 2 m
    // in front of it, on the edge between the third and fourth slices: the
    // third active tick of the second round, 120 + 43, hits. 30 damage,
    // impact 20 > poise 0: staggered for 30 ticks.
    let p = player("162");
    assert_eq!(
        (p["hits"].clone(), p["last_hit"].clone()),
        (0.into(), Value::Null)
    );
    let p = player("163");
    assert_f32_bits(&p["position"], [5.999997, 0.0, 0.0]);
    assert_eq!(p["hits"], 1);
    assert_eq!(p["last_hit"], hit(163));
    assert_eq!(p["health"], health(70.0, "stagger", 30, 60));
    // Standing again on 193; the impact's reset wait set on 163 has 30 left.
    assert_eq!(player("193")["health"], health(70.0, "ok", 0, 30));
    // The third round (ticks 281..=286): the roll from tick 275 is
    // invulnerable through tick 294. On tick 285 the character is 1.1 m left
    // of the stake's center line, 2 m in front: 29° to the left, inside the
    // fifth slice (25°-50°) that tick sweeps, and within reach. Not hit.
    let p = player("285");
    assert_eq!(
        (p["action"].clone(), p["invulnerable"].clone()),
        ("roll".into(), true.into())
    );
    assert_f32_bits(&p["position"], [4.899998, 0.0, 0.0]);
    assert_eq!(p["hits"], 1);
    assert_eq!(player("300")["hits"], 1);
    // Back in front of it from tick 354: hit on 403 and 523 (40, then 10
    // hp left), killed on 643: dead for 120 ticks, lying where it stood.
    let p = player("643");
    assert_eq!(p["hits"], 4);
    assert_eq!(p["last_hit"], hit(643));
    assert_eq!(p["health"], health(0.0, "dead", 120, 0));
    let p = player("762");
    assert_eq!(p["health"], health(0.0, "dead", 1, 0));
    assert_f32_bits(&p["position"], [5.999997, 0.0, 0.0]);
    // Tick 643 + 120: back at the start at full health.
    let state = dir.join("state.json");
    run_headless_replay("763", &script, &state);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, SOULS_STAKE_STATE);
    let dump: Value = serde_json::from_str(&dump).expect("state file is valid JSON");
    let p = &dump["game"]["player"];
    assert_f32_bits(&p["position"], [0.0, 0.0, 0.0]);
    assert_eq!(p["facing"], 0.0);
    assert_eq!(p["health"], health(100.0, "ok", 0, 0));
    assert_eq!(p["hits"], 4);
    // The respawn resets the world: the stake's round, on tick 43 of 120
    // ((763 - 1) % 120 + 1) before G5.1, starts again: 0, then this tick's
    // step 5c makes it 1.
    assert_eq!(dump["game"]["swingers"][0]["swing_tick"], 1);
}
