use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Point { origin: [f64; 3] },
    Volume { min: [f64; 3], max: [f64; 3] },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub id: u32,
    pub name: String,
    pub kind: Kind,
    pub filename: String,
    pub min_falloff: f64,
    pub max_falloff: f64,
    pub max_weight: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub exclusive: bool,
    pub enabled: bool,
}

pub fn zone_filename(raw: &str) -> String {
    raw.trim().replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
}

fn number(v: Option<String>, default: f64) -> f64 {
    v.and_then(|s| s.trim().parse::<f64>().ok()).filter(|n| n.is_finite()).unwrap_or(default)
}

fn flag(v: Option<String>) -> bool {
    v.is_some_and(|s| {
        let s = s.trim();
        s.parse::<f64>().map_or_else(|_| s.eq_ignore_ascii_case("true"), |n| n != 0.0)
    })
}

impl Zone {
    pub fn point(id: u32, origin: [f64; 3], get: &dyn Fn(&str) -> Option<String>) -> Self {
        Self {
            id,
            name: get("targetname").unwrap_or_default(),
            kind: Kind::Point { origin },
            filename: zone_filename(&get("filename").unwrap_or_default()),
            min_falloff: number(get("minfalloff"), 0.0),
            max_falloff: number(get("maxfalloff"), 200.0),
            max_weight: number(get("maxweight"), 1.0).clamp(0.0, 1.0) as f32,
            fade_in: number(get("fadeinduration"), 0.0).max(0.0) as f32,
            fade_out: number(get("fadeoutduration"), 0.0).max(0.0) as f32,
            exclusive: flag(get("exclusive")),
            enabled: !flag(get("startdisabled")),
        }
    }

    pub fn volume(id: u32, min: [f64; 3], max: [f64; 3], get: &dyn Fn(&str) -> Option<String>) -> Self {
        let fade = number(get("fadeduration"), 0.0).max(0.0) as f32;
        Self {
            id,
            name: get("targetname").unwrap_or_default(),
            kind: Kind::Volume { min, max },
            filename: zone_filename(&get("filename").unwrap_or_default()),
            min_falloff: -1.0,
            max_falloff: -1.0,
            max_weight: number(get("maxweight"), 1.0).clamp(0.0, 1.0) as f32,
            fade_in: fade,
            fade_out: fade,
            exclusive: false,
            enabled: !flag(get("startdisabled")),
        }
    }

    pub fn distance(&self, eye: [f64; 3]) -> f64 {
        match &self.kind {
            Kind::Point { origin } => {
                let d = [eye[0] - origin[0], eye[1] - origin[1], eye[2] - origin[2]];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            }
            Kind::Volume { min, max } => {
                let mut sum = 0.0;
                for k in 0..3 {
                    let d = (min[k] - eye[k]).max(eye[k] - max[k]).max(0.0);
                    sum += d * d;
                }
                sum.sqrt()
            }
        }
    }

    pub fn inside(&self, eye: [f64; 3]) -> bool {
        match &self.kind {
            Kind::Point { .. } => true,
            Kind::Volume { .. } => self.distance(eye) <= 0.0,
        }
    }

    pub fn active(&self, eye: [f64; 3]) -> bool {
        self.enabled && self.inside(eye)
    }

    pub fn falloff(&self, eye: [f64; 3]) -> f32 {
        if !matches!(self.kind, Kind::Point { .. }) {
            return 1.0;
        }
        let (lo, hi) = (self.min_falloff, self.max_falloff);
        if lo == -1.0 || hi == -1.0 || lo == hi {
            return 1.0;
        }
        let t = ((self.distance(eye) - lo) / (hi - lo)).clamp(0.0, 1.0);
        (1.0 - t) as f32
    }
}

#[derive(Clone, Debug, Default)]
pub struct Fader {
    levels: HashMap<u32, f32>,
}

impl Fader {
    pub fn step(&mut self, zone: &Zone, eye: [f64; 3], dt: f32) -> f32 {
        let on = zone.active(eye);
        let level = self.levels.entry(zone.id).or_insert(if on { 1.0 } else { 0.0 });
        let (target, span) = if on { (1.0, zone.fade_in) } else { (0.0, zone.fade_out) };
        if span <= 0.0 {
            *level = target;
        } else {
            let step = dt.max(0.0) / span;
            *level = if on { (*level + step).min(1.0) } else { (*level - step).max(0.0) };
        }
        *level
    }

    pub fn settled(&self, zones: &[Zone], eye: [f64; 3]) -> bool {
        zones.iter().all(|z| {
            let target = if z.active(eye) { 1.0 } else { 0.0 };
            self.levels.get(&z.id).is_none_or(|l| (*l - target).abs() < 1e-4)
        })
    }

    pub fn retain(&mut self, zones: &[Zone]) {
        self.levels.retain(|id, _| zones.iter().any(|z| z.id == *id));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Contribution {
    pub id: u32,
    pub filename: String,
    pub weight: f32,
    pub exclusive: bool,
}

pub fn resolve(list: &[Contribution]) -> Vec<Contribution> {
    let live: Vec<Contribution> = list.iter().filter(|c| c.weight > 0.0 && !c.filename.is_empty()).cloned().collect();
    if live.iter().any(|c| c.exclusive) { live.into_iter().filter(|c| c.exclusive).collect() } else { live }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ZoneReport {
    pub id: u32,
    pub name: String,
    pub class: String,
    pub filename: String,
    pub distance: f64,
    pub falloff: f32,
    pub fade: f32,
    pub weight: f32,
    pub exclusive: bool,
    pub enabled: bool,
    pub inside: bool,
    pub applied: bool,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| (*v).to_owned())
    }

    #[test]
    fn falloff_follows_the_engine() {
        let z = Zone::point(
            1,
            [0.0; 3],
            &kv(&[("filename", "Materials\\Correction\\A.raw"), ("minfalloff", "100"), ("maxfalloff", "300")]),
        );
        assert_eq!(z.filename, "materials/correction/a.raw");
        assert_eq!(z.falloff([50.0, 0.0, 0.0]), 1.0);
        assert!((z.falloff([200.0, 0.0, 0.0]) - 0.5).abs() < 1e-6);
        assert_eq!(z.falloff([400.0, 0.0, 0.0]), 0.0);
        let global = Zone::point(2, [0.0; 3], &kv(&[("minfalloff", "-1"), ("maxfalloff", "-1")]));
        assert_eq!(global.falloff([9999.0, 0.0, 0.0]), 1.0);
    }

    #[test]
    fn disabled_and_fades() {
        let mut z =
            Zone::point(1, [0.0; 3], &kv(&[("startdisabled", "1"), ("fadeinduration", "2"), ("fadeoutduration", "1")]));
        assert!(!z.enabled);
        let mut f = Fader::default();
        assert_eq!(f.step(&z, [0.0; 3], 0.1), 0.0);
        z.enabled = true;
        let a = f.step(&z, [0.0; 3], 1.0);
        assert!((a - 0.5).abs() < 1e-6);
        assert_eq!(f.step(&z, [0.0; 3], 5.0), 1.0);
        z.enabled = false;
        assert!((f.step(&z, [0.0; 3], 0.25) - 0.75).abs() < 1e-6);
        assert!(!f.settled(&[z], [0.0; 3]));
    }

    #[test]
    fn volumes_and_exclusive() {
        let v = Zone::volume(3, [0.0; 3], [100.0; 3], &kv(&[("fadeduration", "0")]));
        assert!(v.inside([50.0; 3]));
        assert!(!v.inside([150.0, 50.0, 50.0]));
        assert!((v.distance([150.0, 50.0, 50.0]) - 50.0).abs() < 1e-9);
        let list = [
            Contribution { id: 1, filename: "a".into(), weight: 0.5, exclusive: false },
            Contribution { id: 2, filename: "b".into(), weight: 0.2, exclusive: true },
            Contribution { id: 3, filename: "c".into(), weight: 0.0, exclusive: true },
            Contribution { id: 4, filename: String::new(), weight: 1.0, exclusive: false },
        ];
        let r = resolve(&list);
        assert_eq!(r.iter().map(|c| c.id).collect::<Vec<_>>(), [2]);
        assert_eq!(resolve(&list[..1]).len(), 1);
    }
}
