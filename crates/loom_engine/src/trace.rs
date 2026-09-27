//! `--trace` and serve's `trace`: every tick's events and state dump, one
//! JSON line per tick (`docs/architecture.md` section 6, "轨迹"). Lines are
//! serialized from typed values, never through `serde_json::Value`, so the
//! state is byte for byte the serve `state` result (section 13).

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::games::AnyEvents;
use crate::output::ensure_parent_dir;
use crate::report::TraceReport;
use crate::state::StateDump;

/// A trace file being written, buffered.
pub(crate) struct Trace {
    /// As given, for the report and error messages.
    path: String,
    out: BufWriter<File>,
    /// Lines and events written since the file was (re)started.
    lines: u64,
    events: u64,
}

/// One line: the fields in this order.
#[derive(Serialize)]
struct Line<'a> {
    tick: u64,
    events: AnyEvents<'a>,
    state: &'a StateDump,
}

impl Trace {
    /// Creates (or empties) the file at `path`, creating parent directories.
    pub fn create(path: &Path) -> Result<Self> {
        ensure_parent_dir(path)?;
        let file =
            File::create(path).with_context(|| format!("creating trace {}", path.display()))?;
        log::info!("tracing to {}", path.display());
        Ok(Self {
            path: path.display().to_string(),
            out: BufWriter::new(file),
            lines: 0,
            events: 0,
        })
    }

    /// Writes the line of tick `tick`: its events and the state after it.
    pub fn line(&mut self, tick: u64, events: AnyEvents, state: &StateDump) -> Result<()> {
        let line = Line {
            tick,
            events,
            state,
        };
        serde_json::to_writer(&mut self.out, &line)
            .map_err(anyhow::Error::from)
            .and_then(|()| Ok(self.out.write_all(b"\n")?))
            .with_context(|| format!("writing trace {}", self.path))?;
        self.lines += 1;
        self.events += events.len() as u64;
        Ok(())
    }

    /// Empties the file to write it again from the start.
    pub fn restart(&mut self) -> Result<()> {
        let restart = |out: &mut BufWriter<File>| -> std::io::Result<()> {
            out.flush()?;
            let file = out.get_mut();
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            Ok(())
        };
        restart(&mut self.out).with_context(|| format!("restarting trace {}", self.path))?;
        self.lines = 0;
        self.events = 0;
        Ok(())
    }

    /// Writes out what is buffered and closes the file.
    pub fn finish(mut self) -> Result<TraceReport> {
        self.out
            .flush()
            .with_context(|| format!("writing trace {}", self.path))?;
        log::info!("trace written to {} ({} lines)", self.path, self.lines);
        Ok(TraceReport {
            path: self.path,
            lines: self.lines,
            events: self.events,
        })
    }
}

#[cfg(test)]
mod tests {
    use loom_core::input::ActionState;

    use super::*;
    use crate::games::AnyGame;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("loom-trace-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The rogue project scene at tick 0.
    fn rogue_game() -> AnyGame {
        let src = include_str!("../../../assets/scenes/rogue.ron");
        AnyGame::new(loom_scene::Scene::parse(src).unwrap()).unwrap()
    }

    #[test]
    fn trace_line_is_tick_events_state_in_that_order_and_ends_in_a_newline() {
        let path = test_dir("line").join("sub/t.jsonl");
        let game = rogue_game();
        let input = ActionState::new();
        let state = StateDump::of(&game, &input, &game.bindings());
        let mut trace = Trace::create(&path).unwrap();
        trace.line(0, game.events(), &state).unwrap();
        let events = [rogue::Event::Hit {
            by: "player".to_owned(),
            target: "z".to_owned(),
            damage: 1,
            hp: 0,
            result: rogue::HitResult::Death,
        }];
        trace.line(7, AnyEvents::Rogue(&events), &state).unwrap();
        let report = trace.finish().unwrap();
        assert_eq!((report.lines, report.events), (2, 1));
        assert_eq!(report.path, path.display().to_string());
        let compact = serde_json::to_string(&state).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!(
                "{{\"tick\":0,\"events\":[],\"state\":{compact}}}\n\
                 {{\"tick\":7,\"events\":[{{\"type\":\"hit\",\"by\":\"player\",\"target\":\"z\",\
                 \"damage\":1,\"hp\":0,\"result\":\"death\"}}],\"state\":{compact}}}\n"
            )
        );
    }

    #[test]
    fn restarted_trace_is_written_again_from_the_start() {
        let path = test_dir("restart").join("t.jsonl");
        let game = rogue_game();
        let input = ActionState::new();
        let state = StateDump::of(&game, &input, &game.bindings());
        let mut trace = Trace::create(&path).unwrap();
        for tick in 0..3 {
            trace.line(tick, game.events(), &state).unwrap();
        }
        trace.restart().unwrap();
        trace.line(0, game.events(), &state).unwrap();
        let report = trace.finish().unwrap();
        assert_eq!((report.lines, report.events), (1, 0));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.starts_with("{\"tick\":0,\"events\":[],\"state\":{\"tick\":0,"));
        assert!(!text.contains('\0'));
    }
}
