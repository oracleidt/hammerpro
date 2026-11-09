use std::sync::Arc;
use std::time::{Duration, Instant};

use ahash::{AHashMap, AHashSet};
use glam::{DVec3, Vec3A};
use hp_editor::{Document, Id, SolidNode};
use hp_light::rad::TexLights;
use hp_render::{LightFalloff, LightKind, PreviewLight};
use hp_vmf::Entity;

use crate::producer::{DerivedBuild, DerivedProducer, ProducerChanges, ProducerCx};

use crate::util::parse_floats;

pub(crate) fn appearance(entity: &Entity) -> String {
    let named = entity.get("targetname").is_some_and(|name| !name.trim().is_empty());
    let flags = entity.get("spawnflags").and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0);
    if named && flags & 1 != 0 {
        return "a".into();
    }
    if let Some(pattern) = entity
        .get("pattern")
        .map(str::trim)
        .filter(|p| !p.is_empty() && p.len() <= 256 && p.bytes().all(|c| c.is_ascii_lowercase()))
    {
        return pattern.into();
    }
    let style = entity.get("style").and_then(|s| s.trim().parse::<usize>().ok()).unwrap_or(0);
    if style == 0 { String::new() } else { hp_editor::limits::world_light::default_pattern(style).into() }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LightValue {
    pub rgb: [f64; 3],
    pub brightness: f64,
}

impl LightValue {
    pub fn linear(&self) -> [f32; 3] {
        self.rgb.map(|c| (c / 255.0).max(0.0).powf(2.2) as f32)
    }

    pub fn srgb(&self) -> [f32; 3] {
        self.rgb.map(|c| (c / 255.0).clamp(0.0, 1.0) as f32)
    }

    pub fn intensity(&self) -> f32 {
        (self.brightness / 255.0) as f32
    }

    fn scaled(self, k: f64) -> Self {
        Self { brightness: self.brightness * k, ..self }
    }

    fn key(&self) -> String {
        format!("{} {} {} {}", self.rgb[0], self.rgb[1], self.rgb[2], self.brightness)
    }
}

pub(crate) fn parse_light_value(s: &str, hdr: bool) -> Option<LightValue> {
    let v: Vec<f64> = s.split_ascii_whitespace().take(8).map_while(|p| p.parse::<f64>().ok()).collect();
    let (rgb, brightness) = match v.len() {
        8 if hdr => ([v[4], v[5], v[6]], v[7]),
        4..=8 => ([v[0], v[1], v[2]], v[3]),
        3 => ([v[0], v[1], v[2]], 255.0),
        1 | 2 => ([v[0]; 3], 255.0),
        _ => return None,
    };
    let bad = |c: f64| !c.is_finite() || c < 0.0;
    if rgb.iter().any(|c| bad(*c)) || bad(brightness) {
        return None;
    }
    Some(LightValue { rgb, brightness })
}

fn key_f(e: &Entity, key: &str) -> Option<f64> {
    e.get(key)
        .and_then(|s| s.split_ascii_whitespace().next())
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

fn hdr_scale(e: &Entity, key: &str) -> f64 {
    if e.get(key).is_none() {
        return 1.0;
    }
    key_f(e, key).unwrap_or(0.0).max(0.0)
}

pub(crate) fn light_value(e: &Entity, ldr: &str, hdr_key: &str, scale_key: &str, hdr: bool) -> Option<LightValue> {
    if hdr {
        let v = e
            .get(hdr_key)
            .and_then(|s| parse_light_value(s, true))
            .or_else(|| e.get(ldr).and_then(|s| parse_light_value(s, true)))?;
        Some(v.scaled(hdr_scale(e, scale_key)))
    } else {
        e.get(ldr).and_then(|s| parse_light_value(s, false))
    }
}

pub(crate) fn light_direction(e: &Entity) -> DVec3 {
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let angle = key_f(e, "angle").unwrap_or(0.0);
    let mut out = if angle == -1.0 {
        DVec3::Z
    } else if angle == -2.0 {
        DVec3::NEG_Z
    } else {
        let yaw = if angle != 0.0 { angle } else { angles.y };
        let (sy, cy) = hp_light::lights::sin_cos_degrees(yaw);
        DVec3::new(cy, sy, 0.0)
    };
    let pitch = key_f(e, "pitch").filter(|p| *p != 0.0).unwrap_or(angles.x);
    let (sp, cp) = hp_light::lights::sin_cos_degrees(pitch);
    out = DVec3::new(out.x * cp, out.y * cp, sp);
    out.try_normalize().unwrap_or(DVec3::ZERO)
}

pub(crate) fn target_name(e: &Entity) -> Option<&str> {
    e.get("target").map(str::trim).filter(|t| !t.is_empty())
}

pub(crate) fn aimed_direction(from: DVec3, to: DVec3) -> Option<DVec3> {
    (to - from).try_normalize()
}

pub(crate) fn spot_cones(e: &Entity) -> Option<(f64, f64)> {
    let mut inner = key_f(e, "_inner_cone").unwrap_or(0.0);
    if inner == 0.0 {
        inner = 10.0;
    }
    let mut outer = key_f(e, "_cone").unwrap_or(0.0);
    if outer == 0.0 {
        outer = inner;
    }
    if outer < inner {
        outer = inner;
    }
    if inner == 180.0 && outer == 180.0 {
        return None;
    }
    Some((inner.clamp(0.0, 90.0), outer.clamp(0.0, 90.0)))
}

const MIN_RANGE: f32 = 64.0;
const MAX_RANGE: f32 = 16384.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SunDesc {
    pub ambient: [f32; 3],
    pub ambient_hdr: [f32; 3],
    pub spread_deg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SceneLight {
    pub ldr: Option<PreviewLight>,
    pub hdr: Option<PreviewLight>,
    pub sun: Option<SunDesc>,
}

impl SceneLight {
    pub fn same(l: PreviewLight) -> Self {
        Self { ldr: Some(l), hdr: Some(l), sun: None }
    }

    pub fn pick(&self, hdr: bool) -> Option<PreviewLight> {
        if hdr { self.hdr } else { self.ldr }
    }

    pub fn pos(&self) -> Option<[f32; 3]> {
        self.ldr.or(self.hdr).map(|l| l.pos)
    }

    pub fn ambient(&self, hdr: bool) -> Option<[f32; 3]> {
        self.sun.map(|s| if hdr { s.ambient_hdr } else { s.ambient })
    }

    pub fn moved(self, m: &glam::DMat4) -> Self {
        let turn =
            |d: [f32; 3]| m.transform_vector3(glam::Vec3::from(d).as_dvec3()).normalize_or_zero().as_vec3().to_array();
        let carry = |l: Option<PreviewLight>| {
            l.map(|mut l| {
                l.pos = m.transform_point3(glam::Vec3::from(l.pos).as_dvec3()).as_vec3().to_array();
                l.kind = match l.kind {
                    LightKind::Spot { dir, inner_deg, outer_deg } => {
                        LightKind::Spot { dir: turn(dir), inner_deg, outer_deg }
                    }
                    LightKind::Directional { dir } => LightKind::Directional { dir: turn(dir) },
                    k => k,
                };
                l
            })
        };
        Self { ldr: carry(self.ldr), hdr: carry(self.hdr), ..self }
    }

    pub fn aimed(self, dir: DVec3) -> Self {
        let d = dir.as_vec3().to_array();
        let aim = |l: Option<PreviewLight>| {
            l.map(|mut l| {
                l.kind = match l.kind {
                    LightKind::Spot { inner_deg, outer_deg, .. } => LightKind::Spot { dir: d, inner_deg, outer_deg },
                    LightKind::Directional { .. } => LightKind::Directional { dir: d },
                    k => k,
                };
                l
            })
        };
        Self { ldr: aim(self.ldr), hdr: aim(self.hdr), ..self }
    }
}

fn vrad_light(e: &Entity, value: LightValue, cones: Option<(f64, f64)>) -> Option<hp_light::lights::Light> {
    let mut c = e.clone();
    c.solids.clear();
    c.set("_light", value.key());
    c.remove("_lightHDR");
    match cones {
        Some((inner, outer)) => {
            c.set("_inner_cone", inner.to_string());
            c.set("_cone", outer.to_string());
        }
        None => {
            c.set("_inner_cone", "180");
            c.set("_cone", "180");
        }
    }
    hp_light::lights::parse_light(&c, 1.0)
}

fn local_light(e: &Entity, value: LightValue, spot: bool, origin: [f32; 3], dir: DVec3) -> Option<PreviewLight> {
    let cones = if spot { spot_cones(e) } else { None };
    let vrad = vrad_light(e, value, cones)?;
    let lin = value.linear().iter().fold(0.0f32, |a, b| a.max(*b));
    if lin <= 0.0 {
        return None;
    }
    let intensity = vrad.intensity.max_element() / lin / 255.0;
    if !intensity.is_finite() || intensity <= 0.0 {
        return None;
    }
    let exponent = match vrad.kind {
        hp_light::lights::LightKind::Spot { exponent, .. } => exponent,
        _ => 0.0,
    };
    let falloff = LightFalloff {
        constant: vrad.constant,
        linear: vrad.linear,
        quadratic: vrad.quadratic,
        cap: vrad.cap,
        cutoff: vrad.cutoff,
        fade: [vrad.fade.0, vrad.fade.1],
        exponent,
    };
    let kind = match (vrad.kind, cones) {
        (hp_light::lights::LightKind::Spot { .. }, Some((inner, outer))) => {
            let outer = (outer as f32).clamp(0.1, 179.0);
            LightKind::Spot {
                dir: dir.as_vec3().to_array(),
                inner_deg: (inner as f32).clamp(0.0, outer),
                outer_deg: outer,
            }
        }
        _ => LightKind::Point,
    };
    Some(PreviewLight {
        kind,
        pos: origin,
        color: value.linear(),
        intensity,
        range: vrad.reach.clamp(MIN_RANGE, MAX_RANGE),
        falloff: Some(falloff),
    })
}

fn sun_light(value: Option<LightValue>, origin: [f32; 3], dir: DVec3) -> PreviewLight {
    let (color, intensity) = value.map_or(([0.0; 3], 0.0), |v| (v.linear(), v.intensity()));
    PreviewLight {
        kind: LightKind::Directional { dir: dir.as_vec3().to_array() },
        pos: origin,
        color,
        intensity,
        range: MAX_RANGE,
        falloff: None,
    }
}

fn sky_ambient(e: &Entity, sun: Option<LightValue>, hdr: bool) -> [f32; 3] {
    let own = if hdr {
        e.get("_ambientHDR")
            .and_then(|s| parse_light_value(s, true))
            .or_else(|| e.get("_ambient").and_then(|s| parse_light_value(s, true)))
    } else {
        e.get("_ambient").and_then(|s| parse_light_value(s, false))
    };
    let value = match own {
        Some(v) => v,
        None => match sun {
            Some(s) => s.scaled(0.5),
            None => return [0.0; 3],
        },
    };
    let value = if hdr { value.scaled(hdr_scale(e, "_AmbientScaleHDR")) } else { value };
    let k = value.intensity();
    value.linear().map(|c| c * k)
}

pub(crate) fn casts_no_shadow(e: &Entity) -> bool {
    e.get("disableshadows").and_then(|v| v.trim().parse::<f64>().ok()).is_some_and(|v| v != 0.0)
}

pub(crate) fn casts_game_shadow(e: &Entity) -> bool {
    e.get("model").map(str::trim).is_some_and(|m| {
        let m = m.replace('\\', "/").to_ascii_lowercase();
        m.ends_with(".mdl") && !m.starts_with("models/editor/")
    })
}

pub(crate) fn is_sky_material(name: &str) -> bool {
    name.to_ascii_lowercase().contains("toolsskybox")
}

pub(crate) fn brush_casts_no_shadow(e: &Entity) -> bool {
    !e.classname.eq_ignore_ascii_case("func_detail")
        && !e.get("vrad_brush_cast_shadows").and_then(|v| v.trim().parse::<f64>().ok()).is_some_and(|v| v != 0.0)
}

const DLIGHT_MIN_LIGHT: f32 = 20.0 / 256.0;

fn dynamic_direction(e: &Entity) -> DVec3 {
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let key = e.get("pitch").and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v != 0.0);
    let pitch = key.map_or(angles.x, |p| -p);
    let (sp, cp) = hp_light::lights::sin_cos_degrees(pitch);
    let (sy, cy) = angles.y.to_radians().sin_cos();
    DVec3::new(cp * cy, cp * sy, -sp)
}

fn dynamic_light(e: &Entity) -> (Option<PreviewLight>, Option<[f32; 3]>) {
    let (rgb, n) = parse_floats::<4>(e.get("_light").unwrap_or("255 255 255"));
    if n < 3 || rgb[..3].iter().any(|c| !c.is_finite() || *c < 0.0) {
        return (None, None);
    }
    let key = |k: &str| e.get(k).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    let srgb = [rgb[0], rgb[1], rgb[2]].map(|c| (c / 255.0).clamp(0.0, 1.0) as f32);
    let flags = key("spawnflags").unwrap_or(0.0) as i64;
    let radius = key("distance").unwrap_or(120.0) as f32;
    if flags & 3 == 3 || radius <= 0.0 {
        return (None, Some(srgb));
    }
    let exponent = key("brightness").unwrap_or(0.0).clamp(-16.0, 16.0);
    let scale = 2f64.powf(exponent);
    let color = [rgb[0], rgb[1], rgb[2]].map(|c| (c / 255.0 * scale) as f32);
    let strength = color.iter().fold(0.0f32, |a, b| a.max(*b));
    if !strength.is_finite() || strength <= 0.0 {
        return (None, Some(srgb));
    }
    let radius = radius.min(MAX_RANGE);
    let falloff = LightFalloff {
        quadratic: 1.0 / (DLIGHT_MIN_LIGHT * radius * radius).max(1e-6),
        cap: 1.0e9,
        cutoff: radius,
        exponent: 1.0,
        ..LightFalloff::default()
    };
    let outer = key("_cone").unwrap_or(0.0) as f32;
    let kind = if outer > 0.0 {
        let outer = outer.clamp(0.1, 179.0);
        let inner = (key("_inner_cone").unwrap_or(0.0) as f32).clamp(0.0, outer);
        LightKind::Spot { dir: dynamic_direction(e).as_vec3().to_array(), inner_deg: inner, outer_deg: outer }
    } else {
        LightKind::Point
    };
    let origin = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
    let light = PreviewLight {
        kind,
        pos: origin.as_vec3().to_array(),
        color: color.map(|c| c / strength),
        intensity: strength,
        range: radius.max(1.0),
        falloff: Some(falloff),
    };
    (Some(light), Some(srgb))
}

pub(crate) fn is_sun_class(class: &str) -> bool {
    class == "light_environment"
}

pub(crate) fn parse_light(e: &Entity) -> (Option<SceneLight>, Option<[f32; 3]>) {
    let class = e.classname.as_str();
    match class {
        "light" | "light_spot" | "light_environment" => {}
        "light_dynamic" => {
            let (light, srgb) = dynamic_light(e);
            return (light.map(SceneLight::same), srgb);
        }
        _ => return (None, None),
    }
    let origin = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO).as_vec3().to_array();
    let dir = light_direction(e);
    let ldr = light_value(e, "_light", "_lightHDR", "_lightscaleHDR", false);
    let hdr = light_value(e, "_light", "_lightHDR", "_lightscaleHDR", true);
    let srgb = ldr.or(hdr).map(|v| v.srgb());
    let aimless = dir == DVec3::ZERO;
    if is_sun_class(class) {
        let sun = SunDesc {
            ambient: sky_ambient(e, ldr, false),
            ambient_hdr: sky_ambient(e, hdr, true),
            spread_deg: key_f(e, "SunSpreadAngle").unwrap_or(0.0).clamp(0.0, 90.0) as f32,
        };
        let (ldr, hdr) = if aimless { (None, None) } else { (ldr, hdr) };
        let light = SceneLight {
            ldr: Some(sun_light(ldr, origin, dir)),
            hdr: Some(sun_light(hdr, origin, dir)),
            sun: Some(sun),
        };
        return (Some(light), srgb);
    }
    let spot = class == "light_spot";
    if spot && aimless {
        return (None, srgb);
    }
    let ldr = ldr.and_then(|v| local_light(e, v, spot, origin, dir));
    let hdr = hdr.and_then(|v| local_light(e, v, spot, origin, dir));
    let light = (ldr.is_some() || hdr.is_some()).then_some(SceneLight { ldr, hdr, sun: None });
    (light, srgb)
}

impl crate::AssetManager {
    pub(crate) fn update_buried_lights(&mut self, doc: &Document) {
        let mut buried = AHashSet::new();
        let mut pending = false;
        for (id, s) in &self.scene.ents {
            let Some(light) = s.desc.light else { continue };
            if light.sun.is_some() {
                continue;
            }
            let Some(pos) = light.pos() else { continue };
            let p = DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
            if !p.is_finite() {
                continue;
            }
            let around = hp_geom::Aabb::new(p - DVec3::splat(0.5), p + DVec3::splat(0.5));
            for sid in doc.solids_in_box(&around) {
                let Some(node) = doc.scene().solid(sid) else { continue };
                if !in_world_leaves(doc, node) || !strictly_inside(node, p) {
                    continue;
                }
                match self.brush_opaque(node) {
                    Some(true) => {
                        buried.insert(*id);
                        break;
                    }
                    Some(false) => {}
                    None => pending = true,
                }
            }
        }
        self.scene.buried_pending = pending;
        if buried != self.scene.buried_lights {
            self.scene.buried_lights = buried;
            self.scene.lights_dirty = true;
        }
    }

    fn brush_opaque(&self, node: &SolidNode) -> Option<bool> {
        use hp_vmt::ToolKind;
        for side in &node.solid.sides {
            let key = crate::util::material_key(&side.material);
            let Some(entry) = self.mats.analyses.get(&key) else { continue };
            let Some(a) = entry.value.as_ref()? else { continue };
            if a.water.is_some() || a.flags.intersects(hp_render::MaterialFlags::TRANSLUCENT) {
                return Some(false);
            }
            match a.tool {
                None | Some(ToolKind::Nodraw | ToolKind::Skybox | ToolKind::Skybox2d | ToolKind::BlockLight) => {}
                Some(_) => return Some(false),
            }
        }
        Some(true)
    }
}

fn in_world_leaves(doc: &Document, node: &SolidNode) -> bool {
    if node.disp.iter().any(Option::is_some) || !node.geom.valid {
        return false;
    }
    match node.owner {
        hp_editor::Owner::World => true,
        hp_editor::Owner::Entity(e) => {
            doc.scene().entity(e).is_some_and(|n| n.entity.classname.eq_ignore_ascii_case("func_detail"))
        }
    }
}

fn strictly_inside(node: &SolidNode, p: DVec3) -> bool {
    !node.geom.faces.is_empty() && node.geom.faces.iter().all(|f| f.plane.distance(p) < -0.01)
}

pub(crate) fn producers() -> Vec<Box<dyn DerivedProducer>> {
    vec![Box::new(TexlightProducer::default())]
}

const TEXLIGHT_BUDGET: Duration = Duration::from_millis(3);
const TEXLIGHT_SLICE: usize = 256;

#[derive(Default)]
struct TexlightProducer {
    rad: Option<(u64, Vec<std::path::PathBuf>, Arc<TexLights>)>,
    values: AHashMap<String, Option<glam::Vec3>>,
    by_solid: AHashMap<Id, Vec<PreviewLight>>,
    queue: Vec<Id>,
    queued: AHashSet<Id>,
    on: bool,
    changed: bool,
}

fn rad_files(doc: &Document) -> Vec<std::path::PathBuf> {
    let map = doc.path();
    let mut files: Vec<std::path::PathBuf> = map.map(|p| p.with_extension("rad")).into_iter().collect();
    let custom = doc.light_plus().texlight;
    let custom = custom.trim();
    if !custom.is_empty() {
        let p = std::path::PathBuf::from(custom);
        if p.is_absolute() {
            files.push(p);
        } else if let Some(dir) = map.and_then(|m| m.parent()) {
            files.push(dir.join(p));
        }
    }
    files
}

impl TexlightProducer {
    fn queue_all(&mut self, doc: &Document) {
        self.by_solid.clear();
        self.queued.clear();
        self.queue = doc.scene().solids().keys().copied().collect();
        self.queue.sort_unstable();
        self.queued.extend(self.queue.iter().copied());
        self.changed = true;
    }
}

impl DerivedProducer for TexlightProducer {
    fn name(&self) -> &'static str {
        "texlights"
    }

    fn changed(&mut self, ch: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        let on = cx.opts.lighting_preview;
        if !on {
            if self.on {
                *self = Self { rad: self.rad.take(), changed: true, ..Self::default() };
            }
            return self.changed;
        }
        let files = rad_files(cx.doc);
        let stale = self.rad.as_ref().is_none_or(|(g, f, _)| *g != cx.assets.generation || *f != files);
        if !self.on || ch.full || stale {
            self.on = true;
            if stale {
                self.rad = None;
                self.values.clear();
            }
            self.queue_all(cx.doc);
        } else {
            for &id in ch.doc.solids_changed.iter().chain(&ch.doc.solids_removed) {
                if self.queued.insert(id) {
                    self.queue.push(id);
                }
            }
        }
        !self.queue.is_empty() || self.changed
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let mut out = DerivedBuild::default();
        if self.on && self.rad.is_none() {
            let files = rad_files(cx.doc);
            let extra: Vec<std::path::PathBuf> = files.iter().filter(|p| p.is_file()).cloned().collect();
            let fs = cx.assets.fs.as_ref();
            let rad = hp_light::rad::load(fs, fs.and_then(hp_fs::GameFs::game_dir), &extra);
            self.rad = Some((cx.assets.generation, files, Arc::new(rad)));
        }
        if let Some((_, _, rad)) = self.rad.clone()
            && self.on
        {
            let start = Instant::now();
            let scene = cx.doc.scene();
            while !self.queue.is_empty() {
                let n = self.queue.len().saturating_sub(TEXLIGHT_SLICE);
                for id in self.queue.split_off(n) {
                    self.queued.remove(&id);
                    let values = &mut self.values;
                    let mut value = |name: &str| match values.get(name) {
                        Some(v) => *v,
                        None => {
                            let v = rad.get(&hp_light::rad::normalize_material(name));
                            values.insert(name.to_owned(), v);
                            v
                        }
                    };
                    let lights = match scene.solid(id) {
                        Some(node) if !rad.is_empty() => solid_texlights(node, &mut value),
                        _ => Vec::new(),
                    };
                    let had =
                        if lights.is_empty() { self.by_solid.remove(&id) } else { self.by_solid.insert(id, lights) };
                    self.changed |= had.is_some() || self.by_solid.contains_key(&id);
                }
                if start.elapsed() >= TEXLIGHT_BUDGET {
                    break;
                }
            }
        }
        if self.changed {
            self.changed = false;
            let mut ids: Vec<&Id> = self.by_solid.keys().collect();
            ids.sort_unstable();
            out.lights =
                Some(ids.into_iter().flat_map(|id| self.by_solid[id].iter().copied().map(SceneLight::same)).collect());
        }
        out
    }
}

fn solid_texlights(node: &SolidNode, value: &mut impl FnMut(&str) -> Option<glam::Vec3>) -> Vec<PreviewLight> {
    if node.disp.iter().any(Option::is_some) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for face in &node.geom.faces {
        let Some(side) = node.solid.sides.get(face.side_index) else { continue };
        let Some(v) = value(&side.material) else { continue };
        let Some(light) = face_light(&face.vertices, face.normal(), v) else { continue };
        out.push(light);
    }
    out
}

fn face_light(vertices: &[DVec3], normal: DVec3, value: glam::Vec3) -> Option<PreviewLight> {
    if vertices.len() < 3 || !normal.is_finite() {
        return None;
    }
    let center = vertices.iter().copied().sum::<DVec3>() / vertices.len() as f64;
    let mut area = DVec3::ZERO;
    for i in 1..vertices.len() - 1 {
        area += (vertices[i] - vertices[0]).cross(vertices[i + 1] - vertices[0]);
    }
    let area = (area.length() * 0.5) as f32;
    if !center.is_finite() || !area.is_finite() || area <= 0.0 {
        return None;
    }
    let n = Vec3A::from(normal.as_vec3());
    let origin = Vec3A::from(center.as_vec3()) + n;
    let vrad = hp_light::lights::surface_light(origin, n, Vec3A::from(value), area, 1.0);
    let strength = vrad.intensity.max_element();
    if !strength.is_finite() || strength <= 0.0 {
        return None;
    }
    Some(PreviewLight {
        kind: LightKind::Spot { dir: n.to_array(), inner_deg: 0.0, outer_deg: 90.0 },
        pos: origin.to_array(),
        color: (vrad.intensity / strength).to_array(),
        intensity: strength / 255.0,
        range: vrad.reach.clamp(MIN_RANGE, MAX_RANGE),
        falloff: Some(LightFalloff {
            constant: vrad.constant,
            linear: vrad.linear,
            quadratic: vrad.quadratic,
            cap: vrad.cap,
            cutoff: vrad.cutoff,
            fade: [vrad.fade.0, vrad.fade.1],
            exponent: 0.0,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lights_parented_to_a_train_move_and_turn_with_it() {
        let spot = PreviewLight {
            kind: LightKind::Spot { dir: [1.0, 0.0, 0.0], inner_deg: 10.0, outer_deg: 20.0 },
            pos: [10.0, 0.0, 0.0],
            color: [1.0; 3],
            intensity: 1.0,
            range: 100.0,
            falloff: None,
        };
        let m = glam::DMat4::from_translation(DVec3::new(0.0, 0.0, 50.0))
            * glam::DMat4::from_rotation_z(std::f64::consts::FRAC_PI_2);
        let moved = SceneLight::same(spot).moved(&m).pick(false).expect("light");
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(close(moved.pos, [0.0, 10.0, 50.0]), "{:?}", moved.pos);
        match moved.kind {
            LightKind::Spot { dir, .. } => assert!(close(dir, [0.0, 1.0, 0.0]), "{dir:?}"),
            k => panic!("{k:?}"),
        }
    }

    fn ent(class: &str, props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    fn ldr(l: Option<SceneLight>) -> PreviewLight {
        l.and_then(|l| l.ldr).expect("LDR light")
    }

    fn hdr(l: Option<SceneLight>) -> PreviewLight {
        l.and_then(|l| l.hdr).expect("HDR light")
    }

    fn sun_dir(l: &PreviewLight) -> [f32; 3] {
        match l.kind {
            LightKind::Directional { dir } => dir,
            k => panic!("sun expected, got {k:?}"),
        }
    }

    fn close(a: [f32; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (f64::from(*x) - y).abs() < 1e-4)
    }

    #[test]
    fn light_value_strings_follow_vrad() {
        let v = parse_light_value("255 255 255 200", false).unwrap();
        assert_eq!(v.linear(), [1.0; 3]);
        assert!((v.intensity() - 200.0 / 255.0).abs() < 1e-6);
        let v = parse_light_value("128", false).unwrap();
        assert_eq!(v.intensity(), 1.0);
        assert!((v.linear()[0] - (128.0f64 / 255.0).powf(2.2) as f32).abs() < 1e-6);
        assert_eq!(parse_light_value("255 128 0", false).map(|v| v.intensity()), Some(1.0), "r g b is brightness 255");
        assert_eq!(parse_light_value("64 32", false).map(|v| v.rgb), Some([64.0; 3]), "two numbers read like one");
        let both = "255 0 0 100 0 0 255 400";
        assert_eq!(parse_light_value(both, false).map(|v| (v.rgb, v.brightness)), Some(([255.0, 0.0, 0.0], 100.0)));
        assert_eq!(parse_light_value(both, true).map(|v| (v.rgb, v.brightness)), Some(([0.0, 0.0, 255.0], 400.0)));
        assert_eq!(parse_light_value("10 20 30 40 50", false).map(|v| v.brightness), Some(40.0));
        assert_eq!(parse_light_value("-1 -1 -1 1", true), None);
        assert_eq!(parse_light_value("255 255 255 -5", false), None);
        assert_eq!(parse_light_value("", false), None);
        assert_eq!(parse_light_value("bright", false), None);
        assert_eq!(parse_light_value("255 nan 0 1", false), None);
        assert_eq!(parse_light_value("255 255 255 200 garbage", false).map(|v| v.brightness), Some(200.0));
    }

    #[test]
    fn hdr_keys_pick_the_game_values() {
        let e = ent("light", &[("_light", "255 255 255 200"), ("_lightHDR", "-1 -1 -1 1"), ("_lightscaleHDR", "2")]);
        assert_eq!(light_value(&e, "_light", "_lightHDR", "_lightscaleHDR", false).map(|v| v.brightness), Some(200.0));
        assert_eq!(light_value(&e, "_light", "_lightHDR", "_lightscaleHDR", true).map(|v| v.brightness), Some(400.0));
        let own =
            ent("light", &[("_light", "255 255 255 200"), ("_lightHDR", "255 0 0 50"), ("_lightscaleHDR", "0.5")]);
        let v = light_value(&own, "_light", "_lightHDR", "_lightscaleHDR", true).unwrap();
        assert_eq!((v.rgb, v.brightness), ([255.0, 0.0, 0.0], 25.0));
        let unscaled = ent("light", &[("_light", "255 255 255 200")]);
        assert_eq!(
            light_value(&unscaled, "_light", "_lightHDR", "_lightscaleHDR", true).map(|v| v.brightness),
            Some(200.0)
        );
        let hdr_only = ent("light", &[("_lightHDR", "255 0 0 100")]);
        let (l, _) = parse_light(&hdr_only);
        assert!(l.is_some_and(|l| l.ldr.is_none() && l.hdr.is_some()), "LDR compiles read only _light");
        let point = ent("light", &[("_light", "255 255 255 100"), ("_lightscaleHDR", "3"), ("_quadratic_attn", "1")]);
        let (l, _) = parse_light(&point);
        assert!((hdr(l).intensity / ldr(l).intensity - 3.0).abs() < 1e-4);
    }

    #[test]
    fn local_lights() {
        let point = ent("light", &[("origin", "10 20 30"), ("_light", "255 255 255 255")]);
        let (l, srgb) = parse_light(&point);
        assert!(l.is_some_and(|l| l.sun.is_none()));
        let l = ldr(l);
        assert_eq!(l.kind, LightKind::Point);
        assert_eq!(l.pos, [10.0, 20.0, 30.0]);
        assert_eq!(l.range, 16384.0);
        assert_eq!(srgb, Some([1.0; 3]));
        let at = |l: &PreviewLight, d: f32| {
            let f = l.falloff.expect("VRAD terms");
            l.intensity / (f.constant + d * (f.linear + d * f.quadratic))
        };
        let quad = ent("light", &[("_light", "255 255 255 200"), ("_quadratic_attn", "1")]);
        let q = ldr(parse_light(&quad).0);
        assert!((at(&q, 100.0) - 200.0 / 255.0).abs() < 1e-4, "{}", at(&q, 100.0));
        assert!((at(&q, 200.0) * 4.0 - 200.0 / 255.0).abs() < 1e-4);
        assert!((q.range - 2000.0).abs() < 1.0, "{}", q.range);
        let lin = ent("light", &[("_light", "255 255 255 200"), ("_linear_attn", "1")]);
        let l = ldr(parse_light(&lin).0);
        assert!((at(&l, 100.0) - 2.0 * at(&l, 200.0)).abs() < 1e-4, "linear falloff");
        let fifty = ent(
            "light",
            &[("_light", "255 255 255 200"), ("_fifty_percent_distance", "256"), ("_zero_percent_distance", "512")],
        );
        let f = ldr(parse_light(&fifty).0);
        assert!((at(&f, 256.0) / f.intensity - 0.5).abs() < 0.01, "half the light at the fifty percent distance");
        assert!(at(&f, 512.0) < at(&f, 256.0));
        let hard = ent(
            "light",
            &[
                ("_light", "255 255 255 200"),
                ("_fifty_percent_distance", "100"),
                ("_zero_percent_distance", "200"),
                ("_hardfalloff", "1"),
            ],
        );
        assert!(ldr(parse_light(&hard).0).falloff.is_some_and(|f| f.fade[1] == 200.0));
        let cut = ent("light", &[("_light", "255 255 255 200"), ("_distance", "300")]);
        assert!(ldr(parse_light(&cut).0).falloff.is_some_and(|f| f.cutoff == 300.0));
        let dark = ent("light", &[("_light", "255 255 255 200"), ("targetname", "a"), ("spawnflags", "1")]);
        assert!(parse_light(&dark).0.is_none(), "initially dark named lights stay off");
        let unnamed = ent("light", &[("_light", "255 255 255 200"), ("spawnflags", "1")]);
        assert!(parse_light(&unnamed).0.is_some(), "a light without a name cannot be switched off");
        let styled = ent("light", &[("_light", "255 255 255 200"), ("targetname", "a"), ("style", "33")]);
        assert!(parse_light(&styled).0.is_some());
        assert!(parse_light(&ent("light", &[("_light", "-1 -1 -1 1")])).0.is_none());
        assert!(parse_light(&ent("light", &[("_light", "0 0 0 200")])).0.is_none());
        assert!(parse_light(&ent("light", &[("_light", "255 255 255 0")])).0.is_none());
        assert!(parse_light(&ent("info_target", &[("_light", "255 255 255 200")])).0.is_none());
        for class in ["light_directional", "light_glspot", "light_foo", "LIGHT", "Light_Spot", "LIGHT_ENVIRONMENT"] {
            let (l, _) = parse_light(&ent(class, &[("_light", "255 255 255 200"), ("pitch", "-90")]));
            assert!(l.is_none(), "VRAD only lights the exact classes light, light_spot and light_environment: {class}");
        }
        let far = ent("light", &[("origin", "1e30 0 0"), ("_light", "255 255 255 200")]);
        assert!(parse_light(&far).0.is_some_and(|l| l.ldr.is_some_and(|l| l.pos.iter().all(|v| v.is_finite()))));
        let nan = ent("light", &[("origin", "nan 0 0"), ("angles", "nan nan 0"), ("_light", "255 255 255 200")]);
        assert!(parse_light(&nan).0.is_some_and(|l| l.ldr.is_some_and(|l| l.pos == [0.0; 3])));
    }

    #[test]
    fn spot_cones_follow_vrad() {
        let spot = ent(
            "light_spot",
            &[
                ("_light", "255 255 255 200"),
                ("pitch", "-90"),
                ("_cone", "60"),
                ("_inner_cone", "30"),
                ("_exponent", "2"),
            ],
        );
        let l = ldr(parse_light(&spot).0);
        let LightKind::Spot { dir, inner_deg, outer_deg } = l.kind else { panic!("spot expected") };
        assert!((dir[2] + 1.0).abs() < 1e-5, "{dir:?}");
        assert!((inner_deg - 30.0).abs() < 1e-3 && (outer_deg - 60.0).abs() < 1e-3, "{inner_deg} {outer_deg}");
        assert_eq!(l.falloff.map(|f| f.exponent), Some(2.0));
        let swapped = ent("light_spot", &[("_light", "255 255 255 200"), ("_cone", "60"), ("_inner_cone", "70")]);
        let LightKind::Spot { inner_deg, outer_deg, .. } = ldr(parse_light(&swapped).0).kind else { panic!("spot") };
        assert!((inner_deg - 70.0).abs() < 1e-3 && (outer_deg - 70.0).abs() < 1e-3, "{inner_deg} {outer_deg}");
        let wide = ent("light_spot", &[("_light", "255 255 255 200"), ("_cone", "120"), ("_inner_cone", "95")]);
        let LightKind::Spot { inner_deg, outer_deg, .. } = ldr(parse_light(&wide).0).kind else { panic!("spot") };
        assert_eq!((inner_deg, outer_deg), (90.0, 90.0), "VRAD clamps the cones to 90 degrees");
        let half = ent("light_spot", &[("_light", "255 255 255 200"), ("_cone", "180"), ("_inner_cone", "45")]);
        let LightKind::Spot { outer_deg, .. } = ldr(parse_light(&half).0).kind else { panic!("still a spot") };
        assert_eq!(outer_deg, 90.0);
        let round = ent("light_spot", &[("_light", "255 255 255 200"), ("_cone", "180"), ("_inner_cone", "180")]);
        assert_eq!(ldr(parse_light(&round).0).kind, LightKind::Point, "180 and 180 make a point light");
        let defaults = ent("light_spot", &[("_light", "255 255 255 200")]);
        let LightKind::Spot { inner_deg, outer_deg, .. } = ldr(parse_light(&defaults).0).kind else { panic!("spot") };
        assert_eq!((inner_deg, outer_deg), (10.0, 10.0));
        let point = ent("light", &[("_light", "255 255 255 200"), ("_quadratic_attn", "1"), ("_cone", "30")]);
        let p = ldr(parse_light(&point).0);
        assert_eq!((p.kind, p.falloff.map(|f| f.exponent)), (LightKind::Point, Some(0.0)));
        let down = ent("light_spot", &[("angle", "-2"), ("_light", "1 1 1")]);
        assert_eq!(light_direction(&down), DVec3::ZERO, "VRAD overwrites the vertical with the pitch, 0 by default");
        assert!(parse_light(&down).0.is_none(), "a spot that points nowhere lights nothing");
        let down_pitched = ent("light_spot", &[("angle", "-2"), ("pitch", "-90"), ("_light", "1 1 1")]);
        assert!((light_direction(&down_pitched) - DVec3::NEG_Z).length() < 1e-9);
        let up = ent("light_spot", &[("angle", "-1"), ("pitch", "90"), ("_light", "1 1 1")]);
        assert!((light_direction(&up) - DVec3::Z).length() < 1e-9);
        let blind_sun =
            ent("light_environment", &[("angle", "-2"), ("_light", "255 255 255 200"), ("_ambient", "50 50 50 100")]);
        let blind = parse_light(&blind_sun).0.expect("the sky still lights");
        assert_eq!(ldr(Some(blind)).intensity, 0.0);
        assert!(blind.sun.is_some_and(|s| s.ambient[0] > 0.0));
        let up_tilted = ent("light_spot", &[("angle", "-1"), ("pitch", "-90")]);
        assert!((light_direction(&up_tilted) - DVec3::NEG_Z).length() < 1e-9, "VRAD applies the pitch after angle -1");
    }

    #[test]
    fn sun_direction_follows_vrad() {
        let construct = ent(
            "light_environment",
            &[("angles", "0 306 0"), ("pitch", "-56"), ("_light", "242 234 218 560"), ("_ambient", "114 136 148 420")],
        );
        let (cp, sp) = (56f64.to_radians().cos(), -56f64.to_radians().sin());
        let (cy, sy) = (306f64.to_radians().cos(), 306f64.to_radians().sin());
        let l = ldr(parse_light(&construct).0);
        assert!(close(sun_dir(&l), [cp * cy, cp * sy, sp]), "{:?}", sun_dir(&l));
        assert!(sun_dir(&l)[2] < -0.8, "a negative pitch points down");
        let kasty = ent("light_environment", &[("angles", "0 0 0"), ("pitch", "-120"), ("_light", "136 151 255 180")]);
        let d = sun_dir(&ldr(parse_light(&kasty).0));
        assert!(close(d, [-0.5, 0.0, -(120f64.to_radians().sin())]), "pitch past 90 turns the sun around: {d:?}");
        let angles_only =
            ent("light_environment", &[("angles", "-45 90 0"), ("pitch", "0"), ("_light", "255 255 255 200")]);
        let d = sun_dir(&ldr(parse_light(&angles_only).0));
        assert!(
            close(d, [0.0, 45f64.to_radians().cos(), -(45f64.to_radians().sin())]),
            "pitch 0 reads the angles: {d:?}"
        );
        let key_wins =
            ent("light_environment", &[("angles", "-10 0 0"), ("pitch", "-80"), ("_light", "255 255 255 200")]);
        assert!(sun_dir(&ldr(parse_light(&key_wins).0))[2] < -0.98);
        let yaw =
            ent("light_environment", &[("angle", "90"), ("angles", "0 0 0"), ("pitch", "-30"), ("_light", "1 1 1")]);
        assert!(close(sun_dir(&ldr(parse_light(&yaw).0)), [0.0, 30f64.to_radians().cos(), -0.5]));
        let aimed = parse_light(&construct).0.unwrap().aimed(DVec3::new(0.0, 0.0, -1.0));
        assert_eq!(sun_dir(&aimed.ldr.unwrap()), [0.0, 0.0, -1.0]);
        assert_eq!(aimed_direction(DVec3::ZERO, DVec3::new(0.0, 10.0, 0.0)), Some(DVec3::Y));
        assert_eq!(aimed_direction(DVec3::ONE, DVec3::ONE), None);
    }

    #[test]
    fn sun_colors_ambient_and_spread() {
        let sun = ent(
            "light_environment",
            &[
                ("_light", "255 255 255 300"),
                ("_ambient", "255 255 255 20"),
                ("_lightHDR", "-1 -1 -1 1"),
                ("_lightscaleHDR", "0.5"),
                ("_ambientHDR", "-1 -1 -1 1"),
                ("_AmbientScaleHDR", "2"),
                ("SunSpreadAngle", "5"),
            ],
        );
        let (l, _) = parse_light(&sun);
        let s = l.unwrap();
        assert!((ldr(l).intensity - 300.0 / 255.0).abs() < 1e-5);
        assert!((hdr(l).intensity - 150.0 / 255.0).abs() < 1e-5);
        let d = s.sun.unwrap();
        assert!((d.ambient[0] - 20.0 / 255.0).abs() < 1e-5);
        assert!((d.ambient_hdr[0] - 40.0 / 255.0).abs() < 1e-5);
        assert_eq!(d.spread_deg, 5.0);
        assert_eq!(s.ambient(true), Some(d.ambient_hdr));
        assert!(ldr(l).falloff.is_none());
        let no_ambient = ent("light_environment", &[("_light", "255 255 255 200")]);
        let d = parse_light(&no_ambient).0.unwrap().sun.unwrap();
        assert!((d.ambient[0] - 100.0 / 255.0).abs() < 1e-5, "no _ambient: half the sun like the compiler");
        let dark_sun = ent("light_environment", &[("_light", "0 0 0 0"), ("_ambient", "50 60 70 200")]);
        let d = parse_light(&dark_sun).0.expect("the sky still lights").sun.unwrap();
        assert!(d.ambient[2] > 0.0);
        let night = ent("light_environment", &[("_ambient", "18 48 103 150")]);
        let s = parse_light(&night).0.unwrap();
        assert_eq!(ldr(Some(s)).intensity, 0.0);
        assert!(s.sun.unwrap().ambient[2] > 0.0);
        let wild = ent("light_environment", &[("_light", "255 255 255 200"), ("SunSpreadAngle", "720")]);
        assert_eq!(parse_light(&wild).0.unwrap().sun.unwrap().spread_deg, 90.0);
    }

    #[test]
    fn dynamic_lights() {
        let e = ent(
            "light_dynamic",
            &[("origin", "0 0 100"), ("_light", "255 128 0 200"), ("brightness", "2"), ("distance", "300")],
        );
        let (l, srgb) = parse_light(&e);
        assert!(l.is_some_and(|l| l.ldr == l.hdr && l.sun.is_none()));
        let l = ldr(l);
        assert_eq!(srgb, Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(l.kind, LightKind::Point);
        assert!((l.intensity - 4.0).abs() < 1e-5, "{}", l.intensity);
        assert!((l.color[1] - 128.0 / 255.0).abs() < 1e-5);
        assert_eq!(l.range, 300.0);
        let f = l.falloff.expect("falloff");
        assert_eq!(f.cutoff, 300.0);
        let at = |d: f32| l.intensity / (f.quadratic * d * d);
        assert!((at(300.0) - 4.0 * DLIGHT_MIN_LIGHT).abs() < 1e-4, "{}", at(300.0));
        assert!(at(100.0) > at(200.0) * 3.9);
        let spot = ent(
            "light_dynamic",
            &[("_light", "255 255 255"), ("pitch", "-90"), ("_inner_cone", "20"), ("_cone", "40"), ("distance", "200")],
        );
        let LightKind::Spot { dir, inner_deg, outer_deg } = ldr(parse_light(&spot).0).kind else {
            panic!("spot expected")
        };
        assert!((dir[2] + 1.0).abs() < 1e-5, "{dir:?}");
        assert_eq!((inner_deg, outer_deg), (20.0, 40.0));
        let angled = ent("light_dynamic", &[("angles", "45 90 0"), ("_cone", "30")]);
        let LightKind::Spot { dir, .. } = ldr(parse_light(&angled).0).kind else { panic!("spot") };
        assert!(dir[2] < -0.7 && dir[1] > 0.7, "{dir:?}");
        assert!(parse_light(&ent("light_dynamic", &[("distance", "0")])).0.is_none());
        assert!(parse_light(&ent("light_dynamic", &[("spawnflags", "3")])).0.is_none());
        assert!(parse_light(&ent("light_dynamic", &[("_light", "0 0 0")])).0.is_none());
        assert!(parse_light(&ent("light_dynamic", &[("_light", "nan 1 1"), ("brightness", "inf")])).0.is_none());
    }

    #[test]
    fn shadows_follow_disableshadows() {
        assert!(!casts_no_shadow(&ent("prop_static", &[])));
        assert!(!casts_no_shadow(&ent("prop_static", &[("disableshadows", "0")])));
        assert!(casts_no_shadow(&ent("prop_static", &[("disableshadows", "1")])));
        assert!(casts_no_shadow(&ent("prop_physics", &[("disableshadows", " 1 ")])));
        assert!(!casts_no_shadow(&ent("prop_dynamic", &[("disableshadows", "x")])));
        assert!(casts_game_shadow(&ent("prop_static", &[("model", "models/props_c17/oildrum001.mdl")])));
        assert!(casts_game_shadow(&ent("prop_dynamic", &[("model", "Models\\Props\\Crate.MDL")])));
        assert!(!casts_game_shadow(&ent("info_player_start", &[])), "editor helpers cast no shadow");
        assert!(!casts_game_shadow(&ent("info_node", &[("model", "models/editor/ground_node.mdl")])));
        assert!(!casts_game_shadow(&ent("env_sprite", &[("model", "sprites/glow01.vmt")])));
        assert!(is_sky_material("TOOLS/TOOLSSKYBOX") && is_sky_material("tools/toolsskybox2d"));
        assert!(!is_sky_material("tools/toolsnodraw"));
        assert!(!brush_casts_no_shadow(&ent("func_detail", &[])));
        assert!(brush_casts_no_shadow(&ent("func_door", &[])));
        assert!(brush_casts_no_shadow(&ent("func_brush", &[("vrad_brush_cast_shadows", "0")])));
        assert!(!brush_casts_no_shadow(&ent("func_brush", &[("vrad_brush_cast_shadows", "1")])));
    }

    #[test]
    fn texlight_faces_become_preview_lights() {
        let square = [DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 0.0), DVec3::new(0.0, 64.0, 0.0)];
        let l = face_light(&square, DVec3::Z, glam::Vec3::new(200.0, 100.0, 50.0)).expect("light");
        assert_eq!(l.pos, [32.0, 32.0, 1.0]);
        assert!(matches!(l.kind, LightKind::Spot { dir: [0.0, 0.0, 1.0], .. }));
        assert_eq!(l.color, [1.0, 0.5, 0.25]);
        let expected = 200.0 * 64.0 * 64.0 / std::f32::consts::PI / 255.0;
        assert!((l.intensity - expected).abs() < expected * 1e-4, "{}", l.intensity);
        assert!(l.falloff.is_some_and(|f| f.quadratic == 1.0 && f.constant == 0.0));
        assert!(l.range >= MIN_RANGE && l.range <= MAX_RANGE);
        assert!(face_light(&square[..2], DVec3::Z, glam::Vec3::ONE).is_none());
        assert!(face_light(&[DVec3::ZERO; 4], DVec3::Z, glam::Vec3::ONE).is_none());
        assert!(face_light(&square, DVec3::NAN, glam::Vec3::ONE).is_none());
        assert!(face_light(&square, DVec3::Z, glam::Vec3::ZERO).is_none());
        assert!(face_light(&square, DVec3::Z, glam::Vec3::splat(f32::NAN)).is_none());
    }
}
