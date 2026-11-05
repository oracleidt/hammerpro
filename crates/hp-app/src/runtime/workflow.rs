use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::settings::SelectModeSetting;
use crate::state::AppState;
use crate::view::QuadrantView;

pub const MAX_DROPPED: usize = 32;

#[derive(Debug, Default)]
pub struct WorkflowRuntime {
    pub previous_select_mode: Option<SelectModeSetting>,
    pub build_info: Option<String>,
    pub save_layout_open: bool,
    pub apply_layout: Option<String>,
    pub delete_layout: Option<String>,
    pub dropped: Vec<PathBuf>,
    pub tabs: Vec<MapTabInfo>,
    pub active_tab: u64,
    pub tab_request: Option<TabRequest>,
    pub paste_preview: Option<(u64, std::sync::Arc<hp_editor::PastePreview>)>,
    pub paste_preview_serial: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapTabInfo {
    pub id: u64,
    pub title: String,
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabRequest {
    Switch(u64),
    Close(u64),
    CloseOthers(u64),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowMapMemory {
    pub grid: Option<f64>,
}

impl crate::sidecar::SidecarSection for WorkflowMapMemory {
    const NAME: &'static str = "workflow";
}

impl WorkflowRuntime {
    pub fn push_dropped(&mut self, path: PathBuf) {
        if self.dropped.len() >= MAX_DROPPED {
            self.dropped.remove(0);
        }
        self.dropped.push(path);
    }

    pub fn take_dropped(&mut self, extensions: &[&str]) -> Vec<PathBuf> {
        let matches = |p: &PathBuf| {
            p.extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x)))
        };
        let (taken, kept): (Vec<PathBuf>, Vec<PathBuf>) =
            std::mem::take(&mut self.dropped).into_iter().partition(matches);
        self.dropped = kept;
        taken
    }

    pub fn target_view(state: &AppState) -> usize {
        state.ui.hovered_view.or(state.ui.hovered_logical).or(state.ui.focused_view).filter(|&i| i < 4).unwrap_or(0)
    }

    pub fn target_view_type(state: &AppState) -> QuadrantView {
        let q = state.settings.ui.quadrants.get(Self::target_view(state)).copied().unwrap_or(0);
        QuadrantView::from_index(q)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_files_go_to_their_package() {
        let mut rt = WorkflowRuntime::default();
        rt.push_dropped(PathBuf::from("a.prt"));
        rt.push_dropped(PathBuf::from("b.BSP"));
        rt.push_dropped(PathBuf::from("c.gma"));
        assert_eq!(rt.take_dropped(&["prt"]), vec![PathBuf::from("a.prt")]);
        assert_eq!(rt.take_dropped(&["bsp", "gma", "vpk"]), vec![PathBuf::from("b.BSP"), PathBuf::from("c.gma")]);
        assert!(rt.dropped.is_empty());
        for i in 0..MAX_DROPPED + 5 {
            rt.push_dropped(PathBuf::from(format!("{i}.prt")));
        }
        assert_eq!(rt.dropped.len(), MAX_DROPPED);
        assert_eq!(rt.dropped[0], PathBuf::from("5.prt"));
    }

    #[test]
    fn map_memory_round_trips_in_the_sidecar() {
        let mut sidecar = crate::sidecar::Sidecar::in_memory();
        assert_eq!(sidecar.get::<WorkflowMapMemory>(), WorkflowMapMemory::default());
        sidecar.set(&WorkflowMapMemory { grid: Some(2.5) });
        assert_eq!(sidecar.get::<WorkflowMapMemory>().grid, Some(2.5));
    }
}
