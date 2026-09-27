//! Tests of souls, the bloodstain and picking it back up
//! (`docs/architecture.md` section 11, "魂"), a child of `game` so they can
//! read and set its own fields. The character stands at the origin facing
//! -Z with the camera behind it; the defaults hold unless a test says
//! otherwise: a bloodstain range of 1.5 m.

use glam::Vec3;
use loom_core::debug::DebugDraw;
use loom_core::input::{ActionState, TickInput};

use super::debug::DEBUG_BLOODSTAIN;
use super::*;
use crate::events::{HitAttack, Outcome};

/// The bloodstain marker of the scene files.
const MARKER: &str = r#"(name: "bloodstain", position: (0.0, 0.05, 0.0), scale: (0.4, 0.1, 0.4),
    mesh: Cube, color: (0.3, 0.9, 0.5)),"#;

/// The character with `player` props, then `nodes`.
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

/// A dummy 1.5 m ahead that one light attack kills, reviving 5 ticks
/// later, with `souls` (none written when `None`).
fn dummy(souls: Option<u32>) -> String {
    let souls = souls.map_or(String::new(), |s| format!(r#", ("souls", {s})"#));
    format!(
        r#"(name: "d", position: (0.0, 0.0, -1.5), props: [("lock_height", 1.0),
        ("hurt_radius", 0.4), ("hurt_height", 1.8), ("hp", 10), ("revive_ticks", 5){souls}]),"#
    )
}

/// A bonfire 1 m ahead, in reach.
const BONFIRE: &str = r#"(name: "bonfire", position: (0.0, 0.0, -1.0),
    props: [("bonfire_range", 1.5)]),"#;

/// A stake 2 m ahead swinging all around once, killing the character on
/// tick `at`.
fn lethal_stake(at: u32) -> String {
    format!(
        r#"(name: "stake", position: (0.0, 0.0, 2.0), props: [("swing_damage", 1000),
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

    fn idle(&mut self) -> GameState {
        self.hold(&[], &[])
    }

    /// A tick pressing the interact key (after a tick without it).
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

    /// Kills the character where it stands, as a swinger's hit does
    /// (`player_hit`), outside a tick; the events of that hit.
    fn kill_character(&mut self) -> Vec<Event> {
        self.game.events.clear();
        let mut player = self.game.state.player.clone();
        self.game.player_hit(
            &mut player,
            Vec3::new(0.0, 0.0, 5.0),
            "s".to_owned(),
            1000.0,
            0.0,
            HitAttack::Swing,
        );
        self.game.state.player = player;
        self.events()
    }

    /// Light attacks until the dummy dies; the events of that tick.
    fn kill_dummy(&mut self) -> Vec<Event> {
        for _ in 0..200 {
            let s = self.hold(&[ATTACK], &[]);
            if self.events().iter().any(|e| {
                matches!(
                    e,
                    Event::Hit {
                        result: Outcome::Death,
                        ..
                    }
                )
            }) {
                return self.events();
            }
            if s.player.action == Action::Idle {
                self.idle();
            }
        }
        panic!("the dummy never died");
    }

    fn set_bloodstain(&mut self, position: Vec3, souls: u32) {
        self.game.bloodstain = Some(BloodstainState { position, souls });
    }

    fn marker(&self) -> (bool, Vec3) {
        let scene = self.game.scene();
        let id = scene.find("bloodstain").unwrap();
        (scene.is_drawn(id), scene.transform(id).position)
    }
}

fn death_hit() -> impl Fn(&Event) -> bool {
    |e| {
        matches!(
            e,
            Event::Hit {
                result: Outcome::Death,
                ..
            }
        )
    }
}

// ------------------------------------------------------------------ getting

#[test]
fn killing_a_dummy_with_souls_gives_them_right_after_the_death_hit() {
    let mut run = Run::new("", &dummy(Some(50)));
    let events = run.kill_dummy();
    let at = events.iter().position(death_hit()).unwrap();
    assert_eq!(
        events[at + 1],
        Event::Souls {
            from: "d".to_owned(),
            amount: 50,
            total: 50,
        }
    );
    assert_eq!(run.game.state().player.souls, 50);
}

#[test]
fn killing_the_dummy_again_after_it_revives_gives_its_souls_again() {
    let mut run = Run::new("", &dummy(Some(50)));
    run.kill_dummy();
    let mut s = run.idle();
    while run.game.hurtboxes[0].health.unwrap().is_dead() {
        s = run.idle();
    }
    assert_eq!(s.player.souls, 50);
    let events = run.kill_dummy();
    assert!(events.contains(&Event::Souls {
        from: "d".to_owned(),
        amount: 50,
        total: 100,
    }));
    assert_eq!(run.game.state().player.souls, 100);
}

#[test]
fn killing_a_dummy_without_souls_gives_nothing() {
    let mut run = Run::new("", &dummy(None));
    let events = run.kill_dummy();
    assert!(!events.iter().any(|e| matches!(e, Event::Souls { .. })));
    assert_eq!(run.game.state().player.souls, 0);
}

#[test]
fn hurting_a_dummy_without_killing_it_gives_nothing() {
    let mut run = Run::new(
        "",
        &dummy(Some(50)).replace(r#"("hp", 10)"#, r#"("hp", 100)"#),
    );
    let mut hit = false;
    for _ in 0..60 {
        run.hold(&[ATTACK], &[]);
        hit |= run.events().iter().any(|e| matches!(e, Event::Hit { .. }));
        assert!(
            !run.events()
                .iter()
                .any(|e| matches!(e, Event::Souls { .. }))
        );
        run.idle();
    }
    assert!(hit);
    assert_eq!(run.game.state().player.souls, 0);
}

// ----------------------------------------------------------------- dropping

#[test]
fn death_with_souls_drops_them_into_a_bloodstain_where_the_character_died() {
    let mut run = Run::new("", &lethal_stake(10));
    let at = Vec3::new(1.0, 0.0, -2.0);
    run.game.state.player.position = at;
    run.game.state.player.souls = 50;
    let mut s = run.idle();
    while s.player.hits == 0 {
        s = run.idle();
    }
    assert_eq!(s.player.health.state, Condition::Dead);
    assert!(run.events().contains(&Event::SoulsDropped {
        position: at,
        amount: 50,
        lost: 0,
    }));
    assert_eq!(s.player.souls, 0);
    assert_eq!(
        s.bloodstain,
        Some(BloodstainState {
            position: at,
            souls: 50,
        })
    );
}

#[test]
fn second_death_before_picking_up_loses_the_old_bloodstain() {
    let mut run = Run::new("", "");
    run.set_bloodstain(Vec3::new(3.0, 0.0, 0.0), 40);
    run.game.state.player.souls = 25;
    let events = run.kill_character();
    assert_eq!(
        events.last(),
        Some(&Event::SoulsDropped {
            position: Vec3::ZERO,
            amount: 25,
            lost: 40,
        })
    );
    assert_eq!(
        run.game.state().bloodstain,
        Some(BloodstainState {
            position: Vec3::ZERO,
            souls: 25,
        })
    );
}

#[test]
fn death_without_souls_clears_the_old_bloodstain() {
    let mut run = Run::new("", "");
    run.set_bloodstain(Vec3::new(3.0, 0.0, 0.0), 40);
    let events = run.kill_character();
    assert_eq!(
        events.last(),
        Some(&Event::SoulsDropped {
            position: Vec3::ZERO,
            amount: 0,
            lost: 40,
        })
    );
    assert_eq!(run.game.state().bloodstain, None);
}

#[test]
fn death_without_souls_or_bloodstain_sends_no_souls_event() {
    let mut run = Run::new("", "");
    let events = run.kill_character();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::SoulsDropped { .. }))
    );
    assert_eq!(run.game.state().bloodstain, None);
}

#[test]
fn souls_dropped_comes_after_the_hit_the_interrupted_action_and_the_lock() {
    let mut run = Run::new("", &format!("{}{}", dummy(None), lethal_stake(10)));
    run.game.state.player.souls = 30;
    run.hold(&[LOCK], &[]);
    assert!(run.game.lock.is_some());
    run.hold(&[DRINK], &[]);
    let mut s = run.idle();
    while s.player.hits == 0 {
        s = run.idle();
    }
    let events = run.events();
    assert!(matches!(
        events[0],
        Event::Hit {
            result: Outcome::Death,
            ..
        }
    ));
    assert_eq!(
        events[1..],
        [
            Event::ActionEnd {
                action: Action::Drink,
                reason: EndReason::Interrupted,
            },
            Event::Lock {
                target: None,
                reason: LockReason::Died,
            },
            Event::SoulsDropped {
                position: Vec3::ZERO,
                amount: 30,
                lost: 0,
            },
        ]
    );
}

// ---------------------------------------------------------------- recovering

#[test]
fn interact_within_the_bloodstain_range_picks_its_souls_back_up() {
    let mut run = Run::new("", "");
    run.game.state.player.souls = 5;
    // Exactly the range away.
    run.set_bloodstain(Vec3::new(0.0, 0.0, -1.5), 40);
    let s = run.interact();
    assert_eq!(
        run.events(),
        [Event::SoulsRecovered {
            amount: 40,
            total: 45,
        }]
    );
    assert_eq!((s.player.souls, s.bloodstain), (45, None));
    assert_eq!(s.player.action, Action::Idle);
}

#[test]
fn interact_just_beyond_the_bloodstain_range_goes_on_to_the_bonfires() {
    let mut run = Run::new("", "");
    run.set_bloodstain(Vec3::new(0.0, 0.0, -1.51), 40);
    let s = run.interact();
    assert_eq!(run.dropped(), [(Input::Interact, DropReason::NoBonfire)]);
    assert_eq!(
        (s.player.souls, s.bloodstain.map(|b| b.souls)),
        (0, Some(40))
    );
}

#[test]
fn bloodstain_range_follows_the_prop() {
    let mut run = Run::new(r#"("bloodstain_range", 3)"#, "");
    run.set_bloodstain(Vec3::new(0.0, 0.0, -3.0), 40);
    assert_eq!(run.interact().player.souls, 40);
}

#[test]
fn picking_up_while_sprinting_keeps_sprinting() {
    let mut run = Run::new("", "");
    for _ in 0..20 {
        run.hold(&[ROLL], &[(MOVE_Y, 1.0)]);
    }
    let s = run.game.state();
    assert_eq!(s.player.action, Action::Sprint);
    run.set_bloodstain(s.player.position, 40);
    let s = run.hold(&[ROLL, INTERACT], &[(MOVE_Y, 1.0)]);
    assert_eq!(
        run.events(),
        [Event::SoulsRecovered {
            amount: 40,
            total: 40,
        }]
    );
    assert_eq!(s.player.action, Action::Sprint);
}

#[test]
fn picking_up_ignores_a_living_enemy_in_its_aggro_range() {
    // As in the bonfire tests: an enemy 5 m away, aggro range 5.
    let enemy = r#"(name: "e", position: (0.0, 0.0, 5.0), props: [("lock_height", 1),
        ("hurt_radius", 0.5), ("hurt_height", 2), ("hp", 10), ("enemy_speed", 0.6),
        ("enemy_aggro_range", 5.0), ("enemy_face_angle", 1),
        ("approach_weight_per_meter", 0), ("lunge_distance", 0.2)]),"#;
    let mut run = Run::new("", enemy);
    run.set_bloodstain(Vec3::ZERO, 40);
    assert_eq!(run.interact().player.souls, 40);
}

#[test]
fn bloodstain_and_bonfire_in_reach_pick_up_first_then_sit() {
    let mut run = Run::new("", BONFIRE);
    run.set_bloodstain(Vec3::ZERO, 40);
    let s = run.interact();
    assert_eq!((s.player.souls, s.player.action), (40, Action::Idle));
    run.idle();
    let s = run.interact();
    assert_eq!(s.player.action, Action::Sit);
}

#[test]
fn interact_near_the_bloodstain_while_drinking_staggered_or_dead_is_dropped_as_before() {
    let mut run = Run::new("", "");
    run.set_bloodstain(Vec3::ZERO, 40);
    run.hold(&[DRINK], &[]);
    run.interact();
    assert_eq!(run.dropped(), [(Input::Interact, DropReason::Drinking)]);

    let mut run = Run::new("", "");
    run.set_bloodstain(Vec3::ZERO, 40);
    run.game.health.take(1.0, 1.0, 0.0);
    run.interact();
    assert_eq!(run.dropped(), [(Input::Interact, DropReason::Staggered)]);

    let mut run = Run::new("", "");
    run.set_bloodstain(Vec3::ZERO, 40);
    run.game.health.take(1000.0, 0.0, 0.0);
    let s = run.interact();
    assert_eq!(run.dropped(), [(Input::Interact, DropReason::Dead)]);
    assert_eq!(s.bloodstain.map(|b| b.souls), Some(40));
}

// ------------------------------------------------------------- world resets

#[test]
fn sitting_at_a_bonfire_keeps_the_souls_and_the_bloodstain() {
    let mut run = Run::new("", BONFIRE);
    run.game.state.player.souls = 20;
    let stain = Vec3::new(5.0, 0.0, 0.0);
    run.set_bloodstain(stain, 40);
    let s = run.interact();
    assert_eq!(s.player.action, Action::Sit);
    assert_eq!(s.player.souls, 20);
    assert_eq!(
        s.bloodstain,
        Some(BloodstainState {
            position: stain,
            souls: 40,
        })
    );
}

#[test]
fn respawn_keeps_the_bloodstain_and_comes_back_without_souls() {
    let mut run = Run::new(r#"("respawn_ticks", 3)"#, "");
    run.game.state.player.position = Vec3::new(2.0, 0.0, 0.0);
    run.game.state.player.souls = 20;
    run.kill_character();
    let mut s = run.idle();
    while !run
        .events()
        .iter()
        .any(|e| matches!(e, Event::Respawn { .. }))
    {
        s = run.idle();
    }
    assert_eq!(s.player.position, Vec3::ZERO);
    assert_eq!(s.player.souls, 0);
    assert_eq!(
        s.bloodstain,
        Some(BloodstainState {
            position: Vec3::new(2.0, 0.0, 0.0),
            souls: 20,
        })
    );
}

// -------------------------------------------------------------- presentation

#[test]
fn marker_is_drawn_above_the_bloodstain_only_while_there_is_one() {
    let mut run = Run::new(r#"("respawn_ticks", 3)"#, MARKER);
    assert_eq!(run.marker(), (false, Vec3::new(0.0, 0.05, 0.0)));
    run.idle();
    assert!(!run.marker().0);
    let at = Vec3::new(1.0, 0.0, 2.0);
    run.game.state.player.position = at;
    run.game.state.player.souls = 20;
    run.kill_character();
    run.idle();
    assert_eq!(run.marker(), (true, at + Vec3::new(0.0, 0.05, 0.0)));
    // Back at the spawn point, 2.2 m away: walk up to it and pick it up.
    let mut s = run.idle();
    while s.player.position != Vec3::ZERO || s.player.health.state != Condition::Ok {
        s = run.idle();
    }
    run.game.state.player.position = at;
    run.interact();
    assert!(!run.marker().0);
}

#[test]
fn character_drops_and_picks_up_souls_without_a_marker_node() {
    let mut run = Run::new("", "");
    run.game.state.player.souls = 20;
    run.kill_character();
    assert_eq!(run.game.state().bloodstain.map(|b| b.souls), Some(20));
    run.game.health = run.game.initial.health;
    let s = run.interact();
    assert_eq!((s.player.souls, s.bloodstain), (20, None));
}

#[test]
fn debug_bloodstain_draws_the_range_around_it_only_while_there_is_one() {
    let mut run = Run::new("", "");
    let lines = |run: &Run| {
        let mut draw = DebugDraw::new();
        run.game.debug_draw(&["bloodstain"], &mut draw);
        draw.lines().to_vec()
    };
    assert!(lines(&run).is_empty());
    let at = Vec3::new(1.0, 0.0, 2.0);
    run.set_bloodstain(at, 5);
    let circle = lines(&run);
    assert!(!circle.is_empty());
    for l in &circle {
        assert_eq!(l.color, DEBUG_BLOODSTAIN);
        for p in [l.a, l.b] {
            let d = p - at;
            assert!((d.length() - 1.5).abs() < 1e-4 && d.y == 0.0, "{p}");
        }
    }
}

// ---------------------------------------------------------------- validation

#[test]
fn souls_prop_and_bloodstain_marker_are_checked_in_order_with_exact_messages() {
    let error_of = |player: &str, nodes: &str| match Game::from_source(&scene(player, nodes)) {
        Ok(_) => panic!("the scene should be rejected: {player} {nodes}"),
        Err(e) => e.to_string(),
    };
    for (player, nodes, message) in [
        (
            "",
            r#"(name: "t", props: [("souls", 1.5)]),"#,
            "the node \"t\": prop \"souls\" must be a whole number of at least 0, got 1.5",
        ),
        (
            "",
            r#"(name: "t", props: [("souls", -1)]),"#,
            "the node \"t\": prop \"souls\" must be a whole number of at least 0, got -1",
        ),
        (
            "",
            r#"(name: "t", props: [("souls", 1)]),"#,
            "the node \"t\": prop \"souls\" needs hp",
        ),
        // After the four before it, whatever the order written.
        (
            "",
            r#"(name: "t", props: [("souls", 1), ("revive_ticks", 1)]),"#,
            "the node \"t\": prop \"revive_ticks\" needs hp",
        ),
        (
            "",
            r#"(name: "bloodstain", position: (0.0, 0.05, 0.0)),"#,
            "the bloodstain marker \"bloodstain\" needs a mesh, and no props or children",
        ),
        (
            "",
            r#"(name: "bloodstain", mesh: Cube, props: [("bonfire_range", 1)]),"#,
            "the bloodstain marker \"bloodstain\" needs a mesh, and no props or children",
        ),
        (
            "",
            r#"(name: "bloodstain", mesh: Cube, children: [(name: "x")]),"#,
            "the bloodstain marker \"bloodstain\" needs a mesh, and no props or children",
        ),
        // The props' own checks first.
        (
            "",
            r#"(name: "bloodstain", props: [("souls", 1)]),"#,
            "the node \"bloodstain\": prop \"souls\" needs hp",
        ),
        (
            r#"("bloodstain_range", 0)"#,
            "",
            "the character \"player\": prop \"bloodstain_range\" must be greater than 0, got 0",
        ),
    ] {
        assert_eq!(error_of(player, nodes), message, "{player} {nodes}");
    }
    // A hurtbox with souls and hp, a bloodstain that is not a root node, and
    // a marker with the scene files' shape all load.
    for nodes in [
        r#"(name: "t", props: [("hurt_radius", 1), ("hurt_height", 1), ("hp", 1),
            ("souls", 0)]),"#,
        r#"(name: "g", children: [(name: "bloodstain")]),"#,
        MARKER,
    ] {
        assert!(Game::from_source(&scene("", nodes)).is_ok(), "{nodes}");
    }
}
