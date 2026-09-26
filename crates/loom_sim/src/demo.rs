//! The demo "game": a spinning cube, the scene's root node `cube`, that
//! moves on the XZ plane following the `move_x`/`move_y` axes. Shared by
//! headless and window mode.

use std::f64::consts::TAU;

use glam::{Quat, Vec2, Vec3};
use loom_core::input::{ActionMap, ActionState};
use loom_core::{SimClock, TICK_HZ};
use loom_scene::{NodeId, Scene, SceneError};

/// Fixed tilt around X so that three faces are visible.
const TILT_X: f32 = 0.3;
/// Spin speed around Y in radians per second.
const SPIN_RATE: f64 = 1.0;
/// Movement speed in meters per second.
const MOVE_SPEED: f64 = 2.0;
/// Distance moved per tick at full axis input: `2.0 / 60.0` rounded to f32.
const STEP: f32 = (MOVE_SPEED / TICK_HZ as f64) as f32;

/// Sideways movement in `[-1, 1]`; positive is +X.
pub const MOVE_X: &str = "move_x";
/// Forward movement in `[-1, 1]`; positive is forward (-Z).
pub const MOVE_Y: &str = "move_y";
/// Horizontal view turn in radians per tick (unused by the demo so far).
pub const LOOK_X: &str = "look_x";
/// Vertical view turn in radians per tick (unused by the demo so far).
pub const LOOK_Y: &str = "look_y";

/// The root node the demo drives.
const CUBE: &str = "cube";

const DEMO_BINDINGS: &str = include_str!("demo_bindings.ron");
const DEMO_SCENE: &str = include_str!("demo.scene.ron");

#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error(transparent)]
    Scene(#[from] SceneError),
    #[error("the demo scene needs a root node \"cube\" with a mesh")]
    MissingCube,
}

/// The demo's key bindings, embedded in the binary.
pub fn demo_bindings() -> ActionMap {
    ActionMap::parse(DEMO_BINDINGS)
        .expect("the embedded demo bindings are valid (checked by a unit test)")
}

/// The scene used without `--scene`, embedded in the binary.
pub fn demo_scene() -> Scene {
    Scene::parse(DEMO_SCENE).expect("the embedded demo scene is valid (checked by a unit test)")
}

#[derive(Debug)]
pub struct CubeGame {
    clock: SimClock,
    scene: Scene,
    cube: NodeId,
}

impl CubeGame {
    /// Parses scene source text and starts a game on it.
    pub fn from_source(src: &str) -> Result<Self, GameError> {
        Self::new(Scene::parse(src)?)
    }

    /// Takes over `scene`, which must have a root node `cube` with a mesh.
    pub fn new(scene: Scene) -> Result<Self, GameError> {
        let cube = match scene.find(CUBE) {
            Some(id) if scene.node(id).parent.is_none() && scene.node(id).mesh.is_some() => id,
            _ => return Err(GameError::MissingCube),
        };
        Ok(Self {
            clock: SimClock::new(),
            scene,
            cube,
        })
    }

    /// Runs one tick with this tick's input: moves the cube from its
    /// current position, then sets its spin for the new time.
    pub fn tick(&mut self, input: &ActionState) {
        self.clock.step();
        // Exactly this order and these types: the state must be
        // bit-identical everywhere, and the tests' expected values follow it.
        let mut v = Vec2::new(input.axis(MOVE_X), input.axis(MOVE_Y));
        if v.length_squared() > 1.0 {
            v = v / v.length();
        }
        let yaw = self.yaw();
        let transform = self.scene.transform_mut(self.cube);
        transform.position.x += v.x * STEP;
        transform.position.z -= v.y * STEP; // positive move_y is forward, -Z
        transform.rotation = Quat::from_rotation_y(yaw as f32) * Quat::from_rotation_x(TILT_X);
    }

    pub fn clock(&self) -> SimClock {
        self.clock
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Spin angle around Y in radians, wrapped into `[0, 2π)`. Simulation
    /// state: only exactly rounded operations, so it is identical everywhere.
    pub fn yaw(&self) -> f64 {
        (self.clock.sim_time() * SPIN_RATE) % TAU
    }

    /// The cube's position in meters.
    pub fn position(&self) -> Vec3 {
        self.scene.transform(self.cube).position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_core::input::TickInput;

    fn run(game: &mut CubeGame, ticks: u32, axes: &[(&str, f32)]) {
        let mut input = ActionState::new();
        for _ in 0..ticks {
            input.advance(TickInput {
                actions: Default::default(),
                axes: axes.iter().map(|(a, v)| ((*a).to_owned(), *v)).collect(),
            });
            game.tick(&input);
        }
    }

    fn demo_game() -> CubeGame {
        CubeGame::new(demo_scene()).unwrap()
    }

    fn assert_position_bits(game: &CubeGame, expected: [f32; 3]) {
        let p = game.position();
        assert_eq!(
            [p.x, p.y, p.z].map(f32::to_bits),
            expected.map(f32::to_bits),
            "{p:?} != {expected:?}"
        );
    }

    #[test]
    fn sixty_ticks_is_one_radian() {
        let mut game = demo_game();
        run(&mut game, 60, &[]);
        assert_eq!(game.clock().sim_time(), 1.0);
        let cube = game.scene().find(CUBE).unwrap();
        assert_eq!(
            game.scene().transform(cube).rotation,
            Quat::from_rotation_y(1.0) * Quat::from_rotation_x(0.3)
        );
    }

    #[test]
    fn yaw_wraps_past_a_full_turn() {
        let mut game = demo_game();
        run(&mut game, 400, &[]);
        // 400 / 60 s = 6.67 rad, one full turn plus ~0.38 rad.
        assert_eq!(game.yaw().to_bits(), 0.38348135948708073f64.to_bits());
    }

    #[test]
    fn demo_bindings_define_exactly_the_demo_axes() {
        let map = demo_bindings();
        assert_eq!(map.actions().count(), 0);
        let mut axes: Vec<_> = map.axes().collect();
        axes.sort_unstable();
        assert_eq!(axes, [LOOK_X, LOOK_Y, MOVE_X, MOVE_Y]);
    }

    #[test]
    fn built_in_scene_drives_a_cube_standing_on_the_ground() {
        let game = demo_game();
        assert_eq!(game.position(), Vec3::new(0.0, 0.5, 0.0));
    }

    #[test]
    fn game_rejects_a_scene_without_a_root_cube() {
        let cam = r#"(name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 10.0))"#;
        for nodes in [
            String::new(),
            r#"(name: "cube")"#.to_owned(),
            r#"(name: "box", children: [(name: "cube", mesh: Cube)])"#.to_owned(),
        ] {
            let src = format!(
                "(version: 1, camera: \"cam\", ambient: (0.2, 0.2, 0.2), nodes: [{cam}, {nodes}])"
            );
            let err = CubeGame::new(Scene::parse(&src).unwrap()).unwrap_err();
            assert!(err.to_string().contains("\"cube\""), "{nodes}: {err}");
        }
    }

    #[test]
    fn game_from_invalid_source_reports_the_scene_error() {
        let err = CubeGame::from_source("(version: 2)").unwrap_err();
        assert!(matches!(err, GameError::Scene(_)), "{err}");
    }

    #[test]
    fn full_move_x_for_sixty_ticks_moves_two_meters() {
        let mut game = demo_game();
        run(&mut game, 60, &[(MOVE_X, 1.0)]);
        assert_position_bits(&game, [1.9999994, 0.5, 0.0]);
    }

    #[test]
    // 0.707107 is the expected position after 30 ticks, not an approximation
    // of FRAC_1_SQRT_2 (0.70710677); the two differ in the last bits.
    #[allow(clippy::approx_constant)]
    fn diagonal_movement_is_scaled_by_one_over_root_two() {
        let mut game = demo_game();
        run(&mut game, 30, &[(MOVE_X, 1.0), (MOVE_Y, 1.0)]);
        assert_position_bits(&game, [0.707107, 0.5, -0.707107]);
    }

    #[test]
    fn move_vector_longer_than_one_is_normalized() {
        let mut game = demo_game();
        run(&mut game, 45, &[(MOVE_X, 1.0), (MOVE_Y, 0.5)]);
        assert_position_bits(&game, [1.3416408, 0.5, -0.6708204]);
    }

    #[test]
    fn move_vector_shorter_than_one_is_kept() {
        let mut game = demo_game();
        run(&mut game, 1, &[(MOVE_X, 0.5), (MOVE_Y, -0.5)]);
        let p = game.position();
        assert_eq!((p.x, p.z), (0.5 * STEP, 0.5 * STEP));
    }

    #[test]
    fn look_axes_do_not_move_the_cube() {
        let mut game = demo_game();
        run(&mut game, 10, &[(LOOK_X, 0.04), (LOOK_Y, -0.04)]);
        assert_position_bits(&game, [0.0, 0.5, 0.0]);
    }
}
