//! `--dump-state`: the simulation state as pretty JSON. This file is the
//! cross-machine regression reference (`docs/architecture.md` sections 4
//! and 6), so field names follow the same stability rules as stdout.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use glam::Vec3;
use loom_core::input::{ActionMap, ActionSet, ActionState};
use loom_scene::Transform;
use loom_sim::CubeGame;
use serde::Serialize;

use crate::output::ensure_parent_dir;

#[derive(Debug, Serialize)]
pub(crate) struct StateDump {
    pub tick: u64,
    pub sim_time: f64,
    /// Actions active in the last tick, sorted by name.
    pub actions: ActionSet,
    /// Every mapped axis with its value in the last tick, sorted by name.
    pub axes: BTreeMap<String, f32>,
    /// Local transform of every scene node, keyed by path (sorted).
    pub nodes: BTreeMap<String, Transform>,
    /// Game state, under one field named after the game.
    pub demo: DemoState,
}

/// State of the spinning-cube demo.
#[derive(Debug, Serialize)]
pub(crate) struct DemoState {
    /// Spin angle around Y in radians, in `[0, 2π)`.
    pub yaw: f64,
    /// Position in meters; serialized as `[x, y, z]`.
    pub position: Vec3,
}

impl StateDump {
    pub fn of(game: &CubeGame, input: &ActionState, bindings: &ActionMap) -> Self {
        Self {
            tick: game.clock().tick(),
            sim_time: game.clock().sim_time(),
            actions: input.current().clone(),
            axes: input.axis_values(bindings),
            nodes: game
                .scene()
                .ids()
                .map(|id| (game.scene().path(id), *game.scene().transform(id)))
                .collect(),
            demo: DemoState {
                yaw: game.yaw(),
                position: game.position(),
            },
        }
    }

    /// Pretty JSON (2-space indent) with a trailing newline.
    pub fn to_json(&self) -> String {
        let mut json =
            serde_json::to_string_pretty(self).expect("state types contain only plain data");
        json.push('\n');
        json
    }
}

/// Writes `state` to `path`, creating parent directories. Returns the path
/// as given, for the report.
pub(crate) fn write_state(path: &Path, state: &StateDump) -> Result<String> {
    ensure_parent_dir(path)?;
    std::fs::write(path, state.to_json())
        .with_context(|| format!("writing state dump {}", path.display()))?;
    log::info!("state dumped to {}", path.display());
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_is_pretty_json_with_trailing_newline() {
        let mut game = CubeGame::new(loom_sim::demo_scene()).unwrap();
        let input = ActionState::new();
        for _ in 0..60 {
            game.tick(&input);
        }
        let json = StateDump::of(&game, &input, &loom_sim::demo_bindings()).to_json();
        // Field order, and node paths sorted; exact bytes are checked by the CLI tests.
        assert!(json.starts_with(concat!(
            "{\n  \"tick\": 60,\n  \"sim_time\": 1.0,\n  \"actions\": [],\n",
            "  \"axes\": {\n    \"look_x\": 0.0,\n    \"look_y\": 0.0,\n",
            "    \"move_x\": 0.0,\n    \"move_y\": 0.0\n  },\n",
            "  \"nodes\": {\n    \"cube\": {"
        )));
        let order = [
            "\"cube\"",
            "\"ground\"",
            "\"main_cam\"",
            "\"sun\"",
            "\"demo\"",
        ]
        .map(|k| json.find(k).unwrap());
        assert!(order.is_sorted(), "{order:?}");
        assert!(json.ends_with("\n  }\n}\n") && !json.contains('\r'));
    }
}
