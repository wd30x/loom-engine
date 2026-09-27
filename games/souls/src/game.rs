//! The game: a character that runs relative to an orbit camera and turns
//! smoothly towards where it runs. Every formula and its order is fixed by
//! `docs/architecture.md` section 11; changing either changes the state
//! dump.

use std::f32::consts::{PI, TAU};

use glam::{Quat, Vec2, Vec3};
use loom_core::SimClock;
use loom_core::input::ActionState;
use loom_scene::{MeshKind, NodeId, Scene, SceneError};
use serde::Serialize;

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

/// How a node prop's value is checked.
#[derive(Debug, Clone, Copy)]
enum NodeRule {
    /// Greater than 0.
    Positive,
    /// At least 0.
    NotNegative,
    /// A whole number of at least 1.
    Ticks,
}

/// The props a node other than the character may have, in the order they
/// are checked, with the default of those that have one:
/// - `lock_height` makes it a lock-on target (locked on this many meters
///   above its position);
/// - `hurt_radius` and `hurt_height` together make it a hurtbox;
/// - `hp` makes a hurtbox take damage; the last four go with `hp`.
const NODE_PROPS: [(&str, Option<f64>, NodeRule); 8] = [
    ("lock_height", None, NodeRule::Positive),
    ("hurt_radius", None, NodeRule::Positive),
    ("hurt_height", None, NodeRule::Positive),
    ("hp", None, NodeRule::Positive),
    ("poise", Some(0.0), NodeRule::NotNegative),
    ("stagger_ticks", Some(30.0), NodeRule::Ticks),
    ("poise_reset_ticks", Some(60.0), NodeRule::Ticks),
    ("revive_ticks", Some(180.0), NodeRule::Ticks),
];

/// The name of the child of a hurtbox with `hp` that shows its health.
const HP_BAR: &str = "hp_bar";

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
    #[error("the character {character:?}: prop {prop:?} must be greater than 0, got {value}")]
    NotPositive {
        character: String,
        prop: &'static str,
        value: f64,
    },
    #[error(
        "the character {character:?}: prop {prop:?} must be a whole number of at least {min}, \
         got {value}"
    )]
    NotWhole {
        character: String,
        prop: &'static str,
        min: u32,
        value: f64,
    },
    #[error(
        "the character {character:?}: prop {prop:?} must be at least 0 and less than 90, \
         got {value}"
    )]
    NotBelow90 {
        character: String,
        prop: &'static str,
        value: f64,
    },
    #[error(
        "the character {character:?}: prop {prop:?} must be greater than 0 and at most 360, \
         got {value}"
    )]
    NotAnArc {
        character: String,
        prop: &'static str,
        value: f64,
    },
    #[error("the character {character:?}: prop {prop:?} must be at least 0, got {value}")]
    Negative {
        character: String,
        prop: &'static str,
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
    #[error(
        "the node {node:?}: unknown prop {prop:?} (known: lock_height, hurt_radius, hurt_height, \
         hp, poise, stagger_ticks, poise_reset_ticks, revive_ticks)"
    )]
    UnknownNodeProp { node: String, prop: String },
    #[error("the node {node:?}: prop {prop:?} must be greater than 0, got {value}")]
    NodePropNotPositive {
        node: String,
        prop: &'static str,
        value: f64,
    },
    #[error("the node {node:?}: prop {prop:?} must be at least 0, got {value}")]
    NodePropNegative {
        node: String,
        prop: &'static str,
        value: f64,
    },
    #[error("the node {node:?}: prop {prop:?} must be a whole number of at least 1, got {value}")]
    NodePropNotWhole {
        node: String,
        prop: &'static str,
        value: f64,
    },
    #[error("the node {0:?}: needs both hurt_radius and hurt_height, or neither")]
    HurtPropsUnpaired(String),
    #[error("the node {0:?}: hp needs hurt_radius and hurt_height")]
    HpWithoutHurtbox(String),
    #[error("the node {node:?}: prop {prop:?} needs hp")]
    NeedsHp { node: String, prop: &'static str },
    #[error("the node {0:?}: its hp_bar needs a mesh")]
    HpBarWithoutMesh(String),
    #[error("the lock target {0:?} must be a root node, and not the camera")]
    LockTargetNotRoot(String),
    #[error("the hurtbox {0:?} must be a root node, and not the camera")]
    HurtboxNotRoot(String),
}

/// The game state in the state dump (`game`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameState {
    pub player: PlayerState,
    pub camera: CameraState,
    /// The path of the locked-on target; `None` when not locked on.
    pub lock: Option<String>,
    /// Every hurtbox, in pre-order.
    pub hurtboxes: Vec<HurtboxState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PlayerState {
    /// Feet position in meters; serialized as `[x, y, z]`.
    pub position: Vec3,
    /// Heading in radians around +Y, in `[-π, π)`; 0 faces -Z.
    pub facing: f32,
    pub stamina: f32,
    /// Ticks left before stamina starts coming back.
    pub stamina_wait: u32,
    pub action: Action,
    /// Which tick of a roll, backstep or attack this is, from 1; 0
    /// otherwise.
    pub action_tick: u32,
    pub invulnerable: bool,
    /// The part of the attack running; `None` outside attacks.
    pub attack_phase: Option<AttackPhase>,
    /// Which light attack of a combo this is, from 1; 0 otherwise.
    pub combo: u32,
    /// The action to start once the running attack is over.
    pub buffered: Option<Buffered>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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

/// [`GameState`] but the lock and the hurtboxes, which [`Game::state`] adds:
/// the lock as a path, the hurtboxes from [`Game`]'s own list.
#[derive(Debug, Clone, Copy)]
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
}

/// A hurtbox in the state dump (`game.hurtboxes`).
#[derive(Debug, Clone, PartialEq, Serialize)]
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

/// One hit on a hurtbox.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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
    /// targets: root nodes other than the camera. Places the camera for
    /// tick 0, already pulled in front of anything in the way; the
    /// character is not pushed out of colliders until the first tick.
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
        let (targets, hurtboxes) = targets(&scene, player, camera)?;
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
            },
            camera: CameraState {
                yaw: wrap(orbit.yaw_degrees.to_radians()),
                pitch: orbit.pitch_degrees.to_radians(),
                // Replaced by the first `place_camera` below.
                distance: orbit.distance,
            },
        };
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
        };
        game.place_camera(true);
        Ok(game)
    }

    /// Runs one tick with this tick's input.
    pub fn tick(&mut self, input: &ActionState) {
        self.clock.step();
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
        for health in self.hurtboxes.iter_mut().filter_map(|h| h.health.as_mut()) {
            health.count_down();
        }

        let mut player = self.state.player;
        let mut camera = self.state.camera;

        // 1a. The lock key, and letting go of a target too far away.
        let lock = params.lock;
        if input.just_pressed(LOCK) {
            self.lock = match self.lock {
                None => self.acquire(player.position, camera.yaw),
                Some(_) => None,
            };
            self.flick = 0.0;
            self.switch_wait = 0;
        } else if let Some(t) = self.lock
            && hdist(player.position, self.target_position(t)) > lock.break_range
        {
            self.lock = None;
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
        let mut busy = true;
        if matches!(player.action, Action::Roll | Action::Backstep) {
            let m = params.movement(player.action);
            player.action_tick += 1;
            player.position = resolve(
                &self.boxes,
                self.radius,
                self.height,
                player.position + self.action_dir * m.step,
            );
            // The last tick still moves.
            if player.action_tick == m.ticks {
                self.finished = (player.action, 0);
                player.action = Action::Idle;
                player.action_tick = 0;
            }
        } else if matches!(player.action, Action::Attack | Action::Heavy) {
            player.action_tick += 1;
            self.attack_tick(&mut player, &intent);
        } else if let Some((next, from_buffer)) = next_action(&mut player, &intent) {
            match next {
                Buffered::Roll => self.start_roll(&mut player, &intent),
                Buffered::Attack => {
                    self.start_attack(&mut player, Action::Attack, from_buffer, &intent);
                }
                Buffered::Heavy => {
                    self.start_attack(&mut player, Action::Heavy, from_buffer, &intent);
                }
            }
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
            );
            if sprinting {
                player.action = Action::Sprint;
                player.stamina = f32::max(player.stamina - params.sprint_drain, 0.0);
                player.stamina_wait = params.delay_ticks;
                if player.stamina == 0.0 {
                    self.spent = true;
                }
            } else {
                player.action = Action::Idle;
                busy = false;
            }

            // 5. Turn, at most `max_turn` per tick: towards the target while
            // locked on and not sprinting, otherwise towards `dir`.
            if let Some(t) = self.lock
                && player.action != Action::Sprint
            {
                let to = self.target_position(t);
                if hdist(player.position, to) > 0.0 {
                    player.facing =
                        turn(player.facing, bearing(player.position, to), self.max_turn);
                }
            } else if v.length_squared() > 0.0 {
                player.facing = turn(player.facing, libm::atan2f(-dir.x, -dir.z), self.max_turn);
            }
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
            Action::Idle | Action::Sprint | Action::Attack | Action::Heavy => false,
        };
        self.state = Motion { player, camera };

        // 6. Write the nodes.
        let transform = self.scene.transform_mut(self.player);
        transform.position = player.position;
        transform.rotation = Quat::from_rotation_y(player.facing);
        self.place_camera(false);
        self.show_hurtboxes();
    }

    /// Starts a roll the way the input points, or a backstep without a
    /// direction, and runs its first tick.
    fn start_roll(&mut self, player: &mut PlayerState, intent: &Intent) {
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
        let m = self.params.movement(player.action);
        player.stamina = f32::max(player.stamina - m.cost, 0.0);
        player.stamina_wait = self.params.delay_ticks;
        player.action_tick = 1;
        player.position = resolve(
            &self.boxes,
            self.radius,
            self.height,
            player.position + self.action_dir * m.step,
        );
        if m.ticks == 1 {
            self.finished = (player.action, 0);
            player.action = Action::Idle;
            player.action_tick = 0;
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
        let phase = if player.action_tick <= a.startup {
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
            if intent.input.just_pressed(HEAVY) {
                player.buffered = Some(Buffered::Heavy);
            } else if intent.input.just_pressed(ATTACK) {
                player.buffered = Some(Buffered::Attack);
            } else if intent.tap {
                player.buffered = Some(Buffered::Roll);
            }
        }
        if player.action_tick == a.startup + a.active + a.recovery {
            self.finished = (player.action, player.combo);
            player.action = Action::Idle;
            player.action_tick = 0;
            player.attack_phase = None;
            player.combo = 0;
        }
    }

    /// The hit check of active tick `action_tick - startup` of the attack
    /// `a`, after the slide: the tick sweeps its slice of the sector, right
    /// to left, and hits every hurtbox it touches that is not dead and this
    /// attack has not hit yet. A hurtbox with health takes the damage; one
    /// killed while locked on is let go of.
    fn hit(&mut self, player: &PlayerState, a: Attack) {
        let k = player.action_tick - a.startup;
        let half = a.arc * 0.5;
        let sweep = Sweep {
            p: player.position,
            facing: player.facing,
            reach: a.reach,
            lo: -half + (k - 1) as f32 * a.slice,
            hi: -half + k as f32 * a.slice,
            low: self.params.swing_low,
            high: self.params.swing_high,
        };
        let tick = self.clock.tick();
        for (i, h) in self.hurtboxes.iter_mut().enumerate() {
            if h.health.is_some_and(|health| health.is_dead()) {
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
                if let Some(health) = &mut h.health
                    && health.take(a.damage, a.impact, fall)
                    && self
                        .lock
                        .is_some_and(|t| self.targets[t].hurtbox == Some(i))
                {
                    self.lock = None;
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
    ///   hit (this one included), else the scene colors;
    /// - with health, its pose: the scene rotation, or when staggered or dead
    ///   tipped over by [`STAGGER_TILT_DEGREES`] or [`DEAD_TILT_DEGREES`]
    ///   towards its `fall`, away from the character;
    /// - its hp bar: as long as the health left, the left end in place,
    ///   hidden when dead.
    fn show_hurtboxes(&mut self) {
        let tick = self.clock.tick();
        for h in &self.hurtboxes {
            let condition = h.health.map(|health| health.now.state);
            let dead = condition == Some(Condition::Dead);
            let flashing = h.last_hit.is_some_and(|hit| tick - hit.tick < FLASH_TICKS);
            for &(id, color) in &h.meshes {
                let shown = if dead {
                    color * DEAD_DIM
                } else if flashing {
                    FLASH_COLOR
                } else {
                    color
                };
                self.scene
                    .set_mesh_color(id, shown)
                    .expect("meshes were picked for having a mesh, and every color is valid");
            }
            let Some(health) = h.health else {
                continue;
            };
            let tilt = match health.now.state {
                Condition::Ok => None,
                Condition::Stagger => Some(STAGGER_TILT_DEGREES),
                Condition::Dead => Some(DEAD_TILT_DEGREES),
            };
            self.scene.transform_mut(h.id).rotation = match tilt {
                None => h.base,
                // The top tips over towards `fall`, however the node faces.
                Some(degrees) => {
                    Quat::from_rotation_y(health.fall)
                        * Quat::from_rotation_x(-degrees.to_radians())
                        * Quat::from_rotation_y(-health.fall)
                        * h.base
                }
            };
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
        let player = self.state.player;
        let camera = self.state.camera;
        let rot = Quat::from_rotation_y(camera.yaw) * Quat::from_rotation_x(camera.pitch);
        let pivot = player.position + Vec3::new(0.0, self.pivot_height, 0.0);
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

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Game state for `--dump-state`.
    pub fn state(&self) -> GameState {
        GameState {
            player: self.state.player,
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

/// Every lock-on target and every hurtbox of `scene`, in pre-order, checked
/// node by node (every node but the character), in this order: no props
/// but [`NODE_PROPS`]; each value by its rule; the two hurt props together
/// or not at all; `hp` only on a hurtbox; the last four props only with
/// `hp`; only on root nodes other than the camera; an `hp_bar` with a mesh.
fn targets(
    scene: &Scene,
    player: NodeId,
    camera: NodeId,
) -> Result<(Vec<Target>, Vec<Hurtbox>), GameError> {
    let mut targets = Vec::new();
    let mut hurtboxes = Vec::new();
    for id in scene.ids().filter(|&id| id != player) {
        let node = scene.node(id);
        let path = || scene.path(id);
        if let Some((prop, _)) = node
            .props()
            .find(|&(p, _)| !NODE_PROPS.iter().any(|&(name, ..)| name == p))
        {
            return Err(GameError::UnknownNodeProp {
                node: path(),
                prop: prop.to_owned(),
            });
        }
        for (prop, _, rule) in NODE_PROPS {
            let Some(value) = node.prop(prop) else {
                continue;
            };
            let node = path();
            match rule {
                NodeRule::Positive if value <= 0.0 => {
                    return Err(GameError::NodePropNotPositive { node, prop, value });
                }
                NodeRule::NotNegative if value < 0.0 => {
                    return Err(GameError::NodePropNegative { node, prop, value });
                }
                NodeRule::Ticks if value.fract() != 0.0 || value < 1.0 => {
                    return Err(GameError::NodePropNotWhole { node, prop, value });
                }
                _ => {}
            }
        }
        // Present, or the default; `None` only for a missing prop without one.
        let get = |name: &str| {
            let &(_, default, _) = NODE_PROPS
                .iter()
                .find(|&&(n, ..)| n == name)
                .expect("only names from NODE_PROPS are asked for");
            node.prop(name).or(default)
        };
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
            && let Some(&(prop, ..)) = NODE_PROPS[4..]
                .iter()
                .find(|&&(p, ..)| node.prop(p).is_some())
        {
            return Err(GameError::NeedsHp { node: path(), prop });
        }
        if (lock_height.is_some() || hurt.is_some()) && (node.parent.is_some() || id == camera) {
            return Err(if lock_height.is_some() {
                GameError::LockTargetNotRoot(path())
            } else {
                GameError::HurtboxNotRoot(path())
            });
        }
        // Only a hurtbox with hp has an hp bar; anywhere else the name means
        // nothing.
        let bar = match hp {
            Some(_) => node
                .children
                .iter()
                .copied()
                .find(|&c| scene.node(c).name == HP_BAR),
            None => None,
        };
        if let Some(bar) = bar
            && scene.node(bar).mesh.is_none()
        {
            return Err(GameError::HpBarWithoutMesh(path()));
        }
        let hurtbox = hurt.map(|(radius, height)| {
            // Checked above: whole numbers of at least 1; `as` saturates.
            let ticks = |name: &str| get(name).expect("every tick prop has a default") as u32;
            let health = hp.map(|hp| {
                Health::new(
                    hp as f32,
                    get("poise").expect("poise has a default") as f32,
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
            }
        });
        if let Some(height) = lock_height {
            targets.push(Target {
                id,
                path: path(),
                height: height as f32,
                hurtbox: hurtbox.as_ref().map(|_| hurtboxes.len()),
            });
        }
        hurtboxes.extend(hurtbox);
    }
    Ok((targets, hurtboxes))
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
fn resolve(boxes: &[BoxCollider], r: f32, h: f32, mut p: Vec3) -> Vec3 {
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
    }
    p
}

/// `next_action` of step 4: the buffered action, else a new press (heavy,
/// then light, then a roll-key tap), with whether it was buffered. With no
/// stamina left there is none, and the input, buffered or not, is dropped.
fn next_action(player: &mut PlayerState, intent: &Intent) -> Option<(Buffered, bool)> {
    let next = if let Some(buffered) = player.buffered.take() {
        (buffered, true)
    } else if intent.input.just_pressed(HEAVY) {
        (Buffered::Heavy, false)
    } else if intent.input.just_pressed(ATTACK) {
        (Buffered::Attack, false)
    } else if intent.tap {
        (Buffered::Roll, false)
    } else {
        return None;
    };
    (player.stamina > 0.0).then_some(next)
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
        let p = s.player;
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
            let p = s.player;
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
        let p = s.player;
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
        let p = s.player;
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
