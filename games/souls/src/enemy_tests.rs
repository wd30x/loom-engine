//! Tests of the enemies (`docs/architecture.md` section 11, "敌人和 Boss"),
//! a child of `game` so they can read the enemy's own fields. The test
//! enemy walks 0.1 m a tick and turns 3° a tick; every move is 2 ticks of
//! startup, active and recovery unless a test says otherwise.

use glam::{Quat, Vec3};
use loom_core::debug::DebugDraw;
use loom_core::input::{ActionState, TickInput};

use super::*;

/// The character at the origin facing -Z, the camera behind it; `player`
/// props on the character, then `nodes`.
fn scene(player: &str, nodes: &str) -> String {
    format!(
        r#"(
    version: 1,
    game: "souls",
    camera: "cam",
    ambient: (0.2, 0.2, 0.2),
    nodes: [
        (name: "player", mesh: Capsule(radius: 0.35, height: 1.8),
            character: (run_speed: 3.75, turn_speed: 720.0), props: [{player}]),
        (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0),
            orbit_camera: (target: "player", pivot_height: 1.5, distance: 4.0,
                yaw: 0.0, pitch: 0.0, pitch_min: -60.0, pitch_max: 60.0,
                collision_radius: 0.2, return_speed: 4.0)),
        {nodes}
    ],
)"#
    )
}

/// The test enemy's props with `overrides` changed or added: a hurtbox of
/// radius 0.5 with 100 hp, 0.1 m and 3° a tick, rest 3, every move (the
/// spin too) 2 ticks of each part and of weight 0, a lunge of 0.1 m a
/// tick.
fn enemy_props(overrides: &[(&str, f64)]) -> String {
    let mut props: Vec<(String, f64)> = [
        ("lock_height", 1.0),
        ("hurt_radius", 0.5),
        ("hurt_height", 2.0),
        ("hp", 100.0),
        ("enemy_speed", 6.0),
        ("enemy_turn_speed", 180.0),
        ("enemy_rest_ticks", 3.0),
        ("lunge_distance", 0.2),
    ]
    .iter()
    .map(|&(n, v)| (n.to_owned(), v))
    .collect();
    for m in ["sweep", "cleave", "lunge", "combo", "spin"] {
        for (suffix, v) in [
            ("weight", 0.0),
            ("startup_ticks", 2.0),
            ("active_ticks", 2.0),
            ("recovery_ticks", 2.0),
        ] {
            props.push((format!("{m}_{suffix}"), v));
        }
    }
    for &(name, v) in overrides {
        match props.iter_mut().find(|(n, _)| n == name) {
            Some(p) => p.1 = v,
            None => props.push((name.to_owned(), v)),
        }
    }
    props
        .iter()
        .map(|(n, v)| format!("(\"{n}\", {v:?})"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The enemy "e" at `(x, 0, z)`, turned `degrees` about Y (0 faces -Z),
/// with [`enemy_props`] and an arm on its center line.
fn enemy(x: f32, z: f32, degrees: f32, overrides: &[(&str, f64)]) -> String {
    let props = enemy_props(overrides);
    format!(
        r#"(name: "e", position: ({x:?}, 0.0, {z:?}), rotation: (0.0, {degrees:?}, 0.0),
            props: [{props}],
            children: [(name: "arm", position: (0.0, 1.0, -1.0), scale: (0.1, 0.1, 2.0),
                mesh: Cube, color: (0.4, 0.3, 0.2))]),"#
    )
}

struct Run {
    game: Game,
    input: ActionState,
}

impl Run {
    fn new(player: &str, nodes: &str) -> Self {
        Self {
            game: Game::from_source(&scene(player, nodes)).unwrap(),
            input: ActionState::new(),
        }
    }

    /// One tick with exactly `actions` held and `axes` pushed.
    fn hold(&mut self, actions: &[&str], axes: &[(&str, f32)]) -> GameState {
        self.input.advance(TickInput {
            actions: actions.iter().map(|&a| a.to_owned()).collect(),
            axes: axes.iter().map(|&(a, v)| (a.to_owned(), v)).collect(),
        });
        self.game.tick(&self.input);
        self.game.state()
    }

    /// One tick without input.
    fn idle(&mut self) -> GameState {
        self.hold(&[], &[])
    }

    fn enemy(&self) -> &Enemy {
        &self.game.enemies[0]
    }

    fn position(&self) -> Vec3 {
        let scene = self.game.scene();
        scene.transform(scene.find("e").unwrap()).position
    }

    fn moves(&self) -> Vec<Event> {
        self.game
            .events()
            .iter()
            .filter(|e| matches!(e, Event::EnemyMove { .. }))
            .cloned()
            .collect()
    }
}

fn error_of(nodes: &str) -> String {
    match Game::from_source(&scene("", nodes)) {
        Ok(_) => panic!("the scene should be rejected: {nodes}"),
        Err(e) => e.to_string(),
    }
}

const TURN: f32 = 180.0_f32.to_radians() / 60.0;

// ---------------------------------------------------------------- choosing

#[test]
fn enemy_in_range_of_one_move_chooses_it_alone() {
    // At 3.5 m only the cleave (0..=4) is in range: the sweep ends at 3,
    // the combo at 2.8, the lunge starts at 4. No approach, which would
    // weigh 3 here.
    let w = [
        ("sweep_weight", 1.0),
        ("cleave_weight", 1.0),
        ("lunge_weight", 1.0),
        ("combo_weight", 1.0),
        ("approach_weight_per_meter", 0.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -3.5, 180.0, &w));
    let s = run.idle();
    assert_eq!(
        run.moves(),
        [Event::EnemyMove {
            by: "e".to_owned(),
            r#move: EnemyChoice::Cleave,
            distance: 3.5,
            candidates: vec![EnemyChoice::Cleave],
        }]
    );
    let e = &s.enemies[0];
    assert_eq!(e.state, EnemyCondition::Attack);
    assert_eq!((e.r#move, e.move_tick), (Some(MoveKind::Cleave), 1));
    assert_eq!(e.phase, Some(AttackPhase::Startup));
}

#[test]
fn two_candidates_split_the_seeds_first_number_modulo_their_weights() {
    // Sweep 3, cleave 2: r < 3 is the sweep, 3 or 4 the cleave. Seed 1's
    // first number ends in 5 (r = 0), seed 3's in 3 (r = 3).
    for (seed, chosen, first) in [
        (1.0, MoveKind::Sweep, 10_451_216_379_200_822_465_u64),
        (3.0, MoveKind::Cleave, 2_092_789_425_003_139_053),
    ] {
        let w = [
            ("sweep_weight", 3.0),
            ("cleave_weight", 2.0),
            ("enemy_seed", seed),
        ];
        let mut run = Run::new("", &enemy(0.0, -1.0, 180.0, &w));
        let s = run.idle();
        assert_eq!(first % 5, if chosen == MoveKind::Sweep { 0 } else { 3 });
        let Event::EnemyMove {
            r#move, candidates, ..
        } = &run.moves()[0]
        else {
            panic!("an enemy_move");
        };
        assert_eq!(
            (*r#move, candidates.clone()),
            (chosen.into(), vec![EnemyChoice::Sweep, EnemyChoice::Cleave])
        );
        // One number taken: the state moved on once.
        assert_eq!(
            s.enemies[0].rng,
            (seed as u64).wrapping_add(0x9E37_79B9_7F4A_7C15)
        );
    }
}

#[test]
fn same_seed_chooses_the_same_moves_and_zero_weight_never() {
    let w = [
        ("sweep_weight", 0.0),
        ("cleave_weight", 2.0),
        ("combo_weight", 1.0),
        ("cleave_damage", 1.0),
        ("combo_damage", 1.0),
        ("enemy_seed", 11.0),
    ];
    let chosen = || {
        let mut run = Run::new("", &enemy(0.0, -1.0, 180.0, &w));
        let mut moves = Vec::new();
        for _ in 0..300 {
            run.idle();
            for e in run.moves() {
                let Event::EnemyMove {
                    r#move, candidates, ..
                } = e
                else {
                    unreachable!()
                };
                assert_eq!(candidates, [EnemyChoice::Cleave, EnemyChoice::Combo]);
                moves.push(r#move);
            }
        }
        moves
    };
    let first = chosen();
    assert!(first.len() > 10, "{first:?}");
    assert!(first.contains(&EnemyChoice::Cleave) && first.contains(&EnemyChoice::Combo));
    assert_eq!(first, chosen());
}

#[test]
fn enemy_facing_away_turns_on_the_spot_until_the_face_angle_then_moves() {
    // Facing -Z with the character behind it: 180° off, 3° a tick; with a
    // face angle of 31° the move starts once it is 30° off, on tick 51.
    let w = [("sweep_weight", 1.0), ("enemy_face_angle", 31.0)];
    let mut run = Run::new("", &enemy(0.0, -1.5, 0.0, &w));
    let s = run.idle();
    assert_eq!(s.enemies[0].state, EnemyCondition::Idle);
    assert_eq!(s.enemies[0].facing.to_bits(), (-TURN).to_bits());
    for t in 2..=50 {
        let s = run.idle();
        assert_eq!(s.enemies[0].state, EnemyCondition::Idle, "tick {t}");
        assert!(run.moves().is_empty(), "tick {t}");
    }
    let s = run.idle();
    assert_eq!(s.enemies[0].state, EnemyCondition::Attack);
    assert_eq!(run.moves().len(), 1);
    // The node turns with it.
    let q = run
        .game
        .scene()
        .transform(run.game.scene().find("e").unwrap())
        .rotation;
    assert_eq!(q, Quat::from_rotation_y(s.enemies[0].facing));
}

#[test]
fn enemy_with_no_move_in_range_walks_straight_at_the_character() {
    // Nothing to choose, not even the approach: the walk of G4.1, choosing
    // again every tick, with no approach ticks counted.
    let w = [("approach_weight_per_meter", 0.0)];
    let mut run = Run::new("", &enemy(0.0, -5.0, 180.0, &w));
    let mut z = -5.0_f32;
    for t in 1..=20 {
        let s = run.idle();
        z += 0.1;
        assert_eq!(s.enemies[0].state, EnemyCondition::Chase, "tick {t}");
        assert_eq!(s.enemies[0].chase, 0, "tick {t}");
        assert_eq!(s.enemies[0].rng, 1, "tick {t}");
        assert_eq!(run.position(), Vec3::new(0.0, 0.0, z), "tick {t}");
        assert!(run.moves().is_empty(), "tick {t}");
    }
}

#[test]
fn character_on_the_enemys_spot_is_pushed_off_along_plus_x() {
    // Step 4 pushes the character out before the enemy acts, so the enemy
    // never sees it on its own spot.
    let mut run = Run::new("", &enemy(0.0, 0.0, 90.0, &[("enemy_aggro_range", 0.1)]));
    let s = run.idle();
    assert_eq!(s.player.position, Vec3::new(0.35 + 0.5, 0.0, 0.0));
    assert_eq!(run.position(), Vec3::ZERO);
}

// ---------------------------------------------------------------- timeline

#[test]
fn sweep_runs_its_phases_then_rests_turning_then_chooses_again() {
    let mut run = Run::new("", &enemy(0.0, -1.0, 180.0, &[("sweep_weight", 1.0)]));
    let expect = [
        (EnemyCondition::Attack, 1, Some(AttackPhase::Startup), 0),
        (EnemyCondition::Attack, 2, Some(AttackPhase::Startup), 0),
        (EnemyCondition::Attack, 3, Some(AttackPhase::Active), 0),
        (EnemyCondition::Attack, 4, Some(AttackPhase::Active), 0),
        (EnemyCondition::Attack, 5, Some(AttackPhase::Recovery), 0),
        // The last tick of the move starts the rest.
        (EnemyCondition::Rest, 0, None, 3),
        (EnemyCondition::Rest, 0, None, 2),
        (EnemyCondition::Rest, 0, None, 1),
        (EnemyCondition::Idle, 0, None, 0),
        (EnemyCondition::Attack, 1, Some(AttackPhase::Startup), 0),
    ];
    for (t, (state, move_tick, phase, rest)) in expect.into_iter().enumerate() {
        let s = run.idle();
        let e = &s.enemies[0];
        assert_eq!(
            (e.state, e.move_tick, e.phase, e.rest, e.hit),
            (state, move_tick, phase, rest, 0),
            "tick {}",
            t + 1
        );
        assert_eq!(e.r#move.is_some(), state == EnemyCondition::Attack);
    }
}

#[test]
fn enemy_without_rest_ticks_is_idle_right_after_its_move() {
    let w = [("sweep_weight", 1.0), ("enemy_rest_ticks", 0.0)];
    let mut run = Run::new("", &enemy(0.0, -1.0, 180.0, &w));
    for _ in 0..5 {
        run.idle();
    }
    let s = run.idle();
    assert_eq!(
        (s.enemies[0].state, s.enemies[0].rest),
        (EnemyCondition::Idle, 0)
    );
    let s = run.idle();
    assert_eq!(s.enemies[0].move_tick, 1);
}

#[test]
fn resting_enemy_turns_to_the_character() {
    // The character steps right during the move; the rest turns to it.
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_track_speed", 0.0),
        ("sweep_damage", 1.0),
        ("sweep_impact", 0.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -2.0, 180.0, &w));
    for _ in 0..5 {
        run.hold(&[], &[(MOVE_X, 1.0)]);
    }
    let before = run.idle().enemies[0].facing;
    let s = run.idle();
    assert_eq!(s.enemies[0].state, EnemyCondition::Rest);
    let after = s.enemies[0].facing;
    let to = bearing(run.position(), s.player.position);
    assert!(after != before && wrap(to - after).abs() < wrap(to - before).abs());
}

/// The player's hits by tick, among the first `ticks`.
fn hit_ticks(run: &mut Run, ticks: u64) -> Vec<(u64, HitAttack)> {
    let mut hits = Vec::new();
    for t in 1..=ticks {
        run.idle();
        for e in run.game.events() {
            if let Event::Hit { target, attack, .. } = e
                && target == "player"
            {
                hits.push((t, *attack));
            }
        }
    }
    hits
}

#[test]
fn combo_hits_once_a_cut_and_sweeps_its_even_cut_left_to_right() {
    // The character 45° to the enemy's left, 1 m away: 3 cuts of 2 ticks,
    // each 90° wide; startup 2, gaps of 1, recovery 2. A cut right to left
    // reaches it on its second tick, the reversed second cut on its first:
    // ticks 4, 6 and 10.
    let w = [
        ("combo_weight", 1.0),
        ("combo_arc", 180.0),
        ("combo_reach", 3.0),
        ("combo_track_speed", 0.0),
        ("combo_gap_ticks", 1.0),
        ("combo_damage", 1.0),
        ("enemy_face_angle", 180.0),
    ];
    let mut run = Run::new("", &enemy(-1.0, 0.0, -135.0, &w));
    let mut cuts = Vec::new();
    let mut arm = Vec::new();
    let mut hits = Vec::new();
    for t in 1..=12_u64 {
        let s = run.idle();
        cuts.push(s.enemies[0].hit);
        let scene = run.game.scene();
        arm.push(scene.transform(scene.find("e/arm").unwrap()).rotation);
        for e in run.game.events() {
            if let Event::Hit { attack, .. } = e {
                hits.push((t, *attack));
            }
        }
    }
    // The last tick ends the move: no cut any more.
    assert_eq!(cuts, [1, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 0]);
    assert_eq!(
        hits,
        [
            (4, HitAttack::Combo),
            (6, HitAttack::Combo),
            (10, HitAttack::Combo)
        ]
    );
    assert_eq!(run.game.state().player.hits, 3);
    // The arm: at the right end before the odd cuts, the left before the
    // even one; while active at the end of the slice swept.
    let half = 90.0_f32.to_radians();
    for (t, angle) in [
        (1, -half),
        (3, 0.0),
        (4, half),
        (5, half),
        (6, 0.0),
        (7, -half),
        (8, -half),
        (11, 0.0),
    ] {
        let expected = Quat::from_rotation_y(angle);
        assert!(
            arm[t - 1].abs_diff_eq(expected, 1e-6),
            "tick {t}: {:?}",
            arm[t - 1]
        );
    }
}

#[test]
fn lunge_charges_its_distance_evenly_over_its_active_ticks() {
    // 1 m over 10 active ticks, straight along -Z (facing exactly 0). No
    // approach, which would weigh 8 at 6 m.
    let w = [
        ("lunge_weight", 1.0),
        ("lunge_active_ticks", 10.0),
        ("lunge_distance", 1.0),
        ("lunge_track_speed", 0.0),
        ("approach_weight_per_meter", 0.0),
    ];
    let mut run = Run::new("", &enemy(0.0, 6.0, 0.0, &w));
    let mut z = 6.0_f32;
    for t in 1..=14 {
        run.idle();
        if (3..=12).contains(&t) {
            z -= 0.1;
        }
        assert_eq!(run.position(), Vec3::new(0.0, 0.0, z), "tick {t}");
    }
}

#[test]
fn lunge_into_a_pillar_corner_slides_off_it() {
    let w = [
        ("lunge_weight", 1.0),
        ("lunge_active_ticks", 10.0),
        ("lunge_distance", 3.0),
        ("lunge_track_speed", 0.0),
        ("approach_weight_per_meter", 0.0),
    ];
    // The pillar's left face at x = 0.2, its near face at z = 3.8.
    let pillar = r#"(name: "pillar", position: (0.7, 1.0, 3.5), scale: (1.0, 2.0, 0.6),
        collider: Box),"#;
    let mut run = Run::new("", &format!("{}{pillar}", enemy(0.0, 6.0, 0.0, &w)));
    let mut x = 0.0_f32;
    for t in 1..=12 {
        run.idle();
        let p = run.position();
        // Never into the pillar: the circle stays outside the rectangle
        // x 0.2..1.2, z 3.2..3.8.
        let near = Vec2::new(p.x.clamp(0.2, 1.2), p.z.clamp(3.2, 3.8));
        assert!(
            near.distance(Vec2::new(p.x, p.z)) >= 0.5 - 1e-5,
            "tick {t}: {p}"
        );
        // Only ever off to the left.
        assert!(p.x <= x, "tick {t}: {p}");
        x = p.x;
    }
    // Slid past the corner: pushed off sideways, and on along -Z.
    let p = run.position();
    assert!(p.x < 0.0 && p.z < 6.0 - 1.5, "{p}");
}

#[test]
fn startup_turns_at_track_speed_and_the_swing_and_recovery_do_not_turn() {
    // 60° a second: 1° a tick while the character strafes right.
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_startup_ticks", 20.0),
        ("sweep_track_speed", 60.0),
        ("sweep_damage", 1.0),
        ("sweep_impact", 0.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -2.0, 180.0, &w));
    let track = 60.0_f32.to_radians() / 60.0;
    let mut facing = run.enemy().facing;
    for t in 1..=24 {
        let s = run.hold(&[], &[(MOVE_X, 1.0)]);
        let turned = wrap(s.enemies[0].facing - facing).abs();
        if t <= 20 {
            assert!(turned > 0.0 && turned <= track + 1e-6, "tick {t}: {turned}");
        } else {
            assert_eq!(turned, 0.0, "tick {t}");
        }
        facing = s.enemies[0].facing;
    }
}

// --------------------------------------------------------------------- hits

#[test]
fn sweep_touching_on_both_active_ticks_hits_once_by_its_name() {
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_arc", 360.0),
        ("sweep_reach", 10.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -1.0, 180.0, &w));
    assert_eq!(hit_ticks(&mut run, 6), [(3, HitAttack::Sweep)]);
    let s = run.game.state();
    assert_eq!(s.player.hits, 1);
    assert_eq!(s.player.last_hit.unwrap().by, "e");
    // Sweep damage 25 by default.
    assert_eq!(s.player.health.hp, 75.0);
}

#[test]
fn backstep_through_the_active_ticks_dodges_them() {
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_arc", 360.0),
        ("sweep_reach", 10.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -1.5, 180.0, &w));
    // Pressed on tick 1, tapped on tick 2: a backstep from tick 2,
    // invulnerable on its ticks 1-8.
    run.hold(&[ROLL], &[]);
    let mut dodged = Vec::new();
    for t in 2..=6 {
        run.idle();
        for e in run.game.events() {
            if let Event::Dodged {
                by,
                action,
                action_tick,
            } = e
            {
                dodged.push((t, by.clone(), *action, *action_tick));
            }
        }
    }
    assert_eq!(
        dodged,
        [
            (3, "e".to_owned(), Action::Backstep, 2),
            (4, "e".to_owned(), Action::Backstep, 3)
        ]
    );
    assert_eq!(run.game.state().player.hits, 0);
}

// ------------------------------------------------------------ interruptions

/// Presses the light attack on tick 1 and runs `ticks` ticks; returns the
/// tick of the character's first hit on "e".
fn attack_enemy(run: &mut Run, ticks: u64) -> u64 {
    let mut first = None;
    for t in 1..=ticks {
        let actions: &[&str] = if t == 1 { &[ATTACK] } else { &[] };
        run.hold(actions, &[]);
        let hit = run
            .game
            .events()
            .iter()
            .any(|e| matches!(e, Event::Hit { target, .. } if target == "e"));
        if hit && first.is_none() {
            first = Some(t);
        }
    }
    first.expect("the attack hits the enemy")
}

#[test]
fn staggered_enemy_drops_its_move_and_starts_idle_again() {
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_startup_ticks", 60.0),
        ("stagger_ticks", 5.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    run.hold(&[ATTACK], &[]);
    let mut t = 1;
    let hit = loop {
        t += 1;
        let s = run.idle();
        if s.hurtboxes[0].health.unwrap().state == Condition::Stagger {
            break t;
        }
        assert!(t < 40, "never staggered");
    };
    let s = run.game.state();
    let e = &s.enemies[0];
    assert_eq!(
        (e.state, e.r#move, e.move_tick, e.phase),
        (EnemyCondition::Stagger, None, 0, None)
    );
    // Staggered for its 5 ticks, then idle and at once into a new move.
    for _ in 1..5 {
        assert_eq!(run.idle().enemies[0].state, EnemyCondition::Stagger);
    }
    let s = run.idle();
    assert_eq!(
        s.hurtboxes[0].health.unwrap().state,
        Condition::Ok,
        "hit on {hit}"
    );
    assert_eq!(
        (s.enemies[0].state, s.enemies[0].move_tick),
        (EnemyCondition::Attack, 1)
    );
}

#[test]
fn killed_enemy_lies_dead_and_revives_idle_with_its_random_numbers_kept() {
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_startup_ticks", 60.0),
        ("hp", 10.0),
        ("revive_ticks", 10.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    let rng = run.idle().enemies[0].rng;
    let mut t = 1;
    run.hold(&[ATTACK], &[]);
    loop {
        t += 1;
        if run.idle().enemies[0].state == EnemyCondition::Dead {
            break;
        }
        assert!(t < 40, "never killed");
    }
    // Only the move chosen on tick 1 took a number.
    assert_eq!(run.game.state().enemies[0].rng, rng);
    for _ in 1..10 {
        let s = run.idle();
        assert_eq!(s.enemies[0].state, EnemyCondition::Dead);
    }
    let s = run.idle();
    assert_eq!(s.hurtboxes[0].health.unwrap().state, Condition::Ok);
    assert_eq!(s.enemies[0].state, EnemyCondition::Attack);
    assert_eq!(s.enemies[0].rng, rng.wrapping_add(0x9E37_79B9_7F4A_7C15));
}

#[test]
fn character_leaving_aggro_range_lets_the_running_move_finish_then_idles() {
    // Chosen on tick 1 at 2.9625 m; the character runs off at 0.0625 m a
    // tick, out of the 3 m aggro range from tick 2. No approach, which
    // would weigh 1 there.
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_startup_ticks", 10.0),
        ("enemy_aggro_range", 3.0),
        ("approach_weight_per_meter", 0.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -2.9, 180.0, &w));
    let back = [(MOVE_Y, -1.0)];
    for t in 1..=13 {
        let s = run.hold(&[], &back);
        assert_eq!(s.enemies[0].state, EnemyCondition::Attack, "tick {t}");
    }
    // Its last tick (14) starts the rest, which the next tick drops.
    assert_eq!(run.hold(&[], &back).enemies[0].state, EnemyCondition::Rest);
    for t in 15..=30 {
        let s = run.hold(&[], &back);
        assert_eq!(s.enemies[0].state, EnemyCondition::Idle, "tick {t}");
        assert!(run.moves().is_empty());
    }
}

#[test]
fn enemy_finishes_its_move_on_a_dead_character_and_waits_for_the_respawn() {
    let w = [("sweep_weight", 1.0), ("sweep_arc", 360.0)];
    let mut run = Run::new(
        r#"("hp", 1), ("respawn_ticks", 30)"#,
        &enemy(0.0, -1.0, 180.0, &w),
    );
    let mut states = Vec::new();
    let mut respawn = None;
    let mut chosen = Vec::new();
    for t in 1..=60_u64 {
        let s = run.idle();
        states.push(s.enemies[0].state);
        if run
            .game
            .events()
            .iter()
            .any(|e| matches!(e, Event::Respawn { .. }))
        {
            respawn = Some(t);
        }
        if !run.moves().is_empty() {
            chosen.push(t);
        }
    }
    // Killed on tick 3; the move ends on tick 6 (starting a rest), then it
    // idles until the respawn, and chooses again that very tick.
    let respawn = respawn.expect("the character respawns");
    assert_eq!(respawn, 3 + 30);
    assert_eq!(chosen, [1, respawn]);
    assert_eq!(states[..6].last(), Some(&EnemyCondition::Rest));
    assert!(
        states[6..respawn as usize - 1]
            .iter()
            .all(|&s| s == EnemyCondition::Idle)
    );
}

// ---------------------------------------------------------------- collision

#[test]
fn character_walking_into_an_enemy_stops_at_the_two_radii() {
    // The enemy stands: the character is always out of its aggro range.
    let mut run = Run::new("", &enemy(0.0, -3.0, 180.0, &[("enemy_aggro_range", 0.1)]));
    let mut s = run.idle();
    for _ in 0..60 {
        s = run.hold(&[], &[(MOVE_Y, 1.0)]);
    }
    assert_eq!(s.player.position.x, 0.0);
    assert!(
        (s.player.position.z - (-3.0 + 0.85)).abs() < 1e-5,
        "{}",
        s.player.position
    );
    assert_eq!(run.position(), Vec3::new(0.0, 0.0, -3.0));
}

#[test]
fn enemy_walking_up_to_the_character_stops_at_the_two_radii() {
    let mut run = Run::new("", &enemy(0.0, -3.0, 180.0, &[]));
    for _ in 0..40 {
        run.idle();
    }
    let p = run.position();
    assert_eq!(p.x, 0.0);
    assert!((p.z + 0.85).abs() < 1e-5, "{p}");
    let player = run.game.state().player.position;
    assert!(player.length() < 1e-5, "{player}");
}

#[test]
fn dead_enemy_no_longer_blocks_the_character() {
    let w = [("hp", 1.0), ("enemy_aggro_range", 0.1)];
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    attack_enemy(&mut run, 40);
    assert_eq!(run.game.state().enemies[0].state, EnemyCondition::Dead);
    let mut s = run.idle();
    for _ in 0..60 {
        s = run.hold(&[], &[(MOVE_Y, 1.0)]);
    }
    assert!(s.player.position.z < -3.0, "{}", s.player.position);
}

// ---------------------------------------------------------------- the node

#[test]
fn staggered_enemy_tips_over_from_its_current_facing() {
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_startup_ticks", 60.0),
        ("stagger_ticks", 20.0),
        ("enemy_face_angle", 180.0),
    ];
    // Facing 45° off the character, so the base is not the scene's.
    let mut run = Run::new("", &enemy(0.0, -1.2, 135.0, &w));
    attack_enemy(&mut run, 30);
    let s = run.game.state();
    assert_eq!(s.enemies[0].state, EnemyCondition::Stagger);
    let h = &run.game.hurtboxes[run.enemy().hurtbox];
    let health = h.health.unwrap();
    let expected = pose(
        Condition::Stagger,
        health.fall,
        Quat::from_rotation_y(s.enemies[0].facing),
    );
    let scene = run.game.scene();
    assert_eq!(scene.transform(scene.find("e").unwrap()).rotation, expected);
}

#[test]
fn arm_waits_at_the_sectors_right_end_in_warning_color_during_startup() {
    let w = [("sweep_weight", 1.0), ("sweep_startup_ticks", 5.0)];
    let mut run = Run::new("", &enemy(0.0, -2.0, 180.0, &w));
    run.idle();
    let scene = run.game.scene();
    let arm = scene.find("e/arm").unwrap();
    let half = 160.0_f32.to_radians() * 0.5;
    let q = Quat::from_rotation_y(-half);
    assert_eq!(scene.transform(arm).rotation, q);
    assert_eq!(scene.transform(arm).position, q * Vec3::new(0.0, 1.0, -1.0));
    assert_eq!(scene.node(arm).mesh.unwrap().color, WARN_COLOR);
}

#[test]
fn debug_swings_draw_the_enemys_sector_orange_then_its_slice_red() {
    let w = [("sweep_weight", 1.0), ("sweep_arc", 90.0)];
    let mut run = Run::new("", &enemy(0.0, -5.0, 180.0, &w));
    // Far away: walking, nothing to draw.
    run.idle();
    let colors = |run: &Run| {
        let mut draw = DebugDraw::new();
        run.game.debug_draw(&["swings"], &mut draw);
        let mut colors: Vec<Vec3> = draw.lines().iter().map(|l| l.color).collect();
        colors.dedup();
        colors
    };
    assert!(colors(&run).is_empty());
    let w = [("sweep_weight", 1.0), ("sweep_arc", 90.0)];
    let mut run = Run::new("", &enemy(0.0, -1.0, 180.0, &w));
    run.idle();
    assert_eq!(colors(&run), [DEBUG_STARTUP]);
    run.idle();
    run.idle();
    assert_eq!(colors(&run), [DEBUG_ACTIVE, DEBUG_SLICE]);
}

// --------------------------------------------------------------- events

#[test]
fn enemy_move_comes_first_on_its_tick_before_a_swingers_hit() {
    // The sweep (3 ticks) ends on tick 3: the next move starts on tick 4,
    // when the swinger's round of 5 is active.
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_startup_ticks", 1.0),
        ("sweep_active_ticks", 1.0),
        ("sweep_recovery_ticks", 1.0),
        ("sweep_damage", 1.0),
        ("enemy_rest_ticks", 0.0),
    ];
    let swinger = r#"(name: "s", position: (3.0, 0.0, 0.0), props: [("swing_damage", 1),
        ("swing_startup_ticks", 3), ("swing_active_ticks", 1), ("swing_recovery_ticks", 1),
        ("swing_arc", 360), ("swing_reach", 10)]),"#;
    let mut run = Run::new("", &format!("{}{swinger}", enemy(0.0, -1.0, 180.0, &w)));
    for _ in 0..3 {
        run.idle();
    }
    run.idle();
    let events = run.game.events();
    assert_eq!(
        events[0],
        Event::EnemyMove {
            by: "e".to_owned(),
            r#move: EnemyChoice::Sweep,
            distance: 1.0,
            candidates: vec![EnemyChoice::Sweep],
        }
    );
    assert!(
        matches!(&events[1], Event::Hit { by, .. } if by == "s"),
        "{events:?}"
    );
}

// --------------------------------------------------------------- checking

#[test]
fn enemy_nodes_are_checked_in_order_with_exact_messages() {
    let full = r#"("lock_height", 1), ("hurt_radius", 0.7), ("hurt_height", 2), ("hp", 10),
        ("enemy_speed", 6)"#;
    let node = |props: &str| format!(r#"(name: "t", props: [{props}]),"#);
    for (props, message) in [
        (
            r#"("sweep_damage", 3)"#.to_owned(),
            "the node \"t\": prop \"sweep_damage\" needs enemy_speed",
        ),
        (
            format!(r#"{full}, ("enemy_face_angle", 181)"#),
            "the node \"t\": prop \"enemy_face_angle\" must be greater than 0 and at most 180, \
             got 181",
        ),
        (
            format!(r#"{full}, ("combo_min_range", 3), ("lunge_min_range", 10)"#),
            "the node \"t\": needs lunge_min_range <= lunge_max_range",
        ),
        (
            format!(r#"{full}, ("enemy_swing_low", 2.4)"#),
            "the node \"t\": needs enemy_swing_low < enemy_swing_high",
        ),
        // A band error comes before a missing hurtbox.
        (
            r#"("enemy_speed", 6), ("enemy_swing_low", 3)"#.to_owned(),
            "the node \"t\": needs enemy_swing_low < enemy_swing_high",
        ),
        (
            r#"("enemy_speed", 6), ("lock_height", 1)"#.to_owned(),
            "the enemy \"t\" needs hurt_radius, hurt_height, hp and lock_height",
        ),
        (
            format!(r#"{full}, ("swing_damage", 3)"#),
            "the enemy \"t\" cannot also be a swinger",
        ),
        // Both too long: the walk is reported.
        (
            r#"("lock_height", 1), ("hurt_radius", 0.7), ("hurt_height", 2), ("hp", 10),
                ("enemy_speed", 60), ("lunge_distance", 50)"#
                .to_owned(),
            "the enemy \"t\": enemy moves 1 m per tick, which must be less than hurt_radius 0.7",
        ),
        (
            format!(r#"{full}, ("lunge_distance", 50)"#),
            "the enemy \"t\": lunge moves 5 m per tick, which must be less than hurt_radius 0.7",
        ),
    ] {
        assert_eq!(error_of(&node(&props)), message, "{props}");
    }
    // Minimum ranges up to the maximum are fine, as are the other props.
    assert!(
        Game::from_source(&scene(
            "",
            &node(&format!(
                r#"{full}, ("sweep_min_range", 3), ("combo_hits", 1), ("combo_gap_ticks", 0)"#
            ))
        ))
        .is_ok()
    );
    let with_collider =
        format!(r#"(name: "t", props: [{full}], children: [(name: "block", collider: Box)]),"#);
    assert_eq!(
        error_of(&with_collider),
        "the enemy \"t\" must not have a collider in its subtree: colliders are static"
    );
    // Its arm needs a mesh, like a swinger's.
    let meshless_arm = format!(r#"(name: "t", props: [{full}], children: [(name: "arm")]),"#);
    assert_eq!(
        error_of(&meshless_arm),
        "the node \"t\": its arm needs a mesh"
    );
}

#[test]
fn nested_enemy_is_reported_as_a_nested_lock_target() {
    // An enemy must be a lock-on target, and those must be root nodes: that
    // check covers the enemy, which has no root check of its own.
    let full = r#"("lock_height", 1), ("hurt_radius", 0.7), ("hurt_height", 2), ("hp", 10),
        ("enemy_speed", 6)"#;
    let nested = format!(r#"(name: "g", children: [(name: "t", props: [{full}])]),"#);
    assert_eq!(
        error_of(&nested),
        "the lock target \"g/t\" must be a root node, and not the camera"
    );
}

// ---------------------------------------------------------------- approach

/// The candidates of the only `enemy_move` of the last tick.
fn chosen(run: &Run) -> (EnemyChoice, Vec<EnemyChoice>) {
    match &run.moves()[..] {
        [
            Event::EnemyMove {
                r#move, candidates, ..
            },
        ] => (*r#move, candidates.clone()),
        other => panic!("one enemy_move, not {other:?}"),
    }
}

#[test]
fn approach_is_no_choice_within_its_min_range() {
    // Exactly 2 m, approach_min_range: only the sweep (0..=3).
    let mut run = Run::new("", &enemy(0.0, -2.0, 180.0, &[("sweep_weight", 1.0)]));
    run.idle();
    assert_eq!(chosen(&run), (EnemyChoice::Sweep, vec![EnemyChoice::Sweep]));
}

#[test]
fn approach_weight_is_rounded_down_and_needs_one() {
    // 2 per meter beyond 2 m: floor(2 * (d - 2)) is 1 from 2.5 m on, the
    // min plus 1 / per meter. With the sweep's 1, the seed's first number
    // gives r = 1 of 2 and r = 2 of 3: the approach whenever it is there.
    for (d, weight) in [(2.49, 0), (2.5, 1), (2.51, 1), (2.99, 1), (3.0, 2)] {
        let mut run = Run::new("", &enemy(0.0, -d, 180.0, &[("sweep_weight", 1.0)]));
        run.idle();
        let want = if weight >= 1 {
            (
                EnemyChoice::Approach,
                vec![EnemyChoice::Sweep, EnemyChoice::Approach],
            )
        } else {
            (EnemyChoice::Sweep, vec![EnemyChoice::Sweep])
        };
        assert_eq!(chosen(&run), want, "{d} m");
        assert_eq!(run.enemy().params().approach_weight(d), weight, "{d} m");
    }
}

#[test]
fn approach_alone_is_chosen_facing_the_character_and_turned_to_otherwise() {
    // No move has weight: 5 m, only the approach (weight 6).
    let mut run = Run::new("", &enemy(0.0, -5.0, 180.0, &[]));
    let s = run.idle();
    assert_eq!(
        run.moves(),
        [Event::EnemyMove {
            by: "e".to_owned(),
            r#move: EnemyChoice::Approach,
            distance: 5.0,
            candidates: vec![EnemyChoice::Approach],
        }]
    );
    assert_eq!(
        (s.enemies[0].state, s.enemies[0].chase),
        (EnemyCondition::Chase, 29)
    );
    assert_eq!(s.enemies[0].r#move, None);
    assert_eq!(run.position(), Vec3::new(0.0, 0.0, -5.0 + 0.1));
    // Facing away, 180° off: it turns on the spot, idle, without choosing.
    let mut run = Run::new("", &enemy(0.0, -5.0, 0.0, &[]));
    for t in 1..=40 {
        let s = run.idle();
        assert_eq!(
            (s.enemies[0].state, s.enemies[0].chase),
            (EnemyCondition::Idle, 0),
            "tick {t}"
        );
        assert!(run.moves().is_empty(), "tick {t}");
        assert_eq!(run.position(), Vec3::new(0.0, 0.0, -5.0), "tick {t}");
    }
    let mut t = 40;
    while run.moves().is_empty() {
        t += 1;
        run.idle();
        assert!(t < 60, "never chose");
    }
    assert_eq!(
        chosen(&run),
        (EnemyChoice::Approach, vec![EnemyChoice::Approach])
    );
}

#[test]
fn approach_is_chosen_by_the_seeds_number_like_a_move() {
    // At 3 m: the sweep's 3 (0..=3), then the approach's 2. Seed 1's first
    // number modulo 5 is 0, the sweep; seed 3's is 3, past the sweep.
    for (seed, first, want) in [
        (1.0, 10_451_216_379_200_822_465_u64, EnemyChoice::Sweep),
        (3.0, 2_092_789_425_003_139_053, EnemyChoice::Approach),
    ] {
        assert_eq!(first % 5, if want == EnemyChoice::Sweep { 0 } else { 3 });
        let w = [("sweep_weight", 3.0), ("enemy_seed", seed)];
        let mut run = Run::new("", &enemy(0.0, -3.0, 180.0, &w));
        run.idle();
        assert_eq!(
            chosen(&run),
            (want, vec![EnemyChoice::Sweep, EnemyChoice::Approach])
        );
    }
}

#[test]
fn chosen_approach_walks_its_ticks_then_idles_and_chooses_again() {
    // 10 m, approach only, 5 ticks: a step on each, counting down; the
    // fifth ends it, and the sixth chooses again.
    let mut run = Run::new("", &enemy(0.0, -10.0, 180.0, &[("approach_ticks", 5.0)]));
    let mut z = -10.0_f32;
    let mut rng = SplitMix64(1);
    rng.next();
    for (t, state, chase) in [
        (1, EnemyCondition::Chase, 4),
        (2, EnemyCondition::Chase, 3),
        (3, EnemyCondition::Chase, 2),
        (4, EnemyCondition::Chase, 1),
        (5, EnemyCondition::Idle, 0),
    ] {
        let s = run.idle();
        z += 0.1;
        assert_eq!(
            (s.enemies[0].state, s.enemies[0].chase),
            (state, chase),
            "tick {t}"
        );
        assert_eq!(run.position(), Vec3::new(0.0, 0.0, z), "tick {t}");
        assert_eq!(run.moves().len(), usize::from(t == 1), "tick {t}");
        assert_eq!(s.enemies[0].rng, rng.0, "tick {t}");
    }
    let s = run.idle();
    rng.next();
    assert_eq!(chosen(&run).0, EnemyChoice::Approach);
    assert_eq!(
        (s.enemies[0].state, s.enemies[0].chase, s.enemies[0].rng),
        (EnemyCondition::Chase, 4, rng.0)
    );
}

#[test]
fn approach_ends_once_within_its_min_range_then_the_walk_goes_on() {
    // From 2.55 m (weight 1) at 0.1 m a tick: 2.45 .. 2.05 on ticks 1-5,
    // and 1.95 on tick 6, within 2 m: idle. Then nothing is left to
    // choose, so it walks on as in G4.1, with no count.
    let mut run = Run::new("", &enemy(0.0, -2.55, 180.0, &[]));
    for t in 1..=5 {
        let s = run.idle();
        assert_eq!(
            (s.enemies[0].state, s.enemies[0].chase),
            (EnemyCondition::Chase, 30 - t),
            "tick {t}"
        );
    }
    let s = run.idle();
    assert_eq!(
        (s.enemies[0].state, s.enemies[0].chase),
        (EnemyCondition::Idle, 0)
    );
    assert!((run.position().z + 1.95).abs() < 1e-5, "{}", run.position());
    let s = run.idle();
    assert_eq!(
        (s.enemies[0].state, s.enemies[0].chase),
        (EnemyCondition::Chase, 0)
    );
    assert!(run.moves().is_empty());
    assert!((run.position().z + 1.85).abs() < 1e-5, "{}", run.position());
}

#[test]
fn staggered_enemy_drops_its_approach() {
    // Approaching from 1.2 m (min 0: weight 2) for up to 60 ticks when the
    // character's attack staggers it.
    let w = [
        ("approach_min_range", 0.0),
        ("approach_ticks", 60.0),
        ("stagger_ticks", 5.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    let mut before = None;
    let mut s = run.hold(&[ATTACK], &[]);
    for t in 2..40 {
        if s.hurtboxes[0].health.unwrap().state == Condition::Stagger {
            break;
        }
        before = Some((s.enemies[0].state, s.enemies[0].chase));
        s = run.idle();
        assert!(t < 39, "never staggered");
    }
    let (state, chase) = before.expect("approaching before the hit");
    assert_eq!(state, EnemyCondition::Chase);
    assert!(chase > 0);
    assert_eq!(
        (s.enemies[0].state, s.enemies[0].chase),
        (EnemyCondition::Stagger, 0)
    );
}

#[test]
fn leaving_the_rest_for_a_dead_character_or_the_aggro_range_zeroes_it() {
    // A dead character: killed on tick 3, the sweep ends on tick 6 with a
    // rest of 3, and tick 7 idles with none left.
    let w = [("sweep_weight", 1.0), ("sweep_arc", 360.0)];
    let mut run = Run::new(r#"("hp", 1)"#, &enemy(0.0, -1.0, 180.0, &w));
    let states: Vec<(EnemyCondition, u32)> = (1..=7)
        .map(|_| {
            let s = run.idle();
            (s.enemies[0].state, s.enemies[0].rest)
        })
        .collect();
    assert_eq!(states[5], (EnemyCondition::Rest, 3));
    assert_eq!(states[6], (EnemyCondition::Idle, 0));
    // Out of the aggro range: the character runs off as in
    // character_leaving_aggro_range_lets_the_running_move_finish_then_idles.
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_startup_ticks", 10.0),
        ("enemy_aggro_range", 3.0),
        ("enemy_rest_ticks", 10.0),
        ("approach_weight_per_meter", 0.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -2.9, 180.0, &w));
    let back = [(MOVE_Y, -1.0)];
    let states: Vec<(EnemyCondition, u32)> = (1..=15)
        .map(|_| {
            let s = run.hold(&[], &back);
            (s.enemies[0].state, s.enemies[0].rest)
        })
        .collect();
    assert_eq!(states[13], (EnemyCondition::Rest, 10));
    assert_eq!(states[14], (EnemyCondition::Idle, 0));
}

#[test]
fn staggered_resting_enemy_has_no_rest_left() {
    // The sweep misses (0.1 m reach) and ends on tick 6 with a rest of 60;
    // the character's attack then staggers it.
    let w = [
        ("sweep_weight", 1.0),
        ("sweep_reach", 0.1),
        ("enemy_rest_ticks", 60.0),
        ("stagger_ticks", 5.0),
    ];
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    for _ in 1..=6 {
        run.idle();
    }
    let s = run.game.state();
    assert_eq!(
        (s.enemies[0].state, s.enemies[0].rest),
        (EnemyCondition::Rest, 60)
    );
    let mut s = run.hold(&[ATTACK], &[]);
    for t in 8..40 {
        if s.hurtboxes[0].health.unwrap().state == Condition::Stagger {
            break;
        }
        assert_eq!(s.enemies[0].state, EnemyCondition::Rest, "tick {t}");
        s = run.idle();
        assert!(t < 39, "never staggered");
    }
    assert_eq!(
        (s.enemies[0].state, s.enemies[0].rest),
        (EnemyCondition::Stagger, 0)
    );
}

// ---------------------------------------------------------------- phase 2

/// The test enemy with a phase 2 at half of 40 hp: one light attack (20)
/// takes it there.
const PHASE2: [(&str, f64); 2] = [("hp", 40.0), ("phase2_at", 0.5)];

/// A roar over at once, its blast reaching nobody.
const SHORT_ROAR: [(&str, f64); 3] = [
    ("roar_ticks", 1.0),
    ("roar_blast_tick", 1.0),
    ("roar_reach", 0.01),
];

/// `PHASE2`, then `more`.
fn phase2(more: &[(&'static str, f64)]) -> Vec<(&'static str, f64)> {
    PHASE2.iter().chain(more).copied().collect()
}

/// The last tick's `hit` events on the enemy and `stage` events, as
/// `type target-or-by result`.
fn hits_and_stages(run: &Run) -> Vec<String> {
    run.game
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::Hit { target, result, .. } if target == "e" => {
                Some(format!("hit {target} {result:?}"))
            }
            Event::Stage { by, stage } => Some(format!("stage {by} {stage}")),
            _ => None,
        })
        .collect()
}

/// Attacks the enemy on tick 1 and runs until the tick it is hit; its
/// events.
fn hit_once(run: &mut Run) -> Vec<String> {
    run.hold(&[ATTACK], &[]);
    for _ in 0..40 {
        let events = hits_and_stages(run);
        if !events.is_empty() {
            return events;
        }
        run.idle();
    }
    panic!("never hit");
}

#[test]
fn hit_reaching_the_threshold_enters_phase_2_with_a_roar_for_its_stagger() {
    // 40 hp, a threshold of 20: a light attack (20) leaves exactly 20;
    // from 30 it leaves 10. Poise 0: the hit would stagger it.
    for hp in [40.0, 30.0] {
        let mut run = Run::new(
            "",
            &enemy(0.0, -1.2, 180.0, &[("hp", hp), ("phase2_at", 0.5)]),
        );
        assert_eq!(
            hit_once(&mut run),
            ["hit e Stagger", "stage e 2"],
            "{hp} hp"
        );
        let s = run.game.state();
        let health = s.hurtboxes[0].health.unwrap();
        assert_eq!(health.hp, hp as f32 - 20.0);
        assert_eq!(
            (
                health.state,
                health.state_ticks,
                health.impact,
                health.impact_wait
            ),
            (Condition::Ok, 0, 0.0, 0)
        );
        let e = &s.enemies[0];
        // Step 5c of the same tick: the roar's first tick.
        assert_eq!(
            (e.stage, e.state, e.move_tick, e.phase),
            (2, EnemyCondition::Roar, 1, Some(AttackPhase::Startup))
        );
    }
    // From 50 the hit leaves 30, above the threshold of 25.
    let mut run = Run::new(
        "",
        &enemy(0.0, -1.2, 180.0, &[("hp", 50.0), ("phase2_at", 0.5)]),
    );
    assert_eq!(hit_once(&mut run), ["hit e Stagger"]);
    assert_eq!(run.game.state().enemies[0].stage, 1);
}

#[test]
fn enemy_without_phase2_at_stays_in_phase_1() {
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &[("hp", 40.0)]));
    assert_eq!(hit_once(&mut run), ["hit e Stagger"]);
    let e = &run.game.state().enemies[0];
    assert_eq!((e.stage, e.state), (1, EnemyCondition::Stagger));
}

#[test]
fn killing_hit_skips_phase_2() {
    let mut run = Run::new(
        "",
        &enemy(0.0, -1.2, 180.0, &[("hp", 20.0), ("phase2_at", 0.5)]),
    );
    assert_eq!(hit_once(&mut run), ["hit e Death"]);
    let e = &run.game.state().enemies[0];
    assert_eq!((e.stage, e.state), (1, EnemyCondition::Dead));
}

#[test]
fn entering_phase_2_breaks_off_the_move() {
    let w = phase2(&[("sweep_weight", 1.0), ("sweep_startup_ticks", 60.0)]);
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    run.idle();
    assert_eq!(run.game.state().enemies[0].r#move, Some(MoveKind::Sweep));
    hit_once(&mut run);
    let e = &run.game.state().enemies[0];
    assert_eq!(
        (e.state, e.r#move, e.hit, e.rest, e.chase),
        (EnemyCondition::Roar, None, 0, 0, 0)
    );
}

#[test]
fn roar_blasts_all_around_on_its_tick_then_idles() {
    // A roar of 6 ticks with the blast on the third; the character stands
    // behind the enemy's facing, 1.2 m off, within the 4 m reach.
    let w = phase2(&[
        ("roar_ticks", 6.0),
        ("roar_blast_tick", 3.0),
        ("roar_damage", 7.0),
        ("roar_impact", 0.0),
        ("enemy_face_angle", 180.0),
    ]);
    let mut run = Run::new(r#"("hp", 100)"#, &enemy(0.0, -1.2, 0.0, &w));
    hit_once(&mut run);
    // The hit's tick was the roar's first.
    let mut seen = Vec::new();
    for _ in 2..=6 {
        let s = run.idle();
        let e = &s.enemies[0];
        let hit = run.game.events().iter().any(|ev| {
            matches!(ev, Event::Hit { by, attack: HitAttack::Roar, damage: Some(d), .. }
                if by == "e" && *d == 7.0)
        });
        seen.push((e.state, e.move_tick, e.phase, hit, s.player.health.hp));
    }
    use AttackPhase::{Active, Recovery, Startup};
    use EnemyCondition::{Idle, Roar};
    assert_eq!(
        seen,
        [
            (Roar, 2, Some(Startup), false, 100.0),
            (Roar, 3, Some(Active), true, 93.0),
            (Roar, 4, Some(Recovery), false, 93.0),
            (Roar, 5, Some(Recovery), false, 93.0),
            (Idle, 0, None, false, 93.0),
        ]
    );
}

#[test]
fn roaring_enemy_is_neither_hit_nor_struck() {
    // A long roar; attacks on it only swing through.
    let w = phase2(&[("roar_ticks", 200.0), ("roar_blast_tick", 200.0)]);
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    hit_once(&mut run);
    // The swing that hit it has struck it; wait for its end.
    while run.game.state().player.action != Action::Idle {
        run.idle();
    }
    let before = run.game.state().hurtboxes[0].clone();
    for t in 0..60 {
        let actions: &[&str] = if t % 20 == 0 { &[ATTACK] } else { &[] };
        run.hold(actions, &[]);
        assert!(hits_and_stages(&run).is_empty(), "tick {t}");
        assert!(run.game.struck.is_empty(), "tick {t}");
    }
    let after = run.game.state().hurtboxes[0].clone();
    assert_eq!(after.hits, before.hits);
    assert_eq!(after.health.unwrap().hp, 20.0);
    assert_eq!(run.game.state().enemies[0].state, EnemyCondition::Roar);
}

#[test]
fn backstep_through_the_blast_dodges_it() {
    // The hit's tick is the roar's first, so tick t after it is the
    // roar's t. The attack ends on 19; a backstep pressed on 25 is
    // invulnerable on 26-33, around the blast on 28.
    let w = phase2(&[("roar_blast_tick", 28.0)]);
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    hit_once(&mut run);
    let mut dodged = Vec::new();
    for t in 2..=40 {
        let actions: &[&str] = if t == 25 { &[ROLL] } else { &[] };
        let s = run.hold(actions, &[]);
        for e in run.game.events() {
            match e {
                Event::Dodged { by, .. } => dodged.push((t, by.clone(), s.player.invulnerable)),
                Event::Hit { by, .. } if by == "e" => panic!("hit on tick {t}"),
                _ => {}
            }
        }
    }
    assert_eq!(dodged, [(28, "e".to_owned(), true)]);
}

#[test]
fn blast_misses_beyond_its_reach() {
    // The enemy walked up to the character before the hit: 0.85 m apart,
    // the character's circle (0.35) 0.4 m off the center. A reach of 0.45
    // misses it by 0.05 m, one of 0.55 hits it.
    for (reach, hp) in [(0.45, 100.0), (0.55, 70.0)] {
        let w = phase2(&[
            ("roar_ticks", 3.0),
            ("roar_blast_tick", 2.0),
            ("roar_reach", reach),
        ]);
        let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
        hit_once(&mut run);
        assert!((run.position().z + 0.85).abs() < 1e-5, "{}", run.position());
        for _ in 0..3 {
            run.idle();
        }
        assert_eq!(run.game.state().player.health.hp, hp, "reach {reach}");
    }
}

/// The enemy hit into phase 2 by the attack on tick 1, its roar over at
/// once, then run until it is `Attack`ing; the phases of that move.
fn phase2_move(run: &mut Run) -> Vec<(AttackPhase, u32)> {
    hit_once(run);
    let mut t = 0;
    while run.game.state().enemies[0].state != EnemyCondition::Attack {
        run.idle();
        t += 1;
        assert!(t < 100, "no move");
    }
    let mut phases = Vec::new();
    loop {
        let s = run.game.state();
        let e = &s.enemies[0];
        if e.state != EnemyCondition::Attack {
            return phases;
        }
        assert_eq!(e.stage, 2);
        phases.push((e.phase.unwrap(), e.hit));
        run.idle();
    }
}

#[test]
fn phase_2_moves_start_sooner() {
    // A sweep of startup 20: 15 in phase 2 (scale 0.75); active and
    // recovery kept at 2 each, the last tick already resting.
    let mut w = phase2(&SHORT_ROAR);
    w.extend([
        ("sweep_weight", 1.0),
        ("sweep_startup_ticks", 20.0),
        ("sweep_damage", 1.0),
        ("sweep_impact", 0.0),
    ]);
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    let phases = phase2_move(&mut run);
    let count = |p: AttackPhase| phases.iter().filter(|&&(q, _)| q == p).count();
    assert_eq!(
        (
            count(AttackPhase::Startup),
            count(AttackPhase::Active),
            count(AttackPhase::Recovery)
        ),
        (15, 2, 1)
    );
}

#[test]
fn phase_2_combo_has_its_extra_cut() {
    let mut w = phase2(&SHORT_ROAR);
    w.extend([
        ("combo_weight", 1.0),
        ("combo_damage", 1.0),
        ("combo_impact", 0.0),
        ("combo_gap_ticks", 2.0),
    ]);
    let mut run = Run::new(r#"("hp", 1000)"#, &enemy(0.0, -1.2, 180.0, &w));
    let phases = phase2_move(&mut run);
    let mut cuts: Vec<u32> = phases
        .iter()
        .filter(|&&(p, _)| p == AttackPhase::Active)
        .map(|&(_, hit)| hit)
        .collect();
    cuts.dedup();
    assert_eq!(cuts, [1, 2, 3, 4]);
}

#[test]
fn spin_is_a_choice_only_in_phase_2() {
    let mut w = phase2(&SHORT_ROAR);
    w.extend([
        ("spin_weight", 1.0),
        ("spin_damage", 1.0),
        ("spin_impact", 0.0),
    ]);
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    // Phase 1: nothing to choose within 2 m, so it stands.
    for _ in 0..5 {
        let s = run.idle();
        assert!(run.moves().is_empty());
        assert_eq!(s.enemies[0].state, EnemyCondition::Chase);
    }
    hit_once(&mut run);
    let mut t = 0;
    loop {
        run.idle();
        if !run.moves().is_empty() {
            break;
        }
        t += 1;
        assert!(t < 100, "no move");
    }
    assert_eq!(chosen(&run), (EnemyChoice::Spin, vec![EnemyChoice::Spin]));
}

#[test]
fn phase_2_walks_farther_each_tick() {
    // 1.5 m/s: 0.025 m a tick, 0.05 in phase 2 (scale 0.5), slower than
    // the character walking off; nothing to choose.
    let mut w = phase2(&SHORT_ROAR);
    w.extend([
        ("enemy_speed", 1.5),
        ("phase2_speed_scale", 0.5),
        ("approach_weight_per_meter", 0.0),
    ]);
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    let z0 = run.position().z;
    hit_once(&mut run);
    // Phase 1 walked 0.025 m a tick up to the hit.
    let walked = run.position().z - z0;
    assert!(walked > 0.0 && (walked / 0.025 - libm::roundf(walked / 0.025)).abs() < 1e-3);
    // Back off, then stand, and watch it walk up.
    for _ in 0..60 {
        run.hold(&[], &[(MOVE_Y, -1.0)]);
    }
    let mut z = run.position().z;
    for t in 0..5 {
        run.idle();
        let now = run.position().z;
        assert!((now - z - 0.05).abs() < 1e-5, "tick {t}: {z} to {now}");
        z = now;
    }
}

#[test]
fn approach_weight_max_of_0_never_approaches() {
    let w = [("approach_weight_max", 0.0)];
    let mut run = Run::new("", &enemy(0.0, -5.0, 180.0, &w));
    for t in 1..=5 {
        let s = run.idle();
        assert_eq!(
            (s.enemies[0].state, s.enemies[0].chase),
            (EnemyCondition::Chase, 0),
            "tick {t}"
        );
        assert!(run.moves().is_empty(), "tick {t}");
    }
}

/// The test enemy at `(0, 0, z)` facing +Z with a body of color
/// `(0.5, 0.5, 0.5)` and its arm.
fn enemy_with_body(z: f32, overrides: &[(&str, f64)]) -> String {
    let props = enemy_props(overrides);
    format!(
        r#"(name: "e", position: (0.0, 0.0, {z:?}), rotation: (0.0, 180.0, 0.0),
            props: [{props}],
            children: [(name: "body", mesh: Capsule(radius: 0.5, height: 2.0),
                    color: (0.5, 0.5, 0.5)),
                (name: "arm", position: (0.0, 1.0, -1.0), scale: (0.1, 0.1, 2.0),
                    mesh: Cube, color: (0.4, 0.3, 0.2))]),"#
    )
}

fn body_color(run: &Run) -> Vec3 {
    let scene = run.game.scene();
    scene
        .node(scene.find("e/body").unwrap())
        .mesh
        .unwrap()
        .color
}

#[test]
fn enemy_colors_rank_dead_flash_roar_warning_then_phase_2_red() {
    let body = Vec3::splat(0.5);
    let red = body * Vec3::new(1.0, 0.3, 0.3);
    let w = phase2(&[
        ("roar_ticks", 30.0),
        ("roar_blast_tick", 20.0),
        ("roar_reach", 0.01),
        ("revive_ticks", 10.0),
    ]);
    let mut run = Run::new("", &enemy_with_body(-1.2, &w));
    run.idle();
    assert_eq!(body_color(&run), body);
    // The hit's tick flashes, over the warning.
    hit_once(&mut run);
    assert_eq!(body_color(&run), FLASH_COLOR);
    // After the flash, the warning until the blast (roar tick 20).
    for _ in 1..FLASH_TICKS {
        run.idle();
    }
    assert_eq!(body_color(&run), FLASH_COLOR);
    run.idle();
    assert_eq!(body_color(&run), WARN_COLOR);
    while run.game.state().enemies[0].move_tick < 19 {
        run.idle();
        assert_eq!(body_color(&run), WARN_COLOR);
    }
    // From the blast on, dark red.
    run.idle();
    assert_eq!(run.game.state().enemies[0].phase, Some(AttackPhase::Active));
    assert_eq!(body_color(&run), red);
    while run.game.state().enemies[0].state == EnemyCondition::Roar {
        run.idle();
        assert_eq!(body_color(&run), red);
    }
    // Killed in phase 2: dimmed from its scene color; revived in phase 1.
    let first = hit_once(&mut run);
    assert_eq!(first, ["hit e Death"]);
    assert_eq!(body_color(&run), body * DEAD_DIM);
    assert_eq!(run.game.state().enemies[0].stage, 2);
    // Revived after 10 ticks, past the killing hit's flash.
    for _ in 0..10 {
        run.idle();
    }
    let s = run.game.state();
    assert_eq!(s.enemies[0].stage, 1);
    assert_ne!(s.enemies[0].state, EnemyCondition::Dead);
    assert_eq!(s.hurtboxes[0].health.unwrap().hp, 40.0);
    assert_eq!(body_color(&run), body);
    // The arm keeps its own colors throughout.
    let scene = run.game.scene();
    assert_eq!(
        scene.node(scene.find("e/arm").unwrap()).mesh.unwrap().color,
        Vec3::new(0.4, 0.3, 0.2)
    );
}

#[test]
fn phase_2_props_are_checked_in_order_with_exact_messages() {
    let full = r#"("lock_height", 1), ("hurt_radius", 0.7), ("hurt_height", 2), ("hp", 10),
        ("enemy_speed", 6)"#;
    let node = |props: &str| format!(r#"(name: "t", props: [{props}]),"#);
    for (props, message) in [
        // The enemy_speed check comes first.
        (
            r#"("phase2_at", 0.5), ("roar_ticks", 3)"#.to_owned(),
            "the node \"t\": prop \"phase2_at\" needs enemy_speed",
        ),
        (
            format!(r#"{full}, ("roar_ticks", 3)"#),
            "the node \"t\": prop \"roar_ticks\" needs phase2_at",
        ),
        (
            format!(r#"{full}, ("phase2_at", 1)"#),
            "the node \"t\": prop \"phase2_at\" must be greater than 0 and less than 1, got 1",
        ),
        (
            format!(
                r#"{full}, ("spin_min_range", 4), ("phase2_at", 0.5), ("roar_blast_tick", 71)"#
            ),
            "the node \"t\": needs spin_min_range <= spin_max_range",
        ),
        (
            format!(
                r#"{full}, ("phase2_at", 0.5), ("roar_blast_tick", 71), ("enemy_swing_low", 3)"#
            ),
            "the node \"t\": needs roar_blast_tick <= roar_ticks",
        ),
        // The walk in phase 2 comes after the lunge.
        (
            format!(
                r#"{full}, ("phase2_at", 0.5), ("phase2_speed_scale", 0.1), ("lunge_distance", 50)"#
            ),
            "the enemy \"t\": lunge moves 5 m per tick, which must be less than hurt_radius 0.7",
        ),
        (
            format!(r#"{full}, ("phase2_at", 0.5), ("phase2_speed_scale", 0.125)"#),
            "the enemy \"t\": enemy in phase 2 moves 0.8 m per tick, which must be less than \
             hurt_radius 0.7",
        ),
    ] {
        assert_eq!(error_of(&node(&props)), message, "{props}");
    }
    // Without phase2_at the scale is not checked against the radius, and
    // the spin's props go without it.
    assert!(
        Game::from_source(&scene(
            "",
            &node(&format!(r#"{full}, ("spin_weight", 3), ("spin_reach", 2)"#))
        ))
        .is_ok()
    );
}

#[test]
fn debug_swings_draw_the_roar_as_a_whole_circle_orange_then_red_on_the_blast() {
    // The blast on the roar's 8th tick, when the character's own swing is
    // long over (it is in recovery from the roar's 5th).
    let w = phase2(&[
        ("roar_ticks", 10.0),
        ("roar_blast_tick", 8.0),
        ("roar_reach", 0.01),
    ]);
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    let colors = |run: &Run| {
        let mut draw = DebugDraw::new();
        run.game.debug_draw(&["swings"], &mut draw);
        let mut colors: Vec<Vec3> = draw.lines().iter().map(|l| l.color).collect();
        colors.dedup();
        colors
    };
    hit_once(&mut run);
    for t in 2..=9 {
        run.idle();
        let want: &[Vec3] = match t {
            8 => &[DEBUG_ACTIVE, DEBUG_SLICE],
            9 => &[],
            _ => &[DEBUG_STARTUP],
        };
        if t >= 5 {
            assert_eq!(colors(&run), want, "roar tick {t}");
        }
    }
    // The whole circle: the startup's lines run all the way around.
    let mut run = Run::new("", &enemy(0.0, -1.2, 180.0, &w));
    hit_once(&mut run);
    for _ in 2..=6 {
        run.idle();
    }
    let mut draw = DebugDraw::new();
    run.game.debug_draw(&["swings"], &mut draw);
    let at = run.position();
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for l in draw.lines() {
        for p in [l.a, l.b] {
            let a = libm::atan2f(p.x - at.x, p.z - at.z);
            lo = lo.min(a);
            hi = hi.max(a);
        }
    }
    // Drawn in segments of about 11°: the ends come within one of ±π.
    assert!(lo < -2.9 && hi > 2.9, "{lo} {hi}");
}
