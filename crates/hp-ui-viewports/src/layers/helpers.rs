use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use glam::DVec3;
use hp_app::AppState;
use hp_app::i18n::TrTable;
use hp_app::runtime::helpers::geometry::{
    ConnectionLink, HelperContext, HelperGeometry, HelperSphere, class_has_helpers, class_has_path_links,
    entity_helpers, entity_outputs, incoming_outputs, path_links, rope_link,
};
use hp_app::settings::{ConnectionsMode, HelperScope};
use hp_app::view::ViewInfo;
use hp_editor::{Document, Id, IdSet};
use hp_render::{Overlay, OverlayLine, OverlayPoint};

use super::OverlayLayer;
use crate::overlay::{face_highlight, pixel_world_size, screen_axes};

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(HelpersLayer::default()), Box::new(ConnectionsLayer::default())]
}

pub(crate) const TR: TrTable = &[];

const MAX_SELECTED: usize = 512;
const SIDE_COLOR: [f32; 4] = [0.35, 0.85, 1.0, 1.0];
const HANDLE_COLOR: [f32; 4] = [1.0, 0.9, 0.2, 1.0];

fn f3(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

fn rgba(c: [u8; 3], alpha: f32) -> [f32; 4] {
    [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0, alpha]
}

fn sphere_lines(view: &ViewInfo, s: &HelperSphere, alpha: f32, out: &mut Vec<OverlayLine>) {
    let color = rgba(s.color, alpha);
    let circle = |a: DVec3, b: DVec3, segments: usize, out: &mut Vec<OverlayLine>| {
        let mut prev = s.center + a * s.radius;
        for i in 1..=segments {
            let t = i as f64 / segments as f64 * std::f64::consts::TAU;
            let p = s.center + (a * t.cos() + b * t.sin()) * s.radius;
            out.push(OverlayLine { a: f3(prev), b: f3(p), color });
            prev = p;
        }
    };
    if view.is_2d() {
        let (r, u) = screen_axes(view);
        circle(r, u, 64, out);
    } else {
        circle(DVec3::X, DVec3::Y, 48, out);
        circle(DVec3::X, DVec3::Z, 48, out);
        circle(DVec3::Y, DVec3::Z, 48, out);
    }
}

fn geometry_lines(view: &ViewInfo, g: &HelperGeometry, alpha: f32, out: &mut Vec<OverlayLine>) {
    for l in &g.lines {
        out.push(OverlayLine { a: f3(l.a), b: f3(l.b), color: rgba(l.color, alpha) });
    }
    for s in &g.spheres {
        sphere_lines(view, s, alpha, out);
    }
}

fn arrow_head(view: &ViewInfo, from: DVec3, tip: DVec3, size: f64, color: [f32; 4], out: &mut Vec<OverlayLine>) {
    let d = tip - from;
    let dir = if view.is_2d() {
        let toward = view.kind.toward_viewer();
        (d - toward * d.dot(toward)).try_normalize()
    } else {
        d.try_normalize()
    };
    let Some(dir) = dir else { return };
    let sides: Vec<DVec3> = if view.is_2d() {
        let side = view.kind.toward_viewer().cross(dir);
        vec![side, -side]
    } else {
        let (a, b) = dir.any_orthonormal_pair();
        vec![a, -a, b, -b]
    };
    let back = tip - dir * size;
    for s in sides {
        out.push(OverlayLine { a: f3(tip), b: f3(back + s * size * 0.45), color });
    }
}

fn link_lines(view: &ViewInfo, links: &[ConnectionLink], arrow_px: Option<f64>, out: &mut Vec<OverlayLine>) {
    for l in links {
        let color = rgba(l.state.color(), 1.0);
        if l.to.is_none() {
            let s = arrow_px.map_or(6.0, |px| pixel_world_size(view, l.a) * px * 0.6);
            for (a, b) in [
                (DVec3::new(1.0, 1.0, 1.0), DVec3::new(-1.0, -1.0, -1.0)),
                (DVec3::new(1.0, -1.0, 1.0), DVec3::new(-1.0, 1.0, -1.0)),
            ] {
                out.push(OverlayLine { a: f3(l.a + a * s), b: f3(l.a + b * s), color });
            }
            if view.is_2d() {
                let (r, u) = screen_axes(view);
                out.push(OverlayLine { a: f3(l.a + (r + u) * s), b: f3(l.a - (r + u) * s), color });
                out.push(OverlayLine { a: f3(l.a + (r - u) * s), b: f3(l.a - (r - u) * s), color });
            }
            continue;
        }
        let len = (l.b - l.a).length();
        if len < 1e-6 {
            continue;
        }
        out.push(OverlayLine { a: f3(l.a), b: f3(l.b), color });
        let size = match arrow_px {
            Some(px) => (pixel_world_size(view, l.b) * px).min(len * 0.5),
            None => (len * 0.15).clamp(2.0, 16.0),
        };
        arrow_head(view, l.a, l.b, size, color, out);
    }
}

fn selection_hash(doc: &Document) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let sel = &doc.selection().objects;
    sel.len().hash(&mut h);
    for o in sel {
        o.hash(&mut h);
    }
    h.finish()
}

fn selected_entities(doc: &Document) -> Vec<Id> {
    let mut ids = doc.selected_entity_ids();
    ids.truncate(MAX_SELECTED);
    ids
}

fn visible_entities(doc: &Document) -> impl Iterator<Item = (Id, &hp_vmf::Entity)> + '_ {
    doc.scene().entities().iter().filter(|(id, _)| doc.is_entity_visible(**id)).map(|(id, n)| (*id, &n.entity))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    revision: u64,
    selection: u64,
    generation: u64,
}

#[derive(Default)]
struct HelpersLayer {
    selected: HelperGeometry,
    selected_stamp: Option<Stamp>,
    side_faces: Vec<Vec<DVec3>>,
    all: HelperGeometry,
    all_stamp: Option<Stamp>,
    all_version: u64,
    relevance: HashMap<String, bool>,
    links: HelperGeometry,
    links_stamp: Option<(u64, u64)>,
    link_classes: HashMap<String, bool>,
    relevance_gen: u64,
    enabled: bool,
    scope_all: bool,
}

impl HelpersLayer {
    fn relevant(&mut self, state: &AppState, classname: &str) -> bool {
        if let Some(r) = self.relevance.get(classname) {
            return *r;
        }
        let r = class_has_helpers(state.assets.fgd().and_then(|f| f.class(classname)), classname);
        self.relevance.insert(classname.to_owned(), r);
        r
    }

    fn links_class(&mut self, state: &AppState, classname: &str) -> bool {
        if let Some(r) = self.link_classes.get(classname) {
            return *r;
        }
        let r = class_has_path_links(state.assets.fgd().and_then(|f| f.class(classname)), classname);
        self.link_classes.insert(classname.to_owned(), r);
        r
    }
}

impl OverlayLayer for HelpersLayer {
    fn name(&self) -> &'static str {
        "helpers"
    }

    fn prepare(&mut self, state: &AppState) {
        self.enabled = state.settings.view.show_helpers && state.settings.helpers.show_editor_objects;
        self.scope_all = self.enabled && state.settings.helpers.helper_scope == HelperScope::All;
        if !self.enabled {
            self.selected.clear();
            self.side_faces.clear();
            self.selected_stamp = None;
            return;
        }
        let doc = &state.doc;
        let generation = state.assets.generation().wrapping_mul(2) | u64::from(state.settings.helpers.show_ropes);
        if generation != self.relevance_gen {
            self.relevance.clear();
            self.link_classes.clear();
            self.relevance_gen = generation;
        }
        let stamp = Stamp { revision: doc.revision(), selection: selection_hash(doc), generation };
        let fgd = state.assets.fgd();
        let needs_selected = self.selected_stamp != Some(stamp);
        let needs_all =
            self.scope_all && self.all_stamp.map(|s| (s.revision, s.generation)) != Some((stamp.revision, generation));
        let needs_links = !self.scope_all && self.links_stamp != Some((stamp.revision, generation));
        if self.scope_all && self.links_stamp.is_some() {
            self.links.clear();
            self.links_stamp = None;
            self.all_version = self.all_version.wrapping_add(1);
        }
        if !needs_selected && !needs_all && !needs_links {
            return;
        }
        let names = state.runtime.helpers.names(doc);
        let cx = HelperContext { doc, fgd, names: &names };
        if needs_selected {
            self.selected.clear();
            for id in selected_entities(doc) {
                entity_helpers(&cx, id, &mut self.selected);
                rope_link(&cx, id, &mut self.selected);
            }
            self.side_faces.clear();
            if !self.selected.sides.is_empty() {
                let wanted: IdSet = self.selected.sides.iter().copied().collect();
                let found = doc.find_sides(&wanted);
                for face in found.values() {
                    if let Some(poly) = doc.face_polygon(*face) {
                        self.side_faces.push(poly.to_vec());
                    }
                }
            }
            self.selected_stamp = Some(stamp);
        }
        if needs_all {
            self.all.clear();
            let mut ids: Vec<Id> = Vec::new();
            for (id, e) in visible_entities(doc) {
                if self.relevant(state, &e.classname) {
                    ids.push(id);
                }
            }
            for id in ids {
                entity_helpers(&cx, id, &mut self.all);
            }
            self.all.sides.clear();
            self.all.handles.clear();
            self.all_stamp = Some(stamp);
            self.all_version = self.all_version.wrapping_add(1);
        }
        if needs_links {
            self.links.clear();
            let mut ids: Vec<Id> = Vec::new();
            for (id, e) in visible_entities(doc) {
                if self.links_class(state, &e.classname) {
                    ids.push(id);
                }
            }
            for id in ids {
                path_links(&cx, id, &mut self.links);
            }
            self.links_stamp = Some((stamp.revision, generation));
            self.all_version = self.all_version.wrapping_add(1);
        }
    }

    fn add(&self, _state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if !self.enabled {
            return;
        }
        geometry_lines(view, &self.selected, 1.0, &mut out.lines_on_top);
        for h in &self.selected.handles {
            out.points.push(OverlayPoint { pos: f3(h.pos), color: HANDLE_COLOR, size_px: 7.0 });
        }
        if !view.is_2d() {
            for poly in &self.side_faces {
                face_highlight(out, view, poly, SIDE_COLOR);
            }
        }
    }

    fn retained_version(&self, _state: &AppState, view: &ViewInfo) -> Option<u64> {
        let all_empty = !self.scope_all || self.all.lines.is_empty() && self.all.spheres.is_empty();
        if !self.enabled || all_empty && self.links.lines.is_empty() {
            return None;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.all_version.hash(&mut h);
        view.kind.hash(&mut h);
        Some(h.finish())
    }

    fn retained(&self, _state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if !self.enabled {
            return;
        }
        if self.scope_all {
            geometry_lines(view, &self.all, 0.85, &mut out.lines);
        } else {
            geometry_lines(view, &self.links, 0.85, &mut out.lines);
        }
    }

    fn paint(&self, _state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if !self.enabled {
            return;
        }
        for h in &self.selected.handles {
            if let Some(p) = view.world_to_screen(h.pos)
                && view.rect.contains(p)
            {
                painter.text(
                    p + egui::vec2(6.0, -6.0),
                    egui::Align2::LEFT_BOTTOM,
                    &h.key,
                    crate::paint::small_font(),
                    crate::paint::TEXT_WEAK,
                );
            }
        }
    }
}

#[derive(Default)]
struct ConnectionsLayer {
    mode: Option<ConnectionsMode>,
    selected: Vec<ConnectionLink>,
    selected_stamp: Option<Stamp>,
    all: Vec<ConnectionLink>,
    all_stamp: Option<Stamp>,
    all_version: u64,
}

impl OverlayLayer for ConnectionsLayer {
    fn name(&self) -> &'static str {
        "connections"
    }

    fn prepare(&mut self, state: &AppState) {
        let mode = state.settings.helpers.connections;
        if self.mode != Some(mode) {
            self.mode = Some(mode);
            self.selected_stamp = None;
            self.all_stamp = None;
        }
        if mode == ConnectionsMode::Off {
            self.selected.clear();
            self.all.clear();
            return;
        }
        let doc = &state.doc;
        let generation = state.assets.generation();
        let stamp = Stamp { revision: doc.revision(), selection: selection_hash(doc), generation };
        let needs_selected = mode == ConnectionsMode::Selected && self.selected_stamp != Some(stamp);
        let needs_all = mode == ConnectionsMode::All
            && self.all_stamp.map(|s| (s.revision, s.generation)) != Some((stamp.revision, generation));
        if !needs_selected && !needs_all {
            return;
        }
        let names = state.runtime.helpers.names(doc);
        let cx = HelperContext { doc, fgd: state.assets.fgd(), names: &names };
        if needs_selected {
            self.selected.clear();
            let ids = selected_entities(doc);
            for id in &ids {
                entity_outputs(&cx, *id, &mut self.selected);
            }
            if !ids.is_empty() {
                incoming_outputs(&cx, &ids, &mut self.selected);
            }
            self.selected_stamp = Some(stamp);
        }
        if needs_all {
            self.all.clear();
            let ids: Vec<Id> =
                visible_entities(doc).filter(|(_, e)| !e.connections.is_empty()).map(|(id, _)| id).collect();
            for id in ids {
                entity_outputs(&cx, id, &mut self.all);
            }
            self.all_stamp = Some(stamp);
            self.all_version = self.all_version.wrapping_add(1);
        }
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if state.settings.helpers.connections == ConnectionsMode::Selected {
            link_lines(view, &self.selected, Some(12.0), &mut out.lines_on_top);
        }
    }

    fn retained_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        if state.settings.helpers.connections != ConnectionsMode::All || self.all.is_empty() {
            return None;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.all_version.hash(&mut h);
        view.kind.hash(&mut h);
        Some(h.finish())
    }

    fn retained(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if state.settings.helpers.connections == ConnectionsMode::All {
            link_lines(view, &self.all, None, &mut out.lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2, vec2};
    use glam::DVec2;
    use hp_app::runtime::helpers::geometry::LinkState;
    use hp_app::view::{Camera2d, Camera3d, ViewKind};

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)),
            cam3d: Camera3d {
                pos: DVec3::new(-400.0, 0.0, 128.0),
                yaw_deg: 0.0,
                pitch_deg: 0.0,
                ..Camera3d::default()
            },
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 1.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn spheres_arrows_and_crosses() {
        let s = HelperSphere { center: DVec3::ZERO, radius: 64.0, color: [255, 0, 0] };
        let mut lines = Vec::new();
        sphere_lines(&view(ViewKind::Top), &s, 1.0, &mut lines);
        assert_eq!(lines.len(), 64);
        assert!(lines.iter().all(|l| l.a[2] == 0.0), "in the Top view plane");
        lines.clear();
        sphere_lines(&view(ViewKind::Perspective), &s, 1.0, &mut lines);
        assert_eq!(lines.len(), 144);
        let link = |to: Option<Id>, state| ConnectionLink {
            from: 1,
            to,
            a: DVec3::ZERO,
            b: DVec3::new(100.0, 0.0, 0.0),
            state,
        };
        lines.clear();
        link_lines(&view(ViewKind::Top), &[link(Some(2), LinkState::Valid)], None, &mut lines);
        assert_eq!(lines.len(), 3, "line and a 2D arrow head");
        lines.clear();
        link_lines(&view(ViewKind::Front), &[link(Some(2), LinkState::Valid)], None, &mut lines);
        assert_eq!(lines.len(), 1, "seen end on: no head");
        lines.clear();
        link_lines(&view(ViewKind::Perspective), &[link(Some(2), LinkState::UnknownOutput)], Some(12.0), &mut lines);
        assert_eq!(lines.len(), 5, "line and crossed fins");
        assert_eq!(lines[0].color, rgba(LinkState::UnknownOutput.color(), 1.0));
        lines.clear();
        link_lines(&view(ViewKind::Perspective), &[link(None, LinkState::MissingTarget)], Some(12.0), &mut lines);
        assert_eq!(lines.len(), 2, "a cross at the source");
    }

    #[test]
    fn selected_helpers_and_connections_of_the_test_map() {
        let mut state = hp_app::testing::test_state();
        let mut helpers = HelpersLayer::default();
        let mut links = ConnectionsLayer::default();
        state.doc.set_selection(&[hp_editor::ObjRef::Entity(5)]);
        links.prepare(&state);
        assert_eq!(links.selected.len(), 1, "the incoming output of the trigger");
        assert_eq!(links.selected[0].to, Some(5));
        let mut out = Overlay::default();
        links.add(&state, &view(ViewKind::Perspective), &mut out);
        assert!(!out.lines_on_top.is_empty());
        state.settings.helpers.connections = ConnectionsMode::All;
        links.prepare(&state);
        assert_eq!(links.all.len(), 1);
        let v = links.retained_version(&state, &view(ViewKind::Top));
        assert!(v.is_some());
        links.prepare(&state);
        assert_eq!(links.retained_version(&state, &view(ViewKind::Top)), v);
        assert_ne!(links.retained_version(&state, &view(ViewKind::Front)), v, "2D and 3D differ");
        state.doc.edit_entity(5, |e| e.set("targetname", "other")).expect("rename");
        links.prepare(&state);
        assert_ne!(links.retained_version(&state, &view(ViewKind::Top)), v);
        assert_eq!(links.all[0].state, LinkState::MissingTarget);
        state.doc.set_selection(&[hp_editor::ObjRef::Entity(4)]);
        helpers.prepare(&state);
        assert!(helpers.enabled);
        state.settings.view.show_helpers = false;
        helpers.prepare(&state);
        let mut out = Overlay::default();
        helpers.add(&state, &view(ViewKind::Top), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn path_chains_are_drawn_without_a_selection_like_hammer() {
        let mut state = hp_app::testing::test_state();
        let kv = |name: &str, next: &str| {
            vec![("targetname".to_owned(), name.to_owned()), ("target".to_owned(), next.to_owned())]
        };
        let a = state.doc.create_point_entity("path_track", DVec3::new(0.0, 0.0, 64.0), &kv("t1", "t2")).expect("t1");
        state.doc.create_point_entity("path_track", DVec3::new(256.0, 0.0, 64.0), &kv("t2", "t1")).expect("t2");
        state.doc.set_selection(&[]);
        let mut helpers = HelpersLayer::default();
        helpers.prepare(&state);
        assert_eq!(helpers.links.lines.len(), 2, "one line per link of the loop");
        assert!(helpers.selected.lines.is_empty());
        let v = helpers.retained_version(&state, &view(ViewKind::Top));
        assert!(v.is_some());
        let mut out = Overlay::default();
        helpers.retained(&state, &view(ViewKind::Top), &mut out);
        assert_eq!(out.lines.len(), 2);
        state.doc.edit_entity(a, |e| e.set("target", "nowhere")).expect("edit");
        helpers.prepare(&state);
        assert_eq!(helpers.links.lines.len(), 1);
        assert_ne!(helpers.retained_version(&state, &view(ViewKind::Top)), v);
        state.settings.helpers.helper_scope = HelperScope::All;
        helpers.prepare(&state);
        assert!(helpers.links.lines.is_empty(), "the every entity scope draws them already");
        state.settings.helpers.helper_scope = HelperScope::Selected;
        state.settings.view.show_helpers = false;
        helpers.prepare(&state);
        assert_eq!(helpers.retained_version(&state, &view(ViewKind::Top)), None, "helpers hidden");
    }
}
