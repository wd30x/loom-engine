//! Stamina, roll, sprint, backstep, attacks, lock-on, hits, damage and the
//! character's own health: the parameters, read from the character node's props with defaults, and the
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
const PARAMS: [(&str, f64, Rule); 50] = [
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
    ("hp", 100.0, Rule::Positive),
    ("poise", 0.0, Rule::NotNegative),
    ("stagger_ticks", 30.0, Rule::AtLeast(1)),
    ("poise_reset_ticks", 60.0, Rule::AtLeast(1)),
    ("respawn_ticks", 120.0, Rule::AtLeast(1)),
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
    /// The character's own health: full `hp`, poise, and how long a stagger
    /// lasts, impacts add up and death lasts before the respawn.
    pub hp: f32,
    pub poise: f32,
    pub stagger_ticks: u32,
    pub poise_reset_ticks: u32,
    pub respawn_ticks: u32,
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
            hp: get("hp") as f32,
            poise: get("poise") as f32,
            stagger_ticks: ticks("stagger_ticks"),
            poise_reset_ticks: ticks("poise_reset_ticks"),
            respawn_ticks: ticks("respawn_ticks"),
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
        LastHit, MOVE_X, MOVE_Y, PlayerHit, ROLL, SwingerState,
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
            let p = &s.player;
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
        let p = &states[5].player; // tick 6: the last startup tick
        assert_eq!(
            (p.action, p.attack_phase),
            (Action::Heavy, Some(AttackPhase::Startup))
        );
        assert_eq!((p.position, p.stamina, p.combo), (Vec3::ZERO, 75.0, 0));
        let p = &states[13].player; // tick 14: the last active tick
        assert_eq!(p.attack_phase, Some(AttackPhase::Active));
        assert_eq!(p.position, Vec3::new(0.0, 0.0, -1.0));
        let p = &states[20].player; // tick 21
        assert_eq!(
            (p.action_tick, p.attack_phase),
            (21, Some(AttackPhase::Recovery))
        );
        let p = &states[21].player; // tick 22 ends it
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
        let s = &states[14].player;
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
            let p = &states[tick - 1].player;
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
        let p = &states[5].player; // tick 15
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
             hurt_height, hp, poise, stagger_ticks, poise_reset_ticks, revive_ticks, swing_damage, \
             swing_impact, swing_startup_ticks, swing_active_ticks, swing_recovery_ticks, \
             swing_reach, swing_arc, swing_low, swing_high)"
        );
        // Any node but the character, target or not. (Until G3.2 this used
        // "hp", which G3.3 made a known prop.)
        assert_eq!(
            error_with(
                "",
                r#"(name: "g", children: [(name: "t", props: [("armor", 3)])]),"#
            ),
            "the node \"g/t\": unknown prop \"armor\" (known: lock_height, hurt_radius, \
             hurt_height, hp, poise, stagger_ticks, poise_reset_ticks, revive_ticks, swing_damage, \
             swing_impact, swing_startup_ticks, swing_active_ticks, swing_recovery_ticks, \
             swing_reach, swing_arc, swing_low, swing_high)"
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
                 hurt_height, hp, poise, stagger_ticks, poise_reset_ticks, revive_ticks, swing_damage, \
                 swing_impact, swing_startup_ticks, swing_active_ticks, swing_recovery_ticks, \
                 swing_reach, swing_arc, swing_low, swing_high)",
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

    // ------------------------------------------- the character hit, swingers

    /// A root swinger `name` standing at `(x, y, z)`, not turned (it faces
    /// -Z), dealing 3 damage, with the swing props `props` and an arm on its
    /// center line (turned 10° about X, so its pose is not the identity).
    fn swinger(name: &str, (x, y, z): (f32, f32, f32), props: &str) -> String {
        format!(
            r#"(name: "{name}", position: ({x:?}, {y:?}, {z:?}),
                props: [("swing_damage", 3), {props}],
                children: [(name: "arm", position: (0.0, 1.0, -1.0), rotation: (10.0, 0.0, 0.0),
                    scale: (0.1, 0.1, 2.0), mesh: Cube, color: (0.4, 0.3, 0.2))]),"#
        )
    }

    /// Two ticks each of startup, active and recovery, sweeping 180° in two
    /// 90° slices, right to left: a round of 6 ticks, active on its ticks 3
    /// and 4.
    const RHYTHM: &str = r#"("swing_startup_ticks", 2), ("swing_active_ticks", 2),
        ("swing_recovery_ticks", 2), ("swing_arc", 180)"#;

    /// Swing props: `startup` ticks, then one active tick sweeping all
    /// around within 10 m, then `recovery` ticks. On the character's spot,
    /// such a swinger hits it on tick `startup + 1` of every round, however
    /// far it moved or whichever way it faces.
    fn all_round(startup: u32, recovery: u32) -> String {
        format!(
            r#"("swing_startup_ticks", {startup}), ("swing_active_ticks", 1),
            ("swing_recovery_ticks", {recovery}), ("swing_arc", 360), ("swing_reach", 10)"#
        )
    }

    /// A run with `props` on the character (and nothing else: tests that
    /// want [`PROPS`] include it) and `nodes`.
    fn hit_run(props: &str, nodes: &str) -> Run {
        Run::on(&scene_with(props, nodes))
    }

    const ARM: Vec3 = Vec3::new(0.4, 0.3, 0.2);
    const WARN: Vec3 = Vec3::new(1.0, 0.55, 0.1);

    fn full_health(hp: f32) -> HealthState {
        HealthState {
            hp,
            state: Condition::Ok,
            state_ticks: 0,
            impact: 0.0,
            impact_wait: 0,
        }
    }

    fn hit_by(tick: u64, by: &str) -> Option<PlayerHit> {
        Some(PlayerHit {
            tick,
            by: by.to_owned(),
        })
    }

    #[test]
    fn swinger_counts_its_round_from_the_first_tick_and_starts_over_after_the_period() {
        // Far from the character, and no hurtbox: it swings on its own.
        let mut run = hit_run("", &swinger("s", (0.0, 0.0, -20.0), RHYTHM));
        let s = run.game.state();
        assert_eq!((s.swingers[0].swing_tick, s.swingers[0].phase), (0, None));
        for t in 1..=13_u32 {
            let s = run.tick(false, &NONE);
            let swing_tick = (t - 1) % 6 + 1;
            let phase = match swing_tick {
                1 | 2 => AttackPhase::Startup,
                3 | 4 => AttackPhase::Active,
                _ => AttackPhase::Recovery,
            };
            assert_eq!(
                s.swingers,
                [SwingerState {
                    node: "s".to_owned(),
                    swing_tick,
                    phase: Some(phase)
                }],
                "tick {t}"
            );
            assert!(s.hurtboxes.is_empty());
            assert_eq!(s.player.hits, 0);
        }
    }

    #[test]
    fn swinger_hits_the_right_half_on_its_first_active_tick_and_the_left_on_its_second() {
        // The character 1.5 m in front of the swinger and 0.5 m to its
        // right (+X) or left: about 18° off the center line, and 0.5 m (more
        // than the capsule radius 0.35) from the 0° edge between the slices.
        for (x, tick) in [(-0.5, 3), (0.5, 4)] {
            let mut run = hit_run(r#"("hp", 100)"#, &swinger("s", (x, 0.0, 1.5), RHYTHM));
            for t in 1..=6 {
                let s = run.tick(false, &NONE);
                assert_eq!(s.player.hits, u32::from(t >= tick), "x {x}, tick {t}");
            }
            assert_eq!(run.game.state().player.last_hit, hit_by(tick, "s"));
        }
    }

    #[test]
    fn swinger_hits_the_character_once_a_round_and_again_the_next_round() {
        // On the character's spot: both active ticks touch it; hits on
        // tick 3 (not 4) and tick 9.
        let mut run = hit_run(r#"("hp", 100)"#, &swinger("s", (0.0, 0.0, 0.0), RHYTHM));
        for t in 1..=12 {
            let s = run.tick(false, &NONE);
            let (hits, last) = match t {
                1..=2 => (0, None),
                3..=8 => (1, hit_by(3, "s")),
                _ => (2, hit_by(9, "s")),
            };
            assert_eq!((s.player.hits, s.player.last_hit), (hits, last), "tick {t}");
        }
    }

    #[test]
    fn swing_hits_the_character_when_its_band_just_meets_the_capsule() {
        // Band 0.25-1.5 m above the swinger, the capsule 0-1.8 m, all exact:
        // the band's top on the feet (swinger at -1.5) or its bottom on the
        // head (at 1.55) is a hit, both ends being closed; 1/16 m further
        // is not.
        let band = format!(r#"{RHYTHM}, ("swing_low", 0.25), ("swing_high", 1.5)"#);
        let hits = |y: f32| {
            let mut run = hit_run(r#"("hp", 100)"#, &swinger("s", (0.0, y, 0.0), &band));
            run.ticks(6, false, &NONE).player.hits
        };
        assert_eq!(hits(-1.5), 1);
        assert_eq!(hits(-1.5625), 0);
        assert_eq!(hits(1.55), 1);
        assert_eq!(hits(1.625), 0);
    }

    #[test]
    fn swinger_does_not_hit_the_character_behind_it() {
        // The swinger 1.5 m ahead of the character and facing away (-Z):
        // the character is 1.5 m from both edges of its 180° sector.
        let mut run = hit_run(r#"("hp", 100)"#, &swinger("s", (0.0, 0.0, -1.5), RHYTHM));
        assert_eq!(run.ticks(12, false, &NONE).player.hits, 0);
    }

    /// Props that make a roll and a backstep 8 ticks long, invulnerable on
    /// their first 4, and moving only 0.01 m a tick: the character stays
    /// within 0.08 m of a swinger on its spot, so every slice touches it.
    const SHORT_DODGES: &str = r#"("roll_ticks", 8), ("roll_distance", 0.08),
        ("roll_invuln_to", 4), ("backstep_ticks", 8), ("backstep_distance", 0.08),
        ("backstep_invuln_to", 4)"#;

    /// Taps the roll key on ticks 1-2 (rolling forward, or backstepping
    /// without `axes`, from tick 2: invulnerable on ticks 2..=5), then
    /// waits; returns every tick's state.
    fn dodge(run: &mut Run, ticks: u32, axes: &[(&str, f32)]) -> Vec<GameState> {
        (1..=ticks)
            .map(|t| match t {
                1 => run.tick(true, &NONE),
                2 => run.tick(false, axes),
                _ => run.tick(false, &NONE),
            })
            .collect()
    }

    #[test]
    fn dodge_invulnerable_on_the_only_active_tick_is_not_hit_that_round() {
        // Active on tick 5 (invulnerable: not hit) and 18 (hit).
        for (axes, action) in [(&FORWARD[..], Action::Roll), (&NONE[..], Action::Backstep)] {
            let nodes = swinger("s", (0.0, 0.0, 0.0), &all_round(4, 8));
            let mut run = hit_run(SHORT_DODGES, &nodes);
            let states = dodge(&mut run, 18, axes);
            let s = &states[4];
            assert_eq!(
                (s.player.action, s.player.invulnerable, s.player.hits),
                (action, true, 0),
                "{action:?}"
            );
            assert_eq!(states[16].player.hits, 0, "{action:?}");
            let s = &states[17];
            assert_eq!(
                (
                    s.player.invulnerable,
                    s.player.hits,
                    s.player.last_hit.clone()
                ),
                (false, 1, hit_by(18, "s")),
                "{action:?}"
            );
        }
    }

    #[test]
    fn dodge_is_hit_later_in_the_round_once_invulnerability_is_over_and_broken_off() {
        // Active on ticks 5-8, sweeping 90° each: invulnerable on tick 5,
        // hit on tick 6, the dodge's fifth tick. The hit (poise 0, impact
        // 20) staggers: the dodge stops there.
        let props = r#"("swing_startup_ticks", 4), ("swing_active_ticks", 4),
            ("swing_recovery_ticks", 4), ("swing_arc", 360)"#;
        for (axes, action) in [(&FORWARD[..], Action::Roll), (&NONE[..], Action::Backstep)] {
            let mut run = hit_run(SHORT_DODGES, &swinger("s", (0.0, 0.0, 0.0), props));
            let states = dodge(&mut run, 8, axes);
            let p = &states[4].player;
            assert_eq!(
                (p.action, p.action_tick, p.invulnerable, p.hits),
                (action, 4, true, 0),
                "{action:?}"
            );
            let p = &states[5].player;
            assert_eq!(
                (p.action, p.action_tick, p.invulnerable, p.hits),
                (Action::Idle, 0, false, 1),
                "{action:?}"
            );
            assert_eq!(p.last_hit, hit_by(6, "s"));
            assert_eq!(p.health.state, Condition::Stagger);
            // Once a round: ticks 7 and 8 do not hit again.
            assert_eq!(states[7].player.hits, 1);
        }
    }

    /// A run whose swinger stands on the character's spot and hits it on
    /// ticks 2, 5, 8, ... with `impact`; `props` on the character.
    fn every_third_tick(props: &str, impact: u32) -> Run {
        let swing = format!(r#"{}, ("swing_impact", {impact})"#, all_round(1, 1));
        hit_run(props, &swinger("s", (0.0, 0.0, 0.0), &swing))
    }

    #[test]
    fn character_with_zero_poise_is_staggered_by_every_hit_but_not_by_zero_impact() {
        // Poise 0 and stagger_ticks 30, the defaults: hits on 2, 5, 8.
        let mut run = every_third_tick(r#"("hp", 100)"#, 1);
        for t in 1..=9_u32 {
            let h = run.tick(false, &NONE).player.health;
            match t {
                1 => assert_eq!(h.state, Condition::Ok),
                2 | 5 | 8 => assert_eq!((h.state, h.state_ticks), (Condition::Stagger, 30)),
                _ => assert_eq!(h.state, Condition::Stagger, "tick {t}"),
            }
        }
        let mut run = every_third_tick(r#"("hp", 100)"#, 0);
        for t in 1..=9_u32 {
            let p = run.tick(false, &NONE).player;
            assert_eq!(p.health.state, Condition::Ok, "tick {t}");
            assert_eq!(p.health.impact, 0.0);
        }
        assert_eq!(run.game.state().player.health.hp, 91.0);
    }

    #[test]
    fn character_impacts_up_to_its_poise_do_not_stagger_and_past_it_stagger_for_stagger_ticks() {
        // Poise 2, impact 1: 1 on tick 2, 2 on tick 5 (equal: no stagger),
        // 3 > 2 on tick 8: staggered on ticks 8..=12, the impact emptied.
        let mut run = every_third_tick(r#"("hp", 100), ("poise", 2), ("stagger_ticks", 5)"#, 1);
        for t in 1..=13_u32 {
            let h = run.tick(false, &NONE).player.health;
            match t {
                2 | 5 => assert_eq!(
                    (h.state, h.impact),
                    (Condition::Ok, (t / 3 + 1) as f32),
                    "tick {t}"
                ),
                8 => assert_eq!(
                    h,
                    HealthState {
                        hp: 91.0,
                        state: Condition::Stagger,
                        state_ticks: 5,
                        impact: 0.0,
                        impact_wait: 60
                    }
                ),
                9..=12 => assert_eq!(
                    (h.state, h.state_ticks),
                    (Condition::Stagger, 13 - t),
                    "tick {t}"
                ),
                _ => assert_eq!((h.state, h.state_ticks), (Condition::Ok, 0), "tick {t}"),
            }
        }
    }

    #[test]
    fn character_staggered_again_while_staggered_starts_the_stagger_over() {
        // Poise 1: staggered on tick 5 for 8 ticks; the hit on tick 8 adds
        // up to 1 (5 ticks left), the one on tick 11 to 2: staggered again,
        // 8 ticks from tick 11, the impact emptied (tick 14 adds 1 again).
        let mut run = every_third_tick(r#"("hp", 100), ("poise", 1), ("stagger_ticks", 8)"#, 1);
        let states: Vec<_> = (0..16).map(|_| run.tick(false, &NONE)).collect();
        let h = states[7].player.health;
        assert_eq!(
            (h.state, h.state_ticks, h.impact),
            (Condition::Stagger, 5, 1.0)
        );
        assert_eq!(states[10].player.health.impact, 0.0);
        for t in 11..=16 {
            let h = states[t as usize - 1].player.health;
            assert_eq!(
                (h.state, h.state_ticks),
                (Condition::Stagger, 19 - t),
                "tick {t}"
            );
        }
    }

    #[test]
    fn stagger_breaks_off_an_attack_in_startup_active_or_recovery() {
        // The light attack of [`attack_props`] pressed on tick 1: startup
        // 1-4, active 5-8, recovery 9-14. Hit on tick 3, 6 or 10; the third
        // time with an attack buffered on tick 9. Nothing starts after the
        // 3-tick stagger either.
        for (hit, phase, presses) in [
            (3, AttackPhase::Startup, &[1][..]),
            (6, AttackPhase::Active, &[1][..]),
            (10, AttackPhase::Recovery, &[1, 9][..]),
        ] {
            let props = format!(r#"{}, ("stagger_ticks", 3)"#, attack_props());
            let nodes = swinger("s", (0.0, 0.0, 0.0), &all_round(hit - 1, 100));
            let mut run = hit_run(&props, &nodes);
            let states = press_on(&mut run, ATTACK, presses, hit + 6, &NONE);
            let before = &states[hit as usize - 2].player;
            assert_eq!(
                (before.action, before.attack_phase),
                (Action::Attack, Some(phase))
            );
            if phase == AttackPhase::Recovery {
                assert_eq!(before.buffered, Some(Buffered::Attack));
            }
            for (i, s) in states.iter().enumerate().skip(hit as usize - 1) {
                let p = &s.player;
                assert_eq!(
                    (p.action, p.action_tick, p.attack_phase, p.combo, p.buffered),
                    (Action::Idle, 0, None, 0, None),
                    "hit on {hit}, tick {}",
                    i + 1
                );
            }
            assert_eq!(states[hit as usize - 1].player.hits, 1);
        }
    }

    #[test]
    fn after_a_stagger_the_next_light_attack_starts_a_new_combo() {
        // Light attacks of [`attack_props`] (14 ticks) pressed on 1 and
        // buffered on 10: the second, combo 2, starts on 15 and is hit on
        // 16, staggered on 16..=18. Pressed again on 19: combo 1, and the
        // one buffered during its recovery (28) is combo 2, not 3.
        let props = format!(r#"{}, ("stagger_ticks", 3)"#, attack_props());
        let nodes = swinger("s", (0.0, 0.0, 0.0), &all_round(15, 100));
        let mut run = hit_run(&props, &nodes);
        let states = press_on(&mut run, ATTACK, &[1, 10, 19, 28], 33, &NONE);
        assert_eq!(states[14].player.combo, 2);
        assert_eq!(states[15].player.action, Action::Idle);
        assert_eq!(states[17].player.health.state, Condition::Stagger);
        assert_eq!(states[18].player.health.state, Condition::Ok);
        assert_eq!(
            (states[18].player.action, states[18].player.combo),
            (Action::Attack, 1)
        );
        assert_eq!(
            (states[32].player.action, states[32].player.combo),
            (Action::Attack, 2)
        );
    }

    #[test]
    fn stagger_stops_a_sprint_until_it_is_over() {
        // Holding roll and forward from tick 1 sprints from tick 16; hit on
        // tick 20, staggered on 20..=24: no moving until the sprint goes on
        // on tick 25.
        let props = format!(r#"{PROPS}, ("stagger_ticks", 5)"#);
        let nodes = swinger("s", (0.0, 0.0, 0.0), &all_round(19, 100));
        let mut run = hit_run(&props, &nodes);
        let states: Vec<_> = (0..25).map(|_| run.tick(true, &FORWARD)).collect();
        assert_eq!(states[18].player.action, Action::Sprint);
        let hit = &states[19].player;
        assert_eq!((hit.action, hit.hits), (Action::Idle, 1));
        for s in &states[20..24] {
            assert_eq!(s.player.position, hit.position);
            assert_eq!(s.player.action, Action::Idle);
        }
        let p = &states[24].player;
        assert_eq!(p.action, Action::Sprint);
        assert_eq!(p.position, hit.position + Vec3::new(0.0, 0.0, -0.125));
    }

    #[test]
    fn staggered_character_neither_moves_nor_turns_nor_acts_but_regains_stamina() {
        // A light attack of [`attack_props`] on ticks 1-14 (85 stamina
        // left), hit on tick 15, staggered on 15..=20. On 16..=20 pushed
        // forward and turning the camera, an attack pressed on 16 and roll
        // tapped on 17-18: only the camera turns, and stamina comes back
        // 0.75 a tick (no delay here). On tick 21 it runs, and nothing else
        // starts.
        let props = format!(
            r#"{}, ("stagger_ticks", 6), ("stamina_delay_ticks", 0)"#,
            attack_props()
        );
        let nodes = swinger("s", (0.0, 0.0, 0.0), &all_round(14, 100));
        let mut run = hit_run(&props, &nodes);
        press_on(&mut run, ATTACK, &[1], 14, &NONE);
        let hit = run.hold(&[], &NONE);
        assert_eq!(hit.player.health.state, Condition::Stagger);
        assert_eq!(hit.player.stamina, 85.75);
        let turning = [(MOVE_Y, 1.0), (LOOK_X, 0.01)];
        for t in 16..=20 {
            let actions: &[&str] = match t {
                16 => &[ATTACK],
                17 => &[ROLL],
                _ => &[],
            };
            let s = run.hold(actions, &turning);
            let p = &s.player;
            assert_eq!(p.health.state, Condition::Stagger, "tick {t}");
            assert_eq!(
                (p.position, p.facing),
                (hit.player.position, hit.player.facing)
            );
            assert_eq!(
                (p.action, p.attack_phase, p.buffered),
                (Action::Idle, None, None)
            );
            assert_eq!(p.stamina, 85.75 + 0.75 * (t - 15) as f32, "tick {t}");
            assert_ne!(s.camera.yaw, hit.camera.yaw);
        }
        let s = run.hold(&[], &FORWARD);
        assert_eq!(s.player.health.state, Condition::Ok);
        assert_eq!(s.player.action, Action::Idle);
        assert_ne!(s.player.position, hit.player.position);
    }

    #[test]
    fn staggered_character_still_turns_the_camera_and_locks_on() {
        // Hit on tick 2 (staggered for 30 ticks). The camera turns right on
        // tick 3; the lock key on tick 4 locks on to the target ahead on the
        // left, and the camera starts turning to it.
        let nodes = format!(
            "{}{}",
            swinger("s", (0.0, 0.0, 0.0), &all_round(1, 100)),
            target("t", -5.0, -10.0)
        );
        let mut run = hit_run("", &nodes);
        run.tick(false, &NONE);
        run.tick(false, &NONE);
        let s = run.hold(&[], &[(LOOK_X, 0.1)]);
        assert_eq!(s.player.health.state, Condition::Stagger);
        assert_eq!(s.camera.yaw, -0.1);
        let s = run.hold(&[LOCK], &NONE);
        assert_eq!(s.player.health.state, Condition::Stagger);
        assert_eq!(lock_of(&s), Some("t"));
        assert!(s.camera.yaw > -0.1);
    }

    #[test]
    fn killing_hit_lets_go_of_the_lock_and_a_dead_character_cannot_lock_on() {
        // hp 3, one hit of 3 on tick 3 kills.
        let nodes = format!(
            "{}{}",
            swinger("s", (0.0, 0.0, 0.0), &all_round(2, 100)),
            target("t", 0.0, -10.0)
        );
        let mut run = hit_run(r#"("hp", 3)"#, &nodes);
        assert_eq!(lock_of(&run.hold(&[LOCK], &NONE)), Some("t"));
        assert_eq!(lock_of(&run.hold(&[], &NONE)), Some("t"));
        let s = run.hold(&[], &NONE);
        assert_eq!(s.player.health.state, Condition::Dead);
        assert_eq!(lock_of(&s), None);
        run.hold(&[], &NONE);
        let s = run.hold(&[LOCK], &NONE);
        assert_eq!(
            (lock_of(&s), s.player.health.state),
            (None, Condition::Dead)
        );
    }

    #[test]
    fn dead_character_is_not_hit_until_it_respawns() {
        // Killed on tick 2, dead on 2..=21: the swings of 5, 8, ..., 20 go
        // through it. Back on tick 22, hit again on 23.
        let mut run = every_third_tick(r#"("hp", 3), ("respawn_ticks", 20)"#, 1);
        for t in 1..=23 {
            let p = run.tick(false, &NONE).player;
            let (hits, state) = match t {
                1 => (0, Condition::Ok),
                2..=21 => (1, Condition::Dead),
                22 => (1, Condition::Ok),
                _ => (2, Condition::Dead),
            };
            assert_eq!((p.hits, p.health.state), (hits, state), "tick {t}");
        }
    }

    #[test]
    fn character_respawns_at_full_health_where_it_started_respawn_ticks_after_dying() {
        // An attack on tick 1 spends stamina (1 of [`health_props`], and 30
        // ticks of delay); running right and turning the camera on 4..=10;
        // killed on tick 10, dead on 10..=14. On tick 15 it is back at the
        // start at full health and stamina, the camera as at load, and runs.
        let props = format!(r#"{}, ("hp", 3), ("respawn_ticks", 5)"#, health_props());
        let nodes = swinger("s", (0.0, 0.0, 0.0), &all_round(9, 100));
        let mut run = hit_run(&props, &nodes);
        let away = [(MOVE_X, 1.0), (LOOK_X, 0.05)];
        let mut states = vec![run.hold(&[ATTACK], &NONE), run.tick(false, &NONE)];
        states.push(run.tick(false, &NONE));
        for _ in 4..=14 {
            states.push(run.tick(false, &away));
        }
        let dead = &states[9];
        assert_eq!(dead.player.health.state, Condition::Dead);
        assert!(dead.player.position.x > 0.0 && dead.player.stamina < 100.0);
        let last = &states[13];
        assert_eq!(
            (last.player.health.state, last.player.health.state_ticks),
            (Condition::Dead, 1)
        );
        assert_eq!(last.player.position, dead.player.position);
        assert_ne!(last.camera.yaw, 0.0);
        let s = run.tick(false, &FORWARD);
        let p = &s.player;
        assert_eq!(p.health, full_health(3.0));
        assert_eq!(p.position, Vec3::new(0.0, 0.0, -0.0625));
        assert_eq!(p.facing.to_bits(), 0);
        assert_eq!((p.stamina, p.stamina_wait), (100.0, 0));
        assert_eq!((s.camera.yaw, s.camera.pitch), (0.0, 0.0));
        assert_eq!(p.hits, 1);
    }

    /// A hurtbox swinger 1 m straight ahead (facing away, but sweeping all
    /// around), active on the second tick of each round, with `props`.
    fn target_swinger(props: &str, recovery: u32) -> String {
        swinger(
            "s",
            (0.0, 0.0, -1.0),
            &format!(
                r#"{}, ("hurt_radius", 0.25), ("hurt_height", 1.8), {props}"#,
                all_round(1, recovery)
            ),
        )
    }

    #[test]
    fn swinger_staggered_on_its_active_tick_does_not_swing_then_starts_a_new_round() {
        // The character's attack (pressed on 1, [`health_props`]) hits on
        // tick 2 and staggers the swinger (poise 0) for 3 ticks: on tick 2
        // it does not swing. Back on tick 5 at swing_tick 1, it hits on 6.
        let nodes = target_swinger(r#"("hp", 100), ("stagger_ticks", 3)"#, 100);
        let mut run = hit_run(&health_props(), &nodes);
        let states = press_on(&mut run, ATTACK, &[1], 6, &NONE);
        let swing = |t: usize| {
            let s = &states[t - 1].swingers[0];
            (s.swing_tick, s.phase)
        };
        assert_eq!(swing(1), (1, Some(AttackPhase::Startup)));
        for t in 2..=4 {
            assert_eq!(swing(t), (0, None), "tick {t}");
            assert_eq!(states[t - 1].player.hits, 0);
        }
        assert_eq!(
            health_of(&states[1], "s").state,
            Condition::Stagger,
            "staggered by the character"
        );
        assert_eq!(swing(5), (1, Some(AttackPhase::Startup)));
        assert_eq!(swing(6), (2, Some(AttackPhase::Active)));
        assert_eq!(states[5].player.last_hit, hit_by(6, "s"));
    }

    #[test]
    fn killed_swinger_stops_until_it_revives_then_starts_a_new_round() {
        // Killed on tick 2 (3 damage on hp 3), dead on 2..=6, back on 7,
        // swinging again: it hits on 8.
        let nodes = target_swinger(r#"("hp", 3), ("revive_ticks", 5)"#, 100);
        let mut run = hit_run(&health_props(), &nodes);
        let states = press_on(&mut run, ATTACK, &[1], 8, &NONE);
        for t in 2..=6 {
            assert_eq!(states[t - 1].swingers[0].swing_tick, 0, "tick {t}");
            assert_eq!(health_of(&states[t - 1], "s").state, Condition::Dead);
        }
        assert_eq!(states[6].swingers[0].swing_tick, 1);
        assert_eq!(states[6].player.hits, 0);
        assert_eq!(states[7].player.last_hit, hit_by(8, "s"));
    }

    #[test]
    fn swinger_without_hp_keeps_its_rhythm_when_hit() {
        // A hurtbox without hp, hit by attacks on ticks 2 and 5 (the
        // character's poise high enough not to be staggered by its swings).
        let nodes = target_swinger("", 1);
        let props = format!(r#"{}, ("poise", 1000)"#, health_props());
        let mut run = hit_run(&props, &nodes);
        let states = press_on(&mut run, ATTACK, &[1, 4], 12, &NONE);
        for (i, s) in states.iter().enumerate() {
            assert_eq!(s.swingers[0].swing_tick, i as u32 % 3 + 1, "tick {}", i + 1);
        }
        let h = hurtbox_state(&states[11], "s");
        assert_eq!((h.hits, h.health), (2, None));
        assert_eq!(states[11].player.hits, 4);
    }

    #[test]
    fn swinger_does_not_hit_hurtboxes_in_its_sweep() {
        let nodes = format!(
            "{}{}",
            swinger("s", (0.0, 0.0, 0.0), &all_round(1, 1)),
            hurtbox("h", (0.0, 0.0, -1.0), 0.25, 1.8)
        );
        let mut run = hit_run(r#"("hp", 100)"#, &nodes);
        let s = run.ticks(6, false, &NONE);
        assert_eq!(s.player.hits, 2);
        assert_eq!(hurtbox_state(&s, "h").hits, 0);
    }

    #[test]
    fn character_stands_turned_to_its_facing_and_tips_away_from_the_swinger_when_hit() {
        // The character turned 90° (to face -X), the swinger 1.5 m behind
        // it on +Z: hit on tick 3, it tips 20° towards -Z while staggered
        // (ticks 3..=5), or lies flat that way while dead (3..=7, respawning
        // on 8 with hp 3).
        let (sin, cos) = (
            libm::sinf(20.0_f32.to_radians()),
            libm::cosf(20.0_f32.to_radians()),
        );
        for (props, down, up) in [
            (
                r#"("hp", 100), ("stagger_ticks", 3)"#,
                3..=5,
                Vec3::new(0.0, cos, -sin),
            ),
            (r#"("hp", 3), ("respawn_ticks", 5)"#, 3..=7, Vec3::NEG_Z),
        ] {
            let src = scene_with(props, &swinger("s", (0.0, 0.0, 1.5), RHYTHM)).replace(
                r#"(name: "player","#,
                r#"(name: "player", rotation: (0.0, 90.0, 0.0),"#,
            );
            let mut run = Run::on(&src);
            let facing = run.game.state().player.facing;
            assert!((facing - FRAC_PI_2).abs() < 1e-6);
            for t in 1..=8_u32 {
                run.tick(false, &NONE);
                if down.contains(&t) {
                    assert_near(up_of(&run, "player"), up, &format!("tick {t}"));
                    let tilt = if up == Vec3::NEG_Z {
                        FRAC_PI_2
                    } else {
                        20.0_f32.to_radians()
                    };
                    assert_same_rotation(
                        node_rotation(&run, "player"),
                        Quat::from_rotation_x(-tilt) * Quat::from_rotation_y(facing),
                    );
                } else {
                    // Standing: exactly as before G3.4.
                    assert_eq!(
                        quat_bits(node_rotation(&run, "player")),
                        quat_bits(Quat::from_rotation_y(facing)),
                        "tick {t}"
                    );
                }
            }
        }
    }

    #[test]
    fn hit_character_flashes_for_eight_ticks_and_darkens_while_dead_until_it_respawns() {
        // hp 6: hit on tick 3 (white on 3..=8), killed on tick 9 (darkened on
        // 9..=18), back on 19 in its colors: 10 ticks after the hit, past
        // the flash.
        let src = scene_with(
            r#"("hp", 6), ("respawn_ticks", 10)"#,
            &swinger("s", (0.0, 0.0, 0.0), RHYTHM),
        )
        .replace(
            "character: (run_speed: 3.75, turn_speed: 720.0),",
            r#"character: (run_speed: 3.75, turn_speed: 720.0), color: (0.8, 0.25, 0.2),
            children: [(name: "visor", mesh: Cube, color: (0.9, 0.85, 0.3))],"#,
        );
        let mut run = Run::on(&src);
        let meshes = [
            ("player", Vec3::new(0.8, 0.25, 0.2)),
            ("player/visor", Vec3::new(0.9, 0.85, 0.3)),
        ];
        for t in 1..=20_u32 {
            let s = run.tick(false, &NONE);
            for (path, color) in meshes {
                let expected = match t {
                    3..=8 => Vec3::ONE,
                    9..=18 => color * 0.4,
                    _ => color,
                };
                assert_eq!(color_of(&run, path), expected, "{path} on tick {t}");
            }
            if t == 9 {
                assert_eq!(s.player.health.state, Condition::Dead);
            }
        }
    }

    /// The arm of swinger `s`: its local transform and its color.
    fn arm_of(run: &Run) -> (loom_scene::Transform, Vec3) {
        let scene = run.game.scene();
        let id = scene.find("s/arm").unwrap();
        (*scene.transform(id), color_of(run, "s/arm"))
    }

    #[test]
    fn arm_waits_at_the_right_end_in_warning_color_sweeps_each_slice_then_returns() {
        // Far from the character. Startup (ticks 1-2): turned to the right
        // end, -90°; active (3-4): at the end of the slice swept, 0° then
        // 90°; recovery (5-6): as in the scene file; tick 7 starts over.
        let mut run = hit_run("", &swinger("s", (0.0, 0.0, -20.0), RHYTHM));
        let (scene_arm, _) = arm_of(&run);
        let (p0, r0) = (scene_arm.position, scene_arm.rotation);
        let arc = 180.0_f32.to_radians();
        let (half, slice) = (arc * 0.5, arc / 2.0);
        for t in 1..=7_u32 {
            run.tick(false, &NONE);
            let (arm, color) = arm_of(&run);
            let angle = match t {
                1 | 2 | 7 => Some(-half),
                3 | 4 => Some(-half + (t - 2) as f32 * slice),
                _ => None,
            };
            let (position, rotation) = match angle {
                Some(a) => (Quat::from_rotation_y(a) * p0, Quat::from_rotation_y(a) * r0),
                None => (p0, r0),
            };
            assert_eq!(bits3(arm.position), bits3(position), "tick {t}");
            assert_eq!(quat_bits(arm.rotation), quat_bits(rotation), "tick {t}");
            assert_eq!(arm.scale, scene_arm.scale);
            let warning = matches!(t, 1 | 2 | 7);
            assert_eq!(color, if warning { WARN } else { ARM }, "tick {t}");
        }
        // The right end is +X: to the right of a swinger facing -Z.
        run.tick(false, &NONE);
        let (arm, _) = arm_of(&run);
        assert_near(arm.position, Vec3::new(1.0, 1.0, 0.0), "right end");
    }

    fn bits3(v: Vec3) -> [u32; 3] {
        v.to_array().map(f32::to_bits)
    }

    #[test]
    fn arm_flashes_with_its_swinger_instead_of_warning() {
        // A hurtbox swinger without hp, in startup on ticks 1-10, hit by the
        // character's attack on tick 2: white on 2..=9, warning on 1 and 10.
        let nodes = swinger(
            "s",
            (0.0, 0.0, -1.0),
            r#"("hurt_radius", 0.25), ("hurt_height", 1.8), ("swing_startup_ticks", 10)"#,
        );
        let mut run = hit_run(&health_props(), &nodes);
        for t in 1..=10_u32 {
            let actions: &[&str] = if t == 1 { &[ATTACK] } else { &[] };
            run.hold(actions, &NONE);
            let (_, color) = arm_of(&run);
            let expected = if (2..=9).contains(&t) {
                Vec3::ONE
            } else {
                WARN
            };
            assert_eq!(color, expected, "tick {t}");
        }
    }

    #[test]
    fn dead_swinger_arm_darkens_on_the_center_line_until_it_revives() {
        // Killed on tick 2 (dead on 2..=11): the arm darkened and as in the
        // scene file; back on tick 12 (past the flash) in startup: the right
        // end, warning.
        let nodes = target_swinger(r#"("hp", 3), ("revive_ticks", 10)"#, 100);
        let mut run = hit_run(&health_props(), &nodes);
        let (scene_arm, _) = arm_of(&run);
        for t in 1..=12_u32 {
            let actions: &[&str] = if t == 1 { &[ATTACK] } else { &[] };
            run.hold(actions, &NONE);
            let (arm, color) = arm_of(&run);
            if (2..=11).contains(&t) {
                assert_eq!(color, ARM * 0.4, "tick {t}");
                assert_eq!(bits3(arm.position), bits3(scene_arm.position), "tick {t}");
                assert_eq!(quat_bits(arm.rotation), quat_bits(scene_arm.rotation));
            } else {
                assert_eq!(color, WARN, "tick {t}");
                assert_ne!(arm.position, scene_arm.position, "tick {t}");
            }
        }
    }

    #[test]
    fn character_hp_poise_and_respawn_props_are_checked_with_exact_messages() {
        let whole = "must be a whole number of at least 1";
        for (props, problem) in [
            (
                r#"("hp", 0)"#,
                "prop \"hp\" must be greater than 0, got 0".to_owned(),
            ),
            (
                r#"("poise", -0.5)"#,
                "prop \"poise\" must be at least 0, got -0.5".to_owned(),
            ),
            (
                r#"("stagger_ticks", 0)"#,
                format!("prop \"stagger_ticks\" {whole}, got 0"),
            ),
            (
                r#"("poise_reset_ticks", 1.5)"#,
                format!("prop \"poise_reset_ticks\" {whole}, got 1.5"),
            ),
            (
                r#"("respawn_ticks", 0.5)"#,
                format!("prop \"respawn_ticks\" {whole}, got 0.5"),
            ),
        ] {
            assert_eq!(
                error_of(props),
                format!("the character \"player\": {problem}")
            );
        }
        let accepted = r#"("hp", 0.5), ("poise", 0), ("stagger_ticks", 1),
            ("poise_reset_ticks", 1), ("respawn_ticks", 1)"#;
        assert!(Game::from_source(&scene(accepted)).is_ok());
        // Node props are another table: revive_ticks is not the character's.
        assert_eq!(
            error_of(r#"("revive_ticks", 5)"#),
            "the character \"player\": unknown prop \"revive_ticks\""
        );
    }

    #[test]
    fn character_hp_poise_and_respawn_props_are_checked_after_the_damage_props() {
        assert!(error_of(r#"("hp", 0), ("heavy_impact", -1)"#).contains("heavy_impact"));
        assert!(error_of(r#"("respawn_ticks", 0), ("hp", 0)"#).contains("\"hp\""));
        assert!(error_of(r#"("respawn_ticks", 0), ("poise", -1)"#).contains("\"poise\""));
        // Values before the windows, the ranges and the band.
        assert!(error_of(r#"("roll_invuln_to", 40), ("hp", 0)"#).contains("\"hp\""));
    }

    /// The error for a root node "t" with `props`.
    fn node_error(props: &str) -> String {
        error_with("", &format!(r#"(name: "t", props: [{props}]),"#))
    }

    #[test]
    fn swing_props_are_checked_with_exact_messages() {
        let d = r#"("swing_damage", 3)"#;
        let whole = "must be a whole number of at least 1";
        for (props, problem) in [
            (
                r#"("swing_damage", 0)"#.to_owned(),
                "prop \"swing_damage\" must be greater than 0, got 0".to_owned(),
            ),
            (
                format!(r#"{d}, ("swing_impact", -1)"#),
                "prop \"swing_impact\" must be at least 0, got -1".to_owned(),
            ),
            (
                format!(r#"{d}, ("swing_startup_ticks", 0)"#),
                format!("prop \"swing_startup_ticks\" {whole}, got 0"),
            ),
            (
                format!(r#"{d}, ("swing_active_ticks", 1.5)"#),
                format!("prop \"swing_active_ticks\" {whole}, got 1.5"),
            ),
            (
                format!(r#"{d}, ("swing_recovery_ticks", 0.5)"#),
                format!("prop \"swing_recovery_ticks\" {whole}, got 0.5"),
            ),
            (
                format!(r#"{d}, ("swing_reach", 0)"#),
                "prop \"swing_reach\" must be greater than 0, got 0".to_owned(),
            ),
            (
                format!(r#"{d}, ("swing_arc", 0)"#),
                "prop \"swing_arc\" must be greater than 0 and at most 360, got 0".to_owned(),
            ),
            (
                format!(r#"{d}, ("swing_arc", 360.5)"#),
                "prop \"swing_arc\" must be greater than 0 and at most 360, got 360.5".to_owned(),
            ),
            (
                format!(r#"{d}, ("swing_low", -0.1)"#),
                "prop \"swing_low\" must be at least 0, got -0.1".to_owned(),
            ),
            (
                format!(r#"{d}, ("swing_high", 0)"#),
                "prop \"swing_high\" must be greater than 0, got 0".to_owned(),
            ),
        ] {
            assert_eq!(node_error(&props), format!("the node \"t\": {problem}"));
        }
        let accepted = r#"("swing_damage", 0.5), ("swing_impact", 0),
            ("swing_startup_ticks", 1), ("swing_active_ticks", 1), ("swing_recovery_ticks", 1),
            ("swing_reach", 0.1), ("swing_arc", 360), ("swing_low", 0), ("swing_high", 0.1)"#;
        let nodes = format!(r#"(name: "t", props: [{accepted}]),"#);
        assert!(Game::from_source(&scene_with("", &nodes)).is_ok());
    }

    #[test]
    fn swinger_nodes_are_checked_with_exact_messages() {
        let d = r#"("swing_damage", 3)"#;
        for (props, message) in [
            (
                r#"("swing_reach", 2)"#.to_owned(),
                "the node \"t\": prop \"swing_reach\" needs swing_damage",
            ),
            (
                // The first of the eight in list order, not in the node's.
                r#"("swing_high", 2), ("swing_impact", 5)"#.to_owned(),
                "the node \"t\": prop \"swing_impact\" needs swing_damage",
            ),
            (
                // The other end by its default: 1.6, 0.3.
                format!(r#"{d}, ("swing_low", 1.6)"#),
                "the node \"t\": needs swing_low < swing_high",
            ),
            (
                format!(r#"{d}, ("swing_high", 0.3)"#),
                "the node \"t\": needs swing_low < swing_high",
            ),
            (
                format!(r#"{d}, ("swing_dammage", 3)"#),
                "the node \"t\": unknown prop \"swing_dammage\" (known: lock_height, \
                 hurt_radius, hurt_height, hp, poise, stagger_ticks, poise_reset_ticks, \
                 revive_ticks, swing_damage, swing_impact, swing_startup_ticks, \
                 swing_active_ticks, swing_recovery_ticks, swing_reach, swing_arc, swing_low, \
                 swing_high)",
            ),
        ] {
            assert_eq!(node_error(&props), message);
        }
        assert!(
            Game::from_source(&scene_with(
                "",
                &format!(r#"(name: "t", props: [{d}, ("swing_low", 1.5)]),"#)
            ))
            .is_ok()
        );
        assert_eq!(
            error_with(
                "",
                &format!(r#"(name: "g", children: [(name: "t", props: [{d}])]),"#)
            ),
            "the swinger \"g/t\" must be a root node, and not the camera"
        );
        let src = scene("").replace(
            r#"(name: "cam","#,
            &format!(r#"(name: "cam", props: [{d}],"#),
        );
        match Game::from_source(&src) {
            Err(e) => assert_eq!(
                e.to_string(),
                "the swinger \"cam\" must be a root node, and not the camera"
            ),
            Ok(_) => panic!("the camera cannot be a swinger"),
        }
        assert_eq!(
            error_with(
                "",
                &format!(r#"(name: "t", props: [{d}], children: [(name: "arm")]),"#)
            ),
            "the node \"t\": its arm needs a mesh"
        );
        // "arm" means nothing on a node that is no swinger.
        let nodes = format!(
            r#"(name: "t", props: [{HURT}], children: [(name: "arm")]),
            (name: "u", children: [(name: "arm")]),"#
        );
        assert!(Game::from_source(&scene_with("", &nodes)).is_ok());
    }

    #[test]
    fn swinger_checks_run_in_the_documented_order() {
        let d = r#"("swing_damage", 3)"#;
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
        assert!(node_error(r#"("swing_arc", 0), ("swing_x", 1)"#).contains("unknown prop"));
        assert!(
            node_error(r#"("swing_arc", 0), ("swing_damage", 0)"#).contains("\"swing_damage\"")
        );
        assert!(node_error(r#"("swing_high", 0), ("revive_ticks", 0)"#).contains("revive_ticks"));
        // Values, then the hurt pair, then hp, then the props needing hp.
        assert!(node_error(r#"("hurt_radius", 0.4), ("swing_arc", 0)"#).contains("swing_arc"));
        assert!(node_error(r#"("hurt_radius", 0.4), ("swing_impact", 1)"#).contains("needs both"));
        assert!(node_error(r#"("hp", 5), ("swing_impact", 1)"#).contains("hp needs hurt"));
        assert!(node_error(r#"("poise", 5), ("swing_impact", 1)"#).contains("needs hp"));
        // Needing swing_damage, then the band, then the root rule.
        assert!(node_error(r#"("swing_low", 2)"#).contains("needs swing_damage"));
        assert!(child(&format!(r#"{d}, ("swing_low", 2)"#), "").contains("swing_low <"));
        // The root rule: a lock target's message, then a hurtbox's, then a
        // swinger's.
        assert!(
            child(&format!(r#"{d}, ("lock_height", 1), {HURT}"#), "")
                .starts_with("the lock target \"g/t\"")
        );
        assert!(
            child(&format!(r#"{d}, ("lock_height", 1)"#), "")
                .starts_with("the lock target \"g/t\"")
        );
        assert!(child(&format!("{d}, {HURT}"), "").starts_with("the hurtbox \"g/t\""));
        // The root rule, then the hp bar, then the arm.
        assert!(
            child(&format!(r#"{d}, {HURT}, ("hp", 5)"#), r#"(name: "arm")"#)
                .starts_with("the hurtbox \"g/t\"")
        );
        assert_eq!(
            error_with(
                "",
                &format!(
                    r#"(name: "t", props: [{d}, {HURT}, ("hp", 5)],
                        children: [(name: "arm"), (name: "hp_bar")]),"#
                )
            ),
            "the node \"t\": its hp_bar needs a mesh"
        );
    }

    // ------------------------------------------------------------------ events

    mod events {
        use super::*;
        use crate::{DropReason, EndReason, Event, HitAttack, Input, LockReason, Outcome};

        type Held = (Vec<&'static str>, Vec<(&'static str, f32)>);

        /// Runs `ticks` more ticks, holding what `input(t)` gives on the
        /// run's tick `t`; returns the ticks that had events, with them.
        fn events_of(
            run: &mut Run,
            ticks: u32,
            input: impl Fn(u64) -> Held,
        ) -> Vec<(u64, Vec<Event>)> {
            let mut out = Vec::new();
            for _ in 0..ticks {
                let t = run.game.clock().tick() + 1;
                let (actions, axes) = input(t);
                run.hold(&actions, &axes);
                let events = run.game.events();
                if !events.is_empty() {
                    out.push((t, events.to_vec()));
                }
            }
            out
        }

        /// `action` pressed on the ticks `on`, nothing else.
        fn press(action: &'static str, on: &'static [u64]) -> impl Fn(u64) -> Held {
            move |t| {
                (
                    if on.contains(&t) {
                        vec![action]
                    } else {
                        vec![]
                    },
                    vec![],
                )
            }
        }

        fn start(action: Action, combo: u32, buffered: bool) -> Event {
            Event::ActionStart {
                action,
                combo,
                buffered,
            }
        }

        fn end(action: Action) -> Event {
            Event::ActionEnd {
                action,
                reason: EndReason::Finished,
            }
        }

        fn dropped(input: Input, reason: DropReason) -> Event {
            Event::InputDropped {
                input,
                reason,
                buffered: false,
            }
        }

        fn lock(target: Option<&str>, reason: LockReason) -> Event {
            Event::Lock {
                target: target.map(str::to_owned),
                reason,
            }
        }

        /// A hit by the character's `attack` on `target`, which has health.
        fn struck(
            target: &str,
            attack: HitAttack,
            combo: u32,
            damage: f32,
            hp: f32,
            result: Outcome,
        ) -> Event {
            Event::Hit {
                by: "player".to_owned(),
                target: target.to_owned(),
                attack,
                combo,
                damage: Some(damage),
                hp: Some(hp),
                result,
            }
        }

        /// A hit of 3 damage by the swinger "s" on the character.
        fn swung(hp: f32, result: Outcome) -> Event {
            Event::Hit {
                by: "s".to_owned(),
                target: "player".to_owned(),
                attack: HitAttack::Swing,
                combo: 0,
                damage: Some(3.0),
                hp: Some(hp),
                result,
            }
        }

        #[test]
        fn character_hits_report_hit_then_stagger_then_death_with_the_hp_left() {
            // hp 10, poise 1. Light (3 damage, impact 1) on 2: impact 1, not
            // past the poise; on 5: impact 2, staggered. Heavy (5) on 8: dead.
            let mut run = dummy_run(r#"("hp", 10), ("poise", 1)"#);
            let events = events_of(&mut run, 9, |t| match t {
                1 | 4 => (vec![ATTACK], vec![]),
                7 => (vec![HEAVY], vec![]),
                _ => (vec![], vec![]),
            });
            assert_eq!(
                events,
                [
                    (1, vec![start(Action::Attack, 1, false)]),
                    (
                        2,
                        vec![struck("d", HitAttack::Attack, 1, 3.0, 7.0, Outcome::Hit)]
                    ),
                    (3, vec![end(Action::Attack)]),
                    (4, vec![start(Action::Attack, 1, false)]),
                    (
                        5,
                        vec![struck(
                            "d",
                            HitAttack::Attack,
                            1,
                            3.0,
                            4.0,
                            Outcome::Stagger
                        )]
                    ),
                    (6, vec![end(Action::Attack)]),
                    (7, vec![start(Action::Heavy, 0, false)]),
                    (
                        8,
                        vec![struck("d", HitAttack::Heavy, 0, 5.0, 0.0, Outcome::Death)]
                    ),
                    (9, vec![end(Action::Heavy)]),
                ]
            );
        }

        #[test]
        fn hit_on_a_hurtbox_without_hp_has_null_damage_and_hp() {
            let nodes = hurtbox("h", (0.0, 0.0, -1.5), 0.25, 1.8);
            let mut run = Run::on(&scene_with(&health_props(), &nodes));
            let events = events_of(&mut run, 2, press(ATTACK, &[1]));
            let hit = Event::Hit {
                by: "player".to_owned(),
                target: "h".to_owned(),
                attack: HitAttack::Attack,
                combo: 1,
                damage: None,
                hp: None,
                result: Outcome::Hit,
            };
            assert_eq!(events[1], (2, vec![hit]));
        }

        #[test]
        fn one_slice_hitting_two_hurtboxes_reports_both_in_pre_order() {
            // "a" 20° to the left comes first in the file, though the sweep
            // runs right to left; one active tick sweeps the whole 120°.
            let nodes = format!(
                "{}{}",
                dummy("a", (-0.5, -1.5), r#"("hp", 10)"#),
                dummy("b", (0.5, -1.5), r#"("hp", 10)"#)
            );
            let mut run = Run::on(&scene_with(&health_props(), &nodes));
            let events = events_of(&mut run, 2, press(ATTACK, &[1]));
            assert_eq!(
                events[1],
                (
                    2,
                    vec![
                        struck("a", HitAttack::Attack, 1, 3.0, 7.0, Outcome::Stagger),
                        struck("b", HitAttack::Attack, 1, 3.0, 7.0, Outcome::Stagger),
                    ]
                )
            );
        }

        #[test]
        fn swinger_hit_on_the_character_reports_a_swing() {
            // Impact 20 on poise 0: staggered.
            let mut run = hit_run(r#"("hp", 100)"#, &swinger("s", (0.0, 0.0, 0.0), RHYTHM));
            let events = events_of(&mut run, 6, press(ATTACK, &[]));
            assert_eq!(events, [(3, vec![swung(97.0, Outcome::Stagger)])]);
        }

        #[test]
        fn killing_the_locked_dummy_reports_the_hit_then_letting_go() {
            let mut run = dummy_run(r#"("hp", 3)"#);
            let events = events_of(&mut run, 4, |t| match t {
                1 => (vec![LOCK], vec![]),
                2 => (vec![ATTACK], vec![]),
                _ => (vec![], vec![]),
            });
            assert_eq!(
                events,
                [
                    (1, vec![lock(Some("d"), LockReason::Pressed)]),
                    (2, vec![start(Action::Attack, 1, false)]),
                    (
                        3,
                        vec![
                            struck("d", HitAttack::Attack, 1, 3.0, 0.0, Outcome::Death),
                            lock(None, LockReason::TargetDied),
                        ]
                    ),
                    (4, vec![end(Action::Attack)]),
                ]
            );
        }

        /// The roll key tapped on ticks 1-2, `axes` on tick 2, as [`dodge`].
        fn tap_with(axes: &[(&'static str, f32)]) -> impl Fn(u64) -> Held {
            let axes = axes.to_vec();
            move |t| match t {
                1 => (vec![ROLL], vec![]),
                2 => (vec![], axes.clone()),
                _ => (vec![], vec![]),
            }
        }

        #[test]
        fn sweep_over_an_invulnerable_dodge_is_dodged_and_a_later_round_hits() {
            // As `dodge_invulnerable_on_the_only_active_tick_is_not_hit_that_round`:
            // active on tick 5, the dodge's tick 4, and on tick 18.
            for (axes, action) in [(&FORWARD[..], Action::Roll), (&NONE[..], Action::Backstep)] {
                let nodes = swinger("s", (0.0, 0.0, 0.0), &all_round(4, 8));
                let mut run = hit_run(SHORT_DODGES, &nodes);
                let events = events_of(&mut run, 18, tap_with(axes));
                let dodged = Event::Dodged {
                    by: "s".to_owned(),
                    action,
                    action_tick: 4,
                };
                assert_eq!(
                    events,
                    [
                        (2, vec![start(action, 0, false)]),
                        (5, vec![dodged]),
                        (9, vec![end(action)]),
                        (18, vec![swung(97.0, Outcome::Stagger)]),
                    ],
                    "{action:?}"
                );
            }
        }

        #[test]
        fn same_sweep_on_a_character_standing_still_is_a_hit() {
            let nodes = swinger("s", (0.0, 0.0, 0.0), &all_round(4, 8));
            let mut run = hit_run(SHORT_DODGES, &nodes);
            let events = events_of(&mut run, 9, press(ATTACK, &[]));
            assert_eq!(events, [(5, vec![swung(97.0, Outcome::Stagger)])]);
        }

        #[test]
        fn rolling_outside_the_sweep_reports_only_the_roll() {
            // 20 m away, reach 10.
            let nodes = swinger("s", (0.0, 0.0, -20.0), &all_round(4, 8));
            let mut run = hit_run(SHORT_DODGES, &nodes);
            let events = events_of(&mut run, 18, tap_with(&FORWARD));
            assert_eq!(
                events,
                [
                    (2, vec![start(Action::Roll, 0, false)]),
                    (9, vec![end(Action::Roll)]),
                ]
            );
        }

        #[test]
        fn dodge_hit_after_its_invulnerability_is_interrupted() {
            // As `dodge_is_hit_later_in_the_round_once_invulnerability_is_over_and_broken_off`.
            let props = r#"("swing_startup_ticks", 4), ("swing_active_ticks", 4),
                ("swing_recovery_ticks", 4), ("swing_arc", 360)"#;
            for (axes, action) in [(&FORWARD[..], Action::Roll), (&NONE[..], Action::Backstep)] {
                let mut run = hit_run(SHORT_DODGES, &swinger("s", (0.0, 0.0, 0.0), props));
                let events = events_of(&mut run, 8, tap_with(axes));
                let dodged = Event::Dodged {
                    by: "s".to_owned(),
                    action,
                    action_tick: 4,
                };
                let interrupted = Event::ActionEnd {
                    action,
                    reason: EndReason::Interrupted,
                };
                assert_eq!(
                    events,
                    [
                        (2, vec![start(action, 0, false)]),
                        (5, vec![dodged]),
                        (6, vec![swung(97.0, Outcome::Stagger), interrupted]),
                    ],
                    "{action:?}"
                );
            }
        }

        #[test]
        fn presses_while_staggered_are_dropped_as_staggered() {
            // Hit on tick 2 (impact 1, poise 0): staggered for 30 ticks.
            let mut run = every_third_tick(r#"("hp", 100)"#, 1);
            let events = events_of(&mut run, 4, |t| match t {
                3 => (vec![HEAVY, ATTACK, ROLL], vec![]),
                _ => (vec![], vec![]),
            });
            assert_eq!(
                events,
                [
                    (2, vec![swung(97.0, Outcome::Stagger)]),
                    (
                        3,
                        vec![
                            dropped(Input::Heavy, DropReason::Staggered),
                            dropped(Input::Attack, DropReason::Staggered),
                        ]
                    ),
                    // The roll key's tap is its release.
                    (4, vec![dropped(Input::Roll, DropReason::Staggered)]),
                ]
            );
        }

        #[test]
        fn presses_while_dead_are_dropped_as_dead_until_the_respawn() {
            // Killed on tick 2, dead on 2..=6, back at the spawn point on 7.
            let mut run = every_third_tick(r#"("hp", 3), ("respawn_ticks", 5)"#, 1);
            let events = events_of(&mut run, 7, |t| match t {
                3 => (vec![ATTACK, LOCK], vec![]),
                _ => (vec![], vec![]),
            });
            assert_eq!(
                events,
                [
                    (2, vec![swung(0.0, Outcome::Death)]),
                    (
                        3,
                        vec![
                            dropped(Input::Lock, DropReason::Dead),
                            dropped(Input::Attack, DropReason::Dead),
                        ]
                    ),
                    (
                        7,
                        vec![Event::Respawn {
                            position: Vec3::ZERO
                        }]
                    ),
                ]
            );
        }

        #[test]
        fn attack_pressed_during_a_roll_is_dropped_as_rolling() {
            let mut run = Run::new(&attack_props());
            let events = events_of(&mut run, 5, |t| match t {
                1 => (vec![ROLL], vec![]),
                2 => (vec![], FORWARD.to_vec()),
                4 => (vec![ATTACK], vec![]),
                _ => (vec![], vec![]),
            });
            assert_eq!(
                events,
                [
                    (2, vec![start(Action::Roll, 0, false)]),
                    (4, vec![dropped(Input::Attack, DropReason::Rolling)]),
                ]
            );
        }

        #[test]
        fn light_and_heavy_pressed_together_in_startup_are_both_dropped_as_startup() {
            let mut run = Run::new(&attack_props());
            let events = events_of(&mut run, 5, |t| match t {
                1 => (vec![ATTACK], vec![]),
                2 => (vec![HEAVY], vec![]),
                4 => (vec![ATTACK, HEAVY], vec![]),
                _ => (vec![], vec![]),
            });
            assert_eq!(
                events,
                [
                    (1, vec![start(Action::Attack, 1, false)]),
                    (2, vec![dropped(Input::Heavy, DropReason::Startup)]),
                    (
                        4,
                        vec![
                            dropped(Input::Heavy, DropReason::Startup),
                            dropped(Input::Attack, DropReason::Startup),
                        ]
                    ),
                ]
            );
        }

        #[test]
        fn light_press_starting_with_a_heavy_one_is_outranked() {
            let mut run = Run::new(&attack_props());
            let events = events_of(&mut run, 1, |_| (vec![ATTACK, HEAVY], vec![]));
            assert_eq!(
                events,
                [(
                    1,
                    vec![
                        start(Action::Heavy, 0, false),
                        dropped(Input::Attack, DropReason::Outranked),
                    ]
                )]
            );
        }

        #[test]
        fn light_press_buffered_with_a_heavy_one_is_outranked() {
            // Active on ticks 5-8: the heavy press is buffered, the first
            // buffer of this attack, so nothing is replaced.
            let mut run = Run::new(&attack_props());
            let events = events_of(&mut run, 6, |t| match t {
                1 => (vec![ATTACK], vec![]),
                6 => (vec![ATTACK, HEAVY], vec![]),
                _ => (vec![], vec![]),
            });
            assert_eq!(
                events,
                [
                    (1, vec![start(Action::Attack, 1, false)]),
                    (6, vec![dropped(Input::Attack, DropReason::Outranked)]),
                ]
            );
            assert_eq!(run.game.state().player.buffered, Some(Buffered::Heavy));
        }

        #[test]
        fn press_on_the_tick_a_buffered_attack_starts_is_outranked_by_it() {
            // The light press on 3 (recovery) is buffered and starts on 4,
            // where a heavy press comes too late to count.
            let mut run = Run::new(&health_props());
            let events = events_of(&mut run, 4, |t| match t {
                1 | 3 => (vec![ATTACK], vec![]),
                4 => (vec![HEAVY], vec![]),
                _ => (vec![], vec![]),
            });
            assert_eq!(
                events[2],
                (
                    4,
                    vec![
                        start(Action::Attack, 2, true),
                        dropped(Input::Heavy, DropReason::Outranked),
                    ]
                )
            );
        }

        #[test]
        fn with_no_stamina_the_buffered_action_then_new_presses_are_dropped() {
            // As `with_no_stamina_new_and_buffered_actions_are_dropped`: the
            // attack on tick 1 uses all 10 stamina; the press on 6 is
            // buffered and dropped on 15, the tick after the attack.
            let mut run = Run::new(&format!(r#"{}, ("stamina_max", 10)"#, attack_props()));
            let events = events_of(&mut run, 18, |t| match t {
                1 | 6 | 16 => (vec![ATTACK], vec![]),
                18 => (vec![HEAVY, ATTACK], vec![]),
                _ => (vec![], vec![]),
            });
            let no_stamina = |input, buffered| Event::InputDropped {
                input,
                reason: DropReason::NoStamina,
                buffered,
            };
            assert_eq!(
                events,
                [
                    (1, vec![start(Action::Attack, 1, false)]),
                    (14, vec![end(Action::Attack)]),
                    (15, vec![no_stamina(Input::Attack, true)]),
                    (16, vec![no_stamina(Input::Attack, false)]),
                    (
                        18,
                        vec![
                            no_stamina(Input::Heavy, false),
                            dropped(Input::Attack, DropReason::Outranked),
                        ]
                    ),
                ]
            );
        }

        #[test]
        fn lock_key_with_no_target_in_range_is_dropped_as_no_target() {
            let mut run = Run::new(PROPS);
            let events = events_of(&mut run, 1, press(LOCK, &[1]));
            assert_eq!(
                events,
                [(1, vec![dropped(Input::Lock, DropReason::NoTarget)])]
            );
        }

        #[test]
        fn later_press_replacing_the_buffered_one_is_reported_once() {
            // The light press on 6 is the first buffer: no event.
            let mut run = Run::new(&attack_props());
            let events = events_of(&mut run, 7, |t| match t {
                1 | 6 => (vec![ATTACK], vec![]),
                7 => (vec![HEAVY], vec![]),
                _ => (vec![], vec![]),
            });
            let replaced = Event::BufferReplaced {
                old: Buffered::Attack,
                new: Buffered::Heavy,
            };
            assert_eq!(
                events,
                [
                    (1, vec![start(Action::Attack, 1, false)]),
                    (7, vec![replaced])
                ]
            );
        }

        #[test]
        fn roll_backstep_and_sprint_report_their_start_and_end() {
            // A roll of 24 ticks and a backstep of 16, from tick 2.
            for (axes, action, last) in [
                (&FORWARD[..], Action::Roll, 25),
                (&NONE[..], Action::Backstep, 17),
            ] {
                let mut run = Run::new(PROPS);
                let events = events_of(&mut run, 30, tap_with(axes));
                assert_eq!(
                    events,
                    [
                        (2, vec![start(action, 0, false)]),
                        (last, vec![end(action)])
                    ],
                    "{action:?}"
                );
            }
            // Sprinting from tick 16 of holding the key; released on 21.
            let mut run = Run::new(PROPS);
            let events = events_of(&mut run, 22, |t| {
                (if t <= 20 { vec![ROLL] } else { vec![] }, FORWARD.to_vec())
            });
            assert_eq!(
                events,
                [
                    (16, vec![start(Action::Sprint, 0, false)]),
                    (21, vec![end(Action::Sprint)]),
                ]
            );
        }

        #[test]
        fn buffered_second_light_attack_starts_as_combo_two_from_the_buffer() {
            // Ticks 1-3, the press on 3 (recovery) buffered; then 4-6.
            let mut run = Run::new(&health_props());
            let events = events_of(&mut run, 7, press(ATTACK, &[1, 3]));
            assert_eq!(
                events,
                [
                    (1, vec![start(Action::Attack, 1, false)]),
                    (3, vec![end(Action::Attack)]),
                    (4, vec![start(Action::Attack, 2, true)]),
                    (6, vec![end(Action::Attack)]),
                ]
            );
        }

        #[test]
        fn attacking_out_of_a_sprint_ends_the_sprint_before_the_attack_starts() {
            let mut run = Run::new(&attack_props());
            let events = events_of(&mut run, 21, |t| {
                let actions = if t <= 20 {
                    vec![ROLL]
                } else {
                    vec![ROLL, ATTACK]
                };
                (actions, FORWARD.to_vec())
            });
            assert_eq!(
                events,
                [
                    (16, vec![start(Action::Sprint, 0, false)]),
                    (
                        21,
                        vec![end(Action::Sprint), start(Action::Attack, 1, false)]
                    ),
                ]
            );
        }

        #[test]
        fn roll_of_one_tick_starts_and_ends_on_that_tick() {
            let props = r#"("roll_ticks", 1), ("roll_distance", 0.125), ("roll_invuln_to", 1)"#;
            let mut run = Run::new(props);
            let events = events_of(&mut run, 3, tap_with(&FORWARD));
            assert_eq!(
                events,
                [(2, vec![start(Action::Roll, 0, false), end(Action::Roll)])]
            );
        }

        #[test]
        fn lock_key_locks_on_and_lets_go_as_pressed() {
            let mut run = Run::on(&scene_with("", &three_targets()));
            let events = events_of(&mut run, 3, press(LOCK, &[1, 3]));
            assert_eq!(
                events,
                [
                    (1, vec![lock(Some("mid"), LockReason::Pressed)]),
                    (3, vec![lock(None, LockReason::Pressed)]),
                ]
            );
        }

        #[test]
        fn flick_to_the_right_reports_the_switched_target() {
            let mut run = Run::on(&scene_with("", &three_targets()));
            let events = events_of(&mut run, 2, |t| match t {
                1 => (vec![LOCK], vec![]),
                _ => (vec![], vec![(LOOK_X, 0.12)]),
            });
            assert_eq!(
                events[1],
                (2, vec![lock(Some("right"), LockReason::Switched)])
            );
        }

        #[test]
        fn target_past_lock_break_range_is_let_go_as_out_of_range() {
            // As `target_past_lock_break_range_is_let_go_on_the_tick_it_is`.
            let props = format!(r#"{PROPS}, ("lock_range", 5), ("lock_break_range", 6)"#);
            let mut run = Run::on(&scene_with(&props, &target("ahead", 0.0, -5.0)));
            let events = events_of(&mut run, 20, |t| match t {
                1 => (vec![LOCK], vec![]),
                _ => (vec![], vec![(MOVE_Y, -1.0)]),
            });
            assert_eq!(
                events,
                [
                    (1, vec![lock(Some("ahead"), LockReason::Pressed)]),
                    (19, vec![lock(None, LockReason::OutOfRange)]),
                ]
            );
        }

        #[test]
        fn character_killed_while_locked_on_lets_go_as_died() {
            let nodes = format!(
                "{}{}",
                target("t", 0.0, -10.0),
                swinger("s", (0.0, 0.0, 0.0), &all_round(1, 1))
            );
            let mut run = hit_run(r#"("hp", 3)"#, &nodes);
            let events = events_of(&mut run, 2, press(LOCK, &[1]));
            assert_eq!(
                events,
                [
                    (1, vec![lock(Some("t"), LockReason::Pressed)]),
                    (
                        2,
                        vec![swung(0.0, Outcome::Death), lock(None, LockReason::Died)]
                    ),
                ]
            );
        }

        #[test]
        fn killed_dummy_revives_with_an_event_on_the_tick_it_stands() {
            // Killed on tick 2, dead on 2..=6, back on 7.
            let mut run = dummy_run(r#"("hp", 3), ("revive_ticks", 5)"#);
            let events = events_of(&mut run, 8, press(ATTACK, &[1]));
            assert_eq!(
                events,
                [
                    (1, vec![start(Action::Attack, 1, false)]),
                    (
                        2,
                        vec![struck("d", HitAttack::Attack, 1, 3.0, 0.0, Outcome::Death)]
                    ),
                    (3, vec![end(Action::Attack)]),
                    (
                        7,
                        vec![Event::Revive {
                            node: "d".to_owned()
                        }]
                    ),
                ]
            );
        }

        #[test]
        fn frozen_g3_3_demo_without_input_emits_no_events() {
            let mut run = Run::on(include_str!("g3_3_demo.scene.ron"));
            assert!(run.game.events().is_empty());
            let events = events_of(&mut run, 120, press(ATTACK, &[]));
            assert_eq!(events, []);
        }
    }

    // -------------------------------------------------------------- debug draw

    mod debug_draw {
        use loom_core::debug::{DebugDraw, DebugLine};

        use super::*;

        // The colors of docs/architecture.md section 11, "调试绘制".
        const DEAD: Vec3 = Vec3::new(0.4, 0.4, 0.4);
        const STAGGER: Vec3 = Vec3::new(1.0, 0.9, 0.1);
        const INVULNERABLE: Vec3 = Vec3::new(0.1, 0.9, 1.0);
        const GREEN: Vec3 = Vec3::new(0.2, 1.0, 0.2);
        const ORANGE: Vec3 = Vec3::new(1.0, 0.55, 0.1);
        const DARK_RED: Vec3 = Vec3::new(0.5, 0.1, 0.1);
        const RED: Vec3 = Vec3::new(1.0, 0.15, 0.15);
        const MAGENTA: Vec3 = Vec3::new(1.0, 0.2, 0.9);
        const WHITE: Vec3 = Vec3::new(0.9, 0.9, 0.9);
        const YELLOW: Vec3 = Vec3::new(1.0, 0.9, 0.1);
        const BLUE: Vec3 = Vec3::new(0.3, 0.5, 1.0);
        const DARK_BLUE: Vec3 = Vec3::new(0.2, 0.3, 0.6);

        /// Lines of a cylinder, and of a sector of `degrees` (two radii and
        /// an arc of up to 11.25° a segment).
        const CYLINDER: usize = 32 + 32 + 4;
        fn sector_lines(degrees: f32) -> usize {
            2 + (degrees / 11.25 - 1e-4).ceil() as usize
        }

        fn lines(run: &Run, categories: &[&str]) -> Vec<DebugLine> {
            let mut draw = DebugDraw::new();
            run.game.debug_draw(categories, &mut draw);
            draw.lines().to_vec()
        }

        fn near(a: f32, b: f32) -> bool {
            (a - b).abs() < 1e-5
        }

        /// Checks that `lines` is a cylinder standing on `base`.
        fn assert_cylinder(lines: &[DebugLine], base: Vec3, radius: f32, height: f32) {
            assert_eq!(lines.len(), CYLINDER);
            for l in &lines[..32] {
                assert!(near(l.a.distance(base), radius), "{:?}", l.a);
                assert_eq!(l.a.y, base.y);
            }
            let top = base + Vec3::new(0.0, height, 0.0);
            for l in &lines[32..64] {
                assert!(near(l.a.distance(top), radius), "{:?}", l.a);
            }
            for l in &lines[64..] {
                // Upright, from the bottom circle to the top one.
                assert_eq!((l.a.x, l.a.z), (l.b.x, l.b.z));
                assert!(near(l.b.y - l.a.y, height));
            }
        }

        /// The angle of a radius's outer end as seen from `center`, in the
        /// `bearing` convention.
        fn angle(center: Vec3, l: &DebugLine) -> f32 {
            libm::atan2f(-(l.b.x - center.x), -(l.b.z - center.z))
        }

        #[test]
        fn every_hurtbox_then_the_character_gets_a_green_cylinder_where_hits_check() {
            let mut run = dummy_run(r#"("hp", 10)"#);
            run.tick(false, &NONE);
            let l = lines(&run, &["hurtboxes"]);
            assert_eq!(l.len(), 2 * CYLINDER);
            assert_cylinder(&l[..CYLINDER], Vec3::new(0.0, 0.0, -1.5), 0.25, 1.8);
            assert_cylinder(&l[CYLINDER..], Vec3::ZERO, 0.35, 1.8);
            assert!(l.iter().all(|l| l.color == GREEN));
        }

        #[test]
        fn hurtbox_cylinders_are_yellow_staggered_gray_dead_and_the_character_cyan_invulnerable() {
            // Poise 0: the hit on tick 2 staggers.
            let mut run = dummy_run(r#"("hp", 10)"#);
            press_on(&mut run, ATTACK, &[1], 2, &NONE);
            let l = lines(&run, &["hurtboxes"]);
            assert!(l[..CYLINDER].iter().all(|l| l.color == STAGGER));
            assert!(l[CYLINDER..].iter().all(|l| l.color == GREEN));
            // hp 3: the same hit kills.
            let mut run = dummy_run(r#"("hp", 3)"#);
            press_on(&mut run, ATTACK, &[1], 2, &NONE);
            assert!(
                lines(&run, &["hurtboxes"])[..CYLINDER]
                    .iter()
                    .all(|l| l.color == DEAD)
            );
            // Rolling, invulnerable from its first tick.
            let mut run = Run::new(PROPS);
            run.tick(true, &NONE);
            let s = run.tick(false, &FORWARD);
            assert!(s.player.invulnerable);
            assert!(
                lines(&run, &["hurtboxes"])
                    .iter()
                    .all(|l| l.color == INVULNERABLE)
            );
        }

        #[test]
        fn character_hit_by_a_swing_shows_yellow_and_killed_gray() {
            let mut run = every_third_tick(r#"("hp", 100)"#, 1);
            run.ticks(2, false, &NONE);
            assert_eq!(run.game.state().player.health.state, Condition::Stagger);
            let l = lines(&run, &["hurtboxes"]);
            // The swinger has no hurtbox: the character's cylinder only.
            assert_eq!(l.len(), CYLINDER);
            assert!(l.iter().all(|l| l.color == STAGGER));
            let mut run = every_third_tick(r#"("hp", 3)"#, 1);
            run.ticks(2, false, &NONE);
            assert!(lines(&run, &["hurtboxes"]).iter().all(|l| l.color == DEAD));
        }

        #[test]
        fn dead_dummy_lying_down_keeps_an_upright_cylinder_on_its_node() {
            let mut run = dummy_run(r#"("hp", 3)"#);
            press_on(&mut run, ATTACK, &[1], 3, &NONE);
            // The mesh lies flat...
            assert!(up_of(&run, "d").y.abs() < 1e-5);
            // ...the cylinder stands where the node is.
            let l = lines(&run, &["hurtboxes"]);
            assert_cylinder(&l[..CYLINDER], Vec3::new(0.0, 0.0, -1.5), 0.25, 1.8);
        }

        #[test]
        fn attack_startup_draws_the_whole_sector_in_orange_at_both_band_heights() {
            // attack_props: startup ticks 1-4, 120° wide, reach 2.
            let mut run = Run::new(&attack_props());
            press_on(&mut run, ATTACK, &[1], 1, &NONE);
            let l = lines(&run, &["swings"]);
            let n = sector_lines(120.0);
            assert_eq!(l.len(), 2 * n);
            assert!(l.iter().all(|l| l.color == ORANGE));
            // Heights 0.3 and 1.6 (swing_low and swing_high), radii at ±60°.
            for (sector, y) in [(&l[..n], 0.3), (&l[n..], 1.6)] {
                let center = Vec3::new(0.0, y, 0.0);
                assert_eq!(sector[0].a, center);
                assert!(near(angle(center, &sector[0]), -60f32.to_radians()));
                assert!(near(angle(center, &sector[1]), 60f32.to_radians()));
                assert!(near(sector[0].b.distance(center), 2.0));
            }
        }

        #[test]
        fn active_attack_draws_dark_red_then_this_ticks_slice_in_bright_red() {
            // Active ticks 5-8 of attack_props sweep 30° each from the right:
            // tick 6 is [-30°, 0°], as the hit check (`slice_of`) has it.
            let mut run = Run::new(&attack_props());
            let states = press_on(&mut run, ATTACK, &[1], 6, &NONE);
            let p = &states[5].player;
            assert_eq!(
                (p.attack_phase, p.action_tick),
                (Some(AttackPhase::Active), 6)
            );
            let l = lines(&run, &["swings"]);
            let (full, slice) = (sector_lines(120.0), sector_lines(30.0));
            assert_eq!(l.len(), 2 * full + 2 * slice);
            assert!(l[..2 * full].iter().all(|l| l.color == DARK_RED));
            assert!(l[2 * full..].iter().all(|l| l.color == RED));
            // The character slid 0.125 forward on each active tick so far.
            let at = p.position;
            for (k, y) in [(0, 0.3), (1, 1.6)] {
                let s = &l[2 * full + k * slice..];
                let center = at + Vec3::new(0.0, y, 0.0);
                assert_eq!(s[0].a, center);
                assert!(near(angle(center, &s[0]), -30f32.to_radians()));
                assert!(near(angle(center, &s[1]), 0.0));
            }
            // Recovery (ticks 9-14) draws nothing.
            press_on(&mut run, ATTACK, &[], 3, &NONE);
            assert_eq!(
                run.game.state().player.attack_phase,
                Some(AttackPhase::Recovery)
            );
            assert!(lines(&run, &["swings"]).is_empty());
        }

        #[test]
        fn swinger_rounds_draw_orange_then_its_slice_and_nothing_in_recovery() {
            // RHYTHM: 180° in two 90° slices; startup 1-2, active 3-4,
            // recovery 5-6. Far away, facing -Z, band 0.3-1.6 (defaults).
            let at = Vec3::new(0.0, 0.0, -20.0);
            let mut run = hit_run("", &swinger("s", (0.0, 0.0, -20.0), RHYTHM));
            let n = sector_lines(180.0);
            run.tick(false, &NONE);
            let l = lines(&run, &["swings"]);
            assert_eq!(l.len(), 2 * n);
            assert!(l.iter().all(|l| l.color == ORANGE));
            assert_eq!(l[0].a, at + Vec3::new(0.0, 0.3, 0.0));
            assert_eq!(l[n].a, at + Vec3::new(0.0, 1.6, 0.0));
            run.ticks(3, false, &NONE); // tick 4: the second slice, [0°, 90°]
            let l = lines(&run, &["swings"]);
            let slice = sector_lines(90.0);
            assert_eq!(l.len(), 2 * n + 2 * slice);
            let s = &l[2 * n..2 * n + slice];
            assert!(s.iter().all(|l| l.color == RED));
            let center = at + Vec3::new(0.0, 0.3, 0.0);
            assert!(near(angle(center, &s[0]), 0.0));
            assert!(near(angle(center, &s[1]), 90f32.to_radians()));
            run.tick(false, &NONE); // tick 5: recovery
            assert!(lines(&run, &["swings"]).is_empty());
        }

        #[test]
        fn demo_scene_colliders_are_magenta_boxes_the_stake_block_among_them() {
            let run = Run::on(include_str!("demo.scene.ron"));
            let l = lines(&run, &["colliders"]);
            // Three pillars, the wall, three dummy blocks and the stake's.
            assert_eq!(l.len(), 8 * 12);
            assert!(l.iter().all(|l| l.color == MAGENTA));
            // The stake at (6, 0, 2), its block 0.3 x 1.6 x 0.3 at 0.8 up.
            for end in l[7 * 12..].iter().flat_map(|l| [l.a, l.b]) {
                let d = end - Vec3::new(6.0, 0.8, 2.0);
                assert!(
                    near(d.x.abs(), 0.15) && near(d.y.abs(), 0.8) && near(d.z.abs(), 0.15),
                    "{end}"
                );
            }
        }

        #[test]
        fn lock_draws_target_crosses_and_the_range_then_the_locked_one_in_yellow() {
            let mut run = Run::on(&scene_with("", &three_targets()));
            run.tick(false, &NONE);
            let l = lines(&run, &["lock"]);
            assert_eq!(l.len(), 3 * 3 + 32);
            assert!(l[..9].iter().all(|l| l.color == WHITE));
            // The mid target's lock point, 1.5 m up, with a 0.3 cross.
            let mid = Vec3::new(0.0, 1.5, -10.0);
            assert!(near(((l[3].a + l[3].b) * 0.5).distance(mid), 0.0));
            assert!(near(l[3].a.distance(l[3].b), 0.3));
            assert!(
                l[9..]
                    .iter()
                    .all(|l| l.color == BLUE && near(l.a.length(), 15.0))
            );

            run.hold(&[LOCK], &NONE);
            let l = lines(&run, &["lock"]);
            assert_eq!(l.len(), 9 + 3 + 1 + 32 + 32);
            assert!(l[9..13].iter().all(|l| l.color == YELLOW));
            assert!(near(l[9].a.distance(l[9].b), 0.5));
            // The line from the camera pivot (1.5 m up) to the lock point.
            assert_eq!(l[12].a, Vec3::new(0.0, 1.5, 0.0));
            assert!(near(l[12].b.distance(mid), 0.0));
            assert!(l[13..45].iter().all(|l| l.color == BLUE));
            assert!(
                l[45..]
                    .iter()
                    .all(|l| l.color == DARK_BLUE && near(l.a.length(), 20.0))
            );
        }

        #[test]
        fn dead_lock_target_gets_a_gray_cross() {
            let mut run = dummy_run(r#"("hp", 3)"#);
            press_on(&mut run, ATTACK, &[1], 2, &NONE);
            let l = lines(&run, &["lock"]);
            assert!(l[..3].iter().all(|l| l.color == DEAD));
        }

        #[test]
        fn only_the_categories_given_are_drawn() {
            let mut run = dummy_run(r#"("hp", 10)"#);
            press_on(&mut run, ATTACK, &[1], 1, &NONE);
            let lock_colors = [WHITE, DEAD, YELLOW, BLUE, DARK_BLUE];
            let l = lines(&run, &["lock"]);
            assert!(!l.is_empty());
            assert!(l.iter().all(|l| lock_colors.contains(&l.color)));
            assert!(lines(&run, &[]).is_empty());
            // Every category, in their order: the hurtboxes' lines first.
            let all = lines(&run, &crate::DEBUG_CATEGORIES);
            assert_eq!(&all[..2 * CYLINDER], &lines(&run, &["hurtboxes"])[..]);
            assert_eq!(
                all.len(),
                ["hurtboxes", "swings", "colliders", "lock"]
                    .iter()
                    .map(|c| lines(&run, &[*c]).len())
                    .sum::<usize>()
            );
        }

        #[test]
        fn drawing_every_tick_changes_no_state() {
            // The demo scene: walk to dummy_b locked on, attack, and stand in
            // reach of nothing while the stake swings.
            let input = |t: u32| -> (Vec<&'static str>, Vec<(&'static str, f32)>) {
                match t {
                    1 => (vec![LOCK], vec![(MOVE_Y, 1.0)]),
                    2..=94 => (vec![], vec![(MOVE_Y, 1.0)]),
                    100 | 115 | 190 => (vec![ATTACK], vec![]),
                    _ => (vec![], vec![]),
                }
            };
            let run_all = |draw: bool| {
                let mut run = Run::on(include_str!("demo.scene.ron"));
                (1..=300)
                    .map(|t| {
                        let (actions, axes) = input(t);
                        let s = run.hold(&actions, &axes);
                        if draw {
                            assert!(!lines(&run, &crate::DEBUG_CATEGORIES).is_empty());
                        }
                        (s, run.game.scene().clone())
                    })
                    .collect::<Vec<_>>()
            };
            let (plain, drawn) = (run_all(false), run_all(true));
            for (t, (a, b)) in plain.iter().zip(&drawn).enumerate() {
                assert!(a.0 == b.0, "tick {}", t + 1);
                assert!(a.1 == b.1, "tick {}: the scene differs", t + 1);
            }
        }
    }
}
