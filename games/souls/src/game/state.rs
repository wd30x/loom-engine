//! The state the game dumps (`docs/architecture.md` section 11): the
//! character, the camera, the hurtboxes, the swingers, the bloodstain and
//! the respawn.

use glam::Vec3;
use schemars::JsonSchema;
use serde::Serialize;

use crate::enemy::EnemyState;
use crate::health::HealthState;
use crate::moves::{Action, AttackPhase, Buffered};

/// The game state in the state dump (`game`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct GameState {
    pub player: PlayerState,
    pub camera: CameraState,
    /// The path of the locked-on target; `None` when not locked on.
    pub lock: Option<String>,
    /// Every hurtbox, in pre-order.
    pub hurtboxes: Vec<HurtboxState>,
    /// Every swinger, in pre-order.
    pub swingers: Vec<SwingerState>,
    /// Every enemy, in pre-order.
    pub enemies: Vec<EnemyState>,
    /// Where the character comes back after dying.
    pub respawn: RespawnState,
    /// The souls the character dropped on its last death; `None` when it
    /// left none or picked them back up.
    pub bloodstain: Option<BloodstainState>,
}

/// The bloodstain (`game.bloodstain`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
pub struct BloodstainState {
    /// Where the character died; serialized as `[x, y, z]`.
    #[schemars(with = "[f32; 3]")]
    pub position: Vec3,
    pub souls: u32,
}

/// The respawn point (`game.respawn`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct RespawnState {
    /// Feet position in meters; serialized as `[x, y, z]`.
    #[schemars(with = "[f32; 3]")]
    pub position: Vec3,
    /// Heading in radians around +Y, like the character's `facing`.
    pub facing: f32,
    /// The path of the bonfire last sat at; `None` for the spawn point.
    pub bonfire: Option<String>,
}

/// The character (`game.player`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct PlayerState {
    /// Feet position in meters; serialized as `[x, y, z]`.
    #[schemars(with = "[f32; 3]")]
    pub position: Vec3,
    /// Heading in radians around +Y, in `[-π, π)`; 0 faces -Z.
    pub facing: f32,
    /// From 0 to `stamina_max`.
    pub stamina: f32,
    /// Ticks left before stamina starts coming back.
    pub stamina_wait: u32,
    pub action: Action,
    /// Which tick of a roll, backstep or attack this is, from 1; 0
    /// otherwise.
    pub action_tick: u32,
    /// Inside the invulnerability window of a roll or backstep: swings
    /// miss.
    pub invulnerable: bool,
    /// The part of the attack running; `None` outside attacks.
    pub attack_phase: Option<AttackPhase>,
    /// Which light attack of a combo this is, from 1; 0 otherwise.
    pub combo: u32,
    /// The action to start once the running attack is over.
    pub buffered: Option<Buffered>,
    /// Times a swinger hit the character since loading.
    pub hits: u32,
    /// The latest hit on the character; `None` before the first.
    pub last_hit: Option<PlayerHit>,
    pub health: HealthState,
    /// Drinks left in the flask; `flask_charges` at load.
    pub flask: u32,
    /// Souls carried; 0 at load.
    pub souls: u32,
    /// Where the health bar's trail stands, in hp: full at load; it waits
    /// after a hit, then falls to `hp` (step 5d).
    pub hp_trail: f32,
    /// Ticks left before the trail starts falling.
    pub hp_trail_wait: u32,
}

/// One hit on the character.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct PlayerHit {
    /// The tick it happened on, as in the state dump's `tick`.
    pub tick: u64,
    /// The path of the swinger that hit.
    pub by: String,
}

/// The orbit camera (`game.camera`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
pub struct CameraState {
    /// Radians around +Y; 0 looks along -Z.
    pub yaw: f32,
    /// Radians; positive looks up.
    pub pitch: f32,
    /// Meters from the pivot to the camera: the orbit distance, or less
    /// while something is in the way.
    pub distance: f32,
}

/// [`GameState`] but the lock, the hurtboxes and the swingers, which
/// [`Game::state`](super::Game::state) adds: the lock as a path, the
/// others from [`Game`](super::Game)'s own lists.
#[derive(Debug, Clone)]
pub(super) struct Motion {
    pub(super) player: PlayerState,
    pub(super) camera: CameraState,
}

/// A hurtbox in the state dump (`game.hurtboxes`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct HurtboxState {
    /// The node's path.
    pub node: String,
    /// Times hit since loading.
    pub hits: u32,
    /// The latest hit; `None` before the first.
    pub last_hit: Option<LastHit>,
    /// `None` without an `hp` prop.
    pub health: Option<HealthState>,
}

/// A swinger in the state dump (`game.swingers`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct SwingerState {
    /// The node's path.
    pub node: String,
    /// Which tick of the round, from 1 to its length; 0 while staggered or
    /// dead.
    pub swing_tick: u32,
    /// `None` when `swing_tick` is 0.
    pub phase: Option<AttackPhase>,
}

/// One hit on a hurtbox.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
pub struct LastHit {
    /// The tick it happened on, as in the state dump's `tick`.
    pub tick: u64,
    /// `Attack` or `Heavy`.
    pub attack: Action,
    /// The attack's `combo` (0 for a heavy attack).
    pub combo: u32,
}
