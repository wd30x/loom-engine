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

/// Sideways movement in `[-1, 1]`; positive is to the camera's right.
pub const MOVE_X: &str = "move_x";
/// Forward movement in `[-1, 1]`; positive is away from the camera.
pub const MOVE_Y: &str = "move_y";
/// Horizontal camera turn in radians per tick; positive turns right.
pub const LOOK_X: &str = "look_x";
/// Vertical camera turn in radians per tick; positive looks up.
pub const LOOK_Y: &str = "look_y";

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
        "run_speed {run_speed} is too fast for the capsule radius {radius}: \
         run_speed / 60 must be less than the radius"
    )]
    TooFast { run_speed: f32, radius: f32 },
    #[error("collider {0:?} moves with the player or the camera; colliders must be static")]
    MovingCollider(String),
}

/// The game state in the state dump (`game`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GameState {
    pub player: PlayerState,
    pub camera: CameraState,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PlayerState {
    /// Feet position in meters; serialized as `[x, y, z]`.
    pub position: Vec3,
    /// Heading in radians around +Y, in `[-π, π)`; 0 faces -Z.
    pub facing: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CameraState {
    /// Radians around +Y; 0 looks along -Z.
    pub yaw: f32,
    /// Radians; positive looks up.
    pub pitch: f32,
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

#[derive(Debug)]
pub struct Game {
    clock: SimClock,
    scene: Scene,
    player: NodeId,
    camera: NodeId,
    state: GameState,
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
    distance: f32,
}

impl Game {
    /// Parses scene source text and starts a game on it.
    pub fn from_source(src: &str) -> Result<Self, GameError> {
        Self::new(Scene::parse(src)?)
    }

    /// Takes over `scene`, which needs a root node with a character and a
    /// capsule mesh, and an active camera that is a root node with an orbit
    /// camera (its target is necessarily that character: a scene has at
    /// most one). Colliders must not move with either. Places the camera
    /// for tick 0; the character is not pushed out of colliders until the
    /// first tick.
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
        let boxes = scene
            .colliders()
            .iter()
            .map(|&id| BoxCollider::new(&scene, id))
            .collect();

        let transform = scene.transform(player);
        let f = transform.rotation * Vec3::NEG_Z;
        let state = GameState {
            player: PlayerState {
                position: transform.position,
                facing: positive_zero(wrap(libm::atan2f(-f.x, -f.z))),
            },
            camera: CameraState {
                yaw: wrap(orbit.yaw_degrees.to_radians()),
                pitch: orbit.pitch_degrees.to_radians(),
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
        };
        game.place_camera();
        Ok(game)
    }

    /// Runs one tick with this tick's input.
    pub fn tick(&mut self, input: &ActionState) {
        self.clock.step();
        let GameState { player, camera } = &mut self.state;

        // 1. Camera angles.
        camera.yaw = wrap(camera.yaw - input.axis(LOOK_X));
        camera.pitch = (camera.pitch + input.axis(LOOK_Y)).clamp(self.pitch_min, self.pitch_max);

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

        // 4. Move, then push the capsule out of the colliders.
        player.position = resolve(
            &self.boxes,
            self.radius,
            self.height,
            player.position + dir * self.step,
        );

        // 5. Turn towards `dir`, at most `max_turn` per tick.
        if v.length_squared() > 0.0 {
            let target = libm::atan2f(-dir.x, -dir.z);
            let diff = wrap(target - player.facing);
            player.facing = wrap(player.facing + diff.clamp(-self.max_turn, self.max_turn));
        }

        // 6. Write the nodes.
        let transform = self.scene.transform_mut(self.player);
        transform.position = player.position;
        transform.rotation = Quat::from_rotation_y(player.facing);
        self.place_camera();
    }

    /// Step 6 for the camera node: orbit the pivot above the player.
    fn place_camera(&mut self) {
        let GameState { player, camera } = self.state;
        let rot = Quat::from_rotation_y(camera.yaw) * Quat::from_rotation_x(camera.pitch);
        let transform = self.scene.transform_mut(self.camera);
        transform.position = player.position
            + Vec3::new(0.0, self.pivot_height, 0.0)
            + rot * Vec3::new(0.0, 0.0, self.distance);
        transform.rotation = rot;
    }

    pub fn clock(&self) -> SimClock {
        self.clock
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Game state for `--dump-state`.
    pub fn state(&self) -> GameState {
        self.state
    }
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
    use std::f32::consts::FRAC_PI_4;

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
    fn looking_up_stops_at_pitch_max() {
        let game = run(60, |_| vec![(LOOK_Y, 0.04)]);
        assert_f32(game.state().camera.pitch, 0.34906584);
        assert_camera_node(
            &game,
            [0.0, 0.1319195, 3.7587702],
            [0.17364818, 0.0, 0.0, 0.9848077],
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
        let end = src.find("pitch_max: 20.0)").unwrap() + "pitch_max: 20.0)".len();
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
        let src = src.replace(
            "pitch_min: -70.0, pitch_max: 20.0)),",
            "pitch_min: -70.0, pitch_max: 20.0))]),",
        );
        let err = Game::from_source(&src).unwrap_err();
        assert!(
            matches!(err, GameError::NotRoot(ref p) if p == "rig/main_cam"),
            "{err}"
        );
    }

    // ---- collision (G1.3)

    #[test]
    fn running_into_a_pillar_stops_in_front_of_it() {
        let game = run(150, |_| vec![(MOVE_Y, 1.0)]);
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
                        yaw: 0.0, pitch: -15.0, pitch_min: -70.0, pitch_max: 20.0)),
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
}
