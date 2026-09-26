//! Renderer built directly on wgpu.
//!
//! Every frame is drawn into an offscreen color target. Window mode then
//! blits that target to the surface; headless mode reads it back to CPU
//! memory. The two paths share all drawing code. See `docs/architecture.md`.
