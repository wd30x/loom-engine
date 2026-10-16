//! The game: a character that runs relative to an orbit camera and turns
//! smoothly towards where it runs. Every formula and its order is fixed by
//! `docs/architecture.md` section 11; changing either changes the state
//! dump.

mod bonfire;
mod camera;
mod clips;
mod collision;
mod debug;
mod enemy_ai;
mod enemy_clips;
mod error;
mod hud;
mod player;
mod props;
mod roles;
mod show;
mod state;

use std::f32::consts::{PI, TAU};

use glam::{Quat, Vec2, Vec3};
use loom_core::SimClock;
use loom_core::input::ActionState;
use loom_scene::{MeshKind, NodeId, Scene};

use crate::enemy::EnemyState;
use crate::events::{DropReason, EndReason, Event, Input, LockReason};
use crate::health::{Condition, Health};
use crate::moves::{Action, Buffered, Params};

use collision::{BoxCollider, resolve};
use roles::{
    Bonfire, Enemy, Hurtbox, Initial, Respawn, Roles, Swinger, Target, is_in_subtree, roles,
};
use state::Motion;

pub use debug::DEBUG_CATEGORIES;
pub use error::GameError;
pub(crate) use props::NODE_PROPS;
pub(crate) use roles::{NODE_CONSTRAINTS, NODE_REQUIRES};
pub use state::{
    BloodstainState, CameraState, GameState, HurtboxState, LastHit, PlayerHit, PlayerState,
    RespawnState, SwingerState,
};

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

/// Ticks the health bar's trail stays where it was after a hit.
const HP_TRAIL_WAIT: u32 = 30;
/// Ticks the trail takes to fall the length of full health.
const HP_TRAIL_TICKS: f32 = 60.0;

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
    /// Every node with a mesh or a model in the character's subtree (the
    /// character included), in pre-order, with its color in the scene file
    /// and whether it is a model: what flashes and darkens.
    player_meshes: Vec<(NodeId, Vec3, bool)>,
    /// What picks the character's clips; `None` for a capsule mesh.
    clips: Option<clips::Clips>,
    /// The latest tick's movement input `v` and direction `dir` (steps 2
    /// and 3), for the clips.
    move_input: (Vec2, Vec3),
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
    /// capsule mesh, or a model with an animation of every clip in
    /// [`clips::CLIPS`] and a `character.capsule`, and an active camera that is a root node with an orbit
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
        let (radius, height) = capsule(&scene, player)?;
        let clips = match scene.node(player).model {
            Some(_) => Some(clips::Clips::new(&scene, player)?),
            None => None,
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
            .filter_map(|n| {
                let node = scene.node(n);
                node.mesh
                    .map(|m| (n, m.color, false))
                    .or(node.model.as_ref().map(|m| (n, m.color, true)))
            })
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
                hp_trail: params.hp,
                hp_trail_wait: 0,
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
            clips,
            move_input: (Vec2::ZERO, Vec3::ZERO),
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
        self.move_input = (v, dir);

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

        // 5d. The health bar's trail: level with any gain at once; after a
        // loss it waits, then falls the length of full health a second.
        let hp = player.health.hp;
        if hp >= player.hp_trail {
            player.hp_trail = hp;
            player.hp_trail_wait = 0;
        } else if player.hp_trail_wait > 0 {
            player.hp_trail_wait -= 1;
        } else {
            player.hp_trail = f32::max(hp, player.hp_trail - self.health.hp_max / HP_TRAIL_TICKS);
        }
        self.state = Motion { player, camera };

        // 6. Write the nodes.
        self.show_player();
        self.show_clip();
        self.show_bloodstain();
        self.place_camera(false);
        // An enemy stands turned to its facing.
        for e in &self.enemies {
            self.hurtboxes[e.hurtbox].base = Quat::from_rotation_y(e.facing);
        }
        self.show_hurtboxes();
        self.show_arms();
        self.show_enemy_clips();
    }

    pub fn clock(&self) -> SimClock {
        self.clock
    }

    /// The events of the latest tick, in the order they happened; none
    /// before the first tick.
    pub fn events(&self) -> &[Event] {
        &self.events
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
}

/// The character's capsule `(radius, height)`: its capsule mesh, or with a
/// model its `character.capsule` (`docs/architecture.md` section 11, "模型和
/// 动画").
fn capsule(scene: &Scene, player: NodeId) -> Result<(f32, f32), GameError> {
    let node = scene.node(player);
    let path = || scene.path(player);
    let character = node
        .character
        .expect("Scene::character only returns nodes with a character");
    match (character.capsule, &node.model) {
        (Some(_), _) if node.mesh.is_some() => Err(GameError::CapsuleAndMesh(path())),
        (Some(_), None) => Err(GameError::CapsuleWithoutModel(path())),
        (None, Some(_)) => Err(GameError::ModelWithoutCapsule(path())),
        (Some(_), Some(model)) if model.animation.is_none() => {
            Err(GameError::ModelWithoutAnimation(path()))
        }
        (Some(c), Some(_)) => Ok(c),
        (None, None) => match node.mesh.map(|m| m.kind) {
            Some(MeshKind::Capsule { radius, height }) => Ok((radius, height)),
            _ => Err(GameError::NotACapsule(path())),
        },
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
mod enemy_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod bonfire_tests;

#[cfg(test)]
mod flask_tests;

#[cfg(test)]
mod souls_tests;

#[cfg(test)]
mod hud_tests;

#[cfg(test)]
mod clips_tests;

#[cfg(test)]
mod enemy_clips_tests;
