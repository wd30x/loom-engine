//! Engine core: the fixed-timestep simulation clock, frame pacing and
//! platform-independent input state.
//!
//! This crate must stay free of windowing and GPU dependencies so that the
//! simulation can run (and be tested) fully headless and deterministically.
//! See `docs/architecture.md`.

pub mod time;

pub use time::{FixedTimestep, MAX_TICKS_PER_FRAME, SimClock, TICK_DT, TICK_HZ};
