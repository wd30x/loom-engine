//! The events a tick emits, for traces and `loom serve`'s `step`
//! (`docs/architecture.md` section 11, "事件"): what the state dump cannot
//! show (hits, dodged swings, dropped presses, replaced buffers) and the
//! big moments of the timeline (actions starting and ending, lock-on
//! changes, revivals). Emitting them never changes the state.

use glam::Vec3;
use schemars::JsonSchema;
use serde::Serialize;

use crate::enemy::MoveKind;
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
    /// The character came back to life at its spawn point (step 0b).
    Respawn {
        /// `[x, y, z]`.
        #[schemars(with = "[f32; 3]")]
        position: Vec3,
    },
    /// An enemy chose a move (step 5c), before that move's first tick.
    EnemyMove {
        /// The enemy's node path.
        by: String,
        #[serde(rename = "move")]
        r#move: MoveKind,
        /// The character's horizontal distance.
        distance: f32,
        /// The moves it chose from, in their order.
        candidates: Vec<MoveKind>,
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
}

impl From<MoveKind> for HitAttack {
    fn from(kind: MoveKind) -> Self {
        match kind {
            MoveKind::Sweep => Self::Sweep,
            MoveKind::Cleave => Self::Cleave,
            MoveKind::Lunge => Self::Lunge,
            MoveKind::Combo => Self::Combo,
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

/// A pressed input: the roll key's tap, or the lock key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Input {
    Attack,
    Heavy,
    Roll,
    Lock,
}

impl From<Buffered> for Input {
    fn from(b: Buffered) -> Self {
        match b {
            Buffered::Attack => Self::Attack,
            Buffered::Heavy => Self::Heavy,
            Buffered::Roll => Self::Roll,
        }
    }
}

/// Why a press had no effect; when several apply, the first in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
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
}
