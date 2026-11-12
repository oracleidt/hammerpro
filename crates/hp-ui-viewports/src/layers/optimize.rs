use glam::DVec3;
use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_editor::optimize::Impact;
use hp_render::Overlay;

use super::OverlayLayer;
use crate::overlay::box_lines_on_top;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(FindingLayer)]
}

struct FindingLayer;

pub fn impact_color(i: Impact) -> [f32; 4] {
    match i {
        Impact::High => [1.0, 0.3, 0.25, 1.0],
        Impact::Medium => [1.0, 0.75, 0.2, 1.0],
        Impact::Low => [0.4, 0.8, 1.0, 1.0],
    }
}

impl OverlayLayer for FindingLayer {
    fn name(&self) -> &'static str {
        "optimize_findings"
    }

    fn add(&self, state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        let rt = &state.runtime.optimize;
        if !rt.show {
            return;
        }
        let (Some(report), Some(i)) = (rt.report.as_ref(), rt.selected) else { return };
        let Some(f) = report.findings.get(i) else { return };
        let color = impact_color(f.impact);
        for b in f.boxes.iter().take(64).filter(|b| !b.is_empty()) {
            box_lines_on_top(out, b.min, b.max, color);
        }
        for p in f.points.iter().take(64) {
            box_lines_on_top(out, *p - DVec3::splat(8.0), *p + DVec3::splat(8.0), color);
        }
    }
}

pub fn screenshot_overlay(state: &AppState) -> Overlay {
    let mut out = Overlay::default();
    let rt = &state.runtime.optimize;
    if rt.view
        && let Some(file) = &state.runtime.health.portals
    {
        let xray = state.settings.health.portal_xray;
        let f3 = |v: DVec3| v.as_vec3().to_array();
        for p in &file.portals {
            let color = super::portals::cluster_color(p.clusters[0].min(p.clusters[1]), 0.9);
            let n = p.points.len();
            for i in 0..n {
                let line = hp_render::OverlayLine { a: f3(p.points[i]), b: f3(p.points[(i + 1) % n]), color };
                if xray {
                    out.lines_on_top.push(line);
                } else {
                    out.lines.push(line);
                }
            }
        }
    }
    let selected = rt.report.as_ref().zip(rt.selected).and_then(|(r, i)| r.findings.get(i));
    if rt.show
        && let Some(f) = selected
    {
        let color = impact_color(f.impact);
        for b in f.boxes.iter().take(64).filter(|b| !b.is_empty()) {
            box_lines_on_top(&mut out, b.min, b.max, color);
        }
        for p in f.points.iter().take(64) {
            box_lines_on_top(&mut out, *p - DVec3::splat(8.0), *p + DVec3::splat(8.0), color);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use hp_editor::limits::portals::parse_prt;
    use hp_editor::optimize::{Finding, Impact, OptimizeReport, Topic};

    use super::*;

    #[test]
    fn screenshot_overlay_draws_portals_and_the_selected_finding() {
        let mut state = hp_app::testing::test_state();
        assert!(screenshot_overlay(&state).lines.is_empty());
        let prt = "PRT1\n2\n1\n4 0 1 (0 0 0) (0 64 0) (0 64 64) (0 0 64)\n";
        state.runtime.health.set_portals("x.prt".into(), parse_prt(prt).expect("prt"));
        assert!(screenshot_overlay(&state).lines.is_empty(), "only while the analysis view is on");
        state.runtime.optimize.view = true;
        state.settings.health.portal_xray = false;
        assert_eq!(screenshot_overlay(&state).lines.len(), 4);
        state.settings.health.portal_xray = true;
        assert_eq!(screenshot_overlay(&state).lines_on_top.len(), 4);
        state.runtime.optimize.show = true;
        state.runtime.optimize.selected = Some(0);
        let f = Finding::new(Topic::Hints, "x", Impact::High, String::new()).points(vec![DVec3::ZERO]);
        state.runtime.optimize.report = Some(Arc::new(OptimizeReport { findings: vec![f], ..Default::default() }));
        assert!(screenshot_overlay(&state).lines_on_top.len() > 4);
    }
}
