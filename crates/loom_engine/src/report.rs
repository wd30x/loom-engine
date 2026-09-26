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
    /// `None` when no GPU device was created (headless without screenshot).
    pub adapter: Option<RunAdapter>,
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
            adapter,
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
            r#"{"status":"ok","mode":"headless","frames":60,"ticks":60,"sim_time":1.0,"size":[1280,720],"screenshot":null,"state":"out/s.json","adapter":{"name":"gpu","backend":"Dx12","driver":"d"}}"#
        );
    }

    #[test]
    fn report_without_gpu_has_null_adapter() {
        assert_eq!(
            serde_json::to_string(&headless_report(None)).unwrap(),
            r#"{"status":"ok","mode":"headless","frames":60,"ticks":60,"sim_time":1.0,"size":[1280,720],"screenshot":null,"state":"out/s.json","adapter":null}"#
        );
    }
}
