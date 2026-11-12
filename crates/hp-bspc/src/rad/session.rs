use std::collections::HashMap;
use std::time::Instant;

use glam::Vec3A;
use hp_light::bvh::Bvh;
use rayon::prelude::*;

use super::bake::incremental::{self, GatherCache, StitchCells};
use super::bake::{self, FaceLight};
use super::data::MapData;
use super::lights::{self, Kind, Light, Lights};
use super::surface::{self, LitFace, SurfTri};
use super::{RadOptions, RadQuality, ambient, detail, disp, output, progress, props, texlights};
use crate::emit::lump;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Scene,
    Direct,
    Bounce,
    Output,
    Models,
}

#[derive(Debug)]
pub enum Stop {
    Cancelled,
    Failed(String),
}

impl From<String> for Stop {
    fn from(message: String) -> Self {
        Stop::Failed(message)
    }
}

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stop::Cancelled => f.write_str("cancelled"),
            Stop::Failed(message) => f.write_str(message),
        }
    }
}

pub struct Control<'a> {
    pub cancelled: &'a (dyn Fn() -> bool + Sync),
    pub progress: &'a (dyn Fn(Phase, f32) + Sync),
    pub partial: Option<&'a (dyn Fn(Vec<FaceOut>) + Sync)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceOut {
    pub face: u32,
    pub width: u32,
    pub height: u32,
    pub normals: u32,
    pub styles: Vec<u8>,
    pub values: Vec<[f32; 3]>,
}

#[derive(Clone, Debug, Default)]
pub struct Relit {
    pub full: bool,
    pub faces: Vec<FaceOut>,
    pub lights: usize,
    pub changed_lights: usize,
    pub relit_faces: usize,
    pub direct_seconds: f64,
    pub bounce_seconds: f64,
    pub seconds: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightCost {
    pub index: usize,
    pub hammerid: Option<u32>,
    pub classname: String,
    pub origin: [f32; 3],
    pub faces: usize,
    pub luxels: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct SceneKey {
    hdr: bool,
    ldr: bool,
    texlights: texlights::TexLightOptions,
    props: props::PropOptions,
    no_disp_shadows: bool,
    smooth_angle: u64,
    no_skybox_recurse: bool,
    bounced: bool,
    luxel_density: u32,
}

impl SceneKey {
    fn of(opts: &RadOptions) -> Self {
        SceneKey {
            hdr: opts.hdr,
            ldr: opts.ldr,
            texlights: opts.texlights.clone(),
            props: opts.props.clone(),
            no_disp_shadows: opts.no_disp_shadows,
            smooth_angle: opts.smooth_angle.to_bits(),
            no_skybox_recurse: opts.no_skybox_recurse,
            bounced: opts.bounces > 0,
            luxel_density: opts.luxel_density.to_bits(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StateKey {
    params: [usize; 7],
    bounces: u32,
    soft_sun: u32,
    bounce_budget: usize,
    style_bounce: bool,
}

impl StateKey {
    fn of(opts: &RadOptions) -> Self {
        let p = params(opts);
        StateKey {
            params: [
                p.sky_dirs,
                p.sun_samples,
                p.supersample_passes,
                p.bounce_step,
                p.bounce_rays,
                p.leaf_samples,
                p.leaf_dirs,
            ],
            bounces: opts.bounces,
            soft_sun: opts.soft_sun.to_bits(),
            bounce_budget: opts.bounce_budget,
            style_bounce: p.style_bounce,
        }
    }
}

fn params(opts: &RadOptions) -> bake::Params {
    let mut params = bake::Params::of(opts.quality);
    if opts.no_extra {
        params.supersample_passes = 0;
    }
    if let Some(n) = opts.extra_sky.filter(|n| n.is_finite() && *n > 0.0) {
        params.sky_dirs = ((162.0 * n).round() as usize).clamp(16, 162 * 32);
    }
    if opts.fast_ambient {
        params.leaf_samples = 1;
    }
    params.sun_samples = params.sun_samples.max(opts.min_sun_samples);
    params.style_bounce = !opts.no_style_bounce;
    if opts.normal_sampling {
        params = params.at_least(&bake::Params::of(RadQuality::Normal));
    }
    params
}

struct Scene {
    src: hp_bsp::Bsp,
    map: MapData,
    faces: Vec<LitFace>,
    shadow: Bvh,
    surf: Bvh,
    surf_info: Vec<SurfTri>,
    blockers: Bvh,
    water: Bvh,
    water_reflectivity: Vec3A,
    static_props: props::StaticProps,
    occluders: props::Occluders,
    sky_clusters: Vec<i32>,
    cameras: Vec<(Vec3A, f32)>,
    camera_areas: Vec<bool>,
    emission: texlights::Emission,
    passes: Vec<bool>,
    key: SceneKey,
    luxels: usize,
}

struct State {
    key: StateKey,
    params: bake::Params,
    entities: Vec<hp_bsp::Entity>,
    styled: bool,
    lights: Lights,
    direct: Vec<FaceLight>,
    total: Vec<FaceLight>,
    deltas: u32,
    touched: Vec<bool>,
    bounce_dirty: bool,
    recent: Option<(Vec<u32>, HashMap<usize, FaceLight>)>,
}

const RECENT_BUDGET: usize = 96 << 20;
const FIRST_CHUNK: usize = 1 << 14;

fn shape_key(l: &Light) -> Vec<u32> {
    light_key(&Light { intensity: Vec::new(), reach: 0.0, ..l.clone() })
}

fn scale_of(old: &Light, new: &Light) -> Option<Vec<Vec3A>> {
    if shape_key(old) != shape_key(new) || old.intensity.len() != new.intensity.len() {
        return None;
    }
    old.intensity
        .iter()
        .zip(&new.intensity)
        .map(|(o, n)| {
            let ok = (0..3).all(|c| o[c] > 0.0 || n[c] == 0.0);
            ok.then(|| Vec3A::select(o.cmpgt(Vec3A::ZERO), *n / o.max(Vec3A::splat(1e-30)), Vec3A::ZERO))
        })
        .collect()
}

fn scaled(l: &FaceLight, ratio: &[Vec3A], lux: usize) -> FaceLight {
    let mut out = l.clone();
    let nn = l.normals.max(1);
    for (p, r) in ratio.iter().enumerate() {
        for slot in 0..l.styles.len() {
            for n in 0..nn {
                let at = l.index(p, slot, n, lux);
                if let Some(values) = out.values.get_mut(at..at + lux) {
                    for v in values {
                        *v *= glam::Vec3::from(*r);
                    }
                }
            }
        }
    }
    out
}

fn contribution_bytes(map: &HashMap<usize, FaceLight>) -> usize {
    map.values().map(|f| f.values.len() * 16 + f.dark.len() + 64).sum()
}

pub struct Session {
    scene: std::sync::Arc<Scene>,
    state: Option<State>,
    gather: Option<GatherCache>,
    stitch: Option<StitchCells>,
    carry: Option<Carry>,
}

pub struct Carry {
    key: StateKey,
    passes: usize,
    lights: Lights,
    faces: Vec<LitFace>,
    direct: Vec<FaceLight>,
}

const CARRY_MARGIN: f64 = 24.0;
const SKY_REACH: f64 = 4096.0;

fn face_signature(f: &LitFace) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let q = |v: f64| (v * 64.0).round() as i64;
    (f.w, f.h, f.mins, f.bump, f.disp.is_some(), f.vnormals.is_some()).hash(&mut h);
    for p in &f.poly {
        (q(p.x), q(p.y), q(p.z)).hash(&mut h);
    }
    for row in &f.lm {
        for v in row {
            q(*v * 1024.0).hash(&mut h);
        }
    }
    (q(f.normal.x * 1024.0), q(f.normal.y * 1024.0), q(f.normal.z * 1024.0)).hash(&mut h);
    (f.reflectivity.to_array().map(f32::to_bits), f.minlight.to_bits()).hash(&mut h);
    h.finish()
}

fn boxes_meet(a: (glam::DVec3, glam::DVec3), b: (glam::DVec3, glam::DVec3)) -> bool {
    a.0.cmple(b.1).all() && b.0.cmple(a.1).all()
}

fn union(a: (glam::DVec3, glam::DVec3), p: glam::DVec3) -> (glam::DVec3, glam::DVec3) {
    (a.0.min(p), a.1.max(p))
}

fn touched_by(l: &Light, f: &LitFace, dirty: &[(glam::DVec3, glam::DVec3)]) -> bool {
    let m = glam::DVec3::splat(CARRY_MARGIN);
    let own = (f.bounds.0 - m, f.bounds.1 + m);
    let reach = match l.kind {
        Kind::Point | Kind::Spot { .. } | Kind::Surface => union(own, l.origin.as_dvec3()),
        Kind::Sun => {
            let away = -l.normal.as_dvec3() * SKY_REACH;
            union(union(own, own.0 + away), own.1 + away)
        }
        Kind::SkyAmbient => {
            let r = glam::DVec3::new(SKY_REACH, SKY_REACH, SKY_REACH);
            (own.0 - glam::DVec3::new(r.x, r.y, 0.0), own.1 + r)
        }
    };
    dirty.iter().any(|d| boxes_meet(reach, *d))
}

type FaceDelta = (usize, Option<FaceLight>, Option<FaceLight>);

const CHANGE_ABS: f32 = 0.05;
const FAINT_BOUNCE: f32 = 0.05;
const CHANGE_REL: f32 = 0.002;

fn make_ctx<'a>(
    scene: &'a Scene,
    lights: &'a Lights,
    params: bake::Params,
    work: &'a progress::Work,
    budget: usize,
) -> bake::Ctx<'a> {
    bake::Ctx {
        map: &scene.map,
        faces: &scene.faces,
        shadow: &scene.shadow,
        surf: &scene.surf,
        surf_info: &scene.surf_info,
        lights,
        passes: scene.passes.len(),
        params,
        light_rows: bake::Ctx::rows(&scene.map, lights, &scene.sky_clusters),
        sky_dirs: bake::sphere_dirs(params.sky_dirs),
        sun_dirs: bake::sphere_dirs(params.sun_samples.max(1)),
        cameras: scene.cameras.clone(),
        camera_areas: scene.camera_areas.clone(),
        props: &scene.occluders,
        blockers: &scene.blockers,
        work,
        bounce_budget: budget,
        bounce_tile_budget: budget / 4,
        water: &scene.water,
        water_reflectivity: scene.water_reflectivity,
    }
}

fn reaches(l: &Light, f: &LitFace) -> bool {
    match l.kind {
        Kind::Point | Kind::Spot { .. } | Kind::Surface => {
            let (lo, hi) = (f.bounds.0.as_vec3a(), f.bounds.1.as_vec3a());
            let closest = l.origin.clamp(lo, hi);
            if closest.distance(l.origin) > l.reach {
                return false;
            }
            if l.kind == Kind::Surface {
                let (a, b) = ((lo - 1.0 - l.origin) * l.normal, (hi + 1.0 - l.origin) * l.normal);
                if a.max(b).element_sum() <= 0.0 {
                    return false;
                }
            }
            let d = f.poly.first().map_or(0.0, |p| p.dot(f.normal));
            f.disp.is_some() || f.vnormals.is_some() || l.origin.as_dvec3().dot(f.normal) - d > 1.0
        }
        Kind::Sun | Kind::SkyAmbient => true,
    }
}

fn light_key(l: &Light) -> Vec<u32> {
    let mut k = Vec::with_capacity(32);
    match l.kind {
        Kind::Point => k.push(0),
        Kind::Spot { stopdot, stopdot2, exponent } => {
            k.extend([1, stopdot.to_bits(), stopdot2.to_bits(), exponent.to_bits()]);
        }
        Kind::Sun => k.push(2),
        Kind::SkyAmbient => k.push(3),
        Kind::Surface => k.push(4),
    }
    k.extend(l.origin.to_array().map(f32::to_bits));
    k.extend(l.normal.to_array().map(f32::to_bits));
    for i in &l.intensity {
        k.extend(i.to_array().map(f32::to_bits));
    }
    k.extend([
        u32::from(l.style),
        l.constant.to_bits(),
        l.linear.to_bits(),
        l.quadratic.to_bits(),
        l.radius.to_bits(),
        l.cap.to_bits(),
        l.fade.0.to_bits(),
        l.fade.1.to_bits(),
        l.cluster as u32,
        l.reach.to_bits(),
        u32::from(l.entity_shadows),
    ]);
    k
}

fn diff_lights(old: &Lights, new: &Lights) -> (Vec<Light>, Vec<Light>) {
    let mut pool: HashMap<Vec<u32>, usize> = HashMap::new();
    for l in &old.list {
        *pool.entry(light_key(l)).or_default() += 1;
    }
    let mut added = Vec::new();
    for l in &new.list {
        match pool.get_mut(&light_key(l)) {
            Some(n) if *n > 0 => *n -= 1,
            _ => added.push(l.clone()),
        }
    }
    let mut removed = Vec::new();
    for l in &old.list {
        if let Some(n) = pool.get_mut(&light_key(l))
            && *n > 0
        {
            *n -= 1;
            removed.push(l.clone());
        }
    }
    if old.sun_extent.to_bits() != new.sun_extent.to_bits() {
        let sun = |l: &&Light| l.kind == Kind::Sun;
        for l in old.list.iter().filter(sun) {
            if !removed.iter().any(|r| r == l) {
                removed.push(l.clone());
            }
        }
        for l in new.list.iter().filter(sun) {
            if !added.iter().any(|a| a == l) {
                added.push(l.clone());
            }
        }
    }
    (removed, added)
}

fn light_owners(entities: &[hp_bsp::Entity]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut sky = false;
    for (i, e) in entities.iter().enumerate() {
        match e.classname() {
            "light" | "light_spot" => out.push(i),
            "light_environment" if !sky => {
                sky = true;
                out.extend([i, i]);
            }
            _ => {}
        }
    }
    out.reverse();
    out
}

fn values_len(f: &FaceLight, passes: usize, lux: usize) -> usize {
    passes * f.styles.len() * f.normals.max(1) * lux
}

fn add_aligned(dst: &mut FaceLight, src: &FaceLight, sign: f32, passes: usize, lux: usize, keep: Option<&[u8]>) {
    if src.values.is_empty() {
        return;
    }
    if dst.normals != src.normals || values_len(dst, passes, lux) != dst.values.len() {
        return;
    }
    let nn = dst.normals.max(1);
    for (sslot, style) in src.styles.iter().enumerate() {
        let dslot = match dst.styles.iter().position(|s| s == style) {
            Some(k) => k,
            None if dst.styles.len() < 4 => {
                let old_slots = dst.styles.len();
                let mut grown = vec![glam::Vec3::ZERO; passes * (old_slots + 1) * nn * lux];
                for p in 0..passes {
                    for k in 0..old_slots {
                        let from = ((p * old_slots + k) * nn) * lux;
                        let to = ((p * (old_slots + 1) + k) * nn) * lux;
                        grown[to..to + nn * lux].copy_from_slice(&dst.values[from..from + nn * lux]);
                    }
                }
                dst.values = grown;
                dst.styles.push(*style);
                old_slots
            }
            None => {
                let bounce_only = keep.and_then(|keep| (1..dst.styles.len()).find(|&k| !keep.contains(&dst.styles[k])));
                let Some(k) = bounce_only.filter(|_| sign > 0.0) else { continue };
                dst.styles[k] = *style;
                for p in 0..passes {
                    let at = dst.index(p, k, 0, lux);
                    dst.values[at..at + nn * lux].fill(glam::Vec3::ZERO);
                }
                k
            }
        };
        for p in 0..passes {
            for n in 0..nn {
                let s = src.index(p, sslot, n, lux);
                let d = dst.index(p, dslot, n, lux);
                let (Some(from), Some(to)) = (src.values.get(s..s + lux), dst.values.get_mut(d..d + lux)) else {
                    continue;
                };
                for (a, b) in to.iter_mut().zip(from) {
                    *a += *b * sign;
                }
            }
        }
    }
}

fn prune_styles(f: &mut FaceLight, passes: usize, lux: usize) {
    prune_styles_below(f, passes, lux, 1e-4);
}

fn prune_styles_below(f: &mut FaceLight, passes: usize, lux: usize, floor: f32) {
    let nn = f.normals.max(1);
    if f.styles.len() <= 1 || values_len(f, passes, lux) != f.values.len() {
        return;
    }
    let keep: Vec<usize> = (0..f.styles.len())
        .filter(|&slot| {
            slot == 0
                || (0..passes).any(|p| {
                    (0..nn).any(|n| {
                        let at = f.index(p, slot, n, lux);
                        f.values[at..at + lux].iter().any(|v| v.max_element() > floor)
                    })
                })
        })
        .collect();
    if keep.len() == f.styles.len() {
        return;
    }
    let mut values = vec![glam::Vec3::ZERO; passes * keep.len() * nn * lux];
    for p in 0..passes {
        for (k, &slot) in keep.iter().enumerate() {
            let from = f.index(p, slot, 0, lux);
            let to = ((p * keep.len() + k) * nn) * lux;
            values[to..to + nn * lux].copy_from_slice(&f.values[from..from + nn * lux]);
        }
    }
    f.styles = keep.iter().map(|&k| f.styles[k]).collect();
    f.values = values;
}

fn changed(a: &FaceLight, b: &FaceLight) -> bool {
    if a.styles != b.styles || a.normals != b.normals || a.values.len() != b.values.len() {
        return true;
    }
    a.values.iter().zip(&b.values).any(|(x, y)| {
        let (x, y) = (Vec3A::from(*x), Vec3A::from(*y));
        let d = (x - y).abs();
        let limit = Vec3A::splat(CHANGE_ABS) + x.abs().max(y.abs()) * CHANGE_REL;
        d.cmpgt(limit).any()
    })
}

fn face_center(f: &LitFace) -> Vec3A {
    ((f.bounds.0 + f.bounds.1) * 0.5).as_vec3a()
}

impl Session {
    pub fn new(
        bsp: Vec<u8>,
        opts: &RadOptions,
        models: &dyn crate::props::ModelSource,
        log: &mut dyn FnMut(&str),
        ctl: &Control<'_>,
    ) -> Result<Session, Stop> {
        let scene = std::sync::Arc::new(Scene::build(bsp, opts, models, log, ctl)?);
        Ok(Session { scene, state: None, gather: None, stitch: None, carry: None })
    }

    pub fn compatible(&self, opts: &RadOptions) -> bool {
        self.scene.key == SceneKey::of(opts)
    }

    pub fn has_lighting(&self) -> bool {
        self.state.is_some()
    }

    pub fn faces(&self) -> usize {
        self.scene.faces.len()
    }

    pub fn luxels(&self) -> usize {
        self.scene.luxels
    }

    pub fn shadow_triangles(&self) -> usize {
        self.scene.shadow.len()
    }

    pub fn styled_entities(&self) -> Option<&[hp_bsp::Entity]> {
        self.state.as_ref().map(|s| s.entities.as_slice())
    }

    pub fn deltas_since_full(&self) -> u32 {
        self.state.as_ref().map_or(0, |s| s.deltas)
    }

    pub fn unsettled_faces(&self) -> usize {
        self.state.as_ref().map_or(0, |s| s.touched.iter().filter(|t| **t).count())
    }

    pub fn unsettled(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.bounce_dirty || s.touched.iter().any(|t| *t))
    }

    pub fn output_all(&self) -> Vec<FaceOut> {
        let Some(state) = self.state.as_ref() else { return Vec::new() };
        let faces = &self.scene.faces;
        let pass = self.scene.passes.len().saturating_sub(1);
        let stitch = self.stitch.as_ref();
        (0..faces.len()).into_par_iter().map(|fi| stitched_out(faces, &state.total, stitch, fi, pass)).collect()
    }

    pub fn settle(&mut self, opts: &RadOptions, ctl: &Control<'_>, log: &mut dyn FnMut(&str)) -> Result<Relit, Stop> {
        let Some(mut state) = self.state.take() else { return Ok(Relit::default()) };
        let targets: Vec<usize> = (0..state.touched.len()).filter(|&fi| state.touched[fi]).collect();
        if targets.is_empty() && !state.bounce_dirty {
            self.state = Some(state);
            return Ok(Relit::default());
        }
        let started = Instant::now();
        let work = progress::Work::new();
        let scene = &self.scene;
        let faces = &scene.faces;
        let fresh: Vec<(usize, Option<FaceLight>)> = {
            let ctx = make_ctx(scene, &state.lights, state.params, &work, opts.bounce_budget);
            targets
                .par_iter()
                .map(|&fi| if (ctl.cancelled)() { (fi, None) } else { (fi, Some(ctx.direct(&faces[fi]))) })
                .collect()
        };
        if (ctl.cancelled)() {
            self.state = Some(state);
            return Err(Stop::Cancelled);
        }
        for (fi, l) in fresh {
            if let Some(l) = l {
                state.direct[fi] = l;
                state.touched[fi] = false;
            }
        }
        let direct_seconds = started.elapsed().as_secs_f64();
        let t = Instant::now();
        let ctx = make_ctx(scene, &state.lights, state.params, &work, opts.bounce_budget);
        let total = match total_of(&ctx, &state.direct, opts.bounces, &mut self.gather, true, ctl, log) {
            Ok(total) => total,
            Err(stop) => {
                self.state = Some(state);
                return Err(stop);
            }
        };
        let outs = changed_outputs(faces, &state.total, &total, self.stitch.as_ref(), scene.passes.len());
        drop(ctx);
        log(&format!("{} faces settled exactly in {} ms", targets.len(), started.elapsed().as_millis()));
        state.total = total;
        state.deltas = 0;
        state.bounce_dirty = false;
        let count = state.lights.list.len();
        self.state = Some(state);
        Ok(Relit {
            full: false,
            relit_faces: targets.len(),
            faces: outs,
            lights: count,
            changed_lights: 0,
            direct_seconds,
            bounce_seconds: t.elapsed().as_secs_f64(),
            seconds: started.elapsed().as_secs_f64(),
        })
    }

    pub fn bytes(&self) -> usize {
        let lights = |v: &[FaceLight]| {
            v.iter().map(|f| f.values.capacity() * 16 + f.dark.capacity() + f.styles.capacity() + 64).sum::<usize>()
        };
        let state = self.state.as_ref().map_or(0, |s| lights(&s.direct) + lights(&s.total));
        state
            + self.gather.as_ref().map_or(0, GatherCache::bytes)
            + self.stitch.as_ref().map_or(0, StitchCells::bytes)
            + self.scene.src.bytes().len()
            + (self.scene.shadow.len() + self.scene.surf.len() + self.scene.blockers.len()) * 64
            + self.scene.faces.iter().map(|f| f.poly.len() * 40 + f.poly2.len() * 16 + 400).sum::<usize>()
    }

    pub fn release_lighting(&mut self) {
        self.state = None;
        self.gather = None;
    }

    pub fn forget_lighting(&mut self) {
        self.state = None;
    }

    pub fn into_carry(self) -> Option<Carry> {
        let state = self.state?;
        let passes = self.scene.passes.len();
        let faces = match std::sync::Arc::try_unwrap(self.scene) {
            Ok(scene) => scene.faces,
            Err(shared) => shared.faces.clone(),
        };
        Some(Carry { key: state.key, passes, lights: state.lights, faces, direct: state.direct })
    }

    pub fn accept_carry(&mut self, carry: Option<Carry>) {
        self.carry = carry;
    }

    fn reuse(
        carry: Option<Carry>,
        faces: &[LitFace],
        lights: &Lights,
        key: StateKey,
        passes: usize,
    ) -> (Vec<Option<FaceLight>>, usize) {
        let mut out: Vec<Option<FaceLight>> = vec![None; faces.len()];
        let same = |c: &Carry| {
            let placed = |set: &Lights| {
                let mut keys: Vec<Vec<u32>> =
                    set.list.iter().map(|l| light_key(&Light { cluster: 0, ..l.clone() })).collect();
                keys.sort_unstable();
                keys
            };
            let [old_sky, old_sun, old_super, ..] = c.key.params;
            let [sky, sun, supersample, ..] = key.params;
            old_sky >= sky
                && old_sun >= sun
                && old_super >= supersample
                && c.passes == passes
                && c.lights.sun_extent.to_bits() == lights.sun_extent.to_bits()
                && placed(&c.lights) == placed(lights)
        };
        let Some(carry) = carry.filter(same) else {
            return (out, 0);
        };
        let mut old: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, f) in carry.faces.iter().enumerate() {
            old.entry(face_signature(f)).or_default().push(i);
        }
        let mut matched = vec![None; faces.len()];
        let mut used = vec![false; carry.faces.len()];
        for (i, f) in faces.iter().enumerate() {
            if let Some(list) = old.get_mut(&face_signature(f))
                && let Some(j) = list.pop()
            {
                matched[i] = Some(j);
                used[j] = true;
            }
        }
        let mut dirty: Vec<(glam::DVec3, glam::DVec3)> = Vec::new();
        for (i, f) in faces.iter().enumerate() {
            if matched[i].is_none() {
                dirty.push(f.bounds);
            }
        }
        for (j, f) in carry.faces.iter().enumerate() {
            if !used[j] {
                dirty.push(f.bounds);
            }
        }
        if !matched.iter().any(Option::is_some) {
            return (out, 0);
        }
        let mut direct = carry.direct;
        let kept: Vec<Option<FaceLight>> = faces
            .par_iter()
            .zip(&matched)
            .map(|(f, m)| {
                let j = (*m)?;
                (!lights.list.iter().any(|l| reaches(l, f) && touched_by(l, f, &dirty))).then_some(j)
            })
            .collect::<Vec<Option<usize>>>()
            .into_iter()
            .map(|j| j.and_then(|j| direct.get_mut(j).map(std::mem::take)))
            .collect();
        let count = kept.iter().filter(|k| k.is_some()).count();
        out = kept;
        (out, count)
    }

    fn lights_of(&self, entities: &[hp_bsp::Entity], opts: &RadOptions) -> (Vec<hp_bsp::Entity>, bool, Lights) {
        let mut ents = entities.to_vec();
        let styled = lights::assign_styles(&mut ents, hp_bsp::limits::SWITCHED_LIGHTS.max_usize_in(opts.limits));
        let mut lights = lights::parse(&ents, &self.scene.passes, &self.scene.map);
        if lights.sun_extent <= 0.0 && opts.soft_sun > 0.0 {
            lights.sun_extent = opts.soft_sun.to_radians().sin().max(0.0);
        }
        lights.list.extend(self.scene.emission.lights.iter().cloned());
        (ents, styled, lights)
    }

    pub fn light_costs(&self, entities: &[hp_bsp::Entity], opts: &RadOptions) -> Vec<LightCost> {
        let (ents, _, lights) = self.lights_of(entities, opts);
        let faces = &self.scene.faces;
        let owners = light_owners(&ents);
        lights
            .list
            .par_iter()
            .enumerate()
            .map(|(index, l)| {
                let (mut count, mut luxels) = (0usize, 0u64);
                for f in faces.iter().filter(|f| reaches(l, f)) {
                    count += 1;
                    luxels += f.luxels() as u64;
                }
                let entity = owners.get(index).and_then(|&i| ents.get(i));
                let (hammerid, classname) = match entity {
                    Some(e) => (e.get("hammerid").and_then(|v| v.trim().parse().ok()), e.classname().to_owned()),
                    None => (None, "texture light".to_owned()),
                };
                let classname = match l.kind {
                    Kind::SkyAmbient => format!("{classname} (ambient)"),
                    _ => classname,
                };
                LightCost { index, hammerid, classname, origin: l.origin.to_array(), faces: count, luxels }
            })
            .collect()
    }

    pub fn relight(
        &mut self,
        entities: &[hp_bsp::Entity],
        opts: &RadOptions,
        focus: Option<[f32; 3]>,
        ctl: &Control<'_>,
        log: &mut dyn FnMut(&str),
    ) -> Result<Relit, Stop> {
        let key = StateKey::of(opts);
        let (ents, styled, lights) = self.lights_of(entities, opts);
        let started = Instant::now();
        let mut relit = if self.state.as_ref().is_some_and(|s| s.key == key) {
            self.relight_delta(ents, styled, lights, opts, ctl, log)?
        } else {
            self.relight_full(ents, styled, lights, key, opts, focus, ctl, log)?
        };
        relit.seconds = started.elapsed().as_secs_f64();
        Ok(relit)
    }

    #[allow(clippy::too_many_arguments)]
    fn relight_full(
        &mut self,
        entities: Vec<hp_bsp::Entity>,
        styled: bool,
        lights: Lights,
        key: StateKey,
        opts: &RadOptions,
        focus: Option<[f32; 3]>,
        ctl: &Control<'_>,
        log: &mut dyn FnMut(&str),
    ) -> Result<Relit, Stop> {
        let params = params(opts);
        let work = progress::Work::new();
        let scene = &self.scene;
        let ctx = make_ctx(scene, &lights, params, &work, opts.bounce_budget);
        let faces = &scene.faces;
        let mut order: Vec<usize> = (0..faces.len()).collect();
        if let Some(p) = focus {
            let p = Vec3A::from_array(p);
            order.sort_by(|a, b| {
                face_center(&faces[*a]).distance_squared(p).total_cmp(&face_center(&faces[*b]).distance_squared(p))
            });
        }
        let (mut direct, reused) = Self::reuse(self.carry.take(), faces, &lights, key, scene.passes.len());
        if reused > 0 {
            order.retain(|&fi| direct[fi].is_none());
            log(&format!("{reused} faces kept their direct light: the edit is far from them"));
            if let Some(partial) = ctl.partial {
                let pass = scene.passes.len().saturating_sub(1);
                let kept: Vec<FaceOut> = direct
                    .iter()
                    .enumerate()
                    .filter_map(|(fi, l)| l.as_ref().map(|l| face_out(&faces[fi], l, widen(&l.values), pass)))
                    .collect();
                partial(kept);
            }
        }
        let total_luxels = scene.luxels.max(1) as f64;
        let done = std::sync::atomic::AtomicU64::new(0);
        let t = Instant::now();
        let chunks: Vec<&[usize]> = match ctl.partial {
            Some(_) => {
                let mut out = Vec::new();
                let most = (scene.luxels / 4).max(FIRST_CHUNK);
                let mut target = (scene.luxels / 32).max(FIRST_CHUNK).min(most);
                let (mut start, mut acc) = (0usize, 0usize);
                for (k, &fi) in order.iter().enumerate() {
                    acc += faces[fi].luxels();
                    if acc >= target {
                        out.push(&order[start..=k]);
                        start = k + 1;
                        acc = 0;
                        target = (target * 2).min(most);
                    }
                }
                if start < order.len() {
                    out.push(&order[start..]);
                }
                out
            }
            None => vec![&order[..]],
        };
        for chunk in chunks {
            let lit: Vec<(usize, Option<FaceLight>)> = chunk
                .par_iter()
                .map(|&fi| {
                    if (ctl.cancelled)() {
                        return (fi, None);
                    }
                    let l = ctx.direct(&faces[fi]);
                    let n = done.fetch_add(faces[fi].luxels() as u64, std::sync::atomic::Ordering::Relaxed);
                    (ctl.progress)(Phase::Direct, (n as f64 / total_luxels) as f32);
                    (fi, Some(l))
                })
                .collect();
            if (ctl.cancelled)() {
                return Err(Stop::Cancelled);
            }
            if let Some(partial) = ctl.partial {
                let pass = scene.passes.len().saturating_sub(1);
                let out: Vec<FaceOut> = lit
                    .iter()
                    .filter_map(|(fi, l)| l.as_ref().map(|l| face_out(&faces[*fi], l, widen(&l.values), pass)))
                    .collect();
                partial(out);
            }
            for (fi, l) in lit {
                direct[fi] = l;
            }
        }
        let direct: Vec<FaceLight> = direct.into_iter().map(Option::unwrap_or_default).collect();
        let direct_seconds = t.elapsed().as_secs_f64();
        log(&format!("direct light in {} ms", t.elapsed().as_millis()));
        let t = Instant::now();
        let total = total_of(&ctx, &direct, opts.bounces, &mut self.gather, true, ctl, log)?;
        let bounce_seconds = t.elapsed().as_secs_f64();
        if self.stitch.is_none() {
            self.stitch = Some(StitchCells::of(faces, &direct));
        }
        let t = Instant::now();
        let pass = scene.passes.len().saturating_sub(1);
        let stitch = self.stitch.as_ref();
        let outs: Vec<FaceOut> =
            (0..faces.len()).into_par_iter().map(|fi| stitched_out(faces, &total, stitch, fi, pass)).collect();
        log(&format!("{} lightmaps prepared in {} ms", outs.len(), t.elapsed().as_millis()));
        let count = lights.list.len();
        drop(ctx);
        let touched = vec![false; direct.len()];
        let relit_faces = outs.len() - reused;
        self.state = Some(State {
            key,
            params,
            entities,
            styled,
            lights,
            direct,
            total,
            deltas: 0,
            touched,
            bounce_dirty: false,
            recent: None,
        });
        Ok(Relit {
            full: true,
            relit_faces,
            faces: outs,
            lights: count,
            changed_lights: count,
            direct_seconds,
            bounce_seconds,
            seconds: 0.0,
        })
    }

    fn relight_delta(
        &mut self,
        entities: Vec<hp_bsp::Entity>,
        styled: bool,
        lights: Lights,
        opts: &RadOptions,
        ctl: &Control<'_>,
        log: &mut dyn FnMut(&str),
    ) -> Result<Relit, Stop> {
        let Some(mut state) = self.state.take() else { return Err(Stop::Failed("no lighting to update".into())) };
        let (removed, added) = diff_lights(&state.lights, &lights);
        let count = lights.list.len();
        if removed.is_empty() && added.is_empty() {
            state.entities = entities;
            state.styled = styled;
            state.lights = lights;
            self.state = Some(state);
            return Ok(Relit { full: false, lights: count, ..Relit::default() });
        }
        let changed_lights = removed.len().max(added.len());
        if removed.iter().chain(&added).any(|l| matches!(l.kind, Kind::Sun | Kind::SkyAmbient)) {
            let key = state.key;
            let previous = state.total;
            let mut relit = self.relight_full(entities, styled, lights, key, opts, None, ctl, log)?;
            if let Some(now) = self.state.as_ref() {
                relit.faces = changed_outputs(
                    &self.scene.faces,
                    &previous,
                    &now.total,
                    self.stitch.as_ref(),
                    self.scene.passes.len(),
                );
            }
            relit.full = false;
            relit.changed_lights = changed_lights;
            return Ok(relit);
        }
        let params = state.params;
        let work = progress::Work::new();
        let scene = &self.scene;
        let faces = &scene.faces;
        let passes = scene.passes.len();
        let single = removed.len() == 1 && added.len() == 1;
        let old_set = Lights { list: removed, sun_extent: state.lights.sun_extent };
        let new_set = Lights { list: added, sun_extent: lights.sun_extent };
        let t = Instant::now();
        let mut reused_light = false;
        let (affected, delta): (usize, Vec<FaceDelta>) = if single {
            let (old_l, new_l) = (&old_set.list[0], &new_set.list[0]);
            let cached = state.recent.take().filter(|(k, _)| *k == light_key(old_l)).map(|(_, m)| m);
            reused_light = cached.is_some();
            let ratio = scale_of(old_l, new_l);
            let old_ctx = make_ctx(scene, &old_set, params, &work, opts.bounce_budget);
            let new_ctx = make_ctx(scene, &new_set, params, &work, opts.bounce_budget);
            let targets: Vec<(usize, bool, bool)> = (0..faces.len())
                .filter_map(|fi| {
                    let f = &faces[fi];
                    let o = cached.as_ref().map_or_else(|| reaches(old_l, f), |c| c.contains_key(&fi));
                    let n = reaches(new_l, f);
                    (o || n).then_some((fi, o, n))
                })
                .collect();
            let total_luxels: u64 = targets.iter().map(|t| faces[t.0].luxels() as u64).sum::<u64>().max(1);
            let done = std::sync::atomic::AtomicU64::new(0);
            let delta: Vec<FaceDelta> = targets
                .par_iter()
                .map(|&(fi, o, n)| {
                    if (ctl.cancelled)() {
                        return (fi, None, None);
                    }
                    let f = &faces[fi];
                    let old = o.then(|| {
                        cached.as_ref().and_then(|c| c.get(&fi).cloned()).unwrap_or_else(|| old_ctx.direct(f))
                    });
                    let new = n.then(|| match (&old, &ratio) {
                        (Some(old), Some(r)) => scaled(old, r, f.luxels()),
                        _ => new_ctx.direct(f),
                    });
                    let d = done.fetch_add(f.luxels() as u64, std::sync::atomic::Ordering::Relaxed);
                    (ctl.progress)(Phase::Direct, d as f32 / total_luxels as f32);
                    (fi, old, new)
                })
                .collect();
            let fresh: HashMap<usize, FaceLight> =
                delta.iter().filter_map(|(fi, _, new)| new.as_ref().map(|n| (*fi, n.clone()))).collect();
            if contribution_bytes(&fresh) <= RECENT_BUDGET {
                state.recent = Some((light_key(new_l), fresh));
            }
            (targets.len(), delta)
        } else {
            state.recent = None;
            let old_ctx = make_ctx(scene, &old_set, params, &work, opts.bounce_budget);
            let new_ctx = make_ctx(scene, &new_set, params, &work, opts.bounce_budget);
            let targets: Vec<(usize, bool, bool)> = (0..faces.len())
                .filter_map(|fi| {
                    let f = &faces[fi];
                    let o = old_set.list.iter().any(|l| reaches(l, f));
                    let n = new_set.list.iter().any(|l| reaches(l, f));
                    (o || n).then_some((fi, o, n))
                })
                .collect();
            let total_luxels: u64 = targets.iter().map(|t| faces[t.0].luxels() as u64).sum::<u64>().max(1);
            let done = std::sync::atomic::AtomicU64::new(0);
            let delta = targets
                .par_iter()
                .map(|&(fi, o, n)| {
                    if (ctl.cancelled)() {
                        return (fi, None, None);
                    }
                    let f = &faces[fi];
                    let old = o.then(|| old_ctx.direct(f));
                    let new = n.then(|| new_ctx.direct(f));
                    let d = done.fetch_add(f.luxels() as u64, std::sync::atomic::Ordering::Relaxed);
                    (ctl.progress)(Phase::Direct, d as f32 / total_luxels as f32);
                    (fi, old, new)
                })
                .collect();
            (targets.len(), delta)
        };
        if reused_light {
            log("the changed light kept its previous contribution");
        }
        if (ctl.cancelled)() {
            state.recent = None;
            self.state = Some(state);
            return Err(Stop::Cancelled);
        }
        let mut emit: Vec<(usize, Vec<Vec<Vec3A>>)> = Vec::new();
        let mut styled_emit: HashMap<u8, Vec<(usize, bake::styles::StyleField)>> = HashMap::new();
        let mut dirty = vec![false; faces.len()];
        for (fi, old, new) in delta {
            styled_delta(fi, old.as_ref(), new.as_ref(), passes, faces[fi].luxels(), &mut styled_emit);
            state.touched[fi] = true;
            dirty[fi] = true;
            let lux = faces[fi].luxels();
            let style0 = |l: &FaceLight, p: usize| {
                let at = l.index(p, 0, 0, lux);
                l.values.get(at..at + lux).map(widen).unwrap_or_else(|| vec![Vec3A::ZERO; lux])
            };
            let mut field: Vec<Vec<Vec3A>> = vec![vec![Vec3A::ZERO; lux]; passes];
            for (l, sign) in [(&old, -1.0f32), (&new, 1.0f32)] {
                let Some(l) = l else { continue };
                for (p, slot) in field.iter_mut().enumerate() {
                    for (a, v) in slot.iter_mut().zip(style0(l, p)) {
                        *a += v * sign;
                    }
                }
            }
            if field.iter().flatten().any(|v| v.abs().max_element() > 1e-4) {
                emit.push((fi, field));
            }
            for total in [false, true] {
                let keep = total.then(|| state.direct[fi].styles.clone());
                let target = if total { &mut state.total[fi] } else { &mut state.direct[fi] };
                if let Some(old) = &old {
                    add_aligned(target, old, -1.0, passes, lux, keep.as_deref());
                }
                if let Some(new) = &new {
                    add_aligned(target, new, 1.0, passes, lux, keep.as_deref());
                }
                for v in &mut target.values {
                    *v = v.max(glam::Vec3::ZERO);
                }
                prune_styles(target, passes, lux);
            }
        }
        let direct_seconds = t.elapsed().as_secs_f64();
        log(&format!("{changed_lights} changed lights: {affected} faces relit in {} ms", t.elapsed().as_millis()));
        let t = Instant::now();
        let ctx = make_ctx(scene, &lights, params, &work, opts.bounce_budget);
        let quick = if opts.bounces == 0 {
            Some(0)
        } else {
            self.gather
                .as_ref()
                .and_then(|cache| incremental::bounce_delta(&ctx, cache, &emit, &mut state.total, &mut dirty))
        };
        let quick = match (quick, self.gather.as_ref()) {
            (Some(n), Some(cache)) if opts.bounces > 0 && !opts.no_style_bounce && !styled_emit.is_empty() => {
                match incremental::bounce_style_delta(&ctx, cache, &styled_emit, &mut state.total, opts.bounces) {
                    Ok(touched) => {
                        for fi in touched {
                            dirty[fi] = true;
                            prune_styles_below(&mut state.total[fi], passes, faces[fi].luxels(), FAINT_BOUNCE);
                        }
                        Some(n)
                    }
                    Err(_) => None,
                }
            }
            (q, _) => q,
        };
        let outs = if quick.is_some() {
            state.bounce_dirty |= opts.bounces > 0;
            if let Some(stitch) = self.stitch.as_ref() {
                let seeds: Vec<usize> = (0..faces.len()).filter(|&fi| dirty[fi]).collect();
                for fi in seeds {
                    for p in stitch.partners(fi) {
                        dirty[p] = true;
                    }
                }
            }
            let pass = passes.saturating_sub(1);
            let stitch = self.stitch.as_ref();
            let total = &state.total;
            (0..faces.len())
                .into_par_iter()
                .filter(|&fi| dirty[fi])
                .map(|fi| stitched_out(faces, total, stitch, fi, pass))
                .collect()
        } else {
            let total = match total_of(&ctx, &state.direct, opts.bounces, &mut self.gather, false, ctl, log) {
                Ok(total) => total,
                Err(stop) => {
                    self.state = Some(state);
                    return Err(stop);
                }
            };
            let outs = changed_outputs(faces, &state.total, &total, self.stitch.as_ref(), passes);
            state.total = total;
            state.bounce_dirty = opts.bounces > 0;
            outs
        };
        let bounce_seconds = t.elapsed().as_secs_f64();
        drop(ctx);
        state.lights = lights;
        state.entities = entities;
        state.styled = styled;
        state.deltas += 1;
        self.state = Some(state);
        Ok(Relit {
            full: false,
            relit_faces: affected,
            faces: outs,
            lights: count,
            changed_lights,
            direct_seconds,
            bounce_seconds,
            seconds: 0.0,
        })
    }

    pub fn lit_bsp(
        &self,
        opts: &RadOptions,
        models: &dyn crate::props::ModelSource,
        ctl: &Control<'_>,
        log: &mut dyn FnMut(&str),
    ) -> Result<Vec<u8>, Stop> {
        self.models_job().ok_or_else(|| Stop::Failed("no lighting to write".into()))?.lit_bsp(opts, models, ctl, log)
    }

    pub fn models_job(&self) -> Option<ModelsJob> {
        let state = self.state.as_ref()?;
        Some(ModelsJob {
            scene: self.scene.clone(),
            lights: state.lights.clone(),
            params: state.params,
            total: state.total.clone(),
            entities: state.entities.clone(),
        })
    }
}

pub struct ModelsJob {
    scene: std::sync::Arc<Scene>,
    lights: Lights,
    params: bake::Params,
    total: Vec<FaceLight>,
    entities: Vec<hp_bsp::Entity>,
}

impl ModelsJob {
    pub fn lit_bsp(
        self,
        opts: &RadOptions,
        models: &dyn crate::props::ModelSource,
        ctl: &Control<'_>,
        log: &mut dyn FnMut(&str),
    ) -> Result<Vec<u8>, Stop> {
        let ModelsJob { scene, lights, params, total, entities } = self;
        let state = WriteState { lights, params, total, entities };
        write(&scene, state, opts, models, ctl, log)
    }
}

struct WriteState {
    lights: Lights,
    params: bake::Params,
    total: Vec<FaceLight>,
    entities: Vec<hp_bsp::Entity>,
}

fn write(
    scene: &Scene,
    state: WriteState,
    opts: &RadOptions,
    models: &dyn crate::props::ModelSource,
    ctl: &Control<'_>,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<u8>, Stop> {
    {
        let src = &scene.src;
        let map = &scene.map;
        let faces = &scene.faces;
        let passes = &scene.passes;
        let with_pak = props::MapPak { pak: src.pakfile().ok().filter(|p| !p.is_empty()), inner: models };
        let models: &dyn crate::props::ModelSource = &with_pak;
        let work = progress::Work::new();
        let ctx = make_ctx(scene, &state.lights, state.params, &work, opts.bounce_budget);
        let mut light = state.total;
        bake::stitch(faces, &mut light);
        (ctl.progress)(Phase::Models, 0.1);
        let t = Instant::now();
        let average: Vec<Vec<Vec3A>> = faces
            .par_iter()
            .zip(&light)
            .map(|(f, l)| {
                let lux = f.luxels();
                (0..passes.len())
                    .map(|p| {
                        let at = l.index(p, 0, 0, lux);
                        let finished: Vec<Vec3A> = l
                            .values
                            .get(at..at + lux)
                            .unwrap_or(&[])
                            .iter()
                            .map(|v| output::finish(Vec3A::from(*v), f.minlight))
                            .collect();
                        output::median(&finished)
                    })
                    .collect()
            })
            .collect();
        let colors = ambient::FaceColors { light: &light, average };
        let ambient = ambient::leaf_ambient(&ctx, &colors);
        let probes = opts.probes.as_ref().and_then(|po| ambient::probe_volume(&ctx, &colors, po));
        log(&format!("leaf ambient in {} ms", t.elapsed().as_millis()));
        if (ctl.cancelled)() {
            return Err(Stop::Cancelled);
        }
        (ctl.progress)(Phase::Models, 0.3);
        let detail = detail::light(src, &ctx, &light, models, passes)?;
        let t = Instant::now();
        let prop_files = if opts.props.lighting && scene.static_props.len() > 0 {
            let bounced = opts.bounces > 0 && map.vis.clusters > 0;
            let (files, n, _) =
                props::light(&ctx, &light, &scene.static_props, &opts.props, bounced, passes, ctl.cancelled);
            log(&format!("static props: {n} with vertex lighting in {} ms", t.elapsed().as_millis()));
            files
        } else {
            Vec::new()
        };
        if (ctl.cancelled)() {
            return Err(Stop::Cancelled);
        }
        (ctl.progress)(Phase::Models, 0.9);
        let pak = if prop_files.is_empty() { None } else { Some(props::add_to_pak(src, prop_files)?) };
        let map_flags = props::map_flags(
            &src.lump(lump::MAP_FLAGS).map(|c| c.into_owned()).unwrap_or_default(),
            opts.props.lighting,
            passes,
        );
        let layout = output::layout(faces, &light);
        let faces_lump = output::patch_faces(&map.faces_raw, &layout);
        let raw = |i: usize| src.lump(i).map(|c| c.into_owned()).unwrap_or_default();
        let (vert_normals, vert_indices) = crate::emit::vertex_normals(
            &raw(lump::PLANES),
            &raw(lump::FACES),
            &raw(lump::SURFEDGES),
            &raw(lump::EDGES),
        );
        let worldlights_version = src.lump_info(15).map_or(0, |l| l.version);
        let leafs = {
            let raw_leafs = raw(lump::LEAFS);
            let version = src.lump_info(lump::LEAFS).map_or(1, |l| l.version);
            let size = if version == 0 { 56 } else { 32 };
            let sees_sky = |l: &super::data::Leaf| {
                let center = Vec3A::new(
                    ((l.mins[0] + l.maxs[0]) * 0.5) as f32,
                    ((l.mins[1] + l.maxs[1]) * 0.5) as f32,
                    ((l.mins[2] + l.maxs[2]) * 0.5) as f32,
                );
                let mut seen = super::SkySeen::default();
                for n in &detail::ANORMS {
                    match ctx.trace_sky_kind(center, -Vec3A::from_array(*n)) {
                        Some(false) => {
                            seen.sky3d = true;
                            break;
                        }
                        Some(true) => seen.sky2d = true,
                        None => {}
                    }
                }
                seen
            };
            (super::leaf_sky_flags(&raw_leafs, size, map, &sees_sky), version)
        };
        let dispinfo = crate::disp::strip_no_shadow(&raw(lump::DISPINFO))
            .map(|d| (d, src.lump_info(lump::DISPINFO).map_or(0, |l| l.version)));
        let copy = hp_bsp::Bsp::from_vec(src.bytes().to_vec()).map_err(|e| e.to_string())?;
        let mut edit = copy.edit();
        let fail = |e: hp_bsp::BspError| e.to_string();
        edit.replace_lump_with_version(lump::VERTNORMALS, vert_normals, 0).map_err(fail)?;
        edit.replace_lump_with_version(lump::VERTNORMALINDICES, vert_indices, 0).map_err(fail)?;
        if !scene.emission.light_texinfos.is_empty() {
            let mut texinfo = raw(lump::TEXINFO);
            for &t in &scene.emission.light_texinfos {
                if let Some(b) = texinfo.get_mut(t * 72 + 64..t * 72 + 68) {
                    let flags = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) | crate::materials::surf::LIGHT;
                    b.copy_from_slice(&flags.to_le_bytes());
                }
            }
            let version = src.lump_info(lump::TEXINFO).map_or(0, |l| l.version);
            edit.replace_lump_with_version(lump::TEXINFO, texinfo, version).map_err(fail)?;
        }
        for (p, &hdr) in passes.iter().enumerate() {
            let data = output::lighting(faces, &light, &layout, p);
            let (faces_i, light_i, wl_i, idx_i, amb_i) = if hdr {
                (lump::FACES_HDR, lump::LIGHTING_HDR, 54, lump::LEAF_AMBIENT_INDEX_HDR, lump::LEAF_AMBIENT_LIGHTING_HDR)
            } else {
                (lump::FACES, lump::LIGHTING, 15, lump::LEAF_AMBIENT_INDEX, lump::LEAF_AMBIENT_LIGHTING)
            };
            let faces_version = if map.faces_version == 0 { 1 } else { map.faces_version };
            edit.replace_lump_with_version(faces_i, faces_lump.clone(), faces_version).map_err(fail)?;
            edit.replace_lump_with_version(light_i, data, 1).map_err(fail)?;
            let records = lights::worldlights(&state.lights, p);
            let records =
                if worldlights_version >= 1 { hp_bsp::format::worldlights_v0_to_v1(&records) } else { records };
            edit.replace_lump_with_version(wl_i, records, worldlights_version).map_err(fail)?;
            let (index, samples) = ambient.get(p).cloned().unwrap_or_default();
            edit.replace_lump_with_version(idx_i, index, 0).map_err(fail)?;
            edit.replace_lump_with_version(amb_i, samples, 1).map_err(fail)?;
        }
        if let [only] = passes[..] {
            let (index, samples) = ambient.first().cloned().unwrap_or_default();
            let (idx_i, amb_i) = if only {
                (lump::LEAF_AMBIENT_INDEX, lump::LEAF_AMBIENT_LIGHTING)
            } else {
                (lump::LEAF_AMBIENT_INDEX_HDR, lump::LEAF_AMBIENT_LIGHTING_HDR)
            };
            edit.replace_lump_with_version(idx_i, index, 0).map_err(fail)?;
            edit.replace_lump_with_version(amb_i, samples, 1).map_err(fail)?;
        }
        edit.set_entities(&state.entities).map_err(fail)?;
        if let Some((games, _)) = detail {
            edit.set_game_lumps(games);
        }
        if let Some(v) = &probes {
            edit.replace_lump_with_version(hp_light::probes::LUMP, v.encode(), 0).map_err(fail)?;
        }
        edit.replace_lump_with_version(lump::LEAFS, leafs.0, leafs.1).map_err(fail)?;
        edit.replace_lump_with_version(lump::MAP_FLAGS, map_flags, 0).map_err(fail)?;
        if let Some(pak) = pak {
            edit.set_pakfile(pak).map_err(fail)?;
        }
        if let Some((d, version)) = dispinfo {
            edit.replace_lump_with_version(lump::DISPINFO, d, version).map_err(fail)?;
        }
        let out = edit.to_vec().map_err(fail)?;
        (ctl.progress)(Phase::Models, 1.0);
        Ok(out)
    }
}

fn styled_delta(
    fi: usize,
    old: Option<&FaceLight>,
    new: Option<&FaceLight>,
    passes: usize,
    lux: usize,
    out: &mut HashMap<u8, Vec<(usize, bake::styles::StyleField)>>,
) {
    let mut styles: Vec<u8> =
        old.into_iter().chain(new).flat_map(|l| l.styles.iter().copied()).filter(|s| *s != 0).collect();
    styles.sort_unstable();
    styles.dedup();
    for style in styles {
        let mut field: bake::styles::StyleField = vec![vec![glam::Vec3::ZERO; lux]; passes];
        for (l, sign) in [(old, -1.0f32), (new, 1.0f32)] {
            let Some(l) = l else { continue };
            let Some(slot) = l.styles.iter().position(|s| *s == style) else { continue };
            for (p, row) in field.iter_mut().enumerate() {
                let at = l.index(p, slot, 0, lux);
                if let Some(src) = l.values.get(at..at + lux) {
                    for (a, v) in row.iter_mut().zip(src) {
                        *a += *v * sign;
                    }
                }
            }
        }
        if field.iter().flatten().any(|v| v.abs().max_element() > 1e-4) {
            out.entry(style).or_default().push((fi, field));
        }
    }
}

fn total_of(
    ctx: &bake::Ctx<'_>,
    direct: &[FaceLight],
    bounces: u32,
    gather: &mut Option<GatherCache>,
    exact: bool,
    ctl: &Control<'_>,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<FaceLight>, Stop> {
    let mut light = direct.to_vec();
    bake::fill_dark(ctx.faces, &mut light);
    disp::radial(ctx, &mut light);
    if bounces == 0 {
        bake::fill_dark(ctx.faces, &mut light);
        return Ok(light);
    }
    let unbounced = disp::before_bounce(ctx, &light);
    if exact {
        (ctl.progress)(Phase::Bounce, 0.1);
        ctx.bounce(&mut light, bounces, log).map_err(Stop::Failed)?;
        if (ctl.cancelled)() {
            return Err(Stop::Cancelled);
        }
        if let Some(saved) = unbounced {
            disp::after_bounce(ctx, &mut light, saved);
        }
        bake::fill_dark(ctx.faces, &mut light);
        return Ok(light);
    }
    let step = incremental::bounce_step(ctx);
    if gather.as_ref().is_none_or(|g| g.step != step || g.rays != ctx.params.bounce_rays) {
        *gather = None;
        let total = ctx.faces.iter().map(LitFace::luxels).sum::<usize>().max(1) as f64;
        let done = std::sync::atomic::AtomicU64::new(0);
        let note = |n: usize| {
            let d = done.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed) + n as u64;
            (ctl.progress)(Phase::Bounce, (d as f64 / total) as f32 * 0.8);
        };
        let t = Instant::now();
        let cache = incremental::gather(ctx, ctl.cancelled, &note)?;
        let Some(cache) = cache.filter(|_| !(ctl.cancelled)()) else { return Err(Stop::Cancelled) };
        log(&format!(
            "bounce gather grids: step {}, {} rays, traced in {} ms, {} MB in memory, {} MB in temporary tiles",
            cache.step,
            cache.rays,
            t.elapsed().as_millis(),
            cache.bytes() >> 20,
            cache.spilled() >> 20
        ));
        *gather = Some(cache);
    }
    if let Some(cache) = gather.as_mut() {
        (ctl.progress)(Phase::Bounce, 0.85);
        incremental::bounce_cached(ctx, cache, &mut light, bounces, ctl.cancelled, log)
            .map_err(|e| if (ctl.cancelled)() { Stop::Cancelled } else { Stop::Failed(e) })?;
    }
    if let Some(saved) = unbounced {
        disp::after_bounce(ctx, &mut light, saved);
    }
    bake::fill_dark(ctx.faces, &mut light);
    Ok(light)
}

fn changed_outputs(
    faces: &[LitFace],
    before: &[FaceLight],
    after: &[FaceLight],
    stitch: Option<&StitchCells>,
    passes: usize,
) -> Vec<FaceOut> {
    let mut dirty: Vec<bool> = (0..faces.len())
        .into_par_iter()
        .map(|fi| before.get(fi).is_none_or(|b| after.get(fi).is_some_and(|a| changed(b, a))))
        .collect();
    if let Some(stitch) = stitch {
        let seeds: Vec<usize> = (0..faces.len()).filter(|&fi| dirty[fi]).collect();
        for fi in seeds {
            for p in stitch.partners(fi) {
                dirty[p] = true;
            }
        }
    }
    let pass = passes.saturating_sub(1);
    (0..faces.len())
        .into_par_iter()
        .filter(|&fi| dirty[fi])
        .map(|fi| stitched_out(faces, after, stitch, fi, pass))
        .collect()
}

fn face_out(f: &LitFace, l: &FaceLight, values: Vec<Vec3A>, pass: usize) -> FaceOut {
    let lux = f.luxels();
    let nn = l.normals.max(1);
    let mut out = Vec::with_capacity(l.styles.len() * nn * lux);
    for slot in 0..l.styles.len() {
        for n in 0..nn {
            let at = l.index(pass, slot, n, lux);
            match values.get(at..at + lux) {
                Some(v) => out.extend(v.iter().map(|v| output::finish(*v, f.minlight).to_array())),
                None => out.extend(std::iter::repeat_n([0.0; 3], lux)),
            }
        }
    }
    FaceOut {
        face: f.face as u32,
        width: f.w as u32,
        height: f.h as u32,
        normals: nn as u32,
        styles: l.styles.clone(),
        values: out,
    }
}

fn stitched_out(
    faces: &[LitFace],
    total: &[FaceLight],
    stitch: Option<&StitchCells>,
    fi: usize,
    pass: usize,
) -> FaceOut {
    let l = &total[fi];
    let mut values = widen(&l.values);
    if let Some(stitch) = stitch {
        stitch.apply(fi, faces, total, &mut values);
    }
    face_out(&faces[fi], l, values, pass)
}

impl Scene {
    fn build(
        bsp: Vec<u8>,
        opts: &RadOptions,
        models: &dyn crate::props::ModelSource,
        log: &mut dyn FnMut(&str),
        ctl: &Control<'_>,
    ) -> Result<Scene, Stop> {
        let start = Instant::now();
        (ctl.progress)(Phase::Scene, 0.0);
        let mut src = hp_bsp::Bsp::from_vec(bsp).map_err(|e| format!("not a BSP file: {e}"))?;
        if opts.luxel_density > 0.0 && opts.luxel_density < 1.0 {
            let scaled = output::rescale_luxels(src, opts.luxel_density)?;
            src = hp_bsp::Bsp::from_vec(scaled).map_err(|e| format!("not a BSP file: {e}"))?;
        }
        if !(19..=21).contains(&src.version()) {
            return Err(Stop::Failed(format!("BSP version {} is not supported", src.version())));
        }
        let map = MapData::read(&src)?;
        let passes: Vec<bool> = [(opts.ldr, false), (opts.hdr, true)].iter().filter(|p| p.0).map(|p| p.1).collect();
        if passes.is_empty() {
            return Err(Stop::Failed("nothing to light: neither LDR nor HDR asked".into()));
        }
        let tables = passes
            .iter()
            .map(|&hdr| texlights::LightFileData::read(&opts.texlights.files, hdr, log))
            .collect::<Result<Vec<_>, _>>()?;
        let emission =
            texlights::emitters(&src, &map, &tables, &opts.texlights, opts.bounces > 0 && !opts.texlights.fast);
        (ctl.progress)(Phase::Scene, 0.1);
        let all = surface::lit_faces(&map, opts.smooth_angle);
        let lit_index = bake::lit_index(map.faces.len(), all.iter().flatten());
        if (ctl.cancelled)() {
            return Err(Stop::Cancelled);
        }
        let rules_table = tables.first();
        let no_shadow = |m: &str| rules_table.is_some_and(|t| t.casts_no_shadow(m));
        let forced = |m: &str| rules_table.is_some_and(|t| t.texture_shadow_forced(m));
        let rules = props::ShadowRules { no_shadow: &no_shadow, forced: &forced };
        let shadows = opts.props.shadows;
        let what = props::ReadWhat {
            collision: shadows,
            texture_shadows: opts.props.texture_shadows,
            lighting: opts.props.lighting,
        };
        let (static_props, prop_tris, occluders) = {
            let with_pak = props::MapPak { pak: src.pakfile().ok().filter(|p| !p.is_empty()), inner: models };
            let models: &dyn crate::props::ModelSource = &with_pak;
            let (mut static_props, mut prop_lines) = if shadows || opts.props.lighting {
                props::StaticProps::read(&src, models, what, &rules)?
            } else {
                (props::StaticProps::default(), Vec::new())
            };
            let (prop_tris, occluders) = if shadows {
                static_props.occluders(opts.props.polys, &rules, models, &mut prop_lines)
            } else {
                (Vec::new(), props::Occluders::default())
            };
            for l in &prop_lines {
                log(l);
            }
            (static_props, prop_tris, occluders)
        };
        (ctl.progress)(Phase::Scene, 0.4);
        if (ctl.cancelled)() {
            return Err(Stop::Cancelled);
        }
        let (shadow, _) = surface::shadow_scene(&map, &all, prop_tris, !opts.no_disp_shadows);
        (ctl.progress)(Phase::Scene, 0.7);
        let (surf, surf_info) = surface::surface_scene(&map, &all, &lit_index);
        let blockers = surface::blocker_scene(&map);
        let (water, water_reflectivity) = surface::water_scene(&map);
        let faces: Vec<LitFace> = all.into_iter().flatten().collect();
        let sky_clusters = surface::sky_clusters(&map);
        let luxels: usize = faces.iter().map(LitFace::luxels).sum();
        if luxels > 1 << 26 {
            return Err(Stop::Failed(format!("{luxels} luxels: the lightmap sizes of the faces are broken")));
        }
        let (cameras, camera_areas) =
            if opts.no_skybox_recurse { (Vec::new(), Vec::new()) } else { bake::sky_cameras(&map) };
        log(&format!(
            "lighting scene: {} faces, {} with lightmaps ({luxels} luxels), {} shadow triangles, {} static props, built in {} ms",
            map.faces.len(),
            faces.len(),
            shadow.len(),
            static_props.len(),
            start.elapsed().as_millis()
        ));
        (ctl.progress)(Phase::Scene, 1.0);
        Ok(Scene {
            src,
            map,
            faces,
            shadow,
            surf,
            surf_info,
            blockers,
            water,
            water_reflectivity,
            static_props,
            occluders,
            sky_clusters,
            cameras,
            camera_areas,
            emission,
            passes,
            key: SceneKey::of(opts),
            luxels,
        })
    }
}

fn widen(values: &[glam::Vec3]) -> Vec<Vec3A> {
    values.iter().map(|v| Vec3A::from(*v)).collect()
}

#[cfg(test)]
mod tests {
    const QUICK_REL: f32 = 0.1;
    use std::collections::HashMap;

    use super::super::tests::{compile_vmf, light_entity, room};
    use super::*;

    fn opts() -> RadOptions {
        RadOptions {
            hdr: true,
            ldr: false,
            bounces: 4,
            quality: RadQuality::Draft,
            threads: Some(2),
            ..Default::default()
        }
    }

    fn never() -> bool {
        false
    }

    fn quiet(_: Phase, _: f32) {}

    fn ctl() -> Control<'static> {
        Control { cancelled: &never, progress: &quiet, partial: None }
    }

    fn entities(bsp: &[u8]) -> Vec<hp_bsp::Entity> {
        hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap().entities().unwrap()
    }

    fn session(bsp: &[u8]) -> Session {
        Session::new(bsp.to_vec(), &opts(), &crate::props::NoModels, &mut |_| {}, &ctl()).unwrap()
    }

    fn by_face(faces: Vec<FaceOut>) -> HashMap<u32, FaceOut> {
        faces.into_iter().map(|f| (f.face, f)).collect()
    }

    fn set(ents: &mut [hp_bsp::Entity], id: &str, key: &str, value: &str) {
        let e = ents.iter_mut().find(|e| e.get("hammerid") == Some(id)).expect("entity");
        e.set(key, value);
    }

    fn summed(f: &FaceOut) -> Vec<[f32; 3]> {
        let n = (f.width * f.height * f.normals) as usize;
        let mut out = vec![[0.0; 3]; n];
        for chunk in f.values.chunks(n.max(1)) {
            for (o, v) in out.iter_mut().zip(chunk) {
                for c in 0..3 {
                    o[c] += v[c];
                }
            }
        }
        out
    }

    fn worst(a: &HashMap<u32, FaceOut>, b: &HashMap<u32, FaceOut>) -> (f32, f32) {
        assert_eq!(a.len(), b.len());
        let (mut diff, mut peak) = (0.0f32, 0.0f32);
        for (k, fa) in a {
            let fb = &b[k];
            assert_eq!((fa.width, fa.height, fa.normals), (fb.width, fb.height, fb.normals));
            for (x, y) in summed(fa).iter().zip(summed(fb).iter()) {
                for c in 0..3 {
                    diff = diff.max((x[c] - y[c]).abs());
                    peak = peak.max(x[c].abs()).max(y[c].abs());
                }
            }
        }
        (diff, peak)
    }

    #[test]
    fn a_full_session_writes_the_lightmaps_of_the_compiler() {
        let text = room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", ""));
        let bsp = compile_vmf(&text);
        let o = opts();
        let reference = hp_bsp::Bsp::from_vec(super::super::light_bsp(&bsp, &o, &mut |_| {}).unwrap()).unwrap();
        let mut s = session(&bsp);
        let relit = s.relight(&entities(&bsp), &o, None, &ctl(), &mut |_| {}).unwrap();
        assert!(relit.full);
        assert_eq!(relit.faces.len(), s.faces());
        let lit = hp_bsp::Bsp::from_vec(s.lit_bsp(&o, &crate::props::NoModels, &ctl(), &mut |_| {}).unwrap()).unwrap();
        for l in [lump::LIGHTING_HDR, lump::FACES_HDR, lump::LEAF_AMBIENT_LIGHTING_HDR, lump::LEAFS] {
            assert_eq!(lit.lump(l).unwrap(), reference.lump(l).unwrap(), "lump {l}");
        }
    }

    #[test]
    fn a_direct_style_always_wins_over_a_bounce_layer() {
        let lux = 4;
        let layer = |styles: Vec<u8>, v: f32| FaceLight {
            values: vec![glam::Vec3::splat(v); styles.len() * lux],
            styles,
            normals: 1,
            dark: vec![false; lux],
        };
        let mut total = layer(vec![0, 32, 33, 40], 1.0);
        let direct = layer(vec![0, 32, 33], 1.0);
        let new = layer(vec![34], 5.0);
        add_aligned(&mut total, &new, 1.0, 1, lux, Some(&direct.styles));
        assert_eq!(total.styles, [0, 32, 33, 34], "the bounce-only layer of style 40 gives its slot");
        assert_eq!(total.values[3 * lux], glam::Vec3::splat(5.0));
        let mut full = layer(vec![0, 32, 33, 35], 1.0);
        add_aligned(&mut full, &new, 1.0, 1, lux, Some(&[0, 32, 33, 35]));
        assert_eq!(full.styles, [0, 32, 33, 35], "direct layers are never pushed out");
    }

    #[test]
    fn switchable_light_edits_keep_their_bounced_light() {
        let lights = light_entity(
            500,
            "64 128 128",
            "	\"targetname\" \"lamp\"
",
        ) + &light_entity(501, "448 128 96", "");
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &lights));
        let o = opts();
        let mut s = session(&bsp);
        let mut ents = entities(&bsp);
        let mut shown = by_face(s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap().faces);
        for (key, value) in [("_light", "255 80 20 400"), ("origin", "96 100 150")] {
            set(&mut ents, "500", key, value);
            let relit = s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap();
            assert!(!relit.full, "{key}: incremental");
            for f in relit.faces {
                shown.insert(f.face, f);
            }
            let fresh = by_face(session(&bsp).relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap().faces);
            let (diff, peak) = worst(&shown, &fresh);
            assert!(diff <= peak * QUICK_REL + 0.5, "{key}: the switchable bounce is off by {diff} of {peak}");
        }
    }

    #[test]
    fn light_edits_relight_only_their_faces_and_match_a_full_bake() {
        let lights = light_entity(500, "64 128 128", "") + &light_entity(501, "448 128 96", "");
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &lights));
        let o = opts();
        let mut s = session(&bsp);
        let mut ents = entities(&bsp);
        let mut shown = by_face(s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap().faces);
        let all = shown.len();
        for (key, value) in [("origin", "96 100 150"), ("_light", "255 80 20 400"), ("_fifty_percent_distance", "160")]
        {
            set(&mut ents, "500", key, value);
            let relit = s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap();
            assert!(!relit.full, "{key}: incremental");
            assert_eq!(relit.changed_lights, 1);
            assert!(relit.relit_faces < all, "{key}: {} of {all} faces relit", relit.relit_faces);
            assert!(!relit.faces.is_empty(), "{key}: changed faces");
            for f in relit.faces {
                shown.insert(f.face, f);
            }
            let fresh = by_face(session(&bsp).relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap().faces);
            let (diff, peak) = worst(&shown, &fresh);
            assert!(diff <= peak * QUICK_REL + 0.5, "{key}: the incremental result is off by {diff} of {peak}");
        }
        assert_eq!(s.deltas_since_full(), 3);
        assert!(s.unsettled_faces() > 0 && s.unsettled_faces() < all);
        let settled = s.settle(&o, &ctl(), &mut |_| {}).unwrap();
        for f in settled.faces {
            shown.insert(f.face, f);
        }
        assert_eq!(s.unsettled_faces(), 0);
        let fresh = by_face(session(&bsp).relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap().faces);
        let (diff, peak) = worst(&shown, &fresh);
        assert!(diff <= peak * CHANGE_REL + CHANGE_ABS * 2.0, "the settled result is exact: {diff} of {peak}");
        assert!(s.output_all().len() == all);
        let same = s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap();
        assert!(!same.full && same.faces.is_empty() && same.changed_lights == 0, "nothing to relight");
    }

    #[test]
    fn switchable_lights_get_and_lose_their_style() {
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
        let o = opts();
        let mut s = session(&bsp);
        let mut ents = entities(&bsp);
        s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap();
        let mut lamp = ents.iter().find(|e| e.get("hammerid") == Some("500")).unwrap().clone();
        lamp.set("hammerid", "777");
        lamp.set("origin", "400 128 128");
        lamp.set("targetname", "lamp");
        ents.push(lamp);
        let relit = s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap();
        assert!(!relit.full);
        assert!(relit.faces.iter().any(|f| f.styles.len() == 2 && f.styles[1] >= 32), "a switchable style");
        let styled = s.styled_entities().unwrap();
        assert!(styled.iter().any(|e| e.get("targetname") == Some("lamp") && e.get("style").is_some()));
        ents.pop();
        let relit = s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap();
        assert!(relit.faces.iter().all(|f| f.styles == [0]), "the style slot is gone with its light");
        let fresh = by_face(session(&bsp).relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap().faces);
        let mut shown = HashMap::new();
        for f in relit.faces {
            shown.insert(f.face, f);
        }
        let scene_peak = worst(&fresh, &fresh).1;
        for (face, f) in &shown {
            let (diff, _) = worst(&HashMap::from([(*face, f.clone())]), &HashMap::from([(*face, fresh[face].clone())]));
            assert!(diff <= scene_peak * QUICK_REL + 0.5, "face {face}: {diff} of {scene_peak}");
        }
        for f in s.settle(&o, &ctl(), &mut |_| {}).unwrap().faces {
            shown.insert(f.face, f);
        }
        let all: HashMap<_, _> = s.output_all().into_iter().map(|f| (f.face, f)).collect();
        let (diff, peak) = worst(&all, &fresh);
        assert!(diff <= peak * CHANGE_REL + CHANGE_ABS * 2.0, "the settled result is exact: {diff} of {peak}");
    }

    #[test]
    fn a_quality_change_relights_everything_and_cancelling_keeps_the_last_result() {
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
        let mut s = session(&bsp);
        let mut ents = entities(&bsp);
        s.relight(&ents, &opts(), Some([0.0; 3]), &ctl(), &mut |_| {}).unwrap();
        let normal = RadOptions { quality: RadQuality::Normal, ..opts() };
        let relit = s.relight(&ents, &normal, None, &ctl(), &mut |_| {}).unwrap();
        assert!(relit.full && relit.faces.len() == s.faces());
        set(&mut ents, "500", "origin", "200 120 100");
        let stop = || true;
        let cancelled = Control { cancelled: &stop, progress: &quiet, partial: None };
        assert!(matches!(s.relight(&ents, &normal, None, &cancelled, &mut |_| {}), Err(Stop::Cancelled)));
        assert!(s.has_lighting());
        let relit = s.relight(&ents, &normal, None, &ctl(), &mut |_| {}).unwrap();
        assert!(!relit.full && relit.changed_lights == 1);
    }

    #[test]
    fn sun_edits_relight_every_face_exactly() {
        let sun = "entity\n{\n\t\"id\" \"600\"\n\t\"classname\" \"light_environment\"\n\t\"origin\" \"64 64 64\"\n\t\"_light\" \"255 255 255 300\"\n\t\"_ambient\" \"40 60 90 60\"\n\t\"angles\" \"-60 30 0\"\n\t\"pitch\" \"-60\"\n}\n";
        let text = super::super::tests::room_with("DEV/DEV_MEASUREGENERIC01B", "TOOLS/TOOLSSKYBOX", sun);
        let bsp = compile_vmf(&text);
        let o = opts();
        let mut s = session(&bsp);
        let mut ents = entities(&bsp);
        let mut shown = by_face(s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap().faces);
        set(&mut ents, "600", "pitch", "-35");
        let relit = s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap();
        assert!(!relit.full && relit.changed_lights >= 1);
        assert_eq!(s.unsettled_faces(), 0, "global light changes are exact at once");
        for f in relit.faces {
            shown.insert(f.face, f);
        }
        let fresh = by_face(session(&bsp).relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap().faces);
        let (diff, peak) = worst(&shown, &fresh);
        assert!(diff <= peak * 1e-4 + 0.06, "{diff} of {peak}");
    }

    #[test]
    fn progressive_bakes_stream_the_nearest_faces_first() {
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
        let mut s = session(&bsp);
        let seen = std::sync::Mutex::new(Vec::new());
        let partial = |faces: Vec<FaceOut>| seen.lock().unwrap().push(faces.len());
        let control = Control { cancelled: &never, progress: &quiet, partial: Some(&partial) };
        let relit = s.relight(&entities(&bsp), &opts(), Some([0.0, 0.0, 0.0]), &control, &mut |_| {}).unwrap();
        let chunks = seen.into_inner().unwrap();
        assert!(!chunks.is_empty());
        assert_eq!(chunks.iter().sum::<usize>(), relit.faces.len());
    }

    fn corridor(far: f64) -> String {
        use glam::DVec3;
        let mut id = 1;
        let w = "DEV/DEV_MEASUREGENERIC01B";
        let block = |id: &mut u32, lo: DVec3, hi: DVec3| super::super::tests::block(id, lo, hi, w, w);
        let (lo, hi, t) = (DVec3::ZERO, DVec3::new(2048.0, 256.0, 256.0), 16.0);
        let mut solids = String::new();
        solids += &block(&mut id, DVec3::new(lo.x - t, lo.y - t, lo.z - t), DVec3::new(hi.x + t, hi.y + t, lo.z));
        solids += &block(&mut id, DVec3::new(lo.x - t, lo.y - t, hi.z), DVec3::new(hi.x + t, hi.y + t, hi.z + t));
        solids += &block(&mut id, DVec3::new(lo.x - t, lo.y - t, lo.z), DVec3::new(lo.x, hi.y + t, hi.z));
        solids += &block(&mut id, DVec3::new(hi.x, lo.y - t, lo.z), DVec3::new(hi.x + t, hi.y + t, hi.z));
        solids += &block(&mut id, DVec3::new(lo.x, lo.y - t, lo.z), DVec3::new(hi.x, lo.y, hi.z));
        solids += &block(&mut id, DVec3::new(lo.x, hi.y, lo.z), DVec3::new(hi.x, hi.y + t, hi.z));
        for x in [96.0, 224.0, 352.0] {
            solids += &block(&mut id, DVec3::new(x, 96.0, 32.0), DVec3::new(x + 32.0, 160.0, 96.0));
        }
        solids += &block(&mut id, DVec3::new(far, 96.0, 32.0), DVec3::new(far + 32.0, 160.0, 96.0));
        let lamp = light_entity(
            500,
            "160 128 128",
            "\t\"_fifty_percent_distance\" \"96\"\n\t\"_zero_percent_distance\" \"200\"\n",
        );
        format!(
            "world\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n{solids}}}\n{lamp}entity\n{{\n\t\"id\" \"9000\"\n\t\"classname\" \"info_player_start\"\n\t\"origin\" \"64 64 16\"\n}}\n"
        )
    }

    #[test]
    fn brush_edits_keep_the_direct_light_of_faces_far_from_them() {
        let (before, after) = (compile_vmf(&corridor(1800.0)), compile_vmf(&corridor(1880.0)));
        let o = opts();
        let mut old = session(&before);
        old.relight(&entities(&before), &o, None, &ctl(), &mut |_| {}).unwrap();
        let mut new = session(&after);
        new.accept_carry(old.into_carry());
        let mut lines = Vec::new();
        let relit = new.relight(&entities(&after), &o, None, &ctl(), &mut |l| lines.push(l.to_owned())).unwrap();
        assert!(relit.full);
        assert!(relit.relit_faces < new.faces(), "{} of {} faces lit again: {lines:?}", relit.relit_faces, new.faces());
        let fresh = by_face(session(&after).relight(&entities(&after), &o, None, &ctl(), &mut |_| {}).unwrap().faces);
        let (diff, peak) = worst(&by_face(relit.faces), &fresh);
        assert!(diff <= peak * CHANGE_REL + CHANGE_ABS * 2.0, "kept faces match a fresh bake: {diff} of {peak}");
        let mut other = session(&after);
        other.accept_carry(session(&before).into_carry());
        let relit = other.relight(&entities(&after), &o, None, &ctl(), &mut |_| {}).unwrap();
        assert_eq!(relit.relit_faces, other.faces(), "nothing to keep without a previous result");
    }

    #[test]
    #[ignore = "manual benchmark on a compiled preview BSP named by HP_SESSION_BSP"]
    fn benchmark_a_compiled_map() {
        let Some(path) = std::env::var_os("HP_SESSION_BSP") else { return };
        let bsp = std::fs::read(path).expect("bsp");
        let normal = std::env::var_os("HP_SESSION_NORMAL").is_some();
        let o = RadOptions {
            hdr: true,
            ldr: false,
            bounces: 100,
            quality: if normal { RadQuality::Normal } else { RadQuality::Draft },
            extra_sky: (!normal).then_some(40.0 / 162.0),
            no_extra: !normal,
            normal_sampling: normal,
            min_sun_samples: 6,
            bounce_budget: 192 << 20,
            threads: None,
            ..RadOptions::default()
        };
        let t = Instant::now();
        let mut s = Session::new(bsp.clone(), &o, &crate::props::NoModels, &mut |_| {}, &ctl()).unwrap();
        eprintln!("BENCH scene {} faces {} luxels in {:.3} s", s.faces(), s.luxels(), t.elapsed().as_secs_f64());
        let mut ents = entities(&bsp);
        let r = s.relight(&ents, &o, None, &ctl(), &mut |l| eprintln!("  {l}")).unwrap();
        eprintln!("BENCH full {:.3} s (direct {:.3}, bounce {:.3})", r.seconds, r.direct_seconds, r.bounce_seconds);
        let lamp = ents.iter().position(|e| e.classname() == "light").expect("a light");
        let origin = ents[lamp].get("origin").unwrap_or("0 0 0").to_owned();
        let moved: Vec<f64> = origin.split_whitespace().filter_map(|v| v.parse().ok()).collect();
        for (what, key, value) in [
            ("move", "origin", format!("{} {} {}", moved[0] + 24.0, moved[1] + 16.0, moved[2])),
            ("color", "_light", "255 120 40 900".to_owned()),
            ("brightness", "_light", "255 120 40 2400".to_owned()),
        ] {
            ents[lamp].set(key, value);
            ents[lamp].set("_lightHDR", "-1 -1 -1 1");
            let r = s.relight(&ents, &o, None, &ctl(), &mut |_| {}).unwrap();
            eprintln!(
                "BENCH {what} {:.3} s (direct {:.3}, bounce {:.3}), {} faces relit, {} sent",
                r.seconds,
                r.direct_seconds,
                r.bounce_seconds,
                r.relit_faces,
                r.faces.len()
            );
        }
        let t = Instant::now();
        let r = s.settle(&o, &ctl(), &mut |_| {}).unwrap();
        eprintln!("BENCH settle {:.3} s, {} faces", t.elapsed().as_secs_f64(), r.relit_faces);
    }

    #[test]
    fn without_bounces_the_direct_light_is_shown() {
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
        let o = RadOptions { bounces: 0, ..opts() };
        let mut s = Session::new(bsp.clone(), &o, &crate::props::NoModels, &mut |_| {}, &ctl()).unwrap();
        let relit = s.relight(&entities(&bsp), &o, None, &ctl(), &mut |_| {}).unwrap();
        let peak = relit.faces.iter().flat_map(|f| f.values.iter()).map(|v| v[0]).fold(0.0f32, f32::max);
        assert!(peak > 1.0, "{peak}");
    }

    #[test]
    fn light_costs_name_their_entities() {
        let lights = light_entity(500, "64 128 128", "") + &light_entity(501, "448 128 96", "");
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &lights));
        let s = session(&bsp);
        let costs = s.light_costs(&entities(&bsp), &opts());
        assert_eq!(costs.len(), 2);
        assert!(costs.iter().all(|c| c.classname == "light" && c.luxels > 0 && c.faces > 0));
        let mut ids: Vec<u32> = costs.iter().filter_map(|c| c.hammerid).collect();
        ids.sort_unstable();
        assert_eq!(ids, [500, 501]);
    }
}
