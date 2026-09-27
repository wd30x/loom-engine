//! The events a tick emits, for traces and `loom serve`'s `step`
//! (`docs/architecture.md` section 11, "事件"): what the state dump cannot
//! show (hits, dodged swings, dropped presses, replaced buffers) and the
//! big moments of the timeline (actions starting and ending, lock-on
//! changes, revivals). Emitting them never changes the state.

use glam::Vec3;
use schemars::JsonSchema;
use serde::Serialize;

use crate::enemy::{EnemyChoice, MoveKind};
use crate::moves::{Action, Buffered};

/// One event; serialized as an object whose first field is `type`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The character hit a hurtbox, or a swinger hit the character.
    Hit {
        /// The character's node path, or the swinger's.
        by: String,
        /// The hurtbox's path, or the character's.
        target: String,
        attack: HitAttack,
        /// The light attack's `combo`; 0 otherwise.
        combo: u32,
        /// `None` when the target has no health.
        damage: Option<f32>,
        /// The target's health after the hit; `None` without health.
        hp: Option<f32>,
        result: Outcome,
    },
    /// A swinger's slice touched the character while invulnerable.
    Dodged {
        /// The swinger's node path.
        by: String,
        /// `Roll` or `Backstep`.
        action: Action,
        /// Which tick of the roll or backstep, from 1.
        action_tick: u32,
    },
    /// A press that had no effect at all.
    InputDropped {
        input: Input,
        reason: DropReason,
        /// Only for `NoStamina`: the dropped input was the buffered one.
        buffered: bool,
    },
    /// A press replaced the buffered action.
    BufferReplaced { old: Buffered, new: Buffered },
    /// The character started an action.
    ActionStart {
        /// Never `Idle`.
        action: Action,
        /// Which light attack of a combo, from 1; 0 for other actions.
        combo: u32,
        /// Started from the buffer.
        buffered: bool,
    },
    /// The character's action ended, back to idle.
    ActionEnd { action: Action, reason: EndReason },
    /// The lock-on target changed.
    Lock {
        /// The new target's node path; `None` when let go of.
        target: Option<String>,
        reason: LockReason,
    },
    /// A hurtbox came back to life (step 0b).
    Revive {
        /// The hurtbox's node path.
        node: String,
    },
    /// The character came back to life at its respawn point (step 0b),
    /// the world reset.
    Respawn {
        /// `[x, y, z]`.
        #[schemars(with = "[f32; 3]")]
        position: Vec3,
    },
    /// An enemy chose a move or to approach (step 5c), before that
    /// choice's first tick.
    EnemyMove {
        /// The enemy's node path.
        by: String,
        #[serde(rename = "move")]
        r#move: EnemyChoice,
        /// The character's horizontal distance.
        distance: f32,
        /// What it chose from: the moves in their order, then `approach`.
        candidates: Vec<EnemyChoice>,
    },
    /// An enemy entered a stage (step 4), right after the hit that took it
    /// there; it roars.
    Stage {
        /// The enemy's node path.
        by: String,
        /// Always 2.
        stage: u8,
    },
    /// The character sat down at a bonfire (step 4): full health and
    /// stamina, a new respawn point and the world reset.
    Rest {
        /// The bonfire's node path.
        bonfire: String,
    },
    /// The character stood up from a bonfire (step 4).
    Stand,
    /// The character's drink healed it (step 4), on the drink's
    /// `flask_heal_tick`.
    Heal {
        /// What it actually gave back: 0 at full health.
        amount: f32,
        /// The health after it.
        hp: f32,
    },
    /// The character killed a hurtbox with `souls` (step 4), right after
    /// that hit.
    Souls {
        /// The path of the node killed.
        from: String,
        amount: u32,
        /// The character's souls after it.
        total: u32,
    },
    /// The character died (step 5c): its souls dropped into a new
    /// bloodstain where it stood, and the old one's were lost.
    SoulsDropped {
        /// `[x, y, z]`.
        #[schemars(with = "[f32; 3]")]
        position: Vec3,
        /// The souls in the new bloodstain; 0 when none was left.
        amount: u32,
        /// The souls of the old bloodstain; 0 when there was none.
        lost: u32,
    },
    /// The interact key picked the bloodstain's souls back up (step 4a).
    SoulsRecovered {
        amount: u32,
        /// The character's souls after it.
        total: u32,
    },
}

/// What hit: the character's light or heavy attack, a swinger, or an
/// enemy's move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HitAttack {
    Attack,
    Heavy,
    Swing,
    Sweep,
    Cleave,
    Lunge,
    Combo,
    Spin,
    /// The blast of an enemy's roar on entering phase 2.
    Roar,
}

impl From<MoveKind> for HitAttack {
    fn from(kind: MoveKind) -> Self {
        match kind {
            MoveKind::Sweep => Self::Sweep,
            MoveKind::Cleave => Self::Cleave,
            MoveKind::Lunge => Self::Lunge,
            MoveKind::Combo => Self::Combo,
            MoveKind::Spin => Self::Spin,
        }
    }
}

/// What a hit did to the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// Neither of the others (always, without health).
    Hit,
    /// Staggered it, or staggered it again.
    Stagger,
    Death,
}

/// A pressed input: the roll key's tap, the lock key or the interact key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Input {
    Attack,
    Heavy,
    Roll,
    Lock,
    Interact,
    Drink,
}

impl From<Buffered> for Input {
    fn from(b: Buffered) -> Self {
        match b {
            Buffered::Attack => Self::Attack,
            Buffered::Heavy => Self::Heavy,
            Buffered::Roll => Self::Roll,
            Buffered::Drink => Self::Drink,
        }
    }
}

/// Why a press had no effect; when several apply, the first in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    /// Sitting at a bonfire.
    Sitting,
    Dead,
    Staggered,
    /// Rolling or backstepping.
    Rolling,
    /// In an attack's startup.
    Startup,
    /// A press of higher priority (or the buffered action) took effect.
    Outranked,
    NoStamina,
    /// The lock key found nothing to lock on to.
    NoTarget,
    /// The interact key, doing something other than standing or
    /// sprinting.
    Busy,
    /// The interact key, with no bonfire within its range.
    NoBonfire,
    /// The interact key, with a living enemy within its aggro range.
    EnemyNear,
    /// Drinking from the flask.
    Drinking,
    /// The drink key, with no flask left.
    Empty,
}

/// Why an action ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EndReason {
    /// Ran its ticks, or the sprint stopped.
    Finished,
    /// Staggered or killed.
    Interrupted,
}

/// Why the lock-on target changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LockReason {
    /// The lock key locked on or let go.
    Pressed,
    /// A sideways flick switched targets.
    Switched,
    OutOfRange,
    TargetDied,
    /// The character died.
    Died,
    /// The character sat down at a bonfire.
    Rested,
}
