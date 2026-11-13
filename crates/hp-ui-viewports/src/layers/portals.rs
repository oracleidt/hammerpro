use egui::{Align2, vec2};
use glam::DVec3;
use hp_app::i18n::TrTable;
use hp_app::view::ViewInfo;
use hp_app::{AppState, t, tf};
use hp_render::{Overlay, OverlayLine, OverlayPoint, OverlayTri};

use super::OverlayLayer;
use crate::overlay::{cross_marker, pixel_world_size, screen_axes, thick_line_on_top};
use crate::paint;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(PortalLayer), Box::new(LeakLayer)]
}

pub(crate) const TR: TrTable = &[
    ("viewport.instant_leak", "Leak (instant check): entity", "Fuite (vérification instantanée) : entité"),
    ("viewport.instant_void", "Leak: void", "Fuite : vide"),
    ("viewport.compiler_leak", "Leak (compiler flood): entity", "Fuite (remplissage du compilateur) : entité"),
    ("viewport.pvs_from", "Visibility from cluster {0}", "Visibilité depuis le cluster {0}"),
    ("viewport.pvs_from_cost", "Visibility from cluster {0}: {1}", "Visibilité depuis le cluster {0} : {1}"),
    (
        "viewport.pvs_cost",
        "faces {0}, triangles {1}, props {2}, overlays {3}",
        "faces {0}, triangles {1}, props {2}, overlays {3}",
    ),
    (
        "viewport.pvs_solid",
        "Visibility: in a solid or outside of the map",
        "Visibilité : dans un solide ou hors de la map",
    ),
];

fn f3(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

pub fn cluster_color(cluster: u32, alpha: f32) -> [f32; 4] {
    let h = (cluster as f32 * 0.618_034).fract();
    let (r, g, b) = hsv(h, 0.55, 1.0);
    [r, g, b, alpha]
}

fn hsv(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

pub fn heat_color(t: f32) -> [f32; 4] {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let (g, y, r) = ([0.25, 1.0, 0.35], [1.0, 0.85, 0.2], [1.0, 0.25, 0.2]);
    let (a, b, k) = if t < 0.5 { (g, y, t * 2.0) } else { (y, r, t * 2.0 - 1.0) };
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, 0.9]
}

pub const VISIBLE_COLOR: [f32; 4] = [0.25, 1.0, 0.35, 0.95];
pub const HIDDEN_COLOR: [f32; 4] = [0.55, 0.55, 0.6, 0.18];
pub const GRID_COLOR: [f32; 4] = [0.95, 0.75, 0.2, 0.35];
pub const LEAF_GRID: f64 = 1024.0;
const MAX_GRID_LINES: i64 = 128;

struct PortalLayer;

impl PortalLayer {
    fn visible(state: &AppState) -> bool {
        state.runtime.health.portals.is_some()
    }
}

impl OverlayLayer for PortalLayer {
    fn name(&self) -> &'static str {
        "portals"
    }

    fn retained_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        if !Self::visible(state) {
            return None;
        }
        let rt = &state.runtime.health;
        let s = &state.settings.health;
        let width = s.portal_width_px();
        let zoom =
            if view.is_2d() && width > 1.0 { (view.cam2d.zoom.max(1e-6).log2() * 4.0).round() as i64 } else { 0 };
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(
            &(
                rt.portal_version,
                rt.pvs_version,
                rt.pvs,
                width.to_bits(),
                s.leaf_grid,
                view.is_2d(),
                view.kind as u8,
                zoom,
            ),
            &mut h,
        );
        std::hash::Hash::hash(
            &(s.portal_heat, s.portal_xray, rt.costs.as_ref().map(|c| std::sync::Arc::as_ptr(c) as usize)),
            &mut h,
        );
        Some(std::hash::Hasher::finish(&h))
    }

    fn retained(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let rt = &state.runtime.health;
        let Some(file) = &rt.portals else { return };
        let width = state.settings.health.portal_width_px();
        let visible = rt.pvs.then_some(rt.pvs_visible.as_deref()).flatten();
        let heat = rt.costs.as_ref().map(|c| c.heat()).filter(|h| state.settings.health.portal_heat && !h.is_empty());
        let max_heat = heat.and_then(|h| h.iter().max().copied()).unwrap_or(0).max(1) as f32;
        let color_of = |clusters: [u32; 2]| match (visible, heat) {
            (Some(v), _) => {
                let seen = clusters.iter().any(|c| v.get(*c as usize).copied().unwrap_or(false));
                if seen { VISIBLE_COLOR } else { HIDDEN_COLOR }
            }
            (None, Some(h)) => {
                let cost = clusters.iter().filter_map(|c| h.get(*c as usize).copied()).max().unwrap_or(0);
                heat_color(cost as f32 / max_heat)
            }
            (None, None) => cluster_color(clusters[0].min(clusters[1]), 0.85),
        };
        let offsets: Vec<DVec3> = if view.is_2d() && width > 1.0 {
            let (r, u) = screen_axes(view);
            let px = pixel_world_size(view, DVec3::ZERO);
            thick_steps(width).into_iter().flat_map(|step| [r * px * step, u * px * step]).collect()
        } else {
            Vec::new()
        };
        let xray = state.settings.health.portal_xray && !view.is_2d();
        for p in &file.portals {
            let color = color_of(p.clusters);
            let n = p.points.len();
            let lines = if xray { &mut out.lines_on_top } else { &mut out.lines };
            for i in 0..n {
                let (a, b) = (p.points[i], p.points[(i + 1) % n]);
                lines.push(OverlayLine { a: f3(a), b: f3(b), color });
                for o in &offsets {
                    lines.push(OverlayLine { a: f3(a + *o), b: f3(b + *o), color });
                }
            }
            if !view.is_2d() && width > 1.5 {
                let fill = [color[0], color[1], color[2], 0.06 * width];
                for i in 1..n.saturating_sub(1) {
                    out.triangles
                        .push(OverlayTri { p: [f3(p.points[0]), f3(p.points[i]), f3(p.points[i + 1])], color: fill });
                }
            }
        }
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let rt = &state.runtime.health;
        if let Some(file) = &rt.portals
            && state.settings.health.leaf_grid
            && !file.bounds.is_empty()
        {
            let width = state.settings.health.leaf_width_px();
            add_leaf_grid(out, view, file.bounds.min, file.bounds.max, width);
        }
        if !rt.pvs {
            return;
        }
        if let Some(p) = rt.pvs_point {
            cross_marker(out, view, p, 9.0, VISIBLE_COLOR);
            out.points.push(OverlayPoint { pos: f3(p), color: VISIBLE_COLOR, size_px: 7.0 });
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let rt = &state.runtime.health;
        if !rt.pvs || rt.vis.is_none() {
            return;
        }
        if view.is_2d() && rt.pvs_point.is_none() {
            return;
        }
        let text = match (rt.pvs_cluster, rt.pvs_cost) {
            (Some(c), Some(cost)) => tf!(
                "viewport.pvs_from_cost",
                c,
                tf!("viewport.pvs_cost", cost.faces, cost.triangles, cost.props, cost.overlays)
            ),
            (Some(c), None) => tf!("viewport.pvs_from", c),
            (None, _) => t!("viewport.pvs_solid").to_owned(),
        };
        let pos = match rt.pvs_point.and_then(|p| view.world_to_screen(p)) {
            Some(s) => s + vec2(10.0, -10.0),
            None => view.rect.left_bottom() + vec2(8.0, -26.0),
        };
        paint::text_pill(painter, pos, Align2::LEFT_BOTTOM, text, paint::overlay_color(VISIBLE_COLOR));
    }
}

fn leaf_grid_segments(min: DVec3, max: DVec3, flat: bool) -> Vec<(DVec3, DVec3)> {
    let lo = |v: f64| (v / LEAF_GRID).floor() as i64;
    let hi = |v: f64| (v / LEAF_GRID).ceil() as i64;
    let (x0, x1, y0, y1) = (lo(min.x), hi(max.x), lo(min.y), hi(max.y));
    if x1 - x0 > MAX_GRID_LINES || y1 - y0 > MAX_GRID_LINES {
        return Vec::new();
    }
    let z = if flat { max.z } else { min.z };
    let cell = |i: i64| i as f64 * LEAF_GRID;
    let mut out = Vec::new();
    for i in x0..=x1 {
        let x = cell(i);
        if flat {
            out.push((DVec3::new(x, cell(y0), z), DVec3::new(x, cell(y1), z)));
        } else {
            for j in y0..y1 {
                out.push((DVec3::new(x, cell(j), z), DVec3::new(x, cell(j + 1), z)));
            }
            out.push((DVec3::new(x, cell(y0), min.z), DVec3::new(x, cell(y0), max.z)));
        }
    }
    for j in y0..=y1 {
        let y = cell(j);
        if flat {
            out.push((DVec3::new(cell(x0), y, z), DVec3::new(cell(x1), y, z)));
        } else {
            for i in x0..x1 {
                out.push((DVec3::new(cell(i), y, z), DVec3::new(cell(i + 1), y, z)));
            }
        }
    }
    out
}

fn thick_steps(width: f32) -> Vec<f64> {
    let half = f64::from((width - 1.0).max(0.0)) * 0.5;
    if half < 0.05 {
        return Vec::new();
    }
    let n = (half / 0.5).ceil().max(1.0) as u32;
    (1..=n).map(|k| half * f64::from(k) / f64::from(n)).collect()
}

fn add_leaf_grid(out: &mut Overlay, view: &ViewInfo, min: DVec3, max: DVec3, width: f32) {
    let (right, up) = screen_axes(view);
    let steps = thick_steps(width);
    for (a, b) in leaf_grid_segments(min, max, view.is_2d()) {
        out.lines.push(OverlayLine { a: f3(a), b: f3(b), color: GRID_COLOR });
        if steps.is_empty() {
            continue;
        }
        let px = pixel_world_size(view, (a + b) * 0.5);
        for k in &steps {
            let step = px * k;
            for o in [right * step, -right * step, up * step, -up * step] {
                out.lines.push(OverlayLine { a: f3(a + o), b: f3(b + o), color: GRID_COLOR });
            }
        }
    }
}

pub const INSTANT_LEAK_START: [f32; 4] = [1.0, 0.2, 0.9, 1.0];
pub const INSTANT_LEAK_END: [f32; 4] = [1.0, 0.55, 0.1, 1.0];

pub fn instant_gradient(t: f32) -> [f32; 4] {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let mix = |a: f32, b: f32| a * (1.0 - t) + b * t;
    [
        mix(INSTANT_LEAK_END[0], INSTANT_LEAK_START[0]),
        mix(INSTANT_LEAK_END[1], INSTANT_LEAK_START[1]),
        mix(INSTANT_LEAK_END[2], INSTANT_LEAK_START[2]),
        1.0,
    ]
}

struct LeakLayer;

impl LeakLayer {
    fn points(state: &AppState) -> Option<&[DVec3]> {
        let rt = &state.runtime.health;
        if !rt.show_leak || !state.settings.health.show_leak_path || state.pointfile.is_some() {
            return None;
        }
        rt.leak_path().map(|l| l.points.as_slice()).filter(|p| !p.is_empty())
    }
}

impl OverlayLayer for LeakLayer {
    fn name(&self) -> &'static str {
        "instant_leak"
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let Some(points) = Self::points(state) else { return };
        let segments = points.len().saturating_sub(1).max(1) as f32;
        for (i, w) in points.windows(2).enumerate() {
            let t = (i as f32 + 0.5) / segments;
            thick_line_on_top(out, view, w[0], w[1], instant_gradient(t));
        }
        if let Some(&entity) = points.last() {
            out.points.push(OverlayPoint { pos: f3(entity), color: INSTANT_LEAK_START, size_px: 11.0 });
        }
        if points.len() > 1 {
            out.points.push(OverlayPoint { pos: f3(points[0]), color: INSTANT_LEAK_END, size_px: 7.0 });
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(points) = Self::points(state) else { return };
        let by_compiler = state.runtime.health.leak.as_ref().is_some_and(|l| l.report.compiler);
        let key = if by_compiler { "viewport.compiler_leak" } else { "viewport.instant_leak" };
        let mut labels = vec![(*points.last().unwrap_or(&DVec3::ZERO), key, INSTANT_LEAK_START)];
        if points.len() > 1 {
            labels.push((points[0], "viewport.instant_void", INSTANT_LEAK_END));
        }
        for (p, key, color) in labels {
            if let Some(s) = view.world_to_screen(p)
                && view.rect.shrink(4.0).contains(s)
            {
                paint::text_pill(
                    painter,
                    s + vec2(9.0, -9.0),
                    Align2::LEFT_BOTTOM,
                    t!(key).to_owned(),
                    paint::overlay_color(color),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::{Rect, pos2};
    use glam::DVec2;
    use hp_app::view::{Camera2d, Camera3d, ViewKind};
    use hp_editor::limits::portals::parse_prt;

    use super::*;

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(801.0, 601.0)),
            cam3d: Camera3d {
                pos: DVec3::new(0.0, -300.0, 128.0),
                yaw_deg: 90.0,
                pitch_deg: 0.0,
                ..Camera3d::default()
            },
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 2.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    const PRT: &str = "PRT1\n3\n2\n4 0 1 (0 0 0) (0 64 0) (0 64 64) (0 0 64)\n3 1 2 (64 0 0) (64 64 0) (64 0 64)\n";

    #[test]
    fn heat_map_of_the_render_cost() {
        let mut state = hp_app::testing::test_state();
        state.settings.health.portal_xray = false;
        let v = view(ViewKind::Perspective);
        state.runtime.health.set_portals("x.prt".into(), parse_prt(PRT).expect("prt"));
        state.runtime.health.set_costs(hp_editor::limits::cost::RenderCosts::from_heat(vec![10, 20, 100]));
        let plain = PortalLayer.retained_version(&state, &v);
        state.settings.health.portal_heat = true;
        assert_ne!(PortalLayer.retained_version(&state, &v), plain, "the heat map redraws");
        let mut o = Overlay::default();
        PortalLayer.retained(&state, &v, &mut o);
        assert_eq!(o.lines[0].color, heat_color(0.2));
        assert_eq!(o.lines[4].color, heat_color(1.0));
        state.runtime.health.pvs = true;
        state.runtime.health.pvs_visible = Some(Arc::new(vec![true, false, false]));
        let mut o = Overlay::default();
        PortalLayer.retained(&state, &v, &mut o);
        assert_eq!(o.lines[0].color, VISIBLE_COLOR);
        assert_eq!(heat_color(f32::NAN), heat_color(0.0));
        assert_eq!(heat_color(7.0), heat_color(1.0));
        assert!(heat_color(0.5)[0] > heat_color(0.0)[0]);
    }

    #[test]
    fn portals_are_retained_and_follow_the_visibility() {
        let mut state = hp_app::testing::test_state();
        state.settings.health.portal_xray = false;
        let v = view(ViewKind::Perspective);
        assert_eq!(PortalLayer.retained_version(&state, &v), None, "nothing loaded");
        state.runtime.health.set_portals("x.prt".into(), parse_prt(PRT).expect("prt"));
        let v1 = PortalLayer.retained_version(&state, &v).expect("version");
        let mut o = Overlay::default();
        PortalLayer.retained(&state, &v, &mut o);
        assert_eq!(o.lines.len(), 4 + 3);
        assert_eq!(o.lines[0].color, cluster_color(0, 0.85));
        state.runtime.health.pvs = true;
        state.runtime.health.pvs_visible = Some(Arc::new(vec![true, false, false]));
        state.runtime.health.pvs_version += 1;
        let v2 = PortalLayer.retained_version(&state, &v).expect("version");
        assert_ne!(v1, v2);
        let mut o = Overlay::default();
        PortalLayer.retained(&state, &v, &mut o);
        assert_eq!(o.lines[0].color, VISIBLE_COLOR);
        assert_eq!(o.lines[5].color, HIDDEN_COLOR);
        state.settings.health.portal_xray = true;
        assert_ne!(PortalLayer.retained_version(&state, &v), Some(v2));
        let mut o = Overlay::default();
        PortalLayer.retained(&state, &v, &mut o);
        assert_eq!((o.lines.len(), o.lines_on_top.len()), (0, 4 + 3));
        let mut o = Overlay::default();
        PortalLayer.retained(&state, &view(ViewKind::Top), &mut o);
        assert_eq!((o.lines.len(), o.lines_on_top.len()), (4 + 3, 0));
        state.settings.health.portal_xray = false;
        state.settings.health.portal_width = 3.0;
        state.settings.health.leaf_grid = true;
        let top = view(ViewKind::Top);
        let mut o = Overlay::default();
        PortalLayer.retained(&state, &top, &mut o);
        assert_eq!(o.lines.len(), 7 * 5, "the leaf grid is not in the retained geometry");
        let mut o = Overlay::default();
        PortalLayer.retained(&state, &v, &mut o);
        assert_eq!(o.triangles.len(), 2 + 1);
        let mut thin = Overlay::default();
        PortalLayer.add(&state, &top, &mut thin);
        assert_eq!(thin.lines.len(), 2 + 2, "one line per grid line at 1 px");
        state.settings.health.leaf_width = 3.0;
        let mut thick = Overlay::default();
        PortalLayer.add(&state, &top, &mut thick);
        assert_eq!(thick.lines.len(), (2 + 2) * (1 + 2 * 4), "two half pixel steps on each side");
        state.settings.health.leaf_width = f32::NAN;
        assert_eq!(state.settings.health.leaf_width_px(), 1.0, "a broken setting stays usable");
        state.runtime.health.pvs_point = Some(DVec3::new(10.0, 10.0, 10.0));
        let mut o = Overlay::default();
        PortalLayer.add(&state, &v, &mut o);
        assert!(!o.points.is_empty());
        state.runtime.health.clear_portals();
        assert_eq!(PortalLayer.retained_version(&state, &v), None);
    }

    #[test]
    fn instant_leak_path_is_drawn_when_asked() {
        let mut state = hp_app::testing::test_state();
        hp_app::runtime::HealthRuntime::check_now(&mut state);
        let v = view(ViewKind::Top);
        let mut o = Overlay::default();
        LeakLayer.add(&state, &v, &mut o);
        assert!(o.is_empty(), "not shown until asked");
        state.runtime.health.show_leak = true;
        LeakLayer.add(&state, &v, &mut o);
        if state.runtime.health.leak_path().is_some() {
            assert!(!o.lines_on_top.is_empty());
            assert_eq!(o.points[0].color, INSTANT_LEAK_START);
        }
        state.pointfile =
            Some(hp_app::state::Pointfile { path: "x.lin".into(), points: vec![DVec3::ZERO, DVec3::ONE], cursor: 0 });
        let mut o = Overlay::default();
        LeakLayer.add(&state, &v, &mut o);
        assert!(o.is_empty());
        assert_eq!(instant_gradient(1.0), INSTANT_LEAK_START);
        assert_eq!(instant_gradient(f32::NAN), INSTANT_LEAK_END);
        assert_eq!(cluster_color(3, 0.5)[3], 0.5);
    }
}
