//! The E0 "game": a cube whose orientation is a pure function of the tick
//! count. Shared by headless and window mode.

use std::f64::consts::TAU;

use glam::Mat4;
use loom_core::SimClock;

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

    /// `rotation_y(sim_time * SPIN_RATE) * rotation_x(TILT_X)`.
    pub fn model_matrix(&self) -> Mat4 {
        // Wrap in f64 before narrowing so precision does not decay over time.
        let angle = (self.clock.sim_time() * SPIN_RATE) % TAU;
        Mat4::from_rotation_y(angle as f32) * Mat4::from_rotation_x(TILT_X)
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
}
