//! Stamina, roll, sprint and backstep: the parameters, read from the
//! character node's props with defaults, and the action state
//! (`docs/architecture.md` section 11, "耐力、翻滚、冲刺").

use loom_scene::{NodeId, Scene};
use serde::Serialize;

use crate::GameError;

/// What the character is doing; serialized in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Idle,
    Sprint,
    Roll,
    Backstep,
}

/// How a prop's value is checked.
#[derive(Debug, Clone, Copy)]
enum Rule {
    /// Greater than 0.
    Positive,
    /// A whole number of at least this.
    AtLeast(u32),
}

/// Every prop the character understands, with its default and rule, in
/// the order they are checked. Reading, checking and rejecting unknown
/// names all go through this one table.
const PARAMS: [(&str, f64, Rule); 16] = [
    ("stamina_max", 100.0, Rule::Positive),
    ("stamina_regen", 45.0, Rule::Positive),
    ("stamina_delay_ticks", 30.0, Rule::AtLeast(0)),
    ("tap_ticks", 15.0, Rule::AtLeast(1)),
    ("sprint_speed", 6.0, Rule::Positive),
    ("sprint_cost", 15.0, Rule::Positive),
    ("roll_cost", 20.0, Rule::Positive),
    ("roll_ticks", 32.0, Rule::AtLeast(1)),
    ("roll_distance", 3.2, Rule::Positive),
    ("roll_invuln_from", 1.0, Rule::AtLeast(1)),
    ("roll_invuln_to", 20.0, Rule::AtLeast(1)),
    ("backstep_cost", 12.0, Rule::Positive),
    ("backstep_ticks", 20.0, Rule::AtLeast(1)),
    ("backstep_distance", 1.5, Rule::Positive),
    ("backstep_invuln_from", 1.0, Rule::AtLeast(1)),
    ("backstep_invuln_to", 8.0, Rule::AtLeast(1)),
];

/// A roll or a backstep, in per-tick units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Move {
    pub cost: f32,
    pub ticks: u32,
    /// Meters per tick: `distance / ticks`.
    pub step: f32,
    /// Invulnerable while `invuln_from <= action_tick <= invuln_to`.
    pub invuln_from: u32,
    pub invuln_to: u32,
}

/// The parameters in per-tick units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Params {
    pub stamina_max: f32,
    /// Stamina per tick: `stamina_regen / 60`.
    pub regen_step: f32,
    pub delay_ticks: u32,
    pub tap_ticks: u32,
    /// Meters per tick: `sprint_speed / 60`.
    pub sprint_step: f32,
    /// Stamina per tick: `sprint_cost / 60`.
    pub sprint_drain: f32,
    pub roll: Move,
    pub backstep: Move,
}

impl Params {
    /// Reads and checks the props of the character `id`, whose capsule has
    /// `radius`: unknown names (in file order), then each value (in table
    /// order), then the invulnerability windows (roll, backstep), then the
    /// per-tick distances (sprint, roll, backstep).
    pub fn new(scene: &Scene, id: NodeId, radius: f32) -> Result<Self, GameError> {
        let node = scene.node(id);
        let character = scene.path(id);
        if let Some((prop, _)) = node
            .props()
            .find(|(p, _)| !PARAMS.iter().any(|(name, ..)| name == p))
        {
            return Err(GameError::UnknownProp {
                character,
                prop: prop.to_owned(),
            });
        }
        let mut values = [0.0; PARAMS.len()];
        for (value, &(prop, default, rule)) in values.iter_mut().zip(&PARAMS) {
            let v = node.prop(prop).unwrap_or(default);
            match rule {
                Rule::Positive if v <= 0.0 => {
                    return Err(GameError::NotPositive {
                        character,
                        prop,
                        value: v,
                    });
                }
                Rule::AtLeast(min) if v.fract() != 0.0 || v < f64::from(min) => {
                    return Err(GameError::NotWhole {
                        character,
                        prop,
                        min,
                        value: v,
                    });
                }
                _ => *value = v,
            }
        }
        let get = |prop: &str| {
            let i = PARAMS
                .iter()
                .position(|(name, ..)| *name == prop)
                .expect("only names from PARAMS are asked for");
            values[i]
        };
        // Checked above to be whole and not negative; `as` saturates.
        let ticks = |prop: &str| get(prop) as u32;
        let movement = |action: &str| {
            let n = ticks(&format!("{action}_ticks"));
            Move {
                cost: get(&format!("{action}_cost")) as f32,
                ticks: n,
                step: get(&format!("{action}_distance")) as f32 / n as f32,
                invuln_from: ticks(&format!("{action}_invuln_from")),
                invuln_to: ticks(&format!("{action}_invuln_to")),
            }
        };
        let params = Self {
            stamina_max: get("stamina_max") as f32,
            regen_step: get("stamina_regen") as f32 / 60.0,
            delay_ticks: ticks("stamina_delay_ticks"),
            tap_ticks: ticks("tap_ticks"),
            sprint_step: get("sprint_speed") as f32 / 60.0,
            sprint_drain: get("sprint_cost") as f32 / 60.0,
            roll: movement("roll"),
            backstep: movement("backstep"),
        };
        for (action, m) in [("roll", params.roll), ("backstep", params.backstep)] {
            if !(m.invuln_from <= m.invuln_to && m.invuln_to <= m.ticks) {
                return Err(GameError::InvulnWindow { character, action });
            }
        }
        for (action, step) in [
            ("sprint", params.sprint_step),
            ("roll", params.roll.step),
            ("backstep", params.backstep.step),
        ] {
            if step >= radius {
                return Err(GameError::StepTooLong {
                    character,
                    action,
                    step,
                    radius,
                });
            }
        }
        Ok(params)
    }

    /// The roll or backstep `action`.
    pub fn movement(&self, action: Action) -> Move {
        match action {
            Action::Roll => self.roll,
            Action::Backstep => self.backstep,
            Action::Idle | Action::Sprint => {
                unreachable!("only rolls and backsteps are timed moves")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;
    use loom_core::input::{ActionState, TickInput};

    use super::*;
    use crate::{Game, GameState, LOOK_X, MOVE_X, MOVE_Y, ROLL};

    /// Props of the test scene: every per-tick amount is a power-of-two
    /// fraction, so positions and stamina are exact. Per tick: run 0.0625,
    /// roll 3 / 24 = 0.125, backstep 2 / 16 = 0.125, sprint 7.5 / 60 =
    /// 0.125 costing 30 / 60 = 0.5, regen 45 / 60 = 0.75. The rest are
    /// defaults: stamina 100, delay 30, tap 15, roll 20, backstep 12,
    /// invulnerable on roll ticks 1-20 and backstep ticks 1-8.
    const PROPS: &str = r#"("roll_distance", 3.0), ("roll_ticks", 24),
        ("backstep_distance", 2.0), ("backstep_ticks", 16),
        ("sprint_speed", 7.5), ("sprint_cost", 30), ("stamina_regen", 45)"#;

    /// A character at the origin facing -Z, the camera behind it with yaw
    /// and pitch 0, no colliders; `props` on the character.
    fn scene(props: &str) -> String {
        format!(
            r#"(
    version: 1,
    game: "souls",
    camera: "cam",
    ambient: (0.2, 0.2, 0.2),
    nodes: [
        (name: "player", mesh: Capsule(radius: 0.35, height: 1.8),
            character: (run_speed: 3.75, turn_speed: 720.0), props: [{props}]),
        (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0),
            orbit_camera: (target: "player", pivot_height: 1.5, distance: 4.0,
                yaw: 0.0, pitch: 0.0, pitch_min: -60.0, pitch_max: 60.0,
                collision_radius: 0.2, return_speed: 4.0)),
    ],
)"#
        )
    }

    /// A game and its persistent input, one tick at a time.
    struct Run {
        game: Game,
        input: ActionState,
    }

    impl Run {
        fn new(props: &str) -> Self {
            Self {
                game: Game::from_source(&scene(props)).unwrap(),
                input: ActionState::new(),
            }
        }

        /// One tick with `roll` held or not and `axes` pushed.
        fn tick(&mut self, roll: bool, axes: &[(&str, f32)]) -> GameState {
            let actions = if roll {
                [ROLL.to_owned()].into()
            } else {
                Default::default()
            };
            self.input.advance(TickInput {
                actions,
                axes: axes.iter().map(|&(a, v)| (a.to_owned(), v)).collect(),
            });
            self.game.tick(&self.input);
            self.game.state()
        }

        /// `n` ticks as [`Run::tick`]; returns the last state.
        fn ticks(&mut self, n: u32, roll: bool, axes: &[(&str, f32)]) -> GameState {
            let mut state = self.game.state();
            for _ in 0..n {
                state = self.tick(roll, axes);
            }
            state
        }
    }

    const FORWARD: [(&str, f32); 1] = [(MOVE_Y, 1.0)];
    const NONE: [(&str, f32); 0] = [];

    fn error_of(props: &str) -> String {
        match Game::from_source(&scene(props)) {
            Ok(_) => panic!("props should be rejected: {props}"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn tapping_roll_while_running_forward_rolls_three_meters() {
        let mut run = Run::new(PROPS);
        // Tick 1: pressed, so not a tap yet: an ordinary run step.
        let s = run.tick(true, &FORWARD);
        assert_eq!(s.player.position, Vec3::new(0.0, 0.0, -0.0625));
        assert_eq!(s.player.action, Action::Idle);
        // Tick 2: released after one tick: a tap. The roll starts and moves.
        let s = run.tick(false, &FORWARD);
        assert_eq!(s.player.action, Action::Roll);
        assert_eq!(s.player.action_tick, 1);
        assert_eq!(s.player.position.z, -0.1875);
        assert_eq!(s.player.stamina, 80.0);
        assert_eq!(s.player.stamina_wait, 30);
        // Ticks 3..=25: 23 more roll ticks at 0.125 each; the last one ends it.
        let s = run.ticks(23, false, &FORWARD);
        assert_eq!(s.player.position, Vec3::new(0.0, 0.0, -3.0625));
        assert_eq!(s.player.action, Action::Idle);
        assert_eq!(s.player.action_tick, 0);
        assert_eq!(s.player.facing.to_bits(), 0.0_f32.to_bits());
        assert_eq!(s.player.stamina, 80.0);
    }

    #[test]
    fn roll_is_invulnerable_exactly_on_its_window_ticks() {
        let mut run = Run::new(PROPS);
        run.tick(true, &FORWARD);
        // Roll ticks 1..=24 happen on game ticks 2..=25.
        for roll_tick in 1..=24 {
            let s = run.tick(false, &FORWARD);
            if roll_tick < 24 {
                assert_eq!(s.player.action_tick, roll_tick);
                assert_eq!(s.player.invulnerable, roll_tick <= 20, "{roll_tick}");
            } else {
                // The last tick already counts as idle.
                assert_eq!(s.player.action, Action::Idle);
                assert!(!s.player.invulnerable);
            }
        }
    }

    #[test]
    fn stamina_waits_out_the_delay_after_a_roll_then_regenerates() {
        let mut run = Run::new(PROPS);
        run.tick(true, &FORWARD);
        run.tick(false, &FORWARD); // tick 2: the tap starts a roll
        let s = run.ticks(23, false, &NONE); // ticks 3..=25: the roll, no countdown
        assert_eq!((s.player.stamina, s.player.stamina_wait), (80.0, 30));
        // Ticks 26..=55 count the 30 ticks down.
        let s = run.ticks(30, false, &NONE);
        assert_eq!((s.player.stamina, s.player.stamina_wait), (80.0, 0));
        // Tick 56 is the first to regenerate.
        let s = run.tick(false, &NONE);
        assert_eq!(s.player.stamina, 80.75);
        assert_eq!(run.game.clock().tick(), 56);
        // 26 more ticks would give 100.25: capped at 100.
        let s = run.ticks(26, false, &NONE);
        assert_eq!(s.player.stamina, 100.0);
    }

    #[test]
    fn tapping_roll_without_a_direction_backsteps() {
        let mut run = Run::new(PROPS);
        run.tick(true, &NONE);
        let s = run.tick(false, &NONE);
        assert_eq!(s.player.action, Action::Backstep);
        assert_eq!(s.player.stamina, 88.0);
        // Backstep ticks 1..=16 on game ticks 2..=17, 0.125 towards +Z.
        let s = run.ticks(15, false, &NONE);
        assert_eq!(s.player.position, Vec3::new(0.0, 0.0, 2.0));
        assert_eq!(s.player.action, Action::Idle);
        assert_eq!(s.player.facing.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn backstep_is_invulnerable_on_its_first_eight_ticks() {
        let mut run = Run::new(PROPS);
        run.tick(true, &NONE);
        for backstep_tick in 1..=15 {
            let s = run.tick(false, &NONE);
            assert_eq!(s.player.invulnerable, backstep_tick <= 8, "{backstep_tick}");
        }
    }

    #[test]
    fn holding_roll_with_a_direction_sprints_after_the_tap_window() {
        let mut run = Run::new(PROPS);
        // Ticks 1..=15: held, but still within the tap window: running.
        let s = run.ticks(15, true, &FORWARD);
        assert_eq!(s.player.action, Action::Idle);
        assert_eq!(s.player.position.z, -0.9375);
        assert_eq!(s.player.stamina, 100.0);
        // Ticks 16..=20: sprinting at 0.125, draining 0.5 per tick.
        let s = run.ticks(5, true, &FORWARD);
        assert_eq!(s.player.action, Action::Sprint);
        assert_eq!(s.player.position.z, -1.5625);
        assert_eq!(s.player.stamina, 97.5);
        assert_eq!(s.player.stamina_wait, 30);
        // Releasing after a sprint is no tap: back to running, no roll.
        let s = run.tick(false, &FORWARD);
        assert_eq!(s.player.action, Action::Idle);
        assert_eq!(s.player.position.z, -1.625);
    }

    #[test]
    fn holding_roll_without_a_direction_neither_sprints_nor_drains() {
        let mut run = Run::new(PROPS);
        let s = run.ticks(40, true, &NONE);
        assert_eq!(s.player.action, Action::Idle);
        assert_eq!(s.player.stamina, 100.0);
        assert_eq!(s.player.position, Vec3::ZERO);
        // Releasing after holding that long is no tap either.
        let s = run.tick(false, &NONE);
        assert_eq!(s.player.action, Action::Idle);
    }

    #[test]
    fn sprinting_stamina_away_stops_the_sprint_until_roll_is_pressed_again() {
        // 5 stamina: 10 sprint ticks at 0.5.
        let mut run = Run::new(&format!(r#"{PROPS}, ("stamina_max", 5)"#));
        run.ticks(15, true, &FORWARD);
        let s = run.ticks(10, true, &FORWARD); // ticks 16..=25
        assert_eq!((s.player.action, s.player.stamina), (Action::Sprint, 0.0));
        // Still held: stamina comes back after the delay (ticks 26..=55
        // count down, 56..=62 regenerate to the cap), but no sprint.
        let s = run.ticks(37, true, &FORWARD); // ticks 26..=62
        assert_eq!(s.player.action, Action::Idle);
        assert_eq!(s.player.stamina, 5.0);
        assert_eq!(
            s.player.position.z,
            -(15.0 * 0.0625 + 10.0 * 0.125 + 37.0 * 0.0625)
        );
        // Released and pressed again: sprinting once held past the window.
        run.tick(false, &FORWARD);
        let s = run.ticks(15, true, &FORWARD);
        assert_eq!(s.player.action, Action::Idle);
        let s = run.tick(true, &FORWARD);
        assert_eq!(s.player.action, Action::Sprint);
        assert_eq!(s.player.stamina, 4.5);
    }

    #[test]
    fn tapping_roll_with_no_stamina_just_runs() {
        let mut run = Run::new(&format!(r#"{PROPS}, ("stamina_max", 5)"#));
        run.ticks(25, true, &FORWARD); // sprints the stamina away by tick 25
        let before = run.tick(false, &FORWARD); // tick 26: release, no tap (held 16)
        assert_eq!(before.player.stamina, 0.0);
        run.tick(true, &FORWARD); // tick 27
        let s = run.tick(false, &FORWARD); // tick 28: a tap, but no stamina
        assert_eq!(s.player.action, Action::Idle);
        assert_eq!(s.player.stamina, 0.0);
        assert_eq!(s.player.position.z, before.player.position.z - 2.0 * 0.0625);
    }

    #[test]
    fn roll_with_less_stamina_than_its_cost_still_happens_and_stops_at_zero() {
        let mut run = Run::new(&format!(r#"{PROPS}, ("stamina_max", 10)"#));
        run.tick(true, &FORWARD);
        let s = run.tick(false, &FORWARD);
        assert_eq!(s.player.action, Action::Roll);
        assert_eq!(s.player.stamina, 0.0);
    }

    #[test]
    fn during_a_roll_taps_and_steering_are_ignored_but_the_camera_turns() {
        let mut run = Run::new(PROPS);
        run.tick(true, &FORWARD);
        run.tick(false, &FORWARD); // tick 2: roll tick 1
        // Ticks 3..=24: steer right, look left, tap roll twice.
        let steer = [(MOVE_X, 1.0), (LOOK_X, 0.25)];
        for t in 3..=24 {
            run.tick(t == 5 || t == 10, &steer);
        }
        let s = run.tick(false, &NONE); // tick 25: the roll's last tick
        assert_eq!(s.player.position, Vec3::new(0.0, 0.0, -3.0625));
        assert_eq!(s.player.facing.to_bits(), 0.0_f32.to_bits());
        assert_eq!(s.player.stamina, 80.0);
        assert_ne!(s.camera.yaw, 0.0);
        // The taps were dropped: nothing starts once the roll is over.
        let s = run.tick(false, &NONE);
        assert_eq!(s.player.action, Action::Idle);
    }

    #[test]
    fn single_tick_press_counts_as_a_tap() {
        // Pressed and released between two samples: pressed for one tick.
        let mut run = Run::new(PROPS);
        run.tick(true, &NONE);
        assert_eq!(run.tick(false, &NONE).player.action, Action::Backstep);
    }

    #[test]
    fn leaving_out_every_prop_is_the_same_as_writing_the_defaults() {
        let with = include_str!("demo.scene.ron");
        let start = with.find("            props: [").unwrap();
        let end =
            with[start..].find("            ],\n").unwrap() + start + "            ],\n".len();
        let without = format!("{}{}", &with[..start], &with[end..]);
        assert!(!without.contains("stamina_max"));
        let mut a = Run {
            game: Game::from_source(with).unwrap(),
            input: ActionState::new(),
        };
        let mut b = Run {
            game: Game::from_source(&without).unwrap(),
            input: ActionState::new(),
        };
        for t in 1..=120 {
            let roll = matches!(t, 1 | 60..=90);
            let axes = if t < 40 {
                &FORWARD[..]
            } else {
                &[(MOVE_X, -1.0)][..]
            };
            assert_eq!(a.tick(roll, axes), b.tick(roll, axes), "tick {t}");
        }
    }

    // ------------------------------------------------------------ scene errors

    #[test]
    fn unknown_character_prop_is_rejected() {
        assert_eq!(
            error_of(r#"("roll_distanse", 3.0)"#),
            "the character \"player\": unknown prop \"roll_distanse\""
        );
    }

    #[test]
    fn prop_that_must_be_positive_is_rejected_at_zero() {
        assert_eq!(
            error_of(r#"("roll_cost", 0)"#),
            "the character \"player\": prop \"roll_cost\" must be greater than 0, got 0"
        );
        assert_eq!(
            error_of(r#"("sprint_speed", -1.5)"#),
            "the character \"player\": prop \"sprint_speed\" must be greater than 0, got -1.5"
        );
    }

    #[test]
    fn tick_count_prop_must_be_a_whole_number_of_at_least_its_minimum() {
        assert_eq!(
            error_of(r#"("tap_ticks", 0)"#),
            "the character \"player\": prop \"tap_ticks\" must be a whole number of at least 1, \
             got 0"
        );
        assert_eq!(
            error_of(r#"("roll_ticks", 2.5)"#),
            "the character \"player\": prop \"roll_ticks\" must be a whole number of at least 1, \
             got 2.5"
        );
        assert_eq!(
            error_of(r#"("stamina_delay_ticks", -1)"#),
            "the character \"player\": prop \"stamina_delay_ticks\" must be a whole number of at \
             least 0, got -1"
        );
        assert!(Game::from_source(&scene(r#"("stamina_delay_ticks", 0)"#)).is_ok());
    }

    #[test]
    fn invulnerability_window_must_lie_inside_the_move() {
        assert_eq!(
            error_of(r#"("roll_invuln_to", 33)"#),
            "the character \"player\": needs 1 <= roll_invuln_from <= roll_invuln_to <= \
             roll_ticks"
        );
        assert_eq!(
            error_of(r#"("backstep_invuln_from", 9)"#),
            "the character \"player\": needs 1 <= backstep_invuln_from <= backstep_invuln_to <= \
             backstep_ticks"
        );
    }

    #[test]
    fn move_faster_than_the_capsule_radius_per_tick_is_rejected() {
        assert_eq!(
            error_of(r#"("sprint_speed", 30)"#),
            "the character \"player\": sprint moves 0.5 m per tick, which must be less than the \
             capsule radius 0.35"
        );
        assert_eq!(
            error_of(r#"("roll_distance", 16)"#),
            "the character \"player\": roll moves 0.5 m per tick, which must be less than the \
             capsule radius 0.35"
        );
        assert_eq!(
            error_of(r#"("backstep_distance", 10)"#),
            "the character \"player\": backstep moves 0.5 m per tick, which must be less than the \
             capsule radius 0.35"
        );
    }

    #[test]
    fn character_prop_checks_run_in_the_documented_order() {
        // An unknown name comes before a bad value, even written after it.
        assert!(error_of(r#"("roll_cost", 0), ("rol_ticks", 3)"#).contains("unknown prop"));
        // Values go in table order: stamina_regen before roll_cost.
        assert!(error_of(r#"("roll_cost", 0), ("stamina_regen", 0)"#).contains("stamina_regen"));
        // A bad value before a bad window, a window before a distance.
        assert!(
            error_of(r#"("roll_invuln_to", 40), ("backstep_cost", 0)"#).contains("backstep_cost")
        );
        assert!(
            error_of(r#"("roll_invuln_to", 40), ("sprint_speed", 30)"#).contains("roll_invuln")
        );
        // Distances: sprint before roll.
        assert!(
            error_of(r#"("roll_distance", 16), ("sprint_speed", 30)"#).contains("sprint moves")
        );
    }
}
