//! JSON reports written to stdout. Field names are part of the CLI protocol
//! (`docs/architecture.md` section 6): add fields, never rename or remove.

use serde::Serialize;

use loom_render::AdapterSummary;

#[derive(Debug, Serialize)]
pub(crate) struct RunReport {
    pub status: &'static str,
    /// `"headless"` or `"window"`.
    pub mode: &'static str,
    pub frames: u64,
    pub ticks: u64,
    pub sim_time: f64,
    pub size: [u32; 2],
    pub screenshot: Option<String>,
    /// `--dump-state` path, if the state was dumped.
    pub state: Option<String>,
    /// `--record` path, if input was recorded.
    pub record: Option<String>,
    /// `--trace` summary, if a trace was written.
    pub trace: Option<TraceReport>,
    /// Replay summary, if `--input` was given.
    pub input: Option<InputReport>,
    /// `--scene` path; `None` for the built-in demo scene.
    pub scene: Option<String>,
    /// Which game ran: the scene's top-level `game`, `"souls"` or `"rogue"`.
    pub game: &'static str,
    /// Successful scene hot reloads (window mode with `--scene` only).
    pub reloads: u64,
    /// Scene reloads that failed to parse or validate.
    pub reload_errors: u64,
    /// `None` when no GPU device was created (headless without screenshot).
    pub adapter: Option<RunAdapter>,
    /// Milliseconds from process start until the renderer was created
    /// (device, pipelines and, in window mode, the surface); `None` without
    /// a renderer. Wall clock, for diagnosis only; never in the state dump.
    pub renderer_init_ms: Option<u64>,
}

/// `loom view`: one frame of a scene, rendered without simulating it.
#[derive(Debug, Serialize)]
pub(crate) struct ViewReport {
    pub status: &'static str,
    /// Always `"view"`.
    pub mode: &'static str,
    pub screenshot: String,
    pub size: [u32; 2],
    /// `--scene` path.
    pub scene: String,
    pub adapter: RunAdapter,
    /// Milliseconds from process start until the renderer was created, as
    /// in [`RunReport`].
    pub renderer_init_ms: u64,
}

/// The trace written with `--trace`, counted since the last hot reload.
#[derive(Debug, Serialize)]
pub(crate) struct TraceReport {
    pub path: String,
    /// Lines written: one per tick, tick 0 included.
    pub lines: u64,
    /// Events in those lines.
    pub events: u64,
}

/// The script replayed with `--input`.
#[derive(Debug, Serialize)]
pub(crate) struct InputReport {
    pub path: String,
    /// Events in the script.
    pub events: usize,
    /// Events that took effect; less than `events` if the script outlasts the run.
    pub applied: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct RunAdapter {
    pub name: String,
    pub backend: String,
    pub driver: String,
}

impl From<AdapterSummary> for RunAdapter {
    fn from(a: AdapterSummary) -> Self {
        Self {
            name: a.name,
            backend: a.backend,
            driver: a.driver,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct InfoReport {
    pub status: &'static str,
    pub adapter: InfoAdapter,
}

#[derive(Debug, Serialize)]
pub(crate) struct InfoAdapter {
    pub name: String,
    pub backend: String,
    pub driver: String,
    pub device_type: String,
}

impl From<AdapterSummary> for InfoAdapter {
    fn from(a: AdapterSummary) -> Self {
        Self {
            name: a.name,
            backend: a.backend,
            driver: a.driver,
            device_type: a.device_type,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ErrorReport {
    pub status: &'static str,
    pub error: String,
}

impl ErrorReport {
    pub fn new(error: &str) -> Self {
        Self {
            status: "error",
            error: error.to_owned(),
        }
    }
}

/// Writes one JSON object and a newline to stdout. The only stdout writer.
#[allow(clippy::print_stdout)]
pub(crate) fn emit(report: &impl Serialize) {
    let json = serde_json::to_string(report).expect("report types contain only plain data");
    println!("{json}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless_report(adapter: Option<RunAdapter>) -> RunReport {
        RunReport {
            status: "ok",
            mode: "headless",
            frames: 60,
            ticks: 60,
            sim_time: 1.0,
            size: [1280, 720],
            screenshot: None,
            state: Some("out/s.json".into()),
            record: None,
            trace: None,
            input: Some(InputReport {
                path: "in.ron".into(),
                events: 3,
                applied: 2,
            }),
            scene: None,
            game: "souls",
            reloads: 0,
            reload_errors: 0,
            adapter,
            renderer_init_ms: None,
        }
    }

    #[test]
    fn run_report_shape() {
        let report = headless_report(Some(RunAdapter {
            name: "gpu".into(),
            backend: "Dx12".into(),
            driver: "d".into(),
        }));
        assert_eq!(
            serde_json::to_string(&report).unwrap(),
            r#"{"status":"ok","mode":"headless","frames":60,"ticks":60,"sim_time":1.0,"size":[1280,720],"screenshot":null,"state":"out/s.json","record":null,"trace":null,"input":{"path":"in.ron","events":3,"applied":2},"scene":null,"game":"souls","reloads":0,"reload_errors":0,"adapter":{"name":"gpu","backend":"Dx12","driver":"d"},"renderer_init_ms":null}"#
        );
    }

    #[test]
    fn report_without_gpu_has_null_adapter() {
        assert_eq!(
            serde_json::to_string(&headless_report(None)).unwrap(),
            r#"{"status":"ok","mode":"headless","frames":60,"ticks":60,"sim_time":1.0,"size":[1280,720],"screenshot":null,"state":"out/s.json","record":null,"trace":null,"input":{"path":"in.ron","events":3,"applied":2},"scene":null,"game":"souls","reloads":0,"reload_errors":0,"adapter":null,"renderer_init_ms":null}"#
        );
    }
}
