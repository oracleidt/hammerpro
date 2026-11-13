use std::sync::Arc;

use hp_editor::optimize::cubemaps::{CubemapReport, Suggestion};

#[derive(Debug, Default)]
pub struct CubemapRuntime {
    pub show: bool,
    pub request: bool,
    pub view: bool,
    pub report: Option<Arc<CubemapReport>>,
    pub dismissed: Vec<usize>,
    pub selected: Option<usize>,
    pub focus: Option<usize>,
    pub size: Option<i32>,
    pub wait_vis: bool,
    pub vis_version: u64,
    pub ms: u64,
    pub version: u64,
    pub error: Option<String>,
}

impl CubemapRuntime {
    pub fn stale(&self, revision: u64) -> bool {
        self.report.as_ref().is_some_and(|r| r.revision != revision)
    }

    pub fn pending(&self) -> Vec<(usize, &Suggestion)> {
        self.report
            .as_ref()
            .map(|r| r.suggestions.iter().enumerate().filter(|(i, _)| !self.dismissed.contains(i)).collect())
            .unwrap_or_default()
    }

    pub fn store(&mut self, report: CubemapReport, ms: u64) {
        self.report = Some(Arc::new(report));
        self.dismissed.clear();
        self.selected = None;
        self.focus = None;
        self.ms = ms;
        self.error = None;
        self.version = self.version.wrapping_add(1);
    }
}
