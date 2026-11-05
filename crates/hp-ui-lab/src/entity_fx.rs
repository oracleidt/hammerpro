use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::DVec3;
use hp_app::AppState;
use hp_app::data_colors;
use hp_app::runtime::wind;
use hp_editor::Id;
use hp_render::{ParticleBlend, ParticleOrient};
use hp_ui_viewports::layers::OverlayLayer;

use crate::particles::{MaterialLook, ParticleHandle, ParticleSprite, material_key, read_material_look};

pub const MAX_ENTITIES: usize = 24;
pub const MAX_PARTICLES: usize = 2400;
pub const MAX_PER_ENTITY: usize = 300;
pub const MAX_DISTANCE: f64 = 3072.0;
const MIN_COUNT: usize = 4;
const POINT_BOX: f64 = 48.0;
const SMOKE_CELLS: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FxKind {
    Smoke,
    Embers,
    Cloud,
    Motes,
}

impl FxKind {
    pub fn of(classname: &str) -> Option<FxKind> {
        match classname.to_ascii_lowercase().as_str() {
            "func_smokevolume" => Some(FxKind::Smoke),
            "env_embers" => Some(FxKind::Embers),
            "func_dustcloud" => Some(FxKind::Cloud),
            "func_dustmotes" => Some(FxKind::Motes),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            FxKind::Smoke => "func_smokevolume",
            FxKind::Embers => "env_embers",
            FxKind::Cloud => "func_dustcloud",
            FxKind::Motes => "func_dustmotes",
        }
    }

    pub fn materials(self) -> &'static [&'static str] {
        match self {
            FxKind::Smoke | FxKind::Cloud => &["particle/particle_smokegrenade", "particle/smokesprites_0001"],
            FxKind::Embers => &["effects/ember_swirling001", "particle/fire"],
            FxKind::Motes => &["particle/sparkles", "effects/fleck_cement1"],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fx {
    pub id: Id,
    pub kind: FxKind,
    pub min: DVec3,
    pub max: DVec3,
    pub color_a: [f32; 3],
    pub color_b: [f32; 3],
    pub count: usize,
    pub size: [f32; 2],
    pub life: [f32; 2],
    pub speed: f32,
    pub fall: f32,
    pub alpha: f32,
    pub reach: f64,
    pub material: String,
    pub spin: f32,
    pub grid: [usize; 3],
    pub dist_max: f64,
    pub enabled: bool,
    pub frozen: bool,
    pub wind: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub pos: [f32; 3],
    pub radius: f32,
    pub color: [f32; 4],
    pub roll: f32,
}

fn num(e: &hp_vmf::Entity, key: &str, default: f32) -> f32 {
    e.get(key).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
}

fn flag(e: &hp_vmf::Entity, key: &str, bit: u32) -> bool {
    e.get(key).and_then(|v| v.trim().parse::<u32>().ok()).is_some_and(|v| v & bit != 0)
}

fn color(e: &hp_vmf::Entity, key: &str) -> [f32; 3] {
    let v: Vec<f32> =
        e.get(key).map(|s| s.split_whitespace().filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
    if v.len() >= 3 && v.iter().all(|x| x.is_finite()) {
        [(v[0] / 255.0).clamp(0.0, 1.0), (v[1] / 255.0).clamp(0.0, 1.0), (v[2] / 255.0).clamp(0.0, 1.0)]
    } else {
        [1.0; 3]
    }
}

fn grid_for(ext: DVec3, spacing: f64) -> [usize; 3] {
    let mut step = spacing.max(8.0);
    loop {
        let g = [ext.x, ext.y, ext.z].map(|e| ((e / step).round() as usize).max(1));
        if g[0] * g[1] * g[2] <= SMOKE_CELLS {
            return g;
        }
        step *= 1.15;
    }
}

pub fn build(id: Id, e: &hp_vmf::Entity, bounds: Option<(DVec3, DVec3)>) -> Option<Fx> {
    let kind = FxKind::of(&e.classname)?;
    let (min, max) = bounds.filter(|(a, b)| a.is_finite() && b.is_finite()).or_else(|| {
        let o = e.origin().filter(|o| o.is_finite())?;
        Some((o - DVec3::splat(POINT_BOX), o + DVec3::splat(POINT_BOX)))
    })?;
    let ext = (max - min).max(DVec3::splat(1.0));
    let mut fx = Fx {
        id,
        kind,
        min,
        max: min + ext,
        color_a: [1.0; 3],
        color_b: [1.0; 3],
        count: MIN_COUNT,
        size: [4.0, 4.0],
        life: [4.0, 4.0],
        speed: 10.0,
        fall: 0.0,
        alpha: 0.4,
        reach: MAX_DISTANCE,
        material: String::new(),
        spin: 0.0,
        grid: [1, 1, 1],
        dist_max: MAX_DISTANCE,
        enabled: true,
        frozen: false,
        wind: false,
    };
    match kind {
        FxKind::Smoke => {
            fx.color_a = color(e, "Color1");
            fx.color_b = color(e, "Color2");
            let width = num(e, "ParticleDrawWidth", 120.0).clamp(8.0, 1024.0);
            let spacing = f64::from(num(e, "ParticleSpacingDistance", 80.0).clamp(16.0, 1024.0));
            let density = num(e, "Density", 1.0).clamp(0.0, 4.0);
            fx.grid = grid_for(ext, spacing);
            fx.count = fx.grid[0] * fx.grid[1] * fx.grid[2];
            fx.size = [width * 0.5, width * 0.5];
            fx.life = [30.0, 30.0];
            fx.speed = num(e, "MovementSpeed", 10.0).clamp(0.0, 512.0);
            fx.spin = num(e, "RotationSpeed", 10.0).clamp(-720.0, 720.0);
            fx.alpha = (1.25 * density.min(1.5)).clamp(0.0, 1.0);
            fx.material = e.get("Material").map(|m| m.trim().to_owned()).unwrap_or_default();
        }
        FxKind::Embers => {
            fx.color_a = color(e, "rendercolor");
            fx.color_b = fx.color_a;
            let life = num(e, "lifetime", 4.0).clamp(0.2, 60.0);
            fx.life = [life * 0.75, life];
            let rate = num(e, "density", 50.0).clamp(0.0, 4096.0);
            fx.count = ((rate * life * 0.875) as usize).clamp(MIN_COUNT, MAX_PER_ENTITY);
            fx.speed = num(e, "speed", 32.0).clamp(0.0, 1024.0);
            fx.enabled = flag(e, "spawnflags", 1);
            fx.size = [4.0, 9.0];
            fx.alpha = (num(e, "renderamt", 255.0).clamp(0.0, 255.0) / 255.0).max(0.05);
        }
        FxKind::Cloud | FxKind::Motes => {
            fx.color_a = color(e, "Color");
            fx.color_b = fx.color_a;
            let (size_default, alpha_default) =
                if kind == FxKind::Cloud { ([100.0, 200.0], 30.0) } else { ([10.0, 20.0], 255.0) };
            let size_min = num(e, "SizeMin", size_default[0]).clamp(0.5, 512.0);
            let size_max = num(e, "SizeMax", size_default[1]).clamp(size_min, 512.0);
            let life_min = num(e, "LifetimeMin", 3.0).clamp(0.2, 120.0);
            let life_max = num(e, "LifetimeMax", 5.0).clamp(life_min, 120.0);
            let rate = num(e, "SpawnRate", 40.0).clamp(0.0, 1000.0);
            fx.count = ((rate * (life_min + life_max) * 0.5) as usize).clamp(MIN_COUNT, MAX_PER_ENTITY);
            fx.size = [size_min, size_max];
            fx.life = [life_min, life_max];
            fx.speed = num(e, "SpeedMax", 13.0).clamp(0.0, 512.0);
            fx.fall = num(e, "FallSpeed", 0.0).clamp(-512.0, 512.0);
            fx.alpha = (num(e, "Alpha", alpha_default).clamp(0.0, 255.0) / 255.0).clamp(0.02, 1.0);
            let dist = f64::from(num(e, "DistMax", 1024.0)).clamp(128.0, MAX_DISTANCE);
            fx.dist_max = dist;
            fx.reach = dist;
            fx.enabled = num(e, "StartDisabled", 0.0) < 0.5;
            fx.frozen = num(e, "Frozen", 0.0) >= 0.5;
            fx.wind = num(e, "AffectedByWind", 1.0) >= 0.5;
            if fx.frozen {
                fx.count = (rate as usize).clamp(MIN_COUNT, MAX_PER_ENTITY);
            }
        }
    }
    Some(fx)
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

fn rnd(seed: u64, k: u64) -> f32 {
    (mix(seed ^ k.wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> 40) as f32 / 16_777_216.0
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn inside(p: DVec3, min: DVec3, max: DVec3) -> bool {
    p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y && p.z >= min.z && p.z <= max.z
}

pub fn particles(fx: &Fx, time: f64, scale: f32, eye: DVec3, out: &mut Vec<Pt>) {
    let n = ((fx.count as f32 * scale.clamp(0.0, 1.0)).ceil() as usize).min(fx.count);
    let seed_base = u64::from(fx.id).wrapping_mul(0x2545_F491_4F6C_DD1D);
    let ext = fx.max - fx.min;
    for i in 0..n as u64 {
        let seed = mix(seed_base ^ i.wrapping_mul(0x9E37_79B9));
        let tint = rnd(seed, 9);
        let rgb = [
            lerp(fx.color_a[0], fx.color_b[0], tint),
            lerp(fx.color_a[1], fx.color_b[1], tint),
            lerp(fx.color_a[2], fx.color_b[2], tint),
        ];
        match fx.kind {
            FxKind::Smoke => smoke_puff(fx, i as usize, seed, time, rgb, out),
            FxKind::Embers => ember(fx, seed, time, ext, rgb, out),
            FxKind::Cloud | FxKind::Motes => dust(fx, seed, time, ext, eye, rgb, out),
        }
    }
}

fn smoke_puff(fx: &Fx, i: usize, seed: u64, time: f64, rgb: [f32; 3], out: &mut Vec<Pt>) {
    let [nx, ny, nz] = fx.grid;
    let (ix, iy, iz) = (i % nx, (i / nx) % ny, i / (nx * ny));
    if iz >= nz {
        return;
    }
    let ext = fx.max - fx.min;
    let cell = DVec3::new(ext.x / nx as f64, ext.y / ny as f64, ext.z / nz as f64);
    let center =
        fx.min + DVec3::new(cell.x * (ix as f64 + 0.5), cell.y * (iy as f64 + 0.5), cell.z * (iz as f64 + 0.5));
    let reach = f64::from(fx.speed) * 1.5;
    let sway = |axis: u64, limit: f64| {
        let w = f64::from(0.25 + rnd(seed, 20 + axis) * 0.5);
        let p = f64::from(rnd(seed, 30 + axis)) * std::f64::consts::TAU;
        (time * w + p).sin() * reach.min(limit * 0.5)
    };
    let pos = DVec3::new(center.x + sway(0, cell.x), center.y + sway(1, cell.y), center.z + sway(2, cell.z));
    let spin = fx.spin.to_radians() * time as f32 * lerp(0.6, 1.0, rnd(seed, 41));
    let breathe = 1.0 + 0.06 * ((time as f32) * 0.7 + rnd(seed, 42) * std::f32::consts::TAU).sin();
    out.push(Pt {
        pos: [pos.x as f32, pos.y as f32, pos.z as f32],
        radius: fx.size[0] * lerp(0.9, 1.1, rnd(seed, 43)) * breathe,
        color: [rgb[0], rgb[1], rgb[2], fx.alpha * lerp(0.75, 1.0, rnd(seed, 44))],
        roll: rnd(seed, 40) * std::f32::consts::TAU + spin,
    });
}

fn ember(fx: &Fx, seed: u64, time: f64, ext: DVec3, rgb: [f32; 3], out: &mut Vec<Pt>) {
    let life = lerp(fx.life[0], fx.life[1], rnd(seed, 0)).max(0.2);
    let phase = ((time / f64::from(life)) + f64::from(rnd(seed, 1))).rem_euclid(1.0) as f32;
    let age = phase * life;
    let start = fx.min
        + DVec3::new(ext.x * f64::from(rnd(seed, 2)), ext.y * f64::from(rnd(seed, 3)), ext.z * f64::from(rnd(seed, 4)));
    let theta = rnd(seed, 5) * std::f32::consts::TAU;
    let speed = fx.speed * lerp(0.5, 1.0, rnd(seed, 7));
    let (w1, w2) = (lerp(1.4, 3.2, rnd(seed, 11)), lerp(1.4, 3.2, rnd(seed, 12)));
    let amp = fx.speed * 0.18;
    let drift = DVec3::new(
        f64::from(theta.cos() * speed * 0.55 + (age * w1 + rnd(seed, 13) * std::f32::consts::TAU).sin() * amp),
        f64::from(theta.sin() * speed * 0.55 + (age * w2 + rnd(seed, 14) * std::f32::consts::TAU).sin() * amp),
        f64::from(speed * lerp(0.15, 0.55, rnd(seed, 6))),
    );
    let pos = start + drift * f64::from(age) * 0.6;
    let step = (time * 14.0 + f64::from(rnd(seed, 15)) * 50.0).floor() as u64;
    let flicker = lerp(0.45, 1.0, rnd(seed, 100 + step % 997));
    let fade_in = smooth(phase / 0.08);
    let fade_out = 1.0 - phase;
    let radius = lerp(fx.size[0], fx.size[1], rnd(seed, 10)) * lerp(1.0, 0.55, phase);
    out.push(Pt {
        pos: [pos.x as f32, pos.y as f32, pos.z as f32],
        radius,
        color: [rgb[0], rgb[1], rgb[2], (fx.alpha * fade_in * fade_out * flicker).clamp(0.0, 1.0)],
        roll: rnd(seed, 16) * std::f32::consts::TAU,
    });
}

fn dust(fx: &Fx, seed: u64, time: f64, ext: DVec3, eye: DVec3, rgb: [f32; 3], out: &mut Vec<Pt>) {
    let life = lerp(fx.life[0], fx.life[1], rnd(seed, 0)).max(0.2);
    let mut phase = ((time / f64::from(life)) + f64::from(rnd(seed, 1))).rem_euclid(1.0) as f32;
    if fx.frozen {
        phase = 0.5;
    }
    let age = if fx.frozen { 0.0 } else { f64::from(phase * life) };
    let start = fx.min
        + DVec3::new(ext.x * f64::from(rnd(seed, 2)), ext.y * f64::from(rnd(seed, 3)), ext.z * f64::from(rnd(seed, 4)));
    let s = f64::from(fx.speed);
    let vel = DVec3::new(
        (f64::from(rnd(seed, 5)) * 2.0 - 1.0) * s,
        (f64::from(rnd(seed, 6)) * 2.0 - 1.0) * s,
        (f64::from(rnd(seed, 7)) * 2.0 - 1.0) * s - f64::from(fx.fall),
    );
    let pos = start + vel * age;
    if !inside(pos, fx.min, fx.max) {
        return;
    }
    let fade = smooth(phase / 0.2).min(smooth((1.0 - phase) / 0.3));
    let near = (1.0 - (pos - eye).length() / fx.dist_max.max(1.0)).clamp(0.0, 1.0) as f32;
    out.push(Pt {
        pos: [pos.x as f32, pos.y as f32, pos.z as f32],
        radius: lerp(fx.size[0], fx.size[1], rnd(seed, 10)),
        color: [rgb[0], rgb[1], rgb[2], (fx.alpha * fade * near).clamp(0.0, 1.0)],
        roll: rnd(seed, 16) * std::f32::consts::TAU,
    });
}

fn distance_to_box(p: DVec3, min: DVec3, max: DVec3) -> f64 {
    let c = p.clamp(min, max);
    (p - c).length()
}

struct DustParticle {
    seed: u64,
    pos: DVec3,
    velocity: DVec3,
    age: f32,
    life: f32,
}

impl DustParticle {
    fn new(fx: &Fx, seed: u64, warm: bool) -> Self {
        let ext = fx.max - fx.min;
        let life = lerp(fx.life[0], fx.life[1], rnd(seed, 0)).max(0.2);
        Self {
            seed,
            pos: fx.min + ext * DVec3::new(f64::from(rnd(seed, 2)), f64::from(rnd(seed, 3)), f64::from(rnd(seed, 4))),
            velocity: DVec3::new(
                f64::from((rnd(seed, 5) * 2.0 - 1.0) * fx.speed),
                f64::from((rnd(seed, 6) * 2.0 - 1.0) * fx.speed),
                f64::from((rnd(seed, 7) * 2.0 - 1.0) * fx.speed - fx.fall),
            ),
            age: if warm { life * rnd(seed, 1) } else { 0.0 },
            life,
        }
    }
}

struct Dust {
    source: Fx,
    particles: Vec<DustParticle>,
}

impl Dust {
    fn new(fx: &Fx) -> Self {
        let base = u64::from(fx.id).wrapping_mul(0x2545_F491_4F6C_DD1D);
        let particles = (0..fx.count)
            .map(|i| DustParticle::new(fx, mix(base ^ (i as u64).wrapping_mul(0x9E37_79B9)), true))
            .collect();
        Self { source: fx.clone(), particles }
    }

    fn draw(&mut self, dt: f32, scale: f32, eye: DVec3, out: &mut Vec<Pt>) {
        let fx = &self.source;
        let n = ((fx.count as f32 * scale.clamp(0.0, 1.0)).ceil() as usize).min(fx.count);
        let step = f64::from(dt.clamp(0.0, 0.1));
        for (index, particle) in self.particles.iter_mut().enumerate() {
            particle.age += step as f32;
            if let Some(velocity) = wind::velocity_at(particle.pos) {
                for axis in 0..2 {
                    let delta = velocity[axis] - particle.velocity[axis];
                    particle.velocity[axis] += delta.signum() * delta.abs().min(50.0 * step);
                }
            }
            particle.pos += particle.velocity * step;
            if particle.age >= particle.life || !inside(particle.pos, fx.min, fx.max) {
                *particle = DustParticle::new(fx, mix(particle.seed.wrapping_add(1)), false);
            }
            if index >= n {
                continue;
            }
            let phase = particle.age / particle.life;
            let fade = smooth(phase / 0.2).min(smooth((1.0 - phase) / 0.3));
            let near = (1.0 - particle.pos.distance(eye) / fx.dist_max.max(1.0)).clamp(0.0, 1.0) as f32;
            out.push(Pt {
                pos: particle.pos.as_vec3().to_array(),
                radius: lerp(fx.size[0], fx.size[1], rnd(particle.seed, 10)),
                color: [fx.color_a[0], fx.color_a[1], fx.color_a[2], fx.alpha * fade * near],
                roll: rnd(particle.seed, 16) * std::f32::consts::TAU,
            });
        }
    }
}

pub fn pick_material(fs: Option<&hp_fs::GameFs>, kind: FxKind, custom: &str) -> String {
    let custom = custom.trim();
    let candidates = kind.materials();
    let Some(fs) = fs else {
        return if custom.is_empty() { candidates[0].to_owned() } else { custom.to_owned() };
    };
    std::iter::once(custom)
        .filter(|c| !c.is_empty())
        .chain(candidates.iter().copied())
        .find(|c| fs.material(c).is_some())
        .unwrap_or(if custom.is_empty() { candidates[0] } else { custom })
        .to_owned()
}

pub struct EntityFxLayer {
    handle: ParticleHandle,
    sources: Vec<Fx>,
    key: Option<(u64, u64)>,
    visible: Vec<(usize, f32)>,
    time: f64,
    generation: u64,
    chosen: HashMap<(FxKind, String), String>,
    looked: HashSet<String>,
    dust: HashMap<Id, Dust>,
}

impl EntityFxLayer {
    pub fn new(handle: ParticleHandle) -> Self {
        Self {
            handle,
            sources: Vec::new(),
            key: None,
            visible: Vec::new(),
            time: 0.0,
            generation: u64::MAX,
            chosen: HashMap::new(),
            looked: HashSet::new(),
            dust: HashMap::new(),
        }
    }

    pub fn visible_count(&self) -> usize {
        self.visible.len()
    }

    fn refresh(&mut self, state: &AppState) {
        let key = (state.sidecar.serial(), state.doc.revision());
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        let scene = state.doc.scene();
        self.sources = scene
            .entities()
            .iter()
            .filter_map(|(id, node)| {
                FxKind::of(&node.entity.classname)?;
                let bounds = scene.entity_solid_bounds(*id).map(|b| (b.min, b.max));
                build(*id, &node.entity, bounds)
            })
            .collect();
        self.sources.sort_by_key(|s| s.id);
        self.dust.retain(|id, dust| self.sources.iter().any(|fx| fx.id == *id && *fx == dust.source));
    }

    fn set_active(&self, active: bool) {
        if let Ok(mut s) = self.handle.lock() {
            if s.fx_active != active {
                s.fx_active = active;
            }
            if !active {
                s.fx_sprites.clear();
                s.fx_materials.clear();
            }
        }
    }

    fn build_sprites(&mut self, state: &AppState, eye: DVec3) {
        let wind_active = wind::active();
        if !wind_active {
            self.dust.clear();
        } else {
            self.dust.retain(|id, _| self.visible.iter().any(|(index, _)| self.sources[*index].id == *id));
        }
        let fs = state.assets.fs();
        let generation = state.assets.generation();
        if self.generation != generation {
            self.generation = generation;
            self.chosen.clear();
            self.looked.clear();
        }
        let Ok(mut shared) = self.handle.lock() else { return };
        let crate::particles::ParticleShared { fx_sprites, fx_materials, looks, fx_active, .. } = &mut *shared;
        fx_sprites.clear();
        fx_materials.clear();
        *fx_active = !self.visible.is_empty();
        let mut pts: Vec<Pt> = Vec::with_capacity(MAX_PER_ENTITY);
        for &(i, scale) in &self.visible {
            let fx = &self.sources[i];
            let name = self
                .chosen
                .entry((fx.kind, fx.material.clone()))
                .or_insert_with(|| pick_material(fs, fx.kind, &fx.material))
                .clone();
            let key = material_key(&name);
            if self.looked.insert(key.clone()) {
                let look = match fs {
                    Some(fs) => read_material_look(fs, &key),
                    None => {
                        MaterialLook { additive: crate::particles::guess_additive(&key), ..MaterialLook::default() }
                    }
                };
                looks.insert(key.clone(), Arc::new(look));
            }
            let look = looks.get(&key).cloned().unwrap_or_default();
            let material = match fx_materials.iter().position(|m| *m == name) {
                Some(p) => p as u32,
                None => {
                    fx_materials.push(name.clone());
                    (fx_materials.len() - 1) as u32
                }
            };
            pts.clear();
            if wind_active && fx.wind && !fx.frozen && matches!(fx.kind, FxKind::Cloud | FxKind::Motes) {
                self.dust.entry(fx.id).or_insert_with(|| Dust::new(fx)).draw(state.frame.dt, scale, eye, &mut pts);
            } else {
                particles(fx, self.time, scale, eye, &mut pts);
            }
            for p in &pts {
                fx_sprites.push(ParticleSprite {
                    pos: p.pos,
                    size: p.radius * 2.0,
                    axis: [0.0; 3],
                    roll: p.roll,
                    uv: hp_pcf::sheet::FULL_UV,
                    color: [p.color[0], p.color[1], p.color[2], (p.color[3] * look.opacity).clamp(0.0, 1.0)],
                    bright: look.brightness(),
                    add_self: if look.blend() == ParticleBlend::AddSelf { look.add_self } else { 0.0 },
                    orient: ParticleOrient::Screen,
                    blend: look.blend(),
                    screen_fade: look.fade_size,
                    material,
                    entity: fx.id,
                });
            }
        }
    }
}

impl OverlayLayer for EntityFxLayer {
    fn name(&self) -> &'static str {
        "lab_entity_fx"
    }

    fn prepare(&mut self, state: &AppState) {
        wind::update(state);
        if !state.settings.lab.show_entity_fx {
            self.visible.clear();
            self.dust.clear();
            self.set_active(false);
            return;
        }
        self.refresh(state);
        self.time = state.frame.time;
        let cam = state.cameras.cam3d.pos;
        let mut near: Vec<(f64, usize)> = self
            .sources
            .iter()
            .enumerate()
            .filter(|(_, s)| s.enabled && state.doc.is_entity_visible(s.id))
            .filter_map(|(i, s)| {
                let d = distance_to_box(cam, s.min, s.max);
                (d <= s.reach).then_some((d, i))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(MAX_ENTITIES);
        let mut budget = MAX_PARTICLES;
        self.visible.clear();
        for (d, i) in near {
            if budget < MIN_COUNT {
                break;
            }
            let s = &self.sources[i];
            let reach = s.reach.max(1.0);
            let scale = (1.0 - 0.6 * (d / reach)).clamp(0.4, 1.0) as f32;
            let n = ((s.count as f32 * scale).ceil() as usize).min(s.count).min(budget);
            budget -= n;
            self.visible.push((i, n as f32 / s.count.max(1) as f32));
        }
        if self.visible.is_empty() {
            self.set_active(false);
        } else {
            self.build_sprites(state, cam);
        }
    }

    fn paint(&self, state: &AppState, view: &hp_app::ViewInfo, painter: &egui::Painter) {
        if view.is_2d() || !state.settings.lab.show_entity_fx {
            return;
        }
        let painter = painter.with_clip_rect(view.rect);
        for sample in wind::samples() {
            if !state.doc.is_selected(hp_editor::ObjRef::Entity(sample.id)) {
                continue;
            }
            let speed = sample.velocity.length();
            let direction = sample.velocity.normalize_or_zero();
            let end = sample.origin + direction * (speed * 1.2).clamp(32.0, 320.0);
            let (Some(start), Some(end)) = (view.world_to_screen(sample.origin), view.world_to_screen(end)) else {
                continue;
            };
            let color = if sample.gusting { data_colors::WIND_GUST } else { data_colors::WIND_NORMAL };
            if start.distance_sq(end) > 1.0 {
                painter.arrow(start, end - start, egui::Stroke::new(2.0, color));
            }
            let label = if sample.gusting { hp_app::t!("lab.wind.gust") } else { hp_app::t!("lab.wind.normal") };
            painter.text(
                end + egui::vec2(0.0, -8.0),
                egui::Align2::CENTER_BOTTOM,
                hp_app::tf!("lab.wind.speed", label, format!("{speed:.1}")),
                egui::FontId::proportional(12.0),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(class: &str, props: &[(&str, &str)]) -> hp_vmf::Entity {
        let mut e = hp_vmf::Entity { id: 7, classname: class.into(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    fn bounds() -> Option<(DVec3, DVec3)> {
        Some((DVec3::new(0.0, 0.0, 0.0), DVec3::new(256.0, 256.0, 128.0)))
    }

    fn eye() -> DVec3 {
        DVec3::new(128.0, 128.0, 64.0)
    }

    #[test]
    fn only_the_four_classes_are_previewed() {
        for c in ["func_smokevolume", "env_embers", "func_dustcloud", "func_dustmotes", "FUNC_DUSTMOTES"] {
            assert!(FxKind::of(c).is_some(), "{c}");
        }
        assert!(FxKind::of("info_particle_system").is_none());
        assert!(build(1, &entity("prop_static", &[]), bounds()).is_none());
    }

    #[test]
    fn keyvalues_drive_the_preview() {
        let smoke = build(
            1,
            &entity(
                "func_smokevolume",
                &[
                    ("Color1", "255 0 0"),
                    ("Color2", "0 0 255"),
                    ("ParticleDrawWidth", "60"),
                    ("ParticleSpacingDistance", "32"),
                    ("Density", "2"),
                    ("RotationSpeed", "45"),
                    ("Material", "particle/custom_smoke"),
                ],
            ),
            bounds(),
        )
        .unwrap();
        assert_eq!(smoke.color_a, [1.0, 0.0, 0.0]);
        assert_eq!(smoke.color_b, [0.0, 0.0, 1.0]);
        assert_eq!(smoke.size[1], 30.0);
        assert_eq!(smoke.spin, 45.0);
        assert_eq!(smoke.material, "particle/custom_smoke");
        assert_eq!(smoke.count, smoke.grid[0] * smoke.grid[1] * smoke.grid[2]);
        assert!(smoke.count <= SMOKE_CELLS);
        let embers = build(
            2,
            &entity(
                "env_embers",
                &[("density", "30"), ("lifetime", "2"), ("speed", "64"), ("rendercolor", "255 128 0")],
            ),
            bounds(),
        )
        .unwrap();
        assert_eq!(embers.count, 52);
        assert_eq!(embers.life[1], 2.0);
        assert_eq!(embers.speed, 64.0);
        let huge = build(3, &entity("env_embers", &[("density", "100000"), ("lifetime", "60")]), bounds()).unwrap();
        assert_eq!(huge.count, MAX_PER_ENTITY);
        let dust = build(
            4,
            &entity(
                "func_dustcloud",
                &[
                    ("SpawnRate", "10"),
                    ("LifetimeMin", "2"),
                    ("LifetimeMax", "4"),
                    ("SizeMin", "10"),
                    ("SizeMax", "20"),
                ],
            ),
            bounds(),
        )
        .unwrap();
        assert_eq!(dust.count, 30);
        assert_eq!(dust.size, [10.0, 20.0]);
        let motes = build(5, &entity("func_dustmotes", &[("FallSpeed", "20"), ("Alpha", "100")]), bounds()).unwrap();
        assert_eq!(motes.fall, 20.0);
        assert!(motes.alpha > dust.alpha);
    }

    #[test]
    fn start_states_and_frozen_dust_follow_the_keys() {
        assert!(!build(1, &entity("env_embers", &[]), bounds()).unwrap().enabled);
        assert!(build(1, &entity("env_embers", &[("spawnflags", "1")]), bounds()).unwrap().enabled);
        assert!(!build(1, &entity("env_embers", &[("spawnflags", "2")]), bounds()).unwrap().enabled);
        assert!(build(1, &entity("env_embers", &[("spawnflags", "3")]), bounds()).unwrap().enabled);
        assert!(build(1, &entity("func_dustmotes", &[]), bounds()).unwrap().enabled);
        assert!(!build(1, &entity("func_dustcloud", &[("StartDisabled", "1")]), bounds()).unwrap().enabled);
        assert!(build(1, &entity("func_smokevolume", &[]), bounds()).unwrap().enabled);
        let frozen = build(2, &entity("func_dustmotes", &[("Frozen", "1"), ("SpawnRate", "30")]), bounds()).unwrap();
        assert_eq!(frozen.count, 30);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        particles(&frozen, 1.0, 1.0, eye(), &mut a);
        particles(&frozen, 9.0, 1.0, eye(), &mut b);
        assert_eq!(a, b, "frozen motes do not move");
    }

    #[test]
    fn point_entities_get_a_small_box() {
        let e = entity("env_embers", &[("origin", "100 200 300")]);
        let fx = build(9, &e, None).unwrap();
        assert_eq!(fx.min, DVec3::new(52.0, 152.0, 252.0));
        assert!(build(9, &entity("env_embers", &[]), None).is_none());
    }

    #[test]
    fn dust_and_smoke_stay_in_the_volume_and_everything_moves() {
        for class in ["func_smokevolume", "env_embers", "func_dustcloud", "func_dustmotes"] {
            let fx = build(11, &entity(class, &[("FallSpeed", "30"), ("MovementSpeed", "40")]), bounds()).unwrap();
            let mut a = Vec::new();
            let mut b = Vec::new();
            particles(&fx, 3.0, 1.0, eye(), &mut a);
            particles(&fx, 3.5, 1.0, eye(), &mut b);
            assert!(!a.is_empty() && !b.is_empty(), "{class}");
            for p in a.iter().chain(&b) {
                assert!(p.radius > 0.0 && (0.0..=1.0).contains(&p.color[3]), "{class}");
                assert!(p.pos.iter().all(|v| v.is_finite()), "{class}");
                if matches!(fx.kind, FxKind::Cloud | FxKind::Motes) {
                    assert!(p.pos[0] >= -0.01 && p.pos[0] <= 256.01, "{class}");
                    assert!(p.pos[2] >= -0.01 && p.pos[2] <= 128.01, "{class}");
                }
            }
            let moved = a.len() != b.len() || a.iter().zip(&b).any(|(p, q)| p.pos != q.pos);
            assert!(moved, "{class} animates");
            let mut half = Vec::new();
            particles(&fx, 3.0, 0.5, eye(), &mut half);
            assert!(half.len() < a.len() || a.len() <= MIN_COUNT, "{class}");
        }
    }

    #[test]
    fn embers_flicker_fade_and_leave_the_box() {
        let fx = build(21, &entity("env_embers", &[("density", "100"), ("lifetime", "3"), ("speed", "200")]), bounds())
            .unwrap();
        let mut outside = false;
        let mut seen = std::collections::HashSet::new();
        for step in 0..60 {
            let mut v = Vec::new();
            particles(&fx, 5.0 + step as f64 * 0.07, 1.0, eye(), &mut v);
            for p in &v {
                seen.insert((p.color[3] * 1000.0) as i32);
                if p.pos[2] > 128.0 + 1.0 || p.pos[0] < -1.0 || p.pos[0] > 257.0 {
                    outside = true;
                }
            }
        }
        assert!(outside, "embers drift out of the brush like in game");
        assert!(seen.len() > 20, "the alpha flickers");
    }

    #[test]
    fn smoke_sits_on_a_grid_and_turns() {
        let fx = build(31, &entity("func_smokevolume", &[("ParticleSpacingDistance", "64")]), bounds()).unwrap();
        assert_eq!(fx.grid, [4, 4, 2]);
        let mut a = Vec::new();
        let mut b = Vec::new();
        particles(&fx, 1.0, 1.0, eye(), &mut a);
        particles(&fx, 4.0, 1.0, eye(), &mut b);
        assert_eq!(a.len(), 32);
        assert!(a.iter().zip(&b).all(|(p, q)| (p.roll - q.roll).abs() > 0.01));
        assert!(a.iter().all(|p| (p.radius - 60.0).abs() < 12.0));
    }

    #[test]
    fn dust_fades_with_distance() {
        let fx = build(41, &entity("func_dustmotes", &[("DistMax", "512")]), bounds()).unwrap();
        let mut near = Vec::new();
        let mut far = Vec::new();
        particles(&fx, 2.0, 1.0, eye(), &mut near);
        particles(&fx, 2.0, 1.0, eye() + DVec3::new(450.0, 0.0, 0.0), &mut far);
        let sum = |v: &[Pt]| v.iter().map(|p| p.color[3]).sum::<f32>();
        assert!(sum(&near) > sum(&far) * 2.0);
    }

    #[test]
    fn material_choice_falls_back_to_the_known_sprites() {
        assert_eq!(pick_material(None, FxKind::Embers, ""), "effects/ember_swirling001");
        assert_eq!(pick_material(None, FxKind::Motes, ""), "particle/sparkles");
        assert_eq!(pick_material(None, FxKind::Smoke, "particle/custom"), "particle/custom");
        assert_eq!(pick_material(None, FxKind::Cloud, ""), "particle/particle_smokegrenade");
    }

    #[test]
    fn the_layer_is_bounded_and_follows_the_setting() {
        let mut state = hp_app::testing::test_state();
        let mut map = hp_vmf::Map::new_empty();
        for i in 0..60 {
            let mut e = hp_vmf::Entity { id: 100 + i, classname: "env_embers".into(), ..Default::default() };
            e.set("origin", format!("{} 0 0", i * 16));
            e.set("density", "96");
            e.set("spawnflags", "1");
            map.entities.push(e);
        }
        state.doc = hp_editor::Document::from_map(map);
        state.cameras.cam3d.pos = DVec3::new(-100.0, 0.0, 0.0);
        let handle = crate::particles::new_handle();
        let mut layer = EntityFxLayer::new(handle.clone());
        layer.prepare(&state);
        assert!(layer.visible_count() > 0 && layer.visible_count() <= MAX_ENTITIES);
        {
            let shared = handle.lock().unwrap();
            assert!(shared.fx_active);
            assert!(!shared.fx_sprites.is_empty());
            assert!(shared.fx_sprites.len() <= MAX_PARTICLES + MAX_ENTITIES);
            assert_eq!(shared.fx_materials, vec!["effects/ember_swirling001".to_owned()]);
            assert!(shared.fx_sprites.iter().all(|s| s.material == 0));
        }
        state.settings.lab.show_entity_fx = false;
        layer.prepare(&state);
        assert_eq!(layer.visible_count(), 0);
        let shared = handle.lock().unwrap();
        assert!(!shared.fx_active);
        assert!(shared.fx_sprites.is_empty());
    }

    #[test]
    fn setting_is_on_by_default() {
        assert!(hp_app::settings::LabSettings::default().show_entity_fx);
    }
}
