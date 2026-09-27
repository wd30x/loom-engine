//! `loom` command-line runner. Specified in `docs/tasks/E0-see.md`.
//!
//! stdout carries exactly one JSON object per invocation, except for
//! `loom serve`, whose stdout is one JSON-RPC reply per line; logs go to
//! stderr.

mod debug;
mod games;
mod model;
mod output;
mod reload;
mod report;
mod run;
mod schema;
mod serve;
mod sheet;
mod sim;
mod state;
mod trace;
mod view;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};

use report::ErrorReport;

#[derive(Debug, Parser)]
#[command(name = "loom", version, about = "loom-engine runner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the engine, in a window or headless.
    Run(RunArgs),
    /// Print GPU adapter information without rendering.
    Info,
    /// Serve a headless remote-control session: JSON-RPC requests on stdin,
    /// one reply per line on stdout.
    Serve(ServeArgs),
    /// Render a scene once, headless and without simulating it (so it needs
    /// no character), and save a screenshot.
    View(ViewArgs),
    /// Print the formats of scene files, props, state dumps, trace lines
    /// and events, as JSON Schema.
    Schema,
    /// Check a glTF binary model (.glb) by the import rules and describe
    /// it: size, materials, skins and animations.
    Model(ModelArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct RunArgs {
    /// Do not create a window; one tick per frame, no wall clock.
    #[arg(long)]
    headless: bool,
    /// Exit after N frames. Required with --headless.
    #[arg(long, value_name = "N", required_if_eq("headless", "true"),
          value_parser = clap::value_parser!(u64).range(1..))]
    frames: Option<u64>,
    /// Save the last frame as a PNG before exiting. Parent directories are created.
    #[arg(long, value_name = "PATH")]
    screenshot: Option<PathBuf>,
    /// Write the simulation state as JSON before exiting. Parent directories are created.
    #[arg(long, value_name = "PATH")]
    dump_state: Option<PathBuf>,
    /// Replay an input script; the keyboard no longer drives actions (Esc still quits).
    #[arg(long, value_name = "PATH")]
    input: Option<PathBuf>,
    /// Record this run's per-tick action changes as an input script before exiting.
    /// Parent directories are created.
    #[arg(long, value_name = "PATH")]
    record: Option<PathBuf>,
    /// Write every tick's events and state as JSON lines while running.
    /// Parent directories are created.
    #[arg(long, value_name = "PATH")]
    trace: Option<PathBuf>,
    /// Load a scene file instead of the built-in demo scene.
    #[arg(long, value_name = "PATH")]
    scene: Option<PathBuf>,
    /// Draw debug wireframes over the frame: every category of the game, or
    /// only the comma-separated ones given. F3 toggles them in a window.
    #[arg(long, value_name = "CATEGORIES", num_args = 0..=1, require_equals = true)]
    debug_draw: Option<Option<String>>,
    /// Do not draw the game's HUD, in the window or the screenshot.
    #[arg(long)]
    no_hud: bool,
    /// Window mode: do not capture the mouse (no lock, pointer stays visible)
    /// and ignore mouse motion, so axes come from keys only.
    #[arg(long, conflicts_with = "headless")]
    no_capture: bool,
    /// Render resolution.
    #[arg(long, value_name = "WxH", default_value = "1280x720", value_parser = parse_size)]
    size: Size,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ServeArgs {
    /// Load a scene file instead of the built-in demo scene.
    #[arg(long, value_name = "PATH")]
    scene: Option<PathBuf>,
    /// Screenshot size when a request gives none.
    #[arg(long, value_name = "WxH", default_value = "1280x720", value_parser = parse_size)]
    size: Size,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ViewArgs {
    /// Scene file to render. Its `game` is ignored: nothing is simulated.
    #[arg(long, value_name = "PATH")]
    scene: PathBuf,
    /// Where to save the PNG. Parent directories are created.
    #[arg(long, value_name = "PATH")]
    screenshot: PathBuf,
    /// Render resolution.
    #[arg(long, value_name = "WxH", default_value = "1280x720", value_parser = parse_size)]
    size: Size,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ModelArgs {
    /// The .glb file.
    #[arg(value_name = "PATH")]
    path: PathBuf,
}

/// Render size in physical pixels, both sides non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    pub width: u32,
    pub height: u32,
}

fn parse_size(s: &str) -> Result<Size, String> {
    let err = || format!("expected WIDTHxHEIGHT with positive integers, e.g. 1280x720, got {s:?}");
    let (w, h) = s.split_once(['x', 'X']).ok_or_else(err)?;
    let width: u32 = w.parse().map_err(|_| err())?;
    let height: u32 = h.parse().map_err(|_| err())?;
    if width == 0 || height == 0 {
        return Err(err());
    }
    Ok(Size { width, height })
}

fn main() -> ExitCode {
    // Process start, for `renderer_init_ms` in the report.
    let started = Instant::now();
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("warn,loom=info,loom_render=info,loom_platform=info"),
    )
    .format_timestamp_millis()
    .init();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) if !e.use_stderr() => {
            // --help / --version: plain text for humans, exit 0.
            e.exit();
        }
        Err(e) => {
            // Argument error: clap's message on stderr, JSON on stdout, exit 2.
            let _ = e.print();
            report::emit(&ErrorReport::new(e.render().to_string().trim()));
            return ExitCode::from(2);
        }
    };

    let result = match cli.command {
        Command::Run(args) => run::run(&args, started).map(|r| report::emit(&r)),
        Command::Info => run::info().map(|r| report::emit(&r)),
        Command::View(args) => view::view(&args, started).map(|r| report::emit(&r)),
        Command::Schema => {
            report::emit(&report::SchemaReport {
                status: "ok",
                schema: &schema::LoomSchema::new(),
            });
            Ok(())
        }
        Command::Model(args) => model::describe(&args.path).map(|r| report::emit(&r)),
        // Replies are written as they go; only a startup error is reported below.
        Command::Serve(args) => serve::serve(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        // A bad --debug-draw is only found once the scene says which game
        // runs; it is still an argument error.
        // The replies are out: stdout gets nothing more.
        Err(e) if e.downcast_ref::<serve::AfterReplies>().is_some() => {
            log::error!("{e:#}");
            ExitCode::from(1)
        }
        Err(e) if e.downcast_ref::<debug::DebugDrawError>().is_some() => {
            log::error!("{e:#}");
            report::emit(&ErrorReport::new(&format!("{e:#}")));
            ExitCode::from(2)
        }
        Err(e) => {
            log::error!("{e:#}");
            report::emit(&ErrorReport::new(&format!("{e:#}")));
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes() {
        assert_eq!(
            parse_size("1280x720"),
            Ok(Size {
                width: 1280,
                height: 720
            })
        );
        assert_eq!(
            parse_size("640X360"),
            Ok(Size {
                width: 640,
                height: 360
            })
        );
        for bad in [
            "abc", "1280", "0x720", "1280x0", "x", "-1x5", "12x34x5", " 1x1",
        ] {
            assert!(parse_size(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn headless_requires_frames() {
        let err = Cli::try_parse_from(["loom", "run", "--headless"]).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(Cli::try_parse_from(["loom", "run", "--headless", "--frames", "1"]).is_ok());
        assert!(Cli::try_parse_from(["loom", "run", "--headless", "--frames", "0"]).is_err());
        assert!(Cli::try_parse_from(["loom", "run"]).is_ok());
    }

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
