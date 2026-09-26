//! Fixed-timestep simulation time. See `docs/architecture.md` section 4.
//!
//! Two pieces:
//! - [`FixedTimestep`] turns wall-clock frame deltas into a number of ticks to
//!   run (window mode only; headless mode runs exactly one tick per frame).
//! - [`SimClock`] counts ticks and derives simulation time from the count.

/// Simulation rate in ticks per second.
pub const TICK_HZ: u32 = 60;

/// Duration of one tick in seconds.
pub const TICK_DT: f64 = 1.0 / TICK_HZ as f64;

/// Upper bound on ticks run for a single frame. Time beyond this is dropped
/// so that a slow frame cannot make the next one even slower.
pub const MAX_TICKS_PER_FRAME: u32 = 8;

/// The accumulator counts time in integer units of `1 / (TICK_HZ * 1e9)` s,
/// so one tick is exactly `1e9` units and summing many small deltas never
/// drifts the way repeated `f64` addition would.
const UNITS_PER_TICK: u64 = 1_000_000_000;
const UNITS_PER_SECOND: f64 = UNITS_PER_TICK as f64 * TICK_HZ as f64;

/// Converts wall-clock frame time into a whole number of fixed ticks.
#[derive(Debug, Clone, Default)]
pub struct FixedTimestep {
    accumulator: u64,
}

impl FixedTimestep {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `dt` seconds of wall-clock time and returns how many ticks to run
    /// now, at most [`MAX_TICKS_PER_FRAME`]. When the cap is hit, whole ticks
    /// beyond it are discarded and only the sub-tick remainder is kept.
    ///
    /// Negative, NaN or infinite `dt` counts as zero.
    pub fn advance(&mut self, dt: f64) -> u32 {
        let units = if dt.is_finite() && dt > 0.0 {
            // Saturating float-to-int cast; rounding snaps values such as
            // 1/30 s or 1 ms onto the exact integer grid.
            (dt * UNITS_PER_SECOND).round() as u64
        } else {
            0
        };
        self.accumulator = self.accumulator.saturating_add(units);

        let due = self.accumulator / UNITS_PER_TICK;
        if due > u64::from(MAX_TICKS_PER_FRAME) {
            self.accumulator %= UNITS_PER_TICK;
            MAX_TICKS_PER_FRAME
        } else {
            self.accumulator -= due * UNITS_PER_TICK;
            u32::try_from(due).expect("due <= MAX_TICKS_PER_FRAME fits in u32")
        }
    }

    /// Time currently waiting in the accumulator, in seconds. Always less
    /// than [`TICK_DT`] after [`advance`](Self::advance) returns.
    pub fn accumulated(&self) -> f64 {
        self.accumulator as f64 / UNITS_PER_SECOND
    }
}

/// Tick counter. Simulation time is derived from the count, never summed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimClock {
    tick: u64,
}

impl SimClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the clock by one tick.
    pub fn step(&mut self) {
        self.tick += 1;
    }

    /// Number of ticks run so far.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Simulation time in seconds: `tick / TICK_HZ`.
    pub fn sim_time(&self) -> f64 {
        self.tick as f64 / f64::from(TICK_HZ)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_thirtieth_second_runs_two_ticks() {
        let mut ts = FixedTimestep::new();
        assert_eq!(ts.advance(1.0 / 30.0), 2);
        assert_eq!(ts.accumulated(), 0.0);
    }

    #[test]
    fn many_small_deltas_add_up_exactly() {
        let mut ts = FixedTimestep::new();
        let total: u32 = (0..1000).map(|_| ts.advance(0.001)).sum();
        assert_eq!(total, 60);
        assert_eq!(ts.accumulated(), 0.0);

        let total: u32 = (0..60_000).map(|_| ts.advance(0.001)).sum();
        assert_eq!(total, 3600);
    }

    #[test]
    fn long_frame_is_capped_and_excess_dropped() {
        let mut ts = FixedTimestep::new();
        assert_eq!(ts.advance(1.0), MAX_TICKS_PER_FRAME);
        assert!(ts.accumulated() < TICK_DT);
        // The dropped time does not come back on the next frame.
        assert_eq!(ts.advance(0.0), 0);
    }

    #[test]
    fn remainder_carries_over() {
        let mut ts = FixedTimestep::new();
        assert_eq!(ts.advance(TICK_DT * 0.75), 0);
        assert_eq!(ts.advance(TICK_DT * 0.75), 1);
        assert!((ts.accumulated() - TICK_DT * 0.5).abs() < 1e-12);
    }

    #[test]
    fn invalid_deltas_count_as_zero() {
        let mut ts = FixedTimestep::new();
        for dt in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(ts.advance(dt), 0);
        }
        assert_eq!(ts.accumulated(), 0.0);
    }

    #[test]
    fn sim_time_is_exactly_ticks_over_rate() {
        let mut clock = SimClock::new();
        assert_eq!(clock.sim_time(), 0.0);
        for n in 1..=10_000u64 {
            clock.step();
            assert_eq!(clock.tick(), n);
            assert_eq!(clock.sim_time(), n as f64 / 60.0);
        }
        let mut clock = SimClock::new();
        for _ in 0..60 {
            clock.step();
        }
        assert_eq!(clock.sim_time(), 1.0);
    }
}
