//! The E0 "game": a cube whose orientation is a pure function of the tick
//! count. Shared by headless and window mode.

use std::f64::consts::TAU;

use glam::Mat4;
use loom_core::SimClock;

use crate::state::DemoState;

/// Fixed tilt around X so that three faces are visible.
const TILT_X: f32 = 0.3;
/// Spin speed around Y in radians per second.
const SPIN_RATE: f64 = 1.0;

#[derive(Debug, Default)]
pub(crate) struct CubeGame {
    clock: SimClock,
}

impl CubeGame {
    pub fn tick(&mut self) {
        self.clock.step();
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
        DemoState { yaw: self.yaw() }
    }

    /// `rotation_y(yaw) * rotation_x(TILT_X)`.
    pub fn model_matrix(&self) -> Mat4 {
        // yaw is wrapped in f64 before narrowing, so precision does not decay over time.
        Mat4::from_rotation_y(self.yaw() as f32) * Mat4::from_rotation_x(TILT_X)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_ticks_is_one_radian() {
        let mut game = CubeGame::default();
        for _ in 0..60 {
            game.tick();
        }
        assert_eq!(game.clock().sim_time(), 1.0);
        assert_eq!(
            game.model_matrix(),
            Mat4::from_rotation_y(1.0) * Mat4::from_rotation_x(0.3)
        );
    }

    #[test]
    fn yaw_wraps_past_a_full_turn() {
        let mut game = CubeGame::default();
        for _ in 0..400 {
            game.tick();
        }
        // 400 / 60 s = 6.67 rad, one full turn plus ~0.38 rad.
        assert_eq!(game.yaw().to_bits(), 0.38348135948708073f64.to_bits());
        assert_eq!(game.state().yaw.to_bits(), game.yaw().to_bits());
    }
}
