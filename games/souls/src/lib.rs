//! Game simulation: everything that advances per tick and must be
//! bit-identical on every machine. A capsule character with a Dark Souls
//! style orbit camera, collision, stamina spent on rolls, backsteps,
//! sprinting and attacks, lock-on, hits, health, poise and death, and a
//! stake that swings at the character
//! (`docs/architecture.md` section 11).
//!
//! Transcendental functions come from `libm`, never from std (enforced by
//! this crate's `clippy.toml`). See `docs/architecture.md` sections 1 and 4.

mod demo;
mod enemy;
mod events;
mod game;
mod health;
mod moves;

pub use demo::{bindings, demo_scene, demo_scene_source};
pub use enemy::{EnemyChoice, EnemyCondition, EnemyState, MoveKind};
pub use events::{DropReason, EndReason, Event, HitAttack, Input, LockReason, Outcome};
pub use game::{
    ATTACK, CameraState, DEBUG_CATEGORIES, Game, GameError, GameState, HEAVY, HurtboxState,
    INTERACT, LOCK, LOOK_X, LOOK_Y, LastHit, MOVE_X, MOVE_Y, PlayerHit, PlayerState, ROLL,
    RespawnState, SwingerState,
};
pub use health::{Condition, HealthState};
pub use moves::{Action, AttackPhase, Buffered};

use loom_core::props::PropGroup;

/// The props of each kind of node, from the tables that check them, for
/// `loom schema` (`docs/architecture.md` section 17).
pub const PROP_GROUPS: [(&str, PropGroup); 2] = [
    (
        "character",
        PropGroup {
            props: &moves::PARAMS,
            requires: &[],
            constraints: &moves::CHARACTER_CONSTRAINTS,
        },
    ),
    (
        "node",
        PropGroup {
            props: &game::NODE_PROPS,
            requires: &game::NODE_REQUIRES,
            constraints: &game::NODE_CONSTRAINTS,
        },
    ),
];
