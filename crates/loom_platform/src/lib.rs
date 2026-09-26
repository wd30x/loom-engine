//! Platform layer: owns the winit event loop and window, and translates OS
//! events into `loom_core` input.
//!
//! Headless runs never touch this crate. See `docs/architecture.md`.
