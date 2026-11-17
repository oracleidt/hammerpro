use std::cell::RefCell;
use std::collections::HashMap;

use egui::{Align2, CursorIcon, Pos2, Ui};
use glam::{DVec2, DVec3};
use hp_app::state::AppState;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, t, tf};
use hp_editor::side_index::overlay::{OverlayShape, UV_KEYS, format_uv, is_overlay_class};
use hp_editor::side_index::{format_side_list, toggle_side};
use hp_editor::{FaceRef, Id, IdSet, ObjRef, SelectOp};
use hp_render::Overlay;

use crate::common::{
    COLOR_GHOST, COLOR_REMOVED, Mouse, Ov, Snap, SurfaceHit, active_material, clean_vec, create_entity, fgd_defaults,
    fmt_vec, fresh, material_chip, material_size, paint_label, pick_surface, rgba, set_kv,
};

const HANDLE_RADIUS: f32 = 8.0;
const MAX_EDITED: usize = 32;
const HANDLE: egui::Color32 = egui::Color32::from_rgb(255, 214, 64);
const MIN_FACING_2D: f64 = 0.2;
const ROTATE_OFFSET: f64 = 28.0;
const ROTATE_STEP: f64 = 15.0;

pub fn overlay_basis(normal: DVec3) -> (DVec3, DVec3) {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    let mut major = 0;
    let mut value = n.x;
    if n.y.abs() > value.abs() {
        major = 1;
        value = n.y;
    }
    if n.z.abs() > value.abs() {
        major = 2;
    }
    let u0 = if major == 0 { DVec3::Y } else { DVec3::X };
    let u = (u0 - n * u0.dot(n)).try_normalize().unwrap_or(u0);
    let v = n.cross(u).try_normalize().unwrap_or(DVec3::Y);
    (clean_unit(u), clean_unit(v))
}

fn clean_unit(v: DVec3) -> DVec3 {
    let c = |x: f64| {
        if x.abs() < 1e-9 {
            0.0
        } else if (x.abs() - 1.0).abs() < 1e-9 {
            x.signum()
        } else {
            x
        }
    };
    DVec3::new(c(v.x), c(v.y), c(v.z))
}

pub fn overlay_keyvalues(
    material: &str,
    side_id: Id,
    origin: DVec3,
    normal: DVec3,
    tex_size: DVec2,
) -> Vec<(String, String)> {
    let n = clean_unit(normal.try_normalize().unwrap_or(DVec3::Z));
    let (u, v) = overlay_basis(n);
    let (hw, hh) = (tex_size.x.abs() * 0.5, tex_size.y.abs() * 0.5);
    let uv = |x: f64, y: f64| fmt_vec(DVec3::new(x, y, 0.0));
    let kv = |k: &str, v: String| (k.to_owned(), v);
    vec![
        kv("material", material.to_owned()),
        kv("sides", side_id.to_string()),
        kv("StartU", "0".to_owned()),
        kv("EndU", "1".to_owned()),
        kv("StartV", "0".to_owned()),
        kv("EndV", "1".to_owned()),
        kv("BasisOrigin", fmt_vec(origin)),
        kv("BasisU", fmt_vec(u)),
        kv("BasisV", fmt_vec(v)),
        kv("BasisNormal", fmt_vec(n)),
        kv("uv0", uv(-hw, -hh)),
        kv("uv1", uv(-hw, hh)),
        kv("uv2", uv(hw, hh)),
        kv("uv3", uv(hw, -hh)),
    ]
}

fn is_overlay(state: &AppState, e: &hp_vmf::Entity) -> bool {
    is_overlay_class(&e.classname)
        || state.assets.fgd().and_then(|f| f.class(&e.classname)).is_some_and(|c| c.has_helper("overlay"))
}

fn point_world_size(view: &ViewInfo, p: DVec3) -> f64 {
    let h = f64::from(view.rect.height().max(1.0));
    let half = (view.cam3d.fov_y_deg.to_radians() * 0.5).tan().max(1e-3);
    let depth = (p - view.cam3d.pos).dot(view.cam3d.forward()).max(view.cam3d.near.max(0.01));
    2.0 * half * depth / h
}

fn closest_on_segment(p: DVec2, a: DVec2, b: DVec2) -> DVec2 {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 < 1e-12 {
        return a;
    }
    a + d * ((p - a).dot(d) / len2).clamp(0.0, 1.0)
}

fn inside_convex(p: DVec2, poly: &[DVec2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0f64;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let z = (b - a).perp_dot(p - a);
        if z.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = z.signum();
        } else if z.signum() != sign {
            return false;
        }
    }
    true
}

#[derive(Clone, Debug)]
struct Edited {
    id: Id,
    shape: OverlayShape,
    faces: Vec<FaceRef>,
}

#[derive(Default)]
struct EditedCache {
    stamp: Option<(u64, u64)>,
    overlays: Vec<Edited>,
    faces: HashMap<Id, FaceRef>,
}

fn selection_stamp(state: &AppState) -> (u64, u64) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for o in &state.doc.selection().objects {
        o.hash(&mut h);
    }
    (state.doc.revision(), h.finish())
}

impl EditedCache {
    fn refresh(&mut self, state: &AppState) {
        let stamp = selection_stamp(state);
        if self.stamp == Some(stamp) {
            return;
        }
        self.stamp = Some(stamp);
        self.overlays.clear();
        let doc = &state.doc;
        let mut wanted: Vec<(usize, Id)> = Vec::new();
        for id in doc.selected_entity_ids() {
            if self.overlays.len() >= MAX_EDITED {
                break;
            }
            let Some(node) = doc.scene().entity(id) else { continue };
            if !is_overlay(state, &node.entity) {
                continue;
            }
            let Some(shape) = OverlayShape::from_entity(&node.entity) else { continue };
            let index = self.overlays.len();
            wanted.extend(shape.sides.iter().map(|s| (index, *s)));
            self.overlays.push(Edited { id, shape, faces: Vec::new() });
        }
        let missing: IdSet = wanted
            .iter()
            .map(|(_, s)| *s)
            .filter(|s| !self.faces.get(s).is_some_and(|f| doc.face_has_side(*f, *s)))
            .collect();
        if !missing.is_empty() {
            for s in &missing {
                self.faces.remove(s);
            }
            self.faces.extend(doc.find_sides(&missing));
        }
        for (index, s) in wanted {
            if let (Some(face), Some(o)) = (self.faces.get(&s), self.overlays.get_mut(index)) {
                o.faces.push(*face);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grip {
    Corner(usize),
    Edge(usize),
    Rotate,
    Move,
}

#[derive(Clone, Debug)]
struct GripDrag {
    id: Id,
    grip: Grip,
    view: usize,
    shape: OverlayShape,
    origin: DVec3,
    faces: Vec<Vec<DVec2>>,
    others: Vec<DVec3>,
    press: Pos2,
    anchor: DVec2,
    pos: DVec3,
    stuck: bool,
    label: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OverlayPreview {
    view: usize,
    frame: u64,
    origin: DVec3,
    normal: DVec3,
    u: DVec3,
    v: DVec3,
    half: DVec2,
}

impl OverlayPreview {
    fn quad(&self) -> [DVec3; 4] {
        let o = self.origin + self.normal * 0.5;
        let (u, v) = (self.u * self.half.x, self.v * self.half.y);
        [o - u - v, o - u + v, o + u + v, o + u - v]
    }
}

fn world_per_px(view: &ViewInfo, p: DVec3) -> f64 {
    if view.is_2d() { view.units_per_point() } else { point_world_size(view, p) }
}

fn facing(view: &ViewInfo, normal: DVec3) -> f64 {
    if view.is_2d() { normal.dot(view.kind.toward_viewer()).abs() } else { 1.0 }
}

fn quad_center(uv: &[DVec2; 4]) -> DVec2 {
    (uv[0] + uv[1] + uv[2] + uv[3]) * 0.25
}

fn edge_mid(uv: &[DVec2; 4], i: usize) -> DVec2 {
    (uv[i] + uv[(i + 1) % 4]) * 0.5
}

fn extents(uv: &[DVec2; 4]) -> DVec2 {
    DVec2::new(edge_mid(uv, 2).distance(edge_mid(uv, 0)), edge_mid(uv, 1).distance(edge_mid(uv, 3)))
}

fn rotate_handle(shape: &OverlayShape, view: &ViewInfo) -> DVec3 {
    let c = quad_center(&shape.uv);
    let m = edge_mid(&shape.uv, 1);
    let dir = (m - c).try_normalize().unwrap_or(DVec2::Y);
    let off = world_per_px(view, shape.from_basis(m)) * ROTATE_OFFSET;
    shape.from_basis(m + dir * off)
}

fn plane_hit(shape: &OverlayShape, view: &ViewInfo, pos: Pos2) -> Option<DVec3> {
    let (o, d) = view.ray(pos);
    let denom = shape.normal.dot(d);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = shape.normal.dot(shape.origin - o) / denom;
    (t.is_finite() && (view.is_2d() || t > 0.0)).then(|| o + d * t)
}

fn snapped_screen(state: &AppState, view: &ViewInfo, m: &Mouse, pos: Pos2) -> Pos2 {
    if !view.is_2d() || m.modifiers.alt {
        return pos;
    }
    let snap = Snap::new(state, view, m.modifiers);
    if !snap.is_on() {
        return pos;
    }
    view.screen_of_2d(snap.v2(view.screen_to_2d(pos)))
}

fn inside_faces(faces: &[Vec<DVec2>], p: DVec2) -> bool {
    faces.is_empty() || faces.iter().any(|poly| inside_convex(p, poly))
}

fn clamp_scale(faces: &[Vec<DVec2>], full: f64, points: impl Fn(f64) -> Vec<DVec2>) -> (f64, bool) {
    let ok = |k: f64| points(k).iter().all(|p| inside_faces(faces, *p));
    if ok(full) || !ok(0.0) {
        return (full, false);
    }
    let (mut lo, mut hi) = (0.0, full);
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo, true)
}

fn texture_range(shape: &OverlayShape, grip: Grip, uv: &[DVec2; 4]) -> (DVec2, DVec2) {
    let (old, new) = (extents(&shape.uv), extents(uv));
    let ratio = |a: f64, b: f64| if a > 1e-6 && b.is_finite() { b / a } else { 1.0 };
    let r = DVec2::new(ratio(old.x, new.x), ratio(old.y, new.y));
    let (mut start, mut end) = (shape.start, shape.end);
    let d = shape.end - shape.start;
    let (u_end, v_end) = match grip {
        Grip::Corner(i) => (Some(i >= 2), Some(i == 1 || i == 2)),
        Grip::Edge(0) => (Some(false), None),
        Grip::Edge(1) => (None, Some(true)),
        Grip::Edge(2) => (Some(true), None),
        Grip::Edge(_) => (None, Some(false)),
        _ => (None, None),
    };
    match u_end {
        Some(true) => end.x = shape.start.x + d.x * r.x,
        Some(false) => start.x = shape.end.x - d.x * r.x,
        None => {}
    }
    match v_end {
        Some(true) => end.y = shape.start.y + d.y * r.y,
        Some(false) => start.y = shape.end.y - d.y * r.y,
        None => {}
    }
    (start, end)
}

fn fmt_tex(x: f64) -> String {
    let s = format_uv(DVec2::new(x, 0.0));
    s.split(' ').next().unwrap_or("0").to_owned()
}

fn screen_quad(view: &ViewInfo, shape: &OverlayShape) -> Option<[Pos2; 4]> {
    let c = shape.corners();
    Some([
        view.world_to_screen(c[0])?,
        view.world_to_screen(c[1])?,
        view.world_to_screen(c[2])?,
        view.world_to_screen(c[3])?,
    ])
}

fn screen_hit(view: &ViewInfo, shape: &OverlayShape, pos: Pos2) -> Option<f64> {
    let q = screen_quad(view, shape)?;
    let pts: Vec<DVec2> = q.iter().map(|p| DVec2::new(f64::from(p.x), f64::from(p.y))).collect();
    let p = DVec2::new(f64::from(pos.x), f64::from(pos.y));
    let mut area = 0.0;
    for i in 0..4 {
        area += pts[i].perp_dot(pts[(i + 1) % 4]);
    }
    let area = area.abs() * 0.5;
    if area > 4.0 && inside_convex(p, &pts) {
        return Some(area);
    }
    let near =
        (0..4).map(|i| closest_on_segment(p, pts[i], pts[(i + 1) % 4]).distance(p)).fold(f64::INFINITY, f64::min);
    (near <= f64::from(HANDLE_RADIUS) * 0.6).then_some(area.max(1.0))
}

pub struct OverlayTool {
    pub snap_to_grid: bool,
    preview: Option<OverlayPreview>,
    drag: Option<GripDrag>,
    hover: Option<(Id, Grip, usize, u64)>,
    cache: RefCell<EditedCache>,
}

impl Default for OverlayTool {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayTool {
    pub fn new() -> Self {
        Self {
            snap_to_grid: false,
            preview: None,
            drag: None,
            hover: None,
            cache: RefCell::new(EditedCache::default()),
        }
    }

    fn center(&self, state: &AppState, view: &ViewInfo, hit: &SurfaceHit, m: &Mouse) -> DVec3 {
        let surface = Some(ObjRef::Solid(hit.solid));
        let snap = if self.snap_to_grid { Snap::on_object(state, view, m.modifiers, surface) } else { Snap::OFF };
        let p = hit.face_plane.project(hit.point);
        clean_vec(snap.on_plane(p, hit.face_plane.normal))
    }

    pub fn place(
        &mut self,
        state: &mut AppState,
        side_id: Id,
        face_normal: DVec3,
        center: DVec3,
        keep_selection: bool,
    ) -> Option<Id> {
        let material = active_material(state);
        let size = material_size(state, &material);
        let mut kv = fgd_defaults(state, "info_overlay");
        for (k, v) in overlay_keyvalues(&material, side_id, center, face_normal, size) {
            set_kv(&mut kv, &k, v);
        }
        match create_entity(state, "info_overlay", center, &kv, keep_selection) {
            Ok(id) => Some(id),
            Err(e) => {
                state.notify(Severity::Error, tf!("create.overlay.failed", e));
                None
            }
        }
    }

    fn edited(&self, state: &AppState) -> Vec<Edited> {
        let mut cache = self.cache.borrow_mut();
        cache.refresh(state);
        cache.overlays.clone()
    }

    fn grip_at(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<(Id, Grip)> {
        let mut cache = self.cache.borrow_mut();
        cache.refresh(state);
        let mut best: Option<(u8, f32, Id, Grip)> = None;
        let mut offer = |rank: u8, d: f32, id: Id, g: Grip| {
            if best.is_none_or(|b| (rank, d) < (b.0, b.1)) {
                best = Some((rank, d, id, g));
            }
        };
        for o in &cache.overlays {
            let s = &o.shape;
            if facing(view, s.normal) >= MIN_FACING_2D {
                for (i, c) in s.corners().iter().enumerate() {
                    if let Some(p) = view.world_to_screen(*c).map(|p| p.distance(pos)).filter(|d| *d <= HANDLE_RADIUS) {
                        offer(0, p, o.id, Grip::Corner(i));
                    }
                }
                for i in 0..4 {
                    let m = s.from_basis(edge_mid(&s.uv, i));
                    if let Some(d) = view.world_to_screen(m).map(|p| p.distance(pos)).filter(|d| *d <= HANDLE_RADIUS) {
                        offer(1, d, o.id, Grip::Edge(i));
                    }
                }
                if let Some(d) = view
                    .world_to_screen(rotate_handle(s, view))
                    .map(|p| p.distance(pos))
                    .filter(|d| *d <= HANDLE_RADIUS)
                {
                    offer(2, d, o.id, Grip::Rotate);
                }
            }
            if let Some(area) = screen_hit(view, s, pos) {
                offer(3, area as f32, o.id, Grip::Move);
            }
        }
        best.map(|b| (b.2, b.3))
    }

    fn overlay_at(state: &AppState, hit: &SurfaceHit) -> Option<Id> {
        let scene = state.doc.scene();
        let mut found = None;
        for (id, node) in scene.entities() {
            if !is_overlay(state, &node.entity) || !state.doc.is_entity_visible(*id) {
                continue;
            }
            let Some(shape) = OverlayShape::from_entity(&node.entity) else { continue };
            if shape.sides.contains(&hit.side_id) && shape.contains(shape.to_basis(hit.point)) {
                found = Some(*id);
            }
        }
        found
    }

    fn overlay_at_2d(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<Id> {
        let scene = state.doc.scene();
        let mut best: Option<(f64, Id)> = None;
        for (id, node) in scene.entities() {
            if !is_overlay(state, &node.entity) || !state.doc.is_entity_visible(*id) {
                continue;
            }
            let Some(shape) = OverlayShape::from_entity(&node.entity) else { continue };
            if let Some(area) = screen_hit(view, &shape, pos)
                && best.is_none_or(|b| area < b.0)
            {
                best = Some((area, *id));
            }
        }
        best.map(|b| b.1)
    }

    fn begin_drag(&mut self, state: &mut AppState, view: &ViewInfo, id: Id, grip: Grip, press: Pos2) {
        let Some(edited) = self.edited(state).into_iter().find(|o| o.id == id) else { return };
        let shape = edited.shape.clone();
        let origin = state.doc.scene().entity(id).and_then(|n| n.entity.origin()).unwrap_or(shape.origin);
        let faces: Vec<Vec<DVec2>> = edited
            .faces
            .iter()
            .filter_map(|f| state.doc.face_polygon(*f))
            .map(|poly| poly.iter().map(|p| shape.to_basis(*p)).collect())
            .collect();
        let others: Vec<DVec3> = if matches!(grip, Grip::Corner(_) | Grip::Edge(_)) {
            state
                .doc
                .scene()
                .entities()
                .iter()
                .filter(|(oid, n)| **oid != id && is_overlay(state, &n.entity) && state.doc.is_entity_visible(**oid))
                .filter_map(|(_, n)| OverlayShape::from_entity(&n.entity))
                .flat_map(|s| s.corners())
                .collect()
        } else {
            Vec::new()
        };
        let anchor = plane_hit(&shape, view, press).map_or_else(|| quad_center(&shape.uv), |p| shape.to_basis(p));
        let pos = match grip {
            Grip::Corner(i) => shape.corners()[i],
            Grip::Edge(i) => shape.from_basis(edge_mid(&shape.uv, i)),
            Grip::Rotate => rotate_handle(&shape, view),
            Grip::Move => shape.from_basis(anchor),
        };
        state.doc.begin("Edit overlay");
        self.drag = Some(GripDrag {
            id,
            grip,
            view: view.index,
            shape,
            origin,
            faces,
            others,
            press,
            anchor,
            pos,
            stuck: false,
            label: String::new(),
        });
    }

    fn snap_point(drag: &GripDrag, state: &AppState, view: &ViewInfo, m: &Mouse, pos: Pos2) -> Option<(DVec3, bool)> {
        let s = &drag.shape;
        let on_plane = |q: DVec3| q - s.normal * s.normal.dot(q - s.origin);
        if m.modifiers.shift {
            let best = drag
                .others
                .iter()
                .filter_map(|c| view.world_to_screen(*c).map(|sc| (sc.distance(pos), *c)))
                .filter(|(dist, _)| *dist <= HANDLE_RADIUS)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, c)) = best {
                return Some((on_plane(c), true));
            }
        }
        let mut p = plane_hit(s, view, snapped_screen(state, view, m, pos))?;
        if !m.modifiers.alt {
            if !view.is_2d() {
                p = Snap::new(state, view, m.modifiers).on_plane(p, s.normal);
            }
            let tol = world_per_px(view, p) * f64::from(HANDLE_RADIUS);
            let q = s.to_basis(p);
            let corner = drag.faces.iter().flatten().map(|c| (c.distance(q), *c)).min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, c)) = corner.filter(|(dist, _)| *dist <= tol) {
                p = s.from_basis(c);
            } else {
                let edge = drag
                    .faces
                    .iter()
                    .flat_map(|poly| (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()])))
                    .map(|(a, b)| {
                        let c = closest_on_segment(q, a, b);
                        (c.distance(q), c)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, c)) = edge.filter(|(dist, _)| *dist <= tol) {
                    p = s.from_basis(c);
                }
            }
        }
        Some((clean_vec(on_plane(p)), false))
    }

    fn corner_position(drag: &GripDrag, state: &AppState, view: &ViewInfo, m: &Mouse) -> Option<(DVec3, bool)> {
        let (mut p, stuck) = Self::snap_point(drag, state, view, m, m.pos?)?;
        if stuck {
            return Some((p, true));
        }
        let s = &drag.shape;
        let q = s.to_basis(p);
        if !inside_faces(&drag.faces, q) {
            let edge = drag
                .faces
                .iter()
                .flat_map(|poly| (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()])))
                .map(|(a, b)| closest_on_segment(q, a, b))
                .min_by(|a, b| a.distance(q).total_cmp(&b.distance(q)));
            if let Some(c) = edge {
                p = s.from_basis(c);
            }
        }
        Some((clean_vec(p - s.normal * s.normal.dot(p - s.origin)), false))
    }

    fn move_delta(drag: &GripDrag, state: &AppState, view: &ViewInfo, m: &Mouse, pos: Pos2) -> Option<DVec3> {
        let s = &drag.shape;
        let reference = s.from_basis(quad_center(&s.uv));
        if view.is_2d() {
            let raw = view.screen_to_2d(pos) - view.screen_to_2d(drag.press);
            let snap = if m.modifiers.alt { Snap::OFF } else { Snap::new(state, view, m.modifiers) };
            let d2 = snap.delta2(view.kind.to_2d(reference), raw);
            let w = view.kind.from_2d(d2, 0.0) - view.kind.from_2d(DVec2::ZERO, 0.0);
            let axis = view.kind.toward_viewer();
            let na = s.normal.dot(axis);
            let d = if na.abs() >= MIN_FACING_2D {
                w - axis * (s.normal.dot(w) / na)
            } else {
                w - s.normal * s.normal.dot(w)
            };
            return d.is_finite().then_some(d);
        }
        let p = plane_hit(s, view, pos)?;
        let target = reference + s.from_basis(s.to_basis(p)) - s.from_basis(drag.anchor);
        let snapped =
            if m.modifiers.alt { target } else { Snap::new(state, view, m.modifiers).on_plane(target, s.normal) };
        let d = snapped - reference;
        let d = d - s.normal * s.normal.dot(d);
        d.is_finite().then_some(d)
    }

    fn update_drag(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) {
        let Some(drag) = self.drag.as_ref() else { return };
        if view.index != drag.view {
            return;
        }
        let Some(pos) = m.pos else { return };
        let s = &drag.shape;
        let ctrl = m.modifiers.ctrl || m.modifiers.command;
        let mut changes: Vec<(&'static str, String)> = Vec::new();
        let mut refit = false;
        let (label_pos, stuck, label) = match drag.grip {
            Grip::Corner(i) => {
                let Some((p, stuck)) = Self::corner_position(drag, state, view, m) else { return };
                let mut uv = s.uv;
                uv[i] = s.to_basis(p);
                changes.push((UV_KEYS[i], format_uv(uv[i])));
                if ctrl {
                    let (a, b) = texture_range(s, drag.grip, &uv);
                    changes.extend([
                        ("StartU", fmt_tex(a.x)),
                        ("EndU", fmt_tex(b.x)),
                        ("StartV", fmt_tex(a.y)),
                        ("EndV", fmt_tex(b.y)),
                    ]);
                }
                (p, stuck, format!("{}  {}", UV_KEYS[i], format_uv(uv[i])))
            }
            Grip::Edge(i) => {
                let Some((p, _)) = Self::snap_point(drag, state, view, m, pos) else { return };
                let (a, b) = (s.uv[i], s.uv[(i + 1) % 4]);
                let mid = (a + b) * 0.5;
                let Some(mut n) = (b - a).perp().try_normalize() else { return };
                if n.dot(mid - quad_center(&s.uv)) < 0.0 {
                    n = -n;
                }
                let full = (s.to_basis(p) - mid).dot(n);
                let shifted = |k: f64| vec![a + n * k, b + n * k];
                let (k, stuck) = if m.modifiers.alt { (full, false) } else { clamp_scale(&drag.faces, full, shifted) };
                let mut uv = s.uv;
                uv[i] = a + n * k;
                uv[(i + 1) % 4] = b + n * k;
                changes.push((UV_KEYS[i], format_uv(uv[i])));
                changes.push((UV_KEYS[(i + 1) % 4], format_uv(uv[(i + 1) % 4])));
                if ctrl {
                    let (a, b) = texture_range(s, drag.grip, &uv);
                    changes.extend([
                        ("StartU", fmt_tex(a.x)),
                        ("EndU", fmt_tex(b.x)),
                        ("StartV", fmt_tex(a.y)),
                        ("EndV", fmt_tex(b.y)),
                    ]);
                }
                let e = extents(&uv);
                (s.from_basis(edge_mid(&uv, i)), stuck, format!("{} x {}", fmt_tex(e.x), fmt_tex(e.y)))
            }
            Grip::Rotate => {
                let c = quad_center(&s.uv);
                let Some(p) = plane_hit(s, view, pos) else { return };
                let start = (drag.anchor - c).to_angle();
                let mut deg = ((s.to_basis(p) - c).to_angle() - start).to_degrees();
                let snapped = state.snap_enabled() && !m.modifiers.alt;
                if snapped {
                    deg = (deg / ROTATE_STEP).round() * ROTATE_STEP;
                }
                let deg = (deg + 540.0).rem_euclid(360.0) - 180.0;
                let rot = DVec2::from_angle(deg.to_radians());
                let uv = s.uv.map(|q| c + rot.rotate(q - c));
                for (k, q) in UV_KEYS.iter().zip(uv) {
                    changes.push((k, format_uv(q)));
                }
                changes.push(("sides", format_side_list(&s.sides)));
                refit = true;
                (s.from_basis(c), false, format!("{}\u{b0}", fmt_tex(deg)))
            }
            Grip::Move => {
                let Some(d) = Self::move_delta(drag, state, view, m, pos) else { return };
                let basis = clean_vec(s.origin + d);
                changes.push(("BasisOrigin", fmt_vec(basis)));
                changes.push(("origin", fmt_vec(clean_vec(drag.origin + d))));
                changes.push(("sides", format_side_list(&s.sides)));
                refit = true;
                (s.from_basis(drag.anchor) + d, false, fmt_vec(basis))
            }
        };
        let id = drag.id;
        if let Some(d) = self.drag.as_mut() {
            d.pos = label_pos;
            d.stuck = stuck;
            d.label = label;
        }
        if let Err(e) = state.doc.edit_entity(id, |e| {
            for (k, v) in &changes {
                e.set(k, v.as_str());
            }
        }) {
            state.notify(Severity::Error, tf!("create.overlay.failed", e));
            return;
        }
        if refit && let Err(e) = state.doc.refit_overlay_faces(&[id]) {
            state.notify(Severity::Error, tf!("create.overlay.failed", e));
        }
    }

    fn end_drag(&mut self, state: &mut AppState) {
        if self.drag.take().is_some() {
            state.doc.commit();
        }
    }

    fn toggle_face(&mut self, state: &mut AppState, hit: &SurfaceHit) {
        let edited = self.edited(state);
        if edited.is_empty() {
            return;
        }
        let mut kept_last = false;
        let changes: Vec<(Id, String)> = edited
            .iter()
            .filter_map(|o| {
                let sides = toggle_side(&o.shape.sides, hit.side_id);
                if sides.is_empty() {
                    kept_last = true;
                    return None;
                }
                Some((o.id, format_side_list(&sides)))
            })
            .collect();
        if kept_last {
            state.toasts.push(Severity::Info, t!("create.overlay.last_face"));
        }
        if changes.is_empty() {
            return;
        }
        state.doc.begin("Edit overlay faces");
        for (id, sides) in changes {
            if let Err(e) = state.doc.edit_entity(id, |e| e.set("sides", sides.as_str())) {
                state.notify(Severity::Error, tf!("create.overlay.failed", e));
            }
        }
        state.doc.commit();
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        if self.drag.is_some() {
            if m.dragging || m.drag_started {
                self.update_drag(state, view, m);
            }
            if m.drag_stopped || !m.any_button() {
                self.update_drag(state, view, m);
                self.end_drag(state);
            }
            return ToolFlow::Handled;
        }
        let press = m.origin().unwrap_or(pos);
        let grip = self.grip_at(state, view, press);
        if m.drag_started
            && let Some((id, g)) = grip
        {
            self.preview = None;
            self.begin_drag(state, view, id, g, press);
            self.update_drag(state, view, m);
            return ToolFlow::Handled;
        }
        if view.is_2d() {
            if m.hovering() {
                self.hover = grip.map(|(id, g)| (id, g, view.index, state.frame.index));
                return ToolFlow::Ignored;
            }
            if m.clicked {
                if let Some(id) = Self::overlay_at_2d(state, view, pos) {
                    let op = if m.modifiers.shift { SelectOp::Toggle } else { SelectOp::Replace };
                    state.doc.select(&[ObjRef::Entity(id)], op);
                } else {
                    state.toasts.push(Severity::Info, t!("create.need_3d"));
                }
                return ToolFlow::Handled;
            }
            return if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored };
        }
        let handle = grip.filter(|(_, g)| *g != Grip::Move);
        if m.hovering() {
            self.hover = grip.map(|(id, g)| (id, g, view.index, state.frame.index));
            if handle.is_some() {
                self.preview = None;
                return ToolFlow::Ignored;
            }
            let hit = pick_surface(state, view, pos);
            self.preview = match hit {
                Some(h) if grip.is_none() => {
                    let material = active_material(state);
                    let size = material_size(state, &material);
                    let (u, v) = overlay_basis(h.face_normal);
                    Some(OverlayPreview {
                        view: view.index,
                        frame: state.frame.index,
                        origin: self.center(state, view, &h, m),
                        normal: h.face_normal,
                        u,
                        v,
                        half: size * 0.5,
                    })
                }
                _ => None,
            };
            return ToolFlow::Ignored;
        }
        if m.clicked
            && handle.is_none()
            && let Some(h) = pick_surface(state, view, pos)
        {
            let ctrl = m.modifiers.ctrl || m.modifiers.command;
            if ctrl && !self.edited(state).is_empty() {
                self.toggle_face(state, &h);
            } else if let Some(id) = Self::overlay_at(state, &h) {
                let op = if m.modifiers.shift { SelectOp::Toggle } else { SelectOp::Replace };
                state.doc.select(&[ObjRef::Entity(id)], op);
            } else {
                let c = self.center(state, view, &h, m);
                self.place(state, h.side_id, h.face_normal, c, m.modifiers.shift);
            }
        }
        if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored }
    }

    fn grip_color(&self, state: &AppState, id: Id, g: Grip) -> ([f32; 4], bool) {
        let dragged = self.drag.as_ref().is_some_and(|d| d.id == id && d.grip == g);
        let hovered = self.hover.is_some_and(|h| h.0 == id && h.1 == g && fresh(state, h.3));
        let color = match (dragged, self.drag.as_ref().is_some_and(|d| d.stuck)) {
            (true, true) => rgba(COLOR_REMOVED, 1.0),
            (true, false) => rgba(egui::Color32::WHITE, 1.0),
            _ if hovered => rgba(egui::Color32::WHITE, 1.0),
            _ => rgba(HANDLE, 1.0),
        };
        (color, dragged || hovered)
    }
}

impl Tool for OverlayTool {
    fn id(&self) -> ToolId {
        ToolId::Overlay
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.preview = None;
        self.hover = None;
        self.end_drag(state);
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            self.preview = None;
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let mut ov = Ov::new(out, view);
        let edited = self.edited(state);
        for (n, o) in edited.iter().enumerate() {
            let s = &o.shape;
            if n < 8 && !view.is_2d() {
                for f in &o.faces {
                    if let Some(poly) = state.doc.face_polygon(*f) {
                        let lifted: Vec<DVec3> = poly.iter().map(|p| *p + s.normal * 0.3).collect();
                        ov.polygon_fill(&lifted, rgba(COLOR_GHOST, 0.10));
                    }
                }
            }
            let corners = s.corners();
            ov.polygon_outline(&corners, rgba(HANDLE, 0.9), true);
            if facing(view, s.normal) < MIN_FACING_2D {
                continue;
            }
            for (i, c) in corners.iter().enumerate() {
                let (color, big) = self.grip_color(state, o.id, Grip::Corner(i));
                ov.point(*c, color, if big { 10.0 } else { 8.0 });
            }
            for i in 0..4 {
                let (color, big) = self.grip_color(state, o.id, Grip::Edge(i));
                ov.point(s.from_basis(edge_mid(&s.uv, i)), color, if big { 8.0 } else { 6.0 });
            }
            let r = rotate_handle(s, view);
            let (color, big) = self.grip_color(state, o.id, Grip::Rotate);
            ov.line(s.from_basis(edge_mid(&s.uv, 1)), r, rgba(HANDLE, 0.6));
            ov.point(r, color, if big { 11.0 } else { 9.0 });
        }
        if view.is_2d() {
            return;
        }
        let Some(p) = self.preview.filter(|p| fresh(state, p.frame)) else { return };
        if self.drag.is_some() {
            return;
        }
        let quad = p.quad();
        ov.polygon_fill(&quad, rgba(COLOR_GHOST, 0.22));
        ov.polygon_outline(&quad, rgba(COLOR_GHOST, 1.0), true);
        let o = p.origin + p.normal * 0.5;
        let len = p.half.x.min(p.half.y).clamp(8.0, 64.0);
        ov.line(o, o + p.u * len, rgba(hp_app::theme::AXIS_X, 1.0));
        ov.line(o, o + p.v * len, rgba(hp_app::theme::AXIS_Y, 1.0));
        ov.point(p.origin, rgba(COLOR_GHOST, 1.0), 5.0);
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if let Some(d) = &self.drag
            && d.view == view.index
            && let Some(s) = view.world_to_screen(d.pos)
        {
            let color = if d.stuck { COLOR_REMOVED } else { HANDLE };
            if !d.label.is_empty() {
                paint_label(painter, s + egui::vec2(12.0, -12.0), Align2::LEFT_BOTTOM, &d.label, color);
            }
            return;
        }
        let Some(p) = self.preview.filter(|p| fresh(state, p.frame) && p.view == view.index) else { return };
        if let Some(s) = view.world_to_screen(p.origin)
            && view.rect.contains(s)
        {
            let text = format!("{}  {} x {}", active_material(state), p.half.x * 2.0, p.half.y * 2.0);
            paint_label(painter, s + egui::vec2(12.0, -12.0), Align2::LEFT_BOTTOM, &text, COLOR_GHOST);
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            material_chip(ui, state);
            let material = active_material(state);
            let size = material_size(state, &material);
            ui.label(tf!("create.decal.size", size.x, size.y)).on_hover_text(t!("create.overlay.size_tip"));
            ui.separator();
            ui.checkbox(&mut self.snap_to_grid, t!("action.toggle_snap"));
        });
    }

    fn cursor(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if self.drag.is_some() {
            return Some(CursorIcon::Grabbing);
        }
        match self.grip_at(state, view, pos) {
            Some((_, Grip::Move)) => return Some(CursorIcon::Move),
            Some((_, Grip::Rotate)) => return Some(CursorIcon::Alias),
            Some(_) => return Some(CursorIcon::Grab),
            None => {}
        }
        if view.is_2d() {
            if Self::overlay_at_2d(state, view, pos).is_some() {
                return Some(CursorIcon::PointingHand);
            }
            return Some(CursorIcon::NotAllowed);
        }
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        Some(t!("create.overlay.hint").to_owned())
    }

    fn set_option(&mut self, _state: &mut AppState, key: &str, value: &str) -> bool {
        match key {
            "snap" => crate::common::option_flag(value).map(|b| self.snap_to_grid = b).is_some(),
            _ => false,
        }
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        vec![hp_app::tool::ToolOption::new(
            "snap",
            crate::common::on_off(self.snap_to_grid),
            "on: the overlay snaps to the grid of the face, off: it follows the cursor freely",
        )]
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        self.preview = None;
        if self.drag.take().is_some() {
            state.doc.cancel();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{fgd_state, info_3d_looking_down};
    use egui::Modifiers;

    #[test]
    fn hammer_basis() {
        assert_eq!(overlay_basis(DVec3::Z), (DVec3::X, DVec3::Y));
        assert_eq!(overlay_basis(DVec3::X), (DVec3::Y, DVec3::Z));
        assert_eq!(overlay_basis(DVec3::NEG_Y), (DVec3::X, DVec3::Z));
        let n = DVec3::new(0.3, -0.4, 0.8).normalize();
        let (u, v) = overlay_basis(n);
        assert!(u.dot(n).abs() < 1e-9 && v.dot(n).abs() < 1e-9 && u.dot(v).abs() < 1e-9);
        assert!((u.cross(v) - n).length() < 1e-9);
    }

    #[test]
    fn keyvalue_format() {
        let kv =
            overlay_keyvalues("overlays/test", 42, DVec3::new(10.0, -20.5, 0.0), DVec3::Z, DVec2::new(128.0, 64.0));
        let get = |k: &str| kv.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
        assert_eq!(get("material"), Some("overlays/test"));
        assert_eq!(get("sides"), Some("42"));
        assert_eq!(get("BasisOrigin"), Some("10 -20.5 0"));
        assert_eq!(get("BasisNormal"), Some("0 0 1"));
        assert_eq!(get("BasisU"), Some("1 0 0"));
        assert_eq!(get("BasisV"), Some("0 1 0"));
        assert_eq!(get("uv0"), Some("-64 -32 0"));
        assert_eq!(get("uv1"), Some("-64 32 0"));
        assert_eq!(get("uv2"), Some("64 32 0"));
        assert_eq!(get("uv3"), Some("64 -32 0"));
        assert_eq!(
            (get("StartU"), get("EndU"), get("StartV"), get("EndV")),
            (Some("0"), Some("1"), Some("0"), Some("1"))
        );
    }

    #[test]
    fn places_overlay_on_the_floor() {
        let mut state = fgd_state();
        state.current_material = "overlays/street_crack01".to_owned();
        let mut tool = OverlayTool::new();
        let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 512.0));
        let p = view.world_to_screen(DVec3::new(-40.0, 24.0, 0.0)).expect("screen");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(p));
        assert!(tool.preview.is_some());
        tool.handle_mouse(&mut state, &view, &Mouse::click(p));
        let sel = state.doc.selected_entity_ids();
        let e = &state.doc.scene().entity(sel[0]).expect("overlay").entity;
        assert_eq!(e.classname, "info_overlay");
        assert_eq!(e.get("material"), Some("overlays/street_crack01"));
        assert_eq!(e.get("sides"), Some("1"));
        assert_eq!(e.get("BasisNormal"), Some("0 0 1"));
        assert_eq!(e.get("uv2"), Some("256 256 0"));
        assert_eq!(e.get("fademindist"), Some("-1"), "FGD default");
        let o = e.origin().expect("origin");
        assert_eq!(o.z, 0.0, "on the face plane");
        let basis = hp_vmf::parse_vec3(e.get("BasisOrigin").expect("basis")).expect("vec");
        assert!((basis - o).length() < 1e-3);
    }

    fn small_overlay(state: &mut AppState, x: f64) -> Id {
        let mut kv = overlay_keyvalues("overlays/test", 1, DVec3::new(x, 0.0, 0.0), DVec3::Z, DVec2::splat(64.0));
        set_kv(&mut kv, "origin", fmt_vec(DVec3::new(x, 0.0, 0.0)));
        state.doc.create_point_entity("info_overlay", DVec3::new(x, 0.0, 0.0), &kv).expect("overlay")
    }

    fn uv(state: &AppState, id: Id, key: &str) -> DVec2 {
        let e = &state.doc.scene().entity(id).expect("overlay").entity;
        hp_vmf::parse_vec3(e.get(key).expect("uv")).expect("vec").truncate()
    }

    #[test]
    fn drags_a_corner_with_grid_and_face_clamping() {
        let mut state = crate::tests_support::settle();
        let id = small_overlay(&mut state, 0.0);
        let mut tool = OverlayTool::new();
        let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 700.0));
        let corner = view.world_to_screen(DVec3::new(32.0, 32.0, 0.0)).expect("corner 2");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(corner));
        assert_eq!(tool.hover.map(|h| (h.0, h.1)), Some((id, Grip::Corner(2))));
        let undo_before = state.doc.history_len().0;
        let to = view.world_to_screen(DVec3::new(101.0, 77.0, 0.0)).expect("target");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(corner, to));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(corner, to));
        assert_eq!(uv(&state, id, "uv2"), DVec2::new(96.0, 80.0));
        let far = view.world_to_screen(DVec3::new(300.0, 80.0, 0.0)).expect("far");
        tool.handle_mouse(&mut state, &view, &Mouse::drag(corner, far));
        assert_eq!(uv(&state, id, "uv2"), DVec2::new(256.0, 80.0));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(corner, far));
        assert!(tool.drag.is_none());
        assert_eq!(state.doc.history_len().0, undo_before + 1, "one undo step");
        assert_eq!(state.doc.undo_label(), Some("Edit overlay"));
        assert!(state.doc.undo());
        assert_eq!(uv(&state, id, "uv2"), DVec2::new(32.0, 32.0));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(corner, to));
        assert!(tool.cancel(&mut state));
        assert_eq!(uv(&state, id, "uv2"), DVec2::new(32.0, 32.0));
    }

    #[test]
    fn shift_sticks_to_another_overlay_and_alt_is_free() {
        let mut state = crate::tests_support::settle();
        let other = small_overlay(&mut state, 100.0);
        let id = small_overlay(&mut state, 0.0);
        assert_ne!(other, id);
        let mut tool = OverlayTool::new();
        let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 700.0));
        let corner = view.world_to_screen(DVec3::new(32.0, 32.0, 0.0)).expect("corner");
        let near = view.world_to_screen(DVec3::new(70.0, 33.0, 0.0)).expect("near");
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(corner, near).with_modifiers(shift));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(corner, near).with_modifiers(shift));
        assert!(tool.drag.as_ref().is_some_and(|d| d.stuck));
        assert_eq!(uv(&state, id, "uv2"), DVec2::new(68.0, 32.0));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(corner, near).with_modifiers(shift));
        let corner = view.world_to_screen(DVec3::new(68.0, 32.0, 0.0)).expect("corner");
        let free = view.world_to_screen(DVec3::new(41.3, 22.7, 0.0)).expect("free");
        let alt = Modifiers { alt: true, ..Modifiers::NONE };
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(corner, free).with_modifiers(alt));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(corner, free).with_modifiers(alt));
        let p = uv(&state, id, "uv2");
        assert!((p - DVec2::new(41.3, 22.7)).length() < 0.6, "{p}");
        assert!(p.x.fract() != 0.0 || p.y.fract() != 0.0, "not snapped: {p}");
    }

    #[test]
    fn ctrl_click_toggles_faces_and_click_selects() {
        let mut state = crate::tests_support::settle();
        let id = small_overlay(&mut state, 0.0);
        let mut tool = OverlayTool::new();
        let view = info_3d_looking_down(DVec3::new(0.0, 100.0, 900.0));
        let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
        let wall = view.world_to_screen(DVec3::new(-150.0, 264.0, 128.0)).expect("wall");
        tool.handle_mouse(&mut state, &view, &Mouse::click(wall).with_modifiers(ctrl));
        let sides = |s: &AppState| s.doc.scene().entity(id).and_then(|n| n.entity.get("sides").map(str::to_owned));
        assert_eq!(sides(&state).as_deref(), Some("1 7"));
        assert_eq!(state.doc.undo_label(), Some("Edit overlay faces"));
        tool.handle_mouse(&mut state, &view, &Mouse::click(wall).with_modifiers(ctrl));
        assert_eq!(sides(&state).as_deref(), Some("1"));
        let floor = view.world_to_screen(DVec3::new(-150.0, -150.0, 0.0)).expect("floor");
        tool.handle_mouse(&mut state, &view, &Mouse::click(floor).with_modifiers(ctrl));
        assert_eq!(sides(&state).as_deref(), Some("1"));
        state.doc.set_selection(&[]);
        let on = view.world_to_screen(DVec3::new(10.0, 10.0, 0.0)).expect("on");
        tool.handle_mouse(&mut state, &view, &Mouse::click(on));
        assert_eq!(state.doc.selected_entity_ids(), vec![id]);
        let count = state.doc.scene().entities().len();
        tool.handle_mouse(&mut state, &view, &Mouse::click(floor));
        assert_eq!(state.doc.scene().entities().len(), count + 1);
    }

    fn key(state: &AppState, id: Id, k: &str) -> String {
        state.doc.scene().entity(id).and_then(|n| n.entity.get(k).map(str::to_owned)).unwrap_or_default()
    }

    fn drag_2d(tool: &mut OverlayTool, state: &mut AppState, view: &ViewInfo, from: DVec3, to: DVec3, mods: Modifiers) {
        let a = view.world_to_screen(from).expect("from");
        let b = view.world_to_screen(to).expect("to");
        tool.handle_mouse(state, view, &Mouse::drag_start(a, b).with_modifiers(mods));
        tool.handle_mouse(state, view, &Mouse::drag(a, b).with_modifiers(mods));
        tool.handle_mouse(state, view, &Mouse::drag_end(a, b).with_modifiers(mods));
    }

    #[test]
    fn resizes_an_edge_in_the_top_view_with_grid_and_one_undo_step() {
        let mut state = crate::tests_support::settle();
        let id = small_overlay(&mut state, 0.0);
        let mut tool = OverlayTool::new();
        let view = crate::tests_support::info_2d(hp_app::ViewKind::Top, 1.0);
        let mid = view.world_to_screen(DVec3::new(32.0, 0.0, 0.0)).expect("edge");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(mid));
        assert_eq!(tool.hover.map(|h| (h.0, h.1)), Some((id, Grip::Edge(2))));
        assert_eq!(tool.cursor(&state, &view, mid), Some(CursorIcon::Grab));
        let undo = state.doc.history_len().0;
        drag_2d(&mut tool, &mut state, &view, DVec3::new(32.0, 0.0, 0.0), DVec3::new(101.0, 5.0, 0.0), Modifiers::NONE);
        assert_eq!(uv(&state, id, "uv2"), DVec2::new(96.0, 32.0));
        assert_eq!(uv(&state, id, "uv3"), DVec2::new(96.0, -32.0));
        assert_eq!(uv(&state, id, "uv0"), DVec2::new(-32.0, -32.0));
        assert_eq!(key(&state, id, "EndU"), "1", "plain resize stretches the texture like Hammer");
        assert_eq!(state.doc.history_len().0, undo + 1);
        assert_eq!(state.doc.undo_label(), Some("Edit overlay"));
        let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
        drag_2d(&mut tool, &mut state, &view, DVec3::new(32.0, 32.0, 0.0), DVec3::new(32.0, 96.0, 0.0), ctrl);
        assert_eq!(uv(&state, id, "uv1"), DVec2::new(-32.0, 96.0));
        assert_eq!(key(&state, id, "EndV"), "2", "Ctrl keeps the texture scale");
        assert_eq!(key(&state, id, "StartV"), "0");
        assert_eq!(key(&state, id, "EndU"), "1");
        drag_2d(
            &mut tool,
            &mut state,
            &view,
            DVec3::new(96.0, 96.0, 0.0),
            DVec3::new(900.0, 900.0, 0.0),
            Modifiers::NONE,
        );
        assert_eq!(uv(&state, id, "uv2"), DVec2::new(256.0, 256.0), "the corner stays on its faces");
    }

    #[test]
    fn moves_and_rotates_in_the_top_view() {
        let mut state = crate::tests_support::settle();
        let id = small_overlay(&mut state, 0.0);
        let mut tool = OverlayTool::new();
        let view = crate::tests_support::info_2d(hp_app::ViewKind::Top, 1.0);
        let inside = view.world_to_screen(DVec3::new(10.0, 10.0, 0.0)).expect("inside");
        assert_eq!(tool.cursor(&state, &view, inside), Some(CursorIcon::Move));
        drag_2d(
            &mut tool,
            &mut state,
            &view,
            DVec3::new(10.0, 10.0, 0.0),
            DVec3::new(45.0, 13.0, 0.0),
            Modifiers::NONE,
        );
        assert_eq!(key(&state, id, "BasisOrigin"), "32 0 0");
        assert_eq!(key(&state, id, "origin"), "32 0 0");
        assert_eq!(key(&state, id, "sides"), "1");
        assert_eq!(uv(&state, id, "uv0"), DVec2::new(-32.0, -32.0), "a move keeps the shape");
        assert!(state.doc.undo());
        assert_eq!(key(&state, id, "BasisOrigin"), "0 0 0");
        let handle =
            rotate_handle(&OverlayShape::from_entity(&state.doc.scene().entity(id).unwrap().entity).unwrap(), &view);
        assert!((handle - DVec3::new(0.0, 32.0 + ROTATE_OFFSET, 0.0)).length() < 1e-6);
        drag_2d(&mut tool, &mut state, &view, handle, DVec3::new(-60.0, 3.0, 0.0), Modifiers::NONE);
        assert_eq!(uv(&state, id, "uv0"), DVec2::new(32.0, -32.0), "rotated by 90 degrees, snapped to 15");
        assert_eq!(uv(&state, id, "uv2"), DVec2::new(-32.0, 32.0));
        assert_eq!(state.doc.undo_label(), Some("Edit overlay"));
    }

    #[test]
    fn clicks_select_in_2d_and_edge_on_overlays_have_no_handles() {
        let mut state = crate::tests_support::settle();
        let id = small_overlay(&mut state, 0.0);
        state.doc.set_selection(&[]);
        let mut tool = OverlayTool::new();
        let view = crate::tests_support::info_2d(hp_app::ViewKind::Top, 1.0);
        let on = view.world_to_screen(DVec3::new(5.0, 5.0, 0.0)).expect("on");
        assert_eq!(tool.cursor(&state, &view, on), Some(CursorIcon::PointingHand));
        tool.handle_mouse(&mut state, &view, &Mouse::click(on));
        assert_eq!(state.doc.selected_entity_ids(), vec![id]);
        let off = view.world_to_screen(DVec3::new(300.0, 300.0, 0.0)).expect("off");
        tool.handle_mouse(&mut state, &view, &Mouse::click(off));
        assert_eq!(state.doc.selected_entity_ids(), vec![id]);
        assert_eq!(tool.cursor(&state, &view, off), Some(CursorIcon::NotAllowed));
        let front = crate::tests_support::info_2d(hp_app::ViewKind::Front, 1.0);
        let corner = front.world_to_screen(DVec3::new(32.0, 32.0, 0.0)).expect("corner");
        assert!(tool.grip_at(&state, &front, corner).is_none_or(|(_, g)| g == Grip::Move));
    }
}
