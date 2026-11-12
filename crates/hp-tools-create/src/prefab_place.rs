use std::sync::Arc;

use egui::{Align2, Pos2, RichText, Ui};
use glam::{DVec2, DVec3};
use hp_app::i18n::TrTable;
use hp_app::runtime::{PrefabLoad, PrefabTokens};
use hp_app::state::AppState;
use hp_app::{Severity, ToolFlow, ViewInfo, t, tf};
use hp_editor::EditResult;
use hp_editor::prefab::{InsertOptions, InsertedPrefab, Prefab, PrefabPlacement, fit_box};
use hp_geom::Aabb;
use hp_render::Overlay;

use crate::common::{
    COLOR_GHOST, Mouse, Ov, Snap, box_screen_rect, fresh, ground_point, paint_label, pick_surface, rgba,
};

const MAX_GHOST_EDGES: usize = 4096;
const MIN_DRAG: f64 = 1.0;

pub const PREFAB_TR: TrTable = &[
    ("prefab_place.prefab", "Prefab:", "Prefab :"),
    ("prefab_place.loading", "Loading...", "Chargement..."),
    ("prefab_place.back", "Back to entities", "Revenir aux entités"),
    (
        "prefab_place.back_tip",
        "Place entities again instead of the prefab (Escape)",
        "Placer à nouveau des entités au lieu du prefab (Échap)",
    ),
    ("prefab_place.group", "Group", "Grouper"),
    (
        "prefab_place.group_tip",
        "Group the objects of each inserted prefab",
        "Grouper les objets de chaque prefab inséré",
    ),
    (
        "prefab_place.hint",
        "Prefab {0}: click to insert it, drag a box in a 2D view to insert it scaled, Escape to place entities",
        "Prefab {0} : cliquez pour l'insérer, tracez un rectangle dans une vue 2D pour l'insérer à l'échelle, Échap pour placer des entités",
    ),
    ("prefab_place.still_loading", "The prefab is still loading", "Le prefab est encore en chargement"),
    ("prefab_place.failed", "Cannot use the prefab {0}: {1}", "Impossible d'utiliser le prefab {0} : {1}"),
    ("prefab_place.insert_failed", "Prefab not inserted: {0}", "Prefab non inséré : {0}"),
    ("prefab_place.size", "{0} x {1} x {2}", "{0} x {1} x {2}"),
];

pub fn insert_prefab(state: &mut AppState, prefab: &Prefab, placement: PrefabPlacement) -> EditResult<InsertedPrefab> {
    let token = PrefabTokens::take(&mut state.sidecar, &state.doc, prefab);
    let opts = InsertOptions { token, group: state.settings.placement.group_prefabs };
    state.doc.insert_prefab(prefab, placement, &opts)
}

pub fn rest_offset(bounds: &Aabb, point: DVec3, normal: DVec3) -> DVec3 {
    let n = normal.try_normalize().filter(|n| n.is_finite()).unwrap_or(DVec3::Z);
    let half = bounds.size() * 0.5;
    let push = n.x.abs() * half.x + n.y.abs() * half.y + n.z.abs() * half.z;
    point + n * push - bounds.center()
}

fn snap_offset(offset: DVec3, normal: Option<DVec3>, snap: &Snap) -> DVec3 {
    let keep = normal.and_then(|n| {
        let n = n.try_normalize()?;
        (0..3).find(|&i| n[i].abs() > 0.999)
    });
    let mut out = offset;
    for k in 0..3 {
        if Some(k) != keep {
            out[k] = snap.len(offset[k], k);
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Offset(DVec3),
    Fit(Aabb),
}

impl Target {
    fn placement(self) -> PrefabPlacement {
        match self {
            Target::Offset(d) => PrefabPlacement::Offset(d),
            Target::Fit(b) => PrefabPlacement::Fit(b),
        }
    }

    fn map(self, bounds: &Aabb, p: DVec3) -> DVec3 {
        match self {
            Target::Offset(d) => p + d,
            Target::Fit(to) => {
                let to = fit_box(bounds, &to);
                let (fs, ts) = (bounds.size(), to.size());
                let mut out = DVec3::ZERO;
                for k in 0..3 {
                    let s = if fs[k] > 1e-9 { ts[k] / fs[k] } else { 1.0 };
                    out[k] = to.min[k] + (p[k] - bounds.min[k]) * s;
                }
                out
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Ghost {
    view: usize,
    frame: u64,
    in_3d: bool,
    target: Target,
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    view: usize,
    press: DVec2,
    start: DVec2,
    end: DVec2,
    depth: f64,
}

#[derive(Debug, Default)]
pub(crate) struct PrefabPlacer {
    ghost: Option<Ghost>,
    drag: Option<Drag>,
    reported: Option<std::path::PathBuf>,
}

impl PrefabPlacer {
    pub fn is_armed(state: &AppState) -> bool {
        state.runtime.placement.armed_prefab.is_some()
    }

    pub fn clear(&mut self) {
        self.ghost = None;
        self.drag = None;
    }

    pub fn disarm(&mut self, state: &mut AppState) {
        state.runtime.placement.armed_prefab = None;
        self.clear();
    }

    fn prefab(&mut self, state: &mut AppState, repaint: Option<&egui::Context>) -> Option<Arc<Prefab>> {
        let path = state.runtime.placement.armed_prefab.clone()?;
        match state.runtime.placement.prefabs.get(&path, repaint) {
            PrefabLoad::Ready(p) => Some(p),
            PrefabLoad::Loading => None,
            PrefabLoad::Failed(e) => {
                if self.reported.as_ref() != Some(&path) {
                    self.reported = Some(path.clone());
                    state.notify(Severity::Error, tf!("prefab_place.failed", file_label(&path), e));
                }
                self.disarm(state);
                None
            }
        }
    }

    fn target(state: &AppState, view: &ViewInfo, pos: Pos2, bounds: &Aabb, depth: f64, snap: &Snap) -> Option<Target> {
        if let Some((r, u, k)) = view.kind.axes_2d() {
            let p2 = snap.v2(view.screen_to_2d(pos));
            let c = bounds.center();
            let mut d = DVec3::ZERO;
            d[r] = p2.x - c[r];
            d[u] = p2.y - c[u];
            d[k] = depth - c[k];
            let d = snap_offset(d, None, snap);
            return d.is_finite().then_some(Target::Offset(d));
        }
        let (point, normal) = match pick_surface(state, view, pos) {
            Some(hit) => (hit.point, hit.normal),
            None => (ground_point(view, pos)?, DVec3::Z),
        };
        let d = snap_offset(rest_offset(bounds, point, normal), Some(normal), snap);
        d.is_finite().then_some(Target::Offset(d))
    }

    fn drag_box(view: &ViewInfo, drag: &Drag, bounds: &Aabb, snap: &Snap) -> Option<Aabb> {
        let (r, u, k) = view.kind.axes_2d()?;
        let min_size = if snap.is_on() { snap.step() * MIN_DRAG } else { MIN_DRAG };
        let (a, b) = (drag.start, drag.end);
        if (a.x - b.x).abs() < min_size || (a.y - b.y).abs() < min_size {
            return None;
        }
        let mut lo = DVec3::ZERO;
        let mut hi = DVec3::ZERO;
        lo[r] = a.x.min(b.x);
        hi[r] = a.x.max(b.x);
        lo[u] = a.y.min(b.y);
        hi[u] = a.y.max(b.y);
        let half = bounds.size()[k] * 0.5;
        lo[k] = drag.depth - half;
        hi[k] = drag.depth + half;
        let b = Aabb { min: lo, max: hi };
        (b.min.is_finite() && b.max.is_finite()).then_some(b)
    }

    fn insert(&mut self, state: &mut AppState, prefab: &Prefab, target: Target) {
        if let Err(e) = insert_prefab(state, prefab, target.placement()) {
            state.notify(Severity::Error, tf!("prefab_place.insert_failed", e));
        }
    }

    pub fn handle_mouse(
        &mut self,
        state: &mut AppState,
        view: &ViewInfo,
        m: &Mouse,
        depth_2d: f64,
        repaint: Option<&egui::Context>,
    ) -> ToolFlow {
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let prefab = self.prefab(state, repaint);
        let snap = Snap::new(state, view, m.modifiers);
        let Some(prefab) = prefab else {
            self.clear();
            if m.activated() && Self::is_armed(state) {
                state.notify(Severity::Info, t!("prefab_place.still_loading"));
            }
            return if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored };
        };
        let Some(bounds) = prefab.bounds() else {
            return if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored };
        };
        if view.is_2d() {
            let p2 = snap.v2(view.screen_to_2d(pos));
            if m.drag_started {
                let press = view.screen_to_2d(m.origin().unwrap_or(pos));
                self.drag = Some(Drag { view: view.index, press, start: snap.v2(press), end: p2, depth: depth_2d });
            }
            if let Some(drag) = &mut self.drag
                && drag.view == view.index
                && (m.dragging || m.drag_stopped)
            {
                drag.end = p2;
                let fit = Self::drag_box(view, drag, &bounds, &snap);
                let target = match fit {
                    Some(b) => Some(Target::Fit(b)),
                    None => Self::target(state, view, view.screen_of_2d(drag.press), &bounds, drag.depth, &snap),
                };
                self.ghost =
                    target.map(|target| Ghost { view: view.index, frame: state.frame.index, in_3d: false, target });
                if m.drag_stopped {
                    self.drag = None;
                    if let Some(target) = target {
                        self.insert(state, &prefab, target);
                    }
                }
                return ToolFlow::Handled;
            }
        }
        let target = Self::target(state, view, pos, &bounds, depth_2d, &snap);
        self.ghost =
            target.map(|target| Ghost { view: view.index, frame: state.frame.index, in_3d: !view.is_2d(), target });
        let place = if view.is_2d() { m.clicked } else { m.activated() };
        if place && let Some(target) = target {
            self.insert(state, &prefab, target);
        }
        if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored }
    }

    fn loaded(state: &AppState) -> Option<Arc<Prefab>> {
        let path = state.runtime.placement.armed_prefab.as_ref()?;
        state.runtime.placement.prefabs.peek(path)
    }

    pub fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let Some(g) = self.ghost.filter(|g| fresh(state, g.frame)) else { return };
        if !g.in_3d && g.view != view.index {
            return;
        }
        let Some(prefab) = Self::loaded(state) else { return };
        let Some(bounds) = prefab.bounds() else { return };
        let mut ov = Ov::new(out, view);
        let color = rgba(COLOR_GHOST, 0.9);
        let placed = Aabb::from_points(bounds.corners().map(|c| g.target.map(&bounds, c)));
        ov.box_edges(&placed, rgba(COLOR_GHOST, 0.6), true);
        for [a, b] in prefab.preview_edges().iter().take(MAX_GHOST_EDGES) {
            ov.line(g.target.map(&bounds, *a), g.target.map(&bounds, *b), color);
        }
    }

    pub fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(g) = self.ghost.filter(|g| fresh(state, g.frame) && g.view == view.index) else { return };
        let Some(path) = state.runtime.placement.armed_prefab.as_ref() else { return };
        let Some(prefab) = Self::loaded(state) else { return };
        let Some(bounds) = prefab.bounds() else { return };
        let placed = Aabb::from_points(bounds.corners().map(|c| g.target.map(&bounds, c)));
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let mut text = file_label(path);
        if let Target::Fit(_) = g.target {
            let s = placed.size();
            let f = |x: f64| hp_app::widgets::fmt_coord((x * 100.0).round() / 100.0);
            text = format!("{text}  {}", tf!("prefab_place.size", f(s.x), f(s.y), f(s.z)));
        }
        let anchor = if view.is_2d() {
            box_screen_rect(view, &placed).map(|r| r.center_top() - egui::vec2(0.0, 4.0))
        } else {
            view.world_to_screen(placed.center()).filter(|p| view.rect.contains(*p))
        };
        if let Some(p) = anchor {
            paint_label(&painter, p, Align2::CENTER_BOTTOM, &text, COLOR_GHOST);
        }
    }

    pub fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let Some(path) = state.runtime.placement.armed_prefab.clone() else { return };
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("prefab_place.prefab"));
            let loading = Self::loaded(state).is_none();
            let mut text = RichText::new(file_label(&path)).strong();
            if loading {
                text = RichText::new(t!("prefab_place.loading")).weak().italics();
            }
            ui.label(text).on_hover_text(path.display().to_string());
            if ui.button(t!("prefab_place.back")).on_hover_text(t!("prefab_place.back_tip")).clicked() {
                self.disarm(state);
            }
            ui.separator();
            if ui
                .checkbox(&mut state.settings.placement.group_prefabs, t!("prefab_place.group"))
                .on_hover_text(t!("prefab_place.group_tip"))
                .changed()
            {
                state.settings_dirty = true;
            }
        });
    }

    pub fn status_hint(&self, state: &AppState) -> Option<String> {
        let path = state.runtime.placement.armed_prefab.as_ref()?;
        Some(tf!("prefab_place.hint", file_label(path)))
    }

    pub fn cancel(&mut self, state: &mut AppState) -> bool {
        if self.drag.take().is_some() {
            return true;
        }
        if Self::is_armed(state) {
            self.disarm(state);
            return true;
        }
        false
    }
}

pub fn file_label(path: &std::path::Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{info_2d, info_3d_looking_down, settle};
    use hp_app::ViewKind;

    const PREFAB: &str = r#"world
{
	"id" "1"
	"classname" "worldspawn"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "3" "plane" "(0 64 64) (0 0 64) (0 0 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "4" "plane" "(64 64 0) (64 0 0) (64 0 64)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "5" "plane" "(64 64 64) (0 64 64) (0 64 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "6" "plane" "(64 0 0) (0 0 0) (0 0 64)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
	}
}
entity
{
	"id" "3"
	"classname" "info_target"
	"targetname" "crate_&i"
	"origin" "32 32 32"
}
"#;

    fn armed(test: &str) -> (AppState, std::path::PathBuf) {
        let mut state = settle();
        let dir = std::env::temp_dir().join(format!("hp_prefab_place_{test}_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("crate.vmf");
        std::fs::write(&path, PREFAB).expect("write");
        state.runtime.placement.armed_prefab = Some(path.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while matches!(state.runtime.placement.prefabs.get(&path, None), PrefabLoad::Loading) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let _ = std::fs::remove_dir_all(&dir);
        (state, path)
    }

    fn solids_named(state: &AppState) -> Vec<String> {
        let mut v: Vec<String> = state
            .doc
            .scene()
            .entities()
            .values()
            .filter(|n| n.entity.classname == "info_target")
            .filter_map(|n| n.entity.get("targetname").map(str::to_owned))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn click_in_3d_rests_on_the_floor_with_unique_names() {
        let (mut state, _) = armed("click");
        let mut p = PrefabPlacer::default();
        let view = info_3d_looking_down(DVec3::new(-100.0, -100.0, 600.0));
        let at = view.world_to_screen(DVec3::new(-100.0, -100.0, 0.0)).expect("screen");
        p.handle_mouse(&mut state, &view, &Mouse::hover(at), 0.0, None);
        assert!(p.ghost.is_some());
        let undo = state.doc.history_len().0;
        p.handle_mouse(&mut state, &view, &Mouse::click(at), 0.0, None);
        assert_eq!(state.doc.history_len().0, undo + 1);
        assert_eq!(state.doc.undo_label(), Some("Insert prefab"));
        let sel = state.doc.selected_objects();
        assert_eq!(sel.len(), 2);
        let b = state.doc.selection_bounds(&|e| state.entity_bounds(e)).expect("bounds");
        assert_eq!(b.min.z, 0.0, "rests on the floor: {b:?}");
        assert_eq!(b.min.x % 16.0, 0.0, "on the grid: {b:?}");
        p.handle_mouse(&mut state, &view, &Mouse::click(at), 0.0, None);
        assert_eq!(solids_named(&state), ["crate_1", "crate_2"]);
        assert!(p.cancel(&mut state));
        assert!(!PrefabPlacer::is_armed(&state));
        assert!(!p.cancel(&mut state));
    }

    #[test]
    fn drag_in_2d_scales_to_the_box() {
        let (mut state, _) = armed("drag");
        let mut p = PrefabPlacer::default();
        let view = info_2d(ViewKind::Top, 1.0);
        let a = view.world_to_screen(DVec3::new(-192.0, -192.0, 0.0)).expect("a");
        let b = view.world_to_screen(DVec3::new(0.0, -128.0, 0.0)).expect("b");
        p.handle_mouse(&mut state, &view, &Mouse::drag_start(a, b), 32.0, None);
        p.handle_mouse(&mut state, &view, &Mouse::drag(a, b), 32.0, None);
        assert!(matches!(p.ghost.map(|g| g.target), Some(Target::Fit(_))));
        p.handle_mouse(&mut state, &view, &Mouse::drag_end(a, b), 32.0, None);
        let bounds = state.doc.selection_bounds(&|e| state.entity_bounds(e)).expect("bounds");
        assert_eq!(bounds.min.x, -192.0);
        assert_eq!(bounds.max.x, 0.0);
        assert_eq!(bounds.min.y, -192.0);
        assert_eq!(bounds.max.y, -128.0);
        assert_eq!(bounds.size().z, 64.0, "depth keeps its size");
        assert_eq!(bounds.center().z, 32.0, "centered on the depth");
        let c = view.world_to_screen(DVec3::new(100.0, 100.0, 0.0)).expect("c");
        p.handle_mouse(&mut state, &view, &Mouse::click(c), 32.0, None);
        let bounds = state.doc.selection_bounds(&|e| state.entity_bounds(e)).expect("bounds");
        assert_eq!(bounds.center().x, 96.0);
        assert_eq!(bounds.center().y, 96.0);
    }

    #[test]
    fn translations() {
        let problems = hp_app::i18n::lint_table(PREFAB_TR);
        assert!(problems.is_empty(), "{problems:#?}");
        let src = include_str!("prefab_place.rs");
        for (k, en, fr) in PREFAB_TR {
            assert!(k.starts_with("prefab_place."), "{k}");
            assert!(src.matches(&format!("\"{k}\"")).count() >= 2, "unused key {k}");
            for i in 0..3 {
                let p = format!("{{{i}}}");
                assert_eq!(en.contains(&p), fr.contains(&p), "{k}: {p}");
            }
        }
    }

    #[test]
    fn loading_and_failures() {
        let mut state = settle();
        let mut p = PrefabPlacer::default();
        state.runtime.placement.armed_prefab = Some(std::env::temp_dir().join("hp_no_such_prefab_dir/x.vmf"));
        let view = info_2d(ViewKind::Top, 1.0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while PrefabPlacer::is_armed(&state) {
            p.handle_mouse(&mut state, &view, &Mouse::click(view.rect.center()), 0.0, None);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(p.status_hint(&state).is_none());
        let b = Aabb::new(DVec3::ZERO, DVec3::splat(64.0));
        assert_eq!(rest_offset(&b, DVec3::new(10.0, 10.0, 0.0), DVec3::Z), DVec3::new(-22.0, -22.0, 0.0));
        assert_eq!(rest_offset(&b, DVec3::new(0.0, 0.0, 128.0), DVec3::NEG_Z), DVec3::new(-32.0, -32.0, 64.0));
        let s = Snap::grid(16.0);
        assert_eq!(snap_offset(DVec3::new(-22.0, -22.0, 3.0), Some(DVec3::Z), &s), DVec3::new(-16.0, -16.0, 3.0));
        let fit = Target::Fit(Aabb::new(DVec3::ZERO, DVec3::new(128.0, 64.0, 64.0)));
        assert_eq!(fit.map(&b, DVec3::splat(64.0)), DVec3::new(128.0, 64.0, 64.0));
        assert_eq!(file_label(std::path::Path::new("a/b/door.vmf")), "door");
    }
}
