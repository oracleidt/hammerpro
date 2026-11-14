use glam::{DVec3, Vec3A};

use super::data::MapData;

pub(crate) const EMIT_SURFACE: i32 = 0;
pub(crate) const EMIT_POINT: i32 = 1;
pub(crate) const EMIT_SPOT: i32 = 2;
pub(crate) const EMIT_SKY: i32 = 3;
pub(crate) const EMIT_SKY_AMBIENT: i32 = 5;
pub(crate) const WORLDLIGHT_SIZE: usize = 88;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    Point,
    Spot { stopdot: f32, stopdot2: f32, exponent: f32 },
    Sun,
    SkyAmbient,
    Surface,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Light {
    pub kind: Kind,
    pub origin: Vec3A,
    pub normal: Vec3A,
    pub intensity: Vec<Vec3A>,
    pub style: u8,
    pub constant: f32,
    pub linear: f32,
    pub quadratic: f32,
    pub radius: f32,
    pub cap: f32,
    pub fade: (f32, f32),
    pub cluster: i32,
    pub reach: f32,
    pub entity_shadows: bool,
}

const REACH_THRESHOLD: f32 = 0.05;
const SURFACE_REACH_THRESHOLD: f32 = 0.005;
const TEST_EPSILON: f64 = 0.1;
const DWL_FLAGS_INAMBIENTCUBE: i32 = 1;
const DWL_FLAGS_CASTENTITYSHADOWS: i32 = 2;

impl Light {
    pub fn falloff(&self, to_light: Vec3A, dist: f32) -> f32 {
        if self.kind == Kind::Surface {
            let dot2 = (-to_light.dot(self.normal)).max(0.0);
            let f = dot2 / (dist * dist);
            return if f.is_finite() { f } else { 0.0 };
        }
        let (start, end) = self.fade;
        let hard = end > start;
        if hard && dist > end {
            return 0.0;
        }
        let d = dist.max(1.0);
        let e = d.min(self.cap);
        let denom = self.constant + e * (self.linear + e * self.quadratic);
        let mut f = if denom > 0.0 { 1.0 / denom } else { 0.0 };
        if let Kind::Spot { stopdot, stopdot2, exponent } = self.kind {
            let dot2 = -to_light.dot(self.normal);
            if dot2 <= stopdot2 {
                return 0.0;
            }
            f *= dot2;
            if dot2 <= stopdot && stopdot > stopdot2 {
                let mut mult = ((dot2 - stopdot2) / (stopdot - stopdot2)).clamp(0.0, 1.0);
                if exponent != 0.0 && exponent != 1.0 {
                    mult = mult.powf(exponent);
                }
                f *= mult;
            }
        }
        if hard {
            let t = 1.0 - ((d - start) / (end - start)).clamp(0.0, 1.0);
            f *= t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        }
        if f.is_finite() { f.max(0.0) } else { 0.0 }
    }

    pub fn surface(origin: DVec3, normal: DVec3, intensity: Vec<Vec3A>, map: &MapData) -> Light {
        let peak = intensity.iter().map(|i| i.max_element()).fold(0.0f32, f32::max);
        let reach = if peak > 0.0 { (peak / SURFACE_REACH_THRESHOLD).sqrt().min(1.0e6) } else { 0.0 };
        Light {
            kind: Kind::Surface,
            origin: origin.as_vec3a(),
            normal: normal.as_vec3a(),
            intensity,
            style: 0,
            constant: 0.0,
            linear: 0.0,
            quadratic: 0.0,
            radius: 0.0,
            cap: 1.0e22,
            fade: (0.0, -1.0),
            cluster: cluster_of(map, origin),
            reach,
            entity_shadows: false,
        }
    }

    pub fn in_pass(&self, pass: usize) -> bool {
        self.kind != Kind::Surface || self.intensity.get(pass).is_some_and(|v| *v != Vec3A::ZERO)
    }

    pub fn in_ambient_cube(&self, pass: usize) -> bool {
        if self.kind != Kind::Surface || self.style != 0 || !self.in_pass(pass) {
            return false;
        }
        let i = self.intensity.get(pass).copied().unwrap_or(Vec3A::ZERO) / 255.0;
        i.max_element() * (1.0 / (512.0 * 512.0)) < 0.005
    }

    fn compute_reach(&mut self) {
        let peak = self.intensity.iter().map(|i| i.max_element()).fold(0.0f32, f32::max);
        if peak.is_nan() || peak <= 0.0 {
            self.reach = 0.0;
            return;
        }
        let target = peak / REACH_THRESHOLD;
        let (q, l, c) = (self.quadratic, self.linear, self.constant - target);
        let mut reach = if q > 1e-12 {
            let disc = l * l - 4.0 * q * c;
            if disc > 0.0 { (-l + disc.sqrt()) / (2.0 * q) } else { 0.0 }
        } else if l > 1e-12 {
            -c / l
        } else {
            f32::INFINITY
        };
        if reach.is_nan() || reach < 1.0 {
            reach = 1.0;
        }
        if self.cap < reach {
            reach = f32::INFINITY;
        }
        let (start, end) = self.fade;
        if end > start {
            reach = reach.min(end);
        }
        self.reach = reach.min(1.0e6);
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Lights {
    pub list: Vec<Light>,
    pub sun_extent: f32,
}

impl Lights {
    #[cfg(test)]
    pub fn sun(&self) -> Option<&Light> {
        self.list.iter().find(|l| l.kind == Kind::Sun)
    }

    #[cfg(test)]
    pub fn ambient(&self) -> Option<&Light> {
        self.list.iter().find(|l| l.kind == Kind::SkyAmbient)
    }
}

fn key_f(e: &hp_bsp::Entity, key: &str) -> f32 {
    let v = e.get(key).and_then(|s| s.split_ascii_whitespace().next()).and_then(|s| s.parse::<f32>().ok());
    v.filter(|v| v.is_finite()).unwrap_or(0.0)
}

fn key_vec(e: &hp_bsp::Entity, key: &str) -> DVec3 {
    let mut it =
        e.get(key).unwrap_or("").split_ascii_whitespace().map(|s| s.parse::<f64>().ok().filter(|v| v.is_finite()));
    let mut c = || it.next().flatten().unwrap_or(0.0);
    DVec3::new(c(), c(), c())
}

pub(crate) fn cluster_of(map: &MapData, p: DVec3) -> i32 {
    fn walk(map: &MapData, n: i32, p: DVec3, depth: usize) -> i32 {
        if n < 0 {
            return map.leafs.get((-1 - n) as usize).map_or(-1, |l| l.cluster);
        }
        if depth > map.nodes.len() {
            return -1;
        }
        let Some(node) = map.nodes.get(n as usize) else { return -1 };
        let Some(plane) = map.planes.get(node.plane) else { return -1 };
        let d = p.dot(plane.normal) - plane.dist;
        if d > TEST_EPSILON {
            walk(map, node.children[0], p, depth + 1)
        } else if d < -TEST_EPSILON {
            walk(map, node.children[1], p, depth + 1)
        } else {
            let c = walk(map, node.children[0], p, depth + 1);
            if c != -1 { c } else { walk(map, node.children[1], p, depth + 1) }
        }
    }
    let Some(head) = map.models.first().map(|m| m.head) else { return -1 };
    walk(map, head, p, 0)
}

pub(crate) fn light_for_string(s: &str, hdr: bool) -> Option<Vec3A> {
    let v: Vec<f64> = s.split_ascii_whitespace().take(8).map_while(|p| p.parse::<f64>().ok()).collect();
    let (rgb, scaler, n) = match v.len() {
        8 if hdr => ([v[4], v[5], v[6]], v[7], 4),
        8 => ([v[0], v[1], v[2]], v[3], 4),
        4..=7 => ([v[0], v[1], v[2]], v[3], 4),
        3 => ([v[0], v[1], v[2]], 0.0, 3),
        1 | 2 => ([v[0]; 3], 0.0, 1),
        _ => return None,
    };
    if rgb.iter().any(|c| *c < 0.0 || !c.is_finite()) || scaler < 0.0 || !scaler.is_finite() {
        return None;
    }
    let lin = |c: f64| (c / 255.0).powf(2.2) * 255.0;
    let mut out = [lin(rgb[0]), lin(rgb[1]), lin(rgb[2])];
    if n == 1 {
        out = [out[0]; 3];
    }
    if n == 4 {
        for c in &mut out {
            *c *= scaler / 255.0;
        }
    }
    let out = Vec3A::new(out[0] as f32, out[1] as f32, out[2] as f32);
    out.is_finite().then_some(out)
}

pub(crate) fn light_normal(angles: DVec3, angle: f64, pitch: f64) -> DVec3 {
    if angle == -1.0 {
        return DVec3::Z;
    }
    if angle == -2.0 {
        return DVec3::NEG_Z;
    }
    let mut out = {
        let yaw = if angle == 0.0 { angles.y } else { angle };
        let (s, c) = hp_light::lights::sin_cos_degrees(yaw);
        DVec3::new(c, s, 0.0)
    };
    let pitch = if pitch == 0.0 { angles.x } else { pitch };
    let (s, c) = hp_light::lights::sin_cos_degrees(pitch);
    out.z = s;
    out.x *= c;
    out.y *= c;
    out
}

fn solve_inverse_quadratic(x: [f32; 3], y: [f32; 3]) -> Option<(f32, f32, f32)> {
    let ([x1, x2, x3], [y1, y2, y3]) = (x, y);
    let det = (x1 - x2) * (x1 - x3) * (x2 - x3);
    if det == 0.0 {
        return None;
    }
    let a = (x3 * (-y1 + y2) + x2 * (y1 - y3) + x1 * (-y2 + y3)) / det;
    let b = (x3 * x3 * (y1 - y2) + x1 * x1 * (y2 - y3) + x2 * x2 * (-y1 + y3)) / det;
    let c = (x1 * x3 * (-x1 + x3) * y2 + x2 * x2 * (x3 * y1 - x1 * y3) + x2 * (-(x3 * x3 * y1) + x1 * x1 * y3)) / det;
    Some((a, b, c))
}

fn solve_monotonic(x: [f32; 3], y: [f32; 3], abc: &mut (f32, f32, f32)) {
    let mut f = 0.0f32;
    while f <= 1.0 {
        let lerp = y[0] + (y[2] - y[0]) * (x[1] - x[0]) / (x[2] - x[0]);
        let ty = (1.0 - f) * y[1] + f * lerp;
        let Some(r) = solve_inverse_quadratic(x, [y[0], ty, y[2]]) else { return };
        *abc = r;
        let derivative = 2.0 * r.0 + r.1;
        let inc = y[0] < y[1] && y[1] < y[2];
        let dec = y[0] > y[1] && y[1] > y[2];
        if (inc && derivative >= 0.0) || (dec && derivative <= 0.0) || (!inc && !dec) {
            return;
        }
        f += 0.05;
    }
}

fn set_falloff(l: &mut Light, e: &hp_bsp::Entity) {
    let d50 = key_f(e, "_fifty_percent_distance");
    l.fade = (0.0, -1.0);
    l.cap = 1.0e22;
    if d50 != 0.0 {
        let mut d0 = key_f(e, "_zero_percent_distance");
        if d0 < d50 {
            d0 = 2.0 * d50;
        }
        let mut abc = (0.0, 1.0, 0.0);
        let mut pts = [(0.0f32, 1.0f32), (d50, 2.0), (d0, 256.0)];
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        solve_monotonic([pts[0].0, pts[1].0, pts[2].0], [pts[0].1, pts[1].1, pts[2].1], &mut abc);
        let (mut a, mut b, mut c) = abc;
        let v50 = c + d50 * (b + d50 * a);
        let scale = 2.0 / v50;
        if scale.is_finite() {
            a *= scale;
            b *= scale;
            c *= scale;
        }
        l.quadratic = a;
        l.linear = b;
        l.constant = c;
        if key_f(e, "_hardfalloff") as i32 != 0 {
            l.fade = (0.75 * d0 + 0.25 * d50, d0);
        } else if a.abs() > 0.0 {
            let max = b / (-2.0 * a);
            if max > 0.0 {
                l.cap = max;
                l.fade = (max, 10.0 * max);
            }
        }
    } else {
        let at_least = |v: f32| if v < 0.001 { 0.0 } else { v };
        l.constant = at_least(key_f(e, "_constant_attn"));
        l.linear = at_least(key_f(e, "_linear_attn"));
        l.quadratic = at_least(key_f(e, "_quadratic_attn"));
        if l.constant == 0.0 && l.linear == 0.0 && l.quadratic == 0.0 {
            l.constant = 1.0;
        }
        let ratio = l.constant + 100.0 * l.linear + 100.0 * 100.0 * l.quadratic;
        if ratio > 0.0 {
            for i in &mut l.intensity {
                *i *= ratio;
            }
        }
    }
    if !(l.constant.is_finite() && l.linear.is_finite() && l.quadratic.is_finite()) {
        (l.constant, l.linear, l.quadratic) = (1.0, 0.0, 0.0);
    }
    let r = key_f(e, "_distance");
    l.radius = if r > 0.0 { r.min(1.0e6) } else { 0.0 };
    let (start, end) = l.fade;
    if end > start && end.is_finite() && (l.radius == 0.0 || end < l.radius) {
        l.radius = end.min(1.0e6);
    }
}

fn pass_value(e: &hp_bsp::Entity, ldr_key: &str, hdr_key: &str, hdr: bool) -> Option<Vec3A> {
    if hdr && let Some(v) = e.get(hdr_key).and_then(|s| light_for_string(s, true)) {
        return Some(v);
    }
    e.get(ldr_key).and_then(|s| light_for_string(s, hdr))
}

fn generic(e: &hp_bsp::Entity, entities: &[hp_bsp::Entity], passes: &[bool], kind: Kind) -> Light {
    let origin = key_vec(e, "origin");
    let intensity = passes
        .iter()
        .map(|&hdr| {
            let mut v = pass_value(e, "_light", "_lightHDR", hdr).unwrap_or(Vec3A::ZERO);
            if hdr {
                let s = e.get("_lightscaleHDR").map_or(1.0, |_| key_f(e, "_lightscaleHDR"));
                v *= s;
            }
            v
        })
        .collect();
    let mut normal = light_normal(key_vec(e, "angles"), f64::from(key_f(e, "angle")), f64::from(key_f(e, "pitch")));
    if let Some(t) = e.get("target").filter(|t| !t.is_empty())
        && let Some(dest) = entities.iter().find(|o| o.get("targetname") == Some(t))
    {
        normal = (key_vec(dest, "origin") - origin).normalize_or_zero();
    }
    Light {
        kind,
        origin: origin.as_vec3a(),
        normal: normal.as_vec3a(),
        intensity,
        style: (key_f(e, "style") as i32).clamp(0, 255) as u8,
        constant: 0.0,
        linear: 0.0,
        quadratic: 0.0,
        radius: 0.0,
        cap: 1.0e22,
        fade: (0.0, -1.0),
        cluster: -1,
        reach: 0.0,
        entity_shadows: key_f(e, "_castentityshadow") as i32 != 0,
    }
}

pub(crate) fn parse(entities: &[hp_bsp::Entity], passes: &[bool], map: &MapData) -> Lights {
    let mut list: Vec<Light> = Vec::new();
    let mut out = Lights::default();
    let mut sky_found = false;
    for e in entities {
        let class = e.classname();
        if !class.starts_with("light") || class == "light_dynamic" {
            continue;
        }
        if class == "light_spot" {
            let mut l = generic(e, entities, passes, Kind::Point);
            let mut inner = key_f(e, "_inner_cone");
            if inner == 0.0 {
                inner = 10.0;
            }
            let mut outer = key_f(e, "_cone");
            if outer == 0.0 {
                outer = inner;
            }
            if outer < inner {
                outer = inner;
            }
            if !(inner == 180.0 && outer == 180.0) {
                let (inner, outer) = (inner.min(90.0), outer.min(90.0));
                l.kind = Kind::Spot {
                    stopdot: inner.to_radians().cos(),
                    stopdot2: outer.to_radians().cos(),
                    exponent: key_f(e, "_exponent"),
                };
            }
            set_falloff(&mut l, e);
            list.push(l);
        } else if class == "light_environment" {
            if let Some(a) = e.get("SunSpreadAngle") {
                let deg = a.trim().parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(0.0);
                out.sun_extent = deg.to_radians().sin().max(0.0);
            }
            if sky_found {
                continue;
            }
            sky_found = true;
            let sun = generic(e, entities, passes, Kind::Sun);
            let intensity = passes
                .iter()
                .zip(&sun.intensity)
                .map(|(&hdr, sun)| {
                    let mut v = pass_value(e, "_ambient", "_ambientHDR", hdr).unwrap_or(*sun * 0.5);
                    if hdr {
                        let s = e.get("_AmbientScaleHDR").map_or(1.0, |_| key_f(e, "_AmbientScaleHDR"));
                        v *= s;
                    }
                    v
                })
                .collect();
            let ambient = Light {
                kind: Kind::SkyAmbient,
                normal: Vec3A::ZERO,
                intensity,
                style: 0,
                entity_shadows: false,
                ..sun.clone()
            };
            list.push(sun);
            list.push(ambient);
        } else if class == "light" {
            let mut l = generic(e, entities, passes, Kind::Point);
            set_falloff(&mut l, e);
            list.push(l);
        }
    }
    list.reverse();
    for l in &mut list {
        l.cluster = cluster_of(map, l.origin.as_dvec3());
        if matches!(l.kind, Kind::Point | Kind::Spot { .. }) {
            l.compute_reach();
        }
    }
    out.list = list;
    out
}

pub(crate) fn assign_styles(entities: &mut [hp_bsp::Entity], max_switched: usize) -> bool {
    let is_light = |e: &hp_bsp::Entity| {
        let c = e.classname();
        c.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("light")) && !c.eq_ignore_ascii_case("light_dynamic")
    };
    let has_styles = entities.iter().any(|e| is_light(e) && key_f(e, "style") as i32 >= 32);
    if has_styles {
        return false;
    }
    let mut names: Vec<String> = Vec::new();
    let mut changed = false;
    for e in entities.iter_mut().skip(1) {
        if !is_light(e) {
            continue;
        }
        let Some(t) = e.get("targetname").filter(|t| !t.is_empty()).map(str::to_owned) else { continue };
        let j = match names.iter().position(|n| *n == t) {
            Some(j) => j,
            None => {
                if names.len() >= max_switched {
                    continue;
                }
                names.push(t);
                names.len() - 1
            }
        };
        if let Some(old) = e.get("style").map(str::to_owned)
            && old.trim().parse::<i32>().unwrap_or(0) != 0
        {
            e.set("defaultstyle", old);
        }
        e.set("style", (32 + j).to_string());
        changed = true;
    }
    changed
}

pub(crate) fn worldlights(lights: &Lights, pass: usize) -> Vec<u8> {
    let mut b = Vec::with_capacity(lights.list.len() * WORLDLIGHT_SIZE);
    let f = |b: &mut Vec<u8>, v: f32| b.extend_from_slice(&(if v.is_finite() { v } else { 0.0 }).to_le_bytes());
    let i = |b: &mut Vec<u8>, v: i32| b.extend_from_slice(&v.to_le_bytes());
    for l in lights.list.iter().filter(|l| l.in_pass(pass)) {
        let intensity = l.intensity.get(pass).copied().unwrap_or(Vec3A::ZERO) / 255.0;
        for v in [l.origin, intensity, l.normal] {
            f(&mut b, v.x);
            f(&mut b, v.y);
            f(&mut b, v.z);
        }
        i(&mut b, l.cluster);
        let (ty, stop, stop2, exponent) = match l.kind {
            Kind::Point => (EMIT_POINT, 0.0, 0.0, 0.0),
            Kind::Spot { stopdot, stopdot2, exponent } => (EMIT_SPOT, stopdot, stopdot2, exponent),
            Kind::Sun => (EMIT_SKY, 0.0, 0.0, 0.0),
            Kind::SkyAmbient => (EMIT_SKY_AMBIENT, 0.0, 0.0, 0.0),
            Kind::Surface => (EMIT_SURFACE, 0.0, 0.0, 0.0),
        };
        i(&mut b, ty);
        i(&mut b, i32::from(l.style));
        for v in [stop, stop2, exponent, l.radius, l.constant, l.linear, l.quadratic] {
            f(&mut b, v);
        }
        let flags = if l.in_ambient_cube(pass) { DWL_FLAGS_INAMBIENTCUBE } else { 0 }
            | if l.entity_shadows { DWL_FLAGS_CASTENTITYSHADOWS } else { 0 };
        i(&mut b, flags);
        i(&mut b, 0);
        i(&mut b, 0);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(pairs: &[(&str, &str)]) -> hp_bsp::Entity {
        hp_bsp::Entity { pairs: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect() }
    }

    #[test]
    fn strings() {
        let v = light_for_string("255 255 255 400", false).unwrap();
        assert!((v.x - 400.0).abs() < 1e-3);
        let v = light_for_string("255 128 0", false).unwrap();
        assert!((v.x - 255.0).abs() < 1e-3 && v.z == 0.0);
        assert!(
            (light_for_string("128", false).unwrap().y - light_for_string("128 128 128", false).unwrap().y).abs()
                < 1e-4
        );
        let two = "255 255 255 100 255 0 0 50";
        assert!((light_for_string(two, false).unwrap().y - 100.0).abs() < 1e-3);
        assert_eq!(light_for_string(two, true).unwrap().y, 0.0);
        assert!(light_for_string("-1 -1 -1 1", true).is_none());
        assert!(light_for_string("", false).is_none());
        assert!(light_for_string("a b c", false).is_none());
    }

    #[test]
    fn spot_and_point() {
        let map = MapData::default();
        let es = vec![
            ent(&[
                ("classname", "light"),
                ("origin", "0 0 100"),
                ("_light", "255 255 255 200"),
                ("_quadratic_attn", "1"),
            ]),
            ent(&[
                ("classname", "light_spot"),
                ("origin", "0 0 100"),
                ("pitch", "-90"),
                ("_light", "255 255 255 200"),
                ("_inner_cone", "20"),
                ("_cone", "40"),
                ("_exponent", "2"),
                ("_quadratic_attn", "1"),
            ]),
            ent(&[("classname", "light_dynamic"), ("_light", "255 255 255 200")]),
        ];
        let lights = parse(&es, &[false, true], &map);
        assert_eq!(lights.list.len(), 2);
        let spot = &lights.list[0];
        let point = &lights.list[1];
        assert_eq!(point.kind, Kind::Point);
        let f = point.falloff(Vec3A::Z, 100.0);
        assert!((point.intensity[0].x * f - 200.0).abs() < 0.01);
        assert!((point.intensity[1].x - point.intensity[0].x).abs() < 1e-3);
        assert!((spot.normal.z + 1.0).abs() < 1e-6);
        let center = spot.falloff(Vec3A::Z, 100.0);
        let fringe = spot.falloff(Vec3A::new(30f32.to_radians().sin(), 0.0, 30f32.to_radians().cos()), 100.0);
        let outside = spot.falloff(Vec3A::new(50f32.to_radians().sin(), 0.0, 50f32.to_radians().cos()), 100.0);
        assert!(center > fringe && fringe > 0.0 && outside == 0.0);
        assert!(point.reach > 1000.0 && point.reach.is_finite());
        let wl = worldlights(&lights, 0);
        assert_eq!(wl.len(), 2 * WORLDLIGHT_SIZE);
    }

    #[test]
    fn only_the_exact_light_classes_light_the_map() {
        let map = MapData::default();
        let named = |class: &str| ent(&[("classname", class), ("_light", "255 255 255 200"), ("pitch", "-90")]);
        for class in [
            "light_directional",
            "light_glspot",
            "light_foo",
            "LIGHT",
            "Light_Spot",
            "LIGHT_ENVIRONMENT",
            "light_dynamic",
        ] {
            let lights = parse(&[named(class)], &[false], &map);
            assert!(lights.list.is_empty(), "{class}");
        }
        for (class, count) in [("light", 1), ("light_spot", 1), ("light_environment", 2)] {
            assert_eq!(parse(&[named(class)], &[false], &map).list.len(), count, "{class}");
        }
    }

    #[test]
    fn falloff_curves() {
        let map = MapData::default();
        let e = ent(&[
            ("classname", "light"),
            ("_light", "255 255 255 255"),
            ("_fifty_percent_distance", "200"),
            ("_zero_percent_distance", "400"),
        ]);
        let l = &parse(std::slice::from_ref(&e), &[false], &map).list[0];
        assert!((l.falloff(Vec3A::Z, 200.0) - 0.5).abs() < 0.01);
        let mut prev = f32::INFINITY;
        for d in (1..3000).step_by(13) {
            let f = l.falloff(Vec3A::Z, d as f32);
            assert!(f <= prev + 1e-6, "{d}");
            prev = f;
        }
        let mut hard = e.clone();
        hard.set("_hardfalloff", "1");
        let h = &parse(std::slice::from_ref(&hard), &[false], &map).list[0];
        assert_eq!(h.falloff(Vec3A::Z, 401.0), 0.0);
        assert!(h.falloff(Vec3A::Z, 300.0) > 0.0);
        assert_eq!(h.radius, 400.0, "the engine lights models up to the zero percent distance only");
        let soft = &parse(std::slice::from_ref(&e), &[false], &map).list[0];
        assert_eq!((soft.fade, soft.radius), ((0.0, -1.0), 0.0), "a rising curve never fades: no radius");
        let mut capped = hard.clone();
        capped.set("_distance", "300");
        assert_eq!(parse(&[capped], &[false], &map).list[0].radius, 300.0, "a shorter _distance wins");
        let mut styled = hard.clone();
        styled.set("targetname", "lamp");
        styled.set("style", "32");
        let st = &parse(&[styled], &[false], &map).list[0];
        assert_eq!((st.style, st.radius, st.fade), (32, 400.0, h.fade), "styled lights fall off like the others");
        assert_eq!(st.falloff(Vec3A::Z, 401.0), 0.0);
        let wl = worldlights(&parse(&[hard], &[false], &map), 0);
        assert_eq!(f32::from_le_bytes([wl[60], wl[61], wl[62], wl[63]]), 400.0);
    }

    #[test]
    fn maximum_distance() {
        let map = MapData::default();
        let point = ent(&[
            ("classname", "light"),
            ("_light", "255 255 255 255"),
            ("_quadratic_attn", "1"),
            ("_distance", "256"),
        ]);
        let l = &parse(std::slice::from_ref(&point), &[false], &map).list[0];
        assert_eq!(l.radius, 256.0);
        assert_eq!(l.fade, (0.0, -1.0), "VRAD bakes past _distance: only the engine reads it");
        let free = &parse(
            &[ent(&[("classname", "light"), ("_light", "255 255 255 255"), ("_quadratic_attn", "1")])],
            &[false],
            &map,
        )
        .list[0];
        assert_eq!(l.falloff(Vec3A::Z, 400.0), free.falloff(Vec3A::Z, 400.0));
        assert!(l.falloff(Vec3A::Z, 400.0) > 0.0);
        assert_eq!(l.reach, free.reach);
        let curve = ent(&[
            ("classname", "light_spot"),
            ("_light", "255 255 255 255"),
            ("_fifty_percent_distance", "200"),
            ("_zero_percent_distance", "400"),
            ("_distance", "300"),
            ("pitch", "-90"),
        ]);
        let s = &parse(std::slice::from_ref(&curve), &[false], &map).list[0];
        assert_eq!(s.radius, 300.0);
        assert!(s.falloff(Vec3A::Z, 350.0) > 0.0);
        assert!(s.falloff(Vec3A::Z, 200.0) > s.falloff(Vec3A::Z, 350.0));
        let unlimited = ent(&[("classname", "light"), ("_light", "255 255 255 255"), ("_distance", "-5")]);
        let u = &parse(std::slice::from_ref(&unlimited), &[false], &map).list[0];
        assert_eq!((u.radius, u.fade), (0.0, (0.0, -1.0)));
        let wl = worldlights(&parse(std::slice::from_ref(&point), &[false], &map), 0);
        assert_eq!(f32::from_le_bytes([wl[60], wl[61], wl[62], wl[63]]), 256.0);
    }

    #[test]
    fn environment_and_styles() {
        let map = MapData::default();
        let mut es = vec![
            ent(&[("classname", "worldspawn")]),
            ent(&[
                ("classname", "light_environment"),
                ("targetname", "sun"),
                ("_light", "255 255 255 300"),
                ("_lightHDR", "-1 -1 -1 1"),
                ("_lightscaleHDR", "0.5"),
                ("angles", "0 90 0"),
                ("pitch", "-60"),
                ("SunSpreadAngle", "5"),
            ]),
            ent(&[("classname", "light"), ("targetname", "lamp"), ("_light", "255 255 255 10")]),
            ent(&[("classname", "light_spot"), ("targetname", "lamp"), ("_light", "255 255 255 10")]),
        ];
        assert!(assign_styles(&mut es, crate::compile::MAX_SWITCHED_LIGHTS));
        assert_eq!(es[1].get("style"), Some("32"));
        assert_eq!(es[2].get("style"), Some("33"));
        assert_eq!(es[3].get("style"), Some("33"));
        assert!(!assign_styles(&mut es, crate::compile::MAX_SWITCHED_LIGHTS), "already styled");
        let lights = parse(&es, &[false, true], &map);
        let sun = lights.sun().unwrap();
        assert!(sun.normal.z < -0.8);
        assert_eq!(sun.style, 32);
        assert!((sun.intensity[0].x - 300.0).abs() < 1e-3);
        assert!((sun.intensity[1].x - 150.0).abs() < 1e-3);
        let amb = lights.ambient().unwrap();
        assert!((amb.intensity[0].x - 150.0).abs() < 1e-3);
        assert_eq!(amb.style, 0);
        assert!((lights.sun_extent - 5f32.to_radians().sin()).abs() < 1e-6);
    }

    #[test]
    fn normals() {
        let n = light_normal(DVec3::new(-90.0, 0.0, 0.0), 0.0, 0.0);
        assert!((n.z + 1.0).abs() < 1e-9);
        let n = light_normal(DVec3::ZERO, 90.0, 0.0);
        assert!((n - DVec3::Y).length() < 1e-9);
        let n = light_normal(DVec3::ZERO, -2.0, -90.0);
        assert!((n.z + 1.0).abs() < 1e-9);
        assert_eq!(light_normal(DVec3::ZERO, -1.0, 0.0), DVec3::Z);
        assert_eq!(light_normal(DVec3::ZERO, -2.0, 30.0), DVec3::NEG_Z);
        assert_eq!(light_normal(DVec3::new(45.0, 0.0, 0.0), -1.0, -90.0), DVec3::Z);
    }

    #[test]
    fn entity_shadow_flag() {
        let map = MapData::default();
        let es = vec![
            ent(&[("classname", "worldspawn")]),
            ent(&[("classname", "light"), ("_light", "255 255 255 10"), ("_castentityshadow", "1")]),
            ent(&[("classname", "light_spot"), ("_light", "255 255 255 10")]),
        ];
        let lights = parse(&es, &[false], &map);
        let b = worldlights(&lights, 0);
        let flags: Vec<i32> =
            b.chunks_exact(WORLDLIGHT_SIZE).map(|r| i32::from_le_bytes([r[76], r[77], r[78], r[79]])).collect();
        assert_eq!(flags, vec![0, DWL_FLAGS_CASTENTITYSHADOWS]);
    }
}
