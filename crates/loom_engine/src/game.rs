//! The demo "game": a spinning cube, the scene's root node `cube`, that
//! moves on the XZ plane when the move actions are held. Shared by headless
//! and window mode.

use std::f32::consts::FRAC_1_SQRT_2;
use std::f64::consts::TAU;

use anyhow::{Result, bail};
use glam::Quat;
use loom_core::input::{ActionMap, ActionState};
use loom_core::{SimClock, TICK_HZ};
use loom_scene::{NodeId, Scene};

use crate::state::DemoState;

/// Fixed tilt around X so that three faces are visible.
const TILT_X: f32 = 0.3;
/// Spin speed around Y in radians per second.
const SPIN_RATE: f64 = 1.0;
/// Movement speed in meters per second.
const MOVE_SPEED: f64 = 2.0;
/// Distance moved per tick along one axis: `2.0 / 60.0` rounded to f32.
const STEP: f32 = (MOVE_SPEED / TICK_HZ as f64) as f32;

pub(crate) const MOVE_FORWARD: &str = "move_forward";
pub(crate) const MOVE_BACK: &str = "move_back";
pub(crate) const MOVE_LEFT: &str = "move_left";
pub(crate) const MOVE_RIGHT: &str = "move_right";

/// The root node the demo drives.
const CUBE: &str = "cube";

const DEMO_BINDINGS: &str = include_str!("demo_bindings.ron");
const DEMO_SCENE: &str = include_str!("demo.scene.ron");

/// The demo's key bindings, embedded in the binary.
pub(crate) fn demo_bindings() -> ActionMap {
    ActionMap::parse(DEMO_BINDINGS)
        .expect("the embedded demo bindings are valid (checked by a unit test)")
}

/// The scene used without `--scene`, embedded in the binary.
pub(crate) fn demo_scene() -> Scene {
    Scene::parse(DEMO_SCENE).expect("the embedded demo scene is valid (checked by a unit test)")
}

#[derive(Debug)]
pub(crate) struct CubeGame {
    clock: SimClock,
    scene: Scene,
    cube: NodeId,
}

impl CubeGame {
    /// Parses scene source text and starts a game on it.
    pub fn from_source(src: &str) -> Result<Self> {
        Self::new(Scene::parse(src)?)
    }

    /// Takes over `scene`, which must have a root node `cube` with a mesh.
    pub fn new(scene: Scene) -> Result<Self> {
        let cube = match scene.find(CUBE) {
            Some(id) if scene.node(id).parent.is_none() && scene.node(id).mesh.is_some() => id,
            _ => bail!("the demo scene needs a root node \"cube\" with a mesh"),
        };
        Ok(Self {
            clock: SimClock::new(),
            scene,
            cube,
        })
    }

    /// Runs one tick with this tick's actions: moves the cube from its
    /// current position, then sets its spin for the new time.
    pub fn tick(&mut self, actions: &ActionState) {
        self.clock.step();
        let axis = |action| if actions.pressed(action) { 1.0f32 } else { 0.0 };
        let mut dx = axis(MOVE_RIGHT) - axis(MOVE_LEFT);
        let mut dz = axis(MOVE_BACK) - axis(MOVE_FORWARD); // -Z is forward
        // Exactly this, not normalize(): the state must be bit-identical
        // everywhere, and the expected values in the tests follow this formula.
        if dx != 0.0 && dz != 0.0 {
            dx *= FRAC_1_SQRT_2;
            dz *= FRAC_1_SQRT_2;
        }
        let yaw = self.yaw();
        let transform = self.scene.transform_mut(self.cube);
        transform.position.x += dx * STEP;
        transform.position.z += dz * STEP;
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

    /// Game state for `--dump-state`.
    pub fn state(&self) -> DemoState {
        DemoState {
            yaw: self.yaw(),
            position: self.scene.transform(self.cube).position,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_core::input::ActionSet;

    fn run(game: &mut CubeGame, ticks: u32, active: &[&str]) {
        let mut actions = ActionState::new();
        for _ in 0..ticks {
            actions.advance(
                active
                    .iter()
                    .map(|a| (*a).to_owned())
                    .collect::<ActionSet>(),
            );
            game.tick(&actions);
        }
    }

    fn demo_game() -> CubeGame {
        CubeGame::new(demo_scene()).unwrap()
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
        assert_eq!(game.state().yaw.to_bits(), game.yaw().to_bits());
    }

    #[test]
    fn demo_bindings_define_exactly_the_demo_actions() {
        let map = demo_bindings();
        let mut actions: Vec<_> = map.actions().collect();
        actions.sort_unstable();
        assert_eq!(actions, [MOVE_BACK, MOVE_FORWARD, MOVE_LEFT, MOVE_RIGHT]);
    }

    #[test]
    fn built_in_scene_drives_a_cube_standing_on_the_ground() {
        let game = demo_game();
        assert_eq!(game.state().position, glam::Vec3::new(0.0, 0.5, 0.0));
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
    // 0.707107 is the expected position after 30 ticks, not an approximation
    // of FRAC_1_SQRT_2 (0.70710677); the two differ in the last bits.
    #[allow(clippy::approx_constant)]
    fn diagonal_movement_is_scaled_by_one_over_root_two() {
        let mut game = demo_game();
        run(&mut game, 30, &[MOVE_FORWARD, MOVE_RIGHT]);
        let p = game.state().position;
        assert_eq!(p.x.to_bits(), 0.707107f32.to_bits());
        assert_eq!(p.y.to_bits(), 0.5f32.to_bits());
        assert_eq!(p.z.to_bits(), (-0.707107f32).to_bits());
    }
}
