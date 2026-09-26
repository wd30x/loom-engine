//! `--dump-state`: the simulation state as pretty JSON. This file is the
//! cross-machine regression reference (`docs/architecture.md` sections 4
//! and 6), so field names follow the same stability rules as stdout.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::game::CubeGame;
use crate::run::ensure_parent_dir;

#[derive(Debug, Serialize)]
pub(crate) struct StateDump {
    pub tick: u64,
    pub sim_time: f64,
    /// Game state, under one field named after the game.
    pub demo: DemoState,
}

/// State of the E0 spinning-cube demo.
#[derive(Debug, Serialize)]
pub(crate) struct DemoState {
    /// Spin angle around Y in radians, in `[0, 2π)`.
    pub yaw: f64,
}

impl StateDump {
    pub fn of(game: &CubeGame) -> Self {
        Self {
            tick: game.clock().tick(),
            sim_time: game.clock().sim_time(),
            demo: game.state(),
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
        let mut game = CubeGame::default();
        for _ in 0..60 {
            game.tick();
        }
        assert_eq!(
            StateDump::of(&game).to_json(),
            "{\n  \"tick\": 60,\n  \"sim_time\": 1.0,\n  \"demo\": {\n    \"yaw\": 1.0\n  }\n}\n"
        );
    }
}
