//! The demo "game": a spinning cube that moves on the XZ plane when the
//! move actions are held. Shared by headless and window mode.

use std::f32::consts::FRAC_1_SQRT_2;
use std::f64::consts::TAU;

use glam::{Mat4, Vec3};
use loom_core::input::{ActionMap, ActionState};
use loom_core::{SimClock, TICK_HZ};

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

const DEMO_BINDINGS: &str = include_str!("demo_bindings.ron");

/// The demo's key bindings, embedded in the binary.
pub(crate) fn demo_bindings() -> ActionMap {
    ActionMap::parse(DEMO_BINDINGS)
        .expect("the embedded demo bindings are valid (checked by a unit test)")
}

#[derive(Debug, Default)]
pub(crate) struct CubeGame {
    clock: SimClock,
    position: Vec3,
}

impl CubeGame {
    /// Runs one tick with this tick's actions.
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
        self.position.x += dx * STEP;
        self.position.z += dz * STEP;
    }

    pub fn clock(&self) -> SimClock {
        self.clock
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
            position: self.position,
        }
    }

    /// `translation(position) * rotation_y(yaw) * rotation_x(TILT_X)`.
    pub fn model_matrix(&self) -> Mat4 {
        // yaw is wrapped in f64 before narrowing, so precision does not decay over time.
        Mat4::from_translation(self.position)
            * Mat4::from_rotation_y(self.yaw() as f32)
            * Mat4::from_rotation_x(TILT_X)
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

    #[test]
    fn sixty_ticks_is_one_radian() {
        let mut game = CubeGame::default();
        run(&mut game, 60, &[]);
        assert_eq!(game.clock().sim_time(), 1.0);
        assert_eq!(
            game.model_matrix(),
            Mat4::from_rotation_y(1.0) * Mat4::from_rotation_x(0.3)
        );
    }

    #[test]
    fn yaw_wraps_past_a_full_turn() {
        let mut game = CubeGame::default();
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
    // 0.707107 is the expected position after 30 ticks, not an approximation
    // of FRAC_1_SQRT_2 (0.70710677); the two differ in the last bits.
    #[allow(clippy::approx_constant)]
    fn diagonal_movement_is_scaled_by_one_over_root_two() {
        let mut game = CubeGame::default();
        run(&mut game, 30, &[MOVE_FORWARD, MOVE_RIGHT]);
        let p = game.state().position;
        assert_eq!(p.x.to_bits(), 0.707107f32.to_bits());
        assert_eq!(p.y.to_bits(), 0.0f32.to_bits());
        assert_eq!(p.z.to_bits(), (-0.707107f32).to_bits());
    }
}
