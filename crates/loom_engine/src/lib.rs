//! loom-engine facade: re-exports the engine crates and wires them together
//! for the `loom` runner. See `docs/architecture.md`.

pub use loom_core as core;
pub use loom_platform as platform;
pub use loom_render as render;
