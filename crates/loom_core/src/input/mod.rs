//! Input: keys, action and axis bindings, per-tick sampling, and input
//! scripts for replay and recording. See `docs/architecture.md` section 9.
//!
//! The simulation only ever reads [`ActionState`]. Live keyboard and mouse
//! input ([`InputCollector`]) and script replay ([`ScriptPlayer`]) both
//! produce one [`TickInput`] per tick, so the simulation cannot tell them
//! apart.

mod key;
mod live;
mod map;
mod script;

use std::collections::{BTreeMap, BTreeSet};

pub use key::Key;
pub use live::{ActionState, InputCollector};
pub use map::{ActionMap, MouseAxis};
pub use script::{AxisEvent, InputEvent, InputRecorder, InputScript, ScriptPlayer};

/// The actions active during one tick. Ordered, so iteration and
/// serialization are deterministic.
pub type ActionSet = BTreeSet<String>;

/// Everything the simulation gets as input for one tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TickInput {
    pub actions: ActionSet,
    /// Only the axes with a non-zero value; a missing axis is `0.0`. Values
    /// are finite and never `-0.0` (see [`canonical_axis_value`]).
    pub axes: BTreeMap<String, f32>,
}

impl TickInput {
    /// The value of `axis` this tick; `0.0` if it has no input.
    pub fn axis(&self, axis: &str) -> f32 {
        self.axes.get(axis).copied().unwrap_or(0.0)
    }
}

/// Maps `-0.0` to `0.0` and leaves every other value alone. `-0.0 == 0.0`,
/// so a recording would not notice the difference, but a state dump would
/// print `-0.0` where the replay prints `0.0`. Every axis value (sampled or
/// read from a script) goes through this.
pub fn canonical_axis_value(value: f32) -> f32 {
    if value == 0.0 { 0.0 } else { value }
}

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

    #[error("axis {0:?} is defined more than once")]
    DuplicateAxis(String),
    #[error("invalid axis name {0:?}: must match ^[a-z][a-z0-9_]*$")]
    InvalidAxisName(String),
    #[error("axis {0:?} has the same name as an action")]
    AxisNamedLikeAction(String),
    #[error("axis {axis:?} lists key {key:?} more than once")]
    RepeatedAxisKey { axis: String, key: Key },
    #[error("axis {axis:?}: the value of key {key:?} is not finite")]
    NonFiniteKeyValue { axis: String, key: Key },
    #[error("axis {0:?}: `mouse` and `mouse_scale` must be given together")]
    MouseWithoutScale(String),
    #[error("axis {0:?}: mouse_scale is not finite")]
    NonFiniteMouseScale(String),

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

    #[error("axes[{index}] (tick {tick}): ticks start at 1")]
    AxisTickZero { index: usize, tick: u64 },
    #[error(
        "axes[{index}] (tick {tick}): tick is earlier than the previous axis event's tick {previous}"
    )]
    AxisDecreasingTick {
        index: usize,
        tick: u64,
        previous: u64,
    },
    #[error("axes[{index}] (tick {tick}): axis {axis:?} appears more than once in this tick")]
    AxisRepeatedInTick {
        index: usize,
        tick: u64,
        axis: String,
    },
    #[error("axes[{index}] (tick {tick}): unknown axis {axis:?}")]
    UnknownAxis {
        index: usize,
        tick: u64,
        axis: String,
    },
    #[error("axes[{index}] (tick {tick}): value of axis {axis:?} is not finite")]
    NonFiniteAxisValue {
        index: usize,
        tick: u64,
        axis: String,
    },
    #[error("axes[{index}] (tick {tick}): axis {axis:?} is set to its current value {value}")]
    AxisUnchanged {
        index: usize,
        tick: u64,
        axis: String,
        value: f32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_zero_axis_value_becomes_positive_zero() {
        assert_eq!(canonical_axis_value(-0.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(canonical_axis_value(0.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(canonical_axis_value(-0.5).to_bits(), (-0.5f32).to_bits());
    }
}
