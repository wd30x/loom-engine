//! Stamina, roll, sprint, backstep and attacks: the parameters, read from
//! the character node's props with defaults, and the action state
//! (`docs/architecture.md` section 11, "耐力、翻滚、冲刺" and "攻击").

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
    /// A light attack.
    Attack,
    /// A heavy attack.
    Heavy,
}

/// The part of an attack; serialized in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttackPhase {
    Startup,
    Active,
    Recovery,
}

/// An action pressed during an attack, to start once it is over;
/// serialized in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Buffered {
    Attack,
    Heavy,
    /// The roll key: a roll, or a backstep without a direction.
    Roll,
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
const PARAMS: [(&str, f64, Rule); 28] = [
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
    ("attack_cost", 15.0, Rule::Positive),
    ("attack_startup_ticks", 10.0, Rule::AtLeast(1)),
    ("attack_active_ticks", 4.0, Rule::AtLeast(1)),
    ("attack_recovery_ticks", 16.0, Rule::AtLeast(1)),
    ("attack_distance", 0.3, Rule::Positive),
    ("heavy_cost", 25.0, Rule::Positive),
    ("heavy_startup_ticks", 22.0, Rule::AtLeast(1)),
    ("heavy_active_ticks", 5.0, Rule::AtLeast(1)),
    ("heavy_recovery_ticks", 23.0, Rule::AtLeast(1)),
    ("heavy_distance", 0.6, Rule::Positive),
    ("attack_turn_speed", 180.0, Rule::Positive),
    ("combo_max", 3.0, Rule::AtLeast(1)),
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

/// A light or heavy attack, in per-tick units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Attack {
    pub cost: f32,
    pub startup: u32,
    pub active: u32,
    pub recovery: u32,
    /// Meters per active tick: `distance / active`.
    pub step: f32,
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
    pub attack: Attack,
    pub heavy: Attack,
    /// Radians per tick during startup: `attack_turn_speed.to_radians() / 60`.
    pub attack_turn_step: f32,
    pub combo_max: u32,
}

impl Params {
    /// Reads and checks the props of the character `id`, whose capsule has
    /// `radius`: unknown names (in file order), then each value (in table
    /// order), then the invulnerability windows (roll, backstep), then the
    /// per-tick distances (sprint, roll, backstep, attack, heavy).
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
        let attack = |action: &str| {
            let active = ticks(&format!("{action}_active_ticks"));
            Attack {
                cost: get(&format!("{action}_cost")) as f32,
                startup: ticks(&format!("{action}_startup_ticks")),
                active,
                recovery: ticks(&format!("{action}_recovery_ticks")),
                step: get(&format!("{action}_distance")) as f32 / active as f32,
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
            attack: attack("attack"),
            heavy: attack("heavy"),
            attack_turn_step: (get("attack_turn_speed") as f32).to_radians() / 60.0,
            combo_max: ticks("combo_max"),
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
            ("attack", params.attack.step),
            ("heavy", params.heavy.step),
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
            Action::Idle | Action::Sprint | Action::Attack | Action::Heavy => {
                unreachable!("only rolls and backsteps are moves")
            }
        }
    }

    /// The light or heavy attack `action`.
    pub fn attack_of(&self, action: Action) -> Attack {
        match action {
            Action::Attack => self.attack,
            Action::Heavy => self.heavy,
            Action::Idle | Action::Sprint | Action::Roll | Action::Backstep => {
                unreachable!("only light and heavy attacks are attacks")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;
    use loom_core::input::{ActionState, TickInput};

    use super::*;
    use crate::{ATTACK, Game, GameState, HEAVY, LOOK_X, MOVE_X, MOVE_Y, ROLL};

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
            let actions: &[&str] = if roll { &[ROLL] } else { &[] };
            self.hold(actions, axes)
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
        for t in 1..=200 {
            let actions: &[&str] = match t {
                1 | 60..=90 => &[ROLL],
                100 | 125 | 160 => &[ATTACK],
                140 => &[HEAVY],
                _ => &[],
            };
            let axes = if t < 40 {
                &FORWARD[..]
            } else {
                &[(MOVE_X, -1.0)][..]
            };
            let s = a.hold(actions, axes);
            assert_eq!(s, b.hold(actions, axes), "tick {t}");
            // The press on 125 was buffered: a second light attack.
            if t == 130 {
                assert_eq!((s.player.action, s.player.combo), (Action::Attack, 2));
            }
        }
        assert_eq!(a.game.state().player.action, Action::Heavy);
    }

    // ----------------------------------------------------------------- attacks

    /// [`PROPS`] plus attacks whose slides are exact: light 4 startup, 4
    /// active (0.5 / 4 = 0.125 per tick), 6 recovery, so ticks 1-4, 5-8,
    /// 9-14; heavy 6, 8 (1.0 / 8 = 0.125), 8, so ticks 1-6, 7-14, 15-22.
    /// Costs are the defaults: light 15, heavy 25; combo_max 3.
    fn attack_props() -> String {
        format!(
            r#"{PROPS}, ("attack_startup_ticks", 4), ("attack_active_ticks", 4),
            ("attack_recovery_ticks", 6), ("attack_distance", 0.5),
            ("heavy_startup_ticks", 6), ("heavy_active_ticks", 8),
            ("heavy_recovery_ticks", 8), ("heavy_distance", 1.0)"#
        )
    }

    /// Presses `action` on the ticks in `presses` (1-based, counting from
    /// the run's current tick) and nothing else, for `ticks` ticks with
    /// `axes`; returns the state of every tick.
    fn press_on(
        run: &mut Run,
        action: &str,
        presses: &[u32],
        ticks: u32,
        axes: &[(&str, f32)],
    ) -> Vec<GameState> {
        (1..=ticks)
            .map(|t| {
                let actions: &[&str] = if presses.contains(&t) { &[action] } else { &[] };
                run.hold(actions, axes)
            })
            .collect()
    }

    #[test]
    fn light_attack_winds_up_slides_forward_recovers_then_idles() {
        let mut run = Run::new(&attack_props());
        let states = press_on(&mut run, ATTACK, &[1], 15, &NONE);
        for (i, s) in states[..14].iter().enumerate() {
            let t = i as u32 + 1;
            let p = s.player;
            let (phase, z) = match t {
                1..=4 => (Some(AttackPhase::Startup), 0.0),
                5..=8 => (Some(AttackPhase::Active), -0.125 * (t - 4) as f32),
                9..=13 => (Some(AttackPhase::Recovery), -0.5),
                // The last recovery tick already counts as idle.
                _ => (None, -0.5),
            };
            assert_eq!(p.attack_phase, phase, "tick {t}");
            assert_eq!(p.position, Vec3::new(0.0, 0.0, z), "tick {t}");
            if t < 14 {
                assert_eq!((p.action, p.action_tick, p.combo), (Action::Attack, t, 1));
            } else {
                assert_eq!((p.action, p.action_tick, p.combo), (Action::Idle, 0, 0));
            }
            // Paid on the first tick; no regeneration or countdown while
            // attacking, the last tick included; never invulnerable.
            assert_eq!((p.stamina, p.stamina_wait), (85.0, 30), "tick {t}");
            assert!(!p.invulnerable);
            assert_eq!(p.buffered, None);
        }
        assert_eq!(states[14].player.stamina_wait, 29);
    }

    #[test]
    fn heavy_attack_slides_its_own_distance_and_costs_its_own_stamina() {
        let mut run = Run::new(&attack_props());
        let states = press_on(&mut run, HEAVY, &[1], 22, &NONE);
        let p = states[5].player; // tick 6: the last startup tick
        assert_eq!(
            (p.action, p.attack_phase),
            (Action::Heavy, Some(AttackPhase::Startup))
        );
        assert_eq!((p.position, p.stamina, p.combo), (Vec3::ZERO, 75.0, 0));
        let p = states[13].player; // tick 14: the last active tick
        assert_eq!(p.attack_phase, Some(AttackPhase::Active));
        assert_eq!(p.position, Vec3::new(0.0, 0.0, -1.0));
        let p = states[20].player; // tick 21
        assert_eq!(
            (p.action_tick, p.attack_phase),
            (21, Some(AttackPhase::Recovery))
        );
        let p = states[21].player; // tick 22 ends it
        assert_eq!((p.action, p.attack_phase), (Action::Idle, None));
        assert_eq!(p.position, Vec3::new(0.0, 0.0, -1.0));
    }

    #[test]
    fn attack_turns_only_during_startup_at_most_the_attack_turn_step() {
        let mut run = Run::new(&attack_props());
        // Pushing right (+X, facing -π/2) the whole time.
        let right = [(MOVE_X, 1.0)];
        let states = press_on(&mut run, ATTACK, &[1], 15, &right);
        let turn = 180.0_f32.to_radians() / 60.0;
        let mut facing = 0.0_f32;
        let mut position = Vec3::ZERO;
        for (i, s) in states[..14].iter().enumerate() {
            let t = i + 1;
            if t <= 4 {
                facing -= turn;
            } else if t <= 8 {
                // Slides the way it faces once the startup turn is done.
                position += Vec3::new(-libm::sinf(facing), 0.0, -libm::cosf(facing)) * 0.125;
            }
            assert_eq!(s.player.facing.to_bits(), facing.to_bits(), "tick {t}");
            assert_eq!(s.player.position, position, "tick {t}");
        }
        // Idle again: running and turning at the normal speed (12° a tick).
        let s = states[14].player;
        assert_eq!(s.position.x, position.x + 0.0625);
        assert!(s.facing < facing - turn);
    }

    #[test]
    fn buffered_light_attacks_combo_up_to_combo_max_then_start_over() {
        let mut run = Run::new(&attack_props());
        // Attacks start on ticks 1, 15, 29 and 43, each 14 ticks long; the
        // presses on 6 (active), 23 and 40 (recovery) are buffered.
        let states = press_on(&mut run, ATTACK, &[1, 6, 23, 40], 43, &NONE);
        assert_eq!(states[5].player.buffered, Some(Buffered::Attack));
        assert_eq!(states[13].player.action, Action::Idle); // tick 14 ends attack 1
        assert_eq!(states[13].player.buffered, Some(Buffered::Attack));
        for (tick, combo) in [(15, 2), (29, 3), (43, 1)] {
            let p = states[tick - 1].player;
            assert_eq!(
                (p.action, p.action_tick),
                (Action::Attack, 1),
                "tick {tick}"
            );
            assert_eq!(p.combo, combo, "tick {tick}");
            assert_eq!(p.buffered, None);
        }
        assert_eq!(states[42].player.stamina, 40.0);
    }

    #[test]
    fn light_attack_not_buffered_starts_a_new_combo() {
        // Pressed on the tick right after an attack ended: not buffered.
        let mut run = Run::new(&attack_props());
        let states = press_on(&mut run, ATTACK, &[1, 15], 15, &NONE);
        assert_eq!(
            (states[14].player.action, states[14].player.combo),
            (Action::Attack, 1)
        );
        // Pressed some ticks later.
        let mut run = Run::new(&attack_props());
        let states = press_on(&mut run, ATTACK, &[1, 20], 20, &NONE);
        assert_eq!(
            (states[19].player.action, states[19].player.combo),
            (Action::Attack, 1)
        );
        // Buffered, but after a heavy attack.
        let mut run = Run::new(&attack_props());
        press_on(&mut run, HEAVY, &[1], 20, &NONE);
        let states = press_on(&mut run, ATTACK, &[1], 3, &NONE); // ticks 21..=23
        assert_eq!(states[2].player.combo, 1);
        assert_eq!(states[2].player.action_tick, 1); // started on tick 23
    }

    #[test]
    fn presses_during_startup_are_dropped() {
        let mut run = Run::new(&attack_props());
        let states = press_on(&mut run, ATTACK, &[1, 3], 16, &NONE);
        assert!(states.iter().all(|s| s.player.buffered.is_none()));
        assert_eq!(states[14].player.action, Action::Idle);
        assert_eq!(states[15].player.stamina, 85.0);
    }

    #[test]
    fn later_buffered_press_replaces_an_earlier_one() {
        let mut run = Run::new(&attack_props());
        press_on(&mut run, ATTACK, &[1, 6], 9, &NONE);
        let states = press_on(&mut run, HEAVY, &[1], 6, &NONE); // ticks 10..=15
        assert_eq!(states[0].player.buffered, Some(Buffered::Heavy));
        let p = states[5].player; // tick 15
        assert_eq!((p.action, p.action_tick, p.combo), (Action::Heavy, 1, 0));
        assert_eq!(p.stamina, 60.0);
    }

    #[test]
    fn heavy_beats_light_when_both_are_pressed_on_one_tick() {
        let mut run = Run::new(&attack_props());
        let both = [ATTACK, HEAVY];
        let s = run.hold(&both, &NONE);
        assert_eq!((s.player.action, s.player.stamina), (Action::Heavy, 75.0));
        run.ticks(14, false, &NONE); // ticks 2..=15: heavy recovery from 15
        run.tick(false, &NONE);
        let s = run.hold(&both, &NONE); // tick 17
        assert_eq!(s.player.buffered, Some(Buffered::Heavy));
    }

    #[test]
    fn roll_key_tapped_during_recovery_rolls_right_after_the_attack() {
        for (axes, action, facing) in [
            (
                &[(MOVE_X, 1.0)][..],
                Action::Roll,
                -std::f32::consts::FRAC_PI_2,
            ),
            (&NONE[..], Action::Backstep, 0.0),
        ] {
            let mut run = Run::new(&attack_props());
            run.hold(&[ATTACK], &NONE);
            run.ticks(7, false, &NONE); // ticks 2..=8
            run.tick(true, &NONE); // tick 9: recovery, roll pressed
            let s = run.tick(false, &NONE); // tick 10: released: a tap
            assert_eq!(s.player.buffered, Some(Buffered::Roll));
            let s = run.ticks(4, false, &NONE); // ticks 11..=14
            assert_eq!(s.player.action, Action::Idle);
            // Tick 15: the direction is this tick's input.
            let s = run.tick(false, axes);
            assert_eq!((s.player.action, s.player.action_tick), (action, 1));
            assert_eq!(s.player.facing, facing);
            assert_eq!(s.player.buffered, None);
            let cost = if action == Action::Roll { 20.0 } else { 12.0 };
            assert_eq!(s.player.stamina, 85.0 - cost);
        }
    }

    #[test]
    fn attack_with_less_stamina_than_its_cost_still_happens_and_stops_at_zero() {
        let mut run = Run::new(&format!(r#"{}, ("stamina_max", 10)"#, attack_props()));
        let s = run.hold(&[ATTACK], &NONE);
        assert_eq!((s.player.action, s.player.stamina), (Action::Attack, 0.0));
    }

    #[test]
    fn with_no_stamina_new_and_buffered_actions_are_dropped() {
        let mut run = Run::new(&format!(r#"{}, ("stamina_max", 10)"#, attack_props()));
        let states = press_on(&mut run, ATTACK, &[1, 6], 14, &NONE);
        assert_eq!(states[13].player.buffered, Some(Buffered::Attack));
        // Tick 15: the buffered attack is dropped; an ordinary run step.
        let s = run.tick(false, &FORWARD);
        assert_eq!((s.player.action, s.player.buffered), (Action::Idle, None));
        assert_eq!(s.player.position.z, -0.5625);
        assert_eq!((s.player.stamina, s.player.stamina_wait), (0.0, 29));
        // Tick 16: a new press is dropped too.
        let s = run.hold(&[ATTACK], &FORWARD);
        assert_eq!((s.player.action, s.player.buffered), (Action::Idle, None));
        assert_eq!(s.player.position.z, -0.625);
    }

    #[test]
    fn attacking_while_sprinting_stops_the_sprint() {
        let mut run = Run::new(&attack_props());
        let s = run.ticks(20, true, &FORWARD); // sprinting from tick 16
        assert_eq!(s.player.action, Action::Sprint);
        let s = run.hold(&[ROLL, ATTACK], &FORWARD); // tick 21
        assert_eq!((s.player.action, s.player.combo), (Action::Attack, 1));
        assert_eq!(s.player.position.z, -1.5625); // startup: no step
        assert_eq!(s.player.stamina, 97.5 - 15.0);
        let s = run.ticks(3, true, &FORWARD); // ticks 22..=24: still startup
        assert_eq!(s.player.position.z, -1.5625);
        assert_eq!(s.player.stamina, 82.5);
    }

    #[test]
    fn attacks_pressed_during_a_roll_are_dropped() {
        let mut run = Run::new(&attack_props());
        run.tick(true, &FORWARD);
        run.tick(false, &FORWARD); // tick 2: roll tick 1 of 24
        let states = press_on(&mut run, ATTACK, &[8, 20], 23, &NONE); // ticks 3..=25
        assert!(states.iter().all(|s| s.player.buffered.is_none()));
        assert_eq!(states[22].player.action, Action::Idle);
        let s = run.hold(&[HEAVY], &NONE); // tick 26: a new press works
        assert_eq!(s.player.action, Action::Heavy);
        assert_eq!(s.player.stamina, 55.0);
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
    fn attack_props_are_checked_like_the_others() {
        assert_eq!(
            error_of(r#"("attack_cost", 0)"#),
            "the character \"player\": prop \"attack_cost\" must be greater than 0, got 0"
        );
        assert_eq!(
            error_of(r#"("heavy_active_ticks", 0)"#),
            "the character \"player\": prop \"heavy_active_ticks\" must be a whole number of at \
             least 1, got 0"
        );
        assert_eq!(
            error_of(r#"("attack_turn_speed", -90)"#),
            "the character \"player\": prop \"attack_turn_speed\" must be greater than 0, got -90"
        );
        assert_eq!(
            error_of(r#"("combo_max", 1.5)"#),
            "the character \"player\": prop \"combo_max\" must be a whole number of at least 1, \
             got 1.5"
        );
        assert!(Game::from_source(&scene(r#"("combo_max", 1)"#)).is_ok());
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
        assert_eq!(
            error_of(r#"("attack_distance", 2)"#),
            "the character \"player\": attack moves 0.5 m per tick, which must be less than the \
             capsule radius 0.35"
        );
        assert_eq!(
            error_of(r#"("heavy_distance", 2.5)"#),
            "the character \"player\": heavy moves 0.5 m per tick, which must be less than the \
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
        // Attack values after the G2.1 values, in table order.
        assert!(error_of(r#"("combo_max", 0), ("backstep_cost", 0)"#).contains("backstep_cost"));
        assert!(error_of(r#"("combo_max", 0), ("heavy_cost", 0)"#).contains("heavy_cost"));
        // All values come from one table: before the G2.1 windows.
        assert!(error_of(r#"("roll_invuln_to", 40), ("heavy_cost", 0)"#).contains("heavy_cost"));
        // Attack distances last: after roll, light before heavy.
        assert!(error_of(r#"("heavy_distance", 5), ("roll_distance", 16)"#).contains("roll moves"));
        assert!(
            error_of(r#"("heavy_distance", 5), ("attack_distance", 2)"#).contains("attack moves")
        );
    }
}
