//! The rogue replay against its prototype state.

use std::fs;

use super::common::{path_str, repo_root, report, run_loom_in, test_dir, write_script};

/// The planner's prototype run (E4.4): 13 key presses, the k-th pressed on
/// tick 2k - 1 and released on tick 2k: wait x5, move_e x4, move_n, wait x3.
/// `zombie` walks up to the player, bites once and is killed in three hits;
/// the player steps onto its corpse and north, and `zombie_2` comes round
/// the L-shaped wall to stand next to the player.
pub(super) const ROGUE_SCRIPT: [&str; 26] = [
    r#"(tick: 1, action: "wait", down: true)"#,
    r#"(tick: 2, action: "wait", down: false)"#,
    r#"(tick: 3, action: "wait", down: true)"#,
    r#"(tick: 4, action: "wait", down: false)"#,
    r#"(tick: 5, action: "wait", down: true)"#,
    r#"(tick: 6, action: "wait", down: false)"#,
    r#"(tick: 7, action: "wait", down: true)"#,
    r#"(tick: 8, action: "wait", down: false)"#,
    r#"(tick: 9, action: "wait", down: true)"#,
    r#"(tick: 10, action: "wait", down: false)"#,
    r#"(tick: 11, action: "move_e", down: true)"#,
    r#"(tick: 12, action: "move_e", down: false)"#,
    r#"(tick: 13, action: "move_e", down: true)"#,
    r#"(tick: 14, action: "move_e", down: false)"#,
    r#"(tick: 15, action: "move_e", down: true)"#,
    r#"(tick: 16, action: "move_e", down: false)"#,
    r#"(tick: 17, action: "move_e", down: true)"#,
    r#"(tick: 18, action: "move_e", down: false)"#,
    r#"(tick: 19, action: "move_n", down: true)"#,
    r#"(tick: 20, action: "move_n", down: false)"#,
    r#"(tick: 21, action: "wait", down: true)"#,
    r#"(tick: 22, action: "wait", down: false)"#,
    r#"(tick: 23, action: "wait", down: true)"#,
    r#"(tick: 24, action: "wait", down: false)"#,
    r#"(tick: 25, action: "wait", down: true)"#,
    r#"(tick: 26, action: "wait", down: false)"#,
];

/// State after [`ROGUE_SCRIPT`] and 28 ticks of `assets/scenes/rogue.ron`,
/// as the planner's independent prototype computed it (docs/PLAN.md, E4.4):
/// 13 turns, the player on (4, 1) with 9 hp, `zombie` a corpse on (4, 2)
/// drawn at z (0 + 0.5) / 2 = 0.25, `zombie_2` on (5, 1).
pub(super) const ROGUE_STATE: &str = include_str!("expected/rogue_state.json");

#[test]
fn rogue_replay_fights_a_zombie_to_the_prototype_state() {
    let dir = test_dir("rogue_replay_fights_a_zombie_to_the_prototype_state");
    let script = write_script(&dir, "rogue.ron", &ROGUE_SCRIPT);
    let state = dir.join("state.json");
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
            "--dump-state",
            path_str(&state),
        ],
    );
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["game"], "rogue");
    assert_eq!(r["scene"], "assets/scenes/rogue.ron");
    assert_eq!(r["input"]["applied"], 26);
    let dump = fs::read_to_string(&state).expect("the state dump was written");
    assert_eq!(dump, ROGUE_STATE);
}
