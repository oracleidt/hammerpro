#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitterKind {
    Main,
    Proxy,
    Triggerable,
}

impl EmitterKind {
    pub fn from_class(class: &str) -> Option<Self> {
        let c = class.trim();
        if c.eq_ignore_ascii_case("env_soundscape") {
            Some(EmitterKind::Main)
        } else if c.eq_ignore_ascii_case("env_soundscape_proxy") {
            Some(EmitterKind::Proxy)
        } else if c.eq_ignore_ascii_case("env_soundscape_triggerable") {
            Some(EmitterKind::Triggerable)
        } else {
            None
        }
    }

    pub fn class(self) -> &'static str {
        match self {
            EmitterKind::Main => "env_soundscape",
            EmitterKind::Proxy => "env_soundscape_proxy",
            EmitterKind::Triggerable => "env_soundscape_triggerable",
        }
    }
}

pub const INFINITE_RADIUS: f64 = -1.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Emitter {
    pub id: u32,
    pub kind: EmitterKind,
    pub origin: [f64; 3],
    pub radius: f64,
    pub soundscape: String,
    pub enabled: bool,
    pub targetname: String,
    pub positions: Vec<(u8, String, Option<[f64; 3]>)>,
    pub main: Option<u32>,
}

impl Emitter {
    pub fn infinite(&self) -> bool {
        self.radius == INFINITE_RADIUS
    }

    pub fn distance(&self, p: [f64; 3]) -> f64 {
        let d = [p[0] - self.origin[0], p[1] - self.origin[1], p[2] - self.origin[2]];
        ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) as f32).sqrt() as f64
    }

    pub fn in_range(&self, p: [f64; 3]) -> bool {
        self.infinite() || self.radius > self.distance(p)
    }

    pub fn position(&self, index: u8) -> Option<[f64; 3]> {
        self.positions.iter().find(|(i, _, _)| *i == index).and_then(|(_, _, at)| *at)
    }
}

pub fn parse_radius(text: Option<&str>) -> f64 {
    text.and_then(|t| t.trim().parse::<f32>().ok()).filter(|r| r.is_finite()).map_or(0.0, f64::from)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub index: usize,
    pub distance: f64,
    pub in_range: bool,
    pub line_of_sight: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub active: Option<usize>,
    pub kept_previous: bool,
    pub candidates: Vec<Candidate>,
}

pub fn decide(
    emitters: &[Emitter],
    ear: [f64; 3],
    previous: Option<usize>,
    mut sight: impl FnMut(&Emitter, [f64; 3]) -> bool,
) -> Decision {
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut probe = |index: usize, e: &Emitter, candidates: &mut Vec<Candidate>| -> (f64, bool) {
        let distance = e.distance(ear);
        let in_range = e.in_range(ear);
        let line_of_sight = in_range && sight(e, ear);
        candidates.push(Candidate { index, distance, in_range, line_of_sight });
        (distance, line_of_sight)
    };
    let mut current = previous.filter(|i| *i < emitters.len());
    let mut in_range = false;
    let mut current_distance = 0.0;
    let mut kept = false;
    if let Some(c) = current {
        let e = &emitters[c];
        if e.enabled {
            let (d, seen) = probe(c, e, &mut candidates);
            current_distance = d;
            in_range = seen;
            kept = !seen;
        } else {
            current = None;
        }
    }
    for (i, e) in emitters.iter().enumerate() {
        if Some(i) == previous || !e.enabled {
            continue;
        }
        let (d, seen) = probe(i, e, &mut candidates);
        if seen && (!in_range || d < current_distance) {
            current = Some(i);
            in_range = true;
            current_distance = d;
            kept = false;
        }
    }
    candidates.sort_by_key(|c| c.index);
    Decision { active: current, kept_previous: kept && current.is_some(), candidates }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriggerList {
    touching: Vec<u32>,
    list: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TriggerLink {
    pub trigger: u32,
    pub triggerable: Option<u32>,
}

impl TriggerList {
    pub fn touching(&self) -> &[u32] {
        &self.touching
    }

    pub fn soundscapes(&self) -> &[u32] {
        &self.list
    }

    pub fn reset(&mut self) {
        self.touching.clear();
        self.list.clear();
    }

    pub fn step(&mut self, links: &[TriggerLink], now: &[u32], current: Option<u32>) -> Option<u32> {
        let mut current = current;
        let ended: Vec<u32> = self.touching.iter().copied().filter(|t| !now.contains(t)).collect();
        let started: Vec<u32> = now.iter().copied().filter(|t| !self.touching.contains(t)).collect();
        let link = |t: u32| links.iter().find(|l| l.trigger == t).and_then(|l| l.triggerable);
        for t in ended {
            if let Some(ss) = link(t) {
                self.list.retain(|x| *x != ss);
                current = self.list.first().copied();
            }
        }
        for t in started {
            if let Some(ss) = link(t) {
                self.list.retain(|x| *x != ss);
                self.list.insert(0, ss);
                current = Some(ss);
            }
        }
        self.touching = now.to_vec();
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn em(id: u32, x: f64, radius: f64, name: &str) -> Emitter {
        Emitter {
            id,
            kind: EmitterKind::Main,
            origin: [x, 0.0, 0.0],
            radius,
            soundscape: name.to_owned(),
            enabled: true,
            targetname: String::new(),
            positions: Vec::new(),
            main: None,
        }
    }

    #[test]
    fn classes_and_radius_keys() {
        assert_eq!(EmitterKind::from_class("ENV_SOUNDSCAPE"), Some(EmitterKind::Main));
        assert_eq!(EmitterKind::from_class("env_soundscape_proxy"), Some(EmitterKind::Proxy));
        assert_eq!(EmitterKind::from_class("env_soundscape_triggerable"), Some(EmitterKind::Triggerable));
        assert_eq!(EmitterKind::from_class("ambient_generic"), None);
        assert_eq!(parse_radius(None), 0.0, "an entity without the key has radius 0 in the engine");
        assert_eq!(parse_radius(Some("-5")), -5.0);
        assert_eq!(parse_radius(Some("512")), 512.0);
        assert_eq!(parse_radius(Some("x")), 0.0);
        let mut e = em(1, 0.0, -5.0, "a");
        assert!(!e.in_range([0.0; 3]), "only -1 is infinite");
        e.radius = 100.0;
        assert!(!e.in_range([100.0, 0.0, 0.0]), "the radius must be greater than the distance");
        assert!(e.in_range([99.9, 0.0, 0.0]));
        e.radius = 0.0;
        assert!(!e.in_range([0.0; 3]));
    }

    #[test]
    fn the_nearest_soundscape_in_range_with_sight_wins() {
        let list = vec![em(1, 0.0, 512.0, "a"), em(2, 300.0, 512.0, "b"), em(3, 2000.0, -1.0, "c")];
        let d = decide(&list, [250.0, 0.0, 0.0], None, |_, _| true);
        assert_eq!(d.active, Some(1), "b is nearer than a");
        assert!(!d.kept_previous);
        let d = decide(&list, [250.0, 0.0, 0.0], None, |e, _| e.id != 2);
        assert_eq!(d.active, Some(0), "b is behind a wall, a is next");
        let d = decide(&list, [5000.0, 0.0, 0.0], None, |_, _| true);
        assert_eq!(d.active, Some(2), "radius -1 reaches everywhere");
        let c = d.candidates.iter().find(|c| c.index == 0).unwrap();
        assert!(!c.in_range && !c.line_of_sight);
    }

    #[test]
    fn out_of_every_range_the_previous_soundscape_stays() {
        let mut list = vec![em(1, 0.0, 256.0, "a"), em(2, 1000.0, 256.0, "b")];
        let d = decide(&list, [500.0, 0.0, 0.0], Some(1), |_, _| true);
        assert_eq!(d.active, Some(1));
        assert!(d.kept_previous);
        let d = decide(&list, [500.0, 0.0, 0.0], None, |_, _| true);
        assert_eq!(d.active, None, "nothing heard yet");
        list[1].enabled = false;
        let d = decide(&list, [1000.0, 0.0, 0.0], Some(1), |_, _| true);
        assert_eq!(d.active, None, "a disabled soundscape stops");
        list[0].soundscape.clear();
        let d = decide(&list, [0.0, 0.0, 0.0], None, |_, _| true);
        assert_eq!(d.active, Some(0), "an entity without a valid name still becomes current, like the server");
    }

    #[test]
    fn the_current_soundscape_is_tested_first_and_kept_unless_beaten() {
        let list = vec![em(1, 0.0, 1000.0, "a"), em(2, 100.0, 1000.0, "b")];
        let d = decide(&list, [400.0, 0.0, 0.0], Some(0), |_, _| true);
        assert_eq!(d.active, Some(1), "a nearer visible soundscape takes over");
        let d = decide(&list, [40.0, 0.0, 0.0], Some(1), |_, _| true);
        assert_eq!(d.active, Some(0));
        let d = decide(&list, [400.0, 0.0, 0.0], Some(0), |e, _| e.id == 1);
        assert_eq!(d.active, Some(0), "b is hidden: a stays");
        assert!(!d.kept_previous, "a is still in range and seen");
    }

    #[test]
    fn triggers_push_and_pop_their_soundscapes_like_the_engine() {
        let links = vec![
            TriggerLink { trigger: 10, triggerable: Some(1) },
            TriggerLink { trigger: 11, triggerable: Some(2) },
            TriggerLink { trigger: 12, triggerable: None },
        ];
        let mut t = TriggerList::default();
        assert_eq!(t.step(&links, &[], Some(7)), Some(7), "nothing touched, nothing changes");
        assert_eq!(t.step(&links, &[10], Some(7)), Some(1), "entering a trigger makes its soundscape current");
        assert_eq!(t.step(&links, &[10, 11], Some(1)), Some(2), "the last entered wins");
        assert_eq!(t.soundscapes(), [2, 1]);
        assert_eq!(t.step(&links, &[10], Some(2)), Some(1), "leaving falls back to the other trigger");
        assert_eq!(t.step(&links, &[], Some(1)), None, "leaving the last one clears the soundscape");
        assert_eq!(
            t.step(&links, &[12], Some(5)),
            Some(5),
            "a trigger naming no env_soundscape_triggerable does nothing"
        );
    }
}
