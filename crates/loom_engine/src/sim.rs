//! One simulation run: the game, where its input comes from, and an
//! optional recorder. Headless and window mode both drive it one `tick()`
//! at a time, so the per-tick logic exists only here.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use loom_core::debug::DebugDraw;
use loom_core::input::{
    ActionMap, ActionState, InputCollector, InputRecorder, InputScript, Key, ScriptPlayer,
    TickInput, canonical_axis_value,
};
use loom_render::{RenderError, Renderer};
use loom_scene::Scene;

use crate::RunArgs;
use crate::debug;
use crate::games::AnyGame;
use crate::output::ensure_parent_dir;
use crate::report::{InputReport, TraceReport};
use crate::sheet::Sheets;
use crate::state::StateDump;
use crate::trace::Trace;
use crate::view::render_scene;

/// A scene ready to simulate: the game its top-level `game` names and the
/// sprite sheets it uses, loaded together so a missing sheet or tag fails
/// the load like any other invalid scene.
pub(crate) struct LoadedScene {
    pub game: AnyGame,
    pub sheets: Sheets,
}

impl LoadedScene {
    pub fn from_source(src: &str) -> Result<Self> {
        Self::new(AnyGame::new(Scene::parse(src)?)?)
    }

    /// The built-in souls demo scene.
    pub fn builtin() -> Result<Self> {
        Self::new(AnyGame::builtin()?)
    }

    fn new(game: AnyGame) -> Result<Self> {
        let sheets = Sheets::load(game.scene())?;
        Ok(Self { game, sheets })
    }
}

enum InputSource {
    /// Headless without `--input`: every tick has no input.
    Idle,
    /// `--input`: replay a script. Keyboard and mouse are ignored. The parsed
    /// script is kept so a scene reload can replay it from tick 1.
    Script {
        script: InputScript,
        player: ScriptPlayer,
        path: String,
    },
    /// Window mode without `--input`: the keyboard and the mouse.
    Live(InputCollector),
    /// `loom serve`: the same input every tick until a request changes it
    /// ([`Sim::set_input`]).
    Held(TickInput),
}

pub(crate) struct Sim {
    game: AnyGame,
    /// Sheets of `game`'s scene; only ever drawn with one renderer.
    sheets: Sheets,
    actions: ActionState,
    /// The game's key bindings.
    bindings: ActionMap,
    source: InputSource,
    recorder: Option<InputRecorder>,
    /// `--trace`, or serve's `trace`: a line after every tick.
    trace: Option<Trace>,
    /// `--debug-draw`: the categories it turned on; `None` without it.
    debug_draw: Option<Vec<&'static str>>,
    /// `--scene` as given; `None` for the built-in scene.
    scene_path: Option<String>,
    /// Source text of the scene as loaded (`--scene`, or a scene a serve
    /// session loaded from a file or inline); `None` for the built-in scene.
    scene_source: Option<String>,
}

impl Sim {
    /// Reads and validates `--scene` and `--input` (if given) before any
    /// tick runs. Without a script, `live` selects keyboard input over none.
    pub fn new(args: &RunArgs, live: bool) -> Result<Self> {
        let (loaded, scene_source) = match &args.scene {
            Some(path) => {
                let src = std::fs::read_to_string(path)
                    .with_context(|| format!("reading scene {}", path.display()))?;
                let loaded = LoadedScene::from_source(&src)
                    .with_context(|| format!("invalid scene {}", path.display()))?;
                (loaded, Some(src))
            }
            None => (LoadedScene::builtin()?, None),
        };
        // The categories are the game's, so they are checked after the scene.
        let debug_draw = match &args.debug_draw {
            Some(value) => Some(debug::select_cli(
                loaded.game.name(),
                loaded.game.debug_categories(),
                value.as_deref(),
            )?),
            None => None,
        };
        // The script names this game's actions, so it is read after the scene.
        let bindings = loaded.game.bindings();
        let source = match &args.input {
            Some(path) => {
                let script = load_script(path, &bindings)?;
                InputSource::Script {
                    player: ScriptPlayer::new(script.clone()),
                    script,
                    path: path.display().to_string(),
                }
            }
            None if live => InputSource::Live(InputCollector::new()),
            None => InputSource::Idle,
        };
        let mut sim = Self {
            game: loaded.game,
            sheets: loaded.sheets,
            actions: ActionState::new(),
            bindings,
            source,
            recorder: args.record.is_some().then(InputRecorder::new),
            trace: None,
            debug_draw,
            scene_path: args.scene.as_ref().map(|p| p.display().to_string()),
            scene_source,
        };
        if let Some(path) = &args.trace {
            sim.start_trace(path)?;
        }
        Ok(sim)
    }

    /// A `loom serve` session on `loaded`, from tick 0: the input is held
    /// (nothing pressed to start with) and always recorded. `scene_path`
    /// and `scene_source` say where `loaded` came from, as in [`Sim::new`];
    /// scene text given inline has a source but no path.
    pub fn held(
        loaded: LoadedScene,
        scene_path: Option<String>,
        scene_source: Option<String>,
    ) -> Self {
        Self {
            bindings: loaded.game.bindings(),
            game: loaded.game,
            sheets: loaded.sheets,
            actions: ActionState::new(),
            source: InputSource::Held(TickInput::default()),
            recorder: Some(InputRecorder::new()),
            trace: None,
            debug_draw: None,
            scene_path,
            scene_source,
        }
    }

    /// Starts over from tick 0 with `loaded` (a reloaded scene): the clock,
    /// action state, recording, script replay and trace restart; keys held
    /// on the keyboard stay held. The bindings are `loaded`'s game's.
    pub fn restart(&mut self, loaded: LoadedScene) -> Result<()> {
        self.bindings = loaded.game.bindings();
        self.game = loaded.game;
        self.sheets = loaded.sheets;
        self.actions = ActionState::new();
        if self.recorder.is_some() {
            self.recorder = Some(InputRecorder::new());
        }
        if let InputSource::Script { script, player, .. } = &mut self.source {
            *player = ScriptPlayer::new(script.clone());
        }
        if let Some(trace) = &mut self.trace {
            trace.restart()?;
        }
        self.trace_line()
    }

    /// Source text of the scene as loaded; `None` for the built-in scene.
    pub fn scene_source(&self) -> Option<&str> {
        self.scene_source.as_deref()
    }

    /// Runs one tick: sample this tick's input, advance the action state,
    /// record, step the game, then trace. Only writing the trace can fail.
    pub fn tick(&mut self) -> Result<()> {
        let next = match &mut self.source {
            InputSource::Idle => TickInput::default(),
            InputSource::Script { player, .. } => player.next_tick(),
            InputSource::Live(collector) => collector.sample(&self.bindings),
            InputSource::Held(input) => input.clone(),
        };
        self.actions.advance(next);
        if let Some(recorder) = &mut self.recorder {
            recorder.observe(self.actions.input());
        }
        self.game.tick(&self.actions);
        self.trace_line()
    }

    /// With a trace, writes the line of the current tick: the events of
    /// the latest tick (none at tick 0) and the state now.
    fn trace_line(&mut self) -> Result<()> {
        if let Some(trace) = &mut self.trace {
            let state = StateDump::of(&self.game, &self.actions, &self.bindings);
            trace.line(self.game.clock().tick(), self.game.events(), &state)?;
        }
        Ok(())
    }

    /// Starts tracing to `path` (emptied first), stopping a trace already
    /// running, and writes the current tick's line at once.
    pub fn start_trace(&mut self, path: &Path) -> Result<()> {
        self.stop_trace()?;
        self.trace = Some(Trace::create(path)?);
        self.trace_line()
    }

    /// Stops tracing: writes out and closes the file. `None` without a trace.
    pub fn stop_trace(&mut self) -> Result<Option<TraceReport>> {
        self.trace.take().map(Trace::finish).transpose()
    }

    /// Hands the running trace over to `other` (a serve session starting
    /// over), which empties it and writes its own tick 0.
    pub fn pass_trace_to(&mut self, other: &mut Self) -> Result<()> {
        if let Some(mut trace) = self.trace.take() {
            trace.restart()?;
            other.trace = Some(trace);
            other.trace_line()?;
        }
        Ok(())
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

    /// Raw mouse motion from the window. Ignored while replaying a script.
    pub fn mouse_motion(&mut self, dx: f64, dy: f64) {
        if let InputSource::Live(collector) = &mut self.source {
            collector.mouse_motion(dx, dy);
        }
    }

    /// The window lost focus: release held keys so none gets stuck, and
    /// drop pending mouse motion.
    pub fn focus_lost(&mut self) {
        if let InputSource::Live(collector) = &mut self.source {
            collector.release_all();
        }
    }

    /// Changes the held input of a serve session, from the next tick on:
    /// presses (`true`) or releases (`false`) `actions` and sets `axes`
    /// (canonical, `0.0` removes the axis); what is not mentioned stays.
    /// Every name must be in the bindings and every value finite, or
    /// nothing changes. Returns the held input after the change.
    pub fn set_input(
        &mut self,
        actions: &BTreeMap<String, bool>,
        axes: &BTreeMap<String, f32>,
    ) -> Result<&TickInput> {
        let InputSource::Held(held) = &mut self.source else {
            bail!("only a serve session holds input");
        };
        for action in actions.keys() {
            if !self.bindings.contains(action) {
                bail!("unknown action {action:?}");
            }
        }
        for (axis, value) in axes {
            if !self.bindings.contains_axis(axis) {
                bail!("unknown axis {axis:?}");
            }
            if !value.is_finite() {
                bail!("axis {axis:?}: the value is not finite");
            }
        }
        for (action, &down) in actions {
            if down {
                held.actions.insert(action.clone());
            } else {
                held.actions.remove(action);
            }
        }
        for (axis, &value) in axes {
            let value = canonical_axis_value(value);
            if value == 0.0 {
                held.axes.remove(axis);
            } else {
                held.axes.insert(axis.clone(), value);
            }
        }
        Ok(held)
    }

    /// Input comes from the keyboard and mouse (not a script), so the window
    /// should capture the mouse while focused.
    pub fn is_live(&self) -> bool {
        matches!(self.source, InputSource::Live(_))
    }

    pub fn game(&self) -> &AnyGame {
        &self.game
    }

    /// Draws the current state with `renderer`, which must be the only
    /// renderer this `Sim` ever draws with (it owns the sheets' textures),
    /// with the debug draw of `categories` (the game's names) on top.
    /// Returns `false` if the frame was skipped (zero render size).
    pub fn render(
        &mut self,
        renderer: &mut Renderer,
        categories: &[&str],
    ) -> Result<bool, RenderError> {
        let mut draw = DebugDraw::new();
        if !categories.is_empty() {
            self.game.debug_draw(categories, &mut draw);
        }
        let lines = debug::line_draws(draw.lines());
        render_scene(renderer, self.game.scene(), &mut self.sheets, &lines)
    }

    /// `--debug-draw`: the categories it turned on; `None` without it.
    pub fn debug_draw(&self) -> Option<&[&'static str]> {
        self.debug_draw.as_deref()
    }

    pub fn bindings(&self) -> &ActionMap {
        &self.bindings
    }

    pub fn state_dump(&self) -> StateDump {
        StateDump::of(&self.game, &self.actions, &self.bindings)
    }

    /// The `scene` field of the run report.
    pub fn scene_path(&self) -> Option<String> {
        self.scene_path.clone()
    }

    /// The `input` field of the run report.
    pub fn input_report(&self) -> Option<InputReport> {
        match &self.source {
            InputSource::Script { player, path, .. } => Some(InputReport {
                path: path.clone(),
                events: player.event_count(),
                applied: player.applied(),
            }),
            InputSource::Idle | InputSource::Live(_) | InputSource::Held(_) => None,
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
