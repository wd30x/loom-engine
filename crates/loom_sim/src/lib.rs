//! Game simulation: everything that advances per tick and must be
//! bit-identical on every machine. For now this is the cube demo; the
//! character, camera and collision follow in G1.
//!
//! Transcendental functions come from `libm`, never from std (enforced by
//! this crate's `clippy.toml`). See `docs/architecture.md` sections 1 and 4.

mod demo;

pub use demo::{CubeGame, GameError, LOOK_X, LOOK_Y, MOVE_X, MOVE_Y, demo_bindings, demo_scene};
