use glam::DVec3;
use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_editor::optimize::cubemaps::CubemapReport;
use hp_render::{Overlay, OverlayLine, OverlayTri};

use super::OverlayLayer;
use crate::overlay::box_lines_on_top;

const MAX_TRIS: usize = 120_000;
const MAX_LINES: usize = 8_000;
const GHOST: [f32; 4] = [0.25, 0.95, 1.0, 1.0];
const AMEND: [f32; 4] = [1.0, 0.85, 0.2, 1.0];
const WRONG: [f32; 4] = [1.0, 0.2, 0.2, 1.0];
const NONE: [f32; 3] = [0.5, 0.5, 0.5];

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(CubemapLayer)]
}

struct CubemapLayer;

pub fn probe_color(index: Option<usize>) -> [f32; 3] {
    let Some(i) = index else { return NONE };
    let h = 0.12 + 0.76 * (0.2 + i as f32 * 0.618_034).fract();
    let (s, v) = (0.75f32, 0.95f32);
    let k = |n: f32| (n + h * 6.0) % 6.0;
    let f = |n: f32| v - v * s * k(n).min(4.0 - k(n)).clamp(0.0, 1.0);
    [f(5.0), f(3.0), f(1.0)]
}

fn f3(p: DVec3) -> [f32; 3] {
    p.as_vec3().to_array()
}

fn rgba(c: [f32; 3], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

pub fn assignment(state: &AppState, report: &CubemapReport, is_2d: bool, out: &mut Overlay) {
    let scene = state.doc.scene();
    let mut tris = 0usize;
    let mut lines = 0usize;
    for f in &report.faces {
        let color = probe_color(f.probe);
        if !is_2d && tris < MAX_TRIS {
            let Some(face) = scene.solid(f.solid).and_then(|n| n.geom.faces.get(f.face)) else { continue };
            let lift = face.normal() * 0.5;
            let alpha = if f.reflective { 0.55 } else { 0.18 };
            let fill = if f.wrong() { rgba([1.0, 0.25, 0.25], alpha) } else { rgba(color, alpha) };
            let v = &face.vertices;
            if v.len() >= 3 {
                let p0 = f3(v[0] + lift);
                for w in v[1..].windows(2) {
                    out.triangles.push(OverlayTri { p: [p0, f3(w[0] + lift), f3(w[1] + lift)], color: fill });
                    tris += 1;
                }
            }
        }
        if f.reflective
            && lines < MAX_LINES
            && let Some(p) = f.probe.and_then(|p| report.probes.get(p))
        {
            let line = OverlayLine { a: f3(f.spot + f.normal), b: f3(p.origin), color: rgba(color, 0.9) };
            if f.wrong() {
                out.lines_on_top.push(OverlayLine { color: WRONG, ..line });
            } else {
                out.lines.push(line);
            }
            lines += 1;
        }
    }
    for (i, p) in report.probes.iter().enumerate() {
        let c = rgba(probe_color(Some(i)), 1.0);
        box_lines_on_top(out, p.origin - DVec3::splat(8.0), p.origin + DVec3::splat(8.0), c);
    }
}

pub fn ghosts(state: &AppState, out: &mut Overlay) {
    let rt = &state.runtime.cubemaps;
    for (i, s) in rt.pending() {
        let focused = rt.focus == Some(i);
        let r = if focused { 16.0 } else { 10.0 };
        let color = if s.is_new() { GHOST } else { AMEND };
        box_lines_on_top(out, s.origin - DVec3::splat(r), s.origin + DVec3::splat(r), color);
        for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
            out.lines_on_top.push(OverlayLine {
                a: f3(s.origin - axis * r * 1.6),
                b: f3(s.origin + axis * r * 1.6),
                color,
            });
        }
        out.points.push(hp_render::OverlayPoint {
            pos: f3(s.origin),
            color,
            size_px: if focused { 10.0 } else { 7.0 },
        });
        if focused && let Some(report) = &rt.report {
            for f in report.faces.iter().filter(|f| s.sides.contains(&f.side)).take(256) {
                out.lines_on_top.push(OverlayLine { a: f3(f.spot + f.normal), b: f3(s.origin), color });
            }
        }
    }
    if let (Some(report), Some(i)) = (&rt.report, rt.selected)
        && let Some(f) = report.findings.get(i)
    {
        let color = super::optimize::impact_color(f.impact);
        for b in f.boxes.iter().take(64).filter(|b| !b.is_empty()) {
            box_lines_on_top(out, b.min, b.max, color);
        }
        for p in f.points.iter().take(64) {
            box_lines_on_top(out, *p - DVec3::splat(8.0), *p + DVec3::splat(8.0), color);
        }
    }
}

impl OverlayLayer for CubemapLayer {
    fn name(&self) -> &'static str {
        "cubemaps"
    }

    fn add(&self, state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        if state.runtime.cubemaps.show {
            ghosts(state, out);
        }
    }

    fn retained_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        let rt = &state.runtime.cubemaps;
        if !rt.view || rt.report.is_none() {
            return None;
        }
        Some(rt.version.wrapping_mul(4).wrapping_add(u64::from(view.is_2d())).wrapping_add(0xC0BE_0000))
    }

    fn retained(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let rt = &state.runtime.cubemaps;
        if !rt.view {
            return;
        }
        if let Some(report) = &rt.report {
            assignment(state, report, view.is_2d(), out);
        }
    }
}

pub fn screenshot_overlay(state: &AppState, is_2d: bool, out: &mut Overlay) {
    let rt = &state.runtime.cubemaps;
    if rt.view
        && let Some(report) = &rt.report
    {
        assignment(state, report, is_2d, out);
    }
    if rt.show {
        ghosts(state, out);
    }
}

#[cfg(test)]
mod tests {
    use hp_editor::optimize::cubemaps::{CubemapKnowledge, Suggestion, analyze, class_by_name};

    use super::*;

    #[test]
    fn colors_differ_and_overlays_follow_the_toggles() {
        assert_ne!(probe_color(Some(0)), probe_color(Some(1)));
        assert_eq!(probe_color(None), NONE);
        for i in 0..32 {
            assert!(probe_color(Some(i)).iter().all(|c| (0.0..=1.0).contains(c)));
        }
        let mut state = hp_app::testing::test_state();
        let mut out = Overlay::default();
        screenshot_overlay(&state, false, &mut out);
        assert!(out.is_empty());
        let k = CubemapKnowledge {
            material: &|m: &str| class_by_name(m).or(Some(hp_editor::optimize::cubemaps::MaterialClass::Other)),
            strength: &hp_editor::optimize::cubemaps::full_strength,
        };
        let mut report = analyze(&state.doc, &k, None);
        report.suggestions.push(Suggestion {
            room: None,
            origin: DVec3::new(1.0, 2.0, 64.0),
            size: 0,
            sides: Vec::new(),
            faces: 1,
            probe: None,
            reason: "room_without_cubemap",
        });
        state.runtime.cubemaps.store(report, 1);
        state.runtime.cubemaps.show = true;
        screenshot_overlay(&state, false, &mut out);
        assert!(!out.lines_on_top.is_empty(), "the suggestion ghost is drawn");
        let ghost_only = out.triangles.len();
        state.runtime.cubemaps.view = true;
        let mut with_view = Overlay::default();
        screenshot_overlay(&state, false, &mut with_view);
        let has_faces = state.doc.scene().solids().values().any(|n| !n.geom.faces.is_empty());
        assert_eq!(with_view.triangles.len() > ghost_only, has_faces);
        let mut flat = Overlay::default();
        screenshot_overlay(&state, true, &mut flat);
        assert!(flat.triangles.is_empty(), "no fills in 2D");
    }
}
