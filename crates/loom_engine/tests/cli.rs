//! CLI contract tests: run the real `loom` binary and check stdout, exit
//! code and output files.
//!
//! Only GPU-free invocations belong here (headless, no `--screenshot`), so
//! that `verify.ps1` keeps working on machines without a GPU.

#[path = "cli/bonfire.rs"]
mod bonfire;
#[path = "cli/boss.rs"]
mod boss;
#[path = "cli/common.rs"]
mod common;
#[path = "cli/debug_draw.rs"]
mod debug_draw;
#[path = "cli/flask.rs"]
mod flask;
#[path = "cli/hud.rs"]
mod hud;
#[path = "cli/input.rs"]
mod input;
#[path = "cli/rogue.rs"]
mod rogue;
#[path = "cli/run.rs"]
mod run;
#[path = "cli/scene.rs"]
mod scene;
#[path = "cli/schema.rs"]
mod schema;
#[path = "cli/souls.rs"]
mod souls;
#[path = "cli/souls_moves.rs"]
mod souls_moves;
#[path = "cli/sprites.rs"]
mod sprites;
#[path = "cli/trace.rs"]
mod trace;
