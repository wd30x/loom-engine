//! Input: keys, action bindings, per-tick sampling, and input scripts for
//! replay and recording. See `docs/architecture.md` section 9.
//!
//! The simulation only ever reads [`ActionState`]. Live keyboard input
//! ([`InputCollector`]) and script replay ([`ScriptPlayer`]) both produce one
//! [`ActionSet`] per tick, so the simulation cannot tell them apart.

mod key;
mod live;
mod map;
mod script;

use std::collections::BTreeSet;

pub use key::Key;
pub use live::{ActionState, InputCollector};
pub use map::ActionMap;
pub use script::{InputEvent, InputRecorder, InputScript, ScriptPlayer};

/// The actions active during one tick. Ordered, so iteration and
/// serialization are deterministic.
pub type ActionSet = BTreeSet<String>;

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    /// RON syntax error, unknown field, unknown key name, ... Includes the
    /// line and column.
    #[error(transparent)]
    Syntax(#[from] ron::error::SpannedError),

    #[error("action {0:?} is defined more than once")]
    DuplicateAction(String),
    #[error("invalid action name {0:?}: must match ^[a-z][a-z0-9_]*$")]
    InvalidActionName(String),
    #[error("action {action:?} lists key {key:?} more than once")]
    RepeatedKey { action: String, key: Key },

    #[error("unsupported script version {0}, expected 1")]
    UnsupportedVersion(u32),
    #[error("script tick_hz is {found}, but the engine runs at {expected}")]
    TickRateMismatch { found: u32, expected: u32 },
    #[error("events[{index}] (tick {tick}): ticks start at 1")]
    TickZero { index: usize, tick: u64 },
    #[error(
        "events[{index}] (tick {tick}): tick is earlier than the previous event's tick {previous}"
    )]
    DecreasingTick {
        index: usize,
        tick: u64,
        previous: u64,
    },
    #[error("events[{index}] (tick {tick}): action {action:?} appears more than once in this tick")]
    RepeatedInTick {
        index: usize,
        tick: u64,
        action: String,
    },
    #[error("events[{index}] (tick {tick}): unknown action {action:?}")]
    UnknownAction {
        index: usize,
        tick: u64,
        action: String,
    },
    #[error("events[{index}] (tick {tick}): action {action:?} is pressed while already held")]
    PressWhileHeld {
        index: usize,
        tick: u64,
        action: String,
    },
    #[error("events[{index}] (tick {tick}): action {action:?} is released while not held")]
    ReleaseWhileIdle {
        index: usize,
        tick: u64,
        action: String,
    },
}
