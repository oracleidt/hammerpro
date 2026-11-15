use std::cell::RefCell;

use egui::{Align2, vec2};
use glam::{DVec2, DVec3};
use hp_app::AppState;
use hp_app::i18n::TrTable;
use hp_app::view::ViewInfo;
use hp_geom::Aabb;
use hp_render::{Overlay, OverlayLine, OverlayTri};
use hp_vmf::Cordons;

use super::OverlayLayer;
use crate::overlay::box_lines_on_top;
use crate::paint;

pub const CORDON_COLOR: [f32; 4] = [1.0, 0.16, 0.1, 1.0];
pub const EXCLUDE_CORDON_COLOR: [f32; 4] = [1.0, 0.62, 0.05, 1.0];
const MAX_DRAWN: usize = 256;
const MAX_SHADED: usize = 64;
const LABEL_MIN_WIDTH: f32 = 48.0;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(CordonLayer::default())]
}

pub(crate) const TR: TrTable = &[];

#[derive(Debug, Default)]
struct Cache {
    cordons: Option<Cordons>,
    boxes: Vec<Aabb>,
    exclude: Vec<Aabb>,
    names: Vec<(String, Aabb)>,
}

#[derive(Debug, Default)]
struct Scratch {
    xs: Vec<f64>,
    ys: Vec<f64>,
    rects: Vec<[DVec2; 2]>,
}

#[derive(Default)]
pub struct CordonLayer {
    cache: Cache,
    scratch: RefCell<Scratch>,
}

impl CordonLayer {
    fn refresh(&mut self, state: &AppState) {
        let cordons = state.doc.scene().cordons();
        if self.cache.cordons.as_ref() == Some(cordons) {
            return;
        }
        self.cache.boxes = state.doc.cordon_boxes();
        self.cache.exclude = state.doc.exclude_cordon_boxes();
        self.cache.names.clear();
        if !self.cache.boxes.is_empty() || !self.cache.exclude.is_empty() {
            for c in cordons.cordons.iter().filter(|c| c.active).take(MAX_DRAWN) {
                let b = Aabb::new(c.mins, c.maxs);
                if c.mins.is_finite() && c.maxs.is_finite() && b.size().min_element() > 1e-3 {
                    let name = c.name.char_indices().nth(96).map_or(c.name.as_str(), |(cut, _)| &c.name[..cut]);
                    self.cache.names.push((name.to_owned(), b));
                }
            }
        }
        self.cache.cordons = Some(cordons.clone());
    }
}

fn f3(p: DVec3) -> [f32; 3] {
    p.as_vec3().to_array()
}

pub(crate) fn outside_cells(
    view: [DVec2; 2],
    rects: &[[DVec2; 2]],
    xs: &mut Vec<f64>,
    ys: &mut Vec<f64>,
    mut emit: impl FnMut(DVec2, DVec2),
) {
    let [vmin, vmax] = view;
    if !(vmin.is_finite() && vmax.is_finite()) || vmin.x >= vmax.x || vmin.y >= vmax.y {
        return;
    }
    xs.clear();
    ys.clear();
    xs.extend([vmin.x, vmax.x]);
    ys.extend([vmin.y, vmax.y]);
    for [a, b] in rects {
        xs.extend([a.x.clamp(vmin.x, vmax.x), b.x.clamp(vmin.x, vmax.x)]);
        ys.extend([a.y.clamp(vmin.y, vmax.y), b.y.clamp(vmin.y, vmax.y)]);
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    let covered = |p: DVec2| rects.iter().any(|[a, b]| p.x > a.x && p.x < b.x && p.y > a.y && p.y < b.y);
    for w in ys.windows(2) {
        let (y0, y1) = (w[0], w[1]);
        let mut run: Option<f64> = None;
        for v in xs.windows(2) {
            let (x0, x1) = (v[0], v[1]);
            let inside = covered(DVec2::new((x0 + x1) * 0.5, (y0 + y1) * 0.5));
            match (inside, run) {
                (false, None) => run = Some(x0),
                (true, Some(start)) => {
                    emit(DVec2::new(start, y0), DVec2::new(x0, y1));
                    run = None;
                }
                _ => {}
            }
        }
        if let Some(start) = run
            && let Some(&end) = xs.last()
        {
            emit(DVec2::new(start, y0), DVec2::new(end, y1));
        }
    }
}

impl OverlayLayer for CordonLayer {
    fn name(&self) -> &'static str {
        "cordons"
    }

    fn prepare(&mut self, state: &AppState) {
        self.refresh(state);
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let boxes = &self.cache.boxes;
        let exclude = &self.cache.exclude;
        if boxes.is_empty() && exclude.is_empty() {
            return;
        }
        if view.is_2d() {
            for b in boxes.iter().take(MAX_DRAWN) {
                box_lines_on_top(out, b.min, b.max, CORDON_COLOR);
            }
            for b in exclude.iter().take(MAX_DRAWN) {
                box_lines_on_top(out, b.min, b.max, EXCLUDE_CORDON_COLOR);
            }
            let settings = &state.settings.visibility;
            if settings.shade_outside_2d && !boxes.is_empty() && boxes.len() <= MAX_SHADED {
                self.shade_outside(view, settings.shade_alpha(), out);
            }
            if settings.shade_outside_2d {
                self.shade_excluded(view, settings.shade_alpha(), out);
            }
            return;
        }
        for (list, color) in [(boxes, CORDON_COLOR), (exclude, EXCLUDE_CORDON_COLOR)] {
            let faint = [color[0], color[1], color[2], 0.3];
            for b in list.iter().take(MAX_DRAWN) {
                let c = b.corners();
                for (i, j) in
                    [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)]
                {
                    out.lines.push(OverlayLine { a: f3(c[i]), b: f3(c[j]), color });
                    out.lines_on_top.push(OverlayLine { a: f3(c[i]), b: f3(c[j]), color: faint });
                }
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if !view.is_2d() || !state.settings.visibility.show_cordon_names || self.cache.names.len() < 2 {
            return;
        }
        let color = paint::overlay_color(CORDON_COLOR);
        let Some((_, u, _)) = view.kind.axes_2d() else { return };
        for (name, b) in &self.cache.names {
            let mut corner = b.min;
            corner[u] = b.max[u];
            let (Some(p), Some(q)) = (view.world_to_screen(corner), view.world_to_screen(b.max)) else { continue };
            if (q.x - p.x).abs() < LABEL_MIN_WIDTH || !view.rect.shrink(2.0).contains(p) {
                continue;
            }
            paint::text_pill(painter, p + vec2(3.0, 3.0), Align2::LEFT_TOP, name.clone(), color);
        }
    }
}

impl CordonLayer {
    fn shade_excluded(&self, view: &ViewInfo, alpha: f32, out: &mut Overlay) {
        if alpha <= 0.0 {
            return;
        }
        let Some((r, u, _)) = view.kind.axes_2d() else { return };
        let color = [0.0, 0.0, 0.0, alpha];
        let kind = view.kind;
        for b in self.cache.exclude.iter().take(MAX_SHADED) {
            let p = |x: f64, y: f64| f3(kind.from_2d(DVec2::new(x, y), 0.0));
            let (c00, c10, c11, c01) =
                (p(b.min[r], b.min[u]), p(b.max[r], b.min[u]), p(b.max[r], b.max[u]), p(b.min[r], b.max[u]));
            out.triangles.push(OverlayTri { p: [c00, c10, c11], color });
            out.triangles.push(OverlayTri { p: [c00, c11, c01], color });
        }
    }

    fn shade_outside(&self, view: &ViewInfo, alpha: f32, out: &mut Overlay) {
        if alpha <= 0.0 {
            return;
        }
        let Some((r, u, _)) = view.kind.axes_2d() else { return };
        let Ok(mut guard) = self.scratch.try_borrow_mut() else { return };
        let Scratch { xs, ys, rects } = &mut *guard;
        rects.clear();
        rects.extend(self.cache.boxes.iter().map(|b| [DVec2::new(b.min[r], b.min[u]), DVec2::new(b.max[r], b.max[u])]));
        let a = view.screen_to_2d(view.rect.left_top());
        let b = view.screen_to_2d(view.rect.right_bottom());
        let color = [0.0, 0.0, 0.0, alpha];
        let kind = view.kind;
        outside_cells([a.min(b), a.max(b)], rects, xs, ys, |lo, hi| {
            let p = |x: f64, y: f64| f3(kind.from_2d(DVec2::new(x, y), 0.0));
            let (c00, c10, c11, c01) = (p(lo.x, lo.y), p(hi.x, lo.y), p(hi.x, hi.y), p(lo.x, hi.y));
            out.triangles.push(OverlayTri { p: [c00, c10, c11], color });
            out.triangles.push(OverlayTri { p: [c00, c11, c01], color });
        });
    }
}

#[cfg(test)]
mod tests {
    use egui::{Rect, pos2};
    use hp_app::view::{Camera2d, Camera3d, ViewKind};

    use super::*;

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 1,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)),
            cam3d: Camera3d {
                pos: DVec3::new(-500.0, -500.0, 300.0),
                yaw_deg: 45.0,
                pitch_deg: -20.0,
                ..Camera3d::default()
            },
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 0.25 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    fn area(cells: &[(DVec2, DVec2)]) -> f64 {
        cells.iter().map(|(a, b)| (b.x - a.x) * (b.y - a.y)).sum()
    }

    #[test]
    fn outside_cells_cover_exactly_the_outside() {
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        let vr = [DVec2::new(-100.0, -100.0), DVec2::new(100.0, 100.0)];
        let mut cells = Vec::new();
        outside_cells(vr, &[], &mut xs, &mut ys, |a, b| cells.push((a, b)));
        assert_eq!(cells, vec![(vr[0], vr[1])]);
        cells.clear();
        let rects =
            [[DVec2::new(-50.0, -50.0), DVec2::new(0.0, 0.0)], [DVec2::new(-25.0, -25.0), DVec2::new(500.0, 25.0)]];
        outside_cells(vr, &rects, &mut xs, &mut ys, |a, b| cells.push((a, b)));
        let inside = 50.0 * 50.0 + 125.0 * 50.0 - 25.0 * 25.0;
        assert!((area(&cells) - (200.0 * 200.0 - inside)).abs() < 1e-6, "{}", area(&cells));
        for (a, b) in &cells {
            let c = (*a + *b) * 0.5;
            assert!(!rects.iter().any(|[p, q]| c.x > p.x && c.x < q.x && c.y > p.y && c.y < q.y));
        }
        cells.clear();
        outside_cells(vr, &[[DVec2::splat(-1e9), DVec2::splat(1e9)]], &mut xs, &mut ys, |a, b| cells.push((a, b)));
        assert!(cells.is_empty());
        outside_cells([DVec2::NAN, DVec2::ONE], &rects, &mut xs, &mut ys, |a, b| cells.push((a, b)));
        assert!(cells.is_empty());
    }

    #[test]
    fn draws_only_while_cordons_are_in_use() {
        let mut state = hp_app::testing::test_state();
        let mut layer = CordonLayer::default();
        let views = [view(ViewKind::Perspective), view(ViewKind::Top), view(ViewKind::Front), view(ViewKind::Side)];
        layer.prepare(&state);
        for v in &views {
            let mut out = Overlay::default();
            layer.add(&state, v, &mut out);
            assert!(out.is_empty(), "the test map's cordon is off");
        }
        state.doc.set_cordon_box(0, Aabb::new(DVec3::splat(-128.0), DVec3::splat(128.0))).expect("box");
        state.doc.add_cordon("second", Aabb::new(DVec3::splat(200.0), DVec3::splat(300.0))).expect("add");
        state.doc.set_cordons_enabled(true).expect("on");
        layer.prepare(&state);
        let mut out = Overlay::default();
        layer.add(&state, &views[0], &mut out);
        assert_eq!(out.lines.len(), 24);
        assert!(out.triangles.is_empty(), "no shading in 3D");
        let mut out = Overlay::default();
        layer.add(&state, &views[1], &mut out);
        assert_eq!(out.lines_on_top.len(), 24);
        assert!(!out.triangles.is_empty(), "the outside is shaded in 2D");
        state.settings.visibility.shade_outside_2d = false;
        let mut out = Overlay::default();
        layer.add(&state, &views[1], &mut out);
        assert!(out.triangles.is_empty());
        hp_app::testing::run_ui(&mut state, |ui, state| {
            for v in &views {
                layer.paint(state, v, ui.painter());
            }
        });
        state.doc.set_cordons_enabled(false).expect("off");
        layer.prepare(&state);
        let mut out = Overlay::default();
        layer.add(&state, &views[1], &mut out);
        assert!(out.is_empty());
    }
}
