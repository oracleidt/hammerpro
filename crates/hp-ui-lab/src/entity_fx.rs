use glam::DVec3;
use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_editor::{Id, ObjRef};
use hp_render::{Overlay, OverlayLine, OverlayTri};
use hp_ui_viewports::layers::OverlayLayer;

use crate::particles::ParticleHandle;

pub const MAX_ENTITIES: usize = 24;
pub const MAX_PARTICLES: usize = 1600;
pub const MAX_PER_ENTITY: usize = 120;
pub const MAX_DISTANCE: f64 = 3072.0;
const MIN_COUNT: usize = 4;
const POINT_BOX: f64 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub pos: [f32; 3],
    pub radius: f32,
    pub color: [f32; 4],
}

fn num(e: &hp_vmf::Entity, key: &str, default: f32) -> f32 {
    e.get(key).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
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

pub fn build(id: Id, e: &hp_vmf::Entity, bounds: Option<(DVec3, DVec3)>) -> Option<Fx> {
    let kind = FxKind::of(&e.classname)?;
    let (min, max) = bounds.filter(|(a, b)| a.is_finite() && b.is_finite()).or_else(|| {
        let o = e.origin().filter(|o| o.is_finite())?;
        Some((o - DVec3::splat(POINT_BOX), o + DVec3::splat(POINT_BOX)))
    })?;
    let ext = (max - min).max(DVec3::splat(1.0));
    let volume = ext.x * ext.y * ext.z;
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
    };
    match kind {
        FxKind::Smoke => {
            fx.color_a = color(e, "Color1");
            fx.color_b = color(e, "Color2");
            let width = num(e, "ParticleDrawWidth", 120.0).clamp(8.0, 1024.0);
            let spacing = f64::from(num(e, "ParticleSpacingDistance", 80.0).clamp(16.0, 1024.0));
            let density = num(e, "Density", 1.0).clamp(0.0, 4.0);
            fx.count = ((volume / spacing.powi(3)) as usize).clamp(6, 80);
            let cap = (ext.min_element() as f32 * 0.6).max(12.0);
            let radius = (width * 0.5).min(cap);
            fx.size = [radius * 0.8, radius];
            fx.life = [30.0, 30.0];
            fx.speed = num(e, "MovementSpeed", 10.0).clamp(0.0, 512.0);
            fx.alpha = (0.07 + 0.1 * density).clamp(0.05, 0.5);
        }
        FxKind::Embers => {
            fx.color_a = color(e, "rendercolor");
            fx.color_b = fx.color_a;
            fx.count = (num(e, "density", 50.0).clamp(0.0, 4096.0) as usize).clamp(MIN_COUNT, MAX_PER_ENTITY);
            let life = num(e, "lifetime", 4.0).clamp(0.2, 60.0);
            fx.life = [life * 0.7, life];
            fx.speed = num(e, "speed", 32.0).clamp(0.0, 1024.0);
            fx.size = [1.2, 2.6];
            fx.alpha = 0.9;
        }
        FxKind::Cloud | FxKind::Motes => {
            fx.color_a = color(e, "Color");
            fx.color_b = fx.color_a;
            let size_min = num(e, "SizeMin", 10.0).clamp(0.5, 512.0);
            let size_max = num(e, "SizeMax", 20.0).clamp(size_min, 512.0);
            let (life_min, life_max) = if kind == FxKind::Cloud { (3.0, 5.0) } else { (5.0, 10.0) };
            let life_min = num(e, "LifetimeMin", life_min).clamp(0.2, 120.0);
            let life_max = num(e, "LifetimeMax", life_max).clamp(life_min, 120.0);
            let rate = num(e, "SpawnRate", 40.0).clamp(0.0, 1000.0);
            fx.count = ((rate * (life_min + life_max) * 0.5) as usize).clamp(MIN_COUNT, MAX_PER_ENTITY);
            let k = if kind == FxKind::Cloud { 0.4 } else { 0.22 };
            fx.size = [size_min * k, size_max * k];
            fx.life = [life_min, life_max];
            fx.speed = num(e, "SpeedMax", 13.0).clamp(0.0, 512.0);
            fx.fall = num(e, "FallSpeed", 0.0).clamp(-512.0, 512.0);
            fx.alpha = (num(e, "Alpha", 30.0).clamp(0.0, 255.0) / 255.0 * 2.5).clamp(0.05, 0.6);
            let dist = f64::from(num(e, "DistMax", 1024.0)).clamp(128.0, MAX_DISTANCE);
            fx.reach = dist.max(512.0);
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

fn wrap(v: f64, min: f64, max: f64) -> f64 {
    let ext = (max - min).max(1e-6);
    min + (v - min).rem_euclid(ext)
}

pub fn particles(fx: &Fx, time: f64, scale: f32, out: &mut Vec<Pt>) {
    let n = ((fx.count as f32 * scale.clamp(0.0, 1.0)).ceil() as usize).min(fx.count);
    let seed_base = u64::from(fx.id).wrapping_mul(0x2545_F491_4F6C_DD1D);
    for i in 0..n as u64 {
        let seed = mix(seed_base ^ i.wrapping_mul(0x9E37_79B9));
        let life = lerp(fx.life[0], fx.life[1], rnd(seed, 0)).max(0.2);
        let phase = ((time / f64::from(life)) + f64::from(rnd(seed, 1))).rem_euclid(1.0) as f32;
        let age = phase * life;
        let ext = fx.max - fx.min;
        let start = fx.min
            + DVec3::new(
                ext.x * f64::from(rnd(seed, 2)),
                ext.y * f64::from(rnd(seed, 3)),
                ext.z * f64::from(rnd(seed, 4)),
            );
        let theta = rnd(seed, 5) * std::f32::consts::TAU;
        let z = rnd(seed, 6) * 2.0 - 1.0;
        let r = (1.0 - z * z).max(0.0).sqrt();
        let dir = DVec3::new(f64::from(r * theta.cos()), f64::from(r * theta.sin()), f64::from(z));
        let speed = fx.speed * lerp(0.3, 1.0, rnd(seed, 7));
        let mut v = match fx.kind {
            FxKind::Embers => {
                let wob = f64::from((age * 2.3 + rnd(seed, 8) * 6.0).sin() * fx.speed * 0.25);
                DVec3::new(dir.x * f64::from(speed) * 0.3 + wob, dir.y * f64::from(speed) * 0.3, f64::from(speed) * 0.6)
            }
            _ => dir * f64::from(speed),
        };
        v.z -= f64::from(fx.fall);
        let moved = start + v * f64::from(age);
        let pos = DVec3::new(
            wrap(moved.x, fx.min.x, fx.max.x),
            wrap(moved.y, fx.min.y, fx.max.y),
            wrap(moved.z, fx.min.z, fx.max.z),
        );
        let fade = match fx.kind {
            FxKind::Smoke => 1.0,
            _ => (phase * std::f32::consts::PI).sin().max(0.0),
        };
        let tint = rnd(seed, 9);
        let rgb = [
            lerp(fx.color_a[0], fx.color_b[0], tint),
            lerp(fx.color_a[1], fx.color_b[1], tint),
            lerp(fx.color_a[2], fx.color_b[2], tint),
        ];
        let radius = lerp(fx.size[0], fx.size[1], rnd(seed, 10));
        out.push(Pt {
            pos: [pos.x as f32, pos.y as f32, pos.z as f32],
            radius,
            color: [rgb[0], rgb[1], rgb[2], (fx.alpha * fade).clamp(0.0, 1.0)],
        });
    }
}

fn distance_to_box(p: DVec3, min: DVec3, max: DVec3) -> f64 {
    let c = p.clamp(min, max);
    (p - c).length()
}

pub struct EntityFxLayer {
    handle: ParticleHandle,
    sources: Vec<Fx>,
    key: Option<(u64, u64)>,
    visible: Vec<(usize, f32)>,
    time: f64,
}

impl EntityFxLayer {
    pub fn new(handle: ParticleHandle) -> Self {
        Self { handle, sources: Vec::new(), key: None, visible: Vec::new(), time: 0.0 }
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
    }

    fn set_active(&self, active: bool) {
        if let Ok(mut s) = self.handle.lock()
            && s.fx_active != active
        {
            s.fx_active = active;
        }
    }
}

impl OverlayLayer for EntityFxLayer {
    fn name(&self) -> &'static str {
        "lab_entity_fx"
    }

    fn prepare(&mut self, state: &AppState) {
        if !state.settings.lab.show_entity_fx {
            self.visible.clear();
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
            .filter(|(_, s)| state.doc.is_entity_visible(s.id))
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
            let scale = (1.0 - 0.75 * (d / reach)).clamp(0.25, 1.0) as f32;
            let n = ((s.count as f32 * scale).ceil() as usize).min(s.count).min(budget);
            budget -= n;
            self.visible.push((i, n as f32 / s.count.max(1) as f32));
        }
        self.set_active(!self.visible.is_empty());
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if view.kind.is_2d() || self.visible.is_empty() {
            return;
        }
        let r = view.cam3d.right().as_vec3().to_array();
        let u = view.cam3d.up().as_vec3().to_array();
        const HEX: [(f32, f32); 6] =
            [(1.0, 0.0), (0.5, 0.866), (-0.5, 0.866), (-1.0, 0.0), (-0.5, -0.866), (0.5, -0.866)];
        let mut pts: Vec<Pt> = Vec::with_capacity(MAX_PER_ENTITY);
        for &(i, scale) in &self.visible {
            let fx = &self.sources[i];
            let selected = state.doc.is_selected(ObjRef::Entity(fx.id));
            push_box(out, fx, if selected { 0.9 } else { 0.35 });
            pts.clear();
            particles(fx, self.time, scale, &mut pts);
            for p in &pts {
                let c: [[f32; 3]; 6] = std::array::from_fn(|k| {
                    let (a, b) = HEX[k];
                    [
                        p.pos[0] + (r[0] * a + u[0] * b) * p.radius,
                        p.pos[1] + (r[1] * a + u[1] * b) * p.radius,
                        p.pos[2] + (r[2] * a + u[2] * b) * p.radius,
                    ]
                });
                for (a, b, d) in [(0, 1, 2), (0, 2, 3), (0, 3, 4), (0, 4, 5)] {
                    out.triangles.push(OverlayTri { p: [c[a], c[b], c[d]], color: p.color });
                }
            }
        }
    }
}

fn push_box(out: &mut Overlay, fx: &Fx, alpha: f32) {
    let c = [fx.color_a[0].max(0.35), fx.color_a[1].max(0.35), fx.color_a[2].max(0.35), alpha];
    let (a, b) = (fx.min, fx.max);
    let v = |x: bool, y: bool, z: bool| {
        [(if x { b.x } else { a.x }) as f32, (if y { b.y } else { a.y }) as f32, (if z { b.z } else { a.z }) as f32]
    };
    for (p, q) in [
        ((false, false, false), (true, false, false)),
        ((false, true, false), (true, true, false)),
        ((false, false, true), (true, false, true)),
        ((false, true, true), (true, true, true)),
        ((false, false, false), (false, true, false)),
        ((true, false, false), (true, true, false)),
        ((false, false, true), (false, true, true)),
        ((true, false, true), (true, true, true)),
        ((false, false, false), (false, false, true)),
        ((true, false, false), (true, false, true)),
        ((false, true, false), (false, true, true)),
        ((true, true, false), (true, true, true)),
    ] {
        out.lines.push(OverlayLine { a: v(p.0, p.1, p.2), b: v(q.0, q.1, q.2), color: c });
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
                ],
            ),
            bounds(),
        )
        .unwrap();
        assert_eq!(smoke.color_a, [1.0, 0.0, 0.0]);
        assert_eq!(smoke.color_b, [0.0, 0.0, 1.0]);
        assert_eq!(smoke.size[1], 30.0);
        assert!(smoke.count > 6 && smoke.count <= 80);
        let embers = build(
            2,
            &entity(
                "env_embers",
                &[("density", "30"), ("lifetime", "2"), ("speed", "64"), ("rendercolor", "255 128 0")],
            ),
            bounds(),
        )
        .unwrap();
        assert_eq!(embers.count, 30);
        assert_eq!(embers.life[1], 2.0);
        assert_eq!(embers.speed, 64.0);
        let huge = build(3, &entity("env_embers", &[("density", "100000")]), bounds()).unwrap();
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
        let motes = build(5, &entity("func_dustmotes", &[("FallSpeed", "20"), ("Alpha", "100")]), bounds()).unwrap();
        assert_eq!(motes.fall, 20.0);
        assert!(motes.alpha > dust.alpha);
    }

    #[test]
    fn point_entities_get_a_small_box() {
        let e = entity("env_embers", &[("origin", "100 200 300")]);
        let fx = build(9, &e, None).unwrap();
        assert_eq!(fx.min, DVec3::new(52.0, 152.0, 252.0));
        assert!(build(9, &entity("env_embers", &[]), None).is_none());
    }

    #[test]
    fn particles_stay_in_the_volume_and_move() {
        for class in ["func_smokevolume", "env_embers", "func_dustcloud", "func_dustmotes"] {
            let fx = build(11, &entity(class, &[("FallSpeed", "30")]), bounds()).unwrap();
            let mut a = Vec::new();
            let mut b = Vec::new();
            particles(&fx, 3.0, 1.0, &mut a);
            particles(&fx, 3.5, 1.0, &mut b);
            assert_eq!(a.len(), fx.count);
            for p in a.iter().chain(&b) {
                assert!(p.pos[0] >= fx.min.x as f32 - 0.01 && p.pos[0] <= fx.max.x as f32 + 0.01, "{class}");
                assert!(p.pos[1] >= fx.min.y as f32 - 0.01 && p.pos[1] <= fx.max.y as f32 + 0.01, "{class}");
                assert!(p.pos[2] >= fx.min.z as f32 - 0.01 && p.pos[2] <= fx.max.z as f32 + 0.01, "{class}");
                assert!(p.radius > 0.0 && (0.0..=1.0).contains(&p.color[3]));
            }
            assert!(a.iter().zip(&b).any(|(p, q)| p.pos != q.pos), "{class} animates");
            let mut half = Vec::new();
            particles(&fx, 3.0, 0.5, &mut half);
            assert!(half.len() < a.len() || a.len() <= MIN_COUNT);
        }
    }

    #[test]
    fn the_layer_is_bounded_and_follows_the_setting() {
        let mut state = hp_app::testing::test_state();
        let mut map = hp_vmf::Map::new_empty();
        for i in 0..60 {
            let mut e = hp_vmf::Entity { id: 100 + i, classname: "env_embers".into(), ..Default::default() };
            e.set("origin", format!("{} 0 0", i * 16));
            e.set("density", "96");
            map.entities.push(e);
        }
        state.doc = hp_editor::Document::from_map(map);
        state.cameras.cam3d.pos = DVec3::new(-100.0, 0.0, 0.0);
        let handle = crate::particles::new_handle();
        let mut layer = EntityFxLayer::new(handle.clone());
        layer.prepare(&state);
        assert!(layer.visible_count() > 0 && layer.visible_count() <= MAX_ENTITIES);
        assert!(handle.lock().unwrap().fx_active);
        let total: usize =
            layer.visible.iter().map(|(i, s)| (layer.sources[*i].count as f32 * s).ceil() as usize).sum();
        assert!(total <= MAX_PARTICLES + MAX_ENTITIES);
        state.settings.lab.show_entity_fx = false;
        layer.prepare(&state);
        assert_eq!(layer.visible_count(), 0);
        assert!(!handle.lock().unwrap().fx_active);
    }

    #[test]
    fn setting_is_on_by_default() {
        assert!(hp_app::settings::LabSettings::default().show_entity_fx);
    }
}
