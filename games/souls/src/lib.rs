//! Game simulation: everything that advances per tick and must be
//! bit-identical on every machine. For now a capsule character with a
//! Dark Souls style orbit camera (`docs/architecture.md` section 11);
//! collision follows in G1.3.
//!
//! Transcendental functions come from `libm`, never from std (enforced by
//! this crate's `clippy.toml`). See `docs/architecture.md` sections 1 and 4.

mod demo;
mod game;

pub use demo::{bindings, demo_scene};
pub use game::{
    CameraState, Game, GameError, GameState, LOOK_X, LOOK_Y, MOVE_X, MOVE_Y, PlayerState,
};
