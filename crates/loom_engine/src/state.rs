//! `--dump-state`: the simulation state as pretty JSON. This file is the
//! cross-machine regression reference (`docs/architecture.md` sections 4
//! and 6), so field names follow the same stability rules as stdout.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use loom_core::input::{ActionMap, ActionSet, ActionState};
use loom_scene::Transform;
use serde::Serialize;

use crate::games::{AnyGame, AnyGameState};
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
    /// Game state: souls' (`docs/architecture.md` section 11) or rogue's
    /// (section 15), unwrapped.
    pub game: AnyGameState,
}

impl StateDump {
    pub fn of(game: &AnyGame, input: &ActionState, bindings: &ActionMap) -> Self {
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
            game: game.state(),
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
        // The G2.3 demo scene: the lock-on dummies of today's sort first.
        let scene = include_str!("../../../games/souls/src/g2_3_demo.scene.ron");
        let mut game = AnyGame::new(loom_scene::Scene::parse(scene).unwrap()).unwrap();
        let input = ActionState::new();
        for _ in 0..60 {
            game.tick(&input);
        }
        let json = StateDump::of(&game, &input, &game.bindings()).to_json();
        // Field order, and node paths sorted; exact bytes are checked by the CLI tests.
        assert!(json.starts_with(concat!(
            "{\n  \"tick\": 60,\n  \"sim_time\": 1.0,\n  \"actions\": [],\n",
            "  \"axes\": {\n    \"look_x\": 0.0,\n    \"look_y\": 0.0,\n",
            "    \"move_x\": 0.0,\n    \"move_y\": 0.0\n  },\n",
            "  \"nodes\": {\n    \"ground\": {"
        )));
        let order = [
            "\"ground\"",
            "\"main_cam\"",
            "\"pillars\"",
            "\"pillars/a\"",
            "\"player\"",
            "\"player/visor\"",
            "\"sun\"",
            "\"game\"",
        ]
        .map(|k| json.find(k).unwrap());
        assert!(order.is_sorted(), "{order:?}");
        assert!(json.ends_with("\n  }\n}\n") && !json.contains('\r'));
    }

    #[test]
    fn compact_state_is_the_pretty_dump_without_its_whitespace() {
        // The demo scene locked on to dummy_b and into a light attack:
        // floats, nested objects, nulls and arrays of objects.
        let mut game = AnyGame::builtin().unwrap();
        let bindings = game.bindings();
        let mut input = ActionState::new();
        for tick in 1..=120 {
            let actions = match tick {
                1 => vec!["lock".to_owned()],
                100 => vec!["attack".to_owned()],
                _ => vec![],
            };
            let axes = if tick < 90 {
                [("move_y".to_owned(), 1.0)].into()
            } else {
                Default::default()
            };
            input.advance(loom_core::input::TickInput {
                actions: actions.into_iter().collect(),
                axes,
            });
            game.tick(&input);
        }
        let state = StateDump::of(&game, &input, &bindings);
        let pretty = state.to_json();
        assert!(pretty.contains(r#""lock": "dummy_b""#), "{pretty}");
        assert!(pretty.contains(r#""attack_phase": "recovery""#), "{pretty}");
        let stripped: String = pretty.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(serde_json::to_string(&state).unwrap(), stripped);
    }
}
