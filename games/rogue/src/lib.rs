//! The rogue game: a 2D roguelike example, a simplified Cataclysm: Dark
//! Days Ahead. For now a player moving turn by turn on a tile map, one step
//! per key press, blocked by walls, chased by zombies it fights by bumping
//! into them (`docs/architecture.md` section 15).
//!
//! Positions are whole cells, so the simulation needs no transcendental
//! math; this crate's `clippy.toml` still forbids std's (section 4).

mod game;

pub use game::{
    Event, Game, GameError, GameState, HitResult, MOVES, Message, PlayerState, WAIT, ZombieState,
    bindings,
};

#[cfg(test)]
mod tests;
