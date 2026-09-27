//! The game: a character that runs relative to an orbit camera and turns
//! smoothly towards where it runs. Every formula and its order is fixed by
//! `docs/architecture.md` section 11; changing either changes the state
//! dump.

use std::f32::consts::{PI, TAU};

use glam::{Quat, Vec2, Vec3};
use loom_core::SimClock;
use loom_core::debug::DebugDraw;
use loom_core::input::ActionState;
use loom_core::props::{PropRule, PropSpec};
use loom_scene::{MeshKind, NodeId, Scene, SceneError};
use schemars::JsonSchema;
use serde::Serialize;

use crate::enemy::{
    EnemyChoice, EnemyCondition, EnemyParams, EnemyState, MoveKind, MoveParams, SplitMix64,
};
use crate::events::{DropReason, EndReason, Event, HitAttack, Input, LockReason, Outcome};
use crate::health::{Condition, Health, HealthState, HpBar};
use crate::moves::{Action, Attack, AttackPhase, Buffered, Params};

/// Sideways movement in `[-1, 1]`; positive is to the camera's right.
pub const MOVE_X: &str = "move_x";
/// Forward movement in `[-1, 1]`; positive is away from the camera.
pub const MOVE_Y: &str = "move_y";
/// Horizontal camera turn in radians per tick; positive turns right.
pub const LOOK_X: &str = "look_x";
/// Vertical camera turn in radians per tick; positive looks up.
pub const LOOK_Y: &str = "look_y";
/// The roll key: a tap rolls (or backsteps without a direction), holding
/// it sprints.
pub const ROLL: &str = "roll";
/// A light attack, on the tick it is pressed.
pub const ATTACK: &str = "attack";
/// A heavy attack, on the tick it is pressed.
pub const HEAVY: &str = "heavy";
/// Locks on to a target, or lets go of it, on the tick it is pressed.
pub const LOCK: &str = "lock";
/// Sits down at a bonfire, and stands up; fires on the tick it is pressed.
pub const INTERACT: &str = "interact";
/// Drinks from the flask, on the tick it is pressed.
pub const DRINK: &str = "drink";

/// The eleven props of the move whose names start with `$m`, with its
/// defaults in the order damage, impact, startup, active, recovery,
/// reach, arc, min_range, max_range, weight, track_speed.
macro_rules! move_props {
    ($m:literal, $what:literal, [$damage:expr, $impact:expr, $startup:expr, $active:expr,
        $recovery:expr, $reach:expr, $arc:expr, $min:expr, $max:expr, $weight:expr,
        $track:expr]) => {
        [
            PropSpec {
                name: concat!($m, "damage"),
                default: Some($damage),
                rule: PropRule::Positive,
                unit: "hp",
                description: concat!($what, ": health each hit takes off the character."),
            },
            PropSpec {
                name: concat!($m, "impact"),
                default: Some($impact),
                rule: PropRule::AtLeast { min: 0.0 },
                unit: "impact",
                description: concat!($what, ": what each hit adds up against the character's poise."),
            },
            PropSpec {
                name: concat!($m, "startup_ticks"),
                default: Some($startup),
                rule: PropRule::Whole { min: 1, max: None },
                unit: "ticks",
                description: concat!($what, ": the warning before the swing, turning to the character at track_speed."),
            },
            PropSpec {
                name: concat!($m, "active_ticks"),
                default: Some($active),
                rule: PropRule::Whole { min: 1, max: None },
                unit: "ticks",
                description: concat!($what, ": the swing (for a combo, each cut's)."),
            },
            PropSpec {
                name: concat!($m, "recovery_ticks"),
                default: Some($recovery),
                rule: PropRule::Whole { min: 1, max: None },
                unit: "ticks",
                description: concat!($what, ": after the swing (for a combo, the last cut's)."),
            },
            PropSpec {
                name: concat!($m, "reach"),
                default: Some($reach),
                rule: PropRule::Positive,
                unit: "meters",
                description: concat!($what, ": radius of the swept sector."),
            },
            PropSpec {
                name: concat!($m, "arc"),
                default: Some($arc),
                rule: PropRule::Arc,
                unit: "degrees",
                description: concat!($what, ": angle swept, right to left, centered on the facing."),
            },
            PropSpec {
                name: concat!($m, "min_range"),
                default: Some($min),
                rule: PropRule::AtLeast { min: 0.0 },
                unit: "meters",
                description: concat!($what, ": chosen only with the character at least this far, horizontally."),
            },
            PropSpec {
                name: concat!($m, "max_range"),
                default: Some($max),
                rule: PropRule::Positive,
                unit: "meters",
                description: concat!($what, ": chosen only with the character at most this far; at least min_range."),
            },
            PropSpec {
                name: concat!($m, "weight"),
                default: Some($weight),
                rule: PropRule::Whole { min: 0, max: None },
                unit: "weight",
                description: concat!($what, ": how likely it is chosen, against the weights of the other moves in range; 0: never."),
            },
            PropSpec {
                name: concat!($m, "track_speed"),
                default: Some($track),
                rule: PropRule::AtLeast { min: 0.0 },
                unit: "degrees per second",
                description: concat!($what, ": how fast the enemy turns to the character during the startup."),
            },
        ]
    };
}

/// The props a node other than the character may have, in the order they
/// are checked, with the default of those that have one:
/// - `lock_height` makes it a lock-on target (locked on this many meters
///   above its position);
/// - `hurt_radius` and `hurt_height` together make it a hurtbox;
/// - `hp` makes a hurtbox take damage; the four after it go with `hp`;
/// - `swing_damage` makes it a swinger, hitting the character; the eight
///   after it go with `swing_damage`;
/// - `enemy_speed` makes it an enemy; all the others go with it: the rest
///   of [`ENEMY_PROPS`], eleven for each move, then [`ENEMY_TAIL`],
///   [`APPROACH_PROPS`] and [`PHASE2_PROPS`] (all but `phase2_at` going
///   with it too), then the spin's eleven;
/// - `bonfire_range` makes it a bonfire;
/// - `souls` goes with `hp`: what killing it gives.
pub(crate) const NODE_PROPS: [PropSpec; 97] = join(&[
    &BASE_PROPS,
    &ENEMY_PROPS,
    &move_props!(
        "sweep_",
        "Sweep, a wide swing",
        [25.0, 20.0, 30.0, 8.0, 30.0, 3.0, 160.0, 0.0, 3.0, 3.0, 90.0]
    ),
    &move_props!(
        "cleave_",
        "Cleave, a narrow far-reaching swing",
        [45.0, 40.0, 45.0, 4.0, 40.0, 4.0, 20.0, 0.0, 4.0, 2.0, 150.0]
    ),
    &move_props!(
        "lunge_",
        "Lunge, a charge forward",
        [
            30.0, 30.0, 35.0, 10.0, 35.0, 2.5, 90.0, 4.0, 9.0, 2.0, 180.0
        ]
    ),
    &move_props!(
        "combo_",
        "Combo, cuts in a row",
        [20.0, 15.0, 25.0, 5.0, 45.0, 2.8, 140.0, 0.0, 2.8, 2.0, 90.0]
    ),
    &ENEMY_TAIL,
    &APPROACH_PROPS,
    &PHASE2_PROPS,
    &move_props!(
        "spin_",
        "Spin, a full turn on the spot, only in phase 2",
        [40.0, 40.0, 50.0, 12.0, 40.0, 3.5, 360.0, 0.0, 3.5, 2.0, 0.0]
    ),
    &[PropSpec {
        name: "bonfire_range",
        default: None,
        rule: PropRule::Positive,
        unit: "meters",
        description: "Makes the node a bonfire: the character sits down at it within this \
                      horizontal distance.",
    }],
    &[PropSpec {
        name: "souls",
        default: Some(0.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "souls",
        description: "What the character gets each time it kills the node.",
    }],
]);

/// `parts` one after the other, which must be `N` in all.
const fn join<const N: usize>(parts: &[&[PropSpec]]) -> [PropSpec; N] {
    let mut out = [BASE_PROPS[0]; N];
    let mut i = 0;
    let mut p = 0;
    while p < parts.len() {
        let mut j = 0;
        while j < parts[p].len() {
            out[i] = parts[p][j];
            i += 1;
            j += 1;
        }
        p += 1;
    }
    assert!(i == N, "the parts add up to N");
    out
}

/// The props a node could always have: targets, hurtboxes, health and
/// swingers.
const BASE_PROPS: [PropSpec; 17] = [
    PropSpec {
        name: "lock_height",
        default: None,
        rule: PropRule::Positive,
        unit: "meters",
        description: "Makes the node a lock-on target, locked on this far above its position.",
    },
    PropSpec {
        name: "hurt_radius",
        default: None,
        rule: PropRule::Positive,
        unit: "meters",
        description: "With hurt_height, makes the node a hurtbox: a vertical cylinder of this radius around its position.",
    },
    PropSpec {
        name: "hurt_height",
        default: None,
        rule: PropRule::Positive,
        unit: "meters",
        description: "With hurt_radius: the height of the cylinder, from the node's position up.",
    },
    PropSpec {
        name: "hp",
        default: None,
        rule: PropRule::Positive,
        unit: "hp",
        description: "Gives a hurtbox health: it takes damage, staggers and dies.",
    },
    PropSpec {
        name: "poise",
        default: Some(0.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "impact",
        description: "Impacts added up past this stagger it.",
    },
    PropSpec {
        name: "stagger_ticks",
        default: Some(30.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "How long a stagger lasts.",
    },
    PropSpec {
        name: "poise_reset_ticks",
        default: Some(60.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Ticks without a hit before impacts are forgotten.",
    },
    PropSpec {
        name: "revive_ticks",
        default: Some(180.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Ticks lying dead before coming back with full health.",
    },
    PropSpec {
        name: "swing_damage",
        default: None,
        rule: PropRule::Positive,
        unit: "hp",
        description: "Makes the node a swinger, swinging at the character: health a hit takes off it.",
    },
    PropSpec {
        name: "swing_impact",
        default: Some(20.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "impact",
        description: "What a hit adds up against the character's poise.",
    },
    PropSpec {
        name: "swing_startup_ticks",
        default: Some(40.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "The warning before the swing.",
    },
    PropSpec {
        name: "swing_active_ticks",
        default: Some(6.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "The swing.",
    },
    PropSpec {
        name: "swing_recovery_ticks",
        default: Some(74.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "After the swing, until the next round.",
    },
    PropSpec {
        name: "swing_reach",
        default: Some(2.2),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Radius of the swept sector.",
    },
    PropSpec {
        name: "swing_arc",
        default: Some(150.0),
        rule: PropRule::Arc,
        unit: "degrees",
        description: "Angle swept, right to left, centered on -Z.",
    },
    PropSpec {
        name: "swing_low",
        default: Some(0.3),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "meters",
        description: "Above the character's feet: the bottom of the band the swing sweeps.",
    },
    PropSpec {
        name: "swing_high",
        default: Some(1.6),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Above the character's feet: the top of the band the swing sweeps.",
    },
];

/// An enemy's own props, `enemy_speed` first.
const ENEMY_PROPS: [PropSpec; 8] = [
    PropSpec {
        name: "enemy_speed",
        default: None,
        rule: PropRule::Positive,
        unit: "meters per second",
        description: "Makes the node an enemy: how fast it walks up to the character. Per tick, \
                      less than hurt_radius.",
    },
    PropSpec {
        name: "enemy_turn_speed",
        default: Some(180.0),
        rule: PropRule::Positive,
        unit: "degrees per second",
        description: "How fast it turns to the character walking, resting and before a move.",
    },
    PropSpec {
        name: "enemy_face_angle",
        default: Some(30.0),
        rule: PropRule::PositiveAtMost { max: 180.0 },
        unit: "degrees",
        description: "A move starts only with the character at most this far off its facing.",
    },
    PropSpec {
        name: "enemy_rest_ticks",
        default: Some(40.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "ticks",
        description: "The rest after each move: the character's window to hit back.",
    },
    PropSpec {
        name: "enemy_aggro_range",
        default: Some(15.0),
        rule: PropRule::Positive,
        unit: "meters",
        description: "With the character farther than this, horizontally, it stands still.",
    },
    PropSpec {
        name: "enemy_seed",
        default: Some(1.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "seed",
        description: "The seed of its random numbers, which choose its moves.",
    },
    PropSpec {
        name: "enemy_swing_low",
        default: Some(0.3),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "meters",
        description: "Above its feet: the bottom of the band every move sweeps.",
    },
    PropSpec {
        name: "enemy_swing_high",
        default: Some(2.4),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Above its feet: the top of the band every move sweeps; above \
                      enemy_swing_low.",
    },
];

/// The enemy props after the moves'.
const ENEMY_TAIL: [PropSpec; 3] = [
    PropSpec {
        name: "lunge_distance",
        default: Some(5.0),
        rule: PropRule::Positive,
        unit: "meters",
        description: "How far the lunge charges, evenly over its active ticks; per tick, less \
                      than hurt_radius.",
    },
    PropSpec {
        name: "combo_hits",
        default: Some(3.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "cuts",
        description: "How many cuts the combo has.",
    },
    PropSpec {
        name: "combo_gap_ticks",
        default: Some(12.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "ticks",
        description: "The wait before each cut of the combo after the first, a startup again.",
    },
];

/// An enemy's approach, the choice of walking up to the character: the
/// last enemy props.
const APPROACH_PROPS: [PropSpec; 3] = [
    PropSpec {
        name: "approach_min_range",
        default: Some(2.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "meters",
        description: "Approaching is a choice only with the character farther than this, \
                      horizontally.",
    },
    PropSpec {
        name: "approach_weight_per_meter",
        default: Some(2.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "weight per meter",
        description: "The approach's weight is this times the meters beyond \
                      approach_min_range, rounded down; 0 leaves it out.",
    },
    PropSpec {
        name: "approach_ticks",
        default: Some(30.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "The most ticks one approach walks.",
    },
];

/// The approach's cap, then phase 2: `phase2_at` turns it on, and the
/// seven after it go with it.
const PHASE2_PROPS: [PropSpec; 9] = [
    PropSpec {
        name: "approach_weight_max",
        default: Some(4.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "weight",
        description: "The most the approach's weight can be; 0 never approaches.",
    },
    PropSpec {
        name: "phase2_at",
        default: None,
        rule: PropRule::Between { min: 0.0, max: 1.0 },
        unit: "fraction of full health",
        description: "Makes a phase 2: entered, roaring, once a hit leaves at most this much of \
                      its full health.",
    },
    PropSpec {
        name: "phase2_speed_scale",
        default: Some(0.75),
        rule: PropRule::PositiveAtMost { max: 1.0 },
        unit: "time scale",
        description: "Phase 2: startups, the combo's gap and the rest times this, rounded; \
                      walking, turning and tracking divided by it.",
    },
    PropSpec {
        name: "phase2_combo_hits",
        default: Some(4.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "cuts",
        description: "Phase 2: how many cuts the combo has.",
    },
    PropSpec {
        name: "roar_ticks",
        default: Some(70.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "How long the roar on entering phase 2 lasts, invulnerable.",
    },
    PropSpec {
        name: "roar_blast_tick",
        default: Some(20.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "tick",
        description: "The tick of the roar with the blast all around; at most roar_ticks.",
    },
    PropSpec {
        name: "roar_damage",
        default: Some(30.0),
        rule: PropRule::Positive,
        unit: "hp",
        description: "Health the blast takes off the character.",
    },
    PropSpec {
        name: "roar_impact",
        default: Some(40.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "impact",
        description: "What the blast adds up against the character's poise.",
    },
    PropSpec {
        name: "roar_reach",
        default: Some(4.0),
        rule: PropRule::Positive,
        unit: "meters",
        description: "The radius of the blast.",
    },
];

/// Where the props that go with `hp`, with `swing_damage` and with
/// `enemy_speed` are in [`NODE_PROPS`].
const HP_PROPS: std::ops::Range<usize> = 4..8;
const SWING_PROPS: std::ops::Range<usize> = 9..17;
const ENEMY_OTHER_PROPS: std::ops::Range<usize> = 18..95;
const PHASE2_OTHER_PROPS: std::ops::Range<usize> = 77..84;
/// Where `souls`, which also goes with `hp`, is in [`NODE_PROPS`].
const SOULS_PROP: usize = 96;

/// The name of the child of a hurtbox with `hp` that shows its health.
const HP_BAR: &str = "hp_bar";
/// The name of the child of a swinger that shows its swing.
const ARM: &str = "arm";

/// How long a hurtbox flashes after a hit, in ticks, counting the tick of
/// the hit. A stand-in for real hit feedback, like the constants below.
const FLASH_TICKS: u64 = 8;
/// The color a hurtbox's meshes flash in.
const FLASH_COLOR: Vec3 = Vec3::ONE;
/// A dead hurtbox's meshes are their scene colors times this.
const DEAD_DIM: f32 = 0.4;
/// How far a staggered hurtbox tips over, away from its attacker, in degrees.
const STAGGER_TILT_DEGREES: f32 = 20.0;
/// How far a dead hurtbox tips over: flat on the ground.
const DEAD_TILT_DEGREES: f32 = 90.0;
/// The color of a swinger's arm during its startup: the warning that a
/// swing is coming. A stand-in, like the flash.
const WARN_COLOR: Vec3 = Vec3::new(1.0, 0.55, 0.1);
/// How far below its `position` the character node is drawn while sitting.
const SIT_DROP: f32 = 0.5;
/// The debug draw's bonfire circles and respawn cross: warm orange.
const DEBUG_BONFIRE: Vec3 = Vec3::new(1.0, 0.6, 0.2);
/// The size of the cross above the respawn point's bonfire.
const DEBUG_BONFIRE_CROSS: f32 = 0.5;
/// The debug draw's bloodstain circle: the marker's green.
const DEBUG_BLOODSTAIN: Vec3 = Vec3::new(0.3, 0.9, 0.5);
/// The name of the root node that marks the bloodstain.
const BLOODSTAIN: &str = "bloodstain";

/// What an enemy's scene colors are multiplied by in phase 2: dark red.
const PHASE2_TINT: Vec3 = Vec3::new(1.0, 0.3, 0.3);

/// The debug draw categories, in this order (`docs/architecture.md`
/// section 11, "调试绘制").
pub const DEBUG_CATEGORIES: [&str; 6] = [
    "hurtboxes",
    "swings",
    "colliders",
    "lock",
    "bonfires",
    "bloodstain",
];

/// Debug draw colors, linear RGB.
const DEBUG_DEAD: Vec3 = Vec3::new(0.4, 0.4, 0.4);
const DEBUG_STAGGER: Vec3 = Vec3::new(1.0, 0.9, 0.1);
const DEBUG_INVULNERABLE: Vec3 = Vec3::new(0.1, 0.9, 1.0);
const DEBUG_HURTBOX: Vec3 = Vec3::new(0.2, 1.0, 0.2);
const DEBUG_STARTUP: Vec3 = Vec3::new(1.0, 0.55, 0.1);
const DEBUG_ACTIVE: Vec3 = Vec3::new(0.5, 0.1, 0.1);
const DEBUG_SLICE: Vec3 = Vec3::new(1.0, 0.15, 0.15);
const DEBUG_COLLIDER: Vec3 = Vec3::new(1.0, 0.2, 0.9);
const DEBUG_TARGET: Vec3 = Vec3::new(0.9, 0.9, 0.9);
const DEBUG_LOCKED: Vec3 = Vec3::new(1.0, 0.9, 0.1);
const DEBUG_LOCK_RANGE: Vec3 = Vec3::new(0.3, 0.5, 1.0);
const DEBUG_BREAK_RANGE: Vec3 = Vec3::new(0.2, 0.3, 0.6);
/// Sizes of the lock point crosses: every target's, and the locked one's.
const DEBUG_TARGET_CROSS: f32 = 0.3;
const DEBUG_LOCKED_CROSS: f32 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error(transparent)]
    Scene(#[from] SceneError),
    #[error("the scene needs a node with a character component")]
    NoCharacter,
    #[error("the active camera {0:?} needs an orbit_camera component")]
    NoOrbitCamera(String),
    #[error("{0:?} must be a root node: the game sets its local transform directly")]
    NotRoot(String),
    #[error("the character {0:?} needs a Capsule mesh: it is also the collision shape")]
    NotACapsule(String),
    #[error(
        "the character {0:?} must not be scaled: the capsule is drawn scaled but collides unscaled"
    )]
    ScaledCharacter(String),
    #[error(
        "run_speed {run_speed} is too fast for the capsule radius {radius}: \
         run_speed / 60 must be less than the radius"
    )]
    TooFast { run_speed: f32, radius: f32 },
    #[error("collider {0:?} moves with the player or the camera; colliders must be static")]
    MovingCollider(String),
    #[error("the character {character:?}: unknown prop {prop:?}")]
    UnknownProp { character: String, prop: String },
    #[error("the character {character:?}: prop {prop:?} must be {rule}, got {value}")]
    BadProp {
        character: String,
        prop: &'static str,
        rule: PropRule,
        value: f64,
    },
    #[error(
        "the character {character:?}: needs 1 <= {action}_invuln_from <= {action}_invuln_to \
         <= {action}_ticks"
    )]
    InvulnWindow {
        character: String,
        action: &'static str,
    },
    #[error("the character {character:?}: needs lock_range <= lock_break_range")]
    LockRanges { character: String },
    #[error("the character {character:?}: needs swing_low < swing_high")]
    SwingBand { character: String },
    #[error("the character {character:?}: needs flask_heal_tick <= flask_ticks")]
    FlaskHealTick { character: String },
    #[error("the character {0:?}: its flask needs a mesh")]
    FlaskWithoutMesh(String),
    #[error(
        "the character {character:?}: {action} moves {step} m per tick, which must be less than \
         the capsule radius {radius}"
    )]
    StepTooLong {
        character: String,
        action: &'static str,
        step: f32,
        radius: f32,
    },
    #[error("the node {node:?}: unknown prop {prop:?} (see loom schema: games.souls.props.node)")]
    UnknownNodeProp { node: String, prop: String },
    #[error("the node {node:?}: prop {prop:?} must be {rule}, got {value}")]
    NodeBadProp {
        node: String,
        prop: &'static str,
        rule: PropRule,
        value: f64,
    },
    #[error("the node {0:?}: needs both hurt_radius and hurt_height, or neither")]
    HurtPropsUnpaired(String),
    #[error("the node {0:?}: hp needs hurt_radius and hurt_height")]
    HpWithoutHurtbox(String),
    #[error("the node {node:?}: prop {prop:?} needs hp")]
    NeedsHp { node: String, prop: &'static str },
    #[error("the node {node:?}: prop {prop:?} needs swing_damage")]
    NeedsSwingDamage { node: String, prop: &'static str },
    #[error("the node {0:?}: needs swing_low < swing_high")]
    NodeSwingBand(String),
    #[error("the node {0:?}: its hp_bar needs a mesh")]
    HpBarWithoutMesh(String),
    #[error("the node {0:?}: its arm needs a mesh")]
    ArmWithoutMesh(String),
    #[error("the lock target {0:?} must be a root node, and not the camera")]
    LockTargetNotRoot(String),
    #[error("the hurtbox {0:?} must be a root node, and not the camera")]
    HurtboxNotRoot(String),
    #[error("the swinger {0:?} must be a root node, and not the camera")]
    SwingerNotRoot(String),
    #[error("the node {node:?}: prop {prop:?} needs enemy_speed")]
    NeedsEnemySpeed { node: String, prop: &'static str },
    #[error("the node {node:?}: prop {prop:?} needs phase2_at")]
    NeedsPhase2 { node: String, prop: &'static str },
    #[error("the bonfire {0:?} must be a root node, and not the camera")]
    BonfireNotRoot(String),
    #[error("the bonfire {0:?} cannot also be a lock target, hurtbox, swinger or enemy")]
    BonfireRole(String),
    #[error("the node {0:?}: needs roar_blast_tick <= roar_ticks")]
    EnemyRoarBlast(String),
    #[error("the node {node:?}: needs {m}_min_range <= {m}_max_range")]
    EnemyRanges { node: String, m: &'static str },
    #[error("the node {0:?}: needs enemy_swing_low < enemy_swing_high")]
    EnemySwingBand(String),
    #[error("the enemy {0:?} needs hurt_radius, hurt_height, hp and lock_height")]
    EnemyIncomplete(String),
    #[error("the enemy {0:?} cannot also be a swinger")]
    EnemySwinger(String),
    #[error(
        "the enemy {enemy:?}: {what} moves {step} m per tick, which must be less than hurt_radius \
         {radius}"
    )]
    EnemyStepTooLong {
        enemy: String,
        what: &'static str,
        step: f32,
        radius: f32,
    },
    #[error("the enemy {0:?} must not have a collider in its subtree: colliders are static")]
    EnemyCollider(String),
    #[error("the bloodstain marker {0:?} needs a mesh, and no props or children")]
    BloodstainMarker(String),
}

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

/// A `collider: Box` as seen from above: an oriented rectangle on the XZ
/// plane plus a height range. Computed once at load time
/// (`docs/architecture.md` section 11, collision).
#[derive(Debug, Clone, Copy, PartialEq)]
struct BoxCollider {
    center: Vec2,
    /// The box's local X and Z axes on the XZ plane.
    a: Vec2,
    b: Vec2,
    /// Half extents along `a` and `b`.
    ha: f32,
    hb: f32,
    y_min: f32,
    y_max: f32,
}

impl BoxCollider {
    fn new(scene: &Scene, id: NodeId) -> Self {
        // Ancestors only translate (scene validation), so the local rotation
        // and scale are the world ones.
        let m = scene.world_matrix(id);
        let t = scene.transform(id);
        let a3 = t.rotation * Vec3::X;
        let b3 = t.rotation * Vec3::Z;
        Self {
            center: Vec2::new(m.w_axis.x, m.w_axis.z),
            a: Vec2::new(a3.x, a3.z),
            b: Vec2::new(b3.x, b3.z),
            ha: 0.5 * t.scale.x,
            hb: 0.5 * t.scale.z,
            y_min: m.w_axis.y - 0.5 * t.scale.y,
            y_max: m.w_axis.y + 0.5 * t.scale.y,
        }
    }
}

/// [`GameState`] but the lock, the hurtboxes and the swingers, which
/// [`Game::state`] adds: the lock as a path, the others from [`Game`]'s own
/// lists.
#[derive(Debug, Clone)]
struct Motion {
    player: PlayerState,
    camera: CameraState,
}

/// A node with a `lock_height`: something to lock on to.
#[derive(Debug, Clone)]
struct Target {
    id: NodeId,
    /// The path, for the state dump.
    path: String,
    /// The lock point is this far above the node's position.
    height: f32,
    /// The index in `Game::hurtboxes` of the same node, if it is one: a
    /// dead one cannot be locked on to.
    hurtbox: Option<usize>,
}

/// A node with `hurt_radius` and `hurt_height`: something attacks hit. A
/// vertical cylinder standing on the node's position.
#[derive(Debug, Clone)]
struct Hurtbox {
    id: NodeId,
    /// The path, for the state dump.
    path: String,
    radius: f32,
    height: f32,
    /// Every node with a mesh in the subtree (the node itself included)
    /// but the hp bar, in pre-order, with its color in the scene file: what
    /// flashes and darkens.
    meshes: Vec<(NodeId, Vec3)>,
    /// The node's rotation in the scene file: its pose standing.
    base: Quat,
    hits: u32,
    last_hit: Option<LastHit>,
    /// With an `hp` prop: health, poise and death.
    health: Option<Health>,
    /// What killing it gives the character.
    souls: u32,
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

/// A node with `swing_damage`: it swings at the character on a fixed
/// rhythm (startup, active, recovery, over and over), sweeping a sector
/// right to left like the character's attacks, and never turns.
#[derive(Debug, Clone)]
struct Swinger {
    id: NodeId,
    /// The path, for the state dump and `last_hit.by`.
    path: String,
    /// Computed once from the node's rotation, like the character's.
    facing: f32,
    damage: f32,
    impact: f32,
    startup: u32,
    active: u32,
    /// `startup + active + recovery`, saturating.
    period: u32,
    reach: f32,
    /// Radians: the whole angle of the sector.
    arc: f32,
    /// Radians swept per active tick: `arc / active`.
    slice: f32,
    /// Meters above the node: the height band swept.
    low: f32,
    high: f32,
    /// The index in `Game::hurtboxes` of the same node, if it is one: a
    /// staggered or dead one stops swinging.
    hurtbox: Option<usize>,
    arm: Option<Arm>,
    /// Which tick of the round this is, from 1; 0 while staggered or dead.
    swing_tick: u32,
    /// This round has hit the character already.
    struck: bool,
}

/// The `arm` child of a swinger, as in the scene file: its pose on the
/// center line, and its color.
#[derive(Debug, Clone, Copy)]
struct Arm {
    id: NodeId,
    p0: Vec3,
    r0: Quat,
    color: Vec3,
}

/// A node with `enemy_speed`: it walks up to the character, turns to it
/// and does moves chosen at random by distance (`docs/architecture.md`
/// section 11, "敌人和 Boss"). Always a hurtbox with health and a lock-on
/// target too.
#[derive(Debug, Clone)]
struct Enemy {
    id: NodeId,
    /// The path, for the state dump and events.
    path: String,
    /// Its parameters in phase 1.
    base: EnemyParams,
    /// With `phase2_at`: its phase 2.
    phase2: Option<Phase2>,
    /// 1, or 2 from entering phase 2 until it dies.
    stage: u8,
    /// Its index in `Game::hurtboxes`.
    hurtbox: usize,
    /// Its hurtbox's cylinder, also what it collides with.
    radius: f32,
    height: f32,
    arm: Option<Arm>,
    state: EnemyCondition,
    /// The move being done, while `Attack`.
    current: Option<MoveKind>,
    /// Which tick of the move or roar, from 1; 0 without one.
    move_tick: u32,
    facing: f32,
    rest: u32,
    /// Ticks left of a chosen approach.
    chase: u32,
    rng: SplitMix64,
    /// The cut of the move (1 for the moves that have one) that has hit
    /// the character already; 0 for none.
    struck: u32,
}

/// A node with `bonfire_range`: the character sits down at it.
#[derive(Debug, Clone)]
struct Bonfire {
    id: NodeId,
    path: String,
    range: f32,
}

/// Where the character comes back after dying: its spawn point at load,
/// then where it last sat down at a bonfire.
#[derive(Debug, Clone, Copy)]
struct Respawn {
    position: Vec3,
    facing: f32,
    /// The bonfire sat at, as an index into `Game::bonfires`.
    bonfire: Option<usize>,
}

/// What a world reset gives back: the load-time state of the character's
/// health, the hurtboxes, swingers and enemies, and the enemies' node
/// positions and rotations.
#[derive(Debug, Clone)]
struct Initial {
    health: Health,
    hurtboxes: Vec<Hurtbox>,
    swingers: Vec<Swinger>,
    enemies: Vec<Enemy>,
    enemy_nodes: Vec<(Vec3, Quat)>,
}

/// An enemy's phase 2, from its props, worked out at load.
#[derive(Debug, Clone, Copy)]
struct Phase2 {
    /// The fraction of full health that enters it.
    at: f32,
    params: EnemyParams,
    roar_ticks: u32,
    blast_tick: u32,
    damage: f32,
    impact: f32,
    reach: f32,
}

impl Enemy {
    /// Its parameters in its stage.
    fn params(&self) -> &EnemyParams {
        match &self.phase2 {
            Some(p) if self.stage == 2 => &p.params,
            _ => &self.base,
        }
    }

    /// Where the move is on this tick; `None` outside a move.
    fn moment(&self) -> Option<(MoveKind, crate::enemy::Moment)> {
        let kind = self.current?;
        Some((kind, self.params().moment(kind, self.move_tick)))
    }

    /// While roaring: its phase 2, and the part of the roar it is in
    /// (startup before the blast, active on its tick, then recovery).
    fn roar(&self) -> Option<(Phase2, AttackPhase)> {
        if self.state != EnemyCondition::Roar {
            return None;
        }
        let p = self.phase2.expect("only an enemy with a phase 2 roars");
        let phase = match self.move_tick.cmp(&p.blast_tick) {
            std::cmp::Ordering::Less => AttackPhase::Startup,
            std::cmp::Ordering::Equal => AttackPhase::Active,
            std::cmp::Ordering::Greater => AttackPhase::Recovery,
        };
        Some((p, phase))
    }

    /// Where the move `kind`'s sector is centered, off the facing: behind
    /// for the spin, so that its first slice (at `-half`, `-π`) is in
    /// front; ahead for the others.
    fn center(kind: MoveKind) -> f32 {
        match kind {
            MoveKind::Spin => PI,
            MoveKind::Sweep | MoveKind::Cleave | MoveKind::Lunge | MoveKind::Combo => 0.0,
        }
    }

    /// `(lo, hi)` of the move `kind`'s active tick `k`, left to right when
    /// `reversed`.
    fn slice(&self, kind: MoveKind, k: u32, reversed: bool) -> (f32, f32) {
        let m = self.params().of(kind);
        let (lo, hi) = slice_of(m.arc, m.slice, k);
        if reversed { (-hi, -lo) } else { (lo, hi) }
    }
}

impl Swinger {
    /// The part of the round `swing_tick` is in; `None` at 0.
    fn phase(&self) -> Option<AttackPhase> {
        match self.swing_tick {
            0 => None,
            t if t <= self.startup => Some(AttackPhase::Startup),
            t if t - self.startup <= self.active => Some(AttackPhase::Active),
            _ => Some(AttackPhase::Recovery),
        }
    }

    /// `(lo, hi)` of active tick `k` (1-based): the part of the sector it
    /// sweeps, relative to `facing`, positive to the left.
    fn slice_of(&self, k: u32) -> (f32, f32) {
        slice_of(self.arc, self.slice, k)
    }
}

/// `(lo, hi)` of active tick `k` (1-based) of a sweep of `arc` in slices
/// of `slice`, right to left: relative to `facing`, positive to the left.
fn slice_of(arc: f32, slice: f32, k: u32) -> (f32, f32) {
    let half = arc * 0.5;
    (-half + (k - 1) as f32 * slice, -half + k as f32 * slice)
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

#[derive(Debug)]
pub struct Game {
    clock: SimClock,
    scene: Scene,
    player: NodeId,
    camera: NodeId,
    state: Motion,
    /// Every collider, in pre-order.
    boxes: Vec<BoxCollider>,
    /// The character's capsule.
    radius: f32,
    height: f32,
    /// Meters per tick at full input: `run_speed / 60.0`.
    step: f32,
    /// Radians per tick: `turn_speed.to_radians() / 60.0`.
    max_turn: f32,
    pitch_min: f32,
    pitch_max: f32,
    pivot_height: f32,
    /// The orbit distance: the farthest the camera gets from the pivot.
    distance: f32,
    /// Radius of the camera's collision sphere.
    collision_radius: f32,
    /// Meters per tick the camera moves back out: `return_speed / 60.0`.
    return_step: f32,
    /// Stamina, roll, sprint and backstep, in per-tick units.
    params: Params,
    /// Ticks the roll key has been held, capped at `tap_ticks + 1`.
    hold: u32,
    /// This hold's sprint has used up the stamina.
    spent: bool,
    /// Unit direction of the current roll or backstep.
    action_dir: Vec3,
    /// The action that ended last, and its `combo`; not in the state dump.
    finished: (Action, u32),
    /// Every lock-on target, in pre-order.
    targets: Vec<Target>,
    /// The index in `targets` of the locked-on target.
    lock: Option<usize>,
    /// While locked on: `look_x` summed up over consecutive ticks.
    flick: f32,
    /// Ticks left before the target can be switched again.
    switch_wait: u32,
    /// Every hurtbox, in pre-order, with its hits.
    hurtboxes: Vec<Hurtbox>,
    /// The indices in `hurtboxes` the running attack has already hit.
    struck: Vec<usize>,
    /// Every swinger, in pre-order.
    swingers: Vec<Swinger>,
    /// Every enemy, in pre-order.
    enemies: Vec<Enemy>,
    /// The character's own health, poise and death (without an hp bar).
    health: Health,
    /// Every node with a mesh in the character's subtree (the character
    /// included), in pre-order, with its color in the scene file: what
    /// flashes and darkens.
    player_meshes: Vec<(NodeId, Vec3)>,
    /// The character's direct child named `flask`, shown only while
    /// drinking.
    flask: Option<NodeId>,
    /// Every bonfire, in pre-order.
    bonfires: Vec<Bonfire>,
    /// The root node named `bloodstain`, with its position in the scene
    /// file: where it is drawn above the bloodstain.
    bloodstain_marker: Option<(NodeId, Vec3)>,
    /// Untouched by the world reset.
    bloodstain: Option<BloodstainState>,
    /// Where the character comes back after dying.
    respawn: Respawn,
    /// What a world reset gives back.
    initial: Initial,
    /// The camera's yaw and pitch at load, given back on a respawn.
    start_view: (f32, f32),
    /// The character node's path, for events.
    player_path: String,
    /// The events of the latest tick, in the order they happened.
    events: Vec<Event>,
}

/// This tick's input as step 4 uses it.
struct Intent<'a> {
    input: &'a ActionState,
    /// The roll key was tapped (step 0).
    tap: bool,
    /// Movement input, at most 1 long (step 2).
    v: Vec2,
    /// `v` relative to the camera (step 3).
    dir: Vec3,
}

impl Intent<'_> {
    /// This tick's presses of the action keys, heavy before light before
    /// the drink key before the roll key's tap: the order they outrank
    /// each other in.
    fn presses(&self) -> Vec<Buffered> {
        [
            (self.input.just_pressed(HEAVY), Buffered::Heavy),
            (self.input.just_pressed(ATTACK), Buffered::Attack),
            (self.input.just_pressed(DRINK), Buffered::Drink),
            (self.tap, Buffered::Roll),
        ]
        .into_iter()
        .filter_map(|(pressed, b)| pressed.then_some(b))
        .collect()
    }
}

impl Game {
    /// Parses scene source text and starts a game on it.
    pub fn from_source(src: &str) -> Result<Self, GameError> {
        Self::new(Scene::parse(src)?)
    }

    /// Takes over `scene`, which needs a root node with a character and a
    /// capsule mesh, and an active camera that is a root node with an orbit
    /// camera (its target is necessarily that character: a scene has at
    /// most one). The character must not be scaled, and colliders must not
    /// move with either. Nodes with a `lock_height` prop are lock-on
    /// targets, and nodes with `swing_damage` swingers: root nodes other
    /// than the camera. A direct child of the character named `flask` is its
    /// flask, which needs a mesh. Places the camera for tick 0, already pulled in
    /// front of anything in the way; the character is not pushed out of
    /// colliders until the first tick.
    pub fn new(scene: Scene) -> Result<Self, GameError> {
        let player = scene.character().ok_or(GameError::NoCharacter)?;
        let camera = scene.camera();
        let Some(orbit) = scene.node(camera).orbit_camera else {
            return Err(GameError::NoOrbitCamera(scene.path(camera)));
        };
        for id in [player, camera] {
            if scene.node(id).parent.is_some() {
                return Err(GameError::NotRoot(scene.path(id)));
            }
        }
        let character = scene
            .node(player)
            .character
            .expect("Scene::character only returns nodes with a character");
        let Some(MeshKind::Capsule { radius, height }) = scene.node(player).mesh.map(|m| m.kind)
        else {
            return Err(GameError::NotACapsule(scene.path(player)));
        };
        if scene.transform(player).scale != Vec3::ONE {
            return Err(GameError::ScaledCharacter(scene.path(player)));
        }
        if character.run_speed / 60.0 >= radius {
            return Err(GameError::TooFast {
                run_speed: character.run_speed,
                radius,
            });
        }
        for &id in scene.colliders() {
            let mut node = Some(id);
            while let Some(n) = node {
                if n == player || n == camera {
                    return Err(GameError::MovingCollider(scene.path(id)));
                }
                node = scene.node(n).parent;
            }
        }
        let params = Params::new(&scene, player, radius)?;
        let Roles {
            targets,
            hurtboxes,
            swingers,
            enemies,
            bonfires,
            bloodstain_marker,
        } = roles(&scene, player, camera)?;
        let health = Health::new(
            params.hp,
            params.poise,
            [
                params.stagger_ticks,
                params.poise_reset_ticks,
                params.respawn_ticks,
            ],
            None,
        );
        let player_meshes = scene
            .ids()
            .filter(|&n| is_in_subtree(&scene, n, player))
            .filter_map(|n| scene.node(n).mesh.map(|m| (n, m.color)))
            .collect();
        let flask = scene
            .ids()
            .find(|&n| scene.node(n).parent == Some(player) && scene.node(n).name == "flask");
        if let Some(f) = flask
            && scene.node(f).mesh.is_none()
        {
            return Err(GameError::FlaskWithoutMesh(scene.path(player)));
        }
        let boxes = scene
            .colliders()
            .iter()
            .map(|&id| BoxCollider::new(&scene, id))
            .collect();

        let transform = scene.transform(player);
        let f = transform.rotation * Vec3::NEG_Z;
        let state = Motion {
            player: PlayerState {
                position: transform.position,
                facing: positive_zero(wrap(libm::atan2f(-f.x, -f.z))),
                stamina: params.stamina_max,
                stamina_wait: 0,
                action: Action::Idle,
                action_tick: 0,
                invulnerable: false,
                attack_phase: None,
                combo: 0,
                buffered: None,
                hits: 0,
                last_hit: None,
                health: health.now,
                flask: params.flask_charges,
                souls: 0,
            },
            camera: CameraState {
                yaw: wrap(orbit.yaw_degrees.to_radians()),
                pitch: orbit.pitch_degrees.to_radians(),
                // Replaced by the first `place_camera` below.
                distance: orbit.distance,
            },
        };
        let respawn = Respawn {
            position: state.player.position,
            facing: state.player.facing,
            bonfire: None,
        };
        let initial = Initial {
            health,
            hurtboxes: hurtboxes.clone(),
            swingers: swingers.clone(),
            enemies: enemies.clone(),
            enemy_nodes: enemies
                .iter()
                .map(|e| {
                    let t = scene.transform(e.id);
                    (t.position, t.rotation)
                })
                .collect(),
        };
        let start_view = (state.camera.yaw, state.camera.pitch);
        let player_path = scene.path(player);
        let mut game = Self {
            clock: SimClock::new(),
            scene,
            player,
            camera,
            state,
            boxes,
            radius,
            height,
            step: character.run_speed / 60.0,
            max_turn: character.turn_speed_degrees.to_radians() / 60.0,
            pitch_min: orbit.pitch_min_degrees.to_radians(),
            pitch_max: orbit.pitch_max_degrees.to_radians(),
            pivot_height: orbit.pivot_height,
            distance: orbit.distance,
            collision_radius: orbit.collision_radius,
            return_step: orbit.return_speed / 60.0,
            params,
            hold: 0,
            spent: false,
            action_dir: Vec3::ZERO,
            finished: (Action::Idle, 0),
            targets,
            lock: None,
            flick: 0.0,
            switch_wait: 0,
            hurtboxes,
            struck: Vec::new(),
            swingers,
            enemies,
            health,
            player_meshes,
            flask,
            bonfires,
            bloodstain_marker,
            bloodstain: None,
            respawn,
            initial,
            start_view,
            player_path,
            events: Vec::new(),
        };
        game.place_camera(true);
        if let Some(f) = flask {
            // Hidden until the first drink.
            game.scene.set_visible(f, false);
        }
        if let Some((m, _)) = bloodstain_marker {
            // Hidden until the first bloodstain.
            game.scene.set_visible(m, false);
        }
        Ok(game)
    }

    /// Runs one tick with this tick's input.
    pub fn tick(&mut self, input: &ActionState) {
        self.clock.step();
        self.events.clear();
        let params = self.params;

        // 0. The roll key: a tap fires on release.
        let tap = if input.pressed(ROLL) {
            self.hold = u32::min(
                self.hold.saturating_add(1),
                params.tap_ticks.saturating_add(1),
            );
            false
        } else {
            // `hold` is how long it was held before this release.
            let tap = input.just_released(ROLL) && self.hold <= params.tap_ticks;
            self.hold = 0;
            self.spent = false;
            tap
        };

        // 0b. Staggers and deaths run out; added-up impacts are forgotten.
        for h in &mut self.hurtboxes {
            if let Some(health) = &mut h.health {
                let was_dead = health.is_dead();
                health.count_down();
                if was_dead && !health.is_dead() {
                    self.events.push(Event::Revive {
                        node: h.path.clone(),
                    });
                }
            }
        }
        // The character's too; coming back from the dead is the respawn: at
        // the respawn point with full stamina, the camera turned as at
        // load, the world reset.
        let was_dead = self.health.is_dead();
        self.health.count_down();
        if was_dead && !self.health.is_dead() {
            self.events.push(Event::Respawn {
                position: self.respawn.position,
            });
            self.reset_world();
            let player = &mut self.state.player;
            (player.position, player.facing) = (self.respawn.position, self.respawn.facing);
            player.stamina = params.stamina_max;
            player.stamina_wait = 0;
            player.flask = params.flask_charges;
            (self.state.camera.yaw, self.state.camera.pitch) = self.start_view;
            self.flick = 0.0;
            self.switch_wait = 0;
        }

        let mut player = self.state.player.clone();
        let mut camera = self.state.camera;

        // 1a. The lock key (not while dead), and letting go of a target too
        // far away.
        let lock = params.lock;
        let sitting = player.action == Action::Sit;
        if input.just_pressed(LOCK) && sitting {
            self.drop_inputs(&[Input::Lock], DropReason::Sitting);
        } else if input.just_pressed(LOCK) && self.health.is_dead() {
            self.drop_inputs(&[Input::Lock], DropReason::Dead);
        }
        if input.just_pressed(LOCK) && !self.health.is_dead() && !sitting {
            match self.lock {
                None => match self.acquire(player.position, camera.yaw) {
                    Some(t) => {
                        self.lock = Some(t);
                        self.lock_event(Some(t), LockReason::Pressed);
                    }
                    None => self.drop_inputs(&[Input::Lock], DropReason::NoTarget),
                },
                Some(_) => {
                    self.lock = None;
                    self.lock_event(None, LockReason::Pressed);
                }
            }
            self.flick = 0.0;
            self.switch_wait = 0;
        } else if let Some(t) = self.lock
            && hdist(player.position, self.target_position(t)) > lock.break_range
        {
            self.lock = None;
            self.lock_event(None, LockReason::OutOfRange);
        }

        if let Some(mut t) = self.lock {
            // 1b. Flicking sideways switches targets; look_y does nothing.
            let look_x = input.axis(LOOK_X);
            if self.switch_wait > 0 {
                self.switch_wait -= 1;
                self.flick = 0.0;
            } else if look_x == 0.0 {
                self.flick = 0.0;
            } else {
                self.flick += look_x;
                if self.flick.abs() >= lock.switch_threshold {
                    if let Some(next) = self.switch_target(player.position, t, self.flick > 0.0) {
                        t = next;
                        self.lock = Some(t);
                        self.lock_event(Some(t), LockReason::Switched);
                    }
                    self.flick = 0.0;
                    self.switch_wait = lock.switch_cooldown_ticks;
                }
            }
            // 1c. The camera turns to look at the lock point.
            let pivot = player.position + Vec3::new(0.0, self.pivot_height, 0.0);
            let p = self.target_position(t) + Vec3::new(0.0, self.targets[t].height, 0.0);
            let dh = hdist(pivot, p);
            if dh > 0.0 {
                let yaw_t = bearing(pivot, p);
                let pitch_t = (libm::atan2f(p.y - pivot.y, dh) - lock.pitch_down)
                    .clamp(self.pitch_min, self.pitch_max);
                camera.yaw = wrap(
                    camera.yaw + wrap(yaw_t - camera.yaw).clamp(-lock.cam_step, lock.cam_step),
                );
                camera.pitch += (pitch_t - camera.pitch).clamp(-lock.cam_step, lock.cam_step);
            }
        } else {
            // 1. Camera angles.
            camera.yaw = wrap(camera.yaw - input.axis(LOOK_X));
            camera.pitch =
                (camera.pitch + input.axis(LOOK_Y)).clamp(self.pitch_min, self.pitch_max);
        }

        // 2. Movement input, at most 1 long.
        let mut v = Vec2::new(input.axis(MOVE_X), input.axis(MOVE_Y));
        if v.length_squared() > 1.0 {
            v = v / v.length();
        }

        // 3. Direction relative to the camera (yaw 0 looks along -Z).
        let (s, c) = (libm::sinf(camera.yaw), libm::cosf(camera.yaw));
        let forward = Vec3::new(-s, 0.0, -c);
        let right = Vec3::new(c, 0.0, -s);
        let dir = right * v.x + forward * v.y;

        // 4. Actions and movement; each move ends by pushing the capsule
        // out of the colliders. `busy`: stamina used or an action running.
        let intent = Intent { input, tap, v, dir };
        let presses = intent.presses();
        // 4a. The interact key first: sitting down at a bonfire, or
        // standing up; either keeps the character still this tick.
        let still = input.just_pressed(INTERACT) && self.interact(&mut player);
        let mut busy = true;
        if self.health.now.state != Condition::Ok {
            // Staggered or dead: no moving, turning or actions, and presses
            // are dropped; stamina comes back.
            busy = false;
            let reason = if self.health.is_dead() {
                DropReason::Dead
            } else {
                DropReason::Staggered
            };
            self.drop_presses(&presses, reason);
        } else if still || player.action == Action::Sit {
            // Sitting (or sitting down or standing up this tick): no moving
            // or turning, and presses are dropped; stamina comes back. A
            // push stands it up, not moving yet.
            busy = false;
            self.drop_presses(&presses, DropReason::Sitting);
            if !still && v.length_squared() > 0.0 {
                self.stand(&mut player);
            }
        } else if matches!(player.action, Action::Roll | Action::Backstep) {
            self.drop_presses(&presses, DropReason::Rolling);
            let m = params.movement(player.action);
            player.action_tick += 1;
            player.position = resolve(
                &self.boxes,
                self.radius,
                self.height,
                player.position + self.action_dir * m.step,
                &self.enemy_circles(),
            );
            // The last tick still moves.
            if player.action_tick == m.ticks {
                self.end_action(&mut player, 0);
            }
        } else if matches!(player.action, Action::Attack | Action::Heavy) {
            player.action_tick += 1;
            self.attack_tick(&mut player, &intent);
        } else if player.action == Action::Drink {
            // Drinking costs no stamina, so it comes back.
            busy = false;
            self.drop_presses(&presses, DropReason::Drinking);
            player.action_tick += 1;
            self.drink_tick(&mut player, &intent);
        } else if let Some((next, from_buffer)) = self.next_action(&mut player, &presses) {
            if player.action == Action::Sprint {
                self.events.push(Event::ActionEnd {
                    action: Action::Sprint,
                    reason: EndReason::Finished,
                });
            }
            match next {
                Buffered::Roll => self.start_roll(&mut player, from_buffer, &intent),
                Buffered::Attack => {
                    self.start_attack(&mut player, Action::Attack, from_buffer, &intent);
                }
                Buffered::Heavy => {
                    self.start_attack(&mut player, Action::Heavy, from_buffer, &intent);
                }
                Buffered::Drink => {
                    busy = false;
                    self.start_drink(&mut player, from_buffer, &intent);
                }
            }
            // Pressed on the same tick, but outranked by what started.
            self.drop_presses(outranked(&presses, from_buffer), DropReason::Outranked);
        } else {
            let sprinting = input.pressed(ROLL)
                && self.hold > params.tap_ticks
                && !self.spent
                && player.stamina > 0.0
                && v.length_squared() > 0.0;
            // Without the roll key this is exactly G1's step 4.
            let step = if sprinting {
                params.sprint_step
            } else {
                self.step
            };
            player.position = resolve(
                &self.boxes,
                self.radius,
                self.height,
                player.position + dir * step,
                &self.enemy_circles(),
            );
            let was_sprinting = player.action == Action::Sprint;
            if sprinting {
                if !was_sprinting {
                    self.events.push(Event::ActionStart {
                        action: Action::Sprint,
                        combo: 0,
                        buffered: false,
                    });
                }
                player.action = Action::Sprint;
                player.stamina = f32::max(player.stamina - params.sprint_drain, 0.0);
                player.stamina_wait = params.delay_ticks;
                if player.stamina == 0.0 {
                    self.spent = true;
                }
            } else {
                if was_sprinting {
                    self.events.push(Event::ActionEnd {
                        action: Action::Sprint,
                        reason: EndReason::Finished,
                    });
                }
                player.action = Action::Idle;
                busy = false;
            }

            self.turn_player(&mut player, &intent);
        }

        // 5b. Stamina comes back once `stamina_wait` has run out.
        if !busy {
            if player.stamina_wait > 0 {
                player.stamina_wait -= 1;
            } else {
                player.stamina = f32::min(player.stamina + params.regen_step, params.stamina_max);
            }
        }
        player.invulnerable = match player.action {
            Action::Roll | Action::Backstep => {
                let m = params.movement(player.action);
                (m.invuln_from..=m.invuln_to).contains(&player.action_tick)
            }
            Action::Idle
            | Action::Sprint
            | Action::Attack
            | Action::Heavy
            | Action::Sit
            | Action::Drink => false,
        };

        // 5c. The enemies, then the swingers, act, and may hit the character.
        self.run_enemies(&mut player);
        self.swing(&mut player);
        player.health = self.health.now;
        self.state = Motion { player, camera };

        // 6. Write the nodes.
        self.show_player();
        self.show_bloodstain();
        self.place_camera(false);
        // An enemy stands turned to its facing.
        for e in &self.enemies {
            self.hurtboxes[e.hurtbox].base = Quat::from_rotation_y(e.facing);
        }
        self.show_hurtboxes();
        self.show_arms();
    }

    /// Step 5c: every swinger (pre-order) moves on in its round, or stops
    /// while staggered or dead, to start a new round once it stands again.
    /// An active tick sweeps its slice of the sector and hits the character
    /// it touches, at most once a round, unless the character is dead or
    /// invulnerable.
    fn swing(&mut self, player: &mut PlayerState) {
        for i in 0..self.swingers.len() {
            let s = &mut self.swingers[i];
            if s.hurtbox
                .and_then(|h| self.hurtboxes[h].health)
                .is_some_and(|health| health.now.state != Condition::Ok)
            {
                s.swing_tick = 0;
                continue;
            }
            s.swing_tick = s.swing_tick % s.period + 1;
            if s.swing_tick == 1 {
                s.struck = false;
            }
            if s.phase() != Some(AttackPhase::Active) || s.struck || self.health.is_dead() {
                continue;
            }
            let (lo, hi) = s.slice_of(s.swing_tick - s.startup);
            // Swingers are root nodes: the local position is the world one.
            let at = self.scene.transform(s.id).position;
            let sweep = Sweep {
                p: at,
                facing: s.facing,
                reach: s.reach,
                lo,
                hi,
                low: s.low,
                high: s.high,
            };
            if !sweep.touches(player.position, self.radius, self.height) {
                continue;
            }
            // Invulnerable: no hit, and the round may still hit later.
            if player.invulnerable {
                self.events.push(Event::Dodged {
                    by: s.path.clone(),
                    action: player.action,
                    action_tick: player.action_tick,
                });
                continue;
            }
            s.struck = true;
            let (by, damage, impact) = (s.path.clone(), s.damage, s.impact);
            self.player_hit(player, at, by, damage, impact, HitAttack::Swing);
        }
    }

    /// Step 5c for the enemies (pre-order), before the swingers: each one
    /// follows its hurtbox into a stagger or death, breaking off its move,
    /// else goes on with its move or rest, or chooses a move, or turns to
    /// the character and walks up to it.
    fn run_enemies(&mut self, player: &mut PlayerState) {
        for i in 0..self.enemies.len() {
            let h = self.enemies[i].hurtbox;
            let condition = self.hurtboxes[h]
                .health
                .expect("an enemy has hp (checked at load)")
                .now
                .state;
            let player_dead = self.health.is_dead();
            let at = self.scene.transform(self.enemies[i].id).position;
            let d = hdist(at, player.position);
            let e = &mut self.enemies[i];
            if condition != Condition::Ok {
                e.state = match condition {
                    Condition::Dead => EnemyCondition::Dead,
                    Condition::Stagger | Condition::Ok => EnemyCondition::Stagger,
                };
                e.current = None;
                e.move_tick = 0;
                e.rest = 0;
                e.chase = 0;
                e.struck = 0;
                continue;
            }
            if e.state == EnemyCondition::Dead {
                // Revived: back in phase 1.
                e.stage = 1;
            }
            if matches!(e.state, EnemyCondition::Stagger | EnemyCondition::Dead) {
                e.state = EnemyCondition::Idle;
            }
            if (player_dead || d > e.params().aggro_range)
                && !matches!(e.state, EnemyCondition::Attack | EnemyCondition::Roar)
            {
                e.state = EnemyCondition::Idle;
                e.rest = 0;
                e.chase = 0;
                continue;
            }
            match e.state {
                EnemyCondition::Attack => {
                    e.move_tick += 1;
                    self.enemy_move_tick(i, player);
                }
                EnemyCondition::Rest => {
                    if d > 0.0 {
                        e.facing = turn(e.facing, bearing(at, player.position), e.params().turn);
                    }
                    e.rest = e.rest.saturating_sub(1);
                    if e.rest == 0 {
                        e.state = EnemyCondition::Idle;
                    }
                }
                EnemyCondition::Roar => self.enemy_roar(i, player),
                EnemyCondition::Chase if e.chase > 0 => self.enemy_approach(i, player),
                EnemyCondition::Idle | EnemyCondition::Chase => self.enemy_choose(i, player),
                EnemyCondition::Stagger | EnemyCondition::Dead => {
                    unreachable!("a stagger or death was left above")
                }
            }
        }
    }

    /// An idle enemy `i`, or one walking up with nothing to choose: when it
    /// faces the character closely enough, chooses at random among the
    /// moves in range and the approach, and starts the choice; otherwise
    /// turns to the character, walking up while there is nothing to choose.
    fn enemy_choose(&mut self, i: usize, player: &mut PlayerState) {
        let at = self.scene.transform(self.enemies[i].id).position;
        let d = hdist(at, player.position);
        let e = &mut self.enemies[i];
        let p = *e.params();
        let mut weighted: Vec<(EnemyChoice, u32)> = MoveKind::ALL
            .into_iter()
            .filter_map(|kind| {
                let m = p.of(kind);
                (p.has(kind) && m.weight > 0 && m.min_range <= d && d <= m.max_range)
                    .then_some((kind.into(), m.weight))
            })
            .collect();
        let approach = p.approach_weight(d);
        if approach >= 1 {
            weighted.push((EnemyChoice::Approach, approach));
        }
        let off = wrap(bearing(at, player.position) - e.facing).abs();
        if !weighted.is_empty() && off <= p.face_angle {
            let total: u64 = weighted.iter().map(|&(_, w)| u64::from(w)).sum();
            let r = e.rng.next() % total;
            let mut sum = 0;
            let chosen = weighted
                .iter()
                .find(|&&(_, w)| {
                    sum += u64::from(w);
                    sum > r
                })
                .expect("r is less than the total of the weights")
                .0;
            self.events.push(Event::EnemyMove {
                by: e.path.clone(),
                r#move: chosen,
                distance: d,
                candidates: weighted.iter().map(|&(c, _)| c).collect(),
            });
            let kind = match chosen {
                EnemyChoice::Sweep => MoveKind::Sweep,
                EnemyChoice::Cleave => MoveKind::Cleave,
                EnemyChoice::Lunge => MoveKind::Lunge,
                EnemyChoice::Combo => MoveKind::Combo,
                EnemyChoice::Spin => MoveKind::Spin,
                EnemyChoice::Approach => {
                    e.state = EnemyCondition::Chase;
                    e.chase = p.approach_ticks;
                    self.enemy_approach(i, player);
                    return;
                }
            };
            e.state = EnemyCondition::Attack;
            e.current = Some(kind);
            e.move_tick = 1;
            e.struck = 0;
            self.enemy_move_tick(i, player);
            return;
        }
        if d > 0.0 {
            e.facing = turn(e.facing, bearing(at, player.position), p.turn);
        }
        if weighted.is_empty() {
            e.state = EnemyCondition::Chase;
            self.enemy_walk(i, player.position);
        } else {
            e.state = EnemyCondition::Idle;
        }
    }

    /// A tick of enemy `i`'s chosen approach: turns to the character and
    /// walks up to it; idle again once the approach's ticks are used up or
    /// it has come within `approach_min_range`.
    fn enemy_approach(&mut self, i: usize, player: &PlayerState) {
        let at = self.scene.transform(self.enemies[i].id).position;
        let e = &mut self.enemies[i];
        if hdist(at, player.position) > 0.0 {
            e.facing = turn(e.facing, bearing(at, player.position), e.params().turn);
        }
        let moved = self.enemy_walk(i, player.position);
        let e = &mut self.enemies[i];
        e.chase -= 1;
        if e.chase == 0 || hdist(moved, player.position) <= e.params().approach_min {
            e.chase = 0;
            e.state = EnemyCondition::Idle;
        }
    }

    /// Enemy `i` walks a step straight at the character at `to` (none on
    /// its spot), with collisions; where it ends up.
    fn enemy_walk(&mut self, i: usize, to: Vec3) -> Vec3 {
        let id = self.enemies[i].id;
        let at = self.scene.transform(id).position;
        let d = hdist(at, to);
        if d == 0.0 {
            return at;
        }
        let dir = Vec3::new(to.x - at.x, 0.0, to.z - at.z) / d;
        let moved = self.enemy_resolve(i, at + dir * self.enemies[i].params().step, to);
        self.scene.transform_mut(id).position = moved;
        moved
    }

    /// Tick `move_tick` of enemy `i`'s move: turning to the character
    /// during startup; while active, the lunge's step forward, then the
    /// hit check of this tick's slice; after the last tick, the rest.
    fn enemy_move_tick(&mut self, i: usize, player: &mut PlayerState) {
        let e = &self.enemies[i];
        let (kind, moment) = e.moment().expect("only called during a move");
        let m: MoveParams = *e.params().of(kind);
        let mut at = self.scene.transform(e.id).position;
        match moment.phase {
            AttackPhase::Startup => {
                if hdist(at, player.position) > 0.0 {
                    let e = &mut self.enemies[i];
                    e.facing = turn(e.facing, bearing(at, player.position), m.track);
                }
            }
            AttackPhase::Active => {
                if kind == MoveKind::Lunge {
                    let f = e.facing;
                    let forward = Vec3::new(-libm::sinf(f), 0.0, -libm::cosf(f));
                    at = self.enemy_resolve(
                        i,
                        at + forward * e.params().lunge_step,
                        player.position,
                    );
                    self.scene.transform_mut(e.id).position = at;
                }
                let e = &self.enemies[i];
                let k = moment.active.expect("an active tick has its number");
                let (lo, hi) = e.slice(kind, k, moment.reversed);
                let cut = moment.hit.max(1);
                let sweep = Sweep {
                    p: at,
                    facing: e.facing + Enemy::center(kind),
                    reach: m.reach,
                    lo,
                    hi,
                    low: e.params().low,
                    high: e.params().high,
                };
                if e.struck != cut
                    && !self.health.is_dead()
                    && sweep.touches(player.position, self.radius, self.height)
                {
                    let by = e.path.clone();
                    if player.invulnerable {
                        self.events.push(Event::Dodged {
                            by,
                            action: player.action,
                            action_tick: player.action_tick,
                        });
                    } else {
                        self.enemies[i].struck = cut;
                        self.player_hit(player, at, by, m.damage, m.impact, kind.into());
                    }
                }
            }
            AttackPhase::Recovery => {}
        }
        if moment.last {
            let e = &mut self.enemies[i];
            e.current = None;
            e.move_tick = 0;
            e.struck = 0;
            if e.params().rest_ticks > 0 {
                e.state = EnemyCondition::Rest;
                e.rest = e.params().rest_ticks;
            } else {
                e.state = EnemyCondition::Idle;
            }
        }
    }

    /// A tick of enemy `i`'s roar on entering phase 2: turning to the
    /// character, the blast all around on its tick, then idle after the
    /// last one.
    fn enemy_roar(&mut self, i: usize, player: &mut PlayerState) {
        let at = self.scene.transform(self.enemies[i].id).position;
        let e = &mut self.enemies[i];
        e.move_tick += 1;
        if hdist(at, player.position) > 0.0 {
            e.facing = turn(e.facing, bearing(at, player.position), e.params().turn);
        }
        let (p, phase) = e.roar().expect("roaring");
        if phase == AttackPhase::Active {
            let sweep = Sweep {
                p: at,
                facing: e.facing,
                reach: p.reach,
                lo: -PI,
                hi: PI,
                low: e.params().low,
                high: e.params().high,
            };
            if !self.health.is_dead() && sweep.touches(player.position, self.radius, self.height) {
                let by = e.path.clone();
                if player.invulnerable {
                    self.events.push(Event::Dodged {
                        by,
                        action: player.action,
                        action_tick: player.action_tick,
                    });
                } else {
                    self.player_hit(player, at, by, p.damage, p.impact, HitAttack::Roar);
                }
            }
        }
        let e = &mut self.enemies[i];
        if e.move_tick == p.roar_ticks {
            e.state = EnemyCondition::Idle;
            e.move_tick = 0;
        }
    }

    /// Enemy `i`'s feet moved to `p`, pushed out of the colliders (its
    /// hurtbox as the capsule) and of the character's circle.
    fn enemy_resolve(&self, i: usize, p: Vec3, player: Vec3) -> Vec3 {
        let e = &self.enemies[i];
        let circle = (Vec2::new(player.x, player.z), self.radius + e.radius);
        resolve(&self.boxes, e.radius, e.height, p, &[circle])
    }

    /// The circles the character is pushed out of: every living enemy's,
    /// grown by the character's radius.
    fn enemy_circles(&self) -> Vec<(Vec2, f32)> {
        self.enemies
            .iter()
            .filter(|e| {
                !self.hurtboxes[e.hurtbox]
                    .health
                    .is_some_and(|health| health.is_dead())
            })
            .map(|e| {
                let at = self.scene.transform(e.id).position;
                (Vec2::new(at.x, at.z), self.radius + e.radius)
            })
            .collect()
    }

    /// Step 4a, the interact key: stands up while sitting; otherwise sits
    /// down at the first bonfire (pre-order) in reach, unless dead,
    /// staggered, busy, out of reach of every bonfire or near a living
    /// enemy, which drops the press. Whether it sat down or stood up.
    fn interact(&mut self, player: &mut PlayerState) -> bool {
        if player.action == Action::Sit {
            self.stand(player);
            return true;
        }
        let bonfire = self
            .bonfires
            .iter()
            .position(|b| hdist(self.scene.transform(b.id).position, player.position) <= b.range);
        let enemy_near = self.enemies.iter().any(|e| {
            !self.hurtboxes[e.hurtbox]
                .health
                .is_some_and(|health| health.is_dead())
                && hdist(self.scene.transform(e.id).position, player.position)
                    <= e.params().aggro_range
        });
        let reason = if self.health.is_dead() {
            DropReason::Dead
        } else if self.health.now.state == Condition::Stagger {
            DropReason::Staggered
        } else if player.action == Action::Drink {
            DropReason::Drinking
        } else if !matches!(player.action, Action::Idle | Action::Sprint) {
            DropReason::Busy
        } else if let Some(stain) = self.bloodstain
            && hdist(stain.position, player.position) <= self.params.bloodstain_range
        {
            // At once, whatever the character is doing.
            self.bloodstain = None;
            player.souls = player.souls.saturating_add(stain.souls);
            self.events.push(Event::SoulsRecovered {
                amount: stain.souls,
                total: player.souls,
            });
            return false;
        } else if let Some(b) = bonfire
            && !enemy_near
        {
            self.sit(player, b);
            return true;
        } else if bonfire.is_none() {
            DropReason::NoBonfire
        } else {
            DropReason::EnemyNear
        };
        self.drop_inputs(&[Input::Interact], reason);
        false
    }

    /// Sits down at bonfire `b`: the sprint stops, full health and stamina,
    /// a full flask, the respawn point where the character stands, the
    /// lock let go, the world reset, then the `rest` event.
    fn sit(&mut self, player: &mut PlayerState, b: usize) {
        if player.action == Action::Sprint {
            self.events.push(Event::ActionEnd {
                action: Action::Sprint,
                reason: EndReason::Finished,
            });
        }
        player.action = Action::Sit;
        player.action_tick = 0;
        self.health = self.initial.health;
        player.flask = self.params.flask_charges;
        player.stamina = self.params.stamina_max;
        player.stamina_wait = 0;
        self.respawn = Respawn {
            position: player.position,
            facing: player.facing,
            bonfire: Some(b),
        };
        if self.lock.is_some() {
            self.lock = None;
            self.lock_event(None, LockReason::Rested);
        }
        self.reset_world();
        self.events.push(Event::Rest {
            bonfire: self.bonfires[b].path.clone(),
        });
    }

    /// Stands up from a bonfire.
    fn stand(&mut self, player: &mut PlayerState) {
        player.action = Action::Idle;
        player.action_tick = 0;
        self.events.push(Event::Stand);
    }

    /// The world reset of sitting down and of the respawn: every hurtbox,
    /// swinger and enemy back as at load, but the hurtboxes' `hits` and
    /// `last_hit`, and the enemies' nodes where they stood at load.
    fn reset_world(&mut self) {
        for (h, init) in self.hurtboxes.iter_mut().zip(&self.initial.hurtboxes) {
            let (hits, last_hit) = (h.hits, h.last_hit);
            *h = init.clone();
            h.hits = hits;
            h.last_hit = last_hit;
        }
        self.swingers.clone_from(&self.initial.swingers);
        self.enemies.clone_from(&self.initial.enemies);
        for (e, &(position, rotation)) in self.enemies.iter().zip(&self.initial.enemy_nodes) {
            let t = self.scene.transform_mut(e.id);
            t.position = position;
            t.rotation = rotation;
        }
    }

    /// `player_hit` of step 5c: the swinger `by`, standing at `from`, hits
    /// the character for `damage` and `impact`. A stagger or death breaks
    /// off whatever it was doing (no combo carries over); a death lets go
    /// of the lock, then drops the souls.
    fn player_hit(
        &mut self,
        player: &mut PlayerState,
        from: Vec3,
        by: String,
        damage: f32,
        impact: f32,
        attack: HitAttack,
    ) {
        player.hits += 1;
        player.last_hit = Some(PlayerHit {
            tick: self.clock.tick(),
            by: by.clone(),
        });
        // Away from the swinger; -Z when on one spot, like a hurtbox.
        let fall = if hdist(from, player.position) > 0.0 {
            bearing(from, player.position)
        } else {
            0.0
        };
        let outcome = self.health.take(damage, impact, fall);
        self.events.push(Event::Hit {
            by,
            target: self.player_path.clone(),
            attack,
            combo: 0,
            damage: Some(damage),
            hp: Some(self.health.now.hp),
            result: outcome,
        });
        if self.health.now.state != Condition::Ok {
            if player.action != Action::Idle {
                self.events.push(Event::ActionEnd {
                    action: player.action,
                    reason: EndReason::Interrupted,
                });
            }
            player.action = Action::Idle;
            player.action_tick = 0;
            player.attack_phase = None;
            player.combo = 0;
            player.buffered = None;
            player.invulnerable = false;
            self.finished = (Action::Idle, 0);
        }
        if outcome == Outcome::Death && self.lock.is_some() {
            self.lock = None;
            self.lock_event(None, LockReason::Died);
        }
        if outcome == Outcome::Death {
            self.drop_souls(player);
        }
    }

    /// On the character's death: the old bloodstain goes, and the souls it
    /// carries drop into a new one where it stands.
    fn drop_souls(&mut self, player: &mut PlayerState) {
        let lost = self.bloodstain.take().map_or(0, |stain| stain.souls);
        let amount = std::mem::take(&mut player.souls);
        if amount > 0 {
            self.bloodstain = Some(BloodstainState {
                position: player.position,
                souls: amount,
            });
        }
        if amount > 0 || lost > 0 {
            self.events.push(Event::SoulsDropped {
                position: player.position,
                amount,
                lost,
            });
        }
    }

    /// Step 6 for the bloodstain marker: above the bloodstain by its scene
    /// file position, hidden without one.
    fn show_bloodstain(&mut self) {
        let Some((id, offset)) = self.bloodstain_marker else {
            return;
        };
        if let Some(stain) = self.bloodstain {
            self.scene.transform_mut(id).position = stain.position + offset;
        }
        self.scene.set_visible(id, self.bloodstain.is_some());
    }

    /// Step 6 for the character node: its position, its pose (turned to
    /// `facing`, tipped over away from the swinger when staggered or dead)
    /// and the colors of its meshes, like a hurtbox's.
    fn show_player(&mut self) {
        let tick = self.clock.tick();
        let player = &self.state.player;
        let state = self.health.now.state;
        let transform = self.scene.transform_mut(self.player);
        transform.position = if player.action == Action::Sit {
            player.position - Vec3::new(0.0, SIT_DROP, 0.0)
        } else {
            player.position
        };
        transform.rotation = pose(
            state,
            self.health.fall,
            Quat::from_rotation_y(player.facing),
        );
        let flashing = player
            .last_hit
            .as_ref()
            .is_some_and(|hit| tick - hit.tick < FLASH_TICKS);
        for &(id, color) in &self.player_meshes {
            self.scene
                .set_mesh_color(id, shown(color, state == Condition::Dead, flashing))
                .expect("meshes were picked for having a mesh, and every color is valid");
        }
        if let Some(f) = self.flask {
            let drinking = self.state.player.action == Action::Drink;
            self.scene.set_visible(f, drinking);
        }
    }

    /// After the hurtboxes, every swinger's arm shows its round: at the
    /// sector's right end during startup, at the end of this tick's slice
    /// while active, otherwise on the center line as in the scene file.
    /// It darkens and flashes with its swinger, and otherwise shows
    /// [`WARN_COLOR`] during startup.
    fn show_arms(&mut self) {
        let tick = self.clock.tick();
        for s in &self.swingers {
            let Some(arm) = s.arm else {
                continue;
            };
            let phase = s.phase();
            let angle = match phase {
                Some(AttackPhase::Startup) => Some(-(s.arc * 0.5)),
                Some(AttackPhase::Active) => Some(s.slice_of(s.swing_tick - s.startup).1),
                Some(AttackPhase::Recovery) | None => None,
            };
            let t = self.scene.transform_mut(arm.id);
            match angle {
                // Exactly as in the scene file.
                None => {
                    t.position = arm.p0;
                    t.rotation = arm.r0;
                }
                Some(angle) => {
                    let q = Quat::from_rotation_y(angle);
                    t.position = q * arm.p0;
                    t.rotation = q * arm.r0;
                }
            }
            let hurtbox = s.hurtbox.map(|h| &self.hurtboxes[h]);
            let dead = hurtbox
                .and_then(|h| h.health)
                .is_some_and(|health| health.is_dead());
            let flashing = hurtbox
                .and_then(|h| h.last_hit)
                .is_some_and(|hit| tick - hit.tick < FLASH_TICKS);
            let color = if !dead && !flashing && phase == Some(AttackPhase::Startup) {
                WARN_COLOR
            } else {
                shown(arm.color, dead, flashing)
            };
            self.scene
                .set_mesh_color(arm.id, color)
                .expect("an arm has a mesh (checked at load), and every color is valid");
        }
        // An enemy's arm: the same, by its move; at the other end of the
        // sector for an even cut of a combo.
        for e in &self.enemies {
            let Some(arm) = e.arm else {
                continue;
            };
            let moment = e.moment();
            let phase = moment.map(|(_, m)| m.phase);
            let angle = moment.and_then(|(kind, m)| {
                let half = e.params().of(kind).arc * 0.5;
                let angle = match m.phase {
                    AttackPhase::Startup => Some(if m.reversed { half } else { -half }),
                    AttackPhase::Active => {
                        let k = m.active.expect("an active tick has its number");
                        let (lo, hi) = e.slice(kind, k, m.reversed);
                        Some(if m.reversed { lo } else { hi })
                    }
                    AttackPhase::Recovery => None,
                };
                angle.map(|a| a + Enemy::center(kind))
            });
            let t = self.scene.transform_mut(arm.id);
            match angle {
                None => {
                    t.position = arm.p0;
                    t.rotation = arm.r0;
                }
                Some(angle) => {
                    let q = Quat::from_rotation_y(angle);
                    t.position = q * arm.p0;
                    t.rotation = q * arm.r0;
                }
            }
            let h = &self.hurtboxes[e.hurtbox];
            let dead = h.health.is_some_and(|health| health.is_dead());
            let flashing = h.last_hit.is_some_and(|hit| tick - hit.tick < FLASH_TICKS);
            let color = if !dead && !flashing && phase == Some(AttackPhase::Startup) {
                WARN_COLOR
            } else {
                shown(arm.color, dead, flashing)
            };
            self.scene
                .set_mesh_color(arm.id, color)
                .expect("an arm has a mesh (checked at load), and every color is valid");
        }
    }

    /// Starts a roll the way the input points, or a backstep without a
    /// direction, and runs its first tick.
    fn start_roll(&mut self, player: &mut PlayerState, from_buffer: bool, intent: &Intent) {
        if intent.v.length_squared() > 0.0 {
            // Roll the way the input points, turning there at once.
            player.action = Action::Roll;
            self.action_dir = intent.dir / intent.dir.length();
            player.facing =
                positive_zero(wrap(libm::atan2f(-self.action_dir.x, -self.action_dir.z)));
        } else {
            // Backstep: away from where the character faces, no turn.
            player.action = Action::Backstep;
            self.action_dir = Vec3::new(libm::sinf(player.facing), 0.0, libm::cosf(player.facing));
        }
        self.events.push(Event::ActionStart {
            action: player.action,
            combo: 0,
            buffered: from_buffer,
        });
        let m = self.params.movement(player.action);
        player.stamina = f32::max(player.stamina - m.cost, 0.0);
        player.stamina_wait = self.params.delay_ticks;
        player.action_tick = 1;
        player.position = resolve(
            &self.boxes,
            self.radius,
            self.height,
            player.position + self.action_dir * m.step,
            &self.enemy_circles(),
        );
        if m.ticks == 1 {
            self.end_action(player, 0);
        }
    }

    /// Starts a drink, using up a flask, and runs its first tick.
    fn start_drink(&mut self, player: &mut PlayerState, from_buffer: bool, intent: &Intent) {
        player.flask -= 1;
        player.action = Action::Drink;
        player.action_tick = 1;
        self.events.push(Event::ActionStart {
            action: Action::Drink,
            combo: 0,
            buffered: from_buffer,
        });
        self.drink_tick(player, intent);
    }

    /// Tick `action_tick` of the running drink: walking slowly and turning
    /// as step 5 does, healing on `flask_heal_tick`. The last tick ends
    /// the drink.
    fn drink_tick(&mut self, player: &mut PlayerState, intent: &Intent) {
        player.position = resolve(
            &self.boxes,
            self.radius,
            self.height,
            player.position + intent.dir * self.params.flask_step,
            &self.enemy_circles(),
        );
        self.turn_player(player, intent);
        if player.action_tick == self.params.flask_heal_tick {
            let now = &mut self.health.now;
            let before = now.hp;
            now.hp = f32::min(
                now.hp + self.params.flask_heal * self.health.hp_max,
                self.health.hp_max,
            );
            self.events.push(Event::Heal {
                amount: now.hp - before,
                hp: now.hp,
            });
        }
        if player.action_tick == self.params.flask_ticks {
            self.end_action(player, 0);
        }
    }

    /// Step 5, turning at most `max_turn` per tick: towards the target
    /// while locked on and not sprinting, otherwise towards `dir`.
    fn turn_player(&self, player: &mut PlayerState, intent: &Intent) {
        if let Some(t) = self.lock
            && player.action != Action::Sprint
        {
            let to = self.target_position(t);
            if hdist(player.position, to) > 0.0 {
                player.facing = turn(player.facing, bearing(player.position, to), self.max_turn);
            }
        } else if intent.v.length_squared() > 0.0 {
            let dir = intent.dir;
            player.facing = turn(player.facing, libm::atan2f(-dir.x, -dir.z), self.max_turn);
        }
    }

    /// Starts the light or heavy attack `action` and runs its first tick.
    /// A light attack continues the combo only when it was buffered right
    /// after a light attack that had not reached `combo_max`.
    fn start_attack(
        &mut self,
        player: &mut PlayerState,
        action: Action,
        from_buffer: bool,
        intent: &Intent,
    ) {
        let (last, last_combo) = self.finished;
        player.combo = match action {
            Action::Attack
                if from_buffer && last == Action::Attack && last_combo < self.params.combo_max =>
            {
                last_combo + 1
            }
            Action::Attack => 1,
            _ => 0,
        };
        player.action = action;
        player.action_tick = 1;
        self.events.push(Event::ActionStart {
            action,
            combo: player.combo,
            buffered: from_buffer,
        });
        let a = self.params.attack_of(action);
        player.stamina = f32::max(player.stamina - a.cost, 0.0);
        player.stamina_wait = self.params.delay_ticks;
        // Each attack hits each hurtbox at most once.
        self.struck.clear();
        self.attack_tick(player, intent);
    }

    /// Tick `action_tick` of the running attack: turning during startup,
    /// sliding forward and hitting while active, and from then on buffering
    /// the next action. The last tick ends the attack.
    fn attack_tick(&mut self, player: &mut PlayerState, intent: &Intent) {
        let a = self.params.attack_of(player.action);
        let presses = intent.presses();
        let phase = if player.action_tick <= a.startup {
            // Presses on tick 1 are `next_action`'s, which started the attack.
            if player.action_tick > 1 {
                self.drop_presses(&presses, DropReason::Startup);
            }
            // Towards the target while locked on, otherwise towards `dir`.
            let step = self.params.attack_turn_step;
            if let Some(t) = self.lock {
                let to = self.target_position(t);
                if hdist(player.position, to) > 0.0 {
                    player.facing = turn(player.facing, bearing(player.position, to), step);
                }
            } else if intent.v.length_squared() > 0.0 {
                let target = libm::atan2f(-intent.dir.x, -intent.dir.z);
                player.facing = turn(player.facing, target, step);
            }
            AttackPhase::Startup
        } else if player.action_tick <= a.startup + a.active {
            let forward = Vec3::new(-libm::sinf(player.facing), 0.0, -libm::cosf(player.facing));
            player.position = resolve(
                &self.boxes,
                self.radius,
                self.height,
                player.position + forward * a.step,
                &self.enemy_circles(),
            );
            self.hit(player, a);
            AttackPhase::Active
        } else {
            AttackPhase::Recovery
        };
        player.attack_phase = Some(phase);
        if phase != AttackPhase::Startup {
            // A later press replaces an earlier one; on one tick, heavy
            // beats light beats the roll key. Startup presses are dropped.
            if let Some((&new, outranked)) = presses.split_first() {
                if let Some(old) = player.buffered {
                    self.events.push(Event::BufferReplaced { old, new });
                }
                player.buffered = Some(new);
                self.drop_presses(outranked, DropReason::Outranked);
            }
        }
        if player.action_tick == a.startup + a.active + a.recovery {
            let combo = player.combo;
            self.end_action(player, combo);
            player.attack_phase = None;
            player.combo = 0;
        }
    }

    /// The hit check of active tick `action_tick - startup` of the attack
    /// `a`, after the slide: the tick sweeps its slice of the sector, right
    /// to left, and hits every hurtbox it touches that is not dead, not a
    /// roaring enemy and this attack has not hit yet. A hurtbox with health
    /// takes the damage, and killed gives its souls; one killed while locked
    /// on is let go of; an enemy
    /// left with at most `phase2_at` of its health enters phase 2.
    fn hit(&mut self, player: &mut PlayerState, a: Attack) {
        let (lo, hi) = slice_of(a.arc, a.slice, player.action_tick - a.startup);
        let sweep = Sweep {
            p: player.position,
            facing: player.facing,
            reach: a.reach,
            lo,
            hi,
            low: self.params.swing_low,
            high: self.params.swing_high,
        };
        let tick = self.clock.tick();
        for (i, h) in self.hurtboxes.iter_mut().enumerate() {
            if h.health.is_some_and(|health| health.is_dead()) {
                continue;
            }
            // A roaring enemy is invulnerable: not even struck.
            if self
                .enemies
                .iter()
                .any(|e| e.hurtbox == i && e.state == EnemyCondition::Roar)
            {
                continue;
            }
            // Hurtboxes are root nodes: the local position is the world one.
            let at = self.scene.transform(h.id).position;
            if !self.struck.contains(&i) && sweep.touches(at, h.radius, h.height) {
                h.hits += 1;
                h.last_hit = Some(LastHit {
                    tick,
                    attack: player.action,
                    combo: player.combo,
                });
                self.struck.push(i);
                // Away from the character; -Z when they stand on one spot,
                // where `bearing` would give -π.
                let fall = if hdist(sweep.p, at) > 0.0 {
                    bearing(sweep.p, at)
                } else {
                    0.0
                };
                let attack = match player.action {
                    Action::Heavy => HitAttack::Heavy,
                    _ => HitAttack::Attack,
                };
                let (damage, hp, result) = match &mut h.health {
                    Some(health) => {
                        let outcome = health.take(a.damage, a.impact, fall);
                        (Some(a.damage), Some(health.now.hp), outcome)
                    }
                    None => (None, None, Outcome::Hit),
                };
                self.events.push(Event::Hit {
                    by: self.player_path.clone(),
                    target: h.path.clone(),
                    attack,
                    combo: player.combo,
                    damage,
                    hp,
                    result,
                });
                if result == Outcome::Death && h.souls > 0 {
                    player.souls = player.souls.saturating_add(h.souls);
                    self.events.push(Event::Souls {
                        from: h.path.clone(),
                        amount: h.souls,
                        total: player.souls,
                    });
                }
                if let Some(health) = &mut h.health
                    && let Some(e) = self.enemies.iter_mut().find(|e| e.hurtbox == i)
                {
                    enter_phase2(e, health, &mut self.events);
                }
                if result == Outcome::Death
                    && self
                        .lock
                        .is_some_and(|t| self.targets[t].hurtbox == Some(i))
                {
                    self.lock = None;
                    self.events.push(Event::Lock {
                        target: None,
                        reason: LockReason::TargetDied,
                    });
                }
            }
        }
    }

    /// Whether lock-on target `t` is a hurtbox lying dead.
    fn target_dead(&self, t: usize) -> bool {
        self.targets[t]
            .hurtbox
            .and_then(|h| self.hurtboxes[h].health)
            .is_some_and(|health| health.is_dead())
    }

    /// After step 6, every hurtbox shows its state; everything is set every
    /// tick, whether it changed or not:
    /// - its meshes (but the hp bar): the scene colors times [`DEAD_DIM`]
    ///   when dead, else [`FLASH_COLOR`] within [`FLASH_TICKS`] ticks of a
    ///   hit (this one included), else for an enemy (but its arm, which
    ///   [`Game::show_arms`] colors) [`WARN_COLOR`] roaring before the
    ///   blast and the scene colors times [`PHASE2_TINT`] in phase 2, else
    ///   the scene colors;
    /// - with health, its pose: the scene rotation, or when staggered or dead
    ///   tipped over by [`STAGGER_TILT_DEGREES`] or [`DEAD_TILT_DEGREES`]
    ///   towards its `fall`, away from the character;
    /// - its hp bar: as long as the health left, the left end in place,
    ///   hidden when dead.
    fn show_hurtboxes(&mut self) {
        let tick = self.clock.tick();
        for (i, h) in self.hurtboxes.iter().enumerate() {
            let condition = h.health.map(|health| health.now.state);
            let dead = condition == Some(Condition::Dead);
            let flashing = h.last_hit.is_some_and(|hit| tick - hit.tick < FLASH_TICKS);
            // An enemy's meshes but its arm: the warning color before the
            // roar's blast, else darkened red in phase 2.
            let enemy = self.enemies.iter().find(|e| e.hurtbox == i);
            let warning =
                enemy.is_some_and(|e| e.roar().is_some_and(|(_, p)| p == AttackPhase::Startup));
            let phase2 = enemy.is_some_and(|e| e.stage == 2);
            let arm = enemy.and_then(|e| e.arm).map(|arm| arm.id);
            for &(id, color) in &h.meshes {
                let color = if dead || flashing || Some(id) == arm {
                    shown(color, dead, flashing)
                } else if warning {
                    WARN_COLOR
                } else if phase2 {
                    color * PHASE2_TINT
                } else {
                    color
                };
                self.scene
                    .set_mesh_color(id, color)
                    .expect("meshes were picked for having a mesh, and every color is valid");
            }
            let Some(health) = h.health else {
                continue;
            };
            self.scene.transform_mut(h.id).rotation = pose(health.now.state, health.fall, h.base);
            if let Some(bar) = health.bar {
                let frac = health.now.hp / health.hp_max;
                let t = self.scene.transform_mut(bar.id);
                t.scale.x = bar.width * frac;
                t.position.x = bar.x0 - bar.width * (1.0 - frac) / 2.0;
                // A bar scaled to nothing cannot be drawn properly: hide it.
                self.scene.set_visible(bar.id, !dead);
            }
        }
    }

    /// Step 6 for the camera node: orbit the pivot above the player, pulled
    /// in at once in front of anything in the way, moving back out at
    /// `return_step` per tick once it is gone. When `loading`, the camera
    /// takes the unobstructed distance directly.
    fn place_camera(&mut self, loading: bool) {
        let position = self.state.player.position;
        let camera = self.state.camera;
        let rot = Quat::from_rotation_y(camera.yaw) * Quat::from_rotation_x(camera.pitch);
        let pivot = position + Vec3::new(0.0, self.pivot_height, 0.0);
        let target = self.obstruct(pivot, rot * Vec3::Z);
        let distance = if loading || target < camera.distance {
            target
        } else {
            f32::min(camera.distance + self.return_step, target)
        };
        self.state.camera.distance = distance;
        let transform = self.scene.transform_mut(self.camera);
        transform.position = pivot + rot * Vec3::new(0.0, 0.0, distance);
        transform.rotation = rot;
    }

    /// How far a sphere of `collision_radius` gets from `pivot` along the
    /// unit vector `u` before touching the ground or a collider, at most
    /// `distance`. Exactly the steps of `docs/architecture.md` section 11
    /// (camera collision): each box grown by the radius, slabs in the order
    /// a, y, b, only exactly rounded operations.
    fn obstruct(&self, pivot: Vec3, u: Vec3) -> f32 {
        let cr = self.collision_radius;
        let mut t = self.distance;
        if u.y < 0.0 {
            // The ground; pivot_height > cr, so this is positive.
            t = f32::min(t, (cr - self.pivot_height) / u.y);
        }
        let uxz = Vec2::new(u.x, u.z);
        'boxes: for bx in &self.boxes {
            let o = Vec2::new(pivot.x, pivot.z) - bx.center;
            let mut t0 = 0.0;
            let mut t1 = self.distance;
            for (s, w, lo, hi) in [
                (o.dot(bx.a), uxz.dot(bx.a), -(bx.ha + cr), bx.ha + cr),
                (pivot.y, u.y, bx.y_min - cr, bx.y_max + cr),
                (o.dot(bx.b), uxz.dot(bx.b), -(bx.hb + cr), bx.hb + cr),
            ] {
                if w == 0.0 {
                    if s < lo || s > hi {
                        continue 'boxes; // parallel to this pair of faces, and outside
                    }
                } else {
                    let ta = (lo - s) / w;
                    let tb = (hi - s) / w;
                    t0 = f32::max(t0, f32::min(ta, tb));
                    t1 = f32::min(t1, f32::max(ta, tb));
                }
            }
            if t0 <= t1 {
                t = f32::min(t, t0); // t0 is 0 when the pivot is inside the grown box
            }
        }
        t
    }

    pub fn clock(&self) -> SimClock {
        self.clock
    }

    /// The debug draw categories, in their order.
    pub fn debug_categories() -> &'static [&'static str] {
        &DEBUG_CATEGORIES
    }

    /// Draws the shapes the checks use now, for the `categories` given
    /// (names from [`Game::debug_categories`]; others are ignored), in
    /// category order (`docs/architecture.md` section 11, "调试绘制").
    /// Reads the state only.
    pub fn debug_draw(&self, categories: &[&str], draw: &mut DebugDraw) {
        let on = |name: &str| categories.contains(&name);
        if on("hurtboxes") {
            self.debug_hurtboxes(draw);
        }
        if on("swings") {
            self.debug_swings(draw);
        }
        if on("colliders") {
            for b in &self.boxes {
                let center = Vec3::new(b.center.x, 0.5 * (b.y_min + b.y_max), b.center.y);
                let half = Vec3::new(b.ha, 0.5 * (b.y_max - b.y_min), b.hb);
                // Turned about Y only: local X goes to (cos θ, 0, -sin θ).
                let turn = Quat::from_rotation_y(libm::atan2f(-b.a.y, b.a.x));
                draw.wire_box(center, half, turn, DEBUG_COLLIDER);
            }
        }
        if on("lock") {
            self.debug_lock(draw);
        }
        if on("bonfires") {
            // Bonfires are root nodes: the local position is the world one.
            for (i, b) in self.bonfires.iter().enumerate() {
                let at = self.scene.transform(b.id).position;
                draw.circle(at, b.range, DEBUG_BONFIRE);
                if self.respawn.bonfire == Some(i) {
                    draw.cross(
                        at + Vec3::new(0.0, 1.0, 0.0),
                        DEBUG_BONFIRE_CROSS,
                        DEBUG_BONFIRE,
                    );
                }
            }
        }
        if on("bloodstain")
            && let Some(stain) = self.bloodstain
        {
            draw.circle(
                stain.position,
                self.params.bloodstain_range,
                DEBUG_BLOODSTAIN,
            );
        }
    }

    /// Every hurtbox's cylinder (pre-order), then the character's.
    fn debug_hurtboxes(&self, draw: &mut DebugDraw) {
        let color = |condition: Option<Condition>, invulnerable: bool| match condition {
            Some(Condition::Dead) => DEBUG_DEAD,
            Some(Condition::Stagger) => DEBUG_STAGGER,
            _ if invulnerable => DEBUG_INVULNERABLE,
            _ => DEBUG_HURTBOX,
        };
        for h in &self.hurtboxes {
            // Hurtboxes are root nodes: the local position is the world one.
            let at = self.scene.transform(h.id).position;
            let condition = h.health.map(|health| health.now.state);
            draw.cylinder(at, h.radius, h.height, color(condition, false));
        }
        let player = &self.state.player;
        let color = color(Some(self.health.now.state), player.invulnerable);
        draw.cylinder(player.position, self.radius, self.height, color);
    }

    /// The character's attack, then every swinger's (pre-order), in
    /// startup or active only: the sector at both heights of the band,
    /// and while active this tick's slice.
    fn debug_swings(&self, draw: &mut DebugDraw) {
        let player = &self.state.player;
        if matches!(player.action, Action::Attack | Action::Heavy)
            && let Some(phase) = player.attack_phase
        {
            let a = self.params.attack_of(player.action);
            let slice = (phase == AttackPhase::Active)
                .then(|| slice_of(a.arc, a.slice, player.action_tick - a.startup));
            let band = (self.params.swing_low, self.params.swing_high);
            let sweep = (player.position, player.facing, a.reach, a.arc);
            debug_sweep(draw, sweep, band, phase, slice);
        }
        for e in &self.enemies {
            // Enemies are root nodes: the local position is the world one.
            let at = self.scene.transform(e.id).position;
            let band = (e.params().low, e.params().high);
            // The roar's blast: the whole circle, bright on its tick.
            if let Some((p, phase)) = e.roar() {
                let slice = (phase == AttackPhase::Active).then_some((-PI, PI));
                debug_sweep(draw, (at, e.facing, p.reach, TAU), band, phase, slice);
                continue;
            }
            let Some((kind, m)) = e.moment() else {
                continue;
            };
            let slice = m.active.map(|k| e.slice(kind, k, m.reversed));
            let mv = e.params().of(kind);
            let facing = e.facing + Enemy::center(kind);
            debug_sweep(draw, (at, facing, mv.reach, mv.arc), band, m.phase, slice);
        }
        for s in &self.swingers {
            let Some(phase) = s.phase() else {
                continue;
            };
            let slice =
                (phase == AttackPhase::Active).then(|| s.slice_of(s.swing_tick - s.startup));
            // Swingers are root nodes: the local position is the world one.
            let at = self.scene.transform(s.id).position;
            debug_sweep(
                draw,
                (at, s.facing, s.reach, s.arc),
                (s.low, s.high),
                phase,
                slice,
            );
        }
    }

    /// A cross on every target's lock point (pre-order), the locked one's
    /// and a line to it from the camera pivot, then the lock ranges
    /// around the character's feet.
    fn debug_lock(&self, draw: &mut DebugDraw) {
        let point =
            |t: usize| self.target_position(t) + Vec3::new(0.0, self.targets[t].height, 0.0);
        for t in 0..self.targets.len() {
            let color = if self.target_dead(t) {
                DEBUG_DEAD
            } else {
                DEBUG_TARGET
            };
            draw.cross(point(t), DEBUG_TARGET_CROSS, color);
        }
        let feet = self.state.player.position;
        if let Some(t) = self.lock {
            draw.cross(point(t), DEBUG_LOCKED_CROSS, DEBUG_LOCKED);
            let pivot = feet + Vec3::new(0.0, self.pivot_height, 0.0);
            draw.line(pivot, point(t), DEBUG_LOCKED);
        }
        draw.circle(feet, self.params.lock.range, DEBUG_LOCK_RANGE);
        if self.lock.is_some() {
            draw.circle(feet, self.params.lock.break_range, DEBUG_BREAK_RANGE);
        }
    }

    /// The events of the latest tick, in the order they happened; none
    /// before the first tick.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Ends the running roll, backstep, attack or drink on its last tick; `combo`
    /// is what a buffered light attack continues from.
    fn end_action(&mut self, player: &mut PlayerState, combo: u32) {
        self.events.push(Event::ActionEnd {
            action: player.action,
            reason: EndReason::Finished,
        });
        self.finished = (player.action, combo);
        player.action = Action::Idle;
        player.action_tick = 0;
    }

    /// An event for the lock-on target changing to `t` (`None`: let go).
    fn lock_event(&mut self, t: Option<usize>, reason: LockReason) {
        let target = t.map(|t| self.targets[t].path.clone());
        self.events.push(Event::Lock { target, reason });
    }

    /// `input_dropped` for each of `inputs`.
    fn drop_inputs(&mut self, inputs: &[Input], reason: DropReason) {
        for &input in inputs {
            self.events.push(Event::InputDropped {
                input,
                reason,
                buffered: false,
            });
        }
    }

    /// `input_dropped` for each of this tick's `presses`.
    fn drop_presses(&mut self, presses: &[Buffered], reason: DropReason) {
        let inputs: Vec<Input> = presses.iter().map(|&p| p.into()).collect();
        self.drop_inputs(&inputs, reason);
    }

    /// `next_action` of step 4: the buffered action, else this tick's first
    /// press (heavy, then light, then a roll-key tap), with whether it was
    /// buffered. With no stamina left there is none but a drink, and with
    /// no flask left no drink; then the input, buffered or not, is
    /// dropped, along with the presses it outranked.
    fn next_action(
        &mut self,
        player: &mut PlayerState,
        presses: &[Buffered],
    ) -> Option<(Buffered, bool)> {
        let next = if let Some(buffered) = player.buffered.take() {
            (buffered, true)
        } else {
            (*presses.first()?, false)
        };
        let reason = match next.0 {
            Buffered::Drink if player.flask == 0 => DropReason::Empty,
            Buffered::Drink => return Some(next),
            _ if player.stamina > 0.0 => return Some(next),
            _ => DropReason::NoStamina,
        };
        let (next, from_buffer) = next;
        self.events.push(Event::InputDropped {
            input: next.into(),
            reason,
            buffered: from_buffer,
        });
        self.drop_presses(outranked(presses, from_buffer), DropReason::Outranked);
        None
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Game state for `--dump-state`.
    pub fn state(&self) -> GameState {
        GameState {
            player: self.state.player.clone(),
            camera: self.state.camera,
            lock: self.lock.map(|t| self.targets[t].path.clone()),
            hurtboxes: self
                .hurtboxes
                .iter()
                .map(|h| HurtboxState {
                    node: h.path.clone(),
                    hits: h.hits,
                    last_hit: h.last_hit,
                    health: h.health.map(|health| health.now),
                })
                .collect(),
            swingers: self
                .swingers
                .iter()
                .map(|s| SwingerState {
                    node: s.path.clone(),
                    swing_tick: s.swing_tick,
                    phase: s.phase(),
                })
                .collect(),
            enemies: self
                .enemies
                .iter()
                .map(|e| {
                    let moment = e.moment();
                    EnemyState {
                        node: e.path.clone(),
                        stage: e.stage,
                        state: e.state,
                        r#move: e.current,
                        move_tick: e.move_tick,
                        phase: moment
                            .map(|(_, m)| m.phase)
                            .or(e.roar().map(|(_, phase)| phase)),
                        hit: moment.map_or(0, |(_, m)| m.hit),
                        facing: e.facing,
                        rest: e.rest,
                        chase: e.chase,
                        rng: e.rng.0,
                    }
                })
                .collect(),
            respawn: RespawnState {
                position: self.respawn.position,
                facing: self.respawn.facing,
                bonfire: self.respawn.bonfire.map(|b| self.bonfires[b].path.clone()),
            },
            bloodstain: self.bloodstain,
        }
    }

    /// The paths of the lock-on targets, in their order.
    #[cfg(test)]
    pub(crate) fn target_paths(&self) -> Vec<&str> {
        self.targets.iter().map(|t| t.path.as_str()).collect()
    }

    /// Where target `t` is now: read from the scene every tick.
    fn target_position(&self, t: usize) -> Vec3 {
        // Targets are root nodes, so this is their world position.
        self.scene.transform(self.targets[t].id).position
    }

    /// `acquire()` of step 1a: the living target within `lock_range` and
    /// `lock_angle` closest to where the camera looks (`yaw`), the first
    /// in pre-order on a tie.
    fn acquire(&self, position: Vec3, yaw: f32) -> Option<usize> {
        let lock = self.params.lock;
        let mut best: Option<(usize, f32)> = None;
        for t in (0..self.targets.len()).filter(|&t| !self.target_dead(t)) {
            let to = self.target_position(t);
            let d = hdist(position, to);
            if d == 0.0 || d > lock.range {
                continue;
            }
            let a = wrap(bearing(position, to) - yaw).abs();
            if a > lock.angle {
                continue;
            }
            if best.is_none_or(|(_, best_a)| a < best_a) {
                best = Some((t, a));
            }
        }
        best.map(|(t, _)| t)
    }

    /// `switch_target(side)` of step 1b: of the living targets within
    /// `lock_range` on the right (or left) of `current` as seen from
    /// `position`, the one closest in direction; the first in pre-order on
    /// a tie.
    fn switch_target(&self, position: Vec3, current: usize, right: bool) -> Option<usize> {
        let cur = bearing(position, self.target_position(current));
        let mut best: Option<(usize, f32)> = None;
        for t in (0..self.targets.len()).filter(|&t| t != current && !self.target_dead(t)) {
            let to = self.target_position(t);
            let d = hdist(position, to);
            if d == 0.0 || d > self.params.lock.range {
                continue;
            }
            // Positive is to the left, negative to the right.
            let delta = wrap(bearing(position, to) - cur);
            let better = if right {
                delta < 0.0 && best.is_none_or(|(_, b)| delta > b)
            } else {
                delta > 0.0 && best.is_none_or(|(_, b)| delta < b)
            };
            if better {
                best = Some((t, delta));
            }
        }
        best.map(|(t, _)| t)
    }
}

/// What the nodes other than the character are for, each list in pre-order.
struct Roles {
    targets: Vec<Target>,
    hurtboxes: Vec<Hurtbox>,
    swingers: Vec<Swinger>,
    enemies: Vec<Enemy>,
    bonfires: Vec<Bonfire>,
    /// The root node named `bloodstain`, with its scene file position.
    bloodstain_marker: Option<(NodeId, Vec3)>,
}

/// Which node props only go with another one, as [`roles`] checks them
/// (`loom schema` writes them out).
pub(crate) const NODE_REQUIRES: [&str; 6] = [
    "hurt_radius and hurt_height: both or neither",
    "hp: needs hurt_radius and hurt_height",
    "poise, stagger_ticks, poise_reset_ticks, revive_ticks, souls: need hp",
    "swing_impact, swing_startup_ticks, swing_active_ticks, swing_recovery_ticks, swing_reach, \
     swing_arc, swing_low, swing_high: need swing_damage",
    "every enemy_*, sweep_*, cleave_*, lunge_*, combo_*, approach_*, phase2_*, roar_* and \
     spin_* prop but enemy_speed: needs enemy_speed",
    "phase2_speed_scale, phase2_combo_hits, roar_ticks, roar_blast_tick, roar_damage, \
     roar_impact, roar_reach: need phase2_at",
];

/// The other rules [`roles`] checks on a node's props.
pub(crate) const NODE_CONSTRAINTS: [&str; 12] = [
    "a swinger: swing_low < swing_high",
    "lock-on targets, hurtboxes and swingers are root nodes, and not the camera",
    "a hurtbox with hp: its child named hp_bar, if any, needs a mesh",
    "a swinger or an enemy: its child named arm, if any, needs a mesh",
    "an enemy (a node with enemy_speed): <move>_min_range <= <move>_max_range for sweep, cleave, \
     lunge, combo and spin",
    "an enemy: roar_blast_tick <= roar_ticks",
    "an enemy: enemy_swing_low < enemy_swing_high",
    "an enemy needs hurt_radius, hurt_height, hp and lock_height, and is not a swinger",
    "an enemy: enemy_speed / 60, lunge_distance / lunge_active_ticks and, with phase2_at, \
     enemy_speed / 60 / phase2_speed_scale are less than hurt_radius",
    "an enemy has no collider in its subtree",
    "a bonfire (a node with bonfire_range) is a root node, and not the camera, and not also a \
     lock target, hurtbox, swinger or enemy",
    "the bloodstain marker (the root node named bloodstain) has a mesh, and no props or children",
];

/// Every lock-on target, hurtbox, swinger and enemy of `scene`, in
/// pre-order, checked node by node (every node but the character), in this
/// order: no props but [`NODE_PROPS`]; each value by its rule; the two hurt
/// props together or not at all; `hp` only on a hurtbox; the four props
/// after it only with `hp`; the eight swing props only with `swing_damage`;
/// a swinger's `swing_low < swing_high`; only on root nodes other than the
/// camera; an `hp_bar` with a mesh; an `arm` with a mesh; the enemy props
/// only with `enemy_speed`; the phase 2 props only with `phase2_at`; then,
/// for an enemy, each move's ranges, the roar's blast within it, its swing
/// band, the props it needs, not a swinger, its steps (with phase 2's
/// walk last) shorter than `hurt_radius` and no collider in its subtree
/// (a lock-on target, it is a root node already); last, for a bonfire, a
/// root node other than the camera, then none of the other roles; last,
/// the bloodstain marker's mesh, props and children. `souls` goes with
/// `hp` after the four props before it.
fn roles(scene: &Scene, player: NodeId, camera: NodeId) -> Result<Roles, GameError> {
    let mut targets = Vec::new();
    let mut hurtboxes = Vec::new();
    let mut swingers = Vec::new();
    let mut enemies = Vec::new();
    let mut bonfires = Vec::new();
    let mut bloodstain_marker = None;
    for id in scene.ids().filter(|&id| id != player) {
        let node = scene.node(id);
        let path = || scene.path(id);
        if let Some((prop, _)) = node
            .props()
            .find(|&(p, _)| !NODE_PROPS.iter().any(|spec| spec.name == p))
        {
            return Err(GameError::UnknownNodeProp {
                node: path(),
                prop: prop.to_owned(),
            });
        }
        for spec in NODE_PROPS {
            if let Some(value) = node.prop(spec.name)
                && !spec.rule.allows(value)
            {
                return Err(GameError::NodeBadProp {
                    node: path(),
                    prop: spec.name,
                    rule: spec.rule,
                    value,
                });
            }
        }
        // Present, or the default; `None` only for a missing prop without one.
        let get = |name: &str| {
            let spec = NODE_PROPS
                .iter()
                .find(|spec| spec.name == name)
                .expect("only names from NODE_PROPS are asked for");
            node.prop(name).or(spec.default)
        };
        // Only for props with a default.
        let value = |name: &str| get(name).expect("only props with a default are asked for");
        // Checked above: whole numbers of at least 1; `as` saturates.
        let ticks = |name: &str| value(name) as u32;
        let lock_height = node.prop("lock_height");
        let hurt = match (node.prop("hurt_radius"), node.prop("hurt_height")) {
            (Some(radius), Some(height)) => Some((radius as f32, height as f32)),
            (None, None) => None,
            _ => return Err(GameError::HurtPropsUnpaired(path())),
        };
        let hp = node.prop("hp");
        if hp.is_some() && hurt.is_none() {
            return Err(GameError::HpWithoutHurtbox(path()));
        }
        if hp.is_none()
            && let Some(spec) = NODE_PROPS[HP_PROPS]
                .iter()
                .chain([&NODE_PROPS[SOULS_PROP]])
                .find(|spec| node.prop(spec.name).is_some())
        {
            return Err(GameError::NeedsHp {
                node: path(),
                prop: spec.name,
            });
        }
        let swing_damage = node.prop("swing_damage");
        if swing_damage.is_none()
            && let Some(spec) = NODE_PROPS[SWING_PROPS]
                .iter()
                .find(|spec| node.prop(spec.name).is_some())
        {
            return Err(GameError::NeedsSwingDamage {
                node: path(),
                prop: spec.name,
            });
        }
        // Compared as used: after `as f32`, like the character's band.
        if swing_damage.is_some() && value("swing_low") as f32 >= value("swing_high") as f32 {
            return Err(GameError::NodeSwingBand(path()));
        }
        if (lock_height.is_some() || hurt.is_some() || swing_damage.is_some())
            && (node.parent.is_some() || id == camera)
        {
            return Err(if lock_height.is_some() {
                GameError::LockTargetNotRoot(path())
            } else if hurt.is_some() {
                GameError::HurtboxNotRoot(path())
            } else {
                GameError::SwingerNotRoot(path())
            });
        }
        // Only a hurtbox with hp has an hp bar, and only a swinger an arm;
        // anywhere else the names mean nothing.
        let child = |name: &str| {
            node.children
                .iter()
                .copied()
                .find(|&c| scene.node(c).name == name)
        };
        let bar = hp.and_then(|_| child(HP_BAR));
        if let Some(bar) = bar
            && scene.node(bar).mesh.is_none()
        {
            return Err(GameError::HpBarWithoutMesh(path()));
        }
        let enemy_speed = node.prop("enemy_speed");
        let arm = swing_damage.or(enemy_speed).and_then(|_| child(ARM));
        if let Some(arm) = arm
            && scene.node(arm).mesh.is_none()
        {
            return Err(GameError::ArmWithoutMesh(path()));
        }
        if enemy_speed.is_none()
            && let Some(spec) = NODE_PROPS[ENEMY_OTHER_PROPS]
                .iter()
                .find(|spec| node.prop(spec.name).is_some())
        {
            return Err(GameError::NeedsEnemySpeed {
                node: path(),
                prop: spec.name,
            });
        }
        let phase2_at = node.prop("phase2_at");
        if phase2_at.is_none()
            && let Some(spec) = NODE_PROPS[PHASE2_OTHER_PROPS]
                .iter()
                .find(|spec| node.prop(spec.name).is_some())
        {
            return Err(GameError::NeedsPhase2 {
                node: path(),
                prop: spec.name,
            });
        }
        // Checked on, in order, only for an enemy; its parameters.
        let enemy = match enemy_speed {
            None => None,
            Some(speed) => {
                let range = |kind: MoveKind, end: &str| {
                    value(&format!("{}_{end}_range", kind.name())) as f32
                };
                for kind in MoveKind::ALL {
                    if range(kind, "min") > range(kind, "max") {
                        return Err(GameError::EnemyRanges {
                            node: path(),
                            m: kind.name(),
                        });
                    }
                }
                if ticks("roar_blast_tick") > ticks("roar_ticks") {
                    return Err(GameError::EnemyRoarBlast(path()));
                }
                let (low, high) = (
                    value("enemy_swing_low") as f32,
                    value("enemy_swing_high") as f32,
                );
                if low >= high {
                    return Err(GameError::EnemySwingBand(path()));
                }
                let (Some((radius, height)), Some(_), Some(_)) = (hurt, hp, lock_height) else {
                    return Err(GameError::EnemyIncomplete(path()));
                };
                if swing_damage.is_some() {
                    return Err(GameError::EnemySwinger(path()));
                }
                let moves = MoveKind::ALL.map(|kind| {
                    let v = |suffix: &str| value(&format!("{}_{suffix}", kind.name()));
                    let active = v("active_ticks") as u32;
                    let arc = (v("arc") as f32).to_radians();
                    MoveParams {
                        damage: v("damage") as f32,
                        impact: v("impact") as f32,
                        startup: v("startup_ticks") as u32,
                        active,
                        recovery: v("recovery_ticks") as u32,
                        reach: v("reach") as f32,
                        arc,
                        slice: arc / active as f32,
                        min_range: v("min_range") as f32,
                        max_range: v("max_range") as f32,
                        weight: v("weight") as u32,
                        track: (v("track_speed") as f32).to_radians() / 60.0,
                    }
                });
                let lunge_active = moves[MoveKind::Lunge as usize].active;
                let params = EnemyParams {
                    step: speed as f32 / 60.0,
                    turn: (value("enemy_turn_speed") as f32).to_radians() / 60.0,
                    face_angle: (value("enemy_face_angle") as f32).to_radians(),
                    rest_ticks: ticks("enemy_rest_ticks"),
                    aggro_range: value("enemy_aggro_range") as f32,
                    // Checked above: a whole number of at least 0; `as`
                    // saturates.
                    seed: value("enemy_seed") as u64,
                    low,
                    high,
                    lunge_step: value("lunge_distance") as f32 / lunge_active as f32,
                    combo_hits: ticks("combo_hits"),
                    combo_gap: ticks("combo_gap_ticks"),
                    approach_min: value("approach_min_range") as f32,
                    approach_per_meter: value("approach_weight_per_meter") as f32,
                    approach_ticks: ticks("approach_ticks"),
                    approach_max: ticks("approach_weight_max"),
                    moves,
                    spin: false,
                };
                let phase2 = phase2_at.map(|at| {
                    let s = value("phase2_speed_scale") as f32;
                    Phase2 {
                        at: at as f32,
                        params: params.phase2(s, ticks("phase2_combo_hits")),
                        roar_ticks: ticks("roar_ticks"),
                        blast_tick: ticks("roar_blast_tick"),
                        damage: value("roar_damage") as f32,
                        impact: value("roar_impact") as f32,
                        reach: value("roar_reach") as f32,
                    }
                });
                let steps = [
                    Some(("enemy", params.step)),
                    Some(("lunge", params.lunge_step)),
                    phase2.map(|p| ("enemy in phase 2", p.params.step)),
                ];
                for (what, step) in steps.into_iter().flatten() {
                    if step >= radius {
                        return Err(GameError::EnemyStepTooLong {
                            enemy: path(),
                            what,
                            step,
                            radius,
                        });
                    }
                }
                if scene
                    .colliders()
                    .iter()
                    .any(|&c| is_in_subtree(scene, c, id))
                {
                    return Err(GameError::EnemyCollider(path()));
                }
                Some((params, phase2, radius, height))
            }
        };
        if let Some(range) = node.prop("bonfire_range") {
            if node.parent.is_some() || id == camera {
                return Err(GameError::BonfireNotRoot(path()));
            }
            if lock_height.is_some()
                || hurt.is_some()
                || swing_damage.is_some()
                || enemy_speed.is_some()
            {
                return Err(GameError::BonfireRole(path()));
            }
            bonfires.push(Bonfire {
                id,
                path: path(),
                range: range as f32,
            });
        }
        if node.name == BLOODSTAIN && node.parent.is_none() {
            if node.mesh.is_none() || node.props().next().is_some() || !node.children.is_empty() {
                return Err(GameError::BloodstainMarker(path()));
            }
            bloodstain_marker = Some((id, scene.transform(id).position));
        }
        let mut hurtbox = hurt.map(|(radius, height)| {
            let health = hp.map(|hp| {
                Health::new(
                    hp as f32,
                    value("poise") as f32,
                    ["stagger_ticks", "poise_reset_ticks", "revive_ticks"].map(ticks),
                    bar.map(|bar| {
                        let t = scene.transform(bar);
                        HpBar {
                            id: bar,
                            x0: t.position.x,
                            width: t.scale.x,
                        }
                    }),
                )
            });
            Hurtbox {
                id,
                path: path(),
                radius,
                height,
                meshes: scene
                    .ids()
                    .filter(|&n| is_in_subtree(scene, n, id) && Some(n) != bar)
                    .filter_map(|n| scene.node(n).mesh.map(|m| (n, m.color)))
                    .collect(),
                base: scene.transform(id).rotation,
                hits: 0,
                last_hit: None,
                health,
                // Checked above: a whole number of at least 0.
                souls: value("souls") as u32,
            }
        });
        let hurtbox_index = hurtbox.as_ref().map(|_| hurtboxes.len());
        if let Some(height) = lock_height {
            targets.push(Target {
                id,
                path: path(),
                height: height as f32,
                hurtbox: hurtbox_index,
            });
        }
        if let Some(damage) = swing_damage {
            let [startup, active, recovery] = [
                "swing_startup_ticks",
                "swing_active_ticks",
                "swing_recovery_ticks",
            ]
            .map(ticks);
            let arc = (value("swing_arc") as f32).to_radians();
            // As the character's facing at load.
            let f = scene.transform(id).rotation * Vec3::NEG_Z;
            swingers.push(Swinger {
                id,
                path: path(),
                facing: positive_zero(wrap(libm::atan2f(-f.x, -f.z))),
                damage: damage as f32,
                impact: value("swing_impact") as f32,
                startup,
                active,
                period: startup.saturating_add(active).saturating_add(recovery),
                reach: value("swing_reach") as f32,
                arc,
                slice: arc / active as f32,
                low: value("swing_low") as f32,
                high: value("swing_high") as f32,
                hurtbox: hurtbox_index,
                arm: arm.map(|arm| {
                    let t = scene.transform(arm);
                    Arm {
                        id: arm,
                        p0: t.position,
                        r0: t.rotation,
                        color: scene
                            .node(arm)
                            .mesh
                            .expect("checked above: the arm has a mesh")
                            .color,
                    }
                }),
                swing_tick: 0,
                struck: false,
            });
        }
        if let Some((params, phase2, radius, height)) = enemy {
            // As the character's facing at load.
            let f = scene.transform(id).rotation * Vec3::NEG_Z;
            let facing = positive_zero(wrap(libm::atan2f(-f.x, -f.z)));
            let hurtbox = hurtbox
                .as_mut()
                .expect("checked above: an enemy is a hurtbox");
            hurtbox.base = Quat::from_rotation_y(facing);
            enemies.push(Enemy {
                id,
                path: path(),
                base: params,
                phase2,
                stage: 1,
                hurtbox: hurtboxes.len(),
                radius,
                height,
                arm: arm.map(|arm| {
                    let t = scene.transform(arm);
                    Arm {
                        id: arm,
                        p0: t.position,
                        r0: t.rotation,
                        color: scene
                            .node(arm)
                            .mesh
                            .expect("checked above: the arm has a mesh")
                            .color,
                    }
                }),
                state: EnemyCondition::Idle,
                current: None,
                move_tick: 0,
                facing,
                rest: 0,
                chase: 0,
                rng: SplitMix64(params.seed),
                struck: 0,
            });
        }
        hurtboxes.extend(hurtbox);
    }
    Ok(Roles {
        targets,
        hurtboxes,
        swingers,
        enemies,
        bonfires,
        bloodstain_marker,
    })
}

/// An enemy with `phase2_at` still in phase 1, hit to at most that much of
/// its full health but not killed, enters phase 2: the hit's stagger is
/// dropped for a roar, its move broken off, and a `stage` event follows
/// the hit's.
fn enter_phase2(e: &mut Enemy, health: &mut Health, events: &mut Vec<Event>) {
    let Some(p) = e.phase2 else {
        return;
    };
    let hp = health.now.hp;
    if e.stage != 1 || hp <= 0.0 || hp > p.at * health.hp_max {
        return;
    }
    e.stage = 2;
    health.now.state = Condition::Ok;
    health.now.state_ticks = 0;
    health.now.impact = 0.0;
    health.now.impact_wait = 0;
    e.state = EnemyCondition::Roar;
    e.current = None;
    e.move_tick = 0;
    e.rest = 0;
    e.chase = 0;
    e.struck = 0;
    events.push(Event::Stage {
        by: e.path.clone(),
        stage: 2,
    });
}

/// The rotation of a node with health whose standing rotation is `base`:
/// `base` itself while `Ok` (bit for bit), otherwise tipped over towards
/// `fall` by [`STAGGER_TILT_DEGREES`] or [`DEAD_TILT_DEGREES`].
fn pose(state: Condition, fall: f32, base: Quat) -> Quat {
    let degrees = match state {
        Condition::Ok => return base,
        Condition::Stagger => STAGGER_TILT_DEGREES,
        Condition::Dead => DEAD_TILT_DEGREES,
    };
    // The top tips over towards `fall`, however the node faces.
    Quat::from_rotation_y(fall)
        * Quat::from_rotation_x(-degrees.to_radians())
        * Quat::from_rotation_y(-fall)
        * base
}

/// The color a mesh of scene color `color` shows: darkened when `dead`,
/// else [`FLASH_COLOR`] while `flashing`, else its own.
fn shown(color: Vec3, dead: bool, flashing: bool) -> Vec3 {
    if dead {
        color * DEAD_DIM
    } else if flashing {
        FLASH_COLOR
    } else {
        color
    }
}

/// One attack's sector for the debug draw: `(position, facing, reach,
/// arc)` at the heights `band` above the position. Startup: the whole
/// sector in orange. Active: the whole sector in dark red, then `slice`
/// (this tick's, given while active) in bright red. Recovery draws
/// nothing.
fn debug_sweep(
    draw: &mut DebugDraw,
    (at, facing, reach, arc): (Vec3, f32, f32, f32),
    (low, high): (f32, f32),
    phase: AttackPhase,
    slice: Option<(f32, f32)>,
) {
    let half = arc * 0.5;
    let levels = [low, high].map(|y| at + Vec3::new(0.0, y, 0.0));
    match phase {
        AttackPhase::Startup => {
            for p in levels {
                draw.sector(p, facing, reach, -half, half, DEBUG_STARTUP);
            }
        }
        AttackPhase::Active => {
            for p in levels {
                draw.sector(p, facing, reach, -half, half, DEBUG_ACTIVE);
            }
            if let Some((lo, hi)) = slice {
                for p in levels {
                    draw.sector(p, facing, reach, lo, hi, DEBUG_SLICE);
                }
            }
        }
        AttackPhase::Recovery => {}
    }
}

/// Whether `node` is `root` or one of its descendants.
fn is_in_subtree(scene: &Scene, node: NodeId, root: NodeId) -> bool {
    let mut at = Some(node);
    while let Some(n) = at {
        if n == root {
            return true;
        }
        at = scene.node(n).parent;
    }
    false
}

/// What one active tick of an attack sweeps: the part `[lo, hi]` (radians
/// relative to `facing`, positive to the left) of a sector of radius
/// `reach` around the feet `p`, over the heights `low..=high` above them.
struct Sweep {
    p: Vec3,
    facing: f32,
    reach: f32,
    lo: f32,
    hi: f32,
    low: f32,
    high: f32,
}

impl Sweep {
    /// `swept()` of `docs/architecture.md` section 11 (命中判定): whether
    /// the hurtbox standing at `at` with `radius` and `height` is touched.
    /// Its height range must overlap the band (both closed), and its
    /// circle be within `radius` of the sector part.
    fn touches(&self, at: Vec3, radius: f32, height: f32) -> bool {
        let p = self.p;
        if at.y > p.y + self.high || p.y + self.low > at.y + height {
            return false;
        }
        let d = Vec2::new(at.x - p.x, at.z - p.z);
        let rel = wrap(bearing(p, at) - self.facing);
        let dist = if self.lo <= rel && rel <= self.hi {
            // Within the angles: only the radius matters.
            f32::max(d.length() - self.reach, 0.0)
        } else {
            f32::min(self.ray(d, self.lo), self.ray(d, self.hi))
        };
        dist <= radius
    }

    /// `ray(θ)`: the distance from `d` (relative to `p`) to the edge of
    /// length `reach` at angle `theta`.
    fn ray(&self, d: Vec2, theta: f32) -> f32 {
        let u = Vec2::new(
            -libm::sinf(self.facing + theta),
            -libm::cosf(self.facing + theta),
        );
        let t = d.dot(u).clamp(0.0, self.reach);
        (d - u * t).length()
    }
}

/// The horizontal direction from `from` to `to`, as an angle like `facing`
/// and `yaw`: 0 is -Z, positive is to the left (counterclockwise from
/// above).
fn bearing(from: Vec3, to: Vec3) -> f32 {
    libm::atan2f(-(to.x - from.x), -(to.z - from.z))
}

/// The horizontal distance from `from` to `to`.
fn hdist(from: Vec3, to: Vec3) -> f32 {
    Vec2::new(to.x - from.x, to.z - from.z).length()
}

/// Step 5's turn: `facing` turned towards `target`, by at most `max`.
fn turn(facing: f32, target: f32, max: f32) -> f32 {
    let diff = wrap(target - facing);
    wrap(facing + diff.clamp(-max, max))
}

/// Pushes a capsule of `radius` and `height` with its feet at `p` out of
/// `boxes`: only the component across a box's face changes, so the capsule
/// slides along walls. Exactly the steps of `docs/architecture.md` section
/// 11 (collision): a fixed 4 rounds over the boxes in pre-order, no early
/// exit, only exactly rounded operations.
fn resolve(boxes: &[BoxCollider], r: f32, h: f32, mut p: Vec3, circles: &[(Vec2, f32)]) -> Vec3 {
    for _ in 0..4 {
        for bx in boxes {
            if bx.y_max <= p.y || bx.y_min >= p.y + h {
                continue; // no overlap in height
            }
            let d = Vec2::new(p.x, p.z) - bx.center;
            let la = d.dot(bx.a);
            let lb = d.dot(bx.b);
            let qa = la.clamp(-bx.ha, bx.ha);
            let qb = lb.clamp(-bx.hb, bx.hb);
            let (na, nb);
            if la == qa && lb == qb {
                // Center inside the box: out through the nearest side.
                if bx.ha - la.abs() <= bx.hb - lb.abs() {
                    na = (if la >= 0.0 { 1.0 } else { -1.0 }) * (bx.ha + r);
                    nb = lb;
                } else {
                    na = la;
                    nb = (if lb >= 0.0 { 1.0 } else { -1.0 }) * (bx.hb + r);
                }
            } else {
                let off = Vec2::new(la - qa, lb - qb);
                let d2 = off.length_squared();
                if d2 >= r * r {
                    continue; // not touching
                }
                let n = Vec2::new(qa, qb) + off * (r / d2.sqrt());
                na = n.x;
                nb = n.y;
            }
            let push = bx.a * (na - la) + bx.b * (nb - lb);
            p.x += push.x;
            p.z += push.y;
        }
        // Then out of each circle `(center, reach)` on the XZ plane: along
        // the line from its center, or +X from on top of it.
        for &(c, reach) in circles {
            let d = Vec2::new(p.x - c.x, p.z - c.y);
            let d2 = d.length_squared();
            if d2 >= reach * reach {
                continue; // not touching
            }
            if d2 == 0.0 {
                p.x = c.x + reach;
                p.z = c.y;
            } else {
                let n = d * (reach / d2.sqrt());
                p.x = c.x + n.x;
                p.z = c.y + n.y;
            }
        }
    }
    p
}

/// Of this tick's `presses`, the ones `next_action` outranked: all of
/// them when it took the buffered action, else all but the first.
fn outranked(presses: &[Buffered], from_buffer: bool) -> &[Buffered] {
    if from_buffer {
        presses
    } else {
        presses.get(1..).unwrap_or_default()
    }
}

/// Wraps an angle into `[-π, π)`. `floor` is exactly rounded, so this is
/// identical everywhere; for `a` already in range it returns `a` unchanged.
fn wrap(a: f32) -> f32 {
    a - TAU * ((a + PI) / TAU).floor()
}

/// `-0.0` becomes `0.0`: an unrotated character gives `atan2f(-0.0, 1.0)`.
fn positive_zero(a: f32) -> f32 {
    if a == 0.0 { 0.0 } else { a }
}

#[cfg(test)]
#[path = "enemy_tests.rs"]
mod enemy_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_scene;
    use loom_core::input::TickInput;
    use std::f32::consts::{FRAC_PI_3, FRAC_PI_4};

    type Axes = Vec<(&'static str, f32)>;

    /// Runs `ticks` more ticks; `axes(tick)` gives the axis values of each
    /// tick (1-based, counting from the game's current tick).
    fn advance(game: &mut Game, ticks: u64, axes: impl Fn(u64) -> Axes) {
        let mut input = ActionState::new();
        let first = game.clock().tick() + 1;
        for tick in first..first + ticks {
            input.advance(TickInput {
                actions: Default::default(),
                axes: axes(tick)
                    .into_iter()
                    .map(|(a, v)| (a.to_owned(), v))
                    .collect(),
            });
            game.tick(&input);
        }
    }

    /// Runs `ticks` ticks of the demo scene.
    fn run(ticks: u64, axes: impl Fn(u64) -> Axes) -> Game {
        run_from(Vec3::ZERO, ticks, axes)
    }

    /// Like [`run`], with the player starting at `start`.
    fn run_from(start: Vec3, ticks: u64, axes: impl Fn(u64) -> Axes) -> Game {
        let mut scene = demo_scene();
        let player = scene.character().unwrap();
        scene.transform_mut(player).position = start;
        let mut game = Game::new(scene).unwrap();
        advance(&mut game, ticks, axes);
        game
    }

    fn bits3(v: Vec3) -> [u32; 3] {
        v.to_array().map(f32::to_bits)
    }

    fn bits4(q: Quat) -> [u32; 4] {
        q.to_array().map(f32::to_bits)
    }

    fn assert_vec3(actual: Vec3, expected: [f32; 3]) {
        assert_eq!(
            bits3(actual),
            expected.map(f32::to_bits),
            "{actual} != {expected:?}"
        );
    }

    fn assert_quat(actual: Quat, expected: [f32; 4]) {
        assert_eq!(
            bits4(actual),
            expected.map(f32::to_bits),
            "{actual} != {expected:?}"
        );
    }

    fn assert_f32(actual: f32, expected: f32) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{actual} != {expected}"
        );
    }

    fn node(game: &Game, path: &str) -> loom_scene::Transform {
        *game.scene().transform(game.scene().find(path).unwrap())
    }

    /// Checks the player against the state and the `player` node.
    fn assert_player(game: &Game, position: [f32; 3], facing: f32) {
        let state = game.state().player;
        assert_vec3(state.position, position);
        assert_f32(state.facing, facing);
        assert_vec3(node(game, "player").position, position);
    }

    fn assert_camera_node(game: &Game, position: [f32; 3], rotation: [f32; 4]) {
        let cam = node(game, "main_cam");
        assert_vec3(cam.position, position);
        assert_quat(cam.rotation, rotation);
    }

    const LEVEL_ROTATION: [f32; 4] = [-0.1305262, 0.0, 0.0, 0.9914449];

    #[test]
    fn standing_still_keeps_the_initial_pose() {
        let game = run(60, |_| vec![]);
        assert_player(&game, [0.0, 0.0, 0.0], 0.0);
        assert_f32(game.state().camera.yaw, 0.0);
        assert_f32(game.state().camera.pitch, -0.2617994);
        assert_camera_node(&game, [0.0, 2.5352764, 3.8637035], LEVEL_ROTATION);
        assert_quat(node(&game, "player").rotation, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn camera_is_placed_before_the_first_tick() {
        let game = Game::new(demo_scene()).unwrap();
        assert_camera_node(&game, [0.0, 2.5352764, 3.8637035], LEVEL_ROTATION);
    }

    #[test]
    fn running_forward_for_one_second_covers_four_meters() {
        let game = run(90, |t| if t <= 60 { vec![(MOVE_Y, 1.0)] } else { vec![] });
        assert_player(&game, [0.0, 0.0, -3.9999988], 0.0);
        assert_camera_node(&game, [0.0, 2.5352764, -0.13629532], LEVEL_ROTATION);
    }

    #[test]
    fn running_right_turns_the_player_to_face_right() {
        let game = run(30, |_| vec![(MOVE_X, 1.0)]);
        assert_player(&game, [2.0000007, 0.0, 0.0], -1.5707964);
        assert_quat(
            node(&game, "player").rotation,
            [0.0, -0.70710677, 0.0, 0.70710677],
        );
        assert_camera_node(&game, [2.0000007, 2.5352764, 3.8637035], LEVEL_ROTATION);
    }

    #[test]
    fn turn_is_still_in_progress_after_seven_ticks() {
        let game = run(7, |_| vec![(MOVE_X, 1.0)]);
        assert_f32(game.state().player.position.x, 0.46666667);
        assert_f32(game.state().player.facing, -1.4660766);
        assert_quat(
            node(&game, "player").rotation,
            [0.0, -0.6691306, 0.0, 0.7431448],
        );
    }

    #[test]
    fn turn_completes_on_the_eighth_tick() {
        let game = run(8, |_| vec![(MOVE_X, 1.0)]);
        assert_f32(game.state().player.position.x, 0.53333336);
        assert_f32(game.state().player.facing, -1.5707964);
    }

    #[test]
    fn running_while_turning_the_camera_curves_the_path() {
        let game = run(60, |t| {
            let mut axes = vec![(MOVE_Y, 1.0)];
            if t <= 30 {
                axes.push((LOOK_X, 0.04));
            }
            axes
        });
        assert_player(&game, [2.9577415, 0.0, -2.2566528], -1.1999999);
        assert_f32(game.state().camera.yaw, -1.1999999);
        assert_camera_node(
            &game,
            [-0.6433809, 2.5352762, -0.8566092],
            [-0.10772792, -0.55981183, -0.07370063, 0.8182748],
        );
        assert_quat(
            node(&game, "player").rotation,
            [0.0, -0.5646424, 0.0, 0.8253356],
        );
    }

    #[test]
    fn looking_up_to_pitch_max_pulls_the_camera_in_above_the_ground() {
        let game = run(60, |_| vec![(LOOK_Y, 0.04)]);
        assert_f32(game.state().camera.pitch, FRAC_PI_3); // 1.0471976, pitch_max 60°
        assert_f32(game.state().camera.distance, 1.5011107);
        // y is the collision radius above the ground.
        assert_camera_node(
            &game,
            [0.0, 0.20000005, 0.75055534],
            [0.5, 0.0, 0.0, 0.8660254],
        );
    }

    #[test]
    fn running_backwards_turns_around_to_minus_pi() {
        let game = run(20, |_| vec![(MOVE_Y, -1.0)]);
        assert_player(&game, [0.0, 0.0, 1.3333336], -PI); // -3.1415927
        assert_vec3(node(&game, "main_cam").position, [0.0, 2.5352764, 5.197037]);
    }

    fn turn_left_then_run(t: u64) -> Vec<(&'static str, f32)> {
        if t <= 40 {
            vec![(LOOK_X, -0.1)]
        } else {
            vec![(MOVE_Y, 1.0)]
        }
    }

    #[test]
    fn turning_left_past_pi_wraps_the_camera_yaw() {
        let game = run(40, turn_left_then_run);
        assert_f32(game.state().camera.yaw, -2.2831872);
        assert_camera_node(
            &game,
            [-2.924056, 2.5352764, -2.5254908],
            [-0.05431796, -0.9015187, -0.11868719, 0.41258582],
        );
    }

    #[test]
    fn running_after_a_wrapped_yaw_follows_the_camera() {
        let game = run(70, turn_left_then_run);
        assert_player(&game, [1.5136027, 0.0, 1.3072904], -2.2831872);
        assert_vec3(
            node(&game, "main_cam").position,
            [-1.4104533, 2.5352764, -1.2182003],
        );
        assert_quat(
            node(&game, "player").rotation,
            [0.0, -0.9092978, 0.0, 0.416146],
        );
    }

    /// Steps 1-5 as they were in G1, before G2.1 added the roll key,
    /// copied here as the reference: `(position, facing, yaw, pitch)`.
    fn g1_reference_tick(game: &Game, s: &mut (Vec3, f32, f32, f32), input: &ActionState) {
        let (position, facing, yaw, pitch) = s;
        *yaw = wrap(*yaw - input.axis(LOOK_X));
        *pitch = (*pitch + input.axis(LOOK_Y)).clamp(game.pitch_min, game.pitch_max);
        let mut v = Vec2::new(input.axis(MOVE_X), input.axis(MOVE_Y));
        if v.length_squared() > 1.0 {
            v = v / v.length();
        }
        let (sn, cs) = (libm::sinf(*yaw), libm::cosf(*yaw));
        let forward = Vec3::new(-sn, 0.0, -cs);
        let right = Vec3::new(cs, 0.0, -sn);
        let dir = right * v.x + forward * v.y;
        *position = resolve(
            &game.boxes,
            game.radius,
            game.height,
            *position + dir * game.step,
            &[],
        );
        if v.length_squared() > 0.0 {
            let target = libm::atan2f(-dir.x, -dir.z);
            let diff = wrap(target - *facing);
            *facing = wrap(*facing + diff.clamp(-game.max_turn, game.max_turn));
        }
    }

    #[test]
    fn without_the_roll_key_every_tick_matches_g1_bit_for_bit() {
        // A copy of the old code rather than stored values: it checks every
        // tick, not just the end, with pillars and the slanted wall in the
        // way, and needs no numbers from before the change.
        let mut game = Game::new(demo_scene()).unwrap();
        let start = game.state();
        let mut reference = (
            start.player.position,
            start.player.facing,
            start.camera.yaw,
            start.camera.pitch,
        );
        let mut input = ActionState::new();
        for t in 1..=120_u32 {
            let axes: Axes = match t {
                1..=40 => vec![(MOVE_Y, 1.0), (LOOK_X, 0.01)],
                41..=80 => vec![(MOVE_X, -0.6), (MOVE_Y, 0.8), (LOOK_Y, -0.02)],
                _ => vec![(MOVE_X, 1.0), (MOVE_Y, -1.0), (LOOK_X, -0.03)],
            };
            input.advance(TickInput {
                actions: Default::default(),
                axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
            });
            game.tick(&input);
            g1_reference_tick(&game, &mut reference, &input);
            let s = game.state();
            let actual = (
                s.player.position,
                s.player.facing,
                s.camera.yaw,
                s.camera.pitch,
            );
            assert_eq!(bits3(actual.0), bits3(reference.0), "position at tick {t}");
            for (a, r) in [
                (actual.1, reference.1),
                (actual.2, reference.2),
                (actual.3, reference.3),
            ] {
                assert_eq!(a.to_bits(), r.to_bits(), "tick {t}");
            }
            assert_eq!(s.player.action, Action::Idle);
        }
        assert_ne!(reference.0, start.player.position);
    }

    /// The input of [`without_attack_keys_every_tick_matches_g2_1`] at
    /// `tick`: whether the roll key is held, and the axes.
    fn g2_1_reference_input(tick: u32) -> (bool, Axes) {
        let roll = matches!(
            tick,
            41 | 90..=150 | 170 | 200 | 204 | 208 | 212 | 216 | 262 | 300..=330
        );
        let axes = match tick {
            1..=60 => vec![(MOVE_Y, 1.0), (LOOK_X, 0.01)],
            61..=160 => vec![(MOVE_X, -0.6), (MOVE_Y, 0.8), (LOOK_Y, -0.02)],
            161..=199 => vec![(LOOK_X, -0.03)],
            200..=260 => vec![(MOVE_X, 1.0), (MOVE_Y, -1.0)],
            _ => vec![(MOVE_Y, 1.0), (LOOK_X, 0.02)],
        };
        (roll, axes)
    }

    /// FNV-1a over the bits of every field the state dump had in G2.1.
    fn g2_1_fingerprint(hash: &mut u64, s: &GameState) {
        let p = &s.player;
        let words = [
            p.position.x.to_bits(),
            p.position.y.to_bits(),
            p.position.z.to_bits(),
            p.facing.to_bits(),
            p.stamina.to_bits(),
            p.stamina_wait,
            p.action as u32,
            p.action_tick,
            u32::from(p.invulnerable),
            s.camera.yaw.to_bits(),
            s.camera.pitch.to_bits(),
            s.camera.distance.to_bits(),
        ];
        for byte in words.iter().flat_map(|w| w.to_le_bytes()) {
            *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    #[test]
    fn without_attack_keys_every_tick_matches_g2_1() {
        // The expected hash was computed by this same test on the G2.1 code
        // (commit 6b9c77a), before attacks existed: it covers every tick
        // of a run with rolls, backsteps, sprints, taps without stamina
        // and regeneration, with pillars and the slanted wall in the way.
        let src = demo_with(r#"("stamina_max", 100)"#, r#"("stamina_max", 40)"#);
        let mut game = Game::from_source(&src).unwrap();
        let mut input = ActionState::new();
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut seen = Vec::new();
        let mut taps_without_stamina = 0;
        for t in 1..=400 {
            let (roll, axes) = g2_1_reference_input(t);
            let released = !roll && g2_1_reference_input(t - 1).0;
            input.advance(TickInput {
                actions: if roll {
                    [ROLL.to_owned()].into()
                } else {
                    Default::default()
                },
                axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
            });
            game.tick(&input);
            let s = game.state();
            g2_1_fingerprint(&mut hash, &s);
            let p = &s.player;
            assert_eq!((p.attack_phase, p.combo, p.buffered), (None, 0, None));
            if !seen.contains(&s.player.action) {
                seen.push(s.player.action);
            }
            if released && s.player.stamina == 0.0 && s.player.action == Action::Idle {
                taps_without_stamina += 1;
            }
        }
        assert!(taps_without_stamina > 0);
        assert_eq!(
            seen,
            [Action::Idle, Action::Roll, Action::Sprint, Action::Backstep]
        );
        assert_eq!(hash, 309_999_793_288_603_552);
    }

    /// The input of [`without_the_lock_key_every_tick_matches_g2_3`] at
    /// `tick`: the actions held, and the axes.
    fn g2_3_reference_input(tick: u32) -> (Vec<&'static str>, Axes) {
        let mut actions = Vec::new();
        if matches!(tick, 100..=150 | 175 | 210 | 230 | 262 | 300..=330) {
            actions.push(ROLL);
        }
        if matches!(tick, 5 | 20 | 50 | 340 | 345) {
            actions.push(ATTACK);
        }
        if matches!(tick, 200 | 360) {
            actions.push(HEAVY);
        }
        let axes = match tick {
            1..=60 => vec![(MOVE_Y, 1.0), (LOOK_X, 0.01)],
            61..=160 => vec![(MOVE_X, -0.6), (MOVE_Y, 0.8), (LOOK_Y, -0.02)],
            161..=199 => vec![(LOOK_X, -0.03)],
            200..=260 => vec![(MOVE_X, 1.0), (MOVE_Y, -1.0)],
            _ => vec![(MOVE_Y, 1.0), (LOOK_X, 0.02)],
        };
        (actions, axes)
    }

    /// FNV-1a over the bits of every field the state dump had in G2.3.
    fn g2_3_fingerprint(hash: &mut u64, s: &GameState) {
        let p = &s.player;
        let words = [
            p.position.x.to_bits(),
            p.position.y.to_bits(),
            p.position.z.to_bits(),
            p.facing.to_bits(),
            p.stamina.to_bits(),
            p.stamina_wait,
            p.action as u32,
            p.action_tick,
            u32::from(p.invulnerable),
            p.attack_phase.map_or(0, |a| a as u32 + 1),
            p.combo,
            p.buffered.map_or(0, |b| b as u32 + 1),
            s.camera.yaw.to_bits(),
            s.camera.pitch.to_bits(),
            s.camera.distance.to_bits(),
        ];
        for byte in words.iter().flat_map(|w| w.to_le_bytes()) {
            *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    #[test]
    fn without_the_lock_key_every_tick_matches_g2_3() {
        // The expected hash was computed by this same test on the G2.3 code
        // (commit b450286), before lock-on existed, on the G2.3 demo scene:
        // it covers every tick of a run with a light combo, heavy attacks,
        // buffered and dropped presses, rolls, a backstep and sprints, with
        // pillars and the slanted wall in the way.
        let mut game = Game::from_source(G2_3_DEMO).unwrap();
        let mut input = ActionState::new();
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut seen = Vec::new();
        let mut max_combo = 0;
        let mut buffered = 0;
        for t in 1..=400 {
            let (actions, axes) = g2_3_reference_input(t);
            input.advance(TickInput {
                actions: actions.into_iter().map(str::to_owned).collect(),
                axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
            });
            game.tick(&input);
            let s = game.state();
            g2_3_fingerprint(&mut hash, &s);
            if !seen.contains(&s.player.action) {
                seen.push(s.player.action);
            }
            max_combo = max_combo.max(s.player.combo);
            buffered += u32::from(s.player.buffered.is_some());
        }
        assert_eq!(
            seen,
            [
                Action::Idle,
                Action::Attack,
                Action::Sprint,
                Action::Backstep,
                Action::Heavy,
                Action::Roll
            ]
        );
        assert_eq!(max_combo, 3);
        assert!(buffered > 0);
        assert_eq!(hash, 12_318_382_315_914_199_701);
    }

    /// The input of [`hits_leave_player_camera_and_lock_as_in_g3_1`] at
    /// `tick`: the actions held, and the axes. Locks on to dummy_b, walks
    /// up to it, a buffered three-hit light combo and a buffered heavy
    /// attack; then flicks over to dummy_c, walks up to it, and a light and
    /// a heavy attack on it.
    fn g3_1_reference_input(tick: u32) -> (Vec<&'static str>, Axes) {
        let mut actions = Vec::new();
        if tick == 1 {
            actions.push(LOCK);
        }
        if matches!(tick, 100 | 115 | 145 | 340) {
            actions.push(ATTACK);
        }
        if matches!(tick, 175 | 380) {
            actions.push(HEAVY);
        }
        let axes = match tick {
            2..=95 | 260..=330 => vec![(MOVE_Y, 1.0)],
            250 => vec![(LOOK_X, 0.12)],
            _ => vec![],
        };
        (actions, axes)
    }

    /// FNV-1a over the bits of every `player` and `camera` field and the
    /// `lock` path.
    fn g3_1_fingerprint(hash: &mut u64, s: &GameState) {
        let p = &s.player;
        let words = [
            p.position.x.to_bits(),
            p.position.y.to_bits(),
            p.position.z.to_bits(),
            p.facing.to_bits(),
            p.stamina.to_bits(),
            p.stamina_wait,
            p.action as u32,
            p.action_tick,
            u32::from(p.invulnerable),
            p.attack_phase.map_or(0, |a| a as u32 + 1),
            p.combo,
            p.buffered.map_or(0, |b| b as u32 + 1),
            s.camera.yaw.to_bits(),
            s.camera.pitch.to_bits(),
            s.camera.distance.to_bits(),
        ];
        let lock = match &s.lock {
            Some(path) => path.as_bytes(),
            None => &[0xfe],
        };
        let bytes = words.iter().flat_map(|w| w.to_le_bytes());
        for byte in bytes.chain(lock.iter().copied()).chain([0xff]) {
            *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    #[test]
    fn hits_leave_player_camera_and_lock_as_in_g3_1() {
        // The expected hash was computed by this same test on the G3.1 code
        // (commit 2c5cfc0), before hits existed, on the demo scene with its
        // dummies: it covers every tick of attacks that hit two of them.
        let mut game = Game::new(demo_scene()).unwrap();
        let mut input = ActionState::new();
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut locks = Vec::new();
        let mut max_combo = 0;
        for t in 1..=450 {
            let (actions, axes) = g3_1_reference_input(t);
            input.advance(TickInput {
                actions: actions.into_iter().map(str::to_owned).collect(),
                axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
            });
            game.tick(&input);
            let s = game.state();
            g3_1_fingerprint(&mut hash, &s);
            if locks.last() != Some(&s.lock) {
                locks.push(s.lock.clone());
            }
            max_combo = max_combo.max(s.player.combo);
        }
        assert_eq!(
            locks,
            [Some("dummy_b".to_owned()), Some("dummy_c".to_owned())]
        );
        assert_eq!(max_combo, 3);
        assert_eq!(game.state().player.action, Action::Idle);
        // The attacks did hit: dummy_b and dummy_c, never dummy_a.
        let hits: Vec<_> = game
            .state()
            .hurtboxes
            .iter()
            .map(|h| (h.node.clone(), h.hits))
            .collect();
        assert_eq!(hits[0], ("dummy_a".to_owned(), 0));
        assert!(hits[1].0 == "dummy_b" && hits[1].1 > 0, "{hits:?}");
        assert!(hits[2].0 == "dummy_c" && hits[2].1 > 0, "{hits:?}");
        assert_eq!(hash, 4_107_292_735_894_931_732);
    }

    /// FNV-1a over [`g3_1_fingerprint`]'s fields, then every hurtbox's
    /// `hits`, `last_hit` and `health`, then the transforms of the three
    /// dummies and their hp bars.
    fn g3_3_fingerprint(hash: &mut u64, game: &Game) {
        let s = game.state();
        g3_1_fingerprint(hash, &s);
        let mut words = Vec::new();
        for h in &s.hurtboxes {
            words.push(h.hits);
            match h.last_hit {
                Some(hit) => words.extend([
                    1,
                    hit.tick as u32,
                    (hit.tick >> 32) as u32,
                    hit.attack as u32,
                    hit.combo,
                ]),
                None => words.push(0),
            }
            match h.health {
                Some(health) => words.extend([
                    1,
                    health.hp.to_bits(),
                    health.state as u32,
                    health.state_ticks,
                    health.impact.to_bits(),
                    health.impact_wait,
                ]),
                None => words.push(0),
            }
        }
        for dummy in ["dummy_a", "dummy_b", "dummy_c"] {
            for path in [dummy.to_owned(), format!("{dummy}/hp_bar")] {
                let t = node(game, &path);
                words.extend(bits3(t.position));
                words.extend(bits4(t.rotation));
                words.extend(bits3(t.scale));
            }
        }
        for byte in words.iter().flat_map(|w| w.to_le_bytes()) {
            *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    #[test]
    fn without_swingers_every_tick_matches_g3_3() {
        // The expected hash was computed by this same test on the G3.3 code
        // (commit d4e8080), before swingers existed, on the G3.3 demo scene:
        // it covers every tick of attacks that hit, stagger and hurt two
        // dummies, with their poses and hp bars.
        let mut game = Game::from_source(G3_3_DEMO).unwrap();
        let mut input = ActionState::new();
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut staggers = 0;
        for t in 1..=450 {
            let (actions, axes) = g3_1_reference_input(t);
            input.advance(TickInput {
                actions: actions.into_iter().map(str::to_owned).collect(),
                axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
            });
            game.tick(&input);
            g3_3_fingerprint(&mut hash, &game);
            staggers += game
                .state()
                .hurtboxes
                .iter()
                .filter(|h| h.health.is_some_and(|x| x.state == Condition::Stagger))
                .count();
        }
        assert!(staggers > 0);
        let hits: Vec<_> = game.state().hurtboxes.iter().map(|h| h.hits).collect();
        assert!(hits[0] == 0 && hits[1] > 0 && hits[2] > 0, "{hits:?}");
        assert_eq!(hash, 17_394_125_667_144_692_205);
    }

    /// The demo scene as it was in G3.3, before the stake: tests whose
    /// results the stake would change run on it.
    const G3_3_DEMO: &str = include_str!("g3_3_demo.scene.ron");

    /// The demo scene as it was in G2.3, before the lock-on dummies:
    /// tests whose results the dummies would change run on it.
    const G2_3_DEMO: &str = include_str!("g2_3_demo.scene.ron");

    /// The demo scene with `player`'s line replaced.
    fn demo_with(old: &str, new: &str) -> String {
        let src = include_str!("demo.scene.ron");
        assert!(src.contains(old), "{old}");
        src.replace(old, new)
    }

    #[test]
    fn unrotated_character_faces_positive_zero() {
        let game = Game::new(demo_scene()).unwrap();
        assert_eq!(game.state().player.facing.to_bits(), 0);
    }

    #[test]
    fn rotated_character_starts_facing_its_rotation() {
        let src = demo_with(
            "position: (0.0, 0.0, 0.0),",
            "position: (0.0, 0.0, 0.0), rotation: (0.0, 90.0, 0.0),",
        );
        let game = Game::from_source(&src).unwrap();
        // Not bit-exact: `-f.z` is a tiny non-zero number here.
        assert!((game.state().player.facing - PI / 2.0).abs() <= 1e-6);
    }

    #[test]
    fn game_rejects_a_scene_without_a_character() {
        // Without a character the orbit camera cannot exist either: drop both.
        let src = demo_with("character: (run_speed: 4.0, turn_speed: 720.0),", "");
        let start = src.find(",\n            orbit_camera: (").unwrap();
        let end = src.find("return_speed: 4.0)").unwrap() + "return_speed: 4.0)".len();
        let src = format!("{}{}", &src[..start], &src[end..]);
        let err = Game::from_source(&src).unwrap_err();
        assert!(matches!(err, GameError::NoCharacter), "{err}");
        assert!(err.to_string().contains("character component"), "{err}");
    }

    #[test]
    fn game_rejects_an_active_camera_without_orbit_camera() {
        let src = demo_with("camera: \"main_cam\",", "camera: \"still_cam\",").replace(
            "(name: \"sun\",",
            "(name: \"still_cam\", camera: (fov_y: 60.0, near: 0.1, far: 50.0)),\n        (name: \"sun\",",
        );
        let err = Game::from_source(&src).unwrap_err();
        assert!(
            matches!(err, GameError::NoOrbitCamera(ref p) if p == "still_cam"),
            "{err}"
        );
        assert!(err.to_string().contains("orbit_camera"), "{err}");
    }

    #[test]
    fn game_rejects_a_character_that_is_not_a_root_node() {
        // Move the whole `player` subtree under a new root `squad`.
        let src = include_str!("demo.scene.ron");
        let start = src.find("        (name: \"player\"").unwrap();
        let end = src.find("        (name: \"pillars\"").unwrap();
        let player = &src[start..end];
        let src = format!(
            "{}        (name: \"squad\", children: [\n{}        ]),\n{}",
            &src[..start],
            player,
            &src[end..]
        )
        .replace("target: \"player\"", "target: \"squad/player\"");
        let err = Game::from_source(&src).unwrap_err();
        assert!(
            matches!(err, GameError::NotRoot(ref p) if p == "squad/player"),
            "{err}"
        );
        assert!(err.to_string().contains("root node"), "{err}");
    }

    #[test]
    fn game_rejects_an_orbit_camera_that_is_not_a_root_node() {
        let src = demo_with("camera: \"main_cam\",", "camera: \"rig/main_cam\",").replace(
            "        (name: \"main_cam\",",
            "        (name: \"rig\", children: [(name: \"main_cam\",",
        );
        // Close the extra `children: [(` after the camera node.
        let src = src.replace("return_speed: 4.0)),", "return_speed: 4.0))]),");
        let err = Game::from_source(&src).unwrap_err();
        assert!(
            matches!(err, GameError::NotRoot(ref p) if p == "rig/main_cam"),
            "{err}"
        );
    }

    // ---- collision (G1.3)

    #[test]
    fn running_into_a_pillar_stops_in_front_of_it() {
        // On the G2.3 scene: dummy_b now stands in front of pillar c.
        let mut game = Game::from_source(G2_3_DEMO).unwrap();
        advance(&mut game, 150, |_| vec![(MOVE_Y, 1.0)]);
        // Pillar c's face is at z = -8.5; plus the radius 0.35.
        assert_player(&game, [0.0, 0.0, -8.15], 0.0);
    }

    #[test]
    fn running_past_a_pillar_corner_slides_around_it() {
        let game = run(80, |_| vec![(MOVE_X, 1.0), (MOVE_Y, 1.0)]);
        assert_player(&game, [3.8095913, 0.0, -3.3155816], -FRAC_PI_4); // -0.7853982
    }

    #[test]
    fn running_sideways_into_a_pillar_stops_at_its_face() {
        let game = run_from(Vec3::new(1.0, 0.0, -4.0), 30, |_| vec![(MOVE_X, 1.0)]);
        assert_player(&game, [2.15, 0.0, -4.0], -1.5707964);
    }

    #[test]
    fn running_diagonally_along_a_pillar_slides_along_its_face() {
        let axes = |_| vec![(MOVE_X, 1.0), (MOVE_Y, 1.0)];
        let mut game = run_from(Vec3::new(1.0, 0.0, -3.0), 20, axes);
        // Not touching yet.
        assert_vec3(game.state().player.position, [1.9428096, 0.0, -3.9428072]);
        advance(&mut game, 10, axes);
        // x held at the face (2.5 - 0.35) while z keeps going.
        assert_vec3(game.state().player.position, [2.15, 0.0, -4.4142127]);
    }

    #[test]
    fn running_into_the_slanted_wall_slides_along_it() {
        let game = run(50, |_| vec![(MOVE_X, -1.0), (MOVE_Y, -1.0)]);
        assert_player(&game, [-2.5300741, 0.0, 2.035868], 2.3561945);
    }

    #[test]
    fn sliding_along_the_slanted_wall_keeps_the_center_on_its_face() {
        let mut game = run(39, |_| vec![(MOVE_X, -1.0), (MOVE_Y, -1.0)]);
        let wall = game.boxes[3];
        assert_eq!(
            game.scene().colliders()[3],
            game.scene().find("wall").unwrap()
        );
        for tick in 40..=60 {
            advance(&mut game, 1, |_| vec![(MOVE_X, -1.0), (MOVE_Y, -1.0)]);
            let p = game.state().player.position;
            let lb = (Vec2::new(p.x, p.z) - wall.center).dot(wall.b);
            // Half the wall's thickness (0.25) plus the radius (0.35).
            assert!((lb - (-0.6)).abs() <= 1e-6, "tick {tick}: lb = {lb}");
        }
    }

    #[test]
    fn character_starting_inside_a_pillar_is_pushed_out_on_the_first_tick() {
        let start = Vec3::new(3.2, 0.0, -4.1);
        let game = Game::new({
            let mut scene = demo_scene();
            let player = scene.character().unwrap();
            scene.transform_mut(player).position = start;
            scene
        })
        .unwrap();
        // Not at load time.
        assert_eq!(game.state().player.position, start);
        let game = run_from(start, 1, |_| vec![]);
        // Out through the shallowest side, +X.
        assert_vec3(game.state().player.position, [3.85, 0.0, -4.1]);
    }

    #[test]
    fn box_whose_top_is_at_the_feet_does_not_block() {
        let src = r#"(
            version: 1,
            camera: "cam",
            ambient: (0.2, 0.2, 0.2),
            nodes: [
                (name: "ground", mesh: Plane, scale: (40.0, 1.0, 40.0)),
                (name: "player", mesh: Capsule(radius: 0.35, height: 1.8),
                    character: (run_speed: 4.0, turn_speed: 720.0)),
                (name: "step", position: (0.0, -0.5, -2.0), scale: (2.0, 1.0, 2.0), collider: Box),
                (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 200.0),
                    orbit_camera: (target: "player", pivot_height: 1.5, distance: 4.0,
                        yaw: 0.0, pitch: -15.0, pitch_min: -70.0, pitch_max: 20.0,
                        collision_radius: 0.2, return_speed: 4.0)),
            ],
        )"#;
        let mut game = Game::from_source(src).unwrap();
        assert_eq!(game.boxes[0].y_max, 0.0);
        advance(&mut game, 60, |_| vec![(MOVE_Y, 1.0)]);
        assert_vec3(game.state().player.position, [0.0, 0.0, -3.9999988]);
    }

    #[test]
    fn camera_yaw_of_270_degrees_starts_wrapped() {
        let src = demo_with("yaw: 0.0, pitch: -15.0", "yaw: 270.0, pitch: -15.0");
        let game = Game::from_source(&src).unwrap();
        assert_f32(game.state().camera.yaw, -1.5707965);
    }

    #[test]
    fn wrap_maps_plus_pi_to_minus_pi() {
        assert_f32(wrap(PI), -PI);
        assert_f32(wrap(-PI), -PI);
    }

    #[test]
    fn game_rejects_a_character_without_a_capsule_mesh() {
        let src = demo_with("mesh: Capsule(radius: 0.35, height: 1.8),", "mesh: Cube,");
        let err = Game::from_source(&src).unwrap_err();
        assert!(
            matches!(err, GameError::NotACapsule(ref p) if p == "player"),
            "{err}"
        );
        assert!(err.to_string().contains("Capsule mesh"), "{err}");
        let src = demo_with("mesh: Capsule(radius: 0.35, height: 1.8),", "");
        assert!(matches!(
            Game::from_source(&src),
            Err(GameError::NotACapsule(_))
        ));
    }

    #[test]
    fn game_rejects_a_scaled_character() {
        let src = demo_with(
            "(name: \"player\", position: (0.0, 0.0, 0.0),",
            "(name: \"player\", position: (0.0, 0.0, 0.0), scale: (1.0, 2.0, 1.0),",
        );
        let err = Game::from_source(&src).unwrap_err();
        assert!(
            matches!(err, GameError::ScaledCharacter(ref p) if p == "player"),
            "{err}"
        );
        assert!(err.to_string().contains("scaled"), "{err}");
    }

    #[test]
    fn game_rejects_a_run_speed_too_fast_for_the_radius() {
        let src = demo_with("run_speed: 4.0,", "run_speed: 30.0,");
        let err = Game::from_source(&src).unwrap_err();
        assert!(matches!(err, GameError::TooFast { .. }), "{err}");
        assert_eq!(
            err.to_string(),
            "run_speed 30 is too fast for the capsule radius 0.35: \
             run_speed / 60 must be less than the radius"
        );
        // 21 / 60 = 0.35 exactly is still too fast; 4 (the demo) is fine.
        let src = demo_with("run_speed: 4.0,", "run_speed: 21.0,");
        assert!(matches!(
            Game::from_source(&src),
            Err(GameError::TooFast { .. })
        ));
        assert!(Game::new(demo_scene()).is_ok());
    }

    #[test]
    fn game_rejects_a_collider_that_moves_with_the_player() {
        let src = demo_with(
            "mesh: Cube, color: (0.9, 0.85, 0.3)),",
            "mesh: Cube, color: (0.9, 0.85, 0.3), collider: Box),",
        );
        let err = Game::from_source(&src).unwrap_err();
        assert!(
            matches!(err, GameError::MovingCollider(ref p) if p == "player/visor"),
            "{err}"
        );
        assert!(
            err.to_string().contains("colliders must be static"),
            "{err}"
        );
    }

    // ---- camera collision (G1.7)

    /// Pillar c is between the camera and a player starting here.
    const BEHIND_PILLAR_C: Vec3 = Vec3::new(0.0, 0.0, -11.0);

    #[test]
    fn camera_returns_to_its_distance_at_return_speed() {
        let axes = |t| match t {
            1..=30 => vec![(LOOK_Y, 0.04)],
            31..=45 => vec![(LOOK_Y, -0.08)],
            _ => vec![],
        };
        let mut game = run(45, axes);
        assert_f32(game.state().camera.distance, 2.6119118);
        advance(&mut game, 5, axes);
        assert_f32(game.state().camera.distance, 2.9452448);
        assert_vec3(
            node(&game, "main_cam").position,
            [0.0, 2.2622848, 2.8448882],
        );
        advance(&mut game, 20, axes);
        assert_f32(game.state().camera.distance, 4.0);
        assert_f32(game.state().camera.pitch, -0.26179916);
        assert_vec3(
            node(&game, "main_cam").position,
            [0.0, 2.5352755, 3.8637037],
        );
    }

    #[test]
    fn pillar_between_camera_and_player_pulls_the_camera_in_before_the_first_tick() {
        let game = run_from(BEHIND_PILLAR_C, 0, |_| vec![]);
        assert_eq!(game.clock().tick(), 0);
        assert_f32(game.state().camera.distance, 1.3458589);
        // Pillar c's face is at z = -9.5; plus the collision radius 0.2.
        assert_vec3(node(&game, "main_cam").position, [0.0, 1.848334, -9.7]);
    }

    #[test]
    fn camera_stays_in_while_blocked_and_returns_after() {
        let axes = |_| vec![(MOVE_X, 1.0)];
        let mut game = run_from(BEHIND_PILLAR_C, 1, axes);
        assert_f32(game.state().camera.distance, 1.3458589);
        advance(&mut game, 9, axes);
        assert_f32(game.state().camera.distance, 1.3458589);
        assert_f32(game.state().player.position.x, 0.6666667);
        advance(&mut game, 10, axes);
        assert_f32(game.state().camera.distance, 2.012526);
        advance(&mut game, 20, axes);
        assert_f32(game.state().camera.distance, 3.345858);
        assert_vec3(
            node(&game, "main_cam").position,
            [2.6666667, 2.3659718, -7.7681494],
        );
    }

    #[test]
    // 1.414214 is a position 4 ulps away from SQRT_2 (0x3FB504F7 vs
    // 0x3FB504F3), not √2.
    #[allow(clippy::approx_constant)]
    fn backing_into_the_slanted_wall_keeps_the_camera_in_front_of_it() {
        let axes = |_| vec![(MOVE_X, -1.0), (MOVE_Y, -1.0)];
        let mut game = run(30, axes);
        assert_vec3(game.state().player.position, [-1.414214, 0.0, 1.414214]);
        assert_f32(game.state().camera.distance, 0.15592998);
        assert_vec3(
            node(&game, "main_cam").position,
            [-1.414214, 1.5403577, 1.5648308],
        );
        advance(&mut game, 30, axes);
        assert_vec3(game.state().player.position, [-3.0877507, 0.0, 2.3578424]);
        assert_f32(game.state().camera.distance, 0.17931521);
        assert_vec3(
            node(&game, "main_cam").position,
            [-3.0877507, 1.5464102, 2.5310476],
        );
    }

    #[test]
    fn funnel_needs_more_than_one_push_round() {
        let src = demo_with(
            "        (name: \"sun\",",
            "        (name: \"funnel_l\", position: (-0.7, 1.0, -3.0), rotation: (0.0, 20.0, 0.0), \
             scale: (0.6, 2.0, 4.0), mesh: Cube, collider: Box),\n\
             (name: \"funnel_r\", position: (0.7, 1.0, -3.0), rotation: (0.0, -20.0, 0.0), \
             scale: (0.6, 2.0, 4.0), mesh: Cube, collider: Box),\n        (name: \"sun\",",
        );
        let mut game = Game::from_source(&src).unwrap();
        advance(&mut game, 30, |_| vec![(MOVE_Y, 1.0)]);
        assert_player(&game, [0.03972316, 0.0, -0.7395032], 0.0);
    }
}

#[cfg(test)]
#[path = "bonfire_tests.rs"]
mod bonfire_tests;

#[cfg(test)]
#[path = "flask_tests.rs"]
mod flask_tests;

#[cfg(test)]
#[path = "souls_tests.rs"]
mod souls_tests;
