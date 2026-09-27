//! Why a scene is not a valid souls scene.

use loom_core::props::PropRule;
use loom_scene::SceneError;

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
    #[error("the node {node:?}: display_name has a character the HUD font lacks: {c:?}")]
    BossNameChar { node: String, c: char },
}
