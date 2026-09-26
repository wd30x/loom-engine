//! `loom run` and `loom info`.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use loom_core::input::Key;
use loom_core::{FixedTimestep, TICK_DT};
use loom_platform::{AppControl, Window, WindowApp, WindowConfig};
use loom_render::Renderer;

use crate::RunArgs;
use crate::output::ensure_parent_dir;
use crate::reload::{self, Poll, SceneWatcher};
use crate::report::{InfoReport, RunReport};
use crate::sim::Sim;
use crate::state::write_state;
use crate::view::render_scene;

pub(crate) fn info() -> Result<InfoReport> {
    let adapter = loom_render::probe_adapter()?;
    Ok(InfoReport {
        status: "ok",
        adapter: adapter.into(),
    })
}

/// `started` is the process start, for `renderer_init_ms`.
pub(crate) fn run(args: &RunArgs, started: Instant) -> Result<RunReport> {
    if args.headless {
        run_headless(args, started)
    } else {
        run_window(args, started)
    }
}

/// Whole milliseconds since `started`.
fn millis_since(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Headless: frame k runs exactly tick k. No wall clock.
///
/// Intermediate frames are never seen and rendering is a pure function of
/// the state, so only the final state is rendered, and only when a
/// screenshot is requested. Without one, the GPU is never touched.
fn run_headless(args: &RunArgs, started: Instant) -> Result<RunReport> {
    let frames = args.frames.expect("clap requires --frames with --headless");
    let mut sim = Sim::new(args, false)?;
    for _ in 0..frames {
        sim.tick();
    }
    let (record, state) = write_sim_outputs(args, &sim)?;
    let (screenshot, adapter, renderer_init_ms) = match &args.screenshot {
        Some(path) => {
            let mut renderer = Renderer::new_headless(args.size.width, args.size.height)?;
            let init_ms = millis_since(started);
            render_scene(&mut renderer, sim.game().scene())?;
            let saved = save_screenshot(&renderer, path)?;
            let adapter = renderer.adapter_summary();
            renderer.shutdown();
            (Some(saved), Some(adapter.into()), Some(init_ms))
        }
        None => (None, None, None),
    };
    Ok(RunReport {
        status: "ok",
        mode: "headless",
        frames,
        ticks: sim.game().clock().tick(),
        sim_time: sim.game().clock().sim_time(),
        size: [args.size.width, args.size.height],
        screenshot,
        state,
        record,
        input: sim.input_report(),
        scene: sim.scene_path(),
        reloads: 0,
        reload_errors: 0,
        adapter,
        renderer_init_ms,
    })
}

/// Writes the recording, then the state dump, as requested. Both modes call
/// this once the last tick has run.
fn write_sim_outputs(args: &RunArgs, sim: &Sim) -> Result<(Option<String>, Option<String>)> {
    let record = match &args.record {
        Some(path) => Some(sim.write_record(path)?),
        None => None,
    };
    let state = match &args.dump_state {
        Some(path) => Some(write_state(path, &sim.state_dump())?),
        None => None,
    };
    Ok((record, state))
}

fn run_window(args: &RunArgs, started: Instant) -> Result<RunReport> {
    // Stat before reading: if the file changes in between, the watcher sees
    // a newer time than recorded and simply re-reads it.
    let modified = args
        .scene
        .as_deref()
        .and_then(|p| reload::modified_time(p).ok());
    // Loads --scene and --input before the window opens: a bad file never runs a tick.
    let sim = Sim::new(args, true)?;
    let watcher = sim
        .scene_source()
        .map(|src| SceneWatcher::new(Instant::now(), modified, src.to_owned()));
    let mut app = WindowRun {
        args,
        started,
        renderer_init_ms: None,
        window: None,
        renderer: None,
        sim,
        watcher,
        reloads: 0,
        reload_errors: 0,
        timestep: FixedTimestep::new(),
        last_frame: None,
        frames: 0,
        last_size: [args.size.width, args.size.height],
        has_frame: false,
        screenshot: None,
        state: None,
        record: None,
        error: None,
        done: false,
    };
    let config = WindowConfig {
        title: "loom".to_owned(),
        width: args.size.width,
        height: args.size.height,
    };
    loom_platform::run_window(config, &mut app)?;
    if let Some(e) = app.error {
        return Err(e);
    }
    let renderer = app
        .renderer
        .take()
        .context("the window closed before the renderer was created")?;
    let adapter = renderer.adapter_summary();
    renderer.shutdown();
    // Like the renderer steps: if dropping the window hangs, the log says so.
    log::debug!("exit: window ...");
    let window_drop = Instant::now();
    drop(app.window.take());
    log::debug!("exit: window took {} ms", window_drop.elapsed().as_millis());
    Ok(RunReport {
        status: "ok",
        mode: "window",
        frames: app.frames,
        ticks: app.sim.game().clock().tick(),
        sim_time: app.sim.game().clock().sim_time(),
        size: app.last_size,
        screenshot: app.screenshot,
        state: app.state,
        record: app.record,
        input: app.sim.input_report(),
        scene: app.sim.scene_path(),
        reloads: app.reloads,
        reload_errors: app.reload_errors,
        adapter: Some(adapter.into()),
        renderer_init_ms: app.renderer_init_ms,
    })
}

/// Window mode: wall-clock time drives the fixed-step accumulator.
struct WindowRun<'a> {
    args: &'a RunArgs,
    /// Process start, and how long after it the renderer was ready.
    started: Instant,
    renderer_init_ms: Option<u64>,
    /// Kept to capture and release the mouse on focus changes.
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    sim: Sim,
    /// Watches `--scene` for changes; `None` without `--scene`.
    watcher: Option<SceneWatcher>,
    reloads: u64,
    reload_errors: u64,
    timestep: FixedTimestep,
    last_frame: Option<Instant>,
    /// Frames actually rendered (minimized frames are not counted).
    frames: u64,
    /// Size of the most recently rendered frame.
    last_size: [u32; 2],
    /// The offscreen target holds a rendered frame. Cleared when a resize
    /// replaces (or, when minimized, drops) the target.
    has_frame: bool,
    screenshot: Option<String>,
    state: Option<String>,
    record: Option<String>,
    error: Option<anyhow::Error>,
    /// Set once we decided to exit; later events are ignored.
    done: bool,
}

impl WindowRun<'_> {
    /// Captures (or releases) the mouse for live input; replaying a script
    /// or `--no-capture` leaves the pointer alone.
    fn capture_mouse(&self, captured: bool) {
        if let Some(window) = &self.window
            && self.sim.is_live()
            && !self.args.no_capture
        {
            loom_platform::set_cursor_captured(window, captured);
        }
    }

    fn fail(&mut self, e: anyhow::Error) -> AppControl {
        self.error = Some(e);
        self.done = true;
        AppControl::Exit
    }

    /// The single exit path for every way a window run ends (`--frames`
    /// reached, Esc, window closed). Writes the recording and the state dump
    /// as requested, then saves the last rendered frame for `--screenshot`.
    fn finish(&mut self) -> AppControl {
        if self.done {
            return AppControl::Exit;
        }
        self.done = true;
        let args = self.args;
        // Recording and state exist even if nothing was ever rendered (minimized).
        match write_sim_outputs(args, &self.sim) {
            Ok((record, state)) => (self.record, self.state) = (record, state),
            Err(e) => return self.fail(e),
        }
        let Some(path) = &args.screenshot else {
            return AppControl::Exit;
        };
        match &self.renderer {
            Some(renderer) if self.has_frame => match save_screenshot(renderer, path) {
                Ok(saved) => self.screenshot = Some(saved),
                Err(e) => return self.fail(e),
            },
            _ => log::warn!(
                "not saving screenshot {}: no rendered frame is available \
                 (nothing drawn yet, or the window is minimized)",
                path.display()
            ),
        }
        AppControl::Exit
    }

    /// Polls the scene file (every 0.5 s at most) and applies a reload.
    /// Returns `true` if the simulation restarted on a new scene.
    fn check_scene(&mut self, now: Instant) -> bool {
        let (Some(watcher), Some(path)) = (&mut self.watcher, &self.args.scene) else {
            return false;
        };
        match watcher.poll(
            now,
            || reload::modified_time(path),
            || reload::read_source(path),
        ) {
            Poll::Reloaded(game) => {
                self.sim.restart(*game);
                self.reloads += 1;
                log::info!("scene {} reloaded; restarting from tick 0", path.display());
                true
            }
            Poll::Failed(e) => {
                self.reload_errors += 1;
                log::warn!(
                    "scene {} not reloaded, keeping the current scene: {e:#}",
                    path.display()
                );
                false
            }
            Poll::NotChecked | Poll::Unchanged | Poll::FailedAgain => false,
        }
    }

    fn frame(&mut self) -> Result<AppControl> {
        if self.renderer.is_none() {
            return Ok(AppControl::Continue);
        }
        let now = Instant::now();
        if self.check_scene(now) {
            // Restarted from tick 0: no time has passed in the new run yet.
            self.timestep = FixedTimestep::new();
            self.last_frame = Some(now);
        }
        let dt = self
            .last_frame
            .map_or(0.0, |last| (now - last).as_secs_f64());
        self.last_frame = Some(now);
        log::trace!("frame {} dt {:.2} ms", self.frames + 1, dt * 1000.0);
        for _ in 0..self.timestep.advance(dt) {
            self.sim.tick();
        }

        let renderer = self
            .renderer
            .as_mut()
            .expect("checked at the top of frame()");
        if !render_scene(renderer, self.sim.game().scene())? {
            return Ok(AppControl::Continue);
        }
        self.frames += 1;
        self.has_frame = true;
        let (w, h) = renderer.size();
        self.last_size = [w, h];

        if self.args.frames == Some(self.frames) {
            return Ok(self.finish());
        }
        Ok(AppControl::Continue)
    }
}

impl WindowApp for WindowRun<'_> {
    fn window_created(&mut self, window: Arc<Window>) -> AppControl {
        self.window = Some(window.clone());
        let size = window.inner_size();
        match Renderer::new_with_surface(window, size.width, size.height) {
            Ok(renderer) => {
                self.renderer_init_ms = Some(millis_since(self.started));
                self.renderer = Some(renderer);
                AppControl::Continue
            }
            Err(e) => self.fail(e.into()),
        }
    }

    fn resized(&mut self, width: u32, height: u32) -> AppControl {
        if self.done {
            return AppControl::Exit;
        }
        log::debug!("resized to {width}x{height}");
        let Some(renderer) = &mut self.renderer else {
            return AppControl::Continue;
        };
        let before = renderer.size();
        if let Err(e) = renderer.resize(width, height) {
            return self.fail(e.into());
        }
        if renderer.size() != before {
            self.has_frame = false;
        }
        AppControl::Continue
    }

    fn redraw(&mut self) -> AppControl {
        if self.done {
            return AppControl::Exit;
        }
        match self.frame() {
            Ok(control) => control,
            Err(e) => self.fail(e),
        }
    }

    fn close_requested(&mut self) -> AppControl {
        self.finish()
    }

    fn escape_pressed(&mut self) -> AppControl {
        self.finish()
    }

    fn key(&mut self, key: Key, down: bool) -> AppControl {
        self.sim.key(key, down);
        AppControl::Continue
    }

    fn focus_lost(&mut self) -> AppControl {
        self.sim.focus_lost();
        self.capture_mouse(false);
        AppControl::Continue
    }

    fn focus_gained(&mut self) -> AppControl {
        self.capture_mouse(true);
        AppControl::Continue
    }

    fn mouse_motion(&mut self, dx: f64, dy: f64) -> AppControl {
        if !self.args.no_capture {
            self.sim.mouse_motion(dx, dy);
        }
        AppControl::Continue
    }

    /// While minimized nothing is presented, so vsync no longer paces the
    /// loop: sleep until the next tick is due instead of spinning. Ticks
    /// still follow the wall clock (`docs/architecture.md` section 2).
    fn wait_until(&self) -> Option<Instant> {
        let minimized = self.renderer.as_ref()?.size() == (0, 0);
        if !minimized || self.done {
            return None;
        }
        Some(next_tick_due(self.last_frame?, self.timestep.accumulated()))
    }
}

/// When the accumulator, holding `accumulated` seconds as of `last_frame`,
/// reaches the next whole tick.
fn next_tick_due(last_frame: Instant, accumulated: f64) -> Instant {
    last_frame + Duration::from_secs_f64((TICK_DT - accumulated).clamp(0.0, TICK_DT))
}

/// Reads back the last frame and writes it as PNG. Returns the path as given.
fn save_screenshot(renderer: &Renderer, path: &Path) -> Result<String> {
    let (width, height) = renderer.size();
    let pixels = renderer.read_pixels().context("reading back the frame")?;
    ensure_parent_dir(path)?;
    let image = image::RgbaImage::from_raw(width, height, pixels)
        .context("readback size does not match the render size")?;
    image
        .save_with_format(path, image::ImageFormat::Png)
        .with_context(|| format!("writing screenshot {}", path.display()))?;
    log::info!("screenshot saved to {}", path.display());
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_tick_is_due_one_tick_after_an_empty_accumulator() {
        let t = Instant::now();
        assert_eq!(next_tick_due(t, 0.0), t + Duration::from_secs_f64(TICK_DT));
    }

    #[test]
    fn next_tick_due_subtracts_the_time_already_accumulated() {
        let t = Instant::now();
        let due = next_tick_due(t, TICK_DT * 0.75);
        let expected = t + Duration::from_secs_f64(TICK_DT * 0.25);
        let diff = due.max(expected) - due.min(expected);
        assert!(diff < Duration::from_micros(1), "{diff:?}");
    }

    #[test]
    fn next_tick_due_never_lies_in_the_past_or_beyond_one_tick() {
        let t = Instant::now();
        assert_eq!(next_tick_due(t, TICK_DT * 2.0), t);
        assert_eq!(next_tick_due(t, -1.0), t + Duration::from_secs_f64(TICK_DT));
    }
}
