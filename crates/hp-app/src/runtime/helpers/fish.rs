use glam::DVec3;
use hp_vmf::Entity;

pub const DEFAULT_MODEL: &str = "models/Junkola.mdl";
pub const DEFAULT_COUNT: u32 = 10;
pub const DEFAULT_RANGE: f64 = 150.0;
pub const MAX_RANGE: f64 = 255.0;
pub const MAX_PREVIEW_FISH: u32 = 256;
pub const SURFACE_MARGIN: f64 = 8.0;
const BOTTOM_MARGIN: f64 = 4.0;
const MAX_SWIM_RADIUS: f64 = 24.0;
const MAX_DEPTH_SPREAD: f64 = 16.0;
const ATTEMPTS: u64 = 24;

pub fn is_fish_pool(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case("func_fish_pool")
}

#[derive(Clone, Debug, PartialEq)]
pub struct FishPool {
    pub origin: DVec3,
    pub model: String,
    pub count: u32,
    pub range: f64,
}

fn number(e: &Entity, key: &str) -> Option<f64> {
    e.get(key).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

pub fn fish_range(e: &Entity) -> f64 {
    number(e, "max_range").unwrap_or(DEFAULT_RANGE).clamp(0.0, MAX_RANGE)
}

pub fn fish_pool(e: &Entity) -> Option<FishPool> {
    if !is_fish_pool(&e.classname) {
        return None;
    }
    let origin = e.origin().filter(|o| o.is_finite())?;
    let model = e.get("model").map(str::trim).filter(|m| !m.is_empty()).unwrap_or(DEFAULT_MODEL).to_owned();
    let count = number(e, "fish_count").map_or(DEFAULT_COUNT, |c| c.clamp(0.0, f64::from(MAX_PREVIEW_FISH)) as u32);
    Some(FishPool { origin, model, count, range: fish_range(e) })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fish {
    pub pos: DVec3,
    pub yaw: f64,
    pub swim_radius: f64,
    pub phase: f64,
    pub speed: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterBox {
    pub min: DVec3,
    pub max: DVec3,
}

impl WaterBox {
    pub fn contains(&self, p: DVec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64, fish: u64, attempt: u64) -> Self {
        Self(splitmix(seed ^ splitmix(fish.wrapping_mul(0x1000_0001).wrapping_add(attempt))))
    }

    fn unit(&mut self) -> f64 {
        self.0 = splitmix(self.0);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn in_water(water: &[WaterBox], p: DVec3) -> bool {
    water.iter().any(|w| w.contains(p))
}

pub fn place_fish(seed: u64, pool: &FishPool, water: &[WaterBox]) -> Vec<Fish> {
    let home = water.iter().find(|w| w.contains(pool.origin)).copied();
    let range = pool.range.clamp(0.0, MAX_RANGE);
    let keep = |p: DVec3| -> DVec3 {
        match home {
            Some(h) => {
                let top = h.max.z - SURFACE_MARGIN;
                let bottom = (h.min.z + BOTTOM_MARGIN).min(top);
                DVec3::new(p.x, p.y, p.z.clamp(bottom, top))
            }
            None => p,
        }
    };
    let spread = MAX_DEPTH_SPREAD.min(range * 0.15);
    (0..u64::from(pool.count))
        .map(|i| {
            let mut chosen = None;
            for attempt in 0..ATTEMPTS {
                let mut rng = Rng::new(seed, i, attempt);
                let r = range * rng.unit().sqrt();
                let theta = rng.unit() * std::f64::consts::TAU;
                let dz = (rng.unit() * 2.0 - 1.0) * spread;
                let mut offset = DVec3::new(r * theta.cos(), r * theta.sin(), dz);
                if offset.length() > range {
                    offset *= range / offset.length();
                }
                let p = keep(pool.origin + offset);
                if (p - pool.origin).length() <= range + 1e-9 && (home.is_none() || in_water(water, p)) {
                    chosen = Some((p, rng.unit(), rng.unit(), rng.unit()));
                    break;
                }
            }
            let (pos, a, b, c) = chosen.unwrap_or_else(|| {
                let mut rng = Rng::new(seed, i, ATTEMPTS);
                (keep(pool.origin), rng.unit(), rng.unit(), rng.unit())
            });
            let flat = DVec3::new(pos.x - pool.origin.x, pos.y - pool.origin.y, 0.0).length();
            let dz = pos.z - pool.origin.z;
            let room = ((range * range - dz * dz).max(0.0).sqrt() - flat) * 0.5;
            let mut swim_radius = MAX_SWIM_RADIUS.min(range * 0.2).min(room.max(0.0));
            let phase = b * std::f64::consts::TAU;
            if home.is_some() && swim_radius > 0.0 {
                let fits = (0..8).all(|k| {
                    let t = phase + f64::from(k) / 8.0 * std::f64::consts::TAU;
                    let centre = pos - DVec3::new(phase.cos(), phase.sin(), 0.0) * swim_radius;
                    in_water(water, centre + DVec3::new(t.cos(), t.sin(), 0.0) * swim_radius)
                });
                if !fits {
                    swim_radius = 0.0;
                }
            }
            Fish { pos, yaw: a * 360.0, swim_radius, phase, speed: 0.35 + c * 0.45 }
        })
        .collect()
}

pub fn fish_at(f: &Fish, t: f64) -> (DVec3, f64) {
    if f.swim_radius <= 0.0 || !t.is_finite() {
        return (f.pos, f.yaw);
    }
    let a = f.phase + f.speed * t;
    let centre = f.pos - DVec3::new(f.phase.cos(), f.phase.sin(), 0.0) * f.swim_radius;
    let p = centre + DVec3::new(a.cos(), a.sin(), 0.0) * f.swim_radius;
    (p, (a.to_degrees() + 90.0).rem_euclid(360.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { id: 7, classname: "func_fish_pool".to_owned(), ..Default::default() };
        e.set("origin", "100 200 -64");
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    #[test]
    fn keyvalues_and_defaults() {
        let p = fish_pool(&pool(&[])).expect("pool");
        assert_eq!((p.model.as_str(), p.count, p.range), (DEFAULT_MODEL, 10, 150.0));
        assert_eq!(p.origin, DVec3::new(100.0, 200.0, -64.0));
        let p = fish_pool(&pool(&[("model", " models/fish.mdl "), ("fish_count", "3.0"), ("max_range", "999")]))
            .expect("pool");
        assert_eq!((p.model.as_str(), p.count, p.range), ("models/fish.mdl", 3, MAX_RANGE));
        let p = fish_pool(&pool(&[("fish_count", "-4"), ("max_range", "-1")])).expect("pool");
        assert_eq!((p.count, p.range), (0, 0.0));
        assert_eq!(fish_pool(&pool(&[("fish_count", "100000")])).map(|p| p.count), Some(MAX_PREVIEW_FISH));
        assert_eq!(fish_pool(&pool(&[("fish_count", "many")])).map(|p| p.count), Some(DEFAULT_COUNT));
        let mut other = pool(&[]);
        other.classname = "prop_static".to_owned();
        assert!(fish_pool(&other).is_none());
    }

    #[test]
    fn placement_is_deterministic_and_within_range() {
        let p = fish_pool(&pool(&[("fish_count", "40"), ("max_range", "120")])).expect("pool");
        let a = place_fish(7, &p, &[]);
        assert_eq!(a, place_fish(7, &p, &[]), "same pool, same fish");
        assert_ne!(a, place_fish(8, &p, &[]), "another pool scatters differently");
        assert_eq!(a.len(), 40);
        for f in &a {
            assert!((f.pos - p.origin).length() <= 120.0 + 1e-9, "{f:?}");
            for k in 0..50 {
                let (q, yaw) = fish_at(f, f64::from(k) * 0.37);
                assert!((q - p.origin).length() <= 120.0 + 1e-6, "the swim stays in range: {q:?}");
                assert!((0.0..360.0).contains(&yaw));
            }
        }
        let spread = a.iter().map(|f| (f.pos - p.origin).length()).fold(0.0, f64::max);
        assert!(spread > 60.0, "the fish are scattered, not stacked: {spread}");
        let none = fish_pool(&pool(&[("max_range", "0")])).expect("pool");
        assert!(place_fish(7, &none, &[]).iter().all(|f| f.pos == none.origin && f.swim_radius == 0.0));
    }

    #[test]
    fn fish_stay_under_the_water_surface() {
        let p = fish_pool(&pool(&[("fish_count", "30"), ("max_range", "200")])).expect("pool");
        let water = [WaterBox { min: DVec3::new(0.0, 150.0, -128.0), max: DVec3::new(400.0, 400.0, -60.0) }];
        let fish = place_fish(7, &p, &water);
        assert_eq!(fish.len(), 30);
        for f in &fish {
            assert!(f.pos.z <= -60.0 - SURFACE_MARGIN + 1e-9, "below the surface: {f:?}");
            assert!(water[0].contains(f.pos), "inside the water: {f:?}");
            for k in 0..20 {
                assert!(water[0].contains(fish_at(f, f64::from(k)).0), "the swim stays in the water");
            }
        }
        let dry = fish_pool(&pool(&[("origin", "1000 1000 1000"), ("fish_count", "5")])).expect("pool");
        assert_eq!(place_fish(7, &dry, &water).len(), 5, "out of the water the fish are just scattered");
    }
}
