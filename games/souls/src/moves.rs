//! Stamina, roll, sprint, backstep, attacks, lock-on, hits and damage: the
//! parameters, read from the character node's props with defaults, and the
//! action state (`docs/architecture.md` section 11).

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
    /// An angle in degrees, at least 0 and less than 90.
    Below90,
    /// An angle in degrees, greater than 0 and at most 360.
    Arc,
    /// At least 0.
    NotNegative,
    /// Any value: checked only against another prop.
    Any,
}

/// Every prop the character understands, with its default and rule, in
/// the order they are checked. Reading, checking and rejecting unknown
/// names all go through this one table.
const PARAMS: [(&str, f64, Rule); 45] = [
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
    ("lock_range", 15.0, Rule::Positive),
    ("lock_break_range", 20.0, Rule::Positive),
    ("lock_angle", 60.0, Rule::Positive),
    ("lock_camera_speed", 360.0, Rule::Positive),
    ("lock_pitch_down", 15.0, Rule::Below90),
    ("switch_threshold", 0.1, Rule::Positive),
    ("switch_cooldown_ticks", 20.0, Rule::AtLeast(0)),
    ("attack_reach", 2.0, Rule::Positive),
    ("attack_arc", 120.0, Rule::Arc),
    ("heavy_reach", 2.4, Rule::Positive),
    ("heavy_arc", 90.0, Rule::Arc),
    ("swing_low", 0.3, Rule::NotNegative),
    ("swing_high", 1.6, Rule::Any),
    ("attack_damage", 20.0, Rule::Positive),
    ("heavy_damage", 35.0, Rule::Positive),
    ("attack_impact", 10.0, Rule::NotNegative),
    ("heavy_impact", 30.0, Rule::NotNegative),
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
    /// Meters: the radius of the swept sector.
    pub reach: f32,
    /// Radians: the whole angle of the sector, centered on `facing`.
    pub arc: f32,
    /// Radians swept per active tick: `arc / active`.
    pub slice: f32,
    /// Health a hit takes off.
    pub damage: f32,
    /// What a hit adds up against the poise.
    pub impact: f32,
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
    pub lock: Lock,
    /// Meters above the feet: the height band an attack sweeps, shared by
    /// light and heavy attacks.
    pub swing_low: f32,
    pub swing_high: f32,
}

/// Lock-on, in per-tick units and radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Lock {
    /// Meters: only targets at most this far (horizontally) are picked.
    pub range: f32,
    /// Meters: a locked target farther than this is let go.
    pub break_range: f32,
    /// Radians: how far from the camera's heading a target may be picked.
    pub angle: f32,
    /// Radians per tick the camera turns to follow the target:
    /// `lock_camera_speed.to_radians() / 60`.
    pub cam_step: f32,
    /// Radians the camera looks down past the lock point.
    pub pitch_down: f32,
    /// `look_x` summed up to this switches targets.
    pub switch_threshold: f32,
    pub switch_cooldown_ticks: u32,
}

impl Params {
    /// Reads and checks the props of the character `id`, whose capsule has
    /// `radius`: unknown names (in file order), then each value (in table
    /// order), then the invulnerability windows (roll, backstep), then
    /// `lock_range <= lock_break_range`, then `swing_low < swing_high`,
    /// then the per-tick distances (sprint, roll, backstep, attack, heavy).
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
                Rule::Below90 if !(0.0..90.0).contains(&v) => {
                    return Err(GameError::NotBelow90 {
                        character,
                        prop,
                        value: v,
                    });
                }
                Rule::Arc if !(v > 0.0 && v <= 360.0) => {
                    return Err(GameError::NotAnArc {
                        character,
                        prop,
                        value: v,
                    });
                }
                Rule::NotNegative if v < 0.0 => {
                    return Err(GameError::Negative {
                        character,
                        prop,
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
            let arc = (get(&format!("{action}_arc")) as f32).to_radians();
            Attack {
                cost: get(&format!("{action}_cost")) as f32,
                startup: ticks(&format!("{action}_startup_ticks")),
                active,
                recovery: ticks(&format!("{action}_recovery_ticks")),
                step: get(&format!("{action}_distance")) as f32 / active as f32,
                reach: get(&format!("{action}_reach")) as f32,
                arc,
                slice: arc / active as f32,
                damage: get(&format!("{action}_damage")) as f32,
                impact: get(&format!("{action}_impact")) as f32,
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
            lock: Lock {
                range: get("lock_range") as f32,
                break_range: get("lock_break_range") as f32,
                angle: (get("lock_angle") as f32).to_radians(),
                cam_step: (get("lock_camera_speed") as f32).to_radians() / 60.0,
                pitch_down: (get("lock_pitch_down") as f32).to_radians(),
                switch_threshold: get("switch_threshold") as f32,
                switch_cooldown_ticks: ticks("switch_cooldown_ticks"),
            },
            swing_low: get("swing_low") as f32,
            swing_high: get("swing_high") as f32,
        };
        for (action, m) in [("roll", params.roll), ("backstep", params.backstep)] {
            if !(m.invuln_from <= m.invuln_to && m.invuln_to <= m.ticks) {
                return Err(GameError::InvulnWindow { character, action });
            }
        }
        if params.lock.range > params.lock.break_range {
            return Err(GameError::LockRanges { character });
        }
        if params.swing_low >= params.swing_high {
            return Err(GameError::SwingBand { character });
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
    use glam::{Quat, Vec3};
    use loom_core::input::{ActionState, TickInput};

    use super::*;
    use crate::{
        ATTACK, Condition, Game, GameState, HEAVY, HealthState, HurtboxState, LOCK, LOOK_X, LOOK_Y,
        LastHit, MOVE_X, MOVE_Y, ROLL,
    };
    use std::f32::consts::FRAC_PI_2;

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
        scene_with(props, "")
    }

    /// [`scene`] with `nodes` (RON node tuples, each followed by a comma)
    /// added after the camera.
    fn scene_with(props: &str, nodes: &str) -> String {
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
        {nodes}
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
            Self::on(&scene(props))
        }

        /// A run of the scene source `src`.
        fn on(src: &str) -> Self {
            Self {
                game: Game::from_source(src).unwrap(),
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

    // ----------------------------------------------------------------- lock-on

    /// A root lock-on target `name` on the ground at `(x, z)`, locked on
    /// 1.5 m up: level with the camera pivot.
    fn target(name: &str, x: f32, z: f32) -> String {
        format!(
            r#"(name: "{name}", position: ({x:?}, 0.0, {z:?}), props: [("lock_height", 1.5)]),"#
        )
    }

    /// 10 m ahead: 5 m to the left, straight ahead, 5 m to the right.
    fn three_targets() -> String {
        [
            target("left", -5.0, -10.0),
            target("mid", 0.0, -10.0),
            target("right", 5.0, -10.0),
        ]
        .concat()
    }

    fn lock_of(s: &GameState) -> Option<&str> {
        s.lock.as_deref()
    }

    #[test]
    fn lock_picks_the_target_closest_to_the_view_direction_not_the_nearest() {
        // 10 m straight ahead, and about 2.8 m away 45° to the left, in
        // both orders.
        let (ahead, near) = (target("ahead", 0.0, -10.0), target("near", -2.0, -2.0));
        for nodes in [format!("{ahead}{near}"), format!("{near}{ahead}")] {
            let mut run = Run::on(&scene_with("", &nodes));
            let s = run.hold(&[LOCK], &NONE);
            assert_eq!(lock_of(&s), Some("ahead"), "{nodes}");
        }
    }

    #[test]
    fn lock_skips_targets_beyond_lock_range_or_lock_angle() {
        // 15.5 m ahead (lock_range 15), and 90° to the left (lock_angle 60).
        let nodes = [target("far", 0.0, -15.5), target("aside", -3.0, 0.0)].concat();
        let mut run = Run::on(&scene_with("", &nodes));
        assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), None);
        // Exactly lock_range away is in range.
        let mut run = Run::on(&scene_with("", &target("edge", 0.0, -15.0)));
        assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), Some("edge"));
    }

    #[test]
    fn equal_angles_lock_the_first_target_in_pre_order() {
        // Mirror images 31° to the right and to the left: the same angle,
        // bit for bit.
        let (right, left) = (target("right", 3.0, -5.0), target("left", -3.0, -5.0));
        for (nodes, first) in [
            (format!("{right}{left}"), "right"),
            (format!("{left}{right}"), "left"),
        ] {
            let mut run = Run::on(&scene_with("", &nodes));
            assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), Some(first));
        }
    }

    #[test]
    fn lock_key_with_no_target_in_range_turns_the_camera_as_usual() {
        let mut run = Run::on(&scene_with("", &target("far", 0.0, -30.0)));
        let s = run.hold(&[LOCK], &[(LOOK_X, 0.04), (LOOK_Y, 0.02)]);
        assert_eq!(lock_of(&s), None);
        assert_eq!((s.camera.yaw, s.camera.pitch), (-0.04, 0.02));
    }

    #[test]
    fn pressing_lock_again_lets_go_and_the_camera_turns_on_that_tick() {
        let mut run = Run::on(&scene_with("", &target("ahead", 0.0, -10.0)));
        assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), Some("ahead"));
        let before = run.hold(&[], &NONE); // tick 2: released, still locked
        assert_eq!(lock_of(&before), Some("ahead"));
        let s = run.hold(&[LOCK], &[(LOOK_X, 0.04), (LOOK_Y, 0.02)]);
        assert_eq!(lock_of(&s), None);
        assert_eq!(before.camera.yaw, 0.0);
        assert_eq!(s.camera.yaw, -0.04);
        assert_eq!(s.camera.pitch, before.camera.pitch + 0.02);
    }

    #[test]
    fn target_past_lock_break_range_is_let_go_on_the_tick_it_is() {
        // lock_range 5 and lock_break_range 6; backing away 0.0625 a tick
        // from a target 5 m ahead.
        let props = format!(r#"{PROPS}, ("lock_range", 5), ("lock_break_range", 6)"#);
        let mut run = Run::on(&scene_with(&props, &target("ahead", 0.0, -5.0)));
        run.hold(&[LOCK], &NONE);
        let back = [(MOVE_Y, -1.0)];
        let s = run.ticks(16, false, &back); // ticks 2..=17
        assert_eq!(s.player.position, Vec3::new(0.0, 0.0, 1.0));
        // Tick 18 starts exactly 6 m away: still locked.
        let s = run.tick(false, &back);
        assert_eq!(lock_of(&s), Some("ahead"));
        assert_eq!(s.player.position.z, 1.0625);
        // Tick 19 starts 6.0625 m away: let go, and look_x turns the camera.
        let s = run.hold(&[], &[(MOVE_Y, -1.0), (LOOK_X, 0.04)]);
        assert_eq!(lock_of(&s), None);
        assert_eq!(s.camera.yaw, -0.04);
    }

    #[test]
    fn camera_turns_to_a_target_on_the_left_by_cam_step_a_tick() {
        // Straight to the left and level with the pivot: yaw_t = bearing =
        // π/2. The expected yaw is the pseudocode of docs/architecture.md
        // section 11 (step 1c) run tick by tick in this test, not an
        // analytic solution: cam_step (6°) a tick, the last step landing on
        // yaw_t. (`wrap` leaves these in-range values unchanged.)
        let mut run = Run::on(&scene_with(
            r#"("lock_angle", 100)"#,
            &target("left", -10.0, 0.0),
        ));
        let cam_step = 360.0_f32.to_radians() / 60.0;
        let yaw_t = libm::atan2f(10.0, -0.0);
        let mut yaw = 0.0_f32;
        let mut arrived = None;
        for t in 1..=20 {
            let actions: &[&str] = if t == 1 { &[LOCK] } else { &[] };
            let s = run.hold(actions, &NONE);
            yaw += (yaw_t - yaw).clamp(-cam_step, cam_step);
            assert_eq!(s.camera.yaw.to_bits(), yaw.to_bits(), "tick {t}");
            if yaw == yaw_t && arrived.is_none() {
                arrived = Some(t);
            }
        }
        // 90° at 6° a tick.
        assert_eq!(arrived, Some(15));
        assert_eq!(yaw_t, FRAC_PI_2);
    }

    #[test]
    fn camera_pitch_settles_lock_pitch_down_below_a_level_target_and_ignores_look_y() {
        // The lock point is level with the pivot: pitch_t = -15°. Checked
        // against that analytic value with a tolerance (the last step's
        // `pitch + (pitch_t - pitch)` may round).
        let mut run = Run::on(&scene_with("", &target("ahead", 0.0, -10.0)));
        let cam_step = 360.0_f32.to_radians() / 60.0;
        let s = run.hold(&[LOCK], &[(LOOK_Y, 0.04)]);
        assert_eq!(s.camera.pitch, -cam_step);
        // Looking up the whole time does nothing while locked on.
        let s = run.ticks(10, false, &[(LOOK_Y, 0.04)]);
        assert!(
            (s.camera.pitch + 15.0_f32.to_radians()).abs() <= 1e-6,
            "{}",
            s.camera.pitch
        );
        assert_eq!(s.camera.yaw, 0.0);
    }

    #[test]
    fn flicking_right_or_left_switches_to_the_target_on_that_side() {
        for (look_x, next) in [(0.12, "right"), (-0.12, "left")] {
            let mut run = Run::on(&scene_with("", &three_targets()));
            assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), Some("mid"));
            let s = run.hold(&[], &[(LOOK_X, look_x)]);
            assert_eq!(lock_of(&s), Some(next), "{look_x}");
        }
    }

    #[test]
    fn flick_with_nothing_on_that_side_still_starts_the_cooldown() {
        let mut run = Run::on(&scene_with("", &three_targets()));
        run.hold(&[LOCK], &NONE);
        // Tick 2: to the right one; 20 ticks of cooldown (ticks 3..=22).
        assert_eq!(lock_of(&run.hold(&[], &[(LOOK_X, 0.12)])), Some("right"));
        run.ticks(20, false, &NONE);
        // Tick 23: nothing farther right, no switch, but a new cooldown.
        assert_eq!(lock_of(&run.hold(&[], &[(LOOK_X, 0.12)])), Some("right"));
        for t in 24..=43 {
            let s = run.hold(&[], &[(LOOK_X, -0.12)]);
            assert_eq!(lock_of(&s), Some("right"), "tick {t}");
        }
        // Tick 44: the cooldown is over.
        assert_eq!(lock_of(&run.hold(&[], &[(LOOK_X, -0.12)])), Some("mid"));
    }

    #[test]
    fn holding_l_switches_on_the_third_tick() {
        let mut run = Run::on(&scene_with("", &three_targets()));
        run.hold(&[LOCK], &NONE);
        let l = [(LOOK_X, 0.04)];
        assert_eq!(lock_of(&run.hold(&[], &l)), Some("mid")); // 0.04
        assert_eq!(lock_of(&run.hold(&[], &l)), Some("mid")); // 0.08
        assert_eq!(lock_of(&run.hold(&[], &l)), Some("right")); // 0.12
    }

    #[test]
    fn tick_without_look_x_clears_the_flick() {
        let mut run = Run::on(&scene_with("", &three_targets()));
        run.hold(&[LOCK], &NONE);
        let l = [(LOOK_X, 0.04)];
        for axes in [&l[..], &l, &NONE, &l, &l] {
            assert_eq!(lock_of(&run.hold(&[], axes)), Some("mid"));
        }
        assert_eq!(lock_of(&run.hold(&[], &l)), Some("right"));
    }

    #[test]
    fn locked_strafe_moves_along_the_camera_right_and_faces_the_target() {
        let mut run = Run::on(&scene_with(PROPS, &target("ahead", 0.0, -10.0)));
        run.hold(&[LOCK], &NONE);
        let s = run.hold(&[], &[(MOVE_X, 1.0)]);
        // The camera still looks straight ahead: right is +X.
        assert_eq!(s.camera.yaw, 0.0);
        assert_eq!(s.player.position, Vec3::new(0.0625, 0.0, 0.0));
        // Facing the target from where it now is, not -π/2 (its way).
        assert_eq!(s.player.facing, libm::atan2f(0.0625, 10.0));
    }

    #[test]
    fn locked_turn_to_face_the_target_is_at_most_max_turn_a_tick() {
        // Standing still, the target straight to the right (-π/2).
        let mut run = Run::on(&scene_with(
            r#"("lock_angle", 100)"#,
            &target("right", 10.0, 0.0),
        ));
        let max_turn = 720.0_f32.to_radians() / 60.0;
        let mut facing = 0.0_f32;
        for t in 1..=7 {
            let actions: &[&str] = if t == 1 { &[LOCK] } else { &[] };
            let s = run.hold(actions, &NONE);
            facing -= max_turn;
            assert_eq!(s.player.facing.to_bits(), facing.to_bits(), "tick {t}");
        }
        // 90° at 12° a tick: there on tick 8.
        let s = run.tick(false, &NONE);
        assert!(
            (s.player.facing + FRAC_PI_2).abs() <= 1e-6,
            "{}",
            s.player.facing
        );
    }

    #[test]
    fn locked_sprint_faces_where_it_runs_then_turns_back_to_the_target() {
        let mut run = Run::on(&scene_with(PROPS, &target("ahead", 0.0, -10.0)));
        let bearing = |p: Vec3| libm::atan2f(-(0.0 - p.x), -(-10.0 - p.z));
        run.hold(&[LOCK], &NONE);
        let right = [(MOVE_X, 1.0)];
        // Ticks 2..=16: held within the tap window: running, facing the
        // target.
        let s = run.ticks(15, true, &right);
        assert_eq!(s.player.action, Action::Idle);
        assert!((s.player.facing - bearing(s.player.position)).abs() <= 1e-6);
        // Ticks 17..=24: sprinting, turned to where it runs (the camera's
        // right) at 12° a tick.
        let s = run.ticks(8, true, &right);
        assert_eq!(s.player.action, Action::Sprint);
        let (sn, cs) = (libm::sinf(s.camera.yaw), libm::cosf(s.camera.yaw));
        let way = libm::atan2f(-cs, sn);
        assert!(
            (s.player.facing - way).abs() <= 1e-6,
            "{} {way}",
            s.player.facing
        );
        // Released: running again, back to facing the target.
        let s = run.ticks(10, false, &right);
        assert_eq!(s.player.action, Action::Idle);
        assert!((s.player.facing - bearing(s.player.position)).abs() <= 1e-6);
    }

    #[test]
    fn locked_roll_goes_and_faces_the_input_way_like_an_unlocked_one() {
        let nodes = target("ahead", 0.0, -10.0);
        let mut locked = Run::on(&scene_with(PROPS, &nodes));
        let mut free = Run::on(&scene_with(PROPS, &nodes));
        locked.hold(&[LOCK], &NONE);
        free.hold(&[], &NONE);
        locked.tick(true, &NONE);
        free.tick(true, &NONE);
        // Tick 3: released pushing right, a roll; ticks 4..=26 the rest of
        // its 24 ticks, steering left, which does nothing.
        for t in 3..=26 {
            let axes = [(MOVE_X, if t == 3 { 1.0 } else { -1.0 })];
            let (a, b) = (locked.tick(false, &axes), free.tick(false, &axes));
            assert_eq!(lock_of(&a), Some("ahead"));
            assert_eq!(a.player, b.player, "tick {t}");
            assert_eq!(a.player.facing.to_bits(), b.player.facing.to_bits());
            assert_eq!(
                a.player.position.to_array().map(f32::to_bits),
                b.player.position.to_array().map(f32::to_bits)
            );
            if t == 3 {
                assert_eq!(a.player.action, Action::Roll);
                assert_eq!(a.player.facing, -FRAC_PI_2);
            }
        }
        // Tick 27: running again (no input); the locked one turns back
        // towards the target, the other one stays.
        let (a, b) = (locked.tick(false, &NONE), free.tick(false, &NONE));
        let max_turn = 720.0_f32.to_radians() / 60.0;
        assert_eq!(b.player.facing, -FRAC_PI_2);
        assert_eq!(a.player.facing, -FRAC_PI_2 + max_turn);
    }

    #[test]
    fn locked_attack_turns_to_the_target_during_startup_then_slides_that_way() {
        // The target straight to the right; lock and attack on one tick,
        // pushing no direction.
        let props = format!(r#"{}, ("lock_angle", 100)"#, attack_props());
        let mut run = Run::on(&scene_with(&props, &target("right", 10.0, 0.0)));
        let turn = 180.0_f32.to_radians() / 60.0;
        let mut facing = 0.0_f32;
        let mut position = Vec3::ZERO;
        for t in 1..=8 {
            let actions: &[&str] = if t == 1 { &[LOCK, ATTACK] } else { &[] };
            let s = run.hold(actions, &NONE);
            if t <= 4 {
                facing -= turn;
            } else {
                position += Vec3::new(-libm::sinf(facing), 0.0, -libm::cosf(facing)) * 0.125;
            }
            assert_eq!(s.player.facing.to_bits(), facing.to_bits(), "tick {t}");
            assert_eq!(s.player.position, position, "tick {t}");
        }
        assert!(facing < -0.2);
    }

    // -------------------------------------------------------------------- hits

    /// [`PROPS`] plus attacks that barely slide: 0.0625 m over 4 active
    /// ticks, 1/64 m a tick (exact), so the angles to targets hardly move
    /// and every target below stays at least 5° from a slice's edge.
    /// Light: ticks 1-4 startup, 5-8 active, 9-14 recovery. Heavy: 1-6,
    /// 7-10, 11-18. Arcs, reaches and the swing band are the defaults:
    /// light 120° in four 30° slices from -60° (right) to 60° (left),
    /// heavy 90° in four 22.5° slices; reach 2 and 2.4; swing 0.3-1.6 m.
    fn hit_props() -> String {
        format!(
            r#"{PROPS}, ("attack_startup_ticks", 4), ("attack_active_ticks", 4),
            ("attack_recovery_ticks", 6), ("attack_distance", 0.0625),
            ("heavy_startup_ticks", 6), ("heavy_active_ticks", 4),
            ("heavy_recovery_ticks", 8), ("heavy_distance", 0.0625)"#
        )
    }

    /// A root hurtbox `name` standing at `(x, y, z)` with `radius` and
    /// `height`, and a child mesh `body`.
    fn hurtbox(name: &str, (x, y, z): (f32, f32, f32), radius: f32, height: f32) -> String {
        format!(
            r#"(name: "{name}", position: ({x:?}, {y:?}, {z:?}),
                props: [("hurt_radius", {radius:?}), ("hurt_height", {height:?})],
                children: [(name: "body", mesh: Cube, color: (0.2, 0.4, 0.6))]),"#
        )
    }

    /// `(x, 0, z)` at `meters` from the origin, `degrees` from -Z (positive
    /// to the left, like `facing`).
    fn at(degrees: f32, meters: f32) -> (f32, f32, f32) {
        let a = degrees.to_radians();
        (-meters * libm::sinf(a), 0.0, -meters * libm::cosf(a))
    }

    fn hurtbox_state<'a>(s: &'a GameState, name: &str) -> &'a HurtboxState {
        s.hurtboxes
            .iter()
            .find(|h| h.node == name)
            .expect("the hurtbox exists")
    }

    /// The tick of the latest hit on `name`, if any.
    fn hit_tick(s: &GameState, name: &str) -> Option<u64> {
        hurtbox_state(s, name).last_hit.map(|h| h.tick)
    }

    #[test]
    fn light_attack_sweeps_its_slices_right_to_left_one_per_active_tick() {
        // 45° right is in the first slice (-60°..-30°), 45° left in the
        // last (30°..60°).
        let nodes = [
            hurtbox("right", at(-45.0, 1.5), 0.05, 1.8),
            hurtbox("left", at(45.0, 1.5), 0.05, 1.8),
        ]
        .concat();
        let mut run = Run::on(&scene_with(&hit_props(), &nodes));
        let states = press_on(&mut run, ATTACK, &[1], 14, &NONE);
        for (i, s) in states.iter().enumerate() {
            let t = i as u64 + 1;
            let right = if t >= 5 { Some(5) } else { None };
            let left = if t >= 8 { Some(8) } else { None };
            assert_eq!(hit_tick(s, "right"), right, "tick {t}");
            assert_eq!(hit_tick(s, "left"), left, "tick {t}");
        }
        let last = states.last().unwrap();
        assert_eq!(
            hurtbox_state(last, "left").last_hit,
            Some(LastHit {
                tick: 8,
                attack: Action::Attack,
                combo: 1
            })
        );
        assert_eq!(hurtbox_state(last, "right").hits, 1);
    }

    #[test]
    fn hurtbox_circle_reaching_over_a_slice_edge_is_hit_a_tick_early() {
        // Centered 25° right: in the second slice (-30°..0°), 1.5 * sin 5°
        // ≈ 0.13 m from the -30° edge. A radius of 0.2 reaches over it into
        // the first slice; 0.1 does not.
        for (radius, tick) in [(0.2, 5), (0.1, 6)] {
            let nodes = hurtbox("h", at(-25.0, 1.5), radius, 1.8);
            let mut run = Run::on(&scene_with(&hit_props(), &nodes));
            let states = press_on(&mut run, ATTACK, &[1], 14, &NONE);
            assert_eq!(hit_tick(&states[13], "h"), Some(tick), "radius {radius}");
            assert_eq!(hurtbox_state(&states[13], "h").hits, 1);
        }
    }

    #[test]
    fn hurtbox_is_hit_exactly_up_to_reach_plus_its_radius() {
        // Straight ahead (angle -0.0, the end of the second slice, whose
        // `hi` is exactly 0). On active tick 2 the character is 2/64 m
        // forward: 2.28125 m away is 2.25 = reach 2 + radius 0.25, all
        // exact. 2.3 m is out of reach on every tick.
        let reach = |meters: f32| {
            let nodes = hurtbox("h", (0.0, 0.0, -meters), 0.25, 1.8);
            let mut run = Run::on(&scene_with(&hit_props(), &nodes));
            let states = press_on(&mut run, ATTACK, &[1], 14, &NONE);
            hit_tick(&states[13], "h")
        };
        assert_eq!(reach(2.28125), Some(6));
        assert_eq!(reach(2.3), None);
    }

    #[test]
    fn heavy_attack_reaches_farther_than_a_light_one() {
        // 2.6 m ahead: out of the light reach (2 + 0.25), within the heavy
        // one (2.4 + 0.25) on the heavy's active tick 2, game tick 8.
        let nodes = hurtbox("h", (0.0, 0.0, -2.6), 0.25, 1.8);
        let mut run = Run::on(&scene_with(&hit_props(), &nodes));
        let states = press_on(&mut run, ATTACK, &[1], 14, &NONE);
        assert_eq!(hurtbox_state(&states[13], "h").hits, 0);
        let mut run = Run::on(&scene_with(&hit_props(), &nodes));
        let states = press_on(&mut run, HEAVY, &[1], 18, &NONE);
        assert_eq!(
            hurtbox_state(&states[17], "h").last_hit,
            Some(LastHit {
                tick: 8,
                attack: Action::Heavy,
                combo: 0
            })
        );
    }

    #[test]
    fn hurtbox_is_hit_when_its_heights_just_meet_the_swing_band() {
        // Swing band 0.25-1.5 m, heights all exact: a hurtbox standing at
        // 1.5 (bottom on the band's top) or from 0 to 0.25 (top on its
        // bottom) is hit, both ends being closed; 1/16 m further is not.
        let props = format!(
            r#"{}, ("swing_low", 0.25), ("swing_high", 1.5)"#,
            hit_props()
        );
        let hit = |y: f32, height: f32| {
            let nodes = hurtbox("h", (0.0, y, -1.0), 0.25, height);
            let mut run = Run::on(&scene_with(&props, &nodes));
            let states = press_on(&mut run, ATTACK, &[1], 14, &NONE);
            hurtbox_state(&states[13], "h").hits
        };
        assert_eq!(hit(1.5, 1.8), 1);
        assert_eq!(hit(1.5625, 1.8), 0);
        assert_eq!(hit(0.0, 0.25), 1);
        assert_eq!(hit(-0.0625, 0.25), 0);
    }

    #[test]
    fn each_attack_hits_a_hurtbox_once_and_a_buffered_one_again() {
        // Close and big enough to touch every slice.
        let nodes = hurtbox("h", (0.0, 0.0, -0.5), 1.0, 1.8);
        let mut run = Run::on(&scene_with(&hit_props(), &nodes));
        // Tick 10 is in the first attack's recovery: buffered, the second
        // attack starts on tick 15, its first active tick is 19.
        let states = press_on(&mut run, ATTACK, &[1, 10], 28, &NONE);
        assert_eq!(hurtbox_state(&states[13], "h").hits, 1);
        assert_eq!(hit_tick(&states[13], "h"), Some(5));
        let h = hurtbox_state(&states[27], "h");
        assert_eq!(h.hits, 2);
        assert_eq!(
            h.last_hit,
            Some(LastHit {
                tick: 19,
                attack: Action::Attack,
                combo: 2
            })
        );
    }

    #[test]
    fn one_attack_hits_several_hurtboxes_on_one_tick_or_on_different_ones() {
        // Two in the first slice (40° and 50° right), one in the last.
        let nodes = [
            hurtbox("a", at(-40.0, 1.2), 0.05, 1.8),
            hurtbox("b", at(-50.0, 1.7), 0.05, 1.8),
            hurtbox("c", at(45.0, 1.5), 0.05, 1.8),
        ]
        .concat();
        let mut run = Run::on(&scene_with(&hit_props(), &nodes));
        let states = press_on(&mut run, ATTACK, &[1], 14, &NONE);
        let s = &states[13];
        assert_eq!(hit_tick(s, "a"), Some(5));
        assert_eq!(hit_tick(s, "b"), Some(5));
        assert_eq!(hit_tick(s, "c"), Some(8));
        assert!(s.hurtboxes.iter().all(|h| h.hits == 1));
    }

    /// The color of the mesh at `path`.
    fn color_of(run: &Run, path: &str) -> Vec3 {
        let scene = run.game.scene();
        scene.node(scene.find(path).unwrap()).mesh.unwrap().color
    }

    #[test]
    fn hit_during_a_flash_starts_the_eight_ticks_over() {
        // One-tick phases: an attack lasts 3 ticks (startup, active,
        // recovery), so a buffered one hits 3 ticks after the first, while
        // it is still white: hits on ticks 2 and 5, white on 2..=12.
        let props = format!(
            r#"{PROPS}, ("attack_startup_ticks", 1), ("attack_active_ticks", 1),
            ("attack_recovery_ticks", 1), ("attack_distance", 0.0625)"#
        );
        let mut run = Run::on(&scene_with(
            &props,
            &hurtbox("h", (0.0, 0.0, -1.0), 0.25, 1.8),
        ));
        // Tick 3 is the first attack's recovery: buffered, starts on 4.
        let states = press_on(&mut run, ATTACK, &[1, 3], 14, &NONE);
        assert_eq!(hit_tick(&states[1], "h"), Some(2));
        assert_eq!(hit_tick(&states[4], "h"), Some(5));
        assert_eq!(hurtbox_state(&states[13], "h").hits, 2);
        let mut run = Run::on(&scene_with(
            &props,
            &hurtbox("h", (0.0, 0.0, -1.0), 0.25, 1.8),
        ));
        for t in 1..=14 {
            let actions: &[&str] = if t == 1 || t == 3 { &[ATTACK] } else { &[] };
            run.hold(actions, &NONE);
            let white = (2..=12).contains(&t);
            let expected = if white {
                Vec3::ONE
            } else {
                Vec3::new(0.2, 0.4, 0.6)
            };
            assert_eq!(color_of(&run, "h/body"), expected, "tick {t}");
        }
    }

    #[test]
    fn hit_hurtbox_meshes_flash_white_for_eight_ticks_then_come_back() {
        // `bare` has no mesh of its own, only its child; `full` has both.
        let nodes = format!(
            "{}{}",
            hurtbox("bare", at(-40.0, 1.2), 0.05, 1.8),
            r#"(name: "full", position: (1.0, 0.0, -1.2), mesh: Cube, color: (0.1, 0.2, 0.3),
                props: [("hurt_radius", 0.05), ("hurt_height", 1.8)],
                children: [(name: "inner", mesh: Cube, color: (0.3, 0.2, 0.1))]),"#
        );
        let mut run = Run::on(&scene_with(&hit_props(), &nodes));
        let meshes = [
            ("bare/body", Vec3::new(0.2, 0.4, 0.6)),
            ("full", Vec3::new(0.1, 0.2, 0.3)),
            ("full/inner", Vec3::new(0.3, 0.2, 0.1)),
        ];
        // Both are hit on tick 5 (the first slice; `full` is 39.8° right);
        // presses on 1 and 10 give a second attack hitting on tick 19.
        for t in 1..=30_u64 {
            let actions: &[&str] = if t == 1 || t == 10 { &[ATTACK] } else { &[] };
            let s = run.hold(actions, &NONE);
            let white = (5..=12).contains(&t) || (19..=26).contains(&t);
            for (path, color) in meshes {
                let expected = if white { Vec3::ONE } else { color };
                assert_eq!(color_of(&run, path), expected, "{path} on tick {t}");
            }
            if t == 19 {
                assert_eq!(hit_tick(&s, "full"), Some(19));
                assert_eq!(hurtbox_state(&s, "bare").hits, 2);
            }
        }
    }

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

    /// The error of [`scene_with`] `props` and `nodes`.
    fn error_with(props: &str, nodes: &str) -> String {
        match Game::from_source(&scene_with(props, nodes)) {
            Ok(_) => panic!("scene should be rejected: {props} {nodes}"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn lock_props_are_checked_like_the_others() {
        for (props, problem) in [
            (
                r#"("lock_range", 0)"#,
                "prop \"lock_range\" must be greater than 0, got 0",
            ),
            (
                r#"("lock_break_range", -1)"#,
                "prop \"lock_break_range\" must be greater than 0, got -1",
            ),
            (
                r#"("lock_angle", 0)"#,
                "prop \"lock_angle\" must be greater than 0, got 0",
            ),
            (
                r#"("lock_camera_speed", -360)"#,
                "prop \"lock_camera_speed\" must be greater than 0, got -360",
            ),
            (
                r#"("lock_pitch_down", 90)"#,
                "prop \"lock_pitch_down\" must be at least 0 and less than 90, got 90",
            ),
            (
                r#"("lock_pitch_down", -0.5)"#,
                "prop \"lock_pitch_down\" must be at least 0 and less than 90, got -0.5",
            ),
            (
                r#"("switch_threshold", 0)"#,
                "prop \"switch_threshold\" must be greater than 0, got 0",
            ),
            (
                r#"("switch_cooldown_ticks", 1.5)"#,
                "prop \"switch_cooldown_ticks\" must be a whole number of at least 0, got 1.5",
            ),
        ] {
            assert_eq!(
                error_of(props),
                format!("the character \"player\": {problem}")
            );
        }
        for props in [
            r#"("lock_pitch_down", 0)"#,
            r#"("lock_pitch_down", 89.9)"#,
            r#"("switch_cooldown_ticks", 0)"#,
        ] {
            assert!(Game::from_source(&scene(props)).is_ok(), "{props}");
        }
    }

    #[test]
    fn lock_range_beyond_lock_break_range_is_rejected() {
        assert_eq!(
            error_of(r#"("lock_range", 25)"#),
            "the character \"player\": needs lock_range <= lock_break_range"
        );
        assert_eq!(
            error_of(r#"("lock_range", 10), ("lock_break_range", 9.5)"#),
            "the character \"player\": needs lock_range <= lock_break_range"
        );
        assert!(Game::from_source(&scene(r#"("lock_range", 20)"#)).is_ok());
    }

    #[test]
    fn lock_target_nodes_are_checked() {
        assert_eq!(
            error_with("", r#"(name: "t", props: [("lock_heigth", 1.0)]),"#),
            "the node \"t\": unknown prop \"lock_heigth\" (known: lock_height, hurt_radius, \
             hurt_height, hp, poise, stagger_ticks, poise_reset_ticks, revive_ticks)"
        );
        // Any node but the character, target or not. (Until G3.2 this used
        // "hp", which G3.3 made a known prop.)
        assert_eq!(
            error_with(
                "",
                r#"(name: "g", children: [(name: "t", props: [("armor", 3)])]),"#
            ),
            "the node \"g/t\": unknown prop \"armor\" (known: lock_height, hurt_radius, \
             hurt_height, hp, poise, stagger_ticks, poise_reset_ticks, revive_ticks)"
        );
        assert_eq!(
            error_with("", r#"(name: "t", props: [("lock_height", 0)]),"#),
            "the node \"t\": prop \"lock_height\" must be greater than 0, got 0"
        );
        assert_eq!(
            error_with(
                "",
                r#"(name: "g", children: [(name: "t", props: [("lock_height", 1.0)])]),"#
            ),
            "the lock target \"g/t\" must be a root node, and not the camera"
        );
        let src = scene("").replace(
            r#"(name: "cam","#,
            r#"(name: "cam", props: [("lock_height", 1.0)],"#,
        );
        match Game::from_source(&src) {
            Err(e) => assert_eq!(
                e.to_string(),
                "the lock target \"cam\" must be a root node, and not the camera"
            ),
            Ok(_) => panic!("the camera cannot be a target"),
        }
        // The character's props are the character's: lock_height is unknown.
        assert_eq!(
            error_of(r#"("lock_height", 1.0)"#),
            "the character \"player\": unknown prop \"lock_height\""
        );
    }

    #[test]
    fn lock_checks_run_in_the_documented_order() {
        // The new values come after the G2.3 ones, in table order.
        assert!(error_of(r#"("lock_angle", 0), ("combo_max", 0)"#).contains("combo_max"));
        assert!(
            error_of(r#"("switch_threshold", 0), ("lock_range", 0)"#).contains("\"lock_range\"")
        );
        // Values, then the windows, then the ranges, then the distances.
        assert!(
            error_of(r#"("roll_invuln_to", 40), ("lock_pitch_down", 90)"#)
                .contains("lock_pitch_down")
        );
        assert!(error_of(r#"("lock_range", 25), ("roll_invuln_to", 40)"#).contains("roll_invuln"));
        assert!(
            error_of(r#"("sprint_speed", 30), ("lock_range", 25)"#).contains("lock_break_range")
        );
        // Targets last: after the character's distances.
        let bad_target = r#"(name: "t", props: [("lock_height", 0)]),"#;
        assert!(error_with(r#"("sprint_speed", 30)"#, bad_target).contains("sprint moves"));
        // Node by node in pre-order; in each, unknown names, then the
        // lock_height value, then the root rule.
        let nodes = r#"(name: "a", props: [("lock_height", 0)]), (name: "b", props: [("x", 1)]),"#;
        assert!(error_with("", nodes).starts_with("the node \"a\""));
        let nodes = r#"(name: "t", props: [("lock_height", 0), ("x", 1)]),"#;
        assert!(error_with("", nodes).contains("unknown prop \"x\""));
        let nodes = r#"(name: "g", children: [(name: "t", props: [("lock_height", 0)])]),"#;
        assert!(error_with("", nodes).contains("must be greater than 0"));
    }

    #[test]
    fn hit_props_are_checked_with_exact_messages() {
        for (props, problem) in [
            (
                r#"("attack_arc", 0)"#,
                "prop \"attack_arc\" must be greater than 0 and at most 360, got 0",
            ),
            (
                r#"("attack_arc", 360.5)"#,
                "prop \"attack_arc\" must be greater than 0 and at most 360, got 360.5",
            ),
            (
                r#"("heavy_arc", -90)"#,
                "prop \"heavy_arc\" must be greater than 0 and at most 360, got -90",
            ),
            (
                r#"("attack_reach", 0)"#,
                "prop \"attack_reach\" must be greater than 0, got 0",
            ),
            (
                r#"("heavy_reach", -1)"#,
                "prop \"heavy_reach\" must be greater than 0, got -1",
            ),
            (
                r#"("swing_low", -0.1)"#,
                "prop \"swing_low\" must be at least 0, got -0.1",
            ),
            (r#"("swing_low", 1.6)"#, "needs swing_low < swing_high"),
            (r#"("swing_high", 0.2)"#, "needs swing_low < swing_high"),
        ] {
            assert_eq!(
                error_of(props),
                format!("the character \"player\": {problem}")
            );
        }
        for props in [
            r#"("attack_arc", 360)"#,
            r#"("heavy_arc", 360)"#,
            r#"("swing_low", 0)"#,
            r#"("swing_low", 1.5)"#,
        ] {
            assert!(Game::from_source(&scene(props)).is_ok(), "{props}");
        }
    }

    #[test]
    fn hurtbox_nodes_are_checked_with_exact_messages() {
        for (nodes, message) in [
            (
                r#"(name: "t", props: [("hurt_radius", 0.4)]),"#,
                "the node \"t\": needs both hurt_radius and hurt_height, or neither",
            ),
            (
                r#"(name: "t", props: [("hurt_height", 1.8)]),"#,
                "the node \"t\": needs both hurt_radius and hurt_height, or neither",
            ),
            (
                r#"(name: "t", props: [("hurt_radius", 0), ("hurt_height", 1.8)]),"#,
                "the node \"t\": prop \"hurt_radius\" must be greater than 0, got 0",
            ),
            (
                r#"(name: "t", props: [("hurt_radius", 0.4), ("hurt_height", -1)]),"#,
                "the node \"t\": prop \"hurt_height\" must be greater than 0, got -1",
            ),
            (
                r#"(name: "g", children: [(name: "t",
                    props: [("hurt_radius", 0.4), ("hurt_height", 1.8)])]),"#,
                "the hurtbox \"g/t\" must be a root node, and not the camera",
            ),
            (
                r#"(name: "t", props: [("hurt_radius", 0.4), ("hurt_heigth", 1.8)]),"#,
                "the node \"t\": unknown prop \"hurt_heigth\" (known: lock_height, hurt_radius, \
                 hurt_height, hp, poise, stagger_ticks, poise_reset_ticks, revive_ticks)",
            ),
        ] {
            assert_eq!(error_with("", nodes), message);
        }
        let src = scene("").replace(
            r#"(name: "cam","#,
            r#"(name: "cam", props: [("hurt_radius", 0.4), ("hurt_height", 1.8)],"#,
        );
        match Game::from_source(&src) {
            Err(e) => assert_eq!(
                e.to_string(),
                "the hurtbox \"cam\" must be a root node, and not the camera"
            ),
            Ok(_) => panic!("the camera cannot be a hurtbox"),
        }
        // The lock_height message is unchanged.
        assert_eq!(
            error_with("", r#"(name: "t", props: [("lock_height", -1)]),"#),
            "the node \"t\": prop \"lock_height\" must be greater than 0, got -1"
        );
        // The character's props are the character's: hurt props are unknown.
        assert_eq!(
            error_of(r#"("hurt_radius", 0.4)"#),
            "the character \"player\": unknown prop \"hurt_radius\""
        );
    }

    #[test]
    fn hit_checks_run_in_the_documented_order() {
        // The new values come after the lock ones, in table order.
        assert!(
            error_of(r#"("attack_arc", 0), ("switch_threshold", 0)"#).contains("switch_threshold")
        );
        assert!(error_of(r#"("swing_low", -1), ("attack_arc", 0)"#).contains("attack_arc"));
        // Values, then the lock ranges, then the swing band, then distances.
        assert!(error_of(r#"("swing_low", 2), ("heavy_arc", 0)"#).contains("heavy_arc"));
        assert!(error_of(r#"("swing_low", 2), ("lock_range", 25)"#).contains("lock_break_range"));
        assert!(error_of(r#"("sprint_speed", 30), ("swing_low", 2)"#).contains("swing_low <"));
        // In a node: lock_height, hurt_radius, hurt_height values, pairing,
        // the root rule; a lock target's root error beats a hurtbox's.
        let node = |props: &str| error_with("", &format!(r#"(name: "t", props: [{props}]),"#));
        assert!(node(r#"("hurt_radius", 0), ("lock_height", 0)"#).contains("\"lock_height\""));
        assert!(node(r#"("hurt_height", 0), ("hurt_radius", 0)"#).contains("\"hurt_radius\""));
        assert!(node(r#"("hurt_height", 0)"#).contains("\"hurt_height\" must be greater"));
        let child = |props: &str| {
            error_with(
                "",
                &format!(r#"(name: "g", children: [(name: "t", props: [{props}])]),"#),
            )
        };
        assert!(child(r#"("hurt_radius", 0.4)"#).contains("needs both"));
        assert!(
            child(r#"("lock_height", 1), ("hurt_radius", 0.4), ("hurt_height", 1.8)"#)
                .starts_with("the lock target \"g/t\"")
        );
    }

    // ---------------------------------------------------- health, poise, death

    /// [`PROPS`] plus attacks one tick per phase (startup, active,
    /// recovery), costing 1 stamina and sliding 1/16 m: pressed on tick
    /// `t`, one hits on `t + 1`, and a press on `t + 3` starts the next.
    /// Light: 3 damage, impact 1. Heavy: 5 damage, impact 0.
    fn health_props() -> String {
        format!(
            r#"{PROPS}, ("attack_startup_ticks", 1), ("attack_active_ticks", 1),
            ("attack_recovery_ticks", 1), ("attack_distance", 0.0625), ("attack_cost", 1),
            ("heavy_startup_ticks", 1), ("heavy_active_ticks", 1),
            ("heavy_recovery_ticks", 1), ("heavy_distance", 0.0625), ("heavy_cost", 1),
            ("attack_damage", 3), ("attack_impact", 1), ("heavy_damage", 5), ("heavy_impact", 0)"#
        )
    }

    /// A root lock-on target and hurtbox `name` on the ground at `(x, z)`,
    /// turned 30° (so its pose has a base rotation), with the props
    /// `health`, a mesh `body` and an `hp_bar` 1 m wide centered at x = 0.5:
    /// its left end at x = 0, and at half health 0.5 wide at x = 0.25, all
    /// exact.
    fn dummy(name: &str, (x, z): (f32, f32), health: &str) -> String {
        format!(
            r#"(name: "{name}", position: ({x:?}, 0.0, {z:?}), rotation: (0.0, 30.0, 0.0),
                props: [("lock_height", 1.5), ("hurt_radius", 0.25), ("hurt_height", 1.8),
                    {health}],
                children: [
                    (name: "body", mesh: Cube, color: (0.2, 0.4, 0.6)),
                    (name: "hp_bar", position: (0.5, 2.0, 0.0), scale: (1.0, 0.125, 0.125),
                        mesh: Cube, color: (0.8, 0.1, 0.1)),
                ]),"#
        )
    }

    const BODY: Vec3 = Vec3::new(0.2, 0.4, 0.6);
    const BAR: Vec3 = Vec3::new(0.8, 0.1, 0.1);

    /// A run with one [`dummy`] "d" 1.5 m straight ahead.
    fn dummy_run(health: &str) -> Run {
        Run::on(&scene_with(
            &health_props(),
            &dummy("d", (0.0, -1.5), health),
        ))
    }

    fn health_of(s: &GameState, name: &str) -> HealthState {
        hurtbox_state(s, name).health.expect("the hurtbox has hp")
    }

    #[test]
    fn hp_drops_by_each_hit_to_zero_then_comes_back_full_after_revive_ticks() {
        // hp 10, 3 a hit on ticks 2, 5, 8 and 11: 7, 4, 1, then 0, not -2.
        // Dead from tick 11 for 6 ticks: the 7th, tick 17, is back at full.
        let mut run = dummy_run(r#"("hp", 10), ("poise", 100), ("revive_ticks", 6)"#);
        let states = press_on(&mut run, ATTACK, &[1, 4, 7, 10], 18, &NONE);
        for (t, hp) in [(2, 7.0), (5, 4.0), (8, 1.0), (10, 1.0)] {
            let h = health_of(&states[t - 1], "d");
            assert_eq!((h.hp, h.state), (hp, Condition::Ok), "tick {t}");
        }
        for t in 11..=16 {
            let h = health_of(&states[t - 1], "d");
            assert_eq!(
                (h.hp, h.state, h.state_ticks),
                (0.0, Condition::Dead, 17 - t as u32),
                "tick {t}"
            );
        }
        for t in 17..=18 {
            let h = health_of(&states[t - 1], "d");
            assert_eq!(
                (h.hp, h.state, h.state_ticks),
                (10.0, Condition::Ok, 0),
                "tick {t}"
            );
        }
        assert_eq!(hurtbox_state(&states[17], "d").hits, 4);
    }

    #[test]
    fn hit_of_exactly_the_hp_left_kills() {
        // Two heavy hits of 5 on hp 10 (poise 0, but heavy impact 0).
        let mut run = dummy_run(r#"("hp", 10)"#);
        let states = press_on(&mut run, HEAVY, &[1, 4], 5, &NONE);
        assert_eq!(health_of(&states[1], "d").hp, 5.0);
        assert_eq!(health_of(&states[3], "d").state, Condition::Ok);
        let h = health_of(&states[4], "d");
        assert_eq!((h.hp, h.state, h.state_ticks), (0.0, Condition::Dead, 180));
    }

    #[test]
    fn impacts_up_to_the_poise_do_not_stagger_and_past_it_stagger_for_stagger_ticks() {
        // Poise 2, impact 1 on ticks 2, 5 and 8: 1, 2 (equal: no stagger),
        // then 3 > 2: staggered on ticks 8..=12, the impact emptied.
        let mut run = dummy_run(r#"("hp", 100), ("poise", 2), ("stagger_ticks", 5)"#);
        let states = press_on(&mut run, ATTACK, &[1, 4, 7], 13, &NONE);
        for (t, impact) in [(2, 1.0), (5, 2.0), (7, 2.0)] {
            let h = health_of(&states[t - 1], "d");
            assert_eq!((h.state, h.impact), (Condition::Ok, impact), "tick {t}");
        }
        assert_eq!(
            health_of(&states[7], "d"),
            HealthState {
                hp: 91.0,
                state: Condition::Stagger,
                state_ticks: 5,
                impact: 0.0,
                impact_wait: 60
            }
        );
        for t in 8..=12 {
            let h = health_of(&states[t - 1], "d");
            assert_eq!(
                (h.state, h.state_ticks),
                (Condition::Stagger, 13 - t as u32),
                "tick {t}"
            );
        }
        let h = health_of(&states[12], "d");
        assert_eq!((h.state, h.state_ticks), (Condition::Ok, 0));
    }

    #[test]
    fn stagger_starts_over_when_impacts_add_up_past_the_poise_again() {
        // Poise 1: staggered on tick 5 for 8 ticks. The hit on tick 8 adds
        // up to 1 (no new stagger, 5 ticks left); the one on tick 11 to 2:
        // staggered again, on ticks 11..=18.
        let mut run = dummy_run(r#"("hp", 100), ("poise", 1), ("stagger_ticks", 8)"#);
        let states = press_on(&mut run, ATTACK, &[1, 4, 7, 10], 19, &NONE);
        assert_eq!(
            health_of(&states[7], "d"),
            HealthState {
                hp: 91.0,
                state: Condition::Stagger,
                state_ticks: 5,
                impact: 1.0,
                impact_wait: 60
            }
        );
        for t in 11..=18 {
            let h = health_of(&states[t - 1], "d");
            assert_eq!(
                (h.state, h.state_ticks, h.impact),
                (Condition::Stagger, 19 - t as u32, 0.0),
                "tick {t}"
            );
        }
        assert_eq!(health_of(&states[18], "d").state, Condition::Ok);
    }

    #[test]
    fn zero_poise_staggers_on_every_impact_but_never_on_zero_impact() {
        // Poise 0 (the default), stagger_ticks 30 (the default).
        let mut run = dummy_run(r#"("hp", 100)"#);
        let states = press_on(&mut run, ATTACK, &[1, 4], 5, &NONE);
        for t in [2, 5] {
            let h = health_of(&states[t - 1], "d");
            assert_eq!(
                (h.state, h.state_ticks),
                (Condition::Stagger, 30),
                "tick {t}"
            );
        }
        let mut run = dummy_run(r#"("hp", 100)"#);
        let states = press_on(&mut run, HEAVY, &[1, 4, 7], 9, &NONE);
        for (i, s) in states.iter().enumerate() {
            let h = health_of(s, "d");
            assert_eq!((h.state, h.impact), (Condition::Ok, 0.0), "tick {}", i + 1);
        }
        assert_eq!(health_of(&states[8], "d").hp, 85.0);
    }

    #[test]
    fn added_up_impact_is_forgotten_poise_reset_ticks_after_the_last_hit() {
        // Hits on ticks 2 and 5, poise_reset_ticks 8: still 2 on tick 12,
        // 0 on tick 13.
        let mut run = dummy_run(r#"("hp", 100), ("poise", 100), ("poise_reset_ticks", 8)"#);
        let states = press_on(&mut run, ATTACK, &[1, 4], 14, &NONE);
        let h = health_of(&states[3], "d");
        assert_eq!((h.impact, h.impact_wait), (1.0, 6));
        let h = health_of(&states[11], "d");
        assert_eq!((h.impact, h.impact_wait), (2.0, 1));
        for s in &states[12..] {
            let h = health_of(s, "d");
            assert_eq!((h.impact, h.impact_wait), (0.0, 0));
        }
    }

    #[test]
    fn killing_hit_adds_no_impact_and_empties_it() {
        let mut run = dummy_run(r#"("hp", 6), ("poise", 100)"#);
        let states = press_on(&mut run, ATTACK, &[1, 4], 5, &NONE);
        let h = health_of(&states[1], "d");
        assert_eq!((h.hp, h.impact, h.impact_wait), (3.0, 1.0, 60));
        assert_eq!(
            health_of(&states[4], "d"),
            HealthState {
                hp: 0.0,
                state: Condition::Dead,
                state_ticks: 180,
                impact: 0.0,
                impact_wait: 0
            }
        );
    }

    #[test]
    fn dead_hurtbox_is_not_hit_nor_flashes_until_it_revives() {
        // Killed on tick 2, dead on ticks 2..=11; the swing on tick 5 goes
        // through it. Back on tick 12, hit and killed again on tick 14.
        let mut run = dummy_run(r#"("hp", 3), ("revive_ticks", 10)"#);
        for t in 1..=14_u32 {
            let actions: &[&str] = if [1, 4, 13].contains(&t) {
                &[ATTACK]
            } else {
                &[]
            };
            let s = run.hold(actions, &NONE);
            let h = hurtbox_state(&s, "d");
            let (hits, last, color) = match t {
                1 => (0, None, BODY),
                2..=11 => (1, Some(2), BODY * 0.4),
                12..=13 => (1, Some(2), BODY),
                _ => (2, Some(14), BODY * 0.4),
            };
            assert_eq!(h.hits, hits, "tick {t}");
            assert_eq!(h.last_hit.map(|h| h.tick), last, "tick {t}");
            assert_eq!(color_of(&run, "d/body"), color, "tick {t}");
            if (2..=11).contains(&t) {
                assert_eq!(health_of(&s, "d").hp, 0.0, "tick {t}");
            }
        }
    }

    #[test]
    fn killing_the_locked_target_lets_go_of_it_on_that_tick() {
        let mut run = dummy_run(r#"("hp", 3)"#);
        assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), Some("d"));
        assert_eq!(lock_of(&run.hold(&[ATTACK], &NONE)), Some("d")); // startup
        let s = run.hold(&[], &NONE); // the hit
        assert_eq!(health_of(&s, "d").state, Condition::Dead);
        assert_eq!(lock_of(&s), None);
    }

    #[test]
    fn dead_target_cannot_be_locked_on_until_it_revives() {
        // Killed on tick 2, dead on ticks 2..=11, nothing else to lock on
        // to; it revives in step 0b of tick 12, before the lock key is read.
        let mut run = dummy_run(r#"("hp", 3), ("revive_ticks", 10)"#);
        run.hold(&[ATTACK], &NONE);
        run.hold(&[], &NONE);
        assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), None); // tick 3
        run.ticks(6, false, &NONE); // ticks 4..=9
        assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), None); // tick 10
        let s = run.hold(&[], &NONE); // tick 11
        assert_eq!(health_of(&s, "d").state, Condition::Dead);
        let s = run.hold(&[LOCK], &NONE); // tick 12
        assert_eq!(
            (lock_of(&s), health_of(&s, "d").state),
            (Some("d"), Condition::Ok)
        );
        // With another target in range, that one instead.
        let nodes = format!(
            "{}{}",
            dummy("d", (0.0, -1.5), r#"("hp", 3)"#),
            target("far", 3.0, -10.0)
        );
        let mut run = Run::on(&scene_with(&health_props(), &nodes));
        run.hold(&[ATTACK], &NONE);
        run.hold(&[], &NONE);
        assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), Some("far"));
    }

    #[test]
    fn switching_targets_skips_dead_ones() {
        // Locked on "mid" straight ahead, a flick to the right goes to "d"
        // (30° right), the nearest in direction, or past it once it is dead
        // to "far" (45° right).
        let (x, _, z) = at(-30.0, 1.5);
        let nodes = format!(
            "{}{}{}",
            target("mid", 0.0, -10.0),
            dummy("d", (x, z), r#"("hp", 3)"#),
            target("far", 5.0, -5.0)
        );
        for (kill, next) in [(false, "d"), (true, "far")] {
            let mut run = Run::on(&scene_with(&health_props(), &nodes));
            let first: &[&str] = if kill { &[ATTACK] } else { &[] };
            run.hold(first, &NONE);
            let s = run.hold(&[], &NONE);
            assert_eq!(health_of(&s, "d").state == Condition::Dead, kill);
            assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), Some("mid"));
            let s = run.hold(&[], &[(LOOK_X, 0.12)]);
            assert_eq!(lock_of(&s), Some(next), "kill {kill}");
        }
    }

    #[test]
    fn hurtbox_without_hp_has_no_health_and_keeps_its_pose() {
        let mut run = Run::on(&scene_with(
            &health_props(),
            &hurtbox("h", (0.0, 0.0, -1.5), 0.25, 1.8),
        ));
        let states = press_on(&mut run, ATTACK, &[1, 4, 7], 9, &NONE);
        let h = hurtbox_state(&states[8], "h");
        assert_eq!((h.hits, h.health), (3, None));
        assert_eq!(node_rotation(&run, "h"), Quat::IDENTITY);
    }

    fn node_rotation(run: &Run, path: &str) -> Quat {
        let scene = run.game.scene();
        scene.transform(scene.find(path).unwrap()).rotation
    }

    fn bar_of(run: &Run, path: &str) -> (loom_scene::Transform, bool) {
        let scene = run.game.scene();
        let id = scene.find(&format!("{path}/hp_bar")).unwrap();
        (*scene.transform(id), scene.is_drawn(id))
    }

    fn quat_bits(q: Quat) -> [u32; 4] {
        q.to_array().map(f32::to_bits)
    }

    /// Where the top of the node at `path` points: its rotation applied to
    /// +Y.
    fn up_of(run: &Run, path: &str) -> Vec3 {
        node_rotation(run, path) * Vec3::Y
    }

    fn assert_near(actual: Vec3, expected: Vec3, what: &str) {
        assert!(
            actual.abs_diff_eq(expected, 1e-5),
            "{what}: {actual} != {expected}"
        );
    }

    /// `actual` and `expected` turn every vector the same way (up to
    /// rounding; `q` and `-q` are the same rotation).
    fn assert_same_rotation(actual: Quat, expected: Quat) {
        assert!(
            actual.dot(expected).abs() > 1.0 - 1e-6,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn staggered_hurtbox_tips_away_from_the_character_then_stands_up_again() {
        // Hit from +Z (the dummy straight ahead of the character, facing 0),
        // staggered on ticks 2..=6: its top tips 20° towards -Z, away.
        let mut run = dummy_run(r#"("hp", 100), ("stagger_ticks", 5)"#);
        let base = node_rotation(&run, "d");
        assert!(base.abs_diff_eq(Quat::from_rotation_y(30.0_f32.to_radians()), 1e-6));
        let (sin, cos) = (
            libm::sinf(20.0_f32.to_radians()),
            libm::cosf(20.0_f32.to_radians()),
        );
        for t in 1..=8_u32 {
            let actions: &[&str] = if t == 1 { &[ATTACK] } else { &[] };
            run.hold(actions, &NONE);
            if (2..=6).contains(&t) {
                assert_near(
                    up_of(&run, "d"),
                    Vec3::new(0.0, cos, -sin),
                    &format!("tick {t}"),
                );
            } else {
                // Standing: the scene rotation, bit for bit.
                assert_eq!(
                    quat_bits(node_rotation(&run, "d")),
                    quat_bits(base),
                    "tick {t}"
                );
            }
        }
    }

    #[test]
    fn dead_hurtbox_lies_flat_away_from_the_character_dimmed_without_its_bar() {
        // Killed on tick 2 (dimmed at once, not white), dead on ticks
        // 2..=11, lying towards -Z, away from the character; back on tick 12
        // with the scene's colors, pose and bar.
        let mut run = dummy_run(r#"("hp", 3), ("revive_ticks", 10)"#);
        let base = node_rotation(&run, "d");
        let (full_bar, _) = bar_of(&run, "d");
        for t in 1..=13_u32 {
            let actions: &[&str] = if t == 1 { &[ATTACK] } else { &[] };
            run.hold(actions, &NONE);
            let dead = (2..=11).contains(&t);
            if dead {
                assert_near(up_of(&run, "d"), Vec3::NEG_Z, &format!("tick {t}"));
            } else {
                assert_eq!(
                    quat_bits(node_rotation(&run, "d")),
                    quat_bits(base),
                    "tick {t}"
                );
            }
            let color = if dead { BODY * 0.4 } else { BODY };
            assert_eq!(color_of(&run, "d/body"), color, "tick {t}");
            let (bar, drawn) = bar_of(&run, "d");
            assert_eq!(drawn, !dead, "tick {t}");
            if !dead {
                assert_eq!(bar, full_bar, "tick {t}");
                assert_eq!(color_of(&run, "d/hp_bar"), BAR, "tick {t}");
            }
        }
    }

    #[test]
    fn hurtbox_hit_from_the_side_falls_to_the_far_side() {
        // The character turned to face -X (facing 90°), the dummy 1.5 m
        // that way: killed on tick 2, it lies towards -X.
        let src = scene_with(&health_props(), &dummy("d", (-1.5, 0.0), r#"("hp", 3)"#)).replace(
            r#"(name: "player","#,
            r#"(name: "player", rotation: (0.0, 90.0, 0.0),"#,
        );
        let mut run = Run::on(&src);
        let base = node_rotation(&run, "d");
        let states = press_on(&mut run, ATTACK, &[1], 2, &NONE);
        assert_eq!(health_of(&states[1], "d").state, Condition::Dead);
        assert_near(up_of(&run, "d"), Vec3::NEG_X, "lying");
        // Tipped over about -Z, which turns +Y to -X, and not turned about
        // its own Y: the scene rotation, then only the tilt.
        assert_same_rotation(
            node_rotation(&run, "d"),
            Quat::from_rotation_z(FRAC_PI_2) * base,
        );
    }

    #[test]
    fn fall_direction_does_not_depend_on_how_the_hurtbox_faces() {
        // The same kill from +Z, the dummy turned 0°, 30° or 90°: it lies
        // towards -Z each time.
        for yaw in ["0.0", "30.0", "90.0"] {
            let nodes = dummy("d", (0.0, -1.5), r#"("hp", 3)"#).replace(
                "rotation: (0.0, 30.0, 0.0)",
                &format!("rotation: (0.0, {yaw}, 0.0)"),
            );
            let mut run = Run::on(&scene_with(&health_props(), &nodes));
            press_on(&mut run, ATTACK, &[1], 2, &NONE);
            assert_near(up_of(&run, "d"), Vec3::NEG_Z, &format!("turned {yaw}°"));
            let yaw: f32 = yaw.parse().unwrap();
            assert_same_rotation(
                node_rotation(&run, "d"),
                Quat::from_rotation_x(-FRAC_PI_2) * Quat::from_rotation_y(yaw.to_radians()),
            );
        }
    }

    #[test]
    fn hurtbox_killed_where_the_character_stands_falls_towards_minus_z() {
        // After the slide the character stands at z = -0.0625, right on the
        // dummy: no direction from it, so -Z (not +Z, from atan2(-0, -0)).
        let nodes = dummy("d", (0.0, -0.0625), r#"("hp", 3)"#);
        let mut run = Run::on(&scene_with(&health_props(), &nodes));
        let states = press_on(&mut run, ATTACK, &[1], 2, &NONE);
        assert_eq!(states[1].player.position, Vec3::new(0.0, 0.0, -0.0625));
        assert_eq!(health_of(&states[1], "d").state, Condition::Dead);
        assert_near(up_of(&run, "d"), Vec3::NEG_Z, "lying");
    }

    #[test]
    fn hp_bar_shrinks_from_the_right_and_does_not_flash() {
        let mut run = dummy_run(r#"("hp", 10)"#);
        // Before any hit: as loaded.
        let (loaded, _) = bar_of(&run, "d");
        assert_eq!(loaded.position, Vec3::new(0.5, 2.0, 0.0));
        assert_eq!(loaded.scale, Vec3::new(1.0, 0.125, 0.125));
        run.hold(&[HEAVY], &NONE);
        assert_eq!(bar_of(&run, "d"), (loaded, true));
        // A heavy hit of 5 on tick 2: half the hp, half the width, the left
        // end still at 0.5 - 1 / 2 = 0.
        run.hold(&[], &NONE);
        let (bar, drawn) = bar_of(&run, "d");
        assert!(drawn);
        assert_eq!(bar.scale, Vec3::new(0.5, 0.125, 0.125));
        assert_eq!(bar.position, Vec3::new(0.25, 2.0, 0.0));
        assert_eq!(bar.position.x - bar.scale.x / 2.0, 0.0);
        assert_eq!(bar.rotation, loaded.rotation);
        assert_eq!(color_of(&run, "d/body"), Vec3::ONE);
        assert_eq!(color_of(&run, "d/hp_bar"), BAR);
    }

    #[test]
    fn damage_and_impact_props_are_checked_with_exact_messages() {
        for (props, problem) in [
            (
                r#"("attack_damage", 0)"#,
                "prop \"attack_damage\" must be greater than 0, got 0",
            ),
            (
                r#"("heavy_damage", -1)"#,
                "prop \"heavy_damage\" must be greater than 0, got -1",
            ),
            (
                r#"("attack_impact", -1)"#,
                "prop \"attack_impact\" must be at least 0, got -1",
            ),
            (
                r#"("heavy_impact", -0.5)"#,
                "prop \"heavy_impact\" must be at least 0, got -0.5",
            ),
        ] {
            assert_eq!(
                error_of(props),
                format!("the character \"player\": {problem}")
            );
        }
        for props in [
            r#"("attack_damage", 0.5)"#,
            r#"("heavy_damage", 0.5)"#,
            r#"("attack_impact", 0)"#,
            r#"("heavy_impact", 0)"#,
        ] {
            assert!(Game::from_source(&scene(props)).is_ok(), "{props}");
        }
    }

    const HURT: &str = r#"("hurt_radius", 0.4), ("hurt_height", 1.8)"#;

    /// The error for a root node "t" with the hurt props and `props`.
    fn hurtbox_error(props: &str) -> String {
        error_with("", &format!(r#"(name: "t", props: [{HURT}, {props}]),"#))
    }

    #[test]
    fn health_props_are_checked_with_exact_messages() {
        let whole = "must be a whole number of at least 1";
        for (props, problem) in [
            (
                r#"("hp", 0)"#,
                "prop \"hp\" must be greater than 0, got 0".to_owned(),
            ),
            (
                r#"("hp", 10), ("poise", -0.5)"#,
                "prop \"poise\" must be at least 0, got -0.5".to_owned(),
            ),
            (
                r#"("hp", 10), ("stagger_ticks", 0)"#,
                format!("prop \"stagger_ticks\" {whole}, got 0"),
            ),
            (
                r#"("hp", 10), ("poise_reset_ticks", 1.5)"#,
                format!("prop \"poise_reset_ticks\" {whole}, got 1.5"),
            ),
            (
                r#"("hp", 10), ("revive_ticks", 0.5)"#,
                format!("prop \"revive_ticks\" {whole}, got 0.5"),
            ),
        ] {
            assert_eq!(hurtbox_error(props), format!("the node \"t\": {problem}"));
        }
        let accepted = r#"("hp", 0.5), ("poise", 0), ("stagger_ticks", 1),
            ("poise_reset_ticks", 1), ("revive_ticks", 1)"#;
        let nodes = format!(r#"(name: "t", props: [{HURT}, {accepted}]),"#);
        assert!(Game::from_source(&scene_with("", &nodes)).is_ok());
    }

    #[test]
    fn health_nodes_are_checked_with_exact_messages() {
        for (nodes, message) in [
            (
                r#"(name: "t", props: [("hp", 10)]),"#.to_owned(),
                "the node \"t\": hp needs hurt_radius and hurt_height",
            ),
            (
                r#"(name: "t", props: [("lock_height", 1), ("poise", 5)]),"#.to_owned(),
                "the node \"t\": prop \"poise\" needs hp",
            ),
            (
                // The first of the four in list order, not in the node's.
                format!(
                    r#"(name: "t", props: [{HURT}, ("revive_ticks", 5), ("stagger_ticks", 5)]),"#
                ),
                "the node \"t\": prop \"stagger_ticks\" needs hp",
            ),
            (
                format!(
                    r#"(name: "t", props: [{HURT}, ("hp", 10)], children: [(name: "hp_bar")]),"#
                ),
                "the node \"t\": its hp_bar needs a mesh",
            ),
        ] {
            assert_eq!(error_with("", &nodes), message);
        }
        // "hp_bar" means nothing on a node without hp.
        let nodes = format!(
            r#"(name: "t", props: [{HURT}], children: [(name: "hp_bar")]),
            (name: "u", children: [(name: "hp_bar")]),"#
        );
        assert!(Game::from_source(&scene_with("", &nodes)).is_ok());
    }

    #[test]
    fn health_checks_run_in_the_documented_order() {
        let node = |props: &str| error_with("", &format!(r#"(name: "t", props: [{props}]),"#));
        let child = |props: &str, children: &str| {
            error_with(
                "",
                &format!(
                    r#"(name: "g", children: [(name: "t", props: [{props}],
                        children: [{children}])]),"#
                ),
            )
        };
        // Unknown names, then the values in list order.
        assert!(node(r#"("hp", 0), ("hq", 1)"#).contains("unknown prop \"hq\""));
        assert!(node(r#"("revive_ticks", 0), ("hp", 0)"#).contains("\"hp\" must be greater"));
        assert!(node(r#"("stagger_ticks", 0), ("poise", -1)"#).contains("\"poise\" must be at"));
        // Values, then the hurt pair, then hp needing a hurtbox.
        assert!(node(r#"("hurt_radius", 0.4), ("hp", 0)"#).contains("\"hp\" must be greater"));
        assert!(node(r#"("hurt_radius", 0.4), ("hp", 10)"#).contains("needs both"));
        // Then hp and the last four props, then the root rule.
        assert!(child(r#"("hp", 10)"#, "").contains("hp needs hurt_radius"));
        assert!(child(&format!("{HURT}, (\"poise\", 1)"), "").contains("needs hp"));
        // The root rule, then the hp bar.
        assert!(
            child(&format!("{HURT}, (\"hp\", 10)"), r#"(name: "hp_bar")"#)
                .starts_with("the hurtbox \"g/t\" must be a root node")
        );
    }
}
