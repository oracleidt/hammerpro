use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use glam::{DVec2, DVec3};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::{FaceEdit, FaceRef};
use hp_vmf::Side;

use crate::uv_layers::{Affine2, Layer, LayerEditor, LayerModel, LayerTexture, LayerValues, Transform};
use crate::uv_ops::{EdgeAxis, EdgeOp, labels};
use crate::uv_select::{Elem, SelOp};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UvFrame {
    pub a: DVec3,
    pub b: DVec3,
    pub shift: DVec2,
    pub rotation: f64,
}

impl UvFrame {
    pub(crate) fn of(side: &Side) -> Option<UvFrame> {
        let (su, sv) = (side.uaxis.scale, side.vaxis.scale);
        if su.abs() < 1e-9 || sv.abs() < 1e-9 || !su.is_finite() || !sv.is_finite() {
            return None;
        }
        let a = side.uaxis.axis / su;
        let b = side.vaxis.axis / sv;
        (a.is_finite() && b.is_finite() && a.length_squared() > 0.0 && b.length_squared() > 0.0).then_some(UvFrame {
            a,
            b,
            shift: DVec2::new(side.uaxis.shift, side.vaxis.shift),
            rotation: side.rotation,
        })
    }

    pub(crate) fn uv(&self, p: DVec3) -> DVec2 {
        DVec2::new(p.dot(self.a) + self.shift.x, p.dot(self.b) + self.shift.y)
    }

    pub(crate) fn moved(&self, d: DVec2) -> UvFrame {
        UvFrame { shift: self.shift + d, ..*self }
    }

    pub(crate) fn scaled(&self, k: DVec2, anchor: DVec2) -> UvFrame {
        UvFrame {
            a: self.a * k.x,
            b: self.b * k.y,
            shift: DVec2::new(k.x * self.shift.x + anchor.x * (1.0 - k.x), k.y * self.shift.y + anchor.y * (1.0 - k.y)),
            rotation: self.rotation,
        }
    }

    pub(crate) fn rotated(&self, degrees: f64, center: DVec2) -> UvFrame {
        let (s, c) = degrees.to_radians().sin_cos();
        let rot = |v: DVec2| DVec2::new(c * v.x - s * v.y, s * v.x + c * v.y);
        let sh = rot(self.shift - center) + center;
        UvFrame {
            a: self.a * c - self.b * s,
            b: self.a * s + self.b * c,
            shift: sh,
            rotation: normalize_deg(self.rotation - degrees),
        }
    }

    pub(crate) fn wrapped(&self, tex: DVec2) -> UvFrame {
        let wrap = |v: f64, size: f64| if size > 0.0 && v.is_finite() { v.rem_euclid(size) } else { v };
        UvFrame { shift: DVec2::new(wrap(self.shift.x, tex.x), wrap(self.shift.y, tex.y)), ..*self }
    }

    pub(crate) fn edit(&self, dirs: (DVec3, DVec3)) -> Option<FaceEdit> {
        let axis = |v: DVec3, dir: DVec3| {
            let len = v.length();
            if !(len.is_finite() && len > 1e-12) {
                return None;
            }
            let unit = v / len;
            Some(if unit.dot(dir) < -0.999_999 { (dir, -1.0 / len) } else { (unit, 1.0 / len) })
        };
        let (u, su) = axis(self.a, dirs.0)?;
        let (v, sv) = axis(self.b, dirs.1)?;
        (self.shift.is_finite() && su.is_finite() && sv.is_finite()).then_some(FaceEdit {
            u_axis: Some(u),
            v_axis: Some(v),
            u_scale: Some(su),
            v_scale: Some(sv),
            u_shift: Some(self.shift.x),
            v_shift: Some(self.shift.y),
            rotation_value: Some(self.rotation),
            ..FaceEdit::default()
        })
    }
}

const MAX_TEXTURE_REPEATS: i64 = 40_000;

fn uv_change(old: &UvFrame, new: &UvFrame, poly: &[DVec3], tex: DVec2) -> Option<Affine2> {
    let raw = |v: DVec2| DVec2::new(v.x / tex.x, v.y / tex.y);
    let p0 = *poly.first()?;
    let p1 = *poly.iter().max_by(|a, b| (**a - p0).length_squared().total_cmp(&(**b - p0).length_squared()))?;
    let p2 = *poly.iter().max_by(|a, b| {
        let area = |p: DVec3| (p1 - p0).cross(p - p0).length_squared();
        area(**a).total_cmp(&area(**b))
    })?;
    let (u0, u1, u2) = (raw(old.uv(p0)), raw(old.uv(p1)), raw(old.uv(p2)));
    let (v0, v1, v2) = (raw(new.uv(p0)), raw(new.uv(p1)), raw(new.uv(p2)));
    let (a, b) = (u1 - u0, u2 - u0);
    let det = a.x * b.y - a.y * b.x;
    if !(det.is_finite() && det.abs() > 1e-12) {
        return None;
    }
    let (c, d) = (v1 - v0, v2 - v0);
    let inv = [[b.y / det, -b.x / det], [-a.y / det, a.x / det]];
    let m = [
        [c.x * inv[0][0] + d.x * inv[1][0], c.x * inv[0][1] + d.x * inv[1][1]],
        [c.y * inv[0][0] + d.y * inv[1][0], c.y * inv[0][1] + d.y * inv[1][1]],
    ];
    let t = [v0.x - (m[0][0] * u0.x + m[0][1] * u0.y), v0.y - (m[1][0] * u0.x + m[1][1] * u0.y)];
    let out = Affine2 { m, t };
    (out.m.iter().flatten().chain(out.t.iter()).all(|v| v.is_finite())).then_some(out)
}

fn keep_layers(layers: &mut LayerEditor, state: &mut AppState, before: &[(FaceRef, UvFrame)], tag: &str) {
    let mut kept: Vec<Layer> = Vec::new();
    let mut failed: Vec<(Layer, String)> = Vec::new();
    for (f, old) in before {
        let Some(side) = state.doc.face_side(*f) else { continue };
        let Some(new) = UvFrame::of(side) else { continue };
        let material = crate::util::normalize_material(&side.material);
        let Some(poly) = state.doc.face_polygon(*f).map(<[DVec3]>::to_vec) else { continue };
        let tex = crate::util::material_sizes(state, [material.as_str()])
            .get(&material)
            .map_or(DVec2::splat(128.0), |(w, h)| DVec2::new(f64::from((*w).max(1)), f64::from((*h).max(1))));
        let Some(a) = uv_change(old, &new, &poly, tex) else { continue };
        let (k, e) = layers.keep_in_place(state, *f, &a, tag);
        for l in k {
            if !kept.contains(&l) {
                kept.push(l);
            }
        }
        failed.extend(e);
    }
    if !kept.is_empty() {
        let names: Vec<&str> = kept.iter().map(|l| l.label()).collect();
        state.log(Severity::Info, tf!("tex.uvl.kept", names.join(", ")));
    }
    if let Some((l, why)) = failed.first() {
        state.notify(Severity::Warning, tf!("tex.uvl.keep_failed", l.label(), why));
    }
}

pub(crate) fn normalize_deg(d: f64) -> f64 {
    let r = d.rem_euclid(360.0);
    if r > 180.0 { r - 360.0 } else { r }
}

fn uv_bounds(pts: &[DVec2]) -> Option<(DVec2, DVec2)> {
    let mut it = pts.iter();
    let first = *it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Scale { x: bool, y: bool, at: DVec2, anchor: DVec2 },
    Rotate,
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    Move { last: Pos2, moved: DVec2, frame: UvFrame },
    Scale { handle: DVec2, anchor: DVec2, x: bool, y: bool, rel: DVec2, frame: UvFrame },
    Rotate { start: Pos2, center: DVec2, frame: UvFrame },
    Pan { last: Pos2 },
    LayerMove { last: Pos2, moved: DVec2, values: LayerValues, map: Affine2 },
    LayerScale { handle: DVec2, anchor: DVec2, x: bool, y: bool, values: LayerValues, map: Affine2 },
    LayerRotate { start: Pos2, center: DVec2, values: LayerValues, map: Affine2 },
    Verts { last: Pos2, moved: DVec2, grab: Option<DVec2>, edges: bool },
    EdgeTurn { start: Pos2, center: Pos2 },
    Box { start: Pos2, last: Pos2, op: SelOp },
    LayerVerts { last: Pos2, moved: DVec2, values: LayerValues, map: Affine2 },
    LayerEdgeTurn { start: Pos2, center: Pos2, pivot: [f64; 2], values: LayerValues, map: Affine2 },
}

#[derive(Clone, Debug)]
struct VertFace {
    face: FaceRef,
    frame: UvFrame,
    dirs: (DVec3, DVec3),
    uvs: Vec<DVec2>,
    moved: Vec<bool>,
    pivot: DVec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Knob,
    Handle(Handle),
    Face,
    Other(usize),
    Edge(FaceRef, usize),
    Vertex(FaceRef, usize),
    EdgeKnob,
    Empty,
}

#[derive(Clone, Copy, Debug)]
struct DragState {
    kind: Drag,
    face: FaceRef,
    dirs: (DVec3, DVec3),
    hit: Hit,
}

struct OtherShown {
    index: usize,
    off: DVec2,
    pts: Vec<Pos2>,
}

const MAX_OTHERS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickMode {
    Face,
    Edge,
    Vertex,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PickOp {
    Edge(EdgeOp),
    Origin(FaceRef, usize),
    Nudge(DVec2),
}

const PICK: Color32 = Color32::from_rgb(255, 210, 60);

#[derive(Clone, Copy, Debug)]
enum Op {
    Move(DVec2),
    Scale(DVec2, DVec2),
    Rotate(f64),
}

#[derive(Clone, Copy, Debug)]
struct Other {
    face: FaceRef,
    frame: UvFrame,
    lo: DVec2,
    hi: DVec2,
    dirs: (DVec3, DVec3),
}

impl Other {
    fn of(state: &AppState, face: FaceRef) -> Option<Other> {
        let side = state.doc.face_side(face)?;
        let frame = UvFrame::of(side)?;
        let poly = state.doc.face_polygon(face)?;
        let uvs: Vec<DVec2> = poly.iter().map(|p| frame.uv(*p)).collect();
        let (lo, hi) = uv_bounds(&uvs)?;
        Some(Other { face, frame, lo, hi, dirs: (side.uaxis.axis, side.vaxis.axis) })
    }

    fn apply(&self, op: Op) -> UvFrame {
        match op {
            Op::Move(t) => self.frame.moved(t),
            Op::Scale(k, rel) => self.frame.scaled(k, self.lo + (self.hi - self.lo) * rel),
            Op::Rotate(deg) => self.frame.rotated(deg, (self.lo + self.hi) * 0.5),
        }
    }
}

struct LayerView {
    model: LayerModel,
    values: LayerValues,
    texture: Option<LayerTexture>,
    map: Affine2,
    inv: Affine2,
    editable: bool,
    bake: bool,
}

const LAYER_TILE: Color32 = Color32::from_rgb(80, 200, 255);

fn arr(v: DVec2) -> [f64; 2] {
    [v.x, v.y]
}

fn dv(a: [f64; 2]) -> DVec2 {
    DVec2::new(a[0], a[1])
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct Shown {
    pub rect: Option<Rect>,
    pub face: Vec<Pos2>,
    pub tile: Vec<Pos2>,
    pub others: Vec<(usize, Vec<Pos2>)>,
    pub edge_knob: Option<Pos2>,
}

pub struct UvEditor {
    #[cfg(test)]
    pub(crate) shown: Shown,
    active: usize,
    fitted: Option<(FaceRef, Layer)>,
    center: DVec2,
    zoom: f64,
    drag: Option<DragState>,
    drag_view: Option<DVec2>,
    others: Vec<Other>,
    pub all_faces: bool,
    pub fill: bool,
    layer: Layer,
    layers: LayerEditor,
    serial: u64,
    pub(crate) mode: PickMode,
    pub(crate) verts: Vec<Elem>,
    pub(crate) edges: Vec<Elem>,
    vdrag: Vec<VertFace>,
    keys_here: bool,
    shown_face: Option<FaceRef>,
    fit_tiles: f64,
}

impl Default for UvEditor {
    fn default() -> Self {
        Self {
            #[cfg(test)]
            shown: Shown::default(),
            active: 0,
            fitted: None,
            center: DVec2::ZERO,
            zoom: 0.0,
            drag: None,
            drag_view: None,
            others: Vec::new(),
            all_faces: true,
            fill: false,
            layer: Layer::Base,
            layers: LayerEditor::default(),
            serial: 0,
            mode: PickMode::Face,
            verts: Vec::new(),
            edges: Vec::new(),
            vdrag: Vec::new(),
            keys_here: false,
            shown_face: None,
            fit_tiles: 1.0,
        }
    }
}

impl UvEditor {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState, faces: &[FaceRef]) {
        if faces.is_empty() {
            return;
        }
        if self.active >= faces.len() {
            self.active = 0;
        }
        let all = self.all_faces && faces.len() > 1;
        ui.horizontal_wrapped(|ui| {
            if faces.len() > 1 {
                ui.toggle_value(&mut self.all_faces, tf!("tex.uv.all_faces", faces.len()))
                    .on_hover_text(t!("tex.uv.all_faces_tip"));
                if ui.small_button("◀").clicked() {
                    self.active = (self.active + faces.len() - 1) % faces.len();
                }
                ui.label(tf!("tex.uv.face_n", self.active + 1, faces.len()));
                if ui.small_button("▶").clicked() {
                    self.active = (self.active + 1) % faces.len();
                }
            }
            if ui.small_button(t!("tex.uv.frame")).on_hover_text(t!("tex.uv.frame_tip")).clicked() {
                self.fitted = None;
            }
            ui.separator();
            for (m, label, tip) in [
                (PickMode::Face, t!("tex.uv.mode_face"), t!("tex.uv.mode_face_tip")),
                (PickMode::Edge, t!("tex.uv.mode_edge"), t!("tex.uv.mode_edge_tip")),
                (PickMode::Vertex, t!("tex.uv.mode_vertex"), t!("tex.uv.mode_vertex_tip")),
            ] {
                ui.selectable_value(&mut self.mode, m, label).on_hover_text(tip);
            }
        });
        let face = faces[self.active];
        self.shown_face = Some(face);
        let Some(side) = state.doc.face_side(face).cloned() else { return };
        let Some(poly) = state.doc.face_polygon(face).map(<[DVec3]>::to_vec) else { return };
        let Some(frame) = UvFrame::of(&side) else {
            ui.label(egui::RichText::new(t!("tex.uv.bad_axes")).weak());
            return;
        };
        let material = crate::util::normalize_material(&side.material);
        let sizes = crate::util::material_sizes(state, [material.as_str()]);
        let (tw, th) = sizes.get(&material).copied().unwrap_or((128, 128));
        let tex = DVec2::new(f64::from(tw.max(1)), f64::from(th.max(1)));

        let resolved = crate::uv_layers::resolved_material(state, &material);
        let available = resolved.as_ref().map_or_else(|| vec![Layer::Base], crate::uv_layers::layers_of);
        if !available.contains(&self.layer) {
            self.layer = Layer::Base;
        }
        if available.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.label(t!("tex.uvl.layer"));
                for l in &available {
                    ui.selectable_value(&mut self.layer, *l, l.label());
                }
            });
        }
        let layer = self.layer;
        let layer_model = match (&resolved, layer) {
            (Some(mat), l) if l != Layer::Base => Some(self.layers.sync(mat, &material, l)),
            _ => None,
        };
        let lv: Option<LayerView> = layer_model.as_ref().and_then(|(model, values)| {
            let editable = model.editable();
            let map = model.written(values);
            let inv = map.inverse()?;
            let texture =
                model.edit_texture().map(str::to_owned).and_then(|n| self.layers.texture(ui.ctx(), state, &n));
            Some(LayerView { model: model.clone(), values: *values, texture, map, inv, editable, bake: model.bakes() })
        });

        let width = ui.available_width().max(120.0);
        let height = if self.fill {
            (ui.available_height() - if layer_model.is_some() { 400.0 } else { 90.0 }).max(160.0)
        } else {
            (width * 0.85).clamp(160.0, 340.0)
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);

        let uvs: Vec<DVec2> = poly.iter().map(|p| frame.uv(*p)).collect();
        let Some((lo, hi)) = uv_bounds(&uvs) else { return };
        let to_raw = move |x: DVec2| [x.x / tex.x, x.y / tex.y];
        let from_raw = move |r: [f64; 2]| DVec2::new(r[0] * tex.x, r[1] * tex.y);
        let tile = lv.as_ref().map(|lv| {
            let q = lv.map.apply(to_raw((lo + hi) * 0.5));
            let t0 = DVec2::new(q[0].floor(), q[1].floor());
            (t0, t0 + DVec2::ONE)
        });
        let tile_texels = |lv: &LayerView, (t0, t1): (DVec2, DVec2)| -> [DVec2; 4] {
            [
                from_raw(lv.inv.apply([t0.x, t0.y])),
                from_raw(lv.inv.apply([t1.x, t0.y])),
                from_raw(lv.inv.apply([t1.x, t1.y])),
                from_raw(lv.inv.apply([t0.x, t1.y])),
            ]
        };
        if self.fitted != Some((face, layer)) || !(self.zoom.is_finite() && self.zoom > 0.0) {
            let face_span = (hi - lo).max(tex * 0.25);
            let (mut flo, mut fhi) = (lo, hi);
            if let (Some(lv), Some(t)) = (&lv, tile)
                && let Some((tlo, thi)) = uv_bounds(&tile_texels(lv, t))
                && (thi - tlo).max_element() < face_span.max_element() * 4.0
            {
                flo = flo.min(tlo);
                fhi = fhi.max(thi);
            }
            let size = (fhi - flo).max(tex * 0.25);
            self.center = (flo + fhi) * 0.5 - (lo + hi) * 0.5;
            self.zoom = (f64::from(width.min(height)) * 0.8 / size.max_element()).clamp(1e-3, 1e3);
            self.fitted = Some((face, layer));
        }
        let (view_center, view_zoom) = (self.drag_view.unwrap_or((lo + hi) * 0.5 + self.center), self.zoom);
        let to_screen = move |t: DVec2| {
            let d = (t - view_center) * view_zoom;
            rect.center() + vec2(d.x as f32, d.y as f32)
        };
        let to_uv = move |p: Pos2| {
            let d = p - rect.center();
            view_center + DVec2::new(f64::from(d.x), f64::from(d.y)) / view_zoom
        };

        let view_lo = to_uv(rect.min);
        let view_hi = to_uv(rect.max);
        let (i0, i1) = ((view_lo.x / tex.x).floor() as i64, (view_hi.x / tex.x).ceil() as i64);
        let (j0, j1) = ((view_lo.y / tex.y).floor() as i64, (view_hi.y / tex.y).ceil() as i64);
        let show_base = lv.is_none() || self.layers.show_base;
        let base_texture = resolved
            .as_ref()
            .filter(|_| show_base)
            .and_then(hp_vmt::Material::preview_texture)
            .and_then(|name| self.layers.texture(ui.ctx(), state, &name));
        let tiles = (i1 - i0).saturating_mul(j1 - j0);
        if show_base && (0..=MAX_TEXTURE_REPEATS).contains(&tiles) {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            let mut mesh = base_texture.as_ref().map(|texture| egui::Mesh::with_texture(texture.handle.id()));
            let mut outlines = Vec::new();
            for i in i0..i1 {
                for j in j0..j1 {
                    let a = to_screen(DVec2::new(i as f64 * tex.x, j as f64 * tex.y));
                    let b = to_screen(DVec2::new((i + 1) as f64 * tex.x, (j + 1) as f64 * tex.y));
                    let tile = Rect::from_two_pos(a, b);
                    if let Some(mesh) = &mut mesh {
                        mesh.add_rect_with_uv(tile, uv, Color32::WHITE);
                    }
                    if tiles <= 400 && tile.width().min(tile.height()) >= 12.0 {
                        outlines.push(tile);
                    }
                }
            }
            if let Some(mesh) = mesh {
                painter.add(egui::Shape::mesh(mesh));
            }
            for tile in outlines {
                painter.rect_stroke(
                    tile,
                    0.0,
                    Stroke::new(1.0, Color32::from_black_alpha(90)),
                    egui::StrokeKind::Inside,
                );
            }
        }

        let accent = theme::accent();
        if let Some(lv) = &lv {
            match &lv.texture {
                Some(t) => {
                    let color = if self.layers.show_base {
                        Color32::from_white_alpha((self.layers.opacity.clamp(0.0, 1.0) * 255.0) as u8)
                    } else {
                        Color32::WHITE
                    };
                    let at = |p: Pos2| lv.map.apply(to_raw(to_uv(p)));
                    let mid = at(rect.center());
                    let off = [mid[0].floor(), mid[1].floor()];
                    let mut mesh = egui::Mesh::with_texture(t.handle.id());
                    for c in [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()] {
                        let q = at(c);
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: c,
                            uv: pos2((q[0] - off[0]) as f32, (q[1] - off[1]) as f32),
                            color,
                        });
                    }
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    painter.add(egui::Shape::mesh(mesh));
                }
                None => {
                    painter.text(
                        rect.center_top() + vec2(0.0, 8.0),
                        egui::Align2::CENTER_TOP,
                        t!("tex.uvl.no_texture"),
                        egui::FontId::proportional(12.0),
                        ui.visuals().warn_fg_color,
                    );
                }
            }
            if !lv.editable {
                painter.text(
                    rect.left_bottom() + vec2(6.0, -6.0),
                    egui::Align2::LEFT_BOTTOM,
                    t!("tex.uvl.not_drawn_badge"),
                    egui::FontId::proportional(11.0),
                    ui.visuals().warn_fg_color,
                );
            }
        } else if layer != Layer::Base {
            painter.text(
                rect.center_top() + vec2(0.0, 8.0),
                egui::Align2::CENTER_TOP,
                t!("tex.uvl.flat"),
                egui::FontId::proportional(12.0),
                ui.visuals().warn_fg_color,
            );
        }

        let base_mode = lv.is_none() && layer == Layer::Base;
        let pts: Vec<Pos2> = uvs.iter().map(|t| to_screen(*t)).collect();
        let mut handles: Vec<(Handle, Pos2)> = Vec::new();
        let mut knob: Option<Pos2> = None;
        let mut tile_pts: Vec<Pos2> = Vec::new();
        let mut others: Vec<OtherShown> = Vec::new();
        if let (Some(lv), Some(t)) = (&lv, tile) {
            tile_pts = tile_texels(lv, t).iter().map(|c| to_screen(*c)).collect();
            if lv.editable {
                let top = tile_pts.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
                let cx = tile_pts.iter().map(|c| c.x).sum::<f32>() / 4.0;
                knob = Some(pos2(cx, top - 22.0));
                for h in handles_of(t.0, t.1) {
                    if let Handle::Scale { at, .. } = h {
                        handles.push((h, to_screen(from_raw(lv.inv.apply(arr(at))))));
                    }
                }
            }
        } else if base_mode {
            for (i, f) in faces.iter().enumerate() {
                if i == self.active || others.len() >= MAX_OTHERS {
                    continue;
                }
                let same =
                    state.doc.face_side(*f).is_some_and(|s| crate::util::normalize_material(&s.material) == material);
                let (true, Some(o), Some(poly)) = (same, Other::of(state, *f), state.doc.face_polygon(*f)) else {
                    continue;
                };
                let off = (((o.lo + o.hi) * 0.5 - (lo + hi) * 0.5) / tex).round() * tex;
                let pts = poly.iter().map(|p| to_screen(o.frame.uv(*p) - off)).collect();
                others.push(OtherShown { index: i, off, pts });
            }
            let box_lo = to_screen(lo);
            let box_hi = to_screen(hi);
            knob = Some(pos2((box_lo.x + box_hi.x) * 0.5, box_lo.y.min(box_hi.y) - 22.0));
            for h in handles_of(lo, hi) {
                if let Handle::Scale { at, .. } = h {
                    handles.push((h, to_screen(at)));
                }
            }
        }
        if self.mode != PickMode::Face {
            handles.clear();
            knob = None;
        }
        let outlines: Vec<(FaceRef, &[Pos2])> = std::iter::once((face, pts.as_slice()))
            .chain(others.iter().map(|o| (faces[o.index], o.pts.as_slice())))
            .collect();
        let len_of = |f: FaceRef| outlines.iter().find(|(g, _)| *g == f).map(|(_, o)| o.len());
        self.verts.retain(|(f, i)| len_of(*f).is_some_and(|n| *i < n));
        self.edges.retain(|(f, i)| len_of(*f).is_some_and(|n| *i < n));
        let mode = self.mode;
        let face_mode = mode == PickMode::Face;
        let editable = if base_mode { true } else { lv.as_ref().is_some_and(|lv| lv.editable) };
        let main_edge = if mode == PickMode::Edge { crate::uv_select::main_of(&self.edges, face) } else { None };
        let edge_frame = main_edge.filter(|_| pts.len() >= 2).map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let mid = a + (b - a) * 0.5;
            let along = if (b - a).length_sq() > 0.0 { (b - a).normalized() } else { vec2(1.0, 0.0) };
            let mut normal = vec2(-along.y, along.x);
            if (mid - centroid_of(&pts)).dot(normal) < 0.0 {
                normal = -normal;
            }
            (mid, along, normal)
        });
        let edge_knob = edge_frame.filter(|_| editable).and_then(|(mid, _, normal)| {
            [mid + normal * 52.0, mid - normal * 52.0]
                .into_iter()
                .find(|k| rect.shrink(4.0).contains(*k))
                .map(|k| (k, mid))
        });
        #[cfg(test)]
        {
            self.shown = Shown {
                rect: Some(rect),
                face: pts.clone(),
                tile: tile_pts.clone(),
                others: others.iter().map(|o| (o.index, o.pts.clone())).collect(),
                edge_knob: edge_knob.map(|(k, _)| k),
            };
        }

        let mine: Vec<&[Pos2]> =
            if lv.is_some() { vec![pts.as_slice(), tile_pts.as_slice()] } else { vec![pts.as_slice()] };
        let other_pts: Vec<(usize, &[Pos2])> = others.iter().map(|o| (o.index, o.pts.as_slice())).collect();
        let hit_at = |p: Pos2| {
            if !editable {
                return Hit::Empty;
            }
            match mode {
                PickMode::Vertex => {
                    if let Some((f, i)) = crate::uv_select::nearest_vertex(&outlines, p, 9.0) {
                        return Hit::Vertex(f, i);
                    }
                }
                PickMode::Edge => {
                    if edge_knob.is_some_and(|(k, _)| k.distance(p) <= 9.0) {
                        return Hit::EdgeKnob;
                    }
                    if let Some((f, i)) = crate::uv_select::nearest_edge(&outlines, p, 7.0) {
                        return Hit::Edge(f, i);
                    }
                }
                PickMode::Face => {}
            }
            hit_test(p, knob, &handles, &mine, &other_pts)
        };
        let hover: Option<Hit> = match &self.drag {
            Some(d) => Some(d.hit),
            None => resp.hover_pos().map(hit_at),
        };
        let face_hot = face_mode && (matches!(hover, Some(Hit::Face)) || (all && matches!(hover, Some(Hit::Other(_)))));
        let other_hot = |i: usize| {
            face_mode
                && match hover {
                    Some(Hit::Other(j)) => all || j == i,
                    Some(Hit::Face) => all,
                    _ => false,
                }
        };
        let knob_hot = matches!(hover, Some(Hit::Knob));

        if let (Some(lv), Some(t)) = (&lv, tile) {
            painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(3.5, Color32::from_black_alpha(160))));
            painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(1.5, Color32::WHITE)));
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                let d = DVec2::new(dx, dy);
                let n: Vec<Pos2> = tile_texels(lv, (t.0 + d, t.1 + d)).iter().map(|c| to_screen(*c)).collect();
                painter.add(egui::Shape::closed_line(n, Stroke::new(1.0, Color32::from_white_alpha(70))));
            }
            if face_hot {
                painter.add(egui::Shape::convex_polygon(
                    tile_pts.clone(),
                    LAYER_TILE.gamma_multiply(0.22),
                    Stroke::NONE,
                ));
            }
            painter.add(egui::Shape::closed_line(
                tile_pts.clone(),
                Stroke::new(if face_hot { 3.0 } else { 2.0 }, LAYER_TILE),
            ));
            if let Some(k) = knob {
                let middle = to_screen(from_raw(lv.inv.apply(arr((t.0 + t.1) * 0.5))));
                painter.line_segment([middle, k], Stroke::new(1.0, LAYER_TILE.gamma_multiply(0.6)));
                let fill = if knob_hot { LAYER_TILE } else { ui.visuals().extreme_bg_color };
                painter.circle(k, 6.0, fill, Stroke::new(2.0, LAYER_TILE));
            }
        } else {
            for o in &others {
                let (fill, stroke) = if other_hot(o.index) {
                    (45, Stroke::new(2.0, accent))
                } else {
                    (10, Stroke::new(1.0, accent.gamma_multiply(0.5)))
                };
                painter.add(egui::Shape::convex_polygon(o.pts.clone(), Color32::from_white_alpha(fill), stroke));
            }
            let (fill, width) = if face_hot { (60, 3.0) } else { (28, 2.0) };
            painter.add(egui::Shape::convex_polygon(
                pts.clone(),
                Color32::from_white_alpha(fill),
                Stroke::new(width, accent),
            ));
            for p in &pts {
                painter.circle_filled(*p, 2.5, accent);
            }
            if let Some(k) = knob {
                let box_top = to_screen(lo).y.min(to_screen(hi).y);
                painter.line_segment([pos2(k.x, box_top), k], Stroke::new(1.0, accent));
                let fill = if knob_hot { accent } else { ui.visuals().extreme_bg_color };
                painter.circle(k, 6.0, fill, Stroke::new(2.0, accent));
            }
        }
        if !face_mode {
            for (f, o) in &outlines {
                let n = o.len();
                let mask = match mode {
                    PickMode::Edge => crate::uv_select::moved_mask(n, *f, &[], &self.edges),
                    _ => crate::uv_select::moved_mask(n, *f, &self.verts, &[]),
                };
                let screen: Vec<DVec2> = o.iter().map(|q| DVec2::new(f64::from(q.x), f64::from(q.y))).collect();
                for i in crate::uv_select::pins(&screen, &mask) {
                    painter.rect_stroke(
                        Rect::from_center_size(o[i], Vec2::splat(13.0)),
                        2.0,
                        Stroke::new(1.5, LAYER_TILE),
                        egui::StrokeKind::Middle,
                    );
                }
                if mode == PickMode::Edge {
                    for i in 0..n {
                        let seg = [o[i], o[(i + 1) % n]];
                        if self.edges.contains(&(*f, i)) {
                            painter.line_segment(seg, Stroke::new(6.0, Color32::from_black_alpha(150)));
                            painter.line_segment(seg, Stroke::new(3.5, PICK));
                        } else if hover == Some(Hit::Edge(*f, i)) {
                            painter.line_segment(seg, Stroke::new(4.0, Color32::WHITE));
                        }
                    }
                } else {
                    for (i, q) in o.iter().enumerate() {
                        let hot = hover == Some(Hit::Vertex(*f, i));
                        let (r, fill) = match (mask[i], hot) {
                            (true, true) => (7.0, PICK),
                            (true, false) => (5.5, PICK),
                            (false, true) => (7.0, Color32::WHITE),
                            (false, false) => (4.5, Color32::WHITE),
                        };
                        painter.circle(*q, r, fill, Stroke::new(1.0, Color32::BLACK));
                    }
                }
            }
            if let Some(i) = main_edge {
                for q in [pts[i], pts[(i + 1) % pts.len()]] {
                    painter.circle(q, 4.5, PICK, Stroke::new(1.0, Color32::BLACK));
                }
            }
            if let Some((k, mid)) = edge_knob {
                painter.line_segment([mid, k], Stroke::new(1.0, PICK));
                let fill = if hover == Some(Hit::EdgeKnob) { PICK } else { ui.visuals().extreme_bg_color };
                painter.circle(k, 6.0, fill, Stroke::new(2.0, PICK));
            }
            if let Some(DragState { kind: Drag::Box { start, last, .. }, .. }) = &self.drag {
                let r = Rect::from_two_pos(*start, *last);
                painter.rect(r, 0.0, accent.gamma_multiply(0.12), Stroke::new(1.0, accent), egui::StrokeKind::Inside);
            }
        }
        let handle_color = if lv.is_some() { LAYER_TILE } else { accent };
        for (h, p) in &handles {
            let fill = if hover == Some(Hit::Handle(*h)) { handle_color } else { Color32::WHITE };
            painter.rect(
                Rect::from_center_size(*p, Vec2::splat(8.0)),
                1.0,
                fill,
                Stroke::new(1.0, Color32::BLACK),
                egui::StrokeKind::Middle,
            );
        }
        if let Some(h) = hover.filter(|_| resp.hovered() || self.drag.is_some()) {
            let text = match (h, lv.is_some()) {
                (Hit::Vertex(..), _) => t!("tex.uv.hint_vertex").to_owned(),
                (Hit::Edge(..), _) => t!("tex.uv.hint_edge").to_owned(),
                (Hit::EdgeKnob, _) => t!("tex.uv.hint_edge_turn").to_owned(),
                (Hit::Face | Hit::Other(_) | Hit::Empty, _) if !face_mode && editable => {
                    t!("tex.uv.hint_box").to_owned()
                }
                (Hit::Empty, _) => t!("tex.uv.hint_pan").to_owned(),
                (Hit::Knob, false) => t!("tex.uv.hint_rotate").to_owned(),
                (Hit::Handle(_), false) => t!("tex.uv.hint_scale").to_owned(),
                (Hit::Face | Hit::Other(_), false) if all => tf!("tex.uv.hint_move_all", faces.len()),
                (Hit::Face, false) => t!("tex.uv.hint_move").to_owned(),
                (Hit::Other(_), false) => t!("tex.uv.hint_other").to_owned(),
                (Hit::Knob, true) => tf!("tex.uvl.hint_rotate", layer.label()),
                (Hit::Handle(_), true) => tf!("tex.uvl.hint_scale", layer.label()),
                (Hit::Face | Hit::Other(_), true) => tf!("tex.uvl.hint_move", layer.label()),
            };
            let at = rect.left_top() + vec2(8.0, 8.0);
            let galley = painter.layout_no_wrap(text, egui::FontId::proportional(11.0), Color32::WHITE);
            painter.rect_filled(
                Rect::from_min_size(at, galley.size()).expand(3.0),
                3.0,
                Color32::from_black_alpha(170),
            );
            painter.galley(at, galley, Color32::WHITE);
        }

        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
        let mods = ui.input(|i| i.modifiers);
        if resp.is_pointer_button_down_on() {
            self.keys_here = true;
        } else if ui.input(|i| i.pointer.any_pressed()) && !resp.hovered() {
            self.keys_here = false;
        }
        if resp.drag_started()
            && let Some(p) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
        {
            let secondary = ui.input(|i| i.pointer.secondary_down() || i.pointer.middle_down());
            self.serial += 1;
            let hit = if secondary { Hit::Empty } else { hit_at(p) };
            let sel_drag: Option<Drag> = if secondary || face_mode || !editable {
                None
            } else {
                match hit {
                    Hit::Vertex(f, i) | Hit::Edge(f, i) => {
                        let edges = matches!(hit, Hit::Edge(..));
                        let sel = if edges { &mut self.edges } else { &mut self.verts };
                        if !sel.contains(&(f, i)) {
                            let op = if SelOp::of(mods) == SelOp::Replace { SelOp::Replace } else { SelOp::Add };
                            crate::uv_select::pick(sel, &[(f, i)], op);
                        }
                        let masks = self.selection_masks(state, &outlines, faces, all && base_mode);
                        self.vdrag = masks
                            .iter()
                            .filter(|(g, _)| lv.is_none() || *g == face)
                            .filter_map(|(g, m)| VertFace::of(state, *g, m.clone(), None))
                            .collect();
                        let grab = match hit {
                            Hit::Vertex(g, i) => {
                                outlines.iter().find(|(h, _)| *h == g).and_then(|(_, o)| o.get(i)).map(|q| to_uv(*q))
                            }
                            _ => None,
                        };
                        match &lv {
                            Some(lv) => {
                                Some(Drag::LayerVerts { last: p, moved: DVec2::ZERO, values: lv.values, map: lv.map })
                            }
                            None if base_mode => Some(Drag::Verts { last: p, moved: DVec2::ZERO, grab, edges }),
                            None => None,
                        }
                    }
                    Hit::EdgeKnob => {
                        let center = edge_knob.map_or(p, |(_, mid)| mid);
                        match &lv {
                            Some(lv) => main_edge.filter(|i| *i < uvs.len()).map(|i| {
                                let n = uvs.len();
                                let (r0, r1) = (to_raw(uvs[i]), to_raw(uvs[(i + 1) % n]));
                                let pivot = [(r0[0] + r1[0]) * 0.5, (r0[1] + r1[1]) * 0.5];
                                Drag::LayerEdgeTurn { start: p, center, pivot, values: lv.values, map: lv.map }
                            }),
                            None if base_mode => {
                                let picks = edge_picks(&self.edges, &outlines);
                                let rest: Vec<FaceRef> = if all { faces.to_vec() } else { Vec::new() };
                                let work = crate::uv_ops::edge_work(state, &picks, &rest).unwrap_or_default();
                                self.vdrag = work
                                    .into_iter()
                                    .filter_map(|(g, e)| VertFace::of(state, g, Vec::new(), Some(e)))
                                    .collect();
                                Some(Drag::EdgeTurn { start: p, center })
                            }
                            None => None,
                        }
                    }
                    Hit::Face | Hit::Other(_) | Hit::Empty => {
                        Some(Drag::Box { start: p, last: p, op: SelOp::of(mods) })
                    }
                    _ => None,
                }
            };
            if let Some(kind) = sel_drag {
                if matches!(kind, Drag::Verts { .. } | Drag::EdgeTurn { .. }) {
                    self.drag_view = Some(view_center);
                }
                self.others.clear();
                self.drag = Some(DragState { kind, face, dirs: (side.uaxis.axis, side.vaxis.axis), hit });
            } else {
                let mut target = (face, frame, lo, hi, (side.uaxis.axis, side.vaxis.axis));
                let mut view = view_center;
                let mut kept = hit;
                if let Hit::Other(i) = hit
                    && !all
                    && let Some(o) = others.iter().find(|o| o.index == i)
                    && let Some(other) = Other::of(state, faces[i])
                {
                    target = (faces[i], other.frame, other.lo, other.hi, other.dirs);
                    view = view_center + o.off;
                    self.active = i;
                    self.fitted = Some((faces[i], layer));
                    kept = Hit::Face;
                }
                let (target_face, target_frame, tlo, thi, dirs) = target;
                self.others = if all && base_mode {
                    faces.iter().filter(|f| **f != target_face).filter_map(|f| Other::of(state, *f)).collect()
                } else {
                    Vec::new()
                };
                let kind = match (&lv, tile, hit) {
                    (_, _, Hit::Empty) => Drag::Pan { last: p },
                    (Some(lv), Some(t), _) => {
                        let (values, map) = (lv.values, lv.map);
                        match hit {
                            Hit::Knob => Drag::LayerRotate { start: p, center: (t.0 + t.1) * 0.5, values, map },
                            Hit::Handle(Handle::Scale { x, y, at, anchor }) => {
                                Drag::LayerScale { handle: at, anchor, x, y, values, map }
                            }
                            _ => Drag::LayerMove { last: p, moved: DVec2::ZERO, values, map },
                        }
                    }
                    _ if base_mode => match hit {
                        Hit::Knob => Drag::Rotate { start: p, center: (tlo + thi) * 0.5, frame: target_frame },
                        Hit::Handle(Handle::Scale { x, y, at, anchor }) => {
                            let span = thi - tlo;
                            let rel = |axis: usize| {
                                if span[axis].abs() < 1e-9 { 0.5 } else { (anchor[axis] - tlo[axis]) / span[axis] }
                            };
                            Drag::Scale {
                                handle: at,
                                anchor,
                                x,
                                y,
                                rel: DVec2::new(rel(0), rel(1)),
                                frame: target_frame,
                            }
                        }
                        _ => Drag::Move { last: p, moved: DVec2::ZERO, frame: target_frame },
                    },
                    _ => Drag::Pan { last: p },
                };
                if matches!(kind, Drag::Move { .. } | Drag::Scale { .. } | Drag::Rotate { .. }) {
                    self.drag_view = Some(view);
                }
                self.drag = Some(DragState { kind, face: target_face, dirs, hit: kept });
            }
        }
        let mut op: Option<PickOp> = None;
        let mut same_step = false;
        let click_pos = resp.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.interact_pos()));
        let click = if resp.clicked() && self.drag.is_none() { click_pos.map(hit_at) } else { None };
        let sel_op = SelOp::of(mods);
        match click {
            Some(Hit::Edge(f, i)) => crate::uv_select::pick(&mut self.edges, &[(f, i)], sel_op),
            Some(Hit::Vertex(f, i)) => crate::uv_select::pick(&mut self.verts, &[(f, i)], sel_op),
            Some(Hit::Face | Hit::Empty) if !face_mode && sel_op == SelOp::Replace => match mode {
                PickMode::Edge => self.edges.clear(),
                _ => self.verts.clear(),
            },
            _ => {}
        }
        if resp.double_clicked() {
            match click_pos.map(hit_at) {
                Some(Hit::Edge(f, i)) => {
                    crate::uv_select::pick(&mut self.edges, &[(f, i)], SelOp::Add);
                    if let Some((_, o)) = outlines.iter().find(|(g, _)| *g == f)
                        && o.len() >= 2
                    {
                        let d = o[(i + 1) % o.len()] - o[i];
                        op = Some(PickOp::Edge(EdgeOp::Align(EdgeAxis::nearest(DVec2::new(
                            f64::from(d.x),
                            f64::from(d.y),
                        )))));
                    }
                }
                Some(Hit::Vertex(f, i)) => op = Some(PickOp::Origin(f, i)),
                _ => {}
            }
        }
        let focus_free = ui.ctx().memory(|m| m.focused().is_none());
        if focus_free && editable && self.drag.is_none() && (resp.hovered() || self.keys_here) {
            let has_sel = match mode {
                PickMode::Vertex => !self.verts.is_empty(),
                PickMode::Edge => !self.edges.is_empty(),
                PickMode::Face => false,
            };
            if has_sel {
                if take_key(ui, egui::Key::Escape, false) {
                    self.verts.clear();
                    self.edges.clear();
                }
                take_key(ui, egui::Key::Delete, false);
                take_key(ui, egui::Key::Backspace, false);
            }
            for (key, dir) in [
                (egui::Key::ArrowLeft, DVec2::new(-1.0, 0.0)),
                (egui::Key::ArrowRight, DVec2::new(1.0, 0.0)),
                (egui::Key::ArrowUp, DVec2::new(0.0, -1.0)),
                (egui::Key::ArrowDown, DVec2::new(0.0, 1.0)),
            ] {
                if take_key(ui, key, true) {
                    op = Some(PickOp::Nudge(dir * 8.0));
                } else if take_key(ui, key, false) {
                    op = Some(PickOp::Nudge(dir));
                }
            }
            if main_edge.is_some() {
                if take_key(ui, egui::Key::X, false) {
                    op = Some(PickOp::Edge(EdgeOp::Align(EdgeAxis::X)));
                }
                if take_key(ui, egui::Key::Y, false) {
                    op = Some(PickOp::Edge(EdgeOp::Align(EdgeAxis::Y)));
                }
            }
        }
        if let Some((mid, along, normal)) = edge_frame.filter(|_| editable) {
            let at = mid + normal * 22.0;
            let along = if along.x < -1e-3 || (along.x.abs() <= 1e-3 && along.y < 0.0) { -along } else { along };
            let size = vec2(30.0, 18.0);
            for (k, axis, label, tip) in
                [(-1.0, EdgeAxis::X, "X", t!("tex.uv.align_x")), (1.0, EdgeAxis::Y, "Y", t!("tex.uv.align_y"))]
            {
                let r = Rect::from_center_size(at + along * (k * 18.0), size);
                if !rect.contains_rect(r) {
                    continue;
                }
                let b =
                    ui.interact(r, ui.make_persistent_id(("hp_uv_align", label)), Sense::click()).on_hover_text(tip);
                let style = ui.style().interact(&b);
                painter.rect(r, 3.0, style.bg_fill, style.bg_stroke, egui::StrokeKind::Inside);
                painter.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(12.0),
                    style.text_color(),
                );
                if b.clicked() {
                    op = Some(PickOp::Edge(EdgeOp::Align(axis)));
                }
            }
        }
        if let Some(Hit::Other(i)) = click
            && let Some(o) = others.iter().find(|o| o.index == i)
            && let Some(other) = Other::of(state, faces[i])
        {
            self.active = i;
            self.fitted = Some((faces[i], layer));
            self.center = view_center + o.off - (other.lo + other.hi) * 0.5;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                ui.ctx().input_mut(|i| {
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                });
                if let Some(p) = pointer {
                    let before = to_uv(p);
                    self.zoom = (self.zoom * (1.004f64).powf(f64::from(scroll))).clamp(1e-3, 1e3);
                    let d = p - rect.center();
                    let center = before - DVec2::new(f64::from(d.x), f64::from(d.y)) / self.zoom;
                    match self.drag_view.as_mut() {
                        Some(v) => *v = center,
                        None => self.center = center - (lo + hi) * 0.5,
                    }
                }
            }
        }
        if resp.hovered() || self.drag.is_some() {
            let resize = |x: bool, y: bool| match (x, y) {
                (true, false) => egui::CursorIcon::ResizeHorizontal,
                (false, true) => egui::CursorIcon::ResizeVertical,
                _ => egui::CursorIcon::ResizeNwSe,
            };
            let cursor = match (self.drag.as_ref().map(|d| d.kind), hover) {
                (Some(Drag::Pan { .. }), _) => egui::CursorIcon::Grabbing,
                (
                    Some(Drag::Move { .. } | Drag::LayerMove { .. } | Drag::Verts { .. } | Drag::LayerVerts { .. }),
                    _,
                ) => egui::CursorIcon::Move,
                (
                    Some(
                        Drag::Rotate { .. }
                        | Drag::LayerRotate { .. }
                        | Drag::EdgeTurn { .. }
                        | Drag::LayerEdgeTurn { .. },
                    ),
                    _,
                ) => egui::CursorIcon::Alias,
                (Some(Drag::Scale { x, y, .. } | Drag::LayerScale { x, y, .. }), _) => resize(x, y),
                (Some(Drag::Box { .. }), _) => egui::CursorIcon::Crosshair,
                (None, Some(Hit::Knob | Hit::EdgeKnob)) => egui::CursorIcon::Alias,
                (None, Some(Hit::Handle(Handle::Scale { x, y, .. }))) => resize(x, y),
                (None, Some(Hit::Edge(..) | Hit::Vertex(..))) => egui::CursorIcon::PointingHand,
                (None, Some(Hit::Face | Hit::Other(_) | Hit::Empty)) if !face_mode && editable => {
                    egui::CursorIcon::Crosshair
                }
                (None, Some(Hit::Face | Hit::Other(_))) => egui::CursorIcon::Move,
                _ => egui::CursorIcon::Grab,
            };
            ui.ctx().set_cursor_icon(cursor);
        }
        let mut next: Option<(UvFrame, Op)> = None;
        let mut next_layer: Option<LayerValues> = None;
        let mut next_verts: Option<(DVec2, bool)> = None;
        let mut next_turn: Option<f64> = None;
        if let (Some(d), Some(p)) = (self.drag.as_mut(), resp.interact_pointer_pos()) {
            match &mut d.kind {
                Drag::Pan { last } => {
                    let m = p - *last;
                    let m = DVec2::new(f64::from(m.x), f64::from(m.y)) / self.zoom;
                    match self.drag_view.as_mut() {
                        Some(v) => *v -= m,
                        None => self.center -= m,
                    }
                    *last = p;
                }
                Drag::Move { last, moved, frame } => {
                    let m = p - *last;
                    *last = p;
                    *moved += DVec2::new(f64::from(m.x), f64::from(m.y)) / view_zoom;
                    let mut t = *moved;
                    if !mods.alt {
                        t = t.round();
                    }
                    next = Some((frame.moved(t), Op::Move(t)));
                }
                Drag::Verts { last, moved, grab, edges } => {
                    let m = p - *last;
                    *last = p;
                    *moved += DVec2::new(f64::from(m.x), f64::from(m.y)) / view_zoom;
                    let mut t = *moved;
                    if !mods.alt {
                        t = t.round();
                        if let Some(g) = grab {
                            let v = *g + t;
                            let line = DVec2::new((v.x / tex.x).round() * tex.x, (v.y / tex.y).round() * tex.y);
                            for axis in 0..2 {
                                if ((line[axis] - v[axis]) * view_zoom).abs() < 8.0 {
                                    t[axis] += line[axis] - v[axis];
                                }
                            }
                        }
                    }
                    next_verts = Some((t, *edges));
                }
                Drag::EdgeTurn { start, center } => {
                    let a0 = (*start - *center).angle();
                    let a1 = (p - *center).angle();
                    let mut deg = f64::from(a1 - a0).to_degrees();
                    if !mods.alt {
                        deg = (deg / 15.0).round() * 15.0;
                    }
                    next_turn = Some(normalize_deg(deg));
                }
                Drag::Box { last, .. } => *last = p,
                Drag::Scale { handle, anchor, x, y, rel, frame } => {
                    let m = to_uv(p);
                    let (kx, ky) = scale_factors(m, *handle, *anchor, *x, *y, mods.shift);
                    let snap = |k: f64, axis: DVec3| {
                        let s = 1.0 / axis.length();
                        let target = s / k;
                        if mods.alt || !target.is_finite() || target.abs() < 0.01 {
                            k
                        } else {
                            s / ((target * 100.0).round() / 100.0)
                        }
                    };
                    let (kx, ky) = (snap(kx, frame.a), snap(ky, frame.b));
                    if kx.abs() > 1e-3 && ky.abs() > 1e-3 {
                        let k = DVec2::new(kx, ky);
                        next = Some((frame.scaled(k, *anchor), Op::Scale(k, *rel)));
                    }
                }
                Drag::Rotate { start, center, frame } => {
                    let c = to_screen(*center);
                    let a0 = (*start - c).angle();
                    let a1 = (p - c).angle();
                    let mut deg = f64::from(a1 - a0).to_degrees();
                    if !mods.alt {
                        deg = (deg / 15.0).round() * 15.0;
                    }
                    next = Some((frame.rotated(deg, *center), Op::Rotate(deg)));
                }
                Drag::LayerMove { last, moved, values, map } => {
                    let d = p - *last;
                    *last = p;
                    *moved += DVec2::new(f64::from(d.x), f64::from(d.y)) / view_zoom;
                    if let Some(lv) = &lv {
                        let r = to_raw(*moved);
                        let w = Affine2 {
                            m: map.m,
                            t: [
                                map.t[0] - (map.m[0][0] * r[0] + map.m[0][1] * r[1]),
                                map.t[1] - (map.m[1][0] * r[0] + map.m[1][1] * r[1]),
                            ],
                        };
                        if let Some(mut tr) = lv.model.solve(&w, values, values.tr.rotate) {
                            if !mods.alt {
                                let size = lv.texture.as_ref().map_or([tw, th], |t| t.size);
                                tr = lv.model.snap_translate(values, values.tr.translate, tr, size);
                            }
                            next_layer = Some(LayerValues { tr, ..*values });
                        }
                    }
                }
                Drag::LayerVerts { last, moved, values, map } => {
                    let d = p - *last;
                    *last = p;
                    *moved += DVec2::new(f64::from(d.x), f64::from(d.y)) / view_zoom;
                    let t = if mods.alt { *moved } else { moved.round() };
                    if let (Some(lv), Some(vf)) = (&lv, self.vdrag.first())
                        && let Some(mut tr) = crate::uv_select::vertex_affine(&vf.uvs, &vf.moved, t)
                            .and_then(|a| crate::uv_select::layer_map_moved(map, &a, tex))
                            .and_then(|w| crate::uv_ops::layer_fit(&lv.model, values, &w))
                    {
                        if lv.bake {
                            let size = lv.texture.as_ref().map_or([tw, th], |t| t.size);
                            tr = lv.model.snap_translate(values, values.tr.translate, tr, size);
                        }
                        next_layer = Some(LayerValues { tr, ..*values });
                    }
                }
                Drag::LayerEdgeTurn { start, center, pivot, values, map } => {
                    if let Some(lv) = &lv {
                        let a0 = (*start - *center).angle();
                        let a1 = (p - *center).angle();
                        let mut deg = -f64::from(a1 - a0).to_degrees();
                        if lv.bake {
                            deg = (deg / 90.0).round() * 90.0;
                        } else if !mods.alt {
                            deg = (deg / 15.0).round() * 15.0;
                        }
                        let tr = Transform { rotate: normalize_deg(values.tr.rotate + deg), ..values.tr };
                        let tr = lv.model.anchored(values, tr, *pivot, map.apply(*pivot));
                        next_layer = Some(LayerValues { tr, ..*values });
                    }
                }
                Drag::LayerScale { handle, anchor, x, y, values, map } => {
                    if let Some(lv) = &lv {
                        let m = dv(map.apply(to_raw(to_uv(p))));
                        let (kx, ky) = scale_factors(m, *handle, *anchor, *x, *y, mods.shift);
                        if kx.abs() > 1e-3 && ky.abs() > 1e-3 {
                            let q = [1.0 / kx, 1.0 / ky];
                            let w = Affine2 {
                                m: [[map.m[0][0] * q[0], map.m[0][1] * q[0]], [map.m[1][0] * q[1], map.m[1][1] * q[1]]],
                                t: [q[0] * (map.t[0] - anchor.x) + anchor.x, q[1] * (map.t[1] - anchor.y) + anchor.y],
                            };
                            if let Some(mut tr) = lv.model.solve(&w, values, values.tr.rotate) {
                                if lv.bake {
                                    for i in 0..2 {
                                        if (tr.scale[i] - values.tr.scale[i]).abs() > 1e-9 {
                                            let s = tr.scale[i].round();
                                            tr.scale[i] = if s == 0.0 { tr.scale[i].signum() } else { s };
                                        }
                                    }
                                } else if !mods.alt {
                                    for i in 0..2 {
                                        let s = (tr.scale[i] * 100.0).round() / 100.0;
                                        if (tr.scale[i] - values.tr.scale[i]).abs() > 1e-9 && s.abs() >= 0.01 {
                                            tr.scale[i] = s;
                                        }
                                    }
                                }
                                if let Some(inv) = map.inverse() {
                                    tr = lv.model.anchored(values, tr, inv.apply(arr(*anchor)), arr(*anchor));
                                }
                                next_layer = Some(LayerValues { tr, ..*values });
                            }
                        }
                    }
                }
                Drag::LayerRotate { start, center, values, map } => {
                    if let (Some(lv), Some(inv)) = (&lv, map.inverse()) {
                        let pivot = inv.apply(arr(*center));
                        let c = to_screen(from_raw(pivot));
                        let a0 = (*start - c).angle();
                        let a1 = (p - c).angle();
                        let mut deg = -f64::from(a1 - a0).to_degrees();
                        if lv.bake {
                            deg = (deg / 90.0).round() * 90.0;
                        } else if !mods.alt {
                            deg = (deg / 15.0).round() * 15.0;
                        }
                        let tr = Transform { rotate: normalize_deg(values.tr.rotate + deg), ..values.tr };
                        let tr = lv.model.anchored(values, tr, pivot, arr(*center));
                        next_layer = Some(LayerValues { tr, ..*values });
                    }
                }
            }
        }
        if let Some(v) = next_layer {
            self.layers.change(&material, layer, v);
        }
        if let (Some((new, op)), Some(d)) = (next, self.drag) {
            let tag = format!("hp_uv_edit:{}", self.serial);
            let label = match op {
                Op::Move(_) => labels::MOVE_FACE,
                Op::Scale(..) => labels::SCALE_FACE,
                Op::Rotate(_) => labels::ROTATE_FACE,
            };
            if let Some(edit) = new.edit(d.dirs) {
                let r = crate::faces::apply_edit_named(&mut state.doc, &[d.face], &edit, label, &tag);
                if let Err(e) = r {
                    state.notify(Severity::Error, tf!("tex.tool.edit_failed", e));
                }
            }
            for o in self.others.clone() {
                if let Some(edit) = o.apply(op).edit(o.dirs)
                    && let Err(e) = crate::faces::apply_edit_named(&mut state.doc, &[o.face], &edit, label, &tag)
                {
                    state.notify(Severity::Error, tf!("tex.tool.edit_failed", e));
                    break;
                }
            }
        }
        if next_verts.is_some() || next_turn.is_some() {
            let tag = format!("hp_uv_edit:{}", self.serial);
            let label = match next_verts {
                Some((_, true)) => labels::MOVE_EDGES,
                Some((_, false)) => labels::MOVE_VERTICES,
                None => labels::TURN_EDGE,
            };
            for vf in &self.vdrag {
                let new = match (next_verts, next_turn) {
                    (Some((t, _)), _) => {
                        crate::uv_select::vertex_affine(&vf.uvs, &vf.moved, t).and_then(|a| vf.frame.transformed(&a))
                    }
                    (None, Some(deg)) => Some(vf.frame.rotated(deg, vf.pivot)),
                    (None, None) => None,
                };
                if let Some(edit) = new.and_then(|n| n.edit(vf.dirs))
                    && let Err(e) = crate::faces::apply_edit_named(&mut state.doc, &[vf.face], &edit, label, &tag)
                {
                    state.notify(Severity::Error, tf!("tex.tool.edit_failed", e));
                    break;
                }
            }
        }
        if self.drag.is_some() && (resp.drag_stopped() || !ui.input(|i| i.pointer.any_down())) {
            if let Some(d) = self.drag.take() {
                match d.kind {
                    Drag::Move { frame: start, .. }
                    | Drag::Scale { frame: start, .. }
                    | Drag::Rotate { frame: start, .. } => {
                        self.end_face_drag(
                            state,
                            d.face,
                            std::iter::once((d.face, start))
                                .chain(self.others.iter().map(|o| (o.face, o.frame)))
                                .collect(),
                        );
                    }
                    Drag::Verts { .. } | Drag::EdgeTurn { .. } => {
                        let before = self.vdrag.iter().map(|v| (v.face, v.frame)).collect();
                        self.end_face_drag(state, d.face, before);
                    }
                    Drag::Box { start, last, op } => {
                        let r = Rect::from_two_pos(start, last);
                        match mode {
                            PickMode::Edge => {
                                crate::uv_select::pick(&mut self.edges, &crate::uv_select::edges_in(&outlines, r), op)
                            }
                            PickMode::Vertex => crate::uv_select::pick(
                                &mut self.verts,
                                &crate::uv_select::vertices_in(&outlines, r),
                                op,
                            ),
                            PickMode::Face => {}
                        }
                    }
                    _ => {}
                }
            }
            self.drag = None;
            self.drag_view = None;
            self.others.clear();
            self.vdrag.clear();
        }

        if editable && !face_mode {
            ui.horizontal_wrapped(|ui| {
                let n = if mode == PickMode::Edge { self.edges.len() } else { self.verts.len() };
                let count =
                    if mode == PickMode::Edge { tf!("tex.uv.sel_edges", n) } else { tf!("tex.uv.sel_vertices", n) };
                ui.label(egui::RichText::new(count).small());
                if n > 0 && ui.small_button(t!("tex.uv.clear_sel")).on_hover_text(t!("tex.uv.clear_sel_tip")).clicked()
                {
                    self.verts.clear();
                    self.edges.clear();
                }
                ui.label(egui::RichText::new(t!("tex.uv.one_projection")).small().weak().underline())
                    .on_hover_text(t!("tex.uv.one_projection_tip"));
            });
            match (mode, main_edge) {
                (PickMode::Edge, Some(i)) => {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(t!("tex.uv.align_x")).on_hover_text(t!("tex.uv.align_key")).clicked() {
                            op = Some(PickOp::Edge(EdgeOp::Align(EdgeAxis::X)));
                        }
                        if ui.button(t!("tex.uv.align_y")).on_hover_text(t!("tex.uv.align_key")).clicked() {
                            op = Some(PickOp::Edge(EdgeOp::Align(EdgeAxis::Y)));
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let n = uvs.len();
                        let now = match &lv {
                            Some(lv) => {
                                let (q0, q1) = (lv.map.apply(to_raw(uvs[i])), lv.map.apply(to_raw(uvs[(i + 1) % n])));
                                crate::uv_select::edge_angle(dv(q0), dv(q1))
                            }
                            None => crate::uv_select::edge_angle(uvs[i], uvs[(i + 1) % n]),
                        };
                        ui.label(t!("tex.uv.edge_angle")).on_hover_text(t!("tex.uv.edge_angle_tip"));
                        let mut a = now.unwrap_or(0.0);
                        let r = ui
                            .add_enabled(
                                now.is_some(),
                                egui::DragValue::new(&mut a)
                                    .speed(0.5)
                                    .range(-180.0..=180.0)
                                    .max_decimals(2)
                                    .suffix("°"),
                            )
                            .on_hover_text(t!("tex.uv.edge_angle_tip"));
                        if r.changed() && now.is_some_and(|now| (normalize_deg(a - now)).abs() > 1e-9) {
                            same_step = r.dragged() && !r.drag_started();
                            op = Some(PickOp::Edge(EdgeOp::Angle(a)));
                        }
                        for (text, deg) in [("⟲ 15", -15.0), ("⟳ 15", 15.0), ("⟲ 90", -90.0), ("⟳ 90", 90.0)] {
                            if ui.small_button(text).on_hover_text(t!("tex.uv.turn_edge_tip")).clicked() {
                                op = Some(PickOp::Edge(EdgeOp::Turn(deg)));
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let axis = {
                            let d = pts[(i + 1) % pts.len()] - pts[i];
                            EdgeAxis::nearest(DVec2::new(f64::from(d.x), f64::from(d.y)))
                        };
                        if ui.button(t!("tex.uv.fit_edge")).on_hover_text(t!("tex.uv.fit_edge_tip")).clicked() {
                            op = Some(PickOp::Edge(EdgeOp::Fit(axis, self.fit_tiles)));
                        }
                        ui.add(egui::DragValue::new(&mut self.fit_tiles).range(0.25..=64.0).speed(0.05).suffix(" x"));
                    });
                    ui.label(egui::RichText::new(t!("tex.uv.edge_help")).small().weak());
                }
                (PickMode::Edge, None) => {
                    ui.label(egui::RichText::new(t!("tex.uv.edge_help")).small().weak());
                }
                (PickMode::Vertex, _) => {
                    if let Some(&(f, i)) = self.verts.last()
                        && ui.button(t!("tex.uv.origin")).on_hover_text(t!("tex.uv.origin_tip")).clicked()
                    {
                        op = Some(PickOp::Origin(f, i));
                    }
                    ui.label(egui::RichText::new(t!("tex.uv.vertex_help")).small().weak());
                }
                (PickMode::Face, _) => {}
            }
        }
        if let Some(op) = op {
            if !same_step {
                self.serial += 1;
            }
            let down = ui.input(|i| i.pointer.any_down());
            let ctx = OpCtx {
                faces,
                face,
                all,
                lv: lv.as_ref(),
                material: &material,
                uvs: &uvs,
                tex,
                outlines: &outlines,
                down,
            };
            self.pick_op(state, &ctx, op);
        }

        match &lv {
            Some(lv) => self.layer_quick_ops(ui, &material, lv, tile, lo, hi, to_raw),
            None if layer == Layer::Base => {
                self.base_quick_ops(ui, state, faces, face, all, tex);
                if available.len() > 1 {
                    ui.checkbox(&mut self.layers.keep_layers, t!("tex.uvl.keep_layers"))
                        .on_hover_text(t!("tex.uvl.keep_layers_tip"));
                }
            }
            None => {}
        }
        match &lv {
            Some(lv) => {
                let tr = &lv.values.tr;
                ui.label(
                    egui::RichText::new(tf!(
                        "tex.uv.values",
                        format!("{:.3}", tr.scale[0]),
                        format!("{:.3}", tr.scale[1]),
                        format!("{:.3}", tr.translate[0]),
                        format!("{:.3}", tr.translate[1]),
                        format!("{:.1}", tr.rotate)
                    ))
                    .small()
                    .weak(),
                );
                let help = match (lv.editable, lv.bake) {
                    (true, false) => t!("tex.uvl.help"),
                    (true, true) => t!("tex.uvl.help_bake"),
                    (false, _) => t!("tex.uvl.help_not_drawn"),
                };
                ui.label(egui::RichText::new(help).small().weak());
            }
            None => {
                ui.label(
                    egui::RichText::new(tf!(
                        "tex.uv.values",
                        format!("{:.3}", side.uaxis.scale),
                        format!("{:.3}", side.vaxis.scale),
                        format!("{:.1}", side.uaxis.shift),
                        format!("{:.1}", side.vaxis.shift),
                        format!("{:.1}", side.rotation)
                    ))
                    .small()
                    .weak(),
                );
                ui.label(egui::RichText::new(t!("tex.uv.help")).small().weak());
            }
        }
        if let Some((model, _)) = &layer_model {
            ui.separator();
            let targets: Vec<FaceRef> = if all { faces.to_vec() } else { vec![face] };
            self.layers.panel(ui, state, &material, model, &targets);
        }
        let down = ui.input(|i| i.pointer.any_down());
        self.layers.settle(down);
    }

    pub(crate) fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        let v = value.to_ascii_lowercase();
        match key {
            "uv_vertices" | "uv_edges" => {
                let selection = state.doc.selection();
                let face = selection.faces.iter().nth(self.active).or_else(|| selection.faces.iter().next()).copied();
                let Some(face) = face else { return false };
                let n = state.doc.face_polygon(face).map_or(0, <[DVec3]>::len);
                let mut list: Vec<Elem> = Vec::new();
                for w in v.split(',').map(str::trim).filter(|w| !w.is_empty() && *w != "none") {
                    match w.parse::<usize>() {
                        Ok(i) if i < n => list.push((face, i)),
                        _ => return false,
                    }
                }
                if key == "uv_vertices" {
                    self.mode = PickMode::Vertex;
                    self.verts = list;
                } else {
                    self.mode = PickMode::Edge;
                    self.edges = list;
                }
                true
            }
            "uv_mode" => {
                self.mode = match v.as_str() {
                    "face" => PickMode::Face,
                    "edge" => PickMode::Edge,
                    "vertex" => PickMode::Vertex,
                    _ => return false,
                };
                true
            }
            "uv_layer" => match Layer::from_mcp_name(&v) {
                Some(l) => {
                    self.layer = l;
                    true
                }
                None => false,
            },
            "uv_keep_layers" => match v.as_str() {
                "on" | "true" | "1" => {
                    self.layers.keep_layers = true;
                    true
                }
                "off" | "false" | "0" => {
                    self.layers.keep_layers = false;
                    true
                }
                _ => false,
            },
            "uv_fit_tiles" => match v.parse::<f64>() {
                Ok(n) if n.is_finite() && (0.25..=64.0).contains(&n) => {
                    self.fit_tiles = n;
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    pub(crate) fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        use hp_app::tool::ToolOption;
        let mode = match self.mode {
            PickMode::Face => "face",
            PickMode::Edge => "edge",
            PickMode::Vertex => "vertex",
        };
        let list = |sel: &[Elem]| {
            let words: Vec<String> =
                sel.iter().filter(|(f, _)| Some(*f) == self.shown_face).map(|(_, i)| i.to_string()).collect();
            words.join(",")
        };
        vec![
            ToolOption::new("uv_mode", mode, "UV editor pick mode: face, edge or vertex"),
            ToolOption::new(
                "uv_layer",
                self.layer.mcp_name(),
                "UV editor layer: base, base2, bump, bump2, detail, envmask or blendmod (if the material has it)",
            ),
            ToolOption::new(
                "uv_keep_layers",
                if self.layers.keep_layers { "on" } else { "off" },
                "moving the base texture keeps the layers moved in Hammer Pro in place on the face (on/off)",
            ),
            ToolOption::new(
                "uv_fit_tiles",
                format!("{}", self.fit_tiles),
                "tiles along the edge for Fit along the edge (0.25 to 64)",
            ),
            ToolOption::new(
                "uv_vertices",
                list(&self.verts),
                "selected vertices of the edited face in the Vertex mode, indices (get_faces order) separated by commas, empty for none",
            ),
            ToolOption::new(
                "uv_edges",
                list(&self.edges),
                "selected edges of the edited face in the Edge mode, index of the first vertex of each, separated by commas (the last is the main edge)",
            ),
        ]
    }

    fn selection_masks(
        &self,
        state: &AppState,
        outlines: &[(FaceRef, &[Pos2])],
        faces: &[FaceRef],
        shared: bool,
    ) -> Vec<(FaceRef, Vec<bool>)> {
        let explicit: Vec<(FaceRef, Vec<bool>)> = outlines
            .iter()
            .map(|(f, o)| {
                let m = match self.mode {
                    PickMode::Edge => crate::uv_select::moved_mask(o.len(), *f, &[], &self.edges),
                    PickMode::Vertex => crate::uv_select::moved_mask(o.len(), *f, &self.verts, &[]),
                    PickMode::Face => vec![false; o.len()],
                };
                (*f, m)
            })
            .collect();
        if shared {
            crate::uv_ops::shared_masks(state, &explicit, faces)
        } else {
            explicit.into_iter().filter(|(_, m)| m.iter().any(|x| *x)).collect()
        }
    }

    fn end_face_drag(&mut self, state: &mut AppState, face: FaceRef, before: Vec<(FaceRef, UvFrame)>) {
        if let (Some(v), Some(o)) = (self.drag_view, Other::of(state, face)) {
            self.center = v - (o.lo + o.hi) * 0.5;
        }
        let tag = format!("hp_uv_edit:{}", self.serial);
        for (f, _) in &before {
            wrap_shifts(state, *f, &tag);
        }
        keep_layers(&mut self.layers, state, &before, &tag);
    }

    fn pick_op(&mut self, state: &mut AppState, ctx: &OpCtx<'_>, op: PickOp) {
        let OpCtx { faces, face, all, lv, material, uvs, tex, outlines, down } = *ctx;
        let n = uvs.len();
        let raw = |u: DVec2| [u.x / tex.x, u.y / tex.y];
        if let Some(lv) = lv {
            if !lv.editable || n == 0 {
                return;
            }
            let v = lv.values;
            let main = crate::uv_select::main_of(&self.edges, face).filter(|i| *i < n);
            let tr = match op {
                PickOp::Edge(e) => main
                    .and_then(|i| crate::uv_ops::layer_edge_op(&lv.model, &v, raw(uvs[i]), raw(uvs[(i + 1) % n]), e)),
                PickOp::Origin(f, i) if f == face && i < n => {
                    Some(crate::uv_ops::layer_origin(&lv.model, &v, raw(uvs[i])))
                }
                PickOp::Nudge(d) => {
                    let mask = match self.mode {
                        PickMode::Edge => crate::uv_select::moved_mask(n, face, &[], &self.edges),
                        PickMode::Vertex => crate::uv_select::moved_mask(n, face, &self.verts, &[]),
                        PickMode::Face => vec![true; n],
                    };
                    let mask = if mask.iter().any(|x| *x) { mask } else { vec![true; n] };
                    let tr = crate::uv_select::vertex_affine(uvs, &mask, d)
                        .and_then(|a| crate::uv_select::layer_map_moved(&lv.map, &a, tex))
                        .and_then(|w| crate::uv_ops::layer_fit(&lv.model, &v, &w));
                    match (tr, &lv.texture) {
                        (Some(tr), Some(t)) if lv.bake => Some(lv.model.snap_translate(&v, v.tr.translate, tr, t.size)),
                        (tr, _) => tr,
                    }
                }
                _ => None,
            };
            if let Some(tr) = tr.filter(|tr| *tr != v.tr) {
                self.layers.change(material, lv.model.layer, LayerValues { tr, ..v });
                self.layers.settle(down);
            }
            return;
        }
        if self.layer != Layer::Base {
            return;
        }
        let tag = format!("hp_uv_edit:{}", self.serial);
        let rest: Vec<FaceRef> = if all { faces.to_vec() } else { Vec::new() };
        let mapping = |state: &AppState, f: FaceRef| state.doc.face_side(f).and_then(UvFrame::of);
        let mut before: Vec<(FaceRef, UvFrame)> = Vec::new();
        for f in faces.iter().chain(std::iter::once(&face)) {
            if !before.iter().any(|(g, _)| g == f)
                && let Some(m) = mapping(state, *f)
            {
                before.push((*f, m));
            }
        }
        let r = match op {
            PickOp::Edge(e) => {
                let picks = edge_picks(&self.edges, outlines);
                crate::uv_ops::edges_op(state, &picks, &rest, e, &tag)
            }
            PickOp::Origin(f, i) => crate::uv_ops::vertex_origin(state, f, i, &rest, &tag),
            PickOp::Nudge(d) => {
                let masks = self.selection_masks(state, outlines, faces, all);
                if masks.is_empty() {
                    let targets: Vec<FaceRef> = if all { faces.to_vec() } else { vec![face] };
                    let mut done = Ok(0);
                    for f in targets {
                        let Some(o) = mapping(state, f) else { continue };
                        if let Err(e) = crate::uv_ops::write_frame(state, f, &o.moved(d), labels::MOVE_FACE, &tag) {
                            done = Err(e);
                            break;
                        }
                    }
                    done
                } else {
                    let label = if self.mode == PickMode::Edge { labels::MOVE_EDGES } else { labels::MOVE_VERTICES };
                    crate::uv_ops::move_masks(state, &masks, d, label, &tag)
                }
            }
        };
        before.retain(|(f, old)| mapping(state, *f).is_some_and(|now| now != *old));
        match r {
            Ok(_) => keep_layers(&mut self.layers, state, &before, &tag),
            Err(e) => state.notify(Severity::Error, tf!("tex.tool.edit_failed", e)),
        }
    }

    fn base_quick_ops(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        faces: &[FaceRef],
        face: FaceRef,
        all: bool,
        tex: DVec2,
    ) {
        ui.horizontal_wrapped(|ui| {
            let half = DVec2::splat(0.5);
            let mut op: Option<Op> = None;
            let mut center_tex = false;
            if ui.small_button(t!("tex.uv.flip_u")).clicked() {
                op = Some(Op::Scale(DVec2::new(-1.0, 1.0), half));
            }
            if ui.small_button(t!("tex.uv.flip_v")).clicked() {
                op = Some(Op::Scale(DVec2::new(1.0, -1.0), half));
            }
            if ui.small_button("⟲ 90").clicked() {
                op = Some(Op::Rotate(-90.0));
            }
            if ui.small_button("⟳ 90").clicked() {
                op = Some(Op::Rotate(90.0));
            }
            if ui.small_button(t!("tex.uv.center")).on_hover_text(t!("tex.uv.center_tip")).clicked() {
                center_tex = true;
            }
            if op.is_none() && !center_tex {
                return;
            }
            self.serial += 1;
            let tag = format!("hp_uv_edit:{}", self.serial);
            let targets: Vec<FaceRef> = if all { faces.to_vec() } else { vec![face] };
            let mut before: Vec<(FaceRef, UvFrame)> = Vec::new();
            for f in targets {
                let Some(o) = Other::of(state, f) else { continue };
                before.push((f, o.frame));
                let new = match op {
                    Some(op) => o.apply(op),
                    None => {
                        let m = crate::util::normalize_material(
                            &state.doc.face_side(f).map(|s| s.material.clone()).unwrap_or_default(),
                        );
                        let size = crate::util::material_sizes(state, [m.as_str()])
                            .get(&m)
                            .copied()
                            .map_or(tex, |(w, h)| DVec2::new(f64::from(w.max(1)), f64::from(h.max(1))));
                        o.frame.moved(size * 0.5 - (o.lo + o.hi) * 0.5)
                    }
                };
                if let Some(edit) = new.edit(o.dirs)
                    && let Err(e) = crate::faces::apply_edit(&mut state.doc, &[f], &edit, Some(&tag))
                {
                    state.notify(Severity::Error, tf!("tex.tool.edit_failed", e));
                    break;
                }
                wrap_shifts(state, f, &tag);
            }
            keep_layers(&mut self.layers, state, &before, &tag);
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn layer_quick_ops(
        &mut self,
        ui: &mut Ui,
        material: &str,
        lv: &LayerView,
        tile: Option<(DVec2, DVec2)>,
        lo: DVec2,
        hi: DVec2,
        to_raw: impl Fn(DVec2) -> [f64; 2],
    ) {
        if !lv.editable {
            return;
        }
        let v = lv.values;
        let model = &lv.model;
        let pivot = tile.map(|(t0, t1)| arr((t0 + t1) * 0.5));
        let keep = |tr: Transform| match pivot {
            Some(q) => model.anchored(&v, tr, lv.inv.apply(q), q),
            None => tr,
        };
        let mut new: Option<Transform> = None;
        ui.horizontal_wrapped(|ui| {
            if ui.small_button(t!("tex.uv.flip_u")).clicked() {
                new = Some(keep(Transform { scale: [-v.tr.scale[0], v.tr.scale[1]], ..v.tr }));
            }
            if ui.small_button(t!("tex.uv.flip_v")).clicked() {
                new = Some(keep(Transform { scale: [v.tr.scale[0], -v.tr.scale[1]], ..v.tr }));
            }
            if ui.small_button("⟲ 90").clicked() {
                new = Some(keep(Transform { rotate: normalize_deg(v.tr.rotate + 90.0), ..v.tr }));
            }
            if ui.small_button("⟳ 90").clicked() {
                new = Some(keep(Transform { rotate: normalize_deg(v.tr.rotate - 90.0), ..v.tr }));
            }
            if ui.small_button(t!("tex.uv.center")).on_hover_text(t!("tex.uvl.center_tip")).clicked() {
                let fc = to_raw((lo + hi) * 0.5);
                let q = lv.map.apply(fc);
                new = Some(model.anchored(&v, v.tr, fc, [q[0].floor() + 0.5, q[1].floor() + 0.5]));
            }
            if ui.small_button(t!("tex.uvl.fit")).on_hover_text(t!("tex.uvl.fit_tip")).clicked() {
                let (a, b) = (to_raw(lo), to_raw(hi));
                let span = [b[0] - a[0], b[1] - a[1]];
                if span[0].abs() > 1e-9 && span[1].abs() > 1e-9 {
                    let w = Affine2 {
                        m: [[1.0 / span[0], 0.0], [0.0, 1.0 / span[1]]],
                        t: [-a[0] / span[0], -a[1] / span[1]],
                    };
                    new = model.solve(&w, &v, 0.0);
                    if lv.bake
                        && let Some(tr) = new.as_mut()
                    {
                        for s in &mut tr.scale {
                            *s = if s.round() == 0.0 { s.signum() } else { s.round() };
                        }
                        *tr = model.anchored(&v, *tr, a, [0.0, 0.0]);
                    }
                }
            }
            if ui.small_button(t!("tex.uvl.reset")).on_hover_text(t!("tex.uvl.reset_tip")).clicked() {
                new = Some(Transform::default());
            }
        });
        if let Some(mut tr) = new {
            if lv.bake
                && let Some(t) = &lv.texture
            {
                tr = model.snap_translate(&v, v.tr.translate, tr, t.size);
            }
            if tr == v.tr {
                return;
            }
            self.layers.change(material, lv.model.layer, LayerValues { tr, ..v });
        }
    }
}

fn scale_factors(m: DVec2, handle: DVec2, anchor: DVec2, x: bool, y: bool, uniform: bool) -> (f64, f64) {
    let k = |axis: usize, on: bool| {
        let span = handle[axis] - anchor[axis];
        if !on || span.abs() < 1e-9 { 1.0 } else { (m[axis] - anchor[axis]) / span }
    };
    let (mut kx, mut ky) = (k(0, x), k(1, y));
    if uniform {
        let u = if x && y {
            if (kx - 1.0).abs() > (ky - 1.0).abs() { kx } else { ky }
        } else if x {
            kx
        } else {
            ky
        };
        kx = u.copysign(kx);
        ky = u.copysign(ky);
    }
    (kx, ky)
}

pub(crate) fn wrap_shifts(state: &mut AppState, face: FaceRef, tag: &str) {
    let Some(side) = state.doc.face_side(face) else { return };
    let Some(now) = UvFrame::of(side) else { return };
    let material = crate::util::normalize_material(&side.material);
    if !crate::uv_layers::wraps_cleanly(state, &material) {
        return;
    }
    let dirs = (side.uaxis.axis, side.vaxis.axis);
    let Some(&(w, h)) = crate::util::material_sizes(state, [material.as_str()]).get(&material) else { return };
    let wrapped = now.wrapped(DVec2::new(f64::from(w.max(1)), f64::from(h.max(1))));
    if wrapped.shift != now.shift
        && let Some(edit) = wrapped.edit(dirs)
    {
        let _ = crate::faces::apply_edit(&mut state.doc, &[face], &edit, Some(tag));
    }
}

fn hit_test(
    p: Pos2,
    knob: Option<Pos2>,
    handles: &[(Handle, Pos2)],
    mine: &[&[Pos2]],
    others: &[(usize, &[Pos2])],
) -> Hit {
    if knob.is_some_and(|k| k.distance(p) <= 9.0) {
        return Hit::Knob;
    }
    let nearest =
        handles.iter().map(|(h, s)| (h, s.distance(p))).filter(|(_, d)| *d <= 8.0).min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((h, _)) = nearest {
        return Hit::Handle(*h);
    }
    if mine.iter().any(|poly| over(poly, p)) {
        return Hit::Face;
    }
    others.iter().rev().find(|(_, poly)| over(poly, p)).map_or(Hit::Empty, |(i, _)| Hit::Other(*i))
}

#[derive(Clone, Copy)]
struct OpCtx<'a> {
    faces: &'a [FaceRef],
    face: FaceRef,
    all: bool,
    lv: Option<&'a LayerView>,
    material: &'a str,
    uvs: &'a [DVec2],
    tex: DVec2,
    outlines: &'a [(FaceRef, &'a [Pos2])],
    down: bool,
}

impl VertFace {
    fn of(state: &AppState, face: FaceRef, moved: Vec<bool>, edge: Option<(DVec3, DVec3)>) -> Option<VertFace> {
        let side = state.doc.face_side(face)?;
        let frame = UvFrame::of(side)?;
        let poly = state.doc.face_polygon(face)?;
        if edge.is_none() && poly.len() != moved.len() {
            return None;
        }
        let pivot = edge.map_or(DVec2::ZERO, |(a, b)| (frame.uv(a) + frame.uv(b)) * 0.5);
        Some(VertFace {
            face,
            frame,
            dirs: (side.uaxis.axis, side.vaxis.axis),
            uvs: poly.iter().map(|p| frame.uv(*p)).collect(),
            moved,
            pivot,
        })
    }
}

fn edge_picks(edges: &[Elem], outlines: &[(FaceRef, &[Pos2])]) -> Vec<Elem> {
    outlines.iter().filter_map(|(f, _)| crate::uv_select::main_of(edges, *f).map(|i| (*f, i))).collect()
}

fn take_key(ui: &Ui, key: egui::Key, shift: bool) -> bool {
    ui.input_mut(|i| {
        let pos = i.events.iter().position(|e| {
            matches!(e, egui::Event::Key { key: k, pressed: true, modifiers, .. }
                if *k == key && modifiers.shift == shift && !modifiers.ctrl && !modifiers.alt && !modifiers.command && !modifiers.mac_cmd)
        });
        match pos {
            Some(p) => {
                i.events.remove(p);
                true
            }
            None => false,
        }
    })
}

fn centroid_of(pts: &[Pos2]) -> Pos2 {
    let n = pts.len().max(1) as f32;
    (pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / n).to_pos2()
}

fn over(poly: &[Pos2], p: Pos2) -> bool {
    if poly.len() < 2 {
        return false;
    }
    let mut inside = false;
    let mut near = false;
    for (i, a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (b.x - a.x) * (p.y - a.y) / (b.y - a.y) {
            inside = !inside;
        }
        let ab = b - *a;
        let t = if ab.length_sq() > 0.0 { ((p - *a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
        near |= (*a + ab * t).distance(p) <= 5.0;
    }
    inside || near
}

fn handles_of(lo: DVec2, hi: DVec2) -> Vec<Handle> {
    let mid = (lo + hi) * 0.5;
    let mut out = Vec::with_capacity(9);
    for (x, y, at, anchor) in [
        (true, true, lo, hi),
        (true, true, hi, lo),
        (true, true, DVec2::new(hi.x, lo.y), DVec2::new(lo.x, hi.y)),
        (true, true, DVec2::new(lo.x, hi.y), DVec2::new(hi.x, lo.y)),
        (true, false, DVec2::new(lo.x, mid.y), DVec2::new(hi.x, mid.y)),
        (true, false, DVec2::new(hi.x, mid.y), DVec2::new(lo.x, mid.y)),
        (false, true, DVec2::new(mid.x, lo.y), DVec2::new(mid.x, hi.y)),
        (false, true, DVec2::new(mid.x, hi.y), DVec2::new(mid.x, lo.y)),
    ] {
        out.push(Handle::Scale { x, y, at, anchor });
    }
    out.push(Handle::Rotate);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side() -> Side {
        let mut s = Side::default();
        s.uaxis.axis = DVec3::X;
        s.uaxis.scale = 0.25;
        s.uaxis.shift = 8.0;
        s.vaxis.axis = -DVec3::Y;
        s.vaxis.scale = 0.5;
        s.vaxis.shift = -4.0;
        s
    }

    fn apply(side: &Side, f: &UvFrame) -> Side {
        let edit = f.edit((side.uaxis.axis, side.vaxis.axis)).expect("edit");
        let mut out = side.clone();
        out.uaxis.axis = edit.u_axis.unwrap();
        out.vaxis.axis = edit.v_axis.unwrap();
        out.uaxis.scale = edit.u_scale.unwrap();
        out.vaxis.scale = edit.v_scale.unwrap();
        out.uaxis.shift = edit.u_shift.unwrap();
        out.vaxis.shift = edit.v_shift.unwrap();
        out
    }

    fn close(a: DVec2, b: DVec2) -> bool {
        (a - b).length() < 1e-6
    }

    #[test]
    fn moves_scales_and_rotations_map_the_points() {
        let s = side();
        let f = UvFrame::of(&s).expect("frame");
        let p = DVec3::new(32.0, 16.0, 0.0);
        let q = DVec3::new(-8.0, 64.0, 0.0);
        assert!(close(f.uv(p), DVec2::new(32.0 / 0.25 + 8.0, -16.0 / 0.5 - 4.0)));
        let m = f.moved(DVec2::new(3.0, -2.0));
        assert!(close(m.uv(p), f.uv(p) + DVec2::new(3.0, -2.0)));
        let anchor = f.uv(q);
        let sc = f.scaled(DVec2::new(2.0, 0.5), anchor);
        assert!(close(sc.uv(q), anchor));
        let d = f.uv(p) - anchor;
        assert!(close(sc.uv(p), anchor + DVec2::new(d.x * 2.0, d.y * 0.5)));
        let written = UvFrame::of(&apply(&s, &sc)).expect("frame");
        assert!(close(written.uv(p), sc.uv(p)) && close(written.uv(q), sc.uv(q)));
        assert!((apply(&s, &sc).uaxis.scale - 0.125).abs() < 1e-9, "twice as many texels: half the scale");
        let flip = f.scaled(DVec2::new(-1.0, 1.0), anchor);
        let fs = apply(&s, &flip);
        assert_eq!(fs.uaxis.axis, DVec3::X);
        assert!(fs.uaxis.scale < 0.0);
        assert!(close(UvFrame::of(&fs).unwrap().uv(p), flip.uv(p)));
        let c = (f.uv(p) + f.uv(q)) * 0.5;
        let r = f.rotated(90.0, c);
        assert!(close(r.uv(p) - c, DVec2::new(-(f.uv(p) - c).y, (f.uv(p) - c).x)));
        let written = UvFrame::of(&apply(&s, &r)).expect("frame");
        assert!(close(written.uv(p), r.uv(p)) && close(written.uv(q), r.uv(q)));
        let tex = DVec2::new(128.0, 64.0);
        let w = r.wrapped(tex);
        let d = w.uv(p) - r.uv(p);
        assert!((d.x / tex.x).fract().abs() < 1e-9 && (d.y / tex.y).fract().abs() < 1e-9);
        assert!(w.shift.x >= 0.0 && w.shift.x < 128.0);
        let back = r.rotated(90.0, c).rotated(90.0, c).rotated(90.0, c);
        assert!(close(back.uv(p), f.uv(p)));
        assert!(UvFrame::of(&Side { uaxis: hp_vmf::TexAxis { scale: 0.0, ..s.uaxis }, ..s.clone() }).is_none());
    }

    use egui::{Event, PointerButton};
    use hp_app::testing::test_state;

    pub(crate) struct Rig {
        pub ctx: egui::Context,
        pub ed: UvEditor,
        pub faces: Vec<FaceRef>,
        pub time: f64,
        pub mods: egui::Modifiers,
    }

    impl Rig {
        pub(crate) fn new(faces: Vec<FaceRef>) -> Rig {
            Rig {
                ctx: egui::Context::default(),
                ed: UvEditor::default(),
                faces,
                time: 0.0,
                mods: egui::Modifiers::NONE,
            }
        }

        pub(crate) fn frame(&mut self, state: &mut AppState, events: Vec<Event>) {
            self.time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 1000.0))),
                time: Some(self.time),
                events: std::iter::once(Event::ModifiersChanged(self.mods)).chain(events).collect(),
                ..Default::default()
            };
            let (ed, faces) = (&mut self.ed, &self.faces);
            let mut out = self.ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| ed.ui(ui, state, faces));
            });
            out.textures_delta.clear();
        }

        pub(crate) fn settle(&mut self, state: &mut AppState) {
            for _ in 0..3 {
                self.frame(state, Vec::new());
            }
        }

        fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
            Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE }
        }

        fn button_with(&self, pos: Pos2, button: PointerButton, pressed: bool) -> Event {
            Event::PointerButton { pos, button, pressed, modifiers: self.mods }
        }

        pub(crate) fn drag(&mut self, state: &mut AppState, from: Pos2, to: Pos2, button: PointerButton) {
            self.frame(state, vec![Event::PointerMoved(from)]);
            let down = self.button_with(from, button, true);
            self.frame(state, vec![down]);
            for i in 1..=8 {
                let p = from + (to - from) * (i as f32 / 8.0);
                self.frame(state, vec![Event::PointerMoved(p)]);
            }
            let up = self.button_with(to, button, false);
            self.frame(state, vec![up]);
            self.settle(state);
        }

        pub(crate) fn click(&mut self, state: &mut AppState, at: Pos2) {
            self.frame(state, vec![Event::PointerMoved(at)]);
            let down = self.button_with(at, PointerButton::Primary, true);
            self.frame(state, vec![down]);
            let up = self.button_with(at, PointerButton::Primary, false);
            self.frame(state, vec![up]);
            self.settle(state);
        }
    }

    pub(crate) fn centroid(pts: &[Pos2]) -> Pos2 {
        let n = pts.len().max(1) as f32;
        let s = pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2());
        (s / n).to_pos2()
    }

    fn shift(state: &AppState, f: FaceRef) -> DVec2 {
        let s = state.doc.face_side(f).expect("face");
        DVec2::new(s.uaxis.shift, s.vaxis.shift)
    }

    fn wrapped_delta(a: DVec2, b: DVec2) -> DVec2 {
        let w = |d: f64| (d + 256.0).rem_euclid(512.0) - 256.0;
        DVec2::new(w(b.x - a.x), w(b.y - a.y))
    }

    const TOP: FaceRef = FaceRef { solid: 7, side: 0 };
    const SIDE: FaceRef = FaceRef { solid: 7, side: 2 };

    #[test]
    fn dragging_the_face_moves_its_uv_not_the_texture() {
        let mut state = test_state();
        let mut rig = Rig::new(vec![TOP]);
        rig.settle(&mut state);
        let before = shift(&state, TOP);
        let at = centroid(&rig.ed.shown.face);
        let undo = state.doc.history_len().0;
        rig.drag(&mut state, at, at + vec2(40.0, 0.0), PointerButton::Primary);
        let d = wrapped_delta(before, shift(&state, TOP));
        assert!(d.x > 20.0 && d.y.abs() < 1e-9, "the face UV moves with the mouse: {d:?}");
        let now = centroid(&rig.ed.shown.face);
        assert!((now.x - (at.x + 40.0)).abs() < 3.0 && (now.y - at.y).abs() < 3.0, "{at:?} -> {now:?}");
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step per drag");
    }

    #[test]
    fn dragging_the_background_pans_the_view() {
        let mut state = test_state();
        let mut rig = Rig::new(vec![TOP]);
        rig.settle(&mut state);
        let before = shift(&state, TOP);
        let undo = state.doc.history_len().0;
        let face = centroid(&rig.ed.shown.face);
        let rect = rig.ed.shown.rect.expect("canvas");
        let at = rect.min + vec2(6.0, 6.0);
        rig.drag(&mut state, at, at + vec2(30.0, 20.0), PointerButton::Primary);
        assert_eq!(shift(&state, TOP), before, "the background is not the face");
        assert_eq!(state.doc.history_len().0, undo);
        let now = centroid(&rig.ed.shown.face);
        assert!((now - face - vec2(30.0, 20.0)).length() < 2.0, "the view pans with the mouse: {face:?} -> {now:?}");
        rig.drag(&mut state, now, now + vec2(-10.0, 0.0), PointerButton::Secondary);
        assert_eq!(shift(&state, TOP), before);
    }

    #[test]
    fn selected_faces_move_together_or_alone() {
        let mut state = test_state();
        let mut rig = Rig::new(vec![TOP, SIDE]);
        rig.settle(&mut state);
        assert_eq!(rig.ed.shown.others.len(), 1, "the other selected face is drawn");
        let (top0, side0) = (shift(&state, TOP), shift(&state, SIDE));
        let other = centroid(&rig.ed.shown.others[0].1);
        let undo = state.doc.history_len().0;
        rig.drag(&mut state, other, other + vec2(0.0, 30.0), PointerButton::Primary);
        let (dt, ds) = (wrapped_delta(top0, shift(&state, TOP)), wrapped_delta(side0, shift(&state, SIDE)));
        assert!(dt.y > 15.0 && (dt - ds).length() < 1e-9, "{dt:?} {ds:?}");
        assert_eq!(state.doc.history_len().0, undo + 1);
        rig.ed.all_faces = false;
        rig.settle(&mut state);
        let (top1, side1) = (shift(&state, TOP), shift(&state, SIDE));
        let other = centroid(&rig.ed.shown.others[0].1);
        rig.drag(&mut state, other, other + vec2(30.0, 0.0), PointerButton::Primary);
        assert_eq!(shift(&state, TOP), top1, "the edited face stays");
        let ds = wrapped_delta(side1, shift(&state, SIDE));
        assert!(ds.x > 15.0, "{ds:?}");
        assert_eq!(rig.ed.active, 1);
        let dropped = centroid(&rig.ed.shown.face);
        assert!((dropped - (other + vec2(30.0, 0.0))).length() < 3.0, "{other:?} -> {dropped:?}");
        let rect = rig.ed.shown.rect.expect("canvas");
        let c = rect.center();
        let shift_px = centroid(&rig.ed.shown.others[0].1) - c;
        rig.drag(&mut state, c, c - shift_px, PointerButton::Secondary);
        let top = centroid(&rig.ed.shown.others[0].1);
        assert!(rect.contains(top), "{top:?}");
        let undo = state.doc.history_len().0;
        rig.click(&mut state, top);
        assert_eq!(rig.ed.active, 0);
        assert_eq!(state.doc.history_len().0, undo);
        assert!((centroid(&rig.ed.shown.face) - top).length() < 3.0, "the view keeps the face in place");
    }

    #[test]
    fn handles_still_scale_and_rotate() {
        let mut state = test_state();
        let mut rig = Rig::new(vec![TOP]);
        rig.settle(&mut state);
        let face = rig.ed.shown.face.clone();
        let (lo, hi) = face.iter().fold((face[0], face[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let scale0 = state.doc.face_side(TOP).map(|s| s.uaxis.scale).unwrap_or_default();
        let undo = state.doc.history_len().0;
        let edge = pos2(hi.x, (lo.y + hi.y) * 0.5);
        rig.drag(&mut state, edge, edge + vec2(hi.x - lo.x, 0.0), PointerButton::Primary);
        let scale1 = state.doc.face_side(TOP).map(|s| s.uaxis.scale).unwrap_or_default();
        assert!((scale1 - scale0 * 0.5).abs() < 0.02, "{scale0} -> {scale1}");
        assert_eq!(state.doc.history_len().0, undo + 1);
        rig.settle(&mut state);
        let face = rig.ed.shown.face.clone();
        let (lo, hi) = face.iter().fold((face[0], face[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let knob = pos2((lo.x + hi.x) * 0.5, lo.y - 22.0);
        let c = centroid(&face);
        rig.drag(&mut state, knob, pos2(c.x + (c.y - knob.y), c.y), PointerButton::Primary);
        let rot = state.doc.face_side(TOP).map(|s| s.rotation).unwrap_or_default();
        assert!((rot.abs() - 90.0).abs() < 1e-6, "{rot}");
        assert_eq!(state.doc.history_len().0, undo + 2);
    }

    fn layer_tr(rig: &mut Rig, state: &AppState, material: &str, layer: Layer) -> Transform {
        let mat = crate::uv_layers::resolved_material(state, material).expect("material");
        rig.ed.layers.sync(&mat, material, layer).1.tr
    }

    #[test]
    fn layer_drags_move_the_layer_alone() {
        let tex = crate::tests::png(8, 8);
        let brick = b"\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"custom/uvd/walls/brick_c\"\n\t\"$bumpmap\" \"custom/uvd/walls/brick_n\"\n}\n";
        let lamp = b"\"VertexLitGeneric\"\n{\n\t\"$basetexture\" \"custom/uvd/walls/brick_c\"\n\t\"$detail\" \"custom/uvd/walls/brick_c\"\n}\n";
        let files: Vec<(&str, &[u8])> = vec![
            ("walls/brick_c.png", &tex),
            ("walls/brick_n.png", &tex),
            ("walls/brick.vmt", brick),
            ("props/lamp.vmt", lamp),
        ];
        let (mut state, folder) = crate::tests::custom_state("uv_layer_drag", "uvd", &files, false);
        let lamp = "custom/uvd/props/lamp";
        state.doc.apply_material(&[TOP], lamp).ok();
        let mut rig = Rig::new(vec![TOP]);
        rig.ed.layer = Layer::Detail;
        rig.settle(&mut state);
        assert!(!rig.ed.shown.tile.is_empty(), "the detail tile is drawn");
        let (s0, undo) = (shift(&state, TOP), state.doc.history_len().0);
        let t0 = layer_tr(&mut rig, &state, lamp, Layer::Detail);
        let at = centroid(&rig.ed.shown.tile);
        rig.drag(&mut state, at, at + vec2(30.0, 0.0), PointerButton::Primary);
        let t1 = layer_tr(&mut rig, &state, lamp, Layer::Detail);
        assert!(t1.translate != t0.translate, "{t0:?} -> {t1:?}");
        assert_eq!(shift(&state, TOP), s0, "the base texture does not move");
        assert_eq!(state.doc.history_len().0, undo);
        let rect = rig.ed.shown.rect.expect("canvas");
        let corner = rect.min + vec2(6.0, rect.height() - 30.0);
        let face = centroid(&rig.ed.shown.face);
        rig.drag(&mut state, corner, corner + vec2(20.0, -10.0), PointerButton::Primary);
        assert_eq!(layer_tr(&mut rig, &state, lamp, Layer::Detail), t1, "the background is not the layer");
        assert!((centroid(&rig.ed.shown.face) - face - vec2(20.0, -10.0)).length() < 2.0, "the view pans");
        rig.ed.mode = PickMode::Vertex;
        rig.settle(&mut state);
        let v = rig.ed.shown.face[1];
        rig.click(&mut state, v);
        assert_eq!(rig.ed.verts, vec![(TOP, 1)]);
        rig.drag(&mut state, v, v + vec2(25.0, 15.0), PointerButton::Primary);
        let t2 = layer_tr(&mut rig, &state, lamp, Layer::Detail);
        assert!(t2 != t1, "the layer moves: {t1:?} -> {t2:?}");
        assert_eq!(shift(&state, TOP), s0, "the base texture does not move");
        assert_eq!(state.doc.history_len().0, undo);
        rig.ed.mode = PickMode::Edge;
        rig.settle(&mut state);
        let f = rig.ed.shown.face.clone();
        let mid = f[0] + (f[1] - f[0]) * 0.5;
        rig.click(&mut state, mid);
        assert_eq!(rig.ed.edges, vec![(TOP, 0)]);
        let key = Event::Key {
            key: egui::Key::Y,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        rig.frame(&mut state, vec![Event::PointerMoved(mid), key]);
        rig.settle(&mut state);
        let t3 = layer_tr(&mut rig, &state, lamp, Layer::Detail);
        let mat = crate::uv_layers::resolved_material(&state, lamp).expect("material");
        let model = LayerModel::of(&mat, Layer::Detail);
        let w = model.written(&LayerValues { tr: t3, ..model.values });
        let uvs = texels(&state, TOP);
        let (q0, q1) = (w.apply([uvs[0].x / 8.0, uvs[0].y / 8.0]), w.apply([uvs[1].x / 8.0, uvs[1].y / 8.0]));
        assert!((q1[0] - q0[0]).abs() < 1e-6, "the edge on the Y axis of the layer: {q0:?} {q1:?}");
        assert_eq!(shift(&state, TOP), s0);
        rig.ed.mode = PickMode::Face;

        let brick = "custom/uvd/walls/brick";
        state.doc.apply_material(&[TOP], brick).ok();
        rig.ed.layer = Layer::Bump;
        rig.settle(&mut state);
        let (s0, undo) = (shift(&state, TOP), state.doc.history_len().0);
        let b0 = layer_tr(&mut rig, &state, brick, Layer::Bump);
        let at = centroid(&rig.ed.shown.face);
        rig.drag(&mut state, at, at + vec2(0.0, 25.0), PointerButton::Primary);
        let b1 = layer_tr(&mut rig, &state, brick, Layer::Bump);
        assert!(b1.translate[1] != b0.translate[1], "the bump map moves: {b0:?} -> {b1:?}");
        assert_eq!(shift(&state, TOP), s0, "the base texture does not move");
        assert_eq!(state.doc.history_len().0, undo);
        drop(state);
        let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
    }

    #[test]
    fn hits_pick_handles_then_the_face_then_the_others() {
        let square =
            |x: f32, y: f32| vec![pos2(x, y), pos2(x + 100.0, y), pos2(x + 100.0, y + 100.0), pos2(x, y + 100.0)];
        let face = square(0.0, 0.0);
        let other = square(50.0, 50.0);
        let handle = Handle::Scale { x: true, y: true, at: DVec2::ZERO, anchor: DVec2::ONE };
        let handles = [(handle, pos2(100.0, 100.0))];
        let knob = Some(pos2(50.0, -22.0));
        let mine = [face.as_slice()];
        let others = [(3usize, other.as_slice())];
        let hit = |p: Pos2| hit_test(p, knob, &handles, &mine, &others);
        assert_eq!(hit(pos2(50.0, -20.0)), Hit::Knob);
        assert_eq!(hit(pos2(103.0, 98.0)), Hit::Handle(handle), "a handle wins over both faces");
        assert_eq!(hit(pos2(60.0, 60.0)), Hit::Face, "the edited face wins over the others");
        assert_eq!(hit(pos2(103.0, 50.0)), Hit::Face, "a few pixels off the outline still count");
        assert_eq!(hit(pos2(130.0, 130.0)), Hit::Other(3));
        assert_eq!(hit(pos2(300.0, 10.0)), Hit::Empty);
        assert_eq!(hit_test(pos2(5.0, 5.0), None, &[], &[], &[]), Hit::Empty);
    }

    fn layer_at(state: &AppState, face: FaceRef, material: &str, layer: Layer, tr: Transform, tex: f64) -> [f64; 2] {
        let mat = crate::uv_layers::resolved_material(state, material).expect("material");
        let model = LayerModel::of(&mat, layer);
        let side = state.doc.face_side(face).expect("face");
        let p = state.doc.face_polygon(face).expect("polygon")[0];
        let uv = UvFrame::of(side).expect("frame").uv(p);
        model.written(&LayerValues { tr, ..model.values }).apply([uv.x / tex, uv.y / tex])
    }

    #[test]
    fn moved_layers_stay_in_place_when_the_base_moves() {
        use crate::uv_layers::{LayerEdit, LayerTarget, mcp_apply_layer};
        let tex = crate::tests::png(8, 8);
        let blend = b"\"WorldVertexTransition\"\n{\n\t\"$basetexture\" \"custom/uvk/t/a\"\n\t\"$basetexture2\" \"custom/uvk/t/b\"\n\t\"$bumpmap\" \"custom/uvk/t/a\"\n\t\"$bumpmap2\" \"custom/uvk/t/b\"\n}\n";
        let files: Vec<(&str, &[u8])> = vec![("t/a.png", &tex), ("t/b.png", &tex), ("blend.vmt", blend)];
        let (mut state, folder) = crate::tests::custom_state("uv_keep", "uvk", &files, false);
        let blend = "custom/uvk/blend";
        state.doc.apply_material(&[TOP], blend).ok();
        let mut rig = Rig::new(vec![TOP]);
        rig.ed.layer = Layer::Bump2;
        rig.settle(&mut state);
        let t0 = layer_tr(&mut rig, &state, blend, Layer::Bump2);
        let at = centroid(&rig.ed.shown.tile);
        rig.drag(&mut state, at, at + vec2(20.0, 0.0), PointerButton::Primary);
        let t1 = layer_tr(&mut rig, &state, blend, Layer::Bump2);
        assert_ne!(t0, t1);
        let before = layer_at(&state, TOP, blend, Layer::Bump2, t1, 64.0);
        rig.ed.layer = Layer::Base;
        rig.settle(&mut state);
        let (s0, undo) = (shift(&state, TOP), state.doc.history_len().0);
        let at = centroid(&rig.ed.shown.face);
        rig.drag(&mut state, at, at + vec2(40.0, 10.0), PointerButton::Primary);
        assert_ne!(shift(&state, TOP), s0);
        assert_eq!(state.doc.history_len().0, undo + 1);
        let t2 = layer_tr(&mut rig, &state, blend, Layer::Bump2);
        assert_ne!(t1, t2, "the waiting edit gets the opposite move");
        let after = layer_at(&state, TOP, blend, Layer::Bump2, t2, 64.0);
        assert!((before[0] - after[0]).abs() < 1e-9 && (before[1] - after[1]).abs() < 1e-9, "{before:?} {after:?}");
        rig.ed.layers.keep_layers = false;
        let at = centroid(&rig.ed.shown.face);
        rig.drag(&mut state, at, at + vec2(-25.0, 0.0), PointerButton::Primary);
        assert_eq!(layer_tr(&mut rig, &state, blend, Layer::Bump2), t2);
        rig.ed.layers.keep_layers = true;

        let edit = LayerEdit { translate: Some([0.25, 0.0]), ..LayerEdit::default() };
        mcp_apply_layer(&mut state, blend, Layer::Bump, &edit, &LayerTarget::Faces(vec![TOP]), "x").expect("variant");
        let material = |state: &AppState| state.doc.face_side(TOP).map(|s| s.material.clone()).unwrap_or_default();
        let m1 = material(&state);
        assert!(m1.contains("hp_variants"), "{m1}");
        let mat1 = crate::uv_layers::resolved_material(&state, &m1).expect("variant");
        let b1 = LayerModel::of(&mat1, Layer::Bump).values.tr;
        let before = layer_at(&state, TOP, &m1, Layer::Bump, b1, 64.0);
        rig.settle(&mut state);
        let (s1, undo) = (shift(&state, TOP), state.doc.history_len().0);
        let at = centroid(&rig.ed.shown.face);
        rig.drag(&mut state, at, at + vec2(0.0, 30.0), PointerButton::Primary);
        let m2 = material(&state);
        assert!(m2 != m1 && m2.contains("hp_variants"), "{m1} -> {m2}");
        let leaf = m2.rsplit('/').next().unwrap_or_default();
        let text = std::fs::read_to_string(folder.join("hp_variants").join(format!("{leaf}.vmt"))).unwrap_or_default();
        assert!(text.contains("\"include\" \"materials/custom/uvk/blend.vmt\""), "{text}");
        assert!(text.contains("%hpbumpsource"), "{text}");
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step for the move and the variant");
        let mat2 = crate::uv_layers::resolved_material(&state, &m2).expect("variant");
        let b2 = LayerModel::of(&mat2, Layer::Bump).values.tr;
        let after = layer_at(&state, TOP, &m2, Layer::Bump, b2, 64.0);
        assert!((before[0] - after[0]).abs() < 1e-6 && (before[1] - after[1]).abs() < 1e-6, "{before:?} {after:?}");
        assert!(state.doc.undo());
        assert_eq!((material(&state), shift(&state, TOP)), (m1, s1), "the undo gives both back");
        drop(state);
        let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
    }

    fn turn(state: &mut AppState, f: FaceRef, deg: f64) {
        let side = state.doc.face_side(f).expect("face").clone();
        let frame = UvFrame::of(&side).expect("frame").rotated(deg, DVec2::ZERO);
        let edit = frame.edit((side.uaxis.axis, side.vaxis.axis)).expect("edit");
        crate::faces::apply_edit(&mut state.doc, &[f], &edit, None).expect("turned");
    }

    fn edge_dir(state: &AppState, f: FaceRef, i: usize) -> DVec2 {
        let (a, b) = crate::uv_ops::edge_of(state, f, i).expect("edge");
        let frame = UvFrame::of(state.doc.face_side(f).expect("face")).expect("frame");
        frame.uv(b) - frame.uv(a)
    }

    fn tdist(a: DVec2, b: DVec2) -> f64 {
        wrapped_delta(b, a).length()
    }

    fn texels(state: &AppState, f: FaceRef) -> Vec<DVec2> {
        let frame = UvFrame::of(state.doc.face_side(f).expect("face")).expect("frame");
        state.doc.face_polygon(f).expect("polygon").iter().map(|p| frame.uv(*p)).collect()
    }

    fn key(k: egui::Key, modifiers: egui::Modifiers) -> Event {
        Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    fn click_with(rig: &mut Rig, state: &mut AppState, at: Pos2, mods: egui::Modifiers) {
        rig.mods = mods;
        rig.click(state, at);
        rig.mods = egui::Modifiers::NONE;
    }

    #[test]
    fn vertices_are_picked_one_by_one() {
        let mut state = test_state();
        let mut rig = Rig::new(vec![TOP]);
        rig.ed.mode = PickMode::Vertex;
        rig.settle(&mut state);
        let undo = state.doc.history_len().0;
        let before = texels(&state, TOP);
        let f = rig.ed.shown.face.clone();
        rig.click(&mut state, f[2] + vec2(3.0, -2.0));
        assert_eq!(rig.ed.verts, vec![(TOP, 2)], "a click selects the vertex");
        click_with(&mut rig, &mut state, f[0], egui::Modifiers::SHIFT);
        assert_eq!(rig.ed.verts, vec![(TOP, 2), (TOP, 0)], "Shift adds");
        click_with(&mut rig, &mut state, f[2], egui::Modifiers::COMMAND);
        assert_eq!(rig.ed.verts, vec![(TOP, 0)], "Ctrl toggles");
        rig.click(&mut state, centroid(&f));
        assert!(rig.ed.verts.is_empty());
        assert_eq!(state.doc.history_len().0, undo);
        assert_eq!(texels(&state, TOP), before);
        let rect = rig.ed.shown.rect.expect("canvas");
        let left: Vec<usize> = (0..f.len()).filter(|i| f[*i].x < centroid(&f).x).collect();
        assert_eq!(left.len(), 2);
        rig.drag(
            &mut state,
            pos2(rect.min.x + 3.0, rect.min.y + 3.0),
            pos2(centroid(&f).x - 5.0, rect.max.y - 3.0),
            PointerButton::Primary,
        );
        let mut got: Vec<usize> = rig.ed.verts.iter().map(|(_, i)| *i).collect();
        got.sort_unstable();
        assert_eq!(got, left, "the box selects the vertices inside it");
        assert_eq!(state.doc.history_len().0, undo, "a box selects, it moves nothing");
        let at = centroid(&f);
        rig.frame(&mut state, vec![Event::PointerMoved(at), key(egui::Key::Escape, egui::Modifiers::NONE)]);
        rig.settle(&mut state);
        assert!(rig.ed.verts.is_empty());
    }

    #[test]
    fn picks_work_at_any_zoom() {
        let mut state = test_state();
        let mut rig = Rig::new(vec![TOP]);
        rig.ed.mode = PickMode::Vertex;
        rig.settle(&mut state);
        let fitted = rig.ed.zoom;
        let uvs = texels(&state, TOP);
        let (lo, hi) = uv_bounds(&uvs).expect("bounds");
        for k in [0.2, 1.0, 6.0, 40.0] {
            for (i, u) in uvs.iter().enumerate() {
                rig.ed.zoom = fitted * k;
                rig.ed.center = *u - (lo + hi) * 0.5;
                rig.settle(&mut state);
                let q = rig.ed.shown.face[i];
                rig.click(&mut state, q + vec2(-4.0, 5.0));
                assert_eq!(rig.ed.verts, vec![(TOP, i)], "zoom x{k}, vertex {i}");
                rig.click(&mut state, q + vec2(12.0, 0.0));
                assert!(rig.ed.verts.is_empty() || k < 1.0, "zoom x{k}: {:?}", rig.ed.verts);
            }
        }
        rig.ed.mode = PickMode::Edge;
        for k in [0.5, 1.0, 8.0] {
            rig.ed.zoom = fitted * k;
            rig.ed.center = (uvs[1] + uvs[2]) * 0.5 - (lo + hi) * 0.5;
            rig.settle(&mut state);
            let f = rig.ed.shown.face.clone();
            let mid = f[1] + (f[2] - f[1]) * 0.5;
            let out = (mid - centroid(&f)).normalized();
            rig.click(&mut state, mid + out * 5.0);
            assert_eq!(rig.ed.edges, vec![(TOP, 1)], "zoom x{k}");
        }
    }

    #[test]
    fn dragging_vertices_keeps_the_others() {
        let mut state = test_state();
        let mut rig = Rig::new(vec![TOP]);
        rig.ed.mode = PickMode::Vertex;
        rig.settle(&mut state);
        let undo = state.doc.history_len().0;
        let before = texels(&state, TOP);
        let f = rig.ed.shown.face.clone();
        rig.click(&mut state, f[2]);
        rig.mods = egui::Modifiers::ALT;
        let d = vec2(36.0, -20.0);
        rig.drag(&mut state, f[2], f[2] + d, PointerButton::Primary);
        rig.mods = egui::Modifiers::NONE;
        let after = texels(&state, TOP);
        let want = before[2] + DVec2::new(f64::from(d.x), f64::from(d.y)) / rig.ed.zoom;
        assert!(tdist(after[2], want) < 0.05, "{:?} -> {:?}, want {want:?}", before[2], after[2]);
        assert!(tdist(after[0], before[0]) < 1e-6, "the opposite vertex stays");
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step per drag");
        assert_eq!(state.doc.undo_label(), Some(crate::uv_ops::labels::MOVE_VERTICES));
        assert!((rig.ed.shown.face[2] - (f[2] + d)).length() < 2.0);
        assert!(state.doc.undo());
        assert!(texels(&state, TOP).iter().zip(&before).all(|(a, b)| tdist(*a, *b) < 1e-6), "the undo gives it back");
        rig.settle(&mut state);
        let f = rig.ed.shown.face.clone();
        rig.click(&mut state, f[1]);
        click_with(&mut rig, &mut state, f[2], egui::Modifiers::SHIFT);
        rig.mods = egui::Modifiers::ALT;
        rig.drag(&mut state, f[1], f[1] + vec2(25.0, 0.0), PointerButton::Primary);
        rig.mods = egui::Modifiers::NONE;
        let after = texels(&state, TOP);
        let du = 25.0 / rig.ed.zoom;
        for i in 0..4 {
            let want = if i == 1 || i == 2 { before[i] + DVec2::new(du, 0.0) } else { before[i] };
            assert!(tdist(after[i], want) < 0.05, "vertex {i}: {:?} want {want:?}", after[i]);
        }
        assert!(state.doc.undo());
        rig.settle(&mut state);
        rig.ed.verts = vec![(TOP, 3)];
        let f = rig.ed.shown.face.clone();
        rig.drag(&mut state, f[3], f[3] + vec2(13.3, 7.7), PointerButton::Primary);
        let v = texels(&state, TOP)[3];
        let w = |x: f64| x.rem_euclid(512.0);
        assert!((w(v.x) - w(v.x).round()).abs() < 1e-6 && (w(v.y) - w(v.y).round()).abs() < 1e-6, "{v:?}");
        let undo = state.doc.history_len().0;
        let b = texels(&state, TOP);
        let at = centroid(&rig.ed.shown.face);
        rig.frame(&mut state, vec![Event::PointerMoved(at), key(egui::Key::ArrowRight, egui::Modifiers::NONE)]);
        rig.settle(&mut state);
        rig.frame(&mut state, vec![Event::PointerMoved(at), key(egui::Key::ArrowDown, egui::Modifiers::SHIFT)]);
        rig.settle(&mut state);
        let a = texels(&state, TOP);
        assert!(tdist(a[3], b[3] + DVec2::new(1.0, 8.0)) < 1e-6, "{:?} -> {:?}", b[3], a[3]);
        assert!(tdist(a[1], b[1]) < 1e-6, "the farthest vertex stays");
        assert_eq!(state.doc.history_len().0, undo + 2);
        let solids = state.doc.stats();
        rig.frame(&mut state, vec![Event::PointerMoved(at), key(egui::Key::Delete, egui::Modifiers::NONE)]);
        rig.settle(&mut state);
        assert_eq!(state.doc.stats(), solids);
        assert_eq!(state.doc.history_len().0, undo + 2);
        rig.ed.mode = PickMode::Face;
        let b = texels(&state, TOP);
        rig.frame(&mut state, vec![Event::PointerMoved(at), key(egui::Key::ArrowLeft, egui::Modifiers::NONE)]);
        rig.settle(&mut state);
        let a = texels(&state, TOP);
        let w2 =
            |d: DVec2| DVec2::new((d.x + 256.0).rem_euclid(512.0) - 256.0, (d.y + 256.0).rem_euclid(512.0) - 256.0);
        assert!(a.iter().zip(&b).all(|(x, y)| (w2(*x - *y) - DVec2::new(-1.0, 0.0)).length() < 1e-6));
    }

    #[test]
    fn edges_are_picked_turned_and_aligned() {
        let mut state = test_state();
        turn(&mut state, TOP, 25.0);
        let mut rig = Rig::new(vec![TOP]);
        rig.ed.mode = PickMode::Edge;
        rig.settle(&mut state);
        let f = rig.ed.shown.face.clone();
        let mid = |f: &[Pos2], i: usize| f[i] + (f[(i + 1) % f.len()] - f[i]) * 0.5;
        rig.click(&mut state, mid(&f, 0));
        assert_eq!(rig.ed.edges, vec![(TOP, 0)], "a click selects the edge");
        click_with(&mut rig, &mut state, mid(&f, 2), egui::Modifiers::SHIFT);
        assert_eq!(rig.ed.edges, vec![(TOP, 0), (TOP, 2)], "Shift adds; the last is the main edge");
        let undo = state.doc.history_len().0;
        rig.frame(&mut state, vec![Event::PointerMoved(mid(&f, 2)), key(egui::Key::X, egui::Modifiers::NONE)]);
        rig.settle(&mut state);
        let d = edge_dir(&state, TOP, 2);
        assert!(d.y.abs() < 1e-6 && d.x.abs() > 1.0, "{d:?}");
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
        assert_eq!(state.doc.undo_label(), Some(crate::uv_ops::labels::ALIGN_EDGE));
        let f = rig.ed.shown.face.clone();
        rig.frame(&mut state, vec![Event::PointerMoved(mid(&f, 2)), key(egui::Key::Y, egui::Modifiers::SHIFT)]);
        rig.settle(&mut state);
        assert_eq!(state.doc.history_len().0, undo + 1);
        let len = d.length();
        rig.frame(&mut state, vec![Event::PointerMoved(mid(&f, 2)), key(egui::Key::Y, egui::Modifiers::NONE)]);
        rig.settle(&mut state);
        let d = edge_dir(&state, TOP, 2);
        assert!(d.x.abs() < 1e-6 && (d.length() - len).abs() < 1e-6, "{d:?}");
        assert_eq!(state.doc.history_len().0, undo + 2);
        rig.settle(&mut state);
        let f = rig.ed.shown.face.clone();
        let (a, b) = (f[2], f[3]);
        let m = a + (b - a) * 0.5;
        let along = (b - a).normalized();
        let mut normal = vec2(-along.y, along.x);
        if (m - centroid(&f)).dot(normal) < 0.0 {
            normal = -normal;
        }
        let along = if along.x < -1e-3 || (along.x.abs() <= 1e-3 && along.y < 0.0) { -along } else { along };
        rig.click(&mut state, m + normal * 22.0 - along * 18.0);
        let d = edge_dir(&state, TOP, 2);
        assert!(d.y.abs() < 1e-6, "the X button aligns on X: {d:?}");
        assert_eq!(state.doc.history_len().0, undo + 3);
        let undo = undo + 1;
        let knob = rig.ed.shown.edge_knob.expect("knob");
        let f = rig.ed.shown.face.clone();
        let c = mid(&f, 2);
        let a0 = crate::uv_select::edge_angle(DVec2::ZERO, edge_dir(&state, TOP, 2)).expect("angle");
        let m0 = {
            let t = texels(&state, TOP);
            (t[2] + t[3]) * 0.5
        };
        let r = knob - c;
        rig.drag(&mut state, knob, c + vec2(-r.y, r.x), PointerButton::Primary);
        let a1 = crate::uv_select::edge_angle(DVec2::ZERO, edge_dir(&state, TOP, 2)).expect("angle");
        assert!((normalize_deg(a1 - a0).abs() - 90.0).abs() < 1e-6, "{a0} -> {a1}");
        let m1 = {
            let t = texels(&state, TOP);
            (t[2] + t[3]) * 0.5
        };
        let w = |d: DVec2| DVec2::new((d.x + 256.0).rem_euclid(512.0) - 256.0, (d.y + 256.0).rem_euclid(512.0) - 256.0);
        assert!(w(m1 - m0).length() < 1e-6, "the middle of the edge stays: {m0:?} {m1:?}");
        assert_eq!(state.doc.history_len().0, undo + 3);
        assert_eq!(state.doc.undo_label(), Some(crate::uv_ops::labels::TURN_EDGE));
        rig.settle(&mut state);
        let before = texels(&state, TOP);
        let f = rig.ed.shown.face.clone();
        rig.click(&mut state, mid(&f, 2));
        assert_eq!(rig.ed.edges, vec![(TOP, 2)]);
        rig.mods = egui::Modifiers::ALT;
        rig.drag(&mut state, mid(&f, 2), mid(&f, 2) + vec2(0.0, 30.0), PointerButton::Primary);
        rig.mods = egui::Modifiers::NONE;
        let after = texels(&state, TOP);
        let dv = 30.0 / rig.ed.zoom;
        for i in 0..4 {
            let want = if i == 2 || i == 3 { before[i] + DVec2::new(0.0, dv) } else { before[i] };
            assert!(tdist(after[i], want) < 0.05, "vertex {i}: {:?} want {want:?}", after[i]);
        }
        assert_eq!(state.doc.undo_label(), Some(crate::uv_ops::labels::MOVE_EDGES));
        let rect = rig.ed.shown.rect.expect("canvas");
        rig.click(&mut state, rect.min + vec2(5.0, rect.height() - 5.0));
        assert!(rig.ed.edges.is_empty());
        turn(&mut state, TOP, 10.0);
        rig.settle(&mut state);
        let f = rig.ed.shown.face.clone();
        let at = mid(&f, 1);
        rig.time += 1.0;
        rig.frame(&mut state, vec![Event::PointerMoved(at)]);
        for _ in 0..2 {
            rig.frame(&mut state, vec![Rig::button(at, PointerButton::Primary, true)]);
            rig.frame(&mut state, vec![Rig::button(at, PointerButton::Primary, false)]);
        }
        rig.settle(&mut state);
        let d = edge_dir(&state, TOP, 1);
        assert!(d.x.abs() < 1e-6 || d.y.abs() < 1e-6, "{d:?} {:?} {:?} {:?}", rig.ed.edges, state.doc.undo_label(), at);
        rig.ed.mode = PickMode::Vertex;
        rig.settle(&mut state);
        let v = rig.ed.shown.face[2];
        rig.time += 1.0;
        rig.frame(&mut state, vec![Event::PointerMoved(v)]);
        for _ in 0..2 {
            rig.frame(&mut state, vec![Rig::button(v, PointerButton::Primary, true)]);
            rig.frame(&mut state, vec![Rig::button(v, PointerButton::Primary, false)]);
        }
        rig.settle(&mut state);
        let uv = texels(&state, TOP)[2];
        assert!(
            (uv.x / 512.0 - (uv.x / 512.0).round()).abs() < 1e-9
                && (uv.y / 512.0 - (uv.y / 512.0).round()).abs() < 1e-9,
            "{uv:?}"
        );
        assert_eq!(state.doc.undo_label(), Some(crate::uv_ops::labels::ORIGIN));
    }

    #[test]
    fn several_faces_each_with_their_own_selection() {
        let mut state = test_state();
        let mut rig = Rig::new(vec![TOP, SIDE]);
        rig.ed.mode = PickMode::Vertex;
        rig.ed.all_faces = false;
        rig.settle(&mut state);
        rig.ed.zoom *= 0.6;
        rig.settle(&mut state);
        assert_eq!(rig.ed.shown.others.len(), 1);
        let o = rig.ed.shown.others[0].1.clone();
        let f = rig.ed.shown.face.clone();
        let k = (0..o.len())
            .find(|k| {
                rig.ed.shown.rect.is_some_and(|r| r.shrink(10.0).contains(o[*k]))
                    && f.iter().all(|q| q.distance(o[*k]) > 20.0)
            })
            .expect("visible");
        rig.click(&mut state, o[k]);
        assert_eq!(rig.ed.verts, vec![(SIDE, k)]);
        assert_eq!(rig.ed.active, 0);
        let i = (0..f.len()).find(|i| o.iter().all(|q| q.distance(f[*i]) > 20.0)).expect("vertex");
        click_with(&mut rig, &mut state, f[i], egui::Modifiers::SHIFT);
        assert_eq!(rig.ed.verts, vec![(SIDE, k), (TOP, i)]);
        let (t0, s0) = (texels(&state, TOP), texels(&state, SIDE));
        rig.mods = egui::Modifiers::ALT;
        rig.drag(&mut state, f[i], f[i] + vec2(20.0, 0.0), PointerButton::Primary);
        rig.mods = egui::Modifiers::NONE;
        let (t1, s1) = (texels(&state, TOP), texels(&state, SIDE));
        let du = 20.0 / rig.ed.zoom;
        assert!(tdist(t1[i], t0[i] + DVec2::new(du, 0.0)) < 0.05, "{:?} {:?}", t0[i], t1[i]);
        assert!(tdist(s1[k], s0[k] + DVec2::new(du, 0.0)) < 0.05, "{:?} {:?}", s0[k], s1[k]);
        assert_eq!(rig.ed.verts.len(), 2);
    }

    #[test]
    fn handles_scale_about_the_opposite_side() {
        let h = handles_of(DVec2::ZERO, DVec2::new(10.0, 20.0));
        assert_eq!(h.len(), 9);
        assert!(h.contains(&Handle::Scale {
            x: true,
            y: false,
            at: DVec2::new(10.0, 10.0),
            anchor: DVec2::new(0.0, 10.0)
        }));
    }
}
