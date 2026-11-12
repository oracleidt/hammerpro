use std::sync::Arc;

use hp_editor::limits::cost::RenderCosts;
use hp_editor::limits::portals::{BspVis, PortalFile};
use hp_editor::optimize::OptimizeReport;

pub const ANALYSIS_PORTALS: &str = "visibility analysis.prt";

#[derive(Debug, Default)]
pub struct OptimizeRuntime {
    pub show: bool,
    pub request: bool,
    pub running: bool,
    pub report: Option<Arc<OptimizeReport>>,
    pub portals: Option<Arc<PortalFile>>,
    pub vis: Option<Arc<BspVis>>,
    pub costs: Option<Arc<RenderCosts>>,
    pub view: bool,
    pub selected: Option<usize>,
    pub error: Option<String>,
    pub version: u64,
}

impl OptimizeRuntime {
    pub fn stale(&self, revision: u64) -> bool {
        self.report.as_ref().is_some_and(|r| r.revision != revision)
    }
}
