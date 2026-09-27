//! Stamina, roll, sprint, backstep, attacks, lock-on, hits, damage and the
//! character's own health: the parameters, read from the character node's props with defaults, and the
//! action state (`docs/architecture.md` section 11).

use loom_core::props::{PropRule, PropSpec};
use loom_scene::{NodeId, Scene};
use schemars::JsonSchema;
use serde::Serialize;

use crate::GameError;

/// What the character is doing; serialized in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
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
    /// Sitting at a bonfire.
    Sit,
    /// Drinking from the flask.
    Drink,
}

/// The part of an attack; serialized in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AttackPhase {
    Startup,
    Active,
    Recovery,
}

/// An action pressed during an attack, to start once it is over;
/// serialized in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Buffered {
    Attack,
    Heavy,
    /// The roll key: a roll, or a backstep without a direction.
    Roll,
    /// A drink from the flask.
    Drink,
}

/// Every prop the character understands, with its default and rule, in
/// the order they are checked. Reading, checking, rejecting unknown names
/// and `loom schema` all go through this one table.
pub(crate) const PARAMS: [PropSpec; 56] = [
    PropSpec {
        name: "stamina_max",
        default: Some(100.0),
        rule: PropRule::Positive,
        unit: "stamina points",
        description: "The most stamina the character has.",
    },
    PropSpec {
        name: "stamina_regen",
        default: Some(45.0),
        rule: PropRule::Positive,
        unit: "stamina points per second",
        description: "How fast stamina comes back.",
    },
    PropSpec {
        name: "stamina_delay_ticks",
        default: Some(30.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "ticks",
        description: "Ticks after using stamina before it comes back.",
    },
    PropSpec {
        name: "tap_ticks",
        default: Some(15.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Releasing roll within this many ticks is a tap (a roll, or a backstep without a direction); held longer, it sprints.",
    },
    PropSpec {
        name: "sprint_speed",
        default: Some(6.0),
        rule: PropRule::Positive,
        unit: "meters per second",
        description: "Speed while sprinting.",
    },
    PropSpec {
        name: "sprint_cost",
        default: Some(15.0),
        rule: PropRule::Positive,
        unit: "stamina points per second",
        description: "Stamina used while sprinting.",
    },
    PropSpec {
        name: "roll_cost",
        default: Some(20.0),
        rule: PropRule::Positive,
        unit: "stamina points",
        description: "Stamina a roll uses.",
    },
    PropSpec {
        name: "roll_ticks",
        default: Some(32.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "How long a roll lasts.",
    },
    PropSpec {
        name: "roll_distance",
        default: Some(3.2),
        rule: PropRule::Positive,
        unit: "meters",
        description: "How far a roll goes.",
    },
    PropSpec {
        name: "roll_invuln_from",
        default: Some(1.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Invulnerable from this tick of the roll (1-based).",
    },
    PropSpec {
        name: "roll_invuln_to",
        default: Some(20.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Invulnerable up to this tick of the roll, inclusive.",
    },
    PropSpec {
        name: "backstep_cost",
        default: Some(12.0),
        rule: PropRule::Positive,
        unit: "stamina points",
        description: "Stamina a backstep uses.",
    },
    PropSpec {
        name: "backstep_ticks",
        default: Some(20.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "How long a backstep lasts.",
    },
    PropSpec {
        name: "backstep_distance",
        default: Some(1.5),
        rule: PropRule::Positive,
        unit: "meters",
        description: "How far a backstep goes.",
    },
    PropSpec {
        name: "backstep_invuln_from",
        default: Some(1.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Invulnerable from this tick of the backstep (1-based).",
    },
    PropSpec {
        name: "backstep_invuln_to",
        default: Some(8.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Invulnerable up to this tick of the backstep, inclusive.",
    },
    PropSpec {
        name: "attack_cost",
        default: Some(15.0),
        rule: PropRule::Positive,
        unit: "stamina points",
        description: "Stamina a light attack uses.",
    },
    PropSpec {
        name: "attack_startup_ticks",
        default: Some(10.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Light attack: before the swing; the character can still turn.",
    },
    PropSpec {
        name: "attack_active_ticks",
        default: Some(4.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Light attack: the swing, sliding forward.",
    },
    PropSpec {
        name: "attack_recovery_ticks",
        default: Some(16.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Light attack: after the swing; the next action can be buffered.",
    },
    PropSpec {
        name: "attack_distance",
        default: Some(0.3),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Light attack: how far the character slides during the swing.",
    },
    PropSpec {
        name: "heavy_cost",
        default: Some(25.0),
        rule: PropRule::Positive,
        unit: "stamina points",
        description: "Stamina a heavy attack uses.",
    },
    PropSpec {
        name: "heavy_startup_ticks",
        default: Some(22.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Heavy attack: before the swing; the character can still turn.",
    },
    PropSpec {
        name: "heavy_active_ticks",
        default: Some(5.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Heavy attack: the swing, sliding forward.",
    },
    PropSpec {
        name: "heavy_recovery_ticks",
        default: Some(23.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Heavy attack: after the swing; the next action can be buffered.",
    },
    PropSpec {
        name: "heavy_distance",
        default: Some(0.6),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Heavy attack: how far the character slides during the swing.",
    },
    PropSpec {
        name: "attack_turn_speed",
        default: Some(180.0),
        rule: PropRule::Positive,
        unit: "degrees per second",
        description: "How fast the character turns during an attack's startup.",
    },
    PropSpec {
        name: "combo_max",
        default: Some(3.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "attacks",
        description: "The most light attacks in a row.",
    },
    PropSpec {
        name: "lock_range",
        default: Some(15.0),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Targets this close can be locked on to.",
    },
    PropSpec {
        name: "lock_break_range",
        default: Some(20.0),
        rule: PropRule::Positive,
        unit: "meters",
        description: "A locked-on target farther than this is let go.",
    },
    PropSpec {
        name: "lock_angle",
        default: Some(60.0),
        rule: PropRule::Positive,
        unit: "degrees",
        description: "Targets this far from where the camera looks can be locked on to.",
    },
    PropSpec {
        name: "lock_camera_speed",
        default: Some(360.0),
        rule: PropRule::Positive,
        unit: "degrees per second",
        description: "How fast the camera follows the locked-on target.",
    },
    PropSpec {
        name: "lock_pitch_down",
        default: Some(15.0),
        rule: PropRule::Below {
            min: 0.0,
            max: 90.0,
        },
        unit: "degrees",
        description: "How far below the lock point the camera looks.",
    },
    PropSpec {
        name: "switch_threshold",
        default: Some(0.1),
        rule: PropRule::Positive,
        unit: "radians",
        description: "look_x flicked while locked on, summed up to this, switches targets.",
    },
    PropSpec {
        name: "switch_cooldown_ticks",
        default: Some(20.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "ticks",
        description: "Ticks after a switch before the next one.",
    },
    PropSpec {
        name: "attack_reach",
        default: Some(2.0),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Radius of the light attack's sweep.",
    },
    PropSpec {
        name: "attack_arc",
        default: Some(120.0),
        rule: PropRule::Arc,
        unit: "degrees",
        description: "Angle the light attack sweeps, right to left, centered on the facing.",
    },
    PropSpec {
        name: "heavy_reach",
        default: Some(2.4),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Radius of the heavy attack's sweep.",
    },
    PropSpec {
        name: "heavy_arc",
        default: Some(90.0),
        rule: PropRule::Arc,
        unit: "degrees",
        description: "Angle the heavy attack sweeps, right to left, centered on the facing.",
    },
    PropSpec {
        name: "swing_low",
        default: Some(0.3),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "meters",
        description: "Above the target's feet: the bottom of the band the character's attacks sweep.",
    },
    PropSpec {
        name: "swing_high",
        default: Some(1.6),
        rule: PropRule::Any,
        unit: "meters",
        description: "Above the target's feet: the top of that band.",
    },
    PropSpec {
        name: "attack_damage",
        default: Some(20.0),
        rule: PropRule::Positive,
        unit: "hp",
        description: "Health a light hit takes off.",
    },
    PropSpec {
        name: "heavy_damage",
        default: Some(35.0),
        rule: PropRule::Positive,
        unit: "hp",
        description: "Health a heavy hit takes off.",
    },
    PropSpec {
        name: "attack_impact",
        default: Some(10.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "impact",
        description: "What a light hit adds up against the target's poise.",
    },
    PropSpec {
        name: "heavy_impact",
        default: Some(30.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "impact",
        description: "What a heavy hit adds up against the target's poise.",
    },
    PropSpec {
        name: "hp",
        default: Some(100.0),
        rule: PropRule::Positive,
        unit: "hp",
        description: "The character's own health.",
    },
    PropSpec {
        name: "poise",
        default: Some(0.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "impact",
        description: "Impacts added up past this stagger the character; 0: every hit with any impact does.",
    },
    PropSpec {
        name: "stagger_ticks",
        default: Some(30.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "How long a stagger lasts.",
    },
    PropSpec {
        name: "poise_reset_ticks",
        default: Some(60.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Ticks without a hit before impacts are forgotten.",
    },
    PropSpec {
        name: "respawn_ticks",
        default: Some(120.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Ticks lying dead before coming back at the start.",
    },
    PropSpec {
        name: "flask_charges",
        default: Some(3.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "drinks",
        description: "How many drinks the full flask holds; filled up at a bonfire and on the \
                      respawn.",
    },
    PropSpec {
        name: "flask_heal",
        default: Some(0.4),
        rule: PropRule::PositiveAtMost { max: 1.0 },
        unit: "fraction of full health",
        description: "What one drink gives back.",
    },
    PropSpec {
        name: "flask_ticks",
        default: Some(60.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "How long a drink takes.",
    },
    PropSpec {
        name: "flask_heal_tick",
        default: Some(30.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "tick",
        description: "The tick of a drink that heals; a stagger before it wastes the drink. At \
                      most flask_ticks.",
    },
    PropSpec {
        name: "flask_walk_speed",
        default: Some(1.0),
        rule: PropRule::Positive,
        unit: "meters per second",
        description: "Walking speed while drinking; per tick, less than the capsule radius.",
    },
    PropSpec {
        name: "bloodstain_range",
        default: Some(1.5),
        rule: PropRule::Positive,
        unit: "meters",
        description: "The interact key picks the bloodstain's souls back up within this \
                      horizontal distance.",
    },
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
    /// The flask: drinks when full, the fraction of full health one gives
    /// back, a drink's length and the tick it heals on, and meters per
    /// tick walked while drinking: `flask_walk_speed / 60`.
    pub flask_charges: u32,
    pub flask_heal: f32,
    pub flask_ticks: u32,
    pub flask_heal_tick: u32,
    pub flask_step: f32,
    /// How near, horizontally, the bloodstain must be to pick it up.
    pub bloodstain_range: f32,
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

/// Rules across the character's props, checked after each value, in this
/// order (`loom schema` writes them out).
pub(crate) const CHARACTER_CONSTRAINTS: [&str; 6] = [
    "1 <= roll_invuln_from <= roll_invuln_to <= roll_ticks",
    "1 <= backstep_invuln_from <= backstep_invuln_to <= backstep_ticks",
    "lock_range <= lock_break_range",
    "swing_low < swing_high",
    "flask_heal_tick <= flask_ticks",
    "each move per tick is shorter than the capsule radius: sprint_speed / 60, \
     roll_distance / roll_ticks, backstep_distance / backstep_ticks, \
     attack_distance / attack_active_ticks, heavy_distance / heavy_active_ticks, \
     flask_walk_speed / 60",
];

impl Params {
    /// Reads and checks the props of the character `id`, whose capsule has
    /// `radius`: unknown names (in file order), then each value (in table
    /// order), then [`CHARACTER_CONSTRAINTS`] in order: the invulnerability
    /// windows (roll, backstep), `lock_range <= lock_break_range`,
    /// `swing_low < swing_high`, `flask_heal_tick <= flask_ticks`, the
    /// per-tick distances (sprint, roll, backstep, attack, heavy, drink).
    pub fn new(scene: &Scene, id: NodeId, radius: f32) -> Result<Self, GameError> {
        let node = scene.node(id);
        let character = scene.path(id);
        if let Some((prop, _)) = node
            .props()
            .find(|(p, _)| !PARAMS.iter().any(|spec| spec.name == *p))
        {
            return Err(GameError::UnknownProp {
                character,
                prop: prop.to_owned(),
            });
        }
        let mut values = [0.0; PARAMS.len()];
        for (value, spec) in values.iter_mut().zip(&PARAMS) {
            let default = spec.default.expect("every character prop has a default");
            let v = node.prop(spec.name).unwrap_or(default);
            if !spec.rule.allows(v) {
                return Err(GameError::BadProp {
                    character,
                    prop: spec.name,
                    rule: spec.rule,
                    value: v,
                });
            }
            *value = v;
        }
        let get = |prop: &str| {
            let i = PARAMS
                .iter()
                .position(|spec| spec.name == prop)
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
            flask_charges: ticks("flask_charges"),
            flask_heal: get("flask_heal") as f32,
            flask_ticks: ticks("flask_ticks"),
            flask_heal_tick: ticks("flask_heal_tick"),
            flask_step: get("flask_walk_speed") as f32 / 60.0,
            bloodstain_range: get("bloodstain_range") as f32,
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
        if params.flask_heal_tick > params.flask_ticks {
            return Err(GameError::FlaskHealTick { character });
        }
        for (action, step) in [
            ("sprint", params.sprint_step),
            ("roll", params.roll.step),
            ("backstep", params.backstep.step),
            ("attack", params.attack.step),
            ("heavy", params.heavy.step),
            ("drink", params.flask_step),
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
            Action::Idle
            | Action::Sprint
            | Action::Attack
            | Action::Heavy
            | Action::Sit
            | Action::Drink => {
                unreachable!("only rolls and backsteps are moves")
            }
        }
    }

    /// The light or heavy attack `action`.
    pub fn attack_of(&self, action: Action) -> Attack {
        match action {
            Action::Attack => self.attack,
            Action::Heavy => self.heavy,
            Action::Idle
            | Action::Sprint
            | Action::Roll
            | Action::Backstep
            | Action::Sit
            | Action::Drink => {
                unreachable!("only light and heavy attacks are attacks")
            }
        }
    }
}

#[cfg(test)]
#[path = "moves_tests.rs"]
mod tests;
