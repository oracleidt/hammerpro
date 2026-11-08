use glam::{DVec3, Vec3, Vec3A};
use hp_vmf::Entity;

const REACH_THRESHOLD: f32 = 0.5;
const MAX_REACH: f32 = 65536.0;

pub fn light_for_string(s: &str) -> Option<Vec3> {
    let mut v = [0.0f64; 4];
    let mut n = 0;
    for part in s.split_ascii_whitespace() {
        if n == 4 {
            break;
        }
        v[n] = part.parse::<f64>().ok().filter(|x| x.is_finite())?;
        n += 1;
    }
    let (rgb, brightness) = match n {
        1 => ([v[0]; 3], 255.0),
        3 => ([v[0], v[1], v[2]], 255.0),
        4 => ([v[0], v[1], v[2]], v[3]),
        _ => return None,
    };
    if rgb.iter().any(|c| *c < 0.0) || brightness < 0.0 {
        return None;
    }
    let lin = |c: f64| ((c / 255.0).powf(2.2) * brightness) as f32;
    let out = Vec3::new(lin(rgb[0]), lin(rgb[1]), lin(rgb[2]));
    out.is_finite().then_some(out)
}

fn key_f(e: &Entity, key: &str) -> Option<f64> {
    e.get(key)
        .and_then(|s| s.split_ascii_whitespace().next())
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

pub fn sin_cos_degrees(angle: f64) -> (f64, f64) {
    match angle.rem_euclid(360.0) {
        0.0 => (0.0, 1.0),
        90.0 => (1.0, 0.0),
        180.0 => (0.0, -1.0),
        270.0 => (-1.0, 0.0),
        _ => angle.to_radians().sin_cos(),
    }
}

pub fn light_normal(e: &Entity) -> DVec3 {
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let angle = key_f(e, "angle").unwrap_or(0.0);
    let pitch = key_f(e, "pitch").filter(|p| *p != 0.0).unwrap_or(angles.x);
    let (sp, cp) = sin_cos_degrees(pitch);
    let out = if angle == -1.0 {
        DVec3::Z
    } else if angle == -2.0 {
        DVec3::NEG_Z
    } else {
        let yaw = if angle != 0.0 { angle } else { angles.y };
        let (sy, cy) = sin_cos_degrees(yaw);
        DVec3::new(cp * cy, cp * sy, sp)
    };
    out.try_normalize().unwrap_or(DVec3::NEG_Z)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LightKind {
    Point,
    Spot { dir: Vec3A, inner_cos: f32, outer_cos: f32, exponent: f32 },
    Surface { normal: Vec3A },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub kind: LightKind,
    pub origin: Vec3A,
    pub intensity: Vec3A,
    pub constant: f32,
    pub linear: f32,
    pub quadratic: f32,
    pub cutoff: f32,
    pub fade: (f32, f32),
    pub cap: f32,
    pub entity: u32,
    pub reach: f32,
}

impl Light {
    pub fn falloff(&self, dist: f32) -> f32 {
        let (start, end) = self.fade;
        let hard = end > start;
        if hard && dist > end {
            return 0.0;
        }
        let d = dist.max(1.0);
        let e = d.min(self.cap);
        let denom = self.constant + e * (self.linear + e * self.quadratic);
        let mut f = if denom > 1e-12 { 1.0 / denom } else { 1.0 };
        if hard {
            let t = 1.0 - ((d - start) / (end - start)).clamp(0.0, 1.0);
            f *= t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        }
        f
    }

    pub fn incident(&self, p: Vec3A, n: Vec3A) -> Option<(Vec3A, Vec3A, f32)> {
        let delta = self.origin - p;
        let dist = delta.length();
        if dist.is_nan() || dist <= 1e-4 || dist > self.reach {
            return None;
        }
        let dir = delta / dist;
        let dot = n.dot(dir);
        if dot <= 0.0 {
            return None;
        }
        let mut ratio = dot * self.falloff(dist);
        match self.kind {
            LightKind::Point => {}
            LightKind::Spot { dir: axis, inner_cos, outer_cos, exponent } => {
                let dot2 = -dir.dot(axis);
                if dot2 <= outer_cos {
                    return None;
                }
                if exponent != 0.0 && exponent != 1.0 {
                    ratio *= dot2.max(0.0).powf(exponent);
                } else if exponent == 1.0 {
                    ratio *= dot2;
                }
                if dot2 <= inner_cos && inner_cos > outer_cos {
                    ratio *= (dot2 - outer_cos) / (inner_cos - outer_cos);
                }
            }
            LightKind::Surface { normal } => {
                let dot2 = -dir.dot(normal);
                if dot2 <= 1e-3 {
                    return None;
                }
                ratio *= dot2;
            }
        }
        if ratio.is_nan() || ratio <= 0.0 {
            return None;
        }
        Some((self.intensity * ratio, dir, dist))
    }

    fn compute_reach(&mut self, radius_scale: f32) {
        let peak = self.intensity.max_element();
        let mut reach = MAX_REACH;
        if peak > 0.0 {
            let target = peak / REACH_THRESHOLD;
            let (q, l, c) = (self.quadratic, self.linear, self.constant - target);
            let d = if q > 1e-12 {
                let disc = l * l - 4.0 * q * c;
                if disc > 0.0 { (-l + disc.sqrt()) / (2.0 * q) } else { 0.0 }
            } else if l > 1e-12 {
                -c / l
            } else {
                MAX_REACH
            };
            if d.is_finite() {
                reach = d.max(1.0);
            }
            if self.cap < reach && self.cap > 0.0 {
                reach = reach.max(self.cap);
            }
        } else {
            reach = 0.0;
        }
        let (start, end) = self.fade;
        if end > start {
            reach = reach.min(end);
        }
        let scale = if radius_scale.is_finite() && radius_scale > 0.0 { radius_scale } else { 1.0 };
        self.reach = (reach * scale).clamp(0.0, MAX_REACH);
    }
}

fn solve_inverse_quadratic(x: [f32; 3], y: [f32; 3]) -> Option<(f32, f32, f32)> {
    let [x1, x2, x3] = x;
    let [y1, y2, y3] = y;
    let det = (x1 - x2) * (x1 - x3) * (x2 - x3);
    if det == 0.0 {
        return None;
    }
    let a = (x3 * (-y1 + y2) + x2 * (y1 - y3) + x1 * (-y2 + y3)) / det;
    let b = (x3 * x3 * (y1 - y2) + x1 * x1 * (y2 - y3) + x2 * x2 * (-y1 + y3)) / det;
    let c = (x1 * x3 * (-x1 + x3) * y2 + x2 * x2 * (x3 * y1 - x1 * y3) + x2 * (-(x3 * x3 * y1) + x1 * x1 * y3)) / det;
    Some((a, b, c))
}

fn solve_inverse_quadratic_monotonic(x: [f32; 3], y: [f32; 3]) -> Option<(f32, f32, f32)> {
    let mut last = None;
    let mut f = 0.0f32;
    while f <= 1.0 {
        let lerp = y[0] + (y[2] - y[0]) * (x[1] - x[0]) / (x[2] - x[0]);
        let ty = (1.0 - f) * y[1] + f * lerp;
        let (a, b, c) = solve_inverse_quadratic(x, [y[0], ty, y[2]])?;
        last = Some((a, b, c));
        let derivative = 2.0 * a + b;
        let increasing = y[0] < y[1] && y[1] < y[2];
        let decreasing = y[0] > y[1] && y[1] > y[2];
        if (increasing && derivative >= 0.0) || (decreasing && derivative <= 0.0) || (!increasing && !decreasing) {
            return last;
        }
        f += 0.05;
    }
    last
}

fn set_falloff(l: &mut Light, e: &Entity) {
    let d50 = key_f(e, "_fifty_percent_distance").unwrap_or(0.0) as f32;
    l.cap = 1.0e22;
    l.fade = (0.0, -1.0);
    if d50 > 0.0 {
        let mut d0 = key_f(e, "_zero_percent_distance").unwrap_or(0.0) as f32;
        if d0 < d50 {
            d0 = 2.0 * d50;
        }
        let (mut a, mut b, mut c) = (0.0, 1.0, 0.0);
        if let Some(abc) = solve_inverse_quadratic_monotonic([0.0, d50, d0], [1.0, 2.0, 256.0]) {
            (a, b, c) = abc;
        }
        let v50 = c + d50 * (b + d50 * a);
        if v50.abs() > 1e-12 {
            let scale = 2.0 / v50;
            a *= scale;
            b *= scale;
            c *= scale;
        }
        l.quadratic = a;
        l.linear = b;
        l.constant = c;
        if key_f(e, "_hardfalloff").unwrap_or(0.0) != 0.0 {
            l.fade = (0.75 * d0 + 0.25 * d50, d0);
        } else if a.abs() > 0.0 {
            let max = b / (-2.0 * a);
            if max > 0.0 {
                l.cap = max;
                l.fade = (max, 10.0 * max);
            }
        }
    } else {
        let clamp = |v: f64| if v < 0.001 { 0.0 } else { v as f32 };
        l.constant = clamp(key_f(e, "_constant_attn").unwrap_or(0.0));
        l.linear = clamp(key_f(e, "_linear_attn").unwrap_or(0.0));
        l.quadratic = clamp(key_f(e, "_quadratic_attn").unwrap_or(0.0));
        if l.constant == 0.0 && l.linear == 0.0 && l.quadratic == 0.0 {
            l.constant = 1.0;
        }
        let ratio = l.constant + 100.0 * l.linear + 100.0 * 100.0 * l.quadratic;
        if ratio > 0.0 {
            l.intensity *= ratio;
        }
    }
    let r = key_f(e, "_distance").unwrap_or(0.0) as f32;
    l.cutoff = if r.is_finite() && r > 0.0 { r.min(1.0e6) } else { 0.0 };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sun {
    pub dir: Vec3A,
    pub intensity: Vec3A,
    pub spread_deg: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sky {
    pub sun: Option<Sun>,
    pub ambient: Vec3A,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightSet {
    pub lights: Vec<Light>,
    pub sky: Sky,
}

fn initially_dark(e: &Entity) -> bool {
    let named = e.get("targetname").is_some_and(|t| !t.trim().is_empty());
    let flags = key_f(e, "spawnflags").unwrap_or(0.0) as i64;
    named && flags & 1 != 0
}

fn light_value(e: &Entity, ldr: &str, hdr: &str) -> Option<Vec3> {
    e.get(ldr).and_then(light_for_string).or_else(|| e.get(hdr).and_then(light_for_string))
}

pub fn parse_light(e: &Entity, radius_scale: f32) -> Option<Light> {
    let class = e.classname.as_str();
    if class != "light" && class != "light_spot" {
        return None;
    }
    if initially_dark(e) {
        return None;
    }
    let intensity = light_value(e, "_light", "_lightHDR")?;
    let origin = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO).as_vec3a();
    let mut l = Light {
        kind: LightKind::Point,
        origin,
        intensity: intensity.into(),
        constant: 0.0,
        linear: 0.0,
        quadratic: 1.0,
        cutoff: 0.0,
        fade: (0.0, -1.0),
        cap: 1.0e22,
        entity: e.id,
        reach: 0.0,
    };
    if class == "light_spot" {
        let mut inner = key_f(e, "_inner_cone").unwrap_or(0.0) as f32;
        if inner == 0.0 {
            inner = 10.0;
        }
        let mut outer = key_f(e, "_cone").unwrap_or(0.0) as f32;
        if outer == 0.0 {
            outer = inner;
        }
        if outer < inner {
            outer = inner;
        }
        if outer < 180.0 {
            l.kind = LightKind::Spot {
                dir: light_normal(e).as_vec3a(),
                inner_cos: inner.clamp(0.0, 180.0).to_radians().cos(),
                outer_cos: outer.clamp(0.0, 180.0).to_radians().cos(),
                exponent: key_f(e, "_exponent").unwrap_or(0.0) as f32,
            };
        }
    }
    set_falloff(&mut l, e);
    l.compute_reach(radius_scale);
    (l.reach > 0.0 && l.intensity.max_element() > 0.0).then_some(l)
}

pub fn parse_environment(e: &Entity) -> Option<Sky> {
    if e.classname != "light_environment" {
        return None;
    }
    let sun = light_value(e, "_light", "_lightHDR").filter(|i| i.max_element() > 0.0).map(|i| Sun {
        dir: light_normal(e).as_vec3a(),
        intensity: i.into(),
        spread_deg: key_f(e, "SunSpreadAngle").unwrap_or(0.0).clamp(0.0, 90.0) as f32,
    });
    let ambient = light_value(e, "_ambient", "_ambientHDR").unwrap_or(Vec3::ZERO);
    Some(Sky { sun, ambient: ambient.into() })
}

pub fn collect<'a>(entities: impl IntoIterator<Item = &'a Entity>, radius_scale: f32) -> LightSet {
    let mut out = LightSet::default();
    let mut sky_found = false;
    for e in entities {
        if let Some(l) = parse_light(e, radius_scale) {
            out.lights.push(l);
        } else if !sky_found && let Some(sky) = parse_environment(e) {
            out.sky = sky;
            sky_found = true;
        }
    }
    out
}

pub fn surface_light(center: Vec3A, normal: Vec3A, value: Vec3A, area: f32, radius_scale: f32) -> Light {
    let mut l = Light {
        kind: LightKind::Surface { normal },
        origin: center,
        intensity: value * (area / std::f32::consts::PI),
        constant: 0.0,
        linear: 0.0,
        quadratic: 1.0,
        cutoff: 0.0,
        fade: (0.0, -1.0),
        cap: 1.0e22,
        entity: 0,
        reach: 0.0,
    };
    l.compute_reach(radius_scale);
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(class: &str, props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: class.to_owned(), id: 5, ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    #[test]
    fn cardinal_light_directions_have_no_sideways_component() {
        for yaw in [0.0, 37.0, 90.0, 180.0, 270.0, -90.0] {
            let angles = format!("0 {yaw} 0");
            let e = ent("light_environment", &[("angles", &angles), ("pitch", "-90")]);
            assert_eq!(light_normal(&e), DVec3::NEG_Z);
        }
        assert_eq!(sin_cos_degrees(450.0), (1.0, 0.0));
        assert_eq!(sin_cos_degrees(-180.0), (0.0, -1.0));
        let near = -90.0 + 1e-10;
        assert_eq!(sin_cos_degrees(near), near.to_radians().sin_cos());
        assert_ne!(sin_cos_degrees(near).1, 0.0);
    }

    #[test]
    fn light_strings() {
        let v = light_for_string("255 255 255 200").unwrap();
        assert!((v.x - 200.0).abs() < 1e-3);
        let v = light_for_string("255 128 0").unwrap();
        assert!((v.x - 255.0).abs() < 1e-3);
        assert!((v.y - (128.0f64 / 255.0).powf(2.2) as f32 * 255.0).abs() < 1e-2);
        assert_eq!(v.z, 0.0);
        assert!(light_for_string("100").is_some());
        assert!(light_for_string("-1 -1 -1 1").is_none());
        assert!(light_for_string("1 2").is_none());
        assert!(light_for_string("a b c").is_none());
        assert!(light_for_string("").is_none());
        assert!(light_for_string("1 1 1 nan").is_none());
    }

    #[test]
    fn quadratic_light_is_vrad_like() {
        let e = ent("light", &[("origin", "0 0 100"), ("_light", "255 255 255 200"), ("_quadratic_attn", "1")]);
        let l = parse_light(&e, 1.0).unwrap();
        let (v, dir, dist) = l.incident(Vec3A::ZERO, Vec3A::Z).unwrap();
        assert!((v.x - 200.0).abs() < 0.01, "{v:?}");
        assert!((dist - 100.0).abs() < 1e-4);
        assert!((dir - Vec3A::Z).length() < 1e-6);
        let (v2, ..) = l.incident(Vec3A::new(0.0, 0.0, -100.0), Vec3A::Z).unwrap();
        assert!((v2.x - 50.0).abs() < 0.01);
        assert!(l.incident(Vec3A::ZERO, -Vec3A::Z).is_none());
        assert!((l.reach - 2000.0).abs() < 1.0, "{}", l.reach);
        let short = parse_light(&e, 0.5).unwrap();
        assert!((short.reach - 1000.0).abs() < 1.0);
    }

    #[test]
    fn defaults_and_cutoff() {
        let e = ent("light", &[("_light", "255 255 255 255"), ("_distance", "300")]);
        let l = parse_light(&e, 1.0).unwrap();
        assert_eq!((l.constant, l.linear, l.quadratic), (1.0, 0.0, 0.0));
        assert!((l.falloff(200.0) - 1.0).abs() < 1e-6);
        assert_eq!(l.cutoff, 300.0);
        assert_eq!(l.falloff(301.0), 1.0, "VRAD lights past _distance, only the engine cuts it");
        assert_eq!(l.fade, (0.0, -1.0));
        assert!(l.reach > 300.0);
        let curve =
            ent("light", &[("_light", "255 255 255 255"), ("_fifty_percent_distance", "200"), ("_distance", "250")]);
        let c = parse_light(&curve, 1.0).unwrap();
        assert_eq!(c.cutoff, 250.0);
        assert!(c.falloff(251.0) > 0.0 && c.falloff(150.0) > 0.0);
        let tiny = ent(
            "light",
            &[
                ("_light", "255 255 255 255"),
                ("_constant_attn", "1"),
                ("_linear_attn", "0.01"),
                ("_quadratic_attn", "0.0001"),
            ],
        );
        let t = parse_light(&tiny, 1.0).unwrap();
        assert_eq!((t.constant, t.linear, t.quadratic), (1.0, 0.01, 0.0), "terms under 0.001 read as 0 like VRAD");
        assert!((t.intensity.x - 510.0).abs() < 1e-3, "{}", t.intensity.x);
        let dark = ent("light", &[("_light", "255 255 255 255"), ("targetname", "a"), ("spawnflags", "1")]);
        assert!(parse_light(&dark, 1.0).is_none());
        assert!(parse_light(&ent("info_target", &[("_light", "1 1 1 1")]), 1.0).is_none());
        assert!(parse_light(&ent("light", &[]), 1.0).is_none());
    }

    #[test]
    fn fifty_percent_distance() {
        let e = ent(
            "light",
            &[("_light", "255 255 255 255"), ("_fifty_percent_distance", "200"), ("_zero_percent_distance", "400")],
        );
        let l = parse_light(&e, 1.0).unwrap();
        let f50 = l.falloff(200.0);
        assert!((f50 - 0.5).abs() < 0.01, "{f50}");
        let mut prev = f32::INFINITY;
        for d in (1..800).step_by(7) {
            let f = l.falloff(d as f32);
            assert!(f <= prev + 1e-6, "not decreasing at {d}");
            prev = f;
        }
        assert!(l.falloff(400.0) < f50);
        let mut hard = e.clone();
        hard.set("_hardfalloff", "1");
        let h = parse_light(&hard, 1.0).unwrap();
        assert_eq!(h.falloff(401.0), 0.0);
        assert!(h.reach <= 400.0);
        let (start, end) = h.fade;
        assert_eq!((start, end), (0.75 * 400.0 + 0.25 * 200.0, 400.0));
        let mid = h.falloff(0.5 * (start + end));
        let plain = h.constant + 0.5 * (start + end) * (h.linear + 0.5 * (start + end) * h.quadratic);
        assert!((mid * plain - 0.5).abs() < 1e-3, "the fade is VRAD's quintic curve: {}", mid * plain);
    }

    #[test]
    fn spot_cone() {
        let e = ent(
            "light_spot",
            &[
                ("origin", "0 0 100"),
                ("pitch", "-90"),
                ("_light", "255 255 255 255"),
                ("_inner_cone", "20"),
                ("_cone", "40"),
                ("_quadratic_attn", "1"),
            ],
        );
        let l = parse_light(&e, 1.0).unwrap();
        let LightKind::Spot { dir, .. } = l.kind else { panic!("spot") };
        assert!((dir.z + 1.0).abs() < 1e-5);
        let center = l.incident(Vec3A::ZERO, Vec3A::Z).unwrap().0.x;
        assert!(center > 200.0);
        let p = Vec3A::new(100.0 * 30f32.to_radians().tan(), 0.0, 0.0);
        let fringe = l.incident(p, Vec3A::Z).unwrap().0.x;
        assert!(fringe > 0.0 && fringe < center);
        let p = Vec3A::new(100.0 * 50f32.to_radians().tan(), 0.0, 0.0);
        assert!(l.incident(p, Vec3A::Z).is_none());
        let mut wide = e.clone();
        wide.set("_cone", "180");
        assert_eq!(parse_light(&wide, 1.0).unwrap().kind, LightKind::Point);
    }

    #[test]
    fn environment() {
        let e = ent(
            "light_environment",
            &[("_light", "255 255 255 300"), ("_ambient", "255 255 255 20"), ("angles", "-45 90 0"), ("pitch", "-60")],
        );
        let sky = parse_environment(&e).unwrap();
        let sun = sky.sun.unwrap();
        assert!(sun.dir.z < -0.8, "{:?}", sun.dir);
        assert!((sun.intensity.x - 300.0).abs() < 1e-3);
        assert!((sky.ambient.x - 20.0).abs() < 1e-3);
        let set = collect([&e, &ent("light", &[("_light", "1 1 1 50")])], 1.0);
        assert_eq!(set.lights.len(), 1);
        assert!(set.sky.sun.is_some());
        assert!(parse_environment(&ent("light", &[])).is_none());
    }

    #[test]
    fn normals() {
        assert_eq!(light_normal(&ent("light_spot", &[("angle", "-1")])), DVec3::Z);
        assert_eq!(light_normal(&ent("light_spot", &[("angle", "-2")])), DVec3::NEG_Z);
        let n = light_normal(&ent("light_spot", &[("angles", "0 90 0")]));
        assert!((n - DVec3::Y).length() < 1e-9);
        let bad = light_normal(&ent("light_spot", &[("angles", "nan 0 0")]));
        assert!(bad.is_finite());
    }

    #[test]
    fn surface_lights() {
        let l = surface_light(Vec3A::ZERO, Vec3A::NEG_Z, Vec3A::splat(100.0), 64.0, 1.0);
        let (v, ..) = l.incident(Vec3A::new(0.0, 0.0, -10.0), Vec3A::Z).unwrap();
        assert!(v.x > 0.0);
        assert!(l.incident(Vec3A::new(0.0, 0.0, 10.0), -Vec3A::Z).is_none());
        assert!(l.reach > 10.0);
    }
}
