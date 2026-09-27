//! Tests of the estus flask (`docs/architecture.md` section 11, "原素瓶"),
//! a child of `game` so they can read and set its own fields. The
//! character stands at the origin facing -Z with the camera behind it, and
//! has a `flask` child; the defaults hold unless a test says otherwise: 3
//! drinks of 40 % of 100 hp, 60 ticks each, healing on tick 30, walking at
//! 1 m/s.

use std::f32::consts::FRAC_PI_2;

use glam::Vec3;
use loom_core::input::{ActionState, TickInput};

use super::*;
use crate::moves::AttackPhase;

/// The flask child of the scene files.
const FLASK: &str = r#"(name: "flask", position: (0.5, 1.2, -0.3), scale: (0.12, 0.25, 0.12),
    mesh: Cube, color: (0.9, 0.7, 0.2)),"#;

/// The character with `player` props and `children`, then `nodes`.
fn scene_with(player: &str, children: &str, nodes: &str) -> String {
    format!(
        r#"(
    version: 1,
    game: "souls",
    camera: "cam",
    ambient: (0.2, 0.2, 0.2),
    nodes: [
        (name: "player", mesh: Capsule(radius: 0.35, height: 1.8),
            character: (run_speed: 3.75, turn_speed: 720.0), props: [{player}],
            children: [{children}]),
        (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0),
            orbit_camera: (target: "player", pivot_height: 1.5, distance: 4.0,
                yaw: 0.0, pitch: 0.0, pitch_min: -60.0, pitch_max: 60.0,
                collision_radius: 0.2, return_speed: 4.0)),
        {nodes}
    ],
)"#
    )
}

/// A dummy to lock on to, 8 m ahead.
const DUMMY: &str = r#"(name: "dummy", position: (1.0, 0.0, -8.0),
    props: [("lock_height", 1.0), ("hurt_radius", 0.4), ("hurt_height", 1.8), ("hp", 100)]),"#;

/// A bonfire around the origin.
const BONFIRE: &str = r#"(name: "bonfire", position: (0.0, 0.0, -1.0),
    props: [("bonfire_range", 1.5)]),"#;

/// A stake 2 m ahead swinging all around once, hitting for 10 (and
/// staggering) on tick `at`.
fn stake(at: u32) -> String {
    format!(
        r#"(name: "stake", position: (0.0, 0.0, -2.0), props: [("swing_damage", 10),
        ("swing_startup_ticks", {}), ("swing_active_ticks", 1), ("swing_recovery_ticks", 1000),
        ("swing_arc", 360), ("swing_reach", 10)]),"#,
        at - 1
    )
}

struct Run {
    game: Game,
    input: ActionState,
}

impl Run {
    fn new(player: &str, nodes: &str) -> Self {
        Self {
            game: Game::from_source(&scene_with(player, FLASK, nodes)).unwrap(),
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

    /// A tick pressing the drink key (after a tick without it).
    fn drink(&mut self) -> GameState {
        self.hold(&[DRINK], &[])
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

    fn heals(&self) -> Vec<Event> {
        self.game
            .events()
            .iter()
            .filter(|e| matches!(e, Event::Heal { .. }))
            .cloned()
            .collect()
    }

    fn flask_drawn(&self) -> bool {
        let scene = self.game.scene();
        scene.is_drawn(scene.find("player/flask").unwrap())
    }
}

fn drink_start(buffered: bool) -> Event {
    Event::ActionStart {
        action: Action::Drink,
        combo: 0,
        buffered,
    }
}

// ------------------------------------------------------------------ drinking

#[test]
fn drink_uses_a_flask_and_counts_action_tick_up_to_flask_ticks() {
    let mut run = Run::new("", "");
    assert_eq!(run.game.state().player.flask, 3);
    let s = run.drink();
    assert_eq!(run.events(), [drink_start(false)]);
    assert_eq!(
        (s.player.flask, s.player.action, s.player.action_tick),
        (2, Action::Drink, 1)
    );
    for tick in 2..60 {
        let s = run.idle();
        assert_eq!(
            (s.player.action, s.player.action_tick),
            (Action::Drink, tick)
        );
    }
    let s = run.idle();
    assert!(run.events().contains(&Event::ActionEnd {
        action: Action::Drink,
        reason: EndReason::Finished,
    }));
    assert_eq!(
        (s.player.flask, s.player.action, s.player.action_tick),
        (2, Action::Idle, 0)
    );
}

#[test]
fn drink_heals_flask_heal_of_full_health_on_the_heal_tick_only() {
    let mut run = Run::new("", "");
    run.game.health.take(50.0, 0.0, 0.0);
    run.drink();
    for _ in 2..30 {
        run.idle();
        assert_eq!(run.heals(), []);
    }
    let s = run.idle();
    assert_eq!(s.player.action_tick, 30);
    assert_eq!(
        run.heals(),
        [Event::Heal {
            amount: 40.0,
            hp: 90.0,
        }]
    );
    assert_eq!(s.player.health.hp, 90.0);
    for _ in 31..=60 {
        run.idle();
        assert_eq!(run.heals(), []);
    }
}

#[test]
fn drink_at_full_health_heals_nothing() {
    let mut run = Run::new("", "");
    run.drink();
    for _ in 2..=30 {
        run.idle();
    }
    assert_eq!(
        run.heals(),
        [Event::Heal {
            amount: 0.0,
            hp: 100.0,
        }]
    );
}

#[test]
fn drink_heals_no_higher_than_full_health() {
    let mut run = Run::new("", "");
    run.game.health.take(20.0, 0.0, 0.0);
    run.drink();
    for _ in 2..=30 {
        run.idle();
    }
    assert_eq!(
        run.heals(),
        [Event::Heal {
            amount: 20.0,
            hp: 100.0,
        }]
    );
}

#[test]
fn drink_heal_tick_and_length_follow_the_props() {
    let mut run = Run::new(
        r#"("flask_ticks", 3), ("flask_heal_tick", 3), ("flask_heal", 0.25)"#,
        "",
    );
    run.game.health.take(50.0, 0.0, 0.0);
    run.drink();
    run.idle();
    assert_eq!(run.heals(), []);
    let s = run.idle();
    assert_eq!(
        run.events(),
        [
            Event::Heal {
                amount: 25.0,
                hp: 75.0,
            },
            Event::ActionEnd {
                action: Action::Drink,
                reason: EndReason::Finished,
            },
        ]
    );
    assert_eq!(s.player.action, Action::Idle);
}

#[test]
fn drinking_character_walks_flask_walk_speed_and_turns_towards_the_input() {
    let mut run = Run::new(r#"("flask_walk_speed", 1.5)"#, "");
    let step = 1.5 / 60.0;
    run.drink();
    let mut last = run.game.state().player;
    for _ in 0..10 {
        let s = run.hold(&[], &[(MOVE_X, 1.0)]);
        assert!((s.player.position.x - last.position.x - step).abs() < 1e-6);
        assert_eq!(s.player.position.z, 0.0);
        // 12 degrees a tick towards +X (a facing of -π/2).
        let want = turn(last.facing, -FRAC_PI_2, run.game.max_turn);
        assert_eq!(s.player.facing, want);
        last = s.player;
    }
    assert_eq!(last.facing, -FRAC_PI_2);
}

#[test]
fn drinking_character_locked_on_faces_the_target_while_walking_sideways() {
    let mut run = Run::new("", DUMMY);
    run.hold(&[LOCK], &[]);
    assert!(run.game.lock.is_some());
    run.drink();
    let target = Vec3::new(1.0, 0.0, -8.0);
    for _ in 0..10 {
        let s = run.hold(&[], &[(MOVE_X, 1.0)]);
        assert_eq!(s.player.action, Action::Drink);
        assert_eq!(s.player.facing, bearing(s.player.position, target));
    }
}

#[test]
fn drinking_character_gets_stamina_back_even_from_zero() {
    let mut run = Run::new("", "");
    run.game.state.player.stamina = 0.0;
    let s = run.drink();
    assert_eq!(s.player.action, Action::Drink);
    assert_eq!(run.dropped(), []);
    let before = s.player.stamina;
    let s = run.idle();
    assert!(s.player.stamina > before, "{}", s.player.stamina);
}

#[test]
fn drink_is_not_invulnerable() {
    let mut run = Run::new("", "");
    for _ in 0..60 {
        let s = run.drink();
        assert!(!s.player.invulnerable);
        run.idle();
    }
}

// ------------------------------------------------------------------ dropping

#[test]
fn attack_heavy_roll_drink_and_interact_presses_while_drinking_are_dropped_as_drinking() {
    let mut run = Run::new("", BONFIRE);
    run.drink();
    for press in [ATTACK, HEAVY, DRINK, INTERACT] {
        run.hold(&[press], &[]);
        let input = match press {
            ATTACK => Input::Attack,
            HEAVY => Input::Heavy,
            DRINK => Input::Drink,
            _ => Input::Interact,
        };
        assert_eq!(run.dropped(), [(input, DropReason::Drinking)], "{press}");
        run.idle();
    }
    // A tap of the roll key fires on its release.
    run.hold(&[ROLL], &[]);
    let s = run.idle();
    assert_eq!(run.dropped(), [(Input::Roll, DropReason::Drinking)]);
    assert_eq!((s.player.action, s.player.flask), (Action::Drink, 2));
}

#[test]
fn lock_key_works_while_drinking() {
    let mut run = Run::new("", DUMMY);
    run.drink();
    let s = run.hold(&[LOCK], &[]);
    assert_eq!(s.lock, Some("dummy".to_owned()));
    assert_eq!(s.player.action, Action::Drink);
}

#[test]
fn drink_pressed_with_an_empty_flask_is_dropped_as_empty() {
    let mut run = Run::new(r#"("flask_charges", 0)"#, "");
    let s = run.drink();
    assert_eq!(run.dropped(), [(Input::Drink, DropReason::Empty)]);
    assert_eq!((s.player.action, s.player.flask), (Action::Idle, 0));
}

#[test]
fn fourth_drink_is_dropped_as_empty() {
    let mut run = Run::new(r#"("flask_ticks", 1), ("flask_heal_tick", 1)"#, "");
    for flask in [2, 1, 0] {
        assert_eq!(run.drink().player.flask, flask);
        run.idle();
    }
    run.drink();
    assert_eq!(run.dropped(), [(Input::Drink, DropReason::Empty)]);
}

#[test]
fn drink_while_rolling_is_dropped_as_rolling() {
    let mut run = Run::new("", "");
    run.hold(&[ROLL], &[]);
    let s = run.idle();
    assert_eq!(s.player.action, Action::Backstep);
    let s = run.drink();
    assert_eq!(run.dropped(), [(Input::Drink, DropReason::Rolling)]);
    assert_eq!(s.player.flask, 3);
}

#[test]
fn drink_during_attack_startup_is_dropped_as_startup() {
    let mut run = Run::new("", "");
    run.hold(&[ATTACK], &[]);
    let s = run.drink();
    assert_eq!(s.player.attack_phase, Some(AttackPhase::Startup));
    assert_eq!(run.dropped(), [(Input::Drink, DropReason::Startup)]);
}

#[test]
fn attack_and_drink_on_one_tick_attack_and_drop_the_drink_as_outranked() {
    let mut run = Run::new("", "");
    let s = run.hold(&[ATTACK, DRINK], &[]);
    assert_eq!(run.dropped(), [(Input::Drink, DropReason::Outranked)]);
    assert_eq!((s.player.action, s.player.flask), (Action::Attack, 3));
}

#[test]
fn drink_and_roll_tap_on_one_tick_drink_and_drop_the_roll_as_outranked() {
    let mut run = Run::new("", "");
    run.hold(&[ROLL], &[]);
    let s = run.drink();
    assert_eq!(run.dropped(), [(Input::Roll, DropReason::Outranked)]);
    assert_eq!(s.player.action, Action::Drink);
}

#[test]
fn drink_while_sprinting_ends_the_sprint_and_starts_drinking() {
    let mut run = Run::new("", "");
    for _ in 0..20 {
        run.hold(&[ROLL], &[(MOVE_Y, 1.0)]);
    }
    assert_eq!(run.game.state().player.action, Action::Sprint);
    let s = run.hold(&[ROLL, DRINK], &[(MOVE_Y, 1.0)]);
    assert_eq!(
        run.events(),
        [
            Event::ActionEnd {
                action: Action::Sprint,
                reason: EndReason::Finished,
            },
            drink_start(false),
        ]
    );
    assert_eq!(s.player.action, Action::Drink);
}

#[test]
fn drink_pressed_after_an_attack_is_buffered_and_starts_when_it_ends() {
    let mut run = Run::new("", "");
    run.hold(&[ATTACK], &[]);
    let mut s = run.idle();
    while s.player.attack_phase != Some(AttackPhase::Recovery) {
        s = run.idle();
    }
    let s = run.drink();
    assert_eq!(s.player.buffered, Some(Buffered::Drink));
    let mut s = s;
    while s.player.action == Action::Attack {
        s = run.idle();
    }
    // The attack's last tick ends it; the buffered drink starts on the next.
    let s = run.idle();
    assert_eq!(run.events(), [drink_start(true)]);
    assert_eq!(
        (s.player.action, s.player.action_tick, s.player.flask),
        (Action::Drink, 1, 2)
    );
}

#[test]
fn buffered_drink_with_no_flask_left_is_dropped_as_empty_and_buffered() {
    let mut run = Run::new(r#"("flask_charges", 0)"#, "");
    run.hold(&[ATTACK], &[]);
    let mut s = run.idle();
    while s.player.attack_phase != Some(AttackPhase::Recovery) {
        s = run.idle();
    }
    run.drink();
    let mut dropped = Vec::new();
    for _ in 0..100 {
        run.idle();
        dropped.extend(run.events().into_iter().filter_map(|e| match e {
            Event::InputDropped { .. } => Some(e),
            _ => None,
        }));
    }
    assert_eq!(
        dropped,
        [Event::InputDropped {
            input: Input::Drink,
            reason: DropReason::Empty,
            buffered: true,
        }]
    );
}

// -------------------------------------------------------------- interruption

#[test]
fn stagger_before_the_heal_tick_wastes_the_drink() {
    let mut run = Run::new("", &stake(10));
    run.drink();
    let mut s = run.idle();
    while s.player.hits == 0 {
        assert_eq!(s.player.action, Action::Drink);
        s = run.idle();
    }
    assert_eq!(s.player.last_hit.as_ref().map(|h| h.tick), Some(10));
    assert!(run.events().contains(&Event::ActionEnd {
        action: Action::Drink,
        reason: EndReason::Interrupted,
    }));
    assert_eq!(
        (s.player.action, s.player.flask, s.player.health.hp),
        (Action::Idle, 2, 90.0)
    );
    assert_eq!(s.player.health.state, Condition::Stagger);
    for _ in 0..60 {
        let s = run.idle();
        assert_eq!(run.heals(), []);
        assert_eq!(s.player.health.hp, 90.0);
    }
}

#[test]
fn stagger_after_the_heal_tick_keeps_the_heal() {
    let mut run = Run::new("", &stake(40));
    run.game.health.take(50.0, 0.0, 0.0);
    run.drink();
    let mut s = run.idle();
    while s.player.hits == 0 {
        s = run.idle();
    }
    assert_eq!(s.player.last_hit.as_ref().map(|h| h.tick), Some(40));
    assert!(run.events().contains(&Event::ActionEnd {
        action: Action::Drink,
        reason: EndReason::Interrupted,
    }));
    // 50 + 40 - 10.
    assert_eq!(
        (s.player.action, s.player.flask, s.player.health.hp),
        (Action::Idle, 2, 80.0)
    );
}

// ------------------------------------------------------------------ refilling

#[test]
fn sitting_at_a_bonfire_fills_the_flask() {
    let mut run = Run::new(r#"("flask_ticks", 1), ("flask_heal_tick", 1)"#, BONFIRE);
    run.drink();
    run.idle();
    assert_eq!(run.drink().player.flask, 1);
    run.idle();
    let s = run.hold(&[INTERACT], &[]);
    assert_eq!((s.player.action, s.player.flask), (Action::Sit, 3));
}

#[test]
fn respawn_fills_the_flask() {
    let mut run = Run::new(r#"("flask_ticks", 1), ("flask_heal_tick", 1)"#, "");
    run.drink();
    assert_eq!(run.game.state().player.flask, 2);
    run.game.health.take(1000.0, 0.0, 0.0);
    let mut s = run.idle();
    while !run
        .events()
        .iter()
        .any(|e| matches!(e, Event::Respawn { .. }))
    {
        assert_eq!(s.player.flask, 2);
        s = run.idle();
    }
    assert_eq!(s.player.flask, 3);
}

// -------------------------------------------------------------- presentation

#[test]
fn flask_node_is_drawn_only_while_drinking() {
    let mut run = Run::new(r#"("flask_ticks", 3), ("flask_heal_tick", 2)"#, "");
    assert!(!run.flask_drawn());
    run.idle();
    assert!(!run.flask_drawn());
    run.drink();
    assert!(run.flask_drawn());
    run.idle();
    assert!(run.flask_drawn());
    let s = run.idle();
    assert_eq!(s.player.action, Action::Idle);
    assert!(!run.flask_drawn());
}

#[test]
fn character_without_a_flask_child_drinks_all_the_same() {
    let source = scene_with("", "", "");
    let mut run = Run {
        game: Game::from_source(&source).unwrap(),
        input: ActionState::new(),
    };
    let s = run.drink();
    assert_eq!((s.player.action, s.player.flask), (Action::Drink, 2));
}

// ---------------------------------------------------------------- validation

#[test]
fn flask_props_are_checked_in_order_with_exact_messages() {
    let error_of =
        |player: &str, children: &str| match Game::from_source(&scene_with(player, children, "")) {
            Ok(_) => panic!("the scene should be rejected: {player} {children}"),
            Err(e) => e.to_string(),
        };
    for (player, children, message) in [
        (
            r#"("flask_charges", -1)"#,
            FLASK,
            "the character \"player\": prop \"flask_charges\" must be a whole number of at least \
             0, got -1",
        ),
        (
            r#"("flask_heal", 0)"#,
            FLASK,
            "the character \"player\": prop \"flask_heal\" must be greater than 0 and at most 1, \
             got 0",
        ),
        (
            r#"("flask_heal", 1.5)"#,
            FLASK,
            "the character \"player\": prop \"flask_heal\" must be greater than 0 and at most 1, \
             got 1.5",
        ),
        (
            r#"("flask_ticks", 0)"#,
            FLASK,
            "the character \"player\": prop \"flask_ticks\" must be a whole number of at least 1, \
             got 0",
        ),
        (
            r#"("flask_heal_tick", 0.5)"#,
            FLASK,
            "the character \"player\": prop \"flask_heal_tick\" must be a whole number of at \
             least 1, got 0.5",
        ),
        (
            r#"("flask_walk_speed", 0)"#,
            FLASK,
            "the character \"player\": prop \"flask_walk_speed\" must be greater than 0, got 0",
        ),
        (
            r#"("flask_heal_tick", 61)"#,
            FLASK,
            "the character \"player\": needs flask_heal_tick <= flask_ticks",
        ),
        // After the swing band.
        (
            r#"("flask_heal_tick", 61), ("swing_low", 2)"#,
            FLASK,
            "the character \"player\": needs swing_low < swing_high",
        ),
        (
            r#"("flask_walk_speed", 30)"#,
            FLASK,
            "the character \"player\": drink moves 0.5 m per tick, which must be less than the \
             capsule radius 0.35",
        ),
        // Before the steps.
        (
            r#"("flask_walk_speed", 30), ("flask_heal_tick", 61)"#,
            FLASK,
            "the character \"player\": needs flask_heal_tick <= flask_ticks",
        ),
        // The drink step after the others.
        (
            r#"("flask_walk_speed", 30), ("heavy_distance", 2.5)"#,
            FLASK,
            "the character \"player\": heavy moves 0.5 m per tick, which must be less than the \
             capsule radius 0.35",
        ),
        (
            "",
            r#"(name: "flask", position: (0.5, 1.2, -0.3)),"#,
            "the character \"player\": its flask needs a mesh",
        ),
        // Props before the flask mesh.
        (
            r#"("flask_walk_speed", 30)"#,
            r#"(name: "flask"),"#,
            "the character \"player\": drink moves 0.5 m per tick, which must be less than the \
             capsule radius 0.35",
        ),
    ] {
        assert_eq!(error_of(player, children), message, "{player} {children}");
    }
    // At the edges; a grandchild named flask is not the flask.
    for (player, children) in [
        (r#"("flask_heal_tick", 60), ("flask_heal", 1)"#, FLASK),
        (r#"("flask_charges", 0)"#, FLASK),
        ("", r#"(name: "hand", children: [(name: "flask")]),"#),
    ] {
        assert!(
            Game::from_source(&scene_with(player, children, "")).is_ok(),
            "{player} {children}"
        );
    }
}
