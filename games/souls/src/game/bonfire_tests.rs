//! Tests of the bonfires, sitting and the world reset
//! (`docs/architecture.md` section 11, "篝火"), a child of `game` so they
//! can read and set its own fields. The character stands at the origin
//! facing -Z; the test bonfire is 1 m in front of it, in reach (1.5 m).

use glam::Vec3;
use loom_core::debug::DebugDraw;
use loom_core::input::{ActionState, TickInput};

use super::debug::DEBUG_BONFIRE;
use super::enemy_ai::enter_phase2;
use super::*;
use crate::enemy::EnemyCondition;

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

/// A bonfire at `(0, 0, z)` with `range`.
fn bonfire(z: f32, range: f32) -> String {
    format!(
        r#"(name: "bonfire", position: (0.0, 0.0, {z:?}), props: [("bonfire_range", {range:?})]),"#
    )
}

/// A dummy to lock on to, 8 m ahead.
const DUMMY: &str = r#"(name: "dummy", position: (1.0, 0.0, -8.0),
    props: [("lock_height", 1.0), ("hurt_radius", 0.4), ("hurt_height", 1.8), ("hp", 100)]),"#;

struct Run {
    game: Game,
    input: ActionState,
}

impl Run {
    fn new(player: &str, nodes: &str) -> Self {
        Self::from(&scene(player, nodes))
    }

    fn from(source: &str) -> Self {
        Self {
            game: Game::from_source(source).unwrap(),
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

    fn idle(&mut self) -> GameState {
        self.hold(&[], &[])
    }

    /// A tick pressing the interact key (after a tick without it, so it is
    /// a fresh press).
    fn interact(&mut self) -> GameState {
        self.hold(&[INTERACT], &[])
    }

    fn events(&self) -> Vec<Event> {
        self.game.events().to_vec()
    }

    fn dropped(&self) -> Vec<(Input, DropReason)> {
        self.game
            .events()
            .iter()
            .filter_map(|e| match e {
                Event::InputDropped { input, reason, .. } => Some((*input, *reason)),
                _ => None,
            })
            .collect()
    }
}

fn rest() -> Event {
    Event::Rest {
        bonfire: "bonfire".to_owned(),
    }
}

// ---------------------------------------------------------------- sitting down

#[test]
fn interact_near_a_bonfire_sits_down_with_full_health_and_stamina_and_a_new_respawn_point() {
    let mut run = Run::new("", &bonfire(-1.0, 1.5));
    // Hurt, tired and turned, standing half a meter to the right.
    run.game.health.take(30.0, 0.0, 0.0);
    run.game.state.player.stamina = 10.0;
    run.game.state.player.stamina_wait = 20;
    run.game.state.player.position = Vec3::new(0.5, 0.0, 0.0);
    run.game.state.player.facing = 1.0;
    let s = run.interact();
    assert_eq!(run.events(), [rest()]);
    assert_eq!((s.player.action, s.player.action_tick), (Action::Sit, 0));
    assert_eq!(s.player.health, run.game.initial.health.now);
    assert_eq!(s.player.health.hp, 100.0);
    assert_eq!((s.player.stamina, s.player.stamina_wait), (100.0, 0));
    assert_eq!(
        s.respawn,
        RespawnState {
            position: Vec3::new(0.5, 0.0, 0.0),
            facing: 1.0,
            bonfire: Some("bonfire".to_owned()),
        }
    );
}

#[test]
fn interact_while_sprinting_and_locked_on_ends_the_sprint_lets_go_then_rests() {
    // The bonfire around the origin, 5 m: sprinting 20 ticks stays in it.
    let mut run = Run::new("", &format!("{}{DUMMY}", bonfire(0.0, 5.0)));
    run.hold(&[LOCK], &[]);
    assert!(run.game.lock.is_some());
    for _ in 0..20 {
        run.hold(&[ROLL], &[(MOVE_Y, 1.0)]);
    }
    assert_eq!(run.game.state().player.action, Action::Sprint);
    let s = run.hold(&[ROLL, INTERACT], &[(MOVE_Y, 1.0)]);
    assert_eq!(
        run.events(),
        [
            Event::ActionEnd {
                action: Action::Sprint,
                reason: EndReason::Finished,
            },
            Event::Lock {
                target: None,
                reason: LockReason::Rested,
            },
            rest(),
        ]
    );
    assert_eq!((s.player.action, s.lock), (Action::Sit, None));
}

#[test]
fn interact_at_exactly_the_bonfire_range_sits_down() {
    // 1 m away, a range of exactly 1.
    let mut run = Run::new("", &bonfire(-1.0, 1.0));
    assert_eq!(run.interact().player.action, Action::Sit);
}

#[test]
fn interact_beyond_every_bonfire_is_dropped_as_no_bonfire() {
    let mut run = Run::new("", &bonfire(-1.0, 0.99));
    let s = run.interact();
    assert_eq!(run.dropped(), [(Input::Interact, DropReason::NoBonfire)]);
    assert_eq!(s.player.action, Action::Idle);
}

#[test]
fn interact_while_dead_is_dropped_as_dead() {
    let mut run = Run::new("", &bonfire(-1.0, 1.5));
    run.game.health.take(1000.0, 0.0, 0.0);
    run.interact();
    assert_eq!(run.dropped(), [(Input::Interact, DropReason::Dead)]);
}

#[test]
fn interact_while_staggered_is_dropped_as_staggered() {
    let mut run = Run::new("", &bonfire(-1.0, 1.5));
    run.game.health.take(1.0, 1.0, 0.0);
    assert_eq!(run.game.health.now.state, Condition::Stagger);
    run.interact();
    assert_eq!(run.dropped(), [(Input::Interact, DropReason::Staggered)]);
}

#[test]
fn interact_while_rolling_or_attacking_is_dropped_as_busy() {
    for (press, action) in [(ROLL, Action::Backstep), (ATTACK, Action::Attack)] {
        let mut run = Run::new("", &bonfire(-1.0, 1.5));
        // A tap of the roll key fires on its release.
        run.hold(&[press], &[]);
        if press == ROLL {
            run.idle();
        }
        assert_eq!(run.game.state().player.action, action);
        run.idle();
        let s = run.interact();
        assert_eq!(
            run.dropped(),
            [(Input::Interact, DropReason::Busy)],
            "{press}"
        );
        assert_eq!(s.player.action, action);
    }
}

#[test]
fn interact_with_a_living_enemy_in_its_aggro_range_is_dropped_as_enemy_near() {
    // An enemy 5 m away, aggro range 5, too far to choose anything.
    let enemy = |aggro: f64| {
        format!(
            r#"(name: "e", position: (0.0, 0.0, 5.0), props: [("lock_height", 1),
                ("hurt_radius", 0.5), ("hurt_height", 2), ("hp", 10), ("enemy_speed", 0.6),
                ("enemy_aggro_range", {aggro:?}), ("enemy_face_angle", 1),
                ("approach_weight_per_meter", 0), ("lunge_distance", 0.2)]),"#
        )
    };
    let mut run = Run::new("", &format!("{}{}", bonfire(-1.0, 1.5), enemy(5.0)));
    run.interact();
    assert_eq!(run.dropped(), [(Input::Interact, DropReason::EnemyNear)]);
    // Just out of its range, it does not stop the rest.
    let mut run = Run::new("", &format!("{}{}", bonfire(-1.0, 1.5), enemy(4.99)));
    assert_eq!(run.interact().player.action, Action::Sit);
    // Nor does a dead one.
    let mut run = Run::new("", &format!("{}{}", bonfire(-1.0, 1.5), enemy(5.0)));
    run.game.hurtboxes[0]
        .health
        .as_mut()
        .unwrap()
        .take(100.0, 0.0, 0.0);
    assert_eq!(run.interact().player.action, Action::Sit);
}

// ---------------------------------------------------------------- sitting

#[test]
fn sitting_character_neither_moves_nor_turns_and_drops_attack_roll_and_lock_presses() {
    let mut run = Run::new("", &format!("{}{DUMMY}", bonfire(-1.0, 1.5)));
    run.interact();
    run.idle();
    for keys in [&[ATTACK][..], &[HEAVY], &[LOCK]] {
        let s = run.hold(keys, &[]);
        let input = match keys[0] {
            ATTACK => Input::Attack,
            HEAVY => Input::Heavy,
            _ => Input::Lock,
        };
        assert_eq!(run.dropped(), [(input, DropReason::Sitting)], "{keys:?}");
        assert_eq!(s.player.action, Action::Sit);
        assert_eq!(s.lock, None);
        run.idle();
    }
    // A roll tap: dropped on its release.
    run.hold(&[ROLL], &[]);
    let s = run.idle();
    assert_eq!(run.dropped(), [(Input::Roll, DropReason::Sitting)]);
    assert_eq!(
        (s.player.action, s.player.position, s.player.facing),
        (Action::Sit, Vec3::ZERO, 0.0)
    );
}

#[test]
fn push_stands_the_character_up_without_moving_that_tick() {
    let mut run = Run::new("", &bonfire(-1.0, 1.5));
    run.interact();
    let s = run.hold(&[], &[(MOVE_X, 1.0)]);
    assert_eq!(run.events(), [Event::Stand]);
    assert_eq!(
        (s.player.action, s.player.position, s.player.facing),
        (Action::Idle, Vec3::ZERO, 0.0)
    );
    // The next tick walks.
    let s = run.hold(&[], &[(MOVE_X, 1.0)]);
    assert!(s.player.position.x > 0.0);
}

#[test]
fn second_interact_press_stands_up_too() {
    let mut run = Run::new("", &bonfire(-1.0, 1.5));
    run.interact();
    run.idle();
    let s = run.interact();
    assert_eq!(run.events(), [Event::Stand]);
    assert_eq!(s.player.action, Action::Idle);
    // Held down, it does nothing more.
    run.interact();
    assert!(run.events().is_empty());
}

#[test]
fn sitting_character_gets_stamina_back() {
    let mut run = Run::new("", &bonfire(-1.0, 1.5));
    run.interact();
    run.game.state.player.stamina = 10.0;
    let s = run.idle();
    assert_eq!(s.player.action, Action::Sit);
    assert!(s.player.stamina > 10.0, "{}", s.player.stamina);
}

#[test]
fn swinger_stagger_interrupts_sitting() {
    // A stake 2 m off swinging all around every 5 ticks, hitting on tick 4.
    let stake = r#"(name: "s", position: (0.0, 0.0, -2.0), props: [("swing_damage", 1),
        ("swing_startup_ticks", 3), ("swing_active_ticks", 1), ("swing_recovery_ticks", 1),
        ("swing_arc", 360), ("swing_reach", 10)]),"#;
    let mut run = Run::new("", &format!("{}{stake}", bonfire(-1.0, 1.5)));
    let s = run.interact();
    assert_eq!(s.player.action, Action::Sit);
    let mut s = run.idle();
    while s.player.hits == 0 {
        assert_eq!(s.player.action, Action::Sit);
        s = run.idle();
    }
    assert!(run.events().contains(&Event::ActionEnd {
        action: Action::Sit,
        reason: EndReason::Interrupted,
    }));
    assert_eq!(
        (s.player.action, s.player.health.state),
        (Action::Idle, Condition::Stagger)
    );
}

#[test]
fn sitting_character_node_is_drawn_half_a_meter_low() {
    let mut run = Run::new("", &bonfire(-1.0, 1.5));
    let node = |run: &Run| {
        let scene = run.game.scene();
        scene.transform(scene.find("player").unwrap()).position
    };
    let s = run.interact();
    assert_eq!(node(&run), Vec3::new(0.0, -0.5, 0.0));
    assert_eq!(s.player.position, Vec3::ZERO);
    run.hold(&[], &[(MOVE_X, 1.0)]);
    assert_eq!(node(&run), Vec3::ZERO);
}

// ---------------------------------------------------------------- world reset

/// The state dump's `hurtboxes` (without `hits` and `last_hit`),
/// `swingers` and `enemies`, as text: every field, and like the dump's
/// bytes it tells -0.0 from 0.0.
fn world(s: &GameState) -> String {
    let hurtboxes: Vec<_> = s
        .hurtboxes
        .iter()
        .map(|h| (h.node.clone(), h.health))
        .collect();
    format!("{:?}", (hurtboxes, &s.swingers, &s.enemies))
}

/// `source` loaded, the character put at `at`, one tick without input: what
/// a world reset on that tick must give.
fn fresh(source: &str, at: Vec3) -> Run {
    let mut run = Run::from(source);
    run.game.state.player.position = at;
    run.idle();
    run
}

#[test]
fn sitting_down_resets_the_demo_dummies_and_stake_but_keeps_their_hits() {
    let source = crate::demo_scene_source();
    let mut run = Run::from(source);
    // Hurt and stagger the dummies and the stake, count a hit, let the
    // stake swing a while.
    for h in 0..4 {
        let health = run.game.hurtboxes[h].health.as_mut().unwrap();
        health.take(30.0, 100.0, 1.0);
    }
    run.game.hurtboxes[1].hits = 3;
    for _ in 0..50 {
        run.idle();
    }
    let before = run.game.state();
    assert_ne!(
        world(&before),
        world(&fresh(source, Vec3::ZERO).game.state())
    );
    // Sit at the bonfire (-6, 0, 6).
    let at = Vec3::new(-5.5, 0.0, 5.5);
    run.game.state.player.position = at;
    let s = run.interact();
    assert_eq!(run.events(), [rest()]);
    let load = fresh(source, at);
    assert_eq!(world(&s), world(&load.game.state()));
    for (h, init) in run.game.hurtboxes.iter().zip(&load.game.hurtboxes) {
        assert_eq!(h.health, init.health, "{}", h.path);
    }
    assert_eq!(s.hurtboxes[1].hits, 3);
    assert_eq!(s.hurtboxes[1].last_hit, before.hurtboxes[1].last_hit);
    assert_eq!(s.swingers[0].swing_tick, 1);
}

#[test]
fn sitting_down_resets_the_boss_to_its_load_state_from_phase_2() {
    let source = include_str!("../../../../assets/scenes/boss.ron");
    let mut run = Run::from(source);
    for _ in 0..30 {
        run.idle();
    }
    // Into phase 2, roaring, and moved far off, out of the bonfire's aggro.
    {
        let game = &mut run.game;
        let health = game.hurtboxes[0].health.as_mut().unwrap();
        health.take(400.0, 0.0, 0.0);
        enter_phase2(&mut game.enemies[0], health, &mut game.events);
        let id = game.enemies[0].id;
        game.scene.transform_mut(id).position = Vec3::new(0.0, 0.0, -30.0);
    }
    let s = run.idle();
    let e = &s.enemies[0];
    assert_eq!((e.stage, e.state), (2, EnemyCondition::Roar));
    assert_ne!(e.rng, 7);
    // Sit at the bonfire (4, 0, 12).
    let at = Vec3::new(4.0, 0.0, 11.0);
    run.game.state.player.position = at;
    let s = run.interact();
    assert_eq!(run.events(), [rest()]);
    let load = fresh(source, at);
    assert_eq!(world(&s), world(&load.game.state()));
    let e = &s.enemies[0];
    assert_eq!((e.stage, e.state, e.rng), (1, EnemyCondition::Idle, 7));
    assert_eq!(s.hurtboxes[0].health.unwrap().hp, 600.0);
    let node = |run: &Run| {
        let scene = run.game.scene();
        *scene.transform(scene.find("boss").unwrap())
    };
    assert_eq!(node(&run), node(&load));
}

// ---------------------------------------------------------------- respawn

/// Kills the character and runs to its respawn; the state then.
fn die_and_respawn(run: &mut Run) -> GameState {
    run.game.health.take(1000.0, 0.0, 0.0);
    for _ in 0..200 {
        let s = run.idle();
        if run
            .game
            .events()
            .iter()
            .any(|e| matches!(e, Event::Respawn { .. }))
        {
            return s;
        }
    }
    panic!("no respawn");
}

#[test]
fn death_after_resting_respawns_at_the_rest_point_with_the_world_reset() {
    let source = crate::demo_scene_source();
    let mut run = Run::from(source);
    let at = Vec3::new(-5.5, 0.0, 5.5);
    run.game.state.player.position = at;
    run.game.state.player.facing = 2.0;
    run.interact();
    run.hold(&[], &[(MOVE_X, 1.0)]);
    for _ in 0..30 {
        run.hold(&[], &[(MOVE_X, 1.0)]);
    }
    run.game.hurtboxes[0]
        .health
        .as_mut()
        .unwrap()
        .take(30.0, 0.0, 0.0);
    let s = die_and_respawn(&mut run);
    assert_eq!(
        run.events()
            .iter()
            .find(|e| matches!(e, Event::Respawn { .. })),
        Some(&Event::Respawn { position: at })
    );
    assert_eq!((s.player.position, s.player.facing), (at, 2.0));
    assert_eq!(s.hurtboxes[0].health.unwrap().hp, 100.0);
    assert_eq!(s.swingers[0].swing_tick, 1);
    assert_eq!(s.respawn.bonfire.as_deref(), Some("bonfire"));
}

#[test]
fn death_without_resting_respawns_at_the_spawn_point_with_the_world_reset() {
    let source = crate::demo_scene_source();
    let mut run = Run::from(source);
    let spawn = run.game.state().player.position;
    for _ in 0..30 {
        run.hold(&[], &[(MOVE_X, 1.0)]);
    }
    run.game.hurtboxes[2]
        .health
        .as_mut()
        .unwrap()
        .take(30.0, 0.0, 0.0);
    let s = die_and_respawn(&mut run);
    assert_eq!(s.player.position, spawn);
    assert_eq!(s.respawn.bonfire, None);
    assert_eq!(s.hurtboxes[2].health.unwrap().hp, 100.0);
    assert_eq!(s.swingers[0].swing_tick, 1);
}

// ---------------------------------------------------------------- the rest

#[test]
fn debug_bonfires_draw_each_range_and_a_cross_over_the_respawn_bonfire() {
    let mut run = Run::new("", &bonfire(-1.0, 1.5));
    let lines = |run: &Run| {
        let mut draw = DebugDraw::new();
        run.game.debug_draw(&["bonfires"], &mut draw);
        draw.lines().to_vec()
    };
    let circle = lines(&run);
    assert!(!circle.is_empty());
    for l in &circle {
        assert_eq!(l.color, DEBUG_BONFIRE);
        for p in [l.a, l.b] {
            let d = p - Vec3::new(0.0, 0.0, -1.0);
            assert!((d.length() - 1.5).abs() < 1e-4 && d.y == 0.0, "{p}");
        }
    }
    run.interact();
    let with_cross = lines(&run);
    assert_eq!(with_cross.len(), circle.len() + 3);
    for l in &with_cross[circle.len()..] {
        let mid = (l.a + l.b) * 0.5;
        assert!(mid.abs_diff_eq(Vec3::new(0.0, 1.0, -1.0), 1e-5), "{mid}");
        assert!(((l.b - l.a).length() - 0.5).abs() < 1e-5);
    }
}

#[test]
fn bonfire_nodes_are_checked_in_order_with_exact_messages() {
    let error_of = |nodes: &str| match Game::from_source(&scene("", nodes)) {
        Ok(_) => panic!("the scene should be rejected: {nodes}"),
        Err(e) => e.to_string(),
    };
    for (nodes, message) in [
        (
            r#"(name: "b", props: [("bonfire_range", 0)]),"#.to_owned(),
            "the node \"b\": prop \"bonfire_range\" must be greater than 0, got 0",
        ),
        (
            r#"(name: "g", children: [(name: "b", props: [("bonfire_range", 1)])]),"#.to_owned(),
            "the bonfire \"g/b\" must be a root node, and not the camera",
        ),
        // A nested lock target is reported as one first.
        (
            r#"(name: "g", children: [(name: "b", props: [("bonfire_range", 1),
                ("lock_height", 1)])]),"#
                .to_owned(),
            "the lock target \"g/b\" must be a root node, and not the camera",
        ),
        (
            r#"(name: "b", props: [("bonfire_range", 1), ("lock_height", 1)]),"#.to_owned(),
            "the bonfire \"b\" cannot also be a lock target, hurtbox, swinger or enemy",
        ),
        (
            r#"(name: "b", props: [("bonfire_range", 1), ("hurt_radius", 1),
                ("hurt_height", 1)]),"#
                .to_owned(),
            "the bonfire \"b\" cannot also be a lock target, hurtbox, swinger or enemy",
        ),
        (
            r#"(name: "b", props: [("bonfire_range", 1), ("swing_damage", 1)]),"#.to_owned(),
            "the bonfire \"b\" cannot also be a lock target, hurtbox, swinger or enemy",
        ),
    ] {
        assert_eq!(error_of(&nodes), message, "{nodes}");
    }
    // The camera cannot be one.
    let source = scene("", "").replace(
        r#"(name: "cam", camera:"#,
        r#"(name: "cam", props: [("bonfire_range", 1)], camera:"#,
    );
    match Game::from_source(&source) {
        Ok(_) => panic!("a camera bonfire should be rejected"),
        Err(e) => assert_eq!(
            e.to_string(),
            "the bonfire \"cam\" must be a root node, and not the camera"
        ),
    }
    // It may have a collider.
    let with_block = r#"(name: "b", props: [("bonfire_range", 1)],
        children: [(name: "block", collider: Box)]),"#;
    assert!(Game::from_source(&scene("", with_block)).is_ok());
}
