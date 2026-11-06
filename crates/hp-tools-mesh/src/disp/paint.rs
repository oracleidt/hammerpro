use glam::DVec3;
use hp_editor::{Document, EditResult, FaceRef, Id, ObjRef, PickOptions};
use hp_geom::{Aabb, DispMesh, Falloff, PaintBrush, PaintMode, Ray, ray_disp};
use hp_vmf::DispInfo;
use hp_vmf::multiblend::MultiBlend;

use super::smooth::{SmoothPart, smooth_parts, smooth_values};
use crate::common::tangent_basis;

pub(crate) const UNDO_STROKE: &str = "Paint displacement";
pub(crate) const UNDO_SCULPT: &str = "Sculpt displacement";
pub(crate) const UNDO_ELEVATION: &str = "Displacement elevation";
pub(crate) const ELEVATION_TAG: &str = "hp_tools_mesh.disp_elevation";
pub(crate) const MAX_DT: f32 = 0.1;
pub(crate) const DEFAULT_DT: f32 = 1.0 / 60.0;
pub(crate) const SMOOTH_RATE: f64 = 6.0;
pub(crate) const RING_SEGMENTS: usize = 48;
pub(crate) const UP_FACING_Z: f64 = 0.7;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interaction {
    Select,
    #[default]
    Geometry,
    Alpha,
    Sculpt,
    Tag,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GeoEffect {
    #[default]
    Raise,
    Lower,
    Smooth,
    Set,
    Relax,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaEffect {
    #[default]
    Add,
    Remove,
    Set,
    Smooth,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaintAxis {
    #[default]
    FaceNormal,
    X,
    Y,
    Z,
    Custom,
    SubdivNormal,
    View,
    ViewFlat,
    BrushCenter,
}

pub fn flat_direction(v: DVec3) -> Option<DVec3> {
    let h = DVec3::new(v.x, v.y, 0.0);
    if !h.is_finite() || h.length_squared() < f64::MIN_POSITIVE {
        return None;
    }
    h.try_normalize()
}

pub fn flat_toward_camera(cam: &hp_app::view::Camera3d) -> Option<DVec3> {
    flat_direction(-cam.forward()).or_else(|| {
        let (s, c) = cam.yaw_deg.to_radians().sin_cos();
        flat_direction(DVec3::new(-c, -s, 0.0))
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PaintDir {
    Fixed(DVec3),
    FaceNormal,
    VertexNormal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrushSettings {
    pub geo_effect: GeoEffect,
    pub alpha_effect: AlphaEffect,
    pub radius: f64,
    pub strength: f64,
    pub smooth: f64,
    pub target_height: f64,
    pub falloff: Falloff,
    pub spatial: bool,
    pub axis: PaintAxis,
    pub custom_axis: DVec3,
    pub alpha_value: f64,
    pub alpha_strength: f64,
    pub autosew: bool,
    pub line_of_sight: bool,
    pub blend_channel: Option<usize>,
    pub sculpt: super::sculpt::SculptSettings,
    pub selected_only: bool,
    pub continuous: bool,
    pub tag: super::tags::TagSettings,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            geo_effect: GeoEffect::Raise,
            alpha_effect: AlphaEffect::Add,
            radius: 64.0,
            strength: 64.0,
            smooth: 0.5,
            target_height: 0.0,
            falloff: Falloff::Smooth,
            spatial: false,
            axis: PaintAxis::FaceNormal,
            custom_axis: DVec3::Z,
            alpha_value: 255.0,
            alpha_strength: 255.0,
            autosew: false,
            line_of_sight: false,
            blend_channel: None,
            sculpt: super::sculpt::SculptSettings::default(),
            selected_only: true,
            continuous: true,
            tag: super::tags::TagSettings::default(),
        }
    }
}

impl BrushSettings {
    pub fn axis_vector(&self) -> Option<DVec3> {
        match self.axis {
            PaintAxis::FaceNormal
            | PaintAxis::SubdivNormal
            | PaintAxis::View
            | PaintAxis::ViewFlat
            | PaintAxis::BrushCenter => None,
            PaintAxis::X => Some(DVec3::X),
            PaintAxis::Y => Some(DVec3::Y),
            PaintAxis::Z => Some(DVec3::Z),
            PaintAxis::Custom => Some(self.custom_axis.try_normalize().unwrap_or(DVec3::Z)),
        }
    }

    pub fn resolved(&self, toward_eye: Option<DVec3>, center_normal: Option<DVec3>) -> BrushSettings {
        self.resolved_view(toward_eye, toward_eye.and_then(flat_direction), center_normal)
    }

    pub fn resolved_view(
        &self,
        toward_eye: Option<DVec3>,
        toward_eye_flat: Option<DVec3>,
        center_normal: Option<DVec3>,
    ) -> BrushSettings {
        let mut out = self.clone();
        let source = match out.axis {
            PaintAxis::View => Some(toward_eye),
            PaintAxis::ViewFlat => Some(toward_eye_flat.or_else(|| toward_eye.and_then(flat_direction))),
            PaintAxis::BrushCenter => Some(center_normal),
            _ => None,
        };
        if let Some(dir) = source {
            match dir.and_then(DVec3::try_normalize).filter(|d| d.is_finite()) {
                Some(dir) => {
                    out.axis = PaintAxis::Custom;
                    out.custom_axis = dir;
                }
                None => out.axis = PaintAxis::FaceNormal,
            }
        }
        out
    }

    pub fn sanitize(&mut self) {
        let fix = |v: f64, lo: f64, hi: f64, default: f64| if v.is_finite() { v.clamp(lo, hi) } else { default };
        self.radius = fix(self.radius, 1.0, 8192.0, 64.0);
        self.strength = fix(self.strength, 0.0, 8192.0, 64.0);
        self.smooth = fix(self.smooth, 0.0, 1.0, 0.5);
        self.target_height = fix(self.target_height, -16384.0, 16384.0, 0.0);
        self.alpha_value = fix(self.alpha_value, 0.0, 255.0, 255.0);
        self.alpha_strength = fix(self.alpha_strength, 0.0, 16384.0, 255.0);
        if !self.custom_axis.is_finite() || self.custom_axis.length_squared() < 1e-12 {
            self.custom_axis = DVec3::Z;
        }
        if self.blend_channel.is_some_and(|c| c >= hp_vmf::multiblend::CHANNELS) {
            self.blend_channel = None;
        }
        self.sculpt.sanitize();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LocalOp {
    Move { dir: PaintDir, amount: f64 },
    SetHeight { axis: DVec3, target: f64 },
    Smooth { dir: PaintDir, amount: f64 },
    AlphaSet { target: f64 },
    AlphaSmooth { amount: f64, blend: bool },
    Blend { channel: usize, amount: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalBrush {
    pub center: DVec3,
    pub radius: f64,
    pub falloff: Falloff,
    pub spatial: bool,
    pub op: LocalOp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Sample {
    Stock(PaintBrush),
    Local(LocalBrush),
}

pub(crate) fn build_sample(
    s: &BrushSettings,
    interaction: Interaction,
    center: DVec3,
    invert: bool,
    dt: f32,
) -> Option<Sample> {
    let dt = if dt.is_finite() && dt > 0.0 { dt.min(MAX_DT) } else { DEFAULT_DT };
    let dt = f64::from(dt);
    let stock = |mode: PaintMode, amount: f64| {
        Sample::Stock(PaintBrush { center, radius: s.radius, amount, mode, falloff: s.falloff, spatial: s.spatial })
    };
    let local = |op: LocalOp| {
        Sample::Local(LocalBrush { center, radius: s.radius, falloff: s.falloff, spatial: s.spatial, op })
    };
    match interaction {
        Interaction::Select | Interaction::Sculpt | Interaction::Tag => None,
        Interaction::Geometry => {
            let effect = match (s.geo_effect, invert) {
                (GeoEffect::Raise, true) => GeoEffect::Lower,
                (GeoEffect::Lower, true) => GeoEffect::Raise,
                (e, _) => e,
            };
            let amount = s.strength * dt;
            let dir = match (s.axis, s.axis_vector()) {
                (_, Some(axis)) => PaintDir::Fixed(axis),
                (PaintAxis::SubdivNormal, None) => PaintDir::VertexNormal,
                _ => PaintDir::FaceNormal,
            };
            Some(match (effect, dir) {
                (GeoEffect::Relax, _) => return None,
                (GeoEffect::Smooth, dir) => {
                    local(LocalOp::Smooth { dir, amount: (s.smooth * dt * SMOOTH_RATE).clamp(0.0, 1.0) })
                }
                (GeoEffect::Set, PaintDir::FaceNormal | PaintDir::VertexNormal) => {
                    stock(PaintMode::Set, s.target_height)
                }
                (GeoEffect::Raise, PaintDir::FaceNormal) => stock(PaintMode::Raise, amount),
                (GeoEffect::Lower, PaintDir::FaceNormal) => stock(PaintMode::Lower, amount),
                (GeoEffect::Raise, dir) => local(LocalOp::Move { dir, amount }),
                (GeoEffect::Lower, dir) => local(LocalOp::Move { dir, amount: -amount }),
                (GeoEffect::Set, PaintDir::Fixed(axis)) => local(LocalOp::SetHeight { axis, target: s.target_height }),
            })
        }
        Interaction::Alpha => {
            let effect = match (s.alpha_effect, invert) {
                (AlphaEffect::Add, true) => AlphaEffect::Remove,
                (AlphaEffect::Remove, true) => AlphaEffect::Add,
                (e, _) => e,
            };
            let amount = s.alpha_strength * dt;
            if effect == AlphaEffect::Smooth {
                let amount = (s.smooth * dt * SMOOTH_RATE).clamp(0.0, 1.0);
                return Some(local(LocalOp::AlphaSmooth { amount, blend: s.blend_channel.is_some() }));
            }
            if let Some(channel) = s.blend_channel {
                let amount = (amount / 255.0).min(1.0);
                let amount = if effect == AlphaEffect::Remove { -amount } else { amount };
                return Some(local(LocalOp::Blend { channel, amount }));
            }
            Some(match effect {
                AlphaEffect::Add => stock(PaintMode::AlphaPaint, amount),
                AlphaEffect::Remove => stock(PaintMode::AlphaPaint, -amount),
                AlphaEffect::Set => local(LocalOp::AlphaSet { target: s.alpha_value }),
                AlphaEffect::Smooth => return None,
            })
        }
    }
}

pub(crate) fn paint_disp_local(disp: &mut DispInfo, mesh: &DispMesh, brush: &LocalBrush) -> usize {
    if let LocalOp::Smooth { dir, amount } = brush.op {
        let mut parts = [SmoothPart { mesh, disp, pinned: false }];
        return smooth_parts(&mut parts, brush, dir, amount).first().copied().unwrap_or(0);
    }
    if let LocalOp::AlphaSmooth { amount, blend } = brush.op {
        let mut parts = [SmoothPart { mesh, disp, pinned: false }];
        return smooth_values(&mut parts, brush, amount, blend).first().copied().unwrap_or(0);
    }
    let n = mesh.row_len;
    let count = n * n;
    if count == 0 || mesh.positions.len() != count || mesh.base_positions.len() != count {
        return 0;
    }
    if !brush.radius.is_finite() || brush.radius <= 0.0 || !brush.center.is_finite() {
        return 0;
    }
    if let LocalOp::Blend { channel, amount } = brush.op {
        return paint_blend(disp, mesh, brush, channel, amount);
    }
    let face_normal = mesh.face_normal;
    disp.normals.resize(count, face_normal);
    disp.distances.resize(count, 0.0);
    disp.offsets.resize(count, DVec3::ZERO);
    disp.offset_normals.resize(count, face_normal);
    disp.alphas.resize(count, 0.0);
    let mut changed = 0usize;
    for k in 0..count {
        let (Some(&pos), Some(&base)) = (mesh.positions.get(k), mesh.base_positions.get(k)) else { continue };
        let dist_to_brush = if brush.spatial {
            pos.distance(brush.center)
        } else {
            let d = base - brush.center;
            (d - face_normal * d.dot(face_normal)).length()
        };
        let w = brush.falloff.weight(dist_to_brush / brush.radius);
        if w <= 0.0 {
            continue;
        }
        match brush.op {
            LocalOp::Smooth { .. } | LocalOp::AlphaSmooth { .. } | LocalOp::Blend { .. } => {}
            LocalOp::AlphaSet { target } => {
                let target = if target.is_finite() { target.clamp(0.0, 255.0) } else { continue };
                if let Some(a) = disp.alphas.get_mut(k) {
                    let cur = if a.is_finite() { *a } else { 0.0 };
                    let new_a = (cur + (target - cur) * w).clamp(0.0, 255.0);
                    if (new_a - *a).abs() > 1e-9 {
                        *a = new_a;
                        changed += 1;
                    }
                }
            }
            LocalOp::Move { .. } | LocalOp::SetHeight { .. } => {
                let (axis, amount) = match brush.op {
                    LocalOp::Move { dir: PaintDir::Fixed(axis), amount }
                    | LocalOp::SetHeight { axis, target: amount } => (axis, amount),
                    LocalOp::Move { dir: PaintDir::VertexNormal, amount } => {
                        (mesh.normals.get(k).copied().unwrap_or(face_normal), amount)
                    }
                    LocalOp::Move { amount, .. } => (face_normal, amount),
                    _ => continue,
                };
                if !axis.is_finite() || !amount.is_finite() {
                    continue;
                }
                let (Some(normal), Some(dist)) = (disp.normals.get_mut(k), disp.distances.get_mut(k)) else {
                    continue;
                };
                let vec = *normal * *dist;
                let vec = if vec.is_finite() { vec } else { DVec3::ZERO };
                let new_vec = match brush.op {
                    LocalOp::SetHeight { .. } => vec + axis * ((amount - vec.dot(axis)) * w),
                    _ => vec + axis * (amount * w),
                };
                if (new_vec - vec).length_squared() < 1e-18 {
                    continue;
                }
                let len = new_vec.length();
                if len > 1e-9 {
                    *normal = new_vec / len;
                    *dist = len;
                } else {
                    *normal = face_normal;
                    *dist = 0.0;
                }
                changed += 1;
            }
        }
    }
    changed
}

fn brush_distance(mesh: &DispMesh, brush: &LocalBrush, k: usize) -> Option<f64> {
    let (pos, base) = (*mesh.positions.get(k)?, *mesh.base_positions.get(k)?);
    Some(if brush.spatial {
        pos.distance(brush.center)
    } else {
        let d = base - brush.center;
        let n = mesh.face_normal;
        (d - n * d.dot(n)).length()
    })
}

fn paint_blend(disp: &mut DispInfo, mesh: &DispMesh, brush: &LocalBrush, channel: usize, amount: f64) -> usize {
    let count = mesh.row_len * mesh.row_len;
    if !amount.is_finite() || amount == 0.0 {
        return 0;
    }
    let mut blend =
        MultiBlend::of_or_alphas(disp).filter(|b| b.len() == count).unwrap_or_else(|| MultiBlend::new(count));
    let mut changed = 0;
    for k in 0..count {
        let Some(d) = brush_distance(mesh, brush, k) else { continue };
        let w = brush.falloff.weight(d / brush.radius);
        if w > 0.0 && blend.paint(k, channel, amount * w) {
            changed += 1;
        }
    }
    if changed > 0 && !blend.write_to(disp) {
        return 0;
    }
    changed
}

pub(crate) fn blend_colors(disp: &DispInfo) -> Option<Vec<[f32; 4]>> {
    const COLORS: [[f64; 3]; 4] = [[0.85, 0.85, 0.85], [0.95, 0.3, 0.3], [0.3, 0.85, 0.3], [0.3, 0.55, 1.0]];
    let blend = MultiBlend::of(disp)?;
    Some(
        blend
            .weights
            .iter()
            .map(|w| {
                let mut c = [0.0f64; 3];
                for (weight, color) in w.iter().zip(COLORS) {
                    for (slot, v) in c.iter_mut().zip(color) {
                        *slot += weight * v;
                    }
                }
                [c[0] as f32, c[1] as f32, c[2] as f32, 1.0]
            })
            .collect(),
    )
}

fn brush_reaches(mesh: &DispMesh, center: DVec3, radius: f64, spatial: bool) -> bool {
    let reach = Aabb::new(center, center).expand(radius);
    if spatial {
        return mesh.aabb.intersects(&reach);
    }
    let flat = Aabb::from_points(mesh.base_positions.iter().copied());
    let n_axis = mesh.face_normal.abs();
    let mut grown = reach;
    for k in 0..3 {
        if n_axis[k] > 1e-6 {
            grown.min[k] = f64::NEG_INFINITY;
            grown.max[k] = f64::INFINITY;
        }
    }
    flat.intersects(&grown)
}

pub(crate) fn paint_local(
    doc: &mut Document,
    faces: &[FaceRef],
    pinned: &[FaceRef],
    brush: &LocalBrush,
) -> EditResult<usize> {
    let candidates: Vec<FaceRef> = if faces.is_empty() {
        doc.disp_faces().into_iter().filter(|f| doc.is_solid_visible(f.solid)).collect()
    } else {
        faces.to_vec()
    };
    let smoothing = matches!(brush.op, LocalOp::Smooth { .. } | LocalOp::AlphaSmooth { .. });
    let mut work: Vec<(FaceRef, &DispMesh, DispInfo, &[DVec3], bool)> = Vec::new();
    let pins = if smoothing { pinned } else { &[] };
    for (f, pin) in candidates.into_iter().map(|f| (f, false)).chain(pins.iter().map(|f| (*f, true))) {
        if work.iter().any(|w| w.0 == f) {
            continue;
        }
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let Some(mesh) = node.disp_mesh(f.side) else { continue };
        if !pin && !brush_reaches(mesh, brush.center, brush.radius, brush.spatial) {
            continue;
        }
        let Some(disp) = node.solid.sides.get(f.side).and_then(|s| s.disp.as_ref()) else { continue };
        let Some(poly) = node.geom.face_for_side(f.side) else { continue };
        work.push((f, &**mesh, (**disp).clone(), poly.vertices.as_slice(), pin));
    }
    let counts: Vec<usize> = match brush.op {
        LocalOp::Smooth { dir, amount } => {
            let mut parts: Vec<SmoothPart<'_>> =
                work.iter_mut().map(|(_, mesh, disp, _, pinned)| SmoothPart { mesh, disp, pinned: *pinned }).collect();
            smooth_parts(&mut parts, brush, dir, amount)
        }
        LocalOp::AlphaSmooth { amount, blend } => {
            let mut parts: Vec<SmoothPart<'_>> =
                work.iter_mut().map(|(_, mesh, disp, _, pinned)| SmoothPart { mesh, disp, pinned: *pinned }).collect();
            smooth_values(&mut parts, brush, amount, blend)
        }
        _ => work.iter_mut().map(|(_, mesh, disp, _, _)| paint_disp_local(disp, mesh, brush)).collect(),
    };
    let geometry = !matches!(brush.op, LocalOp::AlphaSet { .. } | LocalOp::AlphaSmooth { .. } | LocalOp::Blend { .. });
    let mut updates: Vec<(Id, Vec<(usize, DispInfo)>)> = Vec::new();
    let mut total = 0usize;
    for ((f, _, mut d, poly, pin), changed) in work.into_iter().zip(counts) {
        if changed == 0 || pin {
            continue;
        }
        if geometry {
            hp_geom::update_disp_tags(&mut d, poly);
        }
        total += changed;
        match updates.iter_mut().find(|(id, _)| *id == f.solid) {
            Some((_, list)) => list.push((f.side, d)),
            None => updates.push((f.solid, vec![(f.side, d)])),
        }
    }
    if updates.is_empty() {
        return Ok(0);
    }
    doc.transaction(UNDO_STROKE, |doc| {
        for (id, list) in updates {
            doc.edit_solid(id, |s| {
                for (side, d) in list {
                    if let Some(sd) = s.sides.get_mut(side) {
                        sd.disp = Some(Box::new(d));
                    }
                }
            })?;
        }
        Ok(total)
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CreateTargets {
    pub faces: Vec<FaceRef>,
    pub from_faces: bool,
    pub skipped: usize,
}

pub(crate) fn create_targets(doc: &Document) -> CreateTargets {
    let is_disp = |f: &FaceRef| doc.face_side(*f).is_some_and(|s| s.disp.is_some());
    let selected: Vec<FaceRef> = doc.selection().faces.iter().copied().filter(|f| !is_disp(f)).collect();
    let from_faces = !selected.is_empty();
    let candidates: Vec<FaceRef> = if from_faces {
        selected
    } else {
        let mut out = Vec::new();
        for id in doc.selected_solid_ids() {
            let Some(node) = doc.scene().solid(id) else { continue };
            for f in &node.geom.faces {
                let face = FaceRef::new(id, f.side_index);
                if f.normal().z > UP_FACING_Z && !is_disp(&face) {
                    out.push(face);
                }
            }
        }
        out
    };
    let mut faces = Vec::with_capacity(candidates.len());
    let mut skipped = 0;
    for f in candidates {
        if doc.face_polygon(f).is_some_and(|p| p.len() == 4) {
            faces.push(f);
        } else {
            skipped += 1;
        }
    }
    CreateTargets { faces, from_faces, skipped }
}

pub(crate) fn selected_disp_faces(doc: &Document) -> Vec<FaceRef> {
    doc.selection().faces.iter().copied().filter(|f| doc.face_side(*f).is_some_and(|s| s.disp.is_some())).collect()
}

pub(crate) fn destroy_targets(doc: &Document) -> Vec<FaceRef> {
    let faces = selected_disp_faces(doc);
    if !faces.is_empty() {
        return faces;
    }
    let mut out = Vec::new();
    for id in doc.selected_solid_ids() {
        let Some(node) = doc.scene().solid(id) else { continue };
        for (i, s) in node.solid.sides.iter().enumerate() {
            if s.disp.is_some() {
                out.push(FaceRef::new(id, i));
            }
        }
    }
    out
}

pub(crate) fn nearby_disp_faces(doc: &Document, center: DVec3, radius: f64) -> Vec<FaceRef> {
    if !center.is_finite() || !radius.is_finite() {
        return Vec::new();
    }
    let reach = Aabb::new(center, center).expand(radius * 2.0 + 64.0);
    let mut out = Vec::new();
    for id in doc.solids_in_box(&reach) {
        if !doc.is_solid_visible(id) {
            continue;
        }
        let Some(node) = doc.scene().solid(id) else { continue };
        for (side, mesh) in node.disp.iter().enumerate() {
            if mesh.as_ref().is_some_and(|m| m.aabb.intersects(&reach)) {
                out.push(FaceRef::new(id, side));
            }
        }
    }
    out
}

pub(crate) fn count_disp_faces(doc: &Document) -> usize {
    doc.scene().solids().values().map(|n| n.solid.sides.iter().filter(|s| s.disp.is_some()).count()).sum()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DispHit {
    pub face: FaceRef,
    pub point: DVec3,
    pub face_normal: DVec3,
}

pub(crate) fn pick_disp(doc: &Document, ray: &Ray) -> Option<DispHit> {
    let filter = |o: ObjRef| match o {
        ObjRef::Solid(id) => doc.scene().solid(id).is_some_and(|n| n.has_disp()),
        ObjRef::Entity(_) => false,
    };
    let opts = PickOptions { include_entities: false, filter: Some(&filter), ..PickOptions::default() };
    let no_bounds = |_: &hp_vmf::Entity| -> Option<Aabb> { None };
    let hit = doc.pick_ray(ray, &opts, &no_bounds)?;
    let id = hit.obj.id();
    let node = doc.scene().solid(id)?;
    if let Some(side) = hit.face
        && let Some(mesh) = node.disp_mesh(side)
    {
        return Some(DispHit { face: FaceRef::new(id, side), point: hit.point, face_normal: mesh.face_normal });
    }
    let mut best: Option<(f64, usize, DVec3)> = None;
    for (side, mesh) in node.disp.iter().enumerate() {
        let Some(mesh) = mesh else { continue };
        if let Some(t) = ray_disp(ray, mesh)
            && best.is_none_or(|(bt, _, _)| t < bt)
        {
            best = Some((t, side, mesh.face_normal));
        }
    }
    let (t, side, face_normal) = best?;
    Some(DispHit { face: FaceRef::new(id, side), point: ray.at(t), face_normal })
}

pub(crate) fn surface_ring(doc: &Document, faces: &[FaceRef], center: DVec3, normal: DVec3, radius: f64) -> Vec<DVec3> {
    let normal = normal.try_normalize().unwrap_or(DVec3::Z);
    let (u, v) = tangent_basis(normal);
    let meshes: Vec<&DispMesh> = faces
        .iter()
        .filter_map(|f| doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)))
        .map(|m| &**m)
        .collect();
    let lift = radius * 2.0 + 256.0;
    let mut ring = Vec::with_capacity(RING_SEGMENTS + 1);
    for i in 0..=RING_SEGMENTS {
        let a = i as f64 / RING_SEGMENTS as f64 * std::f64::consts::TAU;
        let q = center + (u * a.cos() + v * a.sin()) * radius;
        let ray = Ray::new(q + normal * lift, -normal);
        let mut best: Option<f64> = None;
        for m in &meshes {
            if let Some(t) = ray_disp(&ray, m)
                && best.is_none_or(|b| (t - lift).abs() < (b - lift).abs())
            {
                best = Some(t);
            }
        }
        ring.push(match best {
            Some(t) => ray.at(t) + normal * 0.5,
            None => q,
        });
    }
    ring
}

pub(crate) fn mesh_border(mesh: &DispMesh, mut f: impl FnMut(DVec3, DVec3)) {
    let n = mesh.row_len;
    if n < 2 || mesh.positions.len() < n * n {
        return;
    }
    let p = |k: usize| mesh.positions.get(k).copied().unwrap_or(DVec3::ZERO);
    for j in 0..n - 1 {
        f(p(j), p(j + 1));
        f(p((n - 1) * n + j), p((n - 1) * n + j + 1));
        f(p(j * n), p((j + 1) * n));
        f(p(j * n + n - 1), p((j + 1) * n + n - 1));
    }
}

pub(crate) fn visible_from(doc: &Document, face: FaceRef, eye: DVec3, center: DVec3) -> bool {
    let Some(mesh) = doc.scene().solid(face.solid).and_then(|n| n.disp_mesh(face.side)) else { return false };
    let Some(target) =
        mesh.positions.iter().copied().min_by(|a, b| a.distance_squared(center).total_cmp(&b.distance_squared(center)))
    else {
        return false;
    };
    let to = target - eye;
    let dist = to.length();
    let Some(dir) = to.try_normalize() else { return true };
    match pick_disp(doc, &Ray::new(eye, dir)) {
        Some(hit) => hit.face == face || hit.point.distance(eye) >= dist - 1.0,
        None => true,
    }
}
