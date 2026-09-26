//! One simulation run: the game, where its input comes from, and an
//! optional recorder. Headless and window mode both drive it one `tick()`
//! at a time, so the per-tick logic exists only here.

use std::path::Path;

use anyhow::{Context, Result};
use loom_core::input::{
    ActionMap, ActionSet, ActionState, InputCollector, InputRecorder, InputScript, Key,
    ScriptPlayer,
};

use crate::RunArgs;
use crate::game::{CubeGame, demo_bindings};
use crate::output::ensure_parent_dir;
use crate::report::InputReport;
use crate::state::StateDump;

enum InputSource {
    /// Headless without `--input`: every tick has no actions.
    Idle,
    /// `--input`: replay a script. Keyboard input is ignored.
    Script { player: ScriptPlayer, path: String },
    /// Window mode without `--input`: the keyboard.
    Live(InputCollector),
}

pub(crate) struct Sim {
    game: CubeGame,
    actions: ActionState,
    bindings: ActionMap,
    source: InputSource,
    recorder: Option<InputRecorder>,
}

impl Sim {
    /// Reads and validates `--input` (if given) before any tick runs.
    /// Without a script, `live` selects keyboard input over no input.
    pub fn new(args: &RunArgs, live: bool) -> Result<Self> {
        let bindings = demo_bindings();
        let source = match &args.input {
            Some(path) => InputSource::Script {
                player: ScriptPlayer::new(load_script(path, &bindings)?),
                path: path.display().to_string(),
            },
            None if live => InputSource::Live(InputCollector::new()),
            None => InputSource::Idle,
        };
        Ok(Self {
            game: CubeGame::default(),
            actions: ActionState::new(),
            bindings,
            source,
            recorder: args.record.is_some().then(InputRecorder::new),
        })
    }

    /// Runs one tick: sample this tick's actions, advance the action state,
    /// record, then step the game.
    pub fn tick(&mut self) {
        let next = match &mut self.source {
            InputSource::Idle => ActionSet::new(),
            InputSource::Script { player, .. } => player.next_tick(),
            InputSource::Live(collector) => collector.sample(&self.bindings),
        };
        self.actions.advance(next);
        if let Some(recorder) = &mut self.recorder {
            recorder.observe(self.actions.current());
        }
        self.game.tick(&self.actions);
    }

    /// A key event from the window. Ignored while replaying a script.
    pub fn key(&mut self, key: Key, down: bool) {
        if let InputSource::Live(collector) = &mut self.source {
            if down {
                collector.key_down(key);
            } else {
                collector.key_up(key);
            }
        }
    }

    /// The window lost focus: release held keys so none gets stuck.
    pub fn focus_lost(&mut self) {
        if let InputSource::Live(collector) = &mut self.source {
            collector.release_all();
        }
    }

    pub fn game(&self) -> &CubeGame {
        &self.game
    }

    pub fn state_dump(&self) -> StateDump {
        StateDump::of(&self.game, &self.actions)
    }

    /// The `input` field of the run report.
    pub fn input_report(&self) -> Option<InputReport> {
        match &self.source {
            InputSource::Script { player, path } => Some(InputReport {
                path: path.clone(),
                events: player.event_count(),
                applied: player.applied(),
            }),
            InputSource::Idle | InputSource::Live(_) => None,
        }
    }

    /// Writes the recording so far to `path`, creating parent directories.
    /// Returns the path as given, for the report.
    pub fn write_record(&self, path: &Path) -> Result<String> {
        let script = self
            .recorder
            .clone()
            .context("recording was not enabled for this run")?
            .finish();
        ensure_parent_dir(path)?;
        std::fs::write(path, script.to_ron())
            .with_context(|| format!("writing input recording {}", path.display()))?;
        log::info!("input recorded to {}", path.display());
        Ok(path.display().to_string())
    }
}

fn load_script(path: &Path, bindings: &ActionMap) -> Result<InputScript> {
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("reading input script {}", path.display()))?;
    InputScript::parse(&src, bindings)
        .with_context(|| format!("invalid input script {}", path.display()))
}
