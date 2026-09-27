//! `--dump-state`: the simulation state as pretty JSON. This file is the
//! cross-machine regression reference (`docs/architecture.md` sections 4
//! and 6), so field names follow the same stability rules as stdout.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use glam::{Quat, Vec3};
use loom_core::input::{ActionMap, ActionSet, ActionState};
use loom_scene::{Animation, NodeId, Scene};
use schemars::JsonSchema;
use serde::Serialize;

use crate::games::{AnyGame, AnyGameState};
use crate::output::ensure_parent_dir;

/// Generic over the game state only so that `loom schema` can describe
/// each game's dump from this same type (`docs/architecture.md` section 17).
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(rename = "StateDump")]
pub(crate) struct StateDump<G = AnyGameState> {
    /// Ticks run since the scene was loaded.
    pub tick: u64,
    /// `tick / 60`, in seconds.
    pub sim_time: f64,
    /// Actions active in the last tick, sorted by name.
    pub actions: ActionSet,
    /// Every mapped axis with its value in the last tick, sorted by name.
    pub axes: BTreeMap<String, f32>,
    /// Every scene node's local transform, and its animation if it has
    /// one, keyed by path (sorted).
    pub nodes: BTreeMap<String, NodeState>,
    /// The game's own state.
    pub game: G,
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
                .map(|id| (game.scene().path(id), NodeState::of(game.scene(), id)))
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

/// One node in a state dump: its local transform (relative to the parent)
/// and, for an animated model, what plays (`docs/architecture.md` section
/// 19): the clip names and ticks, not the bones' pose.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct NodeState {
    /// `[x, y, z]`, in meters.
    #[schemars(with = "[f32; 3]")]
    pub position: Vec3,
    /// A unit quaternion `[x, y, z, w]`.
    #[schemars(with = "[f32; 4]")]
    pub rotation: Quat,
    /// `[x, y, z]`.
    #[schemars(with = "[f32; 3]")]
    pub scale: Vec3,
    /// Only on a node with an `animation`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation: Option<AnimationState>,
}

impl NodeState {
    fn of(scene: &Scene, id: NodeId) -> Self {
        let t = scene.transform(id);
        Self {
            position: t.position,
            rotation: t.rotation,
            scale: t.scale,
            animation: scene
                .node(id)
                .model
                .as_ref()
                .and_then(|m| m.animation.as_ref())
                .map(AnimationState::of),
        }
    }
}

/// What an animated model plays.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct AnimationState {
    /// The clip playing.
    pub clip: String,
    /// The tick it started at.
    pub start: u64,
    /// The clip blended from, `null` until the first switch.
    pub from: Option<ClipStart>,
    /// Ticks the blend from `from` takes; 0 switches at once.
    pub blend: u64,
}

/// A clip and the tick it started at.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct ClipStart {
    pub clip: String,
    pub start: u64,
}

impl AnimationState {
    fn of(a: &Animation) -> Self {
        Self {
            clip: a.playing.clone(),
            start: a.start,
            from: a.from.as_ref().map(|(clip, start)| ClipStart {
                clip: clip.clone(),
                start: *start,
            }),
            blend: a.blend,
        }
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
