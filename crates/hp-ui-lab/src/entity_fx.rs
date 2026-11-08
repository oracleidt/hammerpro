use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::DVec3;
use hp_app::AppState;
use hp_app::data_colors;
use hp_app::runtime::wind;
use hp_editor::Id;
use hp_render::ParticleOrient;
use hp_ui_viewports::layers::OverlayLayer;

use crate::particles::{MaterialLook, ParticleHandle, ParticleSprite, material_key, read_material_look};

pub const MAX_ENTITIES: usize = 24;
pub const MAX_PARTICLES: usize = 8192;
pub const MAX_PER_ENTITY: usize = 2048;
pub const MAX_DISTANCE: f64 = 3072.0;
const MIN_COUNT: usize = 4;
const SMOKE_CELLS: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FxKind {
    Smoke,
    Embers,
    Cloud,
    Motes,
    Steam,
}

impl FxKind {
    pub fn of(classname: &str) -> Option<FxKind> {
        match classname.to_ascii_lowercase().as_str() {
            "func_smokevolume" => Some(FxKind::Smoke),
            "env_embers" => Some(FxKind::Embers),
            "func_dustcloud" => Some(FxKind::Cloud),
            "func_dustmotes" => Some(FxKind::Motes),
            "env_steam" | "env_steamjet" => Some(FxKind::Steam),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            FxKind::Smoke => "func_smokevolume",
            FxKind::Embers => "env_embers",
            FxKind::Cloud => "func_dustcloud",
            FxKind::Motes => "func_dustmotes",
            FxKind::Steam => "env_steam",
        }
    }

    pub fn materials(self) -> &'static [&'static str] {
        match self {
            FxKind::Smoke => &["particle/particle_smokegrenade", "particle/smokesprites_0001"],
            FxKind::Embers => &["particle/fire"],
            FxKind::Cloud | FxKind::Motes => &["particle/sparkles"],
            FxKind::Steam => &["particle/particle_smokegrenade"],
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
    pub direction: DVec3,
    pub spread: f32,
    pub heatwave: bool,
    pub emissive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub pos: [f32; 3],
    pub radius: f32,
    pub color: [f32; 4],
    pub roll: f32,
    pub yaw: f32,
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
    let mut step = spacing.max(1.0);
    loop {
        let g = [ext.x, ext.y, ext.z].map(|e| ((e / step).round() as usize).max(1));
        if g.iter().try_fold(1usize, |a, b| a.checked_mul(*b)).is_some_and(|n| n <= SMOKE_CELLS) {
            return g;
        }
        step *= 1.15;
    }
}

pub fn build(id: Id, e: &hp_vmf::Entity, bounds: Option<(DVec3, DVec3)>) -> Option<Fx> {
    let kind = FxKind::of(&e.classname)?;
    let bounds = if kind == FxKind::Steam {
        let origin = e.origin().filter(|p| p.is_finite()).unwrap_or(DVec3::ZERO);
        Some((origin, origin + DVec3::splat(1.0)))
    } else {
        bounds
    };
    let (min, max) = bounds.filter(|(a, b)| a.is_finite() && b.is_finite() && a.cmplt(*b).all())?;
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
        direction: direction(e),
        spread: 0.0,
        heatwave: false,
        emissive: flag(e, "spawnflags", 1),
    };
    match kind {
        FxKind::Smoke => {
            fx.color_a = color(e, "Color1");
            fx.color_b = color(e, "Color2");
            let width = num(e, "ParticleDrawWidth", 120.0).clamp(0.01, 4096.0);
            let spacing = f64::from(num(e, "ParticleSpacingDistance", 80.0).clamp(1.0, 4096.0));
            let density = num(e, "Density", 1.0).clamp(0.0, 4.0);
            fx.grid = grid_for(ext, spacing);
            fx.count = fx.grid[0] * fx.grid[1] * fx.grid[2];
            fx.size = [width * 0.5, width * 0.5];
            fx.life = [30.0, 30.0];
            fx.speed = num(e, "MovementSpeed", 10.0).clamp(0.0, 512.0);
            fx.spin = num(e, "RotationSpeed", 10.0).clamp(-720.0, 720.0);
            fx.alpha = density.clamp(0.0, 1.0);
            fx.material = e.get("Material").map(|m| m.trim().to_owned()).unwrap_or_default();
        }
        FxKind::Embers => {
            fx.color_a = color(e, "rendercolor");
            fx.color_b = fx.color_a;
            let life = num(e, "lifetime", 4.0).clamp(0.2, 60.0);
            fx.life = [life, life];
            let rate = num(e, "density", 50.0).clamp(0.0, 4096.0);
            fx.count = ((rate * life) as usize).min(MAX_PER_ENTITY);
            fx.speed = num(e, "speed", 32.0).clamp(0.0, 1024.0);
            fx.enabled = flag(e, "spawnflags", 1) || e.get("targetname").is_none_or(|name| name.trim().is_empty());
            fx.size = [1.0, 1.0];
            fx.wind = num(e, "AffectedByWind", 0.0) >= 0.5;
            fx.alpha = num(e, "renderamt", 255.0).clamp(0.0, 255.0) / 255.0;
        }
        FxKind::Cloud | FxKind::Motes => {
            fx.color_a = color(e, "Color");
            fx.color_b = fx.color_a;
            let (size_default, alpha_default) =
                if kind == FxKind::Cloud { ([100.0, 200.0], 30.0) } else { ([10.0, 20.0], 255.0) };
            let size_min = num(e, "SizeMin", size_default[0]).clamp(0.0, 4096.0);
            let size_max = num(e, "SizeMax", size_default[1]).clamp(size_min, 4096.0);
            let life_min = num(e, "LifetimeMin", 3.0).clamp(0.2, 120.0);
            let life_max = num(e, "LifetimeMax", 5.0).clamp(life_min, 120.0);
            let rate = num(e, "SpawnRate", 40.0).clamp(0.0, 1000.0);
            fx.count = ((rate * (life_min + life_max) * 0.5) as usize).min(MAX_PER_ENTITY);
            fx.size = [size_min, size_max];
            fx.life = [life_min, life_max];
            fx.speed = num(e, "SpeedMax", 13.0).clamp(0.0, 512.0);
            fx.fall = num(e, "FallSpeed", 0.0).clamp(-512.0, 512.0);
            fx.alpha = num(e, "Alpha", alpha_default).clamp(0.0, 255.0) / 255.0;
            let dist = f64::from(num(e, "DistMax", 1024.0)).clamp(1.0, MAX_DISTANCE);
            fx.dist_max = dist;
            fx.reach = dist;
            fx.enabled = num(e, "StartDisabled", 0.0) < 0.5;
            fx.frozen = num(e, "Frozen", 0.0) >= 0.5;
            fx.wind = true;
            if fx.frozen {
                fx.count = (rate as usize).min(MAX_PER_ENTITY);
            }
        }
        FxKind::Steam => {
            fx.color_a = color(e, "rendercolor");
            fx.color_b = fx.color_a;
            fx.speed = num(e, "Speed", 120.0).clamp(0.0, 10000.0);
            fx.spread = num(e, "SpreadSpeed", 15.0).clamp(0.0, 10000.0);
            let life = num(e, "JetLength", 80.0).clamp(0.0, 100000.0) / fx.speed.max(0.001);
            fx.life = [life.clamp(0.001, 120.0); 2];
            fx.count = (num(e, "Rate", 26.0).clamp(0.0, 4096.0) * fx.life[0]).ceil() as usize;
            fx.count = fx.count.min(MAX_PER_ENTITY);
            fx.size = [num(e, "StartSize", 10.0).clamp(0.0, 255.0), num(e, "EndSize", 25.0).clamp(0.0, 255.0)];
            fx.spin = num(e, "RollSpeed", 8.0).clamp(0.0, 720.0);
            fx.alpha = num(e, "renderamt", 255.0).clamp(0.0, 255.0) / 255.0;
            fx.enabled = num(e, "InitialState", 0.0) >= 0.5 && fx.speed > 0.0;
            if e.classname.eq_ignore_ascii_case("env_steamjet") {
                let yaw = e.angles().unwrap_or(DVec3::ZERO).y.to_radians();
                fx.direction = DVec3::new(-yaw.sin(), yaw.cos(), 0.0);
            }
            fx.heatwave = num(e, "type", 0.0) >= 0.5;
            if fx.heatwave {
                fx.material = "sprites/heatwave".to_owned();
            }
        }
    }
    Some(fx)
}

fn direction(e: &hp_vmf::Entity) -> DVec3 {
    let a = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO) * (std::f64::consts::PI / 180.0);
    DVec3::new(a.x.cos() * a.y.cos(), a.x.cos() * a.y.sin(), -a.x.sin())
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

pub fn particles(fx: &Fx, time: f64, scale: f32, eye: DVec3, out: &mut Vec<Pt>) {
    particles_in_view(fx, time, scale, eye, (fx.min + fx.max) * 0.5 - eye, out);
}

fn particles_in_view(fx: &Fx, time: f64, scale: f32, eye: DVec3, forward: DVec3, out: &mut Vec<Pt>) {
    let forward = forward.try_normalize().unwrap_or(DVec3::X);
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
            FxKind::Smoke => smoke_puff(
                fx,
                (i as usize * fx.count / n.max(1)).min(fx.count.saturating_sub(1)),
                seed,
                time,
                eye,
                rgb,
                out,
            ),
            FxKind::Embers => ember(fx, seed, time, ext, rgb, out),
            FxKind::Steam => steam(fx, seed, time, rgb, out),
            FxKind::Cloud | FxKind::Motes => dust(fx, seed, time, ext, eye, forward, rgb, out),
        }
    }
}

fn smoke_puff(fx: &Fx, i: usize, seed: u64, time: f64, eye: DVec3, rgb: [f32; 3], out: &mut Vec<Pt>) {
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
    let spin = fx.spin.to_radians() * time as f32 * lerp(-1.0, 1.0, rnd(seed, 41));
    let near = ((pos.distance(eye) - 10.0) / 20.0).clamp(0.0, 1.0) as f32;
    out.push(Pt {
        pos: [pos.x as f32, pos.y as f32, pos.z as f32],
        radius: fx.size[0],
        color: [rgb[0], rgb[1], rgb[2], fx.alpha * near],
        roll: rnd(seed, 40) * std::f32::consts::TAU + spin,
        yaw: 0.0,
    });
}

fn ember(fx: &Fx, seed: u64, time: f64, ext: DVec3, rgb: [f32; 3], out: &mut Vec<Pt>) {
    let life = lerp(fx.life[0], fx.life[1], rnd(seed, 0)).max(0.2);
    let phase = ((time / f64::from(life)) + f64::from(rnd(seed, 1))).rem_euclid(1.0) as f32;
    let age = phase * life;
    let start = fx.min
        + DVec3::new(ext.x * f64::from(rnd(seed, 2)), ext.y * f64::from(rnd(seed, 3)), ext.z * f64::from(rnd(seed, 4)));
    let jitter = DVec3::new(
        f64::from(rnd(seed, 5) * 2.0 - 1.0),
        f64::from(rnd(seed, 6) * 2.0 - 1.0),
        f64::from(rnd(seed, 7) * 2.0 - 1.0),
    ) * f64::from(fx.speed / 8.0);
    let velocity = fx.direction * f64::from(fx.speed) + jitter;
    let speed = velocity.length();
    let elapsed = f64::from(age).min(speed);
    let mut pos = start + velocity.normalize_or_zero() * (speed * elapsed - 0.5 * elapsed * elapsed);
    if fx.wind
        && let Some(wind) = wind::velocity_at(pos)
    {
        pos += wind * f64::from(age) * 0.5;
    }
    let flicker = lerp(0.75, 1.0, rnd(seed, 9));
    let fade_out = 1.0 - phase;
    let radius = (1.0 - phase).max(0.001);
    out.push(Pt {
        pos: [pos.x as f32, pos.y as f32, pos.z as f32],
        radius,
        color: [
            rgb[0] * flicker * fade_out,
            rgb[1] * flicker * fade_out,
            rgb[2] * flicker * fade_out,
            fx.alpha * fade_out,
        ],
        roll: 0.0,
        yaw: 0.0,
    });
}

fn steam(fx: &Fx, seed: u64, time: f64, rgb: [f32; 3], out: &mut Vec<Pt>) {
    let life = fx.life[0];
    let phase = (time / f64::from(life) + f64::from(rnd(seed, 1))).rem_euclid(1.0) as f32;
    let age = phase * life;
    let right = fx.direction.cross(DVec3::Z).try_normalize().unwrap_or(DVec3::X);
    let up = right.cross(fx.direction).normalize_or_zero();
    let velocity = fx.direction * f64::from(fx.speed)
        + (right * f64::from(rnd(seed, 2) * 2.0 - 1.0) + up * f64::from(rnd(seed, 3) * 2.0 - 1.0))
            * f64::from(fx.spread);
    out.push(Pt {
        pos: (fx.min + velocity * f64::from(age)).as_vec3().to_array(),
        radius: lerp(fx.size[0], fx.size[1], age).max(0.0),
        color: [rgb[0], rgb[1], rgb[2], fx.alpha * (phase * std::f32::consts::PI).sin()],
        roll: rnd(seed, 4) * std::f32::consts::TAU + age * fx.spin.to_radians() * (rnd(seed, 5) * 2.0 - 1.0),
        yaw: 0.0,
    });
}

#[allow(clippy::too_many_arguments)]
fn dust(fx: &Fx, seed: u64, time: f64, ext: DVec3, eye: DVec3, forward: DVec3, rgb: [f32; 3], out: &mut Vec<Pt>) {
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
    let fade = if fx.frozen { 1.0 } else { (phase * std::f32::consts::PI).sin().powi(2) };
    let depth = (pos - eye).dot(forward);
    let near = (1.0 - depth / fx.dist_max.max(1.0)).clamp(0.0, 1.0) as f32;
    if depth <= 0.0 || depth > fx.dist_max {
        return;
    }
    out.push(Pt {
        pos: [pos.x as f32, pos.y as f32, pos.z as f32],
        radius: dust_radius(fx, rnd(seed, 10), depth),
        color: [rgb[0], rgb[1], rgb[2], (fx.alpha * fade * near).clamp(0.0, 1.0)],
        roll: rnd(seed, 16) * std::f32::consts::TAU,
        yaw: 0.0,
    });
}

fn dust_radius(fx: &Fx, random: f32, depth: f64) -> f32 {
    let size = lerp(fx.size[0], fx.size[1], random);
    if fx.kind == FxKind::Motes { size * depth as f32 / 10000.0 } else { size }
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

    fn draw(&mut self, dt: f32, scale: f32, eye: DVec3, forward: DVec3, out: &mut Vec<Pt>) {
        let fx = &self.source;
        let n = ((fx.count as f32 * scale.clamp(0.0, 1.0)).ceil() as usize).min(fx.count);
        let step = f64::from(dt.clamp(0.0, 0.1));
        for (index, particle) in self.particles.iter_mut().enumerate() {
            particle.age += step as f32;
            let velocity = wind::global_velocity();
            for axis in 0..2 {
                let delta = velocity[axis] - particle.velocity[axis];
                particle.velocity[axis] += delta.signum() * delta.abs().min(50.0 * step);
            }
            particle.pos += particle.velocity * step;
            if particle.age >= particle.life {
                *particle = DustParticle::new(fx, mix(particle.seed.wrapping_add(1)), false);
            }
            if index >= n {
                continue;
            }
            let phase = particle.age / particle.life;
            let fade = if fx.frozen { 1.0 } else { (phase * std::f32::consts::PI).sin().powi(2) };
            let depth = (particle.pos - eye).dot(forward);
            let near = (1.0 - depth / fx.dist_max.max(1.0)).clamp(0.0, 1.0) as f32;
            if depth <= 0.0 || depth > fx.dist_max {
                continue;
            }
            out.push(Pt {
                pos: particle.pos.as_vec3().to_array(),
                radius: dust_radius(fx, rnd(particle.seed, 10), depth),
                color: [fx.color_a[0], fx.color_a[1], fx.color_a[2], fx.alpha * fade * near],
                roll: rnd(particle.seed, 16) * std::f32::consts::TAU,
                yaw: 0.0,
            });
        }
    }
}

pub fn pick_material(fs: Option<&hp_fs::GameFs>, kind: FxKind, custom: &str) -> String {
    let custom = custom.trim();
    if !custom.is_empty() {
        return custom.to_owned();
    }
    let candidates = kind.materials();
    let Some(fs) = fs else {
        return candidates[0].to_owned();
    };
    candidates.iter().copied().find(|c| fs.material(c).is_some()).unwrap_or(candidates[0]).to_owned()
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
                let bounds = scene
                    .entity_solid_ids(*id)
                    .into_iter()
                    .filter(|id| state.doc.is_solid_visible(*id))
                    .filter_map(|id| scene.solids().get(&id).map(|node| node.bounds))
                    .reduce(|a, b| a.union(&b))
                    .map(|b| (b.min, b.max));
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
        self.dust.retain(|id, _| self.visible.iter().any(|(index, _)| self.sources[*index].id == *id));
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
            if !fx.frozen && matches!(fx.kind, FxKind::Cloud | FxKind::Motes) {
                self.dust.entry(fx.id).or_insert_with(|| Dust::new(fx)).draw(
                    state.frame.dt,
                    scale,
                    eye,
                    state.cameras.cam3d.forward(),
                    &mut pts,
                );
            } else {
                particles_in_view(fx, self.time, scale, eye, state.cameras.cam3d.forward(), &mut pts);
            }
            for p in &pts {
                fx_sprites.push(ParticleSprite {
                    ribbon: None,
                    animation: hp_render::ParticleAnimation::default(),
                    pos: p.pos,
                    size: p.radius * 2.0,
                    axis: [0.0; 3],
                    roll: p.roll,
                    yaw: p.yaw,
                    uv: hp_pcf::sheet::FULL_UV,
                    color: [p.color[0], p.color[1], p.color[2], p.color[3].clamp(0.0, 1.0)],
                    bright: look.brightness(),
                    add_self: look.blend_parameter(),
                    orient: if matches!(fx.kind, FxKind::Smoke | FxKind::Steam) && !fx.heatwave {
                        if fx.emissive { ParticleOrient::ScreenEmissive } else { ParticleOrient::ScreenLit }
                    } else {
                        ParticleOrient::Screen
                    },
                    blend: look.blend(),
                    screen_fade: look.fade_size,
                    refraction_blur: look.refraction_blur,
                    no_fog: look.no_fog,
                    material,
                    entity: fx.id,
                    draw_order: 0,
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
            .filter(|(_, s)| s.enabled && s.count > 0 && s.alpha > 0.0 && state.doc.is_entity_visible(s.id))
            .filter_map(|(i, s)| {
                let d = distance_to_box(cam, s.min, s.max);
                let in_range = if matches!(s.kind, FxKind::Cloud | FxKind::Motes) {
                    let forward = state.cameras.cam3d.forward();
                    let depth = ((s.min + s.max) * 0.5 - cam).dot(forward);
                    let radius = ((s.max - s.min) * 0.5).dot(forward.abs());
                    depth - radius <= s.reach && depth + radius > 0.0
                } else {
                    d <= s.reach
                };
                in_range.then_some((d, i))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(MAX_ENTITIES);
        let mut budget = MAX_PARTICLES;
        self.visible.clear();
        for (_, i) in near {
            if budget < MIN_COUNT {
                break;
            }
            let s = &self.sources[i];
            let n = s.count.min(budget);
            if n == 0 {
                continue;
            }
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
    fn supported_volume_and_jet_classes_are_previewed() {
        for c in [
            "func_smokevolume",
            "env_embers",
            "func_dustcloud",
            "func_dustmotes",
            "FUNC_DUSTMOTES",
            "env_steam",
            "env_steamjet",
        ] {
            assert!(FxKind::of(c).is_some(), "{c}");
        }
        assert!(FxKind::of("info_particle_system").is_none());
        assert!(build(1, &entity("prop_static", &[]), bounds()).is_none());
    }

    #[test]
    fn dust_loses_horizontal_speed_without_wind_but_preserves_fall_speed() {
        let state = hp_app::testing::test_state();
        wind::update(&state);
        let fx = build(72, &entity("func_dustcloud", &[("SpawnRate", "1")]), bounds()).unwrap();
        let mut dust = Dust::new(&fx);
        dust.particles = vec![DustParticle {
            seed: 1,
            pos: DVec3::ZERO,
            velocity: DVec3::new(20.0, -10.0, -7.0),
            age: 0.0,
            life: 10.0,
        }];
        let mut output = Vec::new();
        dust.draw(0.1, 1.0, eye(), DVec3::X, &mut output);
        assert!(dust.particles[0].velocity.distance(DVec3::new(15.0, -5.0, -7.0)) < 1e-5);
        assert!(dust.particles[0].pos.distance(DVec3::new(1.5, -0.5, -0.7)) < 1e-5);
        for _ in 0..3 {
            dust.draw(0.1, 1.0, eye(), DVec3::X, &mut output);
        }
        assert!(dust.particles[0].velocity.distance(DVec3::new(0.0, 0.0, -7.0)) < 1e-5);
    }

    #[test]
    fn ember_wind_is_opt_in_and_follows_the_controller_direction() {
        let mut state = hp_app::testing::test_state();
        let mut map = hp_vmf::Map::new_empty();
        let mut controller = entity(
            "env_wind",
            &[
                ("angles", "0 90 0"),
                ("minwind", "400"),
                ("maxwind", "400"),
                ("mingustdelay", "1000"),
                ("maxgustdelay", "1000"),
            ],
        );
        controller.id = 90;
        map.entities.push(controller);
        state.doc = hp_editor::Document::from_map(map);
        state.frame.time = 1.0;
        wind::update(&state);
        state.frame.time = 4.0;
        wind::update(&state);
        let off = build(71, &entity("env_embers", &[("AffectedByWind", "0"), ("speed", "0")]), bounds()).unwrap();
        let on = build(71, &entity("env_embers", &[("AffectedByWind", "1"), ("speed", "0")]), bounds()).unwrap();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        particles(&off, 1.0, 1.0, eye(), &mut a);
        particles(&on, 1.0, 1.0, eye(), &mut b);
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(a, b)| b.pos[1] >= a.pos[1]));
        assert!(a.iter().zip(&b).any(|(a, b)| b.pos[1] - a.pos[1] > 100.0));
        state.doc = hp_editor::Document::from_map(hp_vmf::Map::new_empty());
        wind::update(&state);
    }

    #[test]
    fn rates_sizes_and_jet_direction_match_the_entity_parameters() {
        for class in ["env_embers", "func_dustcloud", "func_dustmotes"] {
            let fx = build(
                1,
                &entity(class, &[("density", "0"), ("SpawnRate", "0"), ("SizeMin", "0.02"), ("SizeMax", "0.02")]),
                bounds(),
            )
            .unwrap();
            assert_eq!(fx.count, 0);
        }
        let cloud = build(1, &entity("func_dustcloud", &[("SizeMin", "10"), ("SizeMax", "10")]), bounds()).unwrap();
        let motes = build(1, &entity("func_dustmotes", &[("SizeMin", "10"), ("SizeMax", "10")]), bounds()).unwrap();
        assert_eq!(dust_radius(&cloud, 0.5, 1000.0), 10.0);
        assert_eq!(dust_radius(&motes, 0.5, 1000.0), 1.0);
        let steam = build(
            1,
            &entity(
                "env_steam",
                &[
                    ("origin", "0 0 0"),
                    ("angles", "-90 0 0"),
                    ("Speed", "100"),
                    ("JetLength", "200"),
                    ("SpreadSpeed", "0"),
                    ("Rate", "10"),
                    ("spawnflags", "1"),
                    ("InitialState", "1"),
                ],
            ),
            None,
        )
        .unwrap();
        assert_eq!(steam.count, 20);
        assert!(steam.enabled);
        assert_eq!(steam.life, [2.0, 2.0]);
        let mut points = Vec::new();
        particles(&steam, 1.0, 1.0, eye(), &mut points);
        assert!(
            points
                .iter()
                .all(|p| p.pos[0].abs() < 0.001 && p.pos[1].abs() < 0.001 && p.pos[2] >= 0.0 && p.pos[2] <= 200.0)
        );
        let smoke = build(
            1,
            &entity(
                "func_smokevolume",
                &[("ParticleDrawWidth", "5"), ("ParticleSpacingDistance", "2"), ("Density", "0.2")],
            ),
            Some((DVec3::ZERO, DVec3::splat(8.0))),
        )
        .unwrap();
        assert_eq!(smoke.grid, [4; 3]);
        assert_eq!(smoke.size, [2.5; 2]);
        assert_eq!(smoke.alpha, 0.2);
    }

    #[test]
    fn steam_start_state_and_heatwave_are_independent_of_emissive() {
        for flags in [0, 1] {
            for active in [0, 1] {
                let e = entity(
                    "env_steam",
                    &[("spawnflags", &flags.to_string()), ("InitialState", &active.to_string()), ("type", "1")],
                );
                let fx = build(1, &e, None).unwrap();
                assert_eq!(fx.enabled, active == 1);
                assert_eq!(fx.emissive, flags == 1);
                assert!(fx.heatwave);
                assert_eq!(fx.material, "sprites/heatwave");
            }
        }
        assert!(!build(1, &entity("env_steam", &[("spawnflags", "1")]), None).unwrap().enabled);
    }

    #[test]
    fn dust_and_motes_use_camera_depth_instead_of_radial_distance() {
        for class in ["func_dustcloud", "func_dustmotes"] {
            let fx = build(
                1,
                &entity(class, &[("Frozen", "1"), ("DistMax", "1000")]),
                Some((DVec3::new(100.0, 0.0, 0.0), DVec3::new(200.0, 40.0, 40.0))),
            )
            .unwrap();
            let mut a = Vec::new();
            let mut b = Vec::new();
            particles_in_view(&fx, 1.0, 1.0, DVec3::ZERO, DVec3::X, &mut a);
            particles_in_view(&fx, 1.0, 1.0, DVec3::new(0.0, 800.0, 0.0), DVec3::X, &mut b);
            assert!(!a.is_empty());
            assert_eq!(a, b);
            let mut behind = Vec::new();
            particles_in_view(&fx, 1.0, 1.0, DVec3::ZERO, -DVec3::X, &mut behind);
            assert!(behind.is_empty());
        }
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
        assert_eq!(embers.count, 60);
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
        assert!(build(1, &entity("env_embers", &[]), bounds()).unwrap().enabled);
        assert!(build(1, &entity("env_embers", &[("spawnflags", "1")]), bounds()).unwrap().enabled);
        assert!(
            !build(1, &entity("env_embers", &[("spawnflags", "2"), ("targetname", "controlled")]), bounds())
                .unwrap()
                .enabled
        );
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
    fn particle_volumes_require_real_brush_bounds() {
        for class in ["env_embers", "func_smokevolume", "func_dustcloud", "func_dustmotes"] {
            let e = entity(class, &[("origin", "100 200 300")]);
            assert!(build(9, &e, None).is_none());
            assert!(build(9, &e, Some((DVec3::ZERO, DVec3::ZERO))).is_none());
            assert!(build(9, &e, bounds()).is_some());
        }
    }

    #[test]
    fn bounded_preview_particles_remain_finite_and_animate() {
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
        assert_eq!(pick_material(None, FxKind::Embers, ""), "particle/fire");
        assert_eq!(pick_material(None, FxKind::Motes, ""), "particle/sparkles");
        assert_eq!(pick_material(None, FxKind::Smoke, "particle/custom"), "particle/custom");
        assert_eq!(pick_material(None, FxKind::Cloud, ""), "particle/sparkles");
        assert_eq!(
            pick_material(Some(&hp_fs::GameFs::empty()), FxKind::Smoke, "csgo/particle/missing"),
            "csgo/particle/missing"
        );
    }

    #[test]
    fn the_layer_is_bounded_and_follows_the_setting() {
        let mut state = hp_app::testing::test_state();
        let mut map = hp_vmf::Map::new_empty();
        let brush = hp_vmf::parse(hp_app::testing::TEST_VMF).unwrap().world.solids.remove(0);
        for i in 0..60 {
            let mut e = hp_vmf::Entity { id: 100 + i, classname: "env_embers".into(), ..Default::default() };
            e.set("origin", format!("{} 0 0", i * 16));
            e.set("density", "96");
            e.set("spawnflags", "1");
            let mut solid = brush.clone();
            solid.id = 1000 + i;
            for (side, surface) in solid.sides.iter_mut().enumerate() {
                surface.id = 2000 + i * 10 + side as u32;
            }
            e.solids.push(solid);
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
            assert_eq!(shared.fx_materials, vec!["particle/fire".to_owned()]);
            assert!(shared.fx_sprites.iter().all(|s| s.material == 0));
        }
        let brushes: Vec<_> = (0..60).map(|i| hp_editor::ObjRef::Solid(1000 + i)).collect();
        state.doc.set_hidden(&brushes, true);
        layer.prepare(&state);
        assert_eq!(layer.visible_count(), 0, "hidden brushes cannot leave a fictional particle volume");
        state.doc.set_hidden(&brushes, false);
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
