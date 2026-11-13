use std::collections::HashMap;

use glam::{DMat3, DVec2, DVec3};

use super::curve::{CurveError, Frame, GroundFn, Path, frame_from, sag_curve};
use super::model::{CableStyle, FenceStyle, Mode, ModelAxis, Profile, Sides, SplineMeta};

pub const MAX_SOLIDS: usize = 8192;
pub const MAX_ENTITIES: usize = 4096;
const DEFAULT_LENGTH: f64 = 128.0;
const DEFAULT_RADIUS: f64 = 16.0;
const CABLE_STEPS: usize = 8;

pub type Bounds = (DVec3, DVec3);

#[derive(Clone, Debug, PartialEq)]
pub struct UvFrame {
    pub axis: DVec3,
    pub start: DVec3,
    pub s: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SolidPlan {
    pub rings: [Vec<DVec3>; 2],
    pub material: String,
    pub uv: Option<UvFrame>,
}

impl SolidPlan {
    pub fn points(&self) -> Vec<DVec3> {
        self.rings[0].iter().chain(self.rings[1].iter()).copied().collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityRole {
    Segment,
    Post,
    Cap,
    Rope,
    Prop,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityPlan {
    pub class: String,
    pub origin: DVec3,
    pub angles: DVec3,
    pub model: Option<String>,
    pub scale: f64,
    pub role: EntityRole,
    pub bounds: Option<Bounds>,
    pub next: Option<usize>,
    pub keyvalues: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub length: f64,
    pub curve: Vec<DVec3>,
    pub solids: Vec<SolidPlan>,
    pub entities: Vec<EntityPlan>,
    pub wires: Vec<Vec<DVec3>>,
    pub skipped: usize,
    pub notes: Vec<String>,
}

impl Plan {
    pub fn count(&self, role: EntityRole) -> usize {
        self.entities.iter().filter(|e| e.role == role).count()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GenError {
    Curve(CurveError),
    Invalid(String),
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenError::Curve(e) => e.fmt(f),
            GenError::Invalid(s) => write!(f, "{s}"),
        }
    }
}

impl From<CurveError> for GenError {
    fn from(e: CurveError) -> Self {
        GenError::Curve(e)
    }
}

#[derive(Default)]
pub struct GenEnv<'a> {
    pub ground: Option<&'a GroundFn<'a>>,
    pub bounds: HashMap<String, Bounds>,
}

impl GenEnv<'_> {
    fn model_bounds(&self, model: &str) -> Option<Bounds> {
        self.bounds
            .get(model)
            .copied()
            .filter(|(a, b)| a.is_finite() && b.is_finite() && (*b - *a).min_element() >= 0.0)
    }
}

pub fn model_names(meta: &SplineMeta) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |m: &str| {
        if !m.is_empty() && !out.iter().any(|x| x == m) {
            out.push(m.to_owned());
        }
    };
    match meta.mode {
        Mode::Fence if meta.fence == FenceStyle::Model => {
            add(&meta.model);
            add(&meta.post_model);
            add(&meta.start_cap);
            add(&meta.end_cap);
        }
        Mode::Props => {
            for m in &meta.models {
                add(m);
            }
        }
        _ => {}
    }
    out
}

struct Rng(u64);

impl Rng {
    fn new(seed: u32, salt: u64) -> Self {
        Rng(u64::from(seed).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03)
            ^ 0x2545_F491)
    }

    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

fn polygon(sides: u32, rx: f64, rz: f64, cz: f64) -> Vec<DVec2> {
    let n = sides.max(3);
    let step = std::f64::consts::TAU / f64::from(n);
    let start = step * 0.5;
    (0..n)
        .map(|i| {
            let a = start + step * f64::from(i);
            DVec2::new(rx * a.cos(), cz + rz * a.sin())
        })
        .collect()
}

fn rect(x0: f64, z0: f64, x1: f64, z1: f64) -> Vec<DVec2> {
    vec![DVec2::new(x0, z0), DVec2::new(x1, z0), DVec2::new(x1, z1), DVec2::new(x0, z1)]
}

pub fn is_convex(poly: &[DVec2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let c = poly[(i + 2) % n];
        let cross = (b - a).perp_dot(c - b);
        if cross.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

fn area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f64>() * 0.5
}

pub fn profile_parts(meta: &SplineMeta) -> Result<Vec<Vec<DVec2>>, GenError> {
    let (w, h, r) = (meta.size_w, meta.size_h, meta.radius);
    let parts = match meta.profile {
        Profile::Pipe => vec![polygon(meta.sides, r, r, r)],
        Profile::RailTube => vec![polygon(meta.sides, r, r, h.max(r))],
        Profile::Curb => {
            let bevel = (w * 0.25).min(h * 0.5);
            vec![vec![
                DVec2::new(-w * 0.5, 0.0),
                DVec2::new(w * 0.5, 0.0),
                DVec2::new(w * 0.5, h - bevel),
                DVec2::new(w * 0.5 - bevel, h),
                DVec2::new(-w * 0.5, h),
            ]]
        }
        Profile::Trim => {
            let t = (w.min(h) * 0.35).max(0.5);
            vec![rect(-w * 0.5, 0.0, -w * 0.5 + t, h), rect(-w * 0.5 + t, 0.0, w * 0.5, t)]
        }
        Profile::Custom => {
            if meta.custom.len() < 3 {
                return Err(GenError::Invalid("a custom profile needs at least three points".to_owned()));
            }
            let mut poly: Vec<DVec2> = meta.custom.iter().map(|p| *p * (w / 16.0)).collect();
            if !is_convex(&poly) {
                return Err(GenError::Invalid("the custom profile must be convex".to_owned()));
            }
            if area(&poly) < 0.0 {
                poly.reverse();
            }
            vec![poly]
        }
    };
    if parts.iter().any(|p| area(p).abs() < 0.25) {
        return Err(GenError::Invalid("the profile is too small".to_owned()));
    }
    Ok(parts)
}

fn round_profile(profile: Profile) -> bool {
    matches!(profile, Profile::Pipe | Profile::RailTube)
}

fn stations(path: &Path, segment: f64) -> Vec<Frame> {
    let len = path.length();
    let count = ((len / segment).round() as usize).max(1);
    (0..=count).map(|i| path.at(len * i as f64 / count as f64)).collect()
}

fn ring(f: &Frame, part: &[DVec2], ox: f64, oz: f64, scale_z: bool) -> Vec<DVec3> {
    part.iter()
        .map(|p| {
            let z = if scale_z { p.y * f.width } else { p.y };
            f.local(p.x * f.width + ox, z + oz)
        })
        .collect()
}

fn sweep_into(
    out: &mut Plan,
    frames: &[Frame],
    parts: &[Vec<DVec2>],
    material: &str,
    offset: (f64, f64),
    scale_z: bool,
) -> Result<(), GenError> {
    for w in frames.windows(2) {
        for part in parts {
            if out.solids.len() >= MAX_SOLIDS {
                return Err(GenError::Invalid(format!("more than {MAX_SOLIDS} brushes: raise the segment length")));
            }
            let a = ring(&w[0], part, offset.0, offset.1, scale_z);
            let b = ring(&w[1], part, offset.0, offset.1, scale_z);
            let axis = (w[1].pos - w[0].pos).try_normalize().unwrap_or(w[0].tangent);
            if w[0].pos.distance(w[1].pos) < 0.25 {
                continue;
            }
            out.solids.push(SolidPlan {
                rings: [a, b],
                material: material.to_owned(),
                uv: Some(UvFrame { axis, start: w[0].pos, s: w[0].s }),
            });
        }
    }
    Ok(())
}

pub fn model_axis_of(meta_axis: ModelAxis, b: Option<Bounds>) -> ModelAxis {
    match meta_axis {
        ModelAxis::Auto => match b {
            Some((lo, hi)) if (hi.y - lo.y) > (hi.x - lo.x) * 1.05 => ModelAxis::Y,
            _ => ModelAxis::X,
        },
        a => a,
    }
}

fn model_length(b: Option<Bounds>, axis: ModelAxis) -> Option<f64> {
    let (lo, hi) = b?;
    let l = match axis {
        ModelAxis::Y => hi.y - lo.y,
        _ => hi.x - lo.x,
    };
    (l > 1.0).then_some(l)
}

fn basis(dir: DVec3, up_hint: DVec3, axis: ModelAxis) -> DMat3 {
    let d = dir.try_normalize().unwrap_or(DVec3::X);
    let side =
        up_hint.cross(d).try_normalize().unwrap_or_else(|| DVec3::Z.cross(d).try_normalize().unwrap_or(DVec3::Y));
    let up = d.cross(side).normalize();
    match axis {
        ModelAxis::Y => {
            let x = d.cross(up).normalize();
            DMat3::from_cols(x, d, up)
        }
        _ => DMat3::from_cols(d, side, up),
    }
}

fn angles_for(dir: DVec3, up_hint: DVec3, axis: ModelAxis, slope: bool) -> (DVec3, DMat3) {
    let d = if slope { dir } else { DVec3::new(dir.x, dir.y, 0.0) };
    let d = d.try_normalize().unwrap_or(DVec3::X);
    let hint = if slope { up_hint } else { DVec3::Z };
    let m = basis(d, hint, axis);
    (hp_geom::clean_angles(hp_geom::matrix_angles(&m)), m)
}

fn center_along(b: Option<Bounds>, axis: ModelAxis) -> f64 {
    match b {
        Some((lo, hi)) => match axis {
            ModelAxis::Y => (lo.y + hi.y) * 0.5,
            _ => (lo.x + hi.x) * 0.5,
        },
        None => 0.0,
    }
}

fn spacing_for(len: f64, spacing: f64, fit: bool, closed: bool) -> (f64, usize) {
    let spacing = spacing.max(1.0);
    let raw = len / spacing;
    let count =
        if fit { raw.round().max(1.0) as usize } else { raw.floor().max(if closed { 1.0 } else { 0.0 }) as usize };
    let step = if fit { len / count.max(1) as f64 } else { spacing };
    (step, count)
}

fn prop_entity(
    meta: &SplineMeta,
    model: &str,
    origin: DVec3,
    angles: DVec3,
    scale: f64,
    role: EntityRole,
    bounds: Option<Bounds>,
) -> EntityPlan {
    EntityPlan {
        class: meta.prop_class.clone(),
        origin,
        angles,
        model: Some(model.to_owned()),
        scale,
        role,
        bounds,
        next: None,
        keyvalues: Vec::new(),
    }
}

fn fence_models(meta: &SplineMeta, path: &Path, env: &GenEnv<'_>, out: &mut Plan) -> Result<(), GenError> {
    let len = path.length();
    let mb = env.model_bounds(&meta.model);
    let axis = model_axis_of(meta.model_axis, mb);
    let unit = if meta.spacing > 0.0 {
        meta.spacing
    } else {
        model_length(mb, axis).unwrap_or_else(|| {
            out.notes.push(format!("the size of {} is not known yet: spacing {DEFAULT_LENGTH} used", meta.model));
            DEFAULT_LENGTH
        })
    };
    let (step, count) = spacing_for(len, unit, meta.fit_spacing, path.closed);
    if !meta.model.is_empty() && count > MAX_ENTITIES {
        return Err(GenError::Invalid(format!("more than {MAX_ENTITIES} fence segments: raise the spacing")));
    }
    let off = |f: &Frame| f.right * (meta.offset_x * f.width) + f.up * meta.offset_z;
    let mut joints: Vec<f64> = Vec::new();
    for i in 0..count {
        let s0 = step * i as f64;
        let s1 = (s0 + step).min(if path.closed { s0 + step } else { len });
        joints.push(s0);
        if meta.model.is_empty() {
            continue;
        }
        let fa = path.at(s0);
        let fb = path.at(s1);
        let a = fa.pos + off(&fa);
        let b = fb.pos + off(&fb);
        let dir = b - a;
        if dir.length() < 1e-3 {
            continue;
        }
        let mid_frame = path.at((s0 + s1) * 0.5);
        let (angles, m) = angles_for(dir, mid_frame.up, axis, meta.align_slope);
        let fwd = match axis {
            ModelAxis::Y => m.y_axis,
            _ => m.x_axis,
        };
        let origin = (a + b) * 0.5 - fwd * center_along(mb, axis);
        out.entities.push(prop_entity(meta, &meta.model, origin, angles, 1.0, EntityRole::Segment, mb));
    }
    if !path.closed {
        joints.push(len);
    }
    let post_bounds = env.model_bounds(&meta.post_model);
    let mut post_at: Vec<f64> = Vec::new();
    if !meta.post_model.is_empty() {
        if meta.post_every_segment {
            post_at.extend(joints.iter().copied());
        }
        if meta.posts_at_points {
            for s in &path.control_s {
                if post_at.iter().all(|x| (x - s).abs() > 1.0) {
                    post_at.push(*s);
                }
            }
        }
        if post_at.is_empty() {
            out.notes.push("a post model is set but no post position is on".to_owned());
        }
    }
    post_at.sort_by(f64::total_cmp);
    for s in post_at {
        let f = path.at(s);
        let (angles, _) = angles_for(f.tangent, DVec3::Z, ModelAxis::X, false);
        out.entities.push(prop_entity(
            meta,
            &meta.post_model,
            f.pos + off(&f),
            angles,
            1.0,
            EntityRole::Post,
            post_bounds,
        ));
    }
    if !path.closed {
        for (model, s, flip) in [(&meta.start_cap, 0.0, true), (&meta.end_cap, len, false)] {
            if model.is_empty() {
                continue;
            }
            let f = path.at(s);
            let dir = if flip { -f.tangent } else { f.tangent };
            let (angles, _) = angles_for(dir, f.up, ModelAxis::X, meta.align_slope);
            let b = env.model_bounds(model);
            out.entities.push(prop_entity(meta, model, f.pos + off(&f), angles, 1.0, EntityRole::Cap, b));
        }
    }
    if out.entities.is_empty() {
        return Err(GenError::Invalid("set a fence model or a post model".to_owned()));
    }
    Ok(())
}

fn fence_rails(meta: &SplineMeta, path: &Path, out: &mut Plan) -> Result<(), GenError> {
    let len = path.length();
    let (step, count) = spacing_for(len, meta.post_spacing, true, path.closed);
    let posts = if path.closed { count } else { count + 1 };
    let half = meta.post_size * 0.5;
    let base = |f: &Frame| f.pos + f.right * (meta.offset_x * f.width) + DVec3::Z * meta.offset_z;
    for i in 0..posts {
        let f = path.at(step * i as f64);
        let flat = DVec3::new(f.tangent.x, f.tangent.y, 0.0).try_normalize().unwrap_or(DVec3::X);
        let side = DVec3::Z.cross(flat);
        let c = base(&f);
        let foot: Vec<DVec3> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .iter()
            .map(|(a, b)| c + flat * (a * half) + side * (b * half))
            .collect();
        let top: Vec<DVec3> = foot.iter().map(|p| *p + DVec3::Z * meta.post_height).collect();
        out.solids.push(SolidPlan { rings: [foot, top], material: meta.material.clone(), uv: None });
    }
    let frames = stations(path, meta.segment);
    let rs = meta.rail_size * 0.5;
    let mut parts = Vec::new();
    for k in 0..meta.rail_count {
        let frac =
            if meta.rail_count == 1 { 0.85 } else { 0.35 + 0.55 * f64::from(k) / f64::from(meta.rail_count - 1) };
        let z = meta.post_height * frac;
        parts.push(rect(-rs, z - rs, rs, z + rs));
    }
    let flat_frames: Vec<Frame> = frames.iter().map(|f| frame_from(f.pos, f.tangent, 0.0, f.width, f.s)).collect();
    sweep_into(out, &flat_frames, &parts, &meta.material, (meta.offset_x, meta.offset_z), false)
}

fn rope_points(meta: &SplineMeta, path: &Path) -> Vec<DVec3> {
    let lift = |f: &Frame| f.pos + f.right * (meta.offset_x * f.width) + DVec3::Z * meta.offset_z;
    if meta.rope_every > 0.0 {
        let len = path.length();
        let (step, count) = spacing_for(len, meta.rope_every, true, path.closed);
        let n = if path.closed { count } else { count + 1 };
        (0..n).map(|i| lift(&path.at(step * i as f64))).collect()
    } else {
        let mut pts: Vec<DVec3> = path.control_s.iter().map(|s| lift(&path.at(*s))).collect();
        pts.dedup_by(|a, b| a.distance_squared(*b) < 1e-6);
        pts
    }
}

fn cable(meta: &SplineMeta, path: &Path, out: &mut Plan) -> Result<(), GenError> {
    let pts = rope_points(meta, path);
    if pts.len() < 2 {
        return Err(GenError::Invalid("a cable needs at least two points".to_owned()));
    }
    let mut pairs: Vec<(DVec3, DVec3)> = pts.windows(2).map(|w| (w[0], w[1])).collect();
    if path.closed && pts.len() >= 3 {
        pairs.push((pts[pts.len() - 1], pts[0]));
    }
    match meta.cable {
        CableStyle::Rope => {
            if pts.len() > MAX_ENTITIES {
                return Err(GenError::Invalid(format!("more than {MAX_ENTITIES} rope points: raise rope_every")));
            }
            let n = pts.len();
            for (i, p) in pts.iter().enumerate() {
                let next = if i + 1 < n {
                    Some(i + 1)
                } else if path.closed && n >= 3 {
                    Some(0)
                } else {
                    None
                };
                let class = if i == 0 { "move_rope" } else { "keyframe_rope" };
                let mut kv = vec![
                    ("Slack".to_owned(), format_num(meta.slack)),
                    ("Width".to_owned(), format_num(meta.rope_width)),
                    ("RopeMaterial".to_owned(), meta.rope_material.clone()),
                    ("Subdiv".to_owned(), meta.subdiv.to_string()),
                    ("Type".to_owned(), "0".to_owned()),
                    ("TextureScale".to_owned(), "1".to_owned()),
                ];
                if i == 0 {
                    kv.push(("PositionInterpolator".to_owned(), "2".to_owned()));
                }
                out.entities.push(EntityPlan {
                    class: class.to_owned(),
                    origin: *p,
                    angles: DVec3::ZERO,
                    model: None,
                    scale: 1.0,
                    role: EntityRole::Rope,
                    bounds: None,
                    next,
                    keyvalues: kv,
                });
            }
            for (a, b) in &pairs {
                out.wires.push(sag_curve(*a, *b, meta.slack, CABLE_STEPS * 2));
            }
        }
        CableStyle::Brush => {
            let parts = vec![polygon(meta.sides, meta.radius, meta.radius, 0.0)];
            for (a, b) in &pairs {
                let steps = ((a.distance(*b) / meta.segment).ceil() as usize).clamp(2, 64);
                let curve = sag_curve(*a, *b, meta.slack, steps);
                let mut s = 0.0;
                let mut frames = Vec::with_capacity(curve.len());
                for (k, p) in curve.iter().enumerate() {
                    if k > 0 {
                        s += curve[k - 1].distance(*p);
                    }
                    let prev = curve[k.saturating_sub(1)];
                    let next = curve[(k + 1).min(curve.len() - 1)];
                    frames.push(frame_from(*p, next - prev, 0.0, 1.0, s));
                }
                sweep_into(out, &frames, &parts, &meta.material, (0.0, 0.0), true)?;
                out.wires.push(curve);
            }
        }
    }
    Ok(())
}

fn footprint(b: Option<Bounds>, scale: f64) -> f64 {
    match b {
        Some((lo, hi)) => {
            let h = (hi - lo) * 0.5;
            (h.x.max(h.y) * scale).max(1.0)
        }
        None => DEFAULT_RADIUS * scale,
    }
}

fn props(meta: &SplineMeta, path: &Path, env: &GenEnv<'_>, out: &mut Plan) -> Result<(), GenError> {
    if meta.models.is_empty() {
        return Err(GenError::Invalid("give at least one model".to_owned()));
    }
    let len = path.length();
    let unit = if meta.spacing > 0.0 {
        meta.spacing
    } else {
        let b = env.model_bounds(&meta.models[0]);
        (footprint(b, meta.scale_max) * 2.0 + 16.0).max(32.0)
    };
    let (step, count) = spacing_for(len, unit, meta.fit_spacing, path.closed);
    let n = if path.closed { count } else { count + 1 };
    let sides: &[f64] = match meta.sides_mode {
        Sides::Center => &[0.0],
        Sides::Left => &[-1.0],
        Sides::Right => &[1.0],
        Sides::Both => &[-1.0, 1.0],
    };
    let mut kept: Vec<(DVec2, f64)> = Vec::new();
    for i in 0..n {
        for (k, side) in sides.iter().enumerate() {
            if out.entities.len() >= MAX_ENTITIES {
                return Err(GenError::Invalid(format!("more than {MAX_ENTITIES} props: raise the spacing")));
            }
            let mut rng = Rng::new(meta.seed, (i as u64) * 4 + k as u64);
            let along = if meta.jitter > 0.0 { rng.range(-0.5, 0.5) * meta.jitter.min(step * 0.9) } else { 0.0 };
            let s = (step * i as f64 + along).clamp(0.0, len);
            let f = path.at(s);
            let lateral = if *side == 0.0 { meta.offset_x } else { side * meta.offset_x.abs() };
            let jitter = if meta.jitter > 0.0 { rng.range(-1.0, 1.0) * meta.jitter } else { 0.0 };
            let flat_right = DVec3::new(f.right.x, f.right.y, 0.0).try_normalize().unwrap_or(DVec3::NEG_Y);
            let mut origin = f.pos + flat_right * ((lateral + jitter) * f.width) + DVec3::Z * meta.offset_z;
            let pick = (rng.next() * meta.models.len() as f64) as usize;
            let model = &meta.models[pick.min(meta.models.len() - 1)];
            let scale = if meta.scale_max > meta.scale_min {
                rng.range(meta.scale_min, meta.scale_max)
            } else {
                meta.scale_min
            };
            let scale = (scale * 1000.0).round() / 1000.0;
            let mut yaw = if meta.face_path { f.tangent.y.atan2(f.tangent.x).to_degrees() } else { 0.0 };
            if *side < 0.0 && meta.face_path {
                yaw += 180.0;
            }
            if meta.yaw_jitter > 0.0 {
                yaw += rng.range(-meta.yaw_jitter, meta.yaw_jitter);
            }
            let b = env.model_bounds(model);
            if meta.drop
                && let Some(g) = env.ground
                && let Some(z) = g(DVec2::new(origin.x, origin.y), origin.z)
            {
                let below = b.map_or(0.0, |(lo, _)| (-lo.z * scale).max(0.0));
                origin.z = z + below + meta.offset_z;
            }
            let r = footprint(b, scale);
            let xy = DVec2::new(origin.x, origin.y);
            if meta.avoid_overlap && kept.iter().any(|(p, pr)| p.distance(xy) < pr + r - 0.5) {
                out.skipped += 1;
                continue;
            }
            kept.push((xy, r));
            let angles = hp_geom::clean_angles(DVec3::new(0.0, yaw.rem_euclid(360.0), 0.0));
            out.entities.push(prop_entity(meta, model, origin, angles, scale, EntityRole::Prop, b));
        }
    }
    Ok(())
}

pub fn format_num(x: f64) -> String {
    let r = (x * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

pub fn plan(meta: &SplineMeta, env: &GenEnv<'_>) -> Result<Plan, GenError> {
    let meta = meta.clone().clamped();
    let path = Path::build(&meta, env.ground)?;
    let mut out = Plan { length: path.length(), curve: path.polyline(), ..Plan::default() };
    match meta.mode {
        Mode::Path => {}
        Mode::Sweep => {
            let parts = profile_parts(&meta)?;
            let frames = stations(&path, meta.segment);
            sweep_into(
                &mut out,
                &frames,
                &parts,
                &meta.material,
                (meta.offset_x, meta.offset_z),
                round_profile(meta.profile),
            )?;
        }
        Mode::Fence => match meta.fence {
            FenceStyle::Model => fence_models(&meta, &path, env, &mut out)?,
            FenceStyle::Rails => fence_rails(&meta, &path, &mut out)?,
        },
        Mode::Cable => cable(&meta, &path, &mut out)?,
        Mode::Props => props(&meta, &path, env, &mut out)?,
    }
    Ok(out)
}
