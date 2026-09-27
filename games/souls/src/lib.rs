//! Game simulation: everything that advances per tick and must be
//! bit-identical on every machine. A capsule character with a Dark Souls
//! style orbit camera, collision, stamina spent on rolls, backsteps,
//! sprinting and attacks, lock-on and hits (`docs/architecture.md` section 11).
//!
//! Transcendental functions come from `libm`, never from std (enforced by
//! this crate's `clippy.toml`). See `docs/architecture.md` sections 1 and 4.

mod demo;
mod game;
mod moves;

pub use demo::{bindings, demo_scene};
pub use game::{
    ATTACK, CameraState, Game, GameError, GameState, HEAVY, HurtboxState, LOCK, LOOK_X, LOOK_Y,
    LastHit, MOVE_X, MOVE_Y, PlayerState, ROLL,
};
pub use moves::{Action, AttackPhase, Buffered};
