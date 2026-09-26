//! `loom run` and `loom info`.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use loom_core::FixedTimestep;
use loom_platform::{AppControl, Window, WindowApp, WindowConfig};
use loom_render::Renderer;

use crate::RunArgs;
use crate::game::CubeGame;
use crate::report::{InfoReport, RunReport};

pub(crate) fn info() -> Result<InfoReport> {
    let adapter = loom_render::probe_adapter()?;
    Ok(InfoReport {
        status: "ok",
        adapter: adapter.into(),
    })
}

pub(crate) fn run(args: &RunArgs) -> Result<RunReport> {
    if args.headless {
        run_headless(args)
    } else {
        run_window(args)
    }
}

/// Headless: frame k runs exactly tick k, then renders. No wall clock.
fn run_headless(args: &RunArgs) -> Result<RunReport> {
    let frames = args.frames.expect("clap requires --frames with --headless");
    let mut renderer = Renderer::new_headless(args.size.width, args.size.height)?;
    let mut game = CubeGame::default();
    for _ in 0..frames {
        game.tick();
        renderer.render(game.model_matrix())?;
    }
    let screenshot = match &args.screenshot {
        Some(path) => Some(save_screenshot(&renderer, path)?),
        None => None,
    };
    Ok(RunReport {
        status: "ok",
        mode: "headless",
        frames,
        ticks: game.clock().tick(),
        sim_time: game.clock().sim_time(),
        size: [args.size.width, args.size.height],
        screenshot,
        adapter: renderer.adapter_summary().into(),
    })
}

fn run_window(args: &RunArgs) -> Result<RunReport> {
    let mut app = WindowRun {
        args,
        renderer: None,
        game: CubeGame::default(),
        timestep: FixedTimestep::new(),
        last_frame: None,
        frames: 0,
        last_size: [args.size.width, args.size.height],
        screenshot: None,
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
        .context("the window closed before the renderer was created")?;
    Ok(RunReport {
        status: "ok",
        mode: "window",
        frames: app.frames,
        ticks: app.game.clock().tick(),
        sim_time: app.game.clock().sim_time(),
        size: app.last_size,
        screenshot: app.screenshot,
        adapter: renderer.adapter_summary().into(),
    })
}

/// Window mode: wall-clock time drives the fixed-step accumulator.
struct WindowRun<'a> {
    args: &'a RunArgs,
    renderer: Option<Renderer>,
    game: CubeGame,
    timestep: FixedTimestep,
    last_frame: Option<Instant>,
    /// Frames actually rendered (minimized frames are not counted).
    frames: u64,
    /// Size of the most recently rendered frame.
    last_size: [u32; 2],
    screenshot: Option<String>,
    error: Option<anyhow::Error>,
    /// Set once we decided to exit; later events are ignored.
    done: bool,
}

impl WindowRun<'_> {
    fn fail(&mut self, e: anyhow::Error) -> AppControl {
        self.error = Some(e);
        self.done = true;
        AppControl::Exit
    }

    fn frame(&mut self) -> Result<AppControl> {
        let Some(renderer) = &mut self.renderer else {
            return Ok(AppControl::Continue);
        };
        let now = Instant::now();
        let dt = self
            .last_frame
            .map_or(0.0, |last| (now - last).as_secs_f64());
        self.last_frame = Some(now);
        log::trace!("frame {} dt {:.2} ms", self.frames + 1, dt * 1000.0);
        for _ in 0..self.timestep.advance(dt) {
            self.game.tick();
        }

        if !renderer.render(self.game.model_matrix())? {
            return Ok(AppControl::Continue);
        }
        self.frames += 1;
        let (w, h) = renderer.size();
        self.last_size = [w, h];

        if self.args.frames == Some(self.frames) {
            if let Some(path) = &self.args.screenshot {
                self.screenshot = Some(save_screenshot(renderer, path)?);
            }
            self.done = true;
            return Ok(AppControl::Exit);
        }
        Ok(AppControl::Continue)
    }
}

impl WindowApp for WindowRun<'_> {
    fn window_created(&mut self, window: Arc<Window>) -> AppControl {
        let size = window.inner_size();
        match Renderer::new_with_surface(window, size.width, size.height) {
            Ok(renderer) => {
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
        match self.renderer.as_mut().map(|r| r.resize(width, height)) {
            Some(Err(e)) => self.fail(e.into()),
            _ => AppControl::Continue,
        }
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
        self.done = true;
        AppControl::Exit
    }

    fn escape_pressed(&mut self) -> AppControl {
        self.done = true;
        AppControl::Exit
    }
}

/// Reads back the last frame and writes it as PNG. Returns the path as given.
fn save_screenshot(renderer: &Renderer, path: &Path) -> Result<String> {
    let (width, height) = renderer.size();
    let pixels = renderer.read_pixels().context("reading back the frame")?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    let image = image::RgbaImage::from_raw(width, height, pixels)
        .context("readback size does not match the render size")?;
    image
        .save_with_format(path, image::ImageFormat::Png)
        .with_context(|| format!("writing screenshot {}", path.display()))?;
    log::info!("screenshot saved to {}", path.display());
    Ok(path.display().to_string())
}
