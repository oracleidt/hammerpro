use std::collections::HashMap;

use crate::library::{Flat, FlatKind};
use crate::script::{Interval, Position, wave_path};
use crate::soundlevel;

pub const FADE_TIME: f64 = 3.0;
pub const RANDOM_RADIUS: f64 = 36.0;
pub const POSITIONED_START_VOLUME: f32 = 0.05;
pub const MAX_POSITIONS: usize = 32;

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next_u64() % n as u64) as usize }
    }

    pub fn interval(&mut self, i: Interval) -> f32 {
        i.sample(self.unit())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Loop {
    pub wave: String,
    pub pitch: i32,
    pub ambient: bool,
    pub position: [f64; 3],
    pub soundlevel: f32,
    pub current: f32,
    pub target: f32,
    pub serial: u64,
    id: u64,
}

impl Loop {
    pub fn key(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ self.serial;
        h = h.wrapping_mul(0x0100_0000_01b3);
        h & 0x7fff_ffff_ffff_ffff
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RandomSound {
    waves: Vec<String>,
    volume: Interval,
    pitch: Interval,
    soundlevel: Interval,
    time: Interval,
    master: f32,
    ambient: bool,
    random_position: bool,
    position: [f64; 3],
    next: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    Ambient,
    At([f64; 3]),
    AroundListener(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fired {
    pub wave: String,
    pub volume: f32,
    pub pitch: i32,
    pub soundlevel: f32,
    pub place: Place,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dsp {
    pub room: Option<i32>,
    pub player: Option<i32>,
    pub volume: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct Client {
    pub loops: Vec<Loop>,
    randoms: Vec<RandomSound>,
    loop_id: u64,
    serial: u64,
    now: f64,
    next_random: f64,
    rng: Rng,
    pub playing: Option<(u32, String)>,
    pub dsp: Dsp,
}

impl Default for Client {
    fn default() -> Self {
        Self::new(0x5eed)
    }
}

pub fn audible_pitch(pitch: i32) -> bool {
    pitch > 0
}

impl Client {
    pub fn new(seed: u64) -> Self {
        Self {
            loops: Vec::new(),
            randoms: Vec::new(),
            loop_id: 0,
            serial: 0,
            now: 0.0,
            next_random: 0.0,
            rng: Rng::new(seed),
            playing: None,
            dsp: Dsp::default(),
        }
    }

    pub fn now(&self) -> f64 {
        self.now
    }

    pub fn random_count(&self) -> usize {
        self.randoms.len()
    }

    pub fn clear(&mut self) {
        self.loops.clear();
        self.randoms.clear();
        self.playing = None;
        self.dsp = Dsp::default();
    }

    pub fn start(&mut self, entity: u32, name: &str, flat: &Flat, positions: &[Option<[f64; 3]>]) {
        for l in &mut self.loops {
            l.target = 0.0;
        }
        self.loop_id += 1;
        self.randoms.clear();
        self.next_random = self.now;
        self.playing = Some((entity, name.to_owned()));
        if flat.dsp.is_some() {
            self.dsp.room = flat.dsp;
        }
        if flat.dsp_player.is_some() {
            self.dsp.player = flat.dsp_player;
        }
        self.dsp.volume = flat.dsp_volume;
        let mut masters: HashMap<u32, f32> = HashMap::new();
        let position_of = |p: Position| match p {
            Position::Index(i) => positions.get(usize::from(i)).copied().flatten(),
            _ => None,
        };
        for s in &flat.sounds {
            let mut master = 1.0;
            for (node, i) in &s.masters {
                let v = *masters.entry(*node).or_insert_with(|| self.rng.interval(*i));
                master *= v;
            }
            match s.kind {
                FlatKind::Looping => {
                    let volume = master * self.rng.interval(s.volume);
                    let pitch = self.rng.interval(s.pitch) as i32;
                    let Some(wave) = s.waves.first().and_then(|w| wave_path(w)) else { continue };
                    if volume == 0.0 {
                        continue;
                    }
                    match s.position {
                        Position::Index(_) => match position_of(s.position) {
                            Some(at) => self.add_loop(&wave, false, volume, s.soundlevel, pitch, at),
                            None => continue,
                        },
                        _ => self.add_loop(&wave, true, volume, soundlevel::NONE, pitch, [0.0; 3]),
                    }
                }
                FlatKind::Random => {
                    let waves: Vec<String> = s.waves.iter().filter_map(|w| wave_path(w)).collect();
                    if waves.is_empty() {
                        continue;
                    }
                    let (ambient, random_position, position) = match s.position {
                        Position::None => (true, false, [0.0; 3]),
                        Position::Random => (false, true, [0.0; 3]),
                        Position::Index(_) => match position_of(s.position) {
                            Some(at) => (false, false, at),
                            None => continue,
                        },
                    };
                    let next = self.now + 0.5 * f64::from(self.rng.interval(s.time));
                    self.randoms.push(RandomSound {
                        waves,
                        volume: s.volume,
                        pitch: s.pitch,
                        soundlevel: Interval::fixed(s.soundlevel),
                        time: s.time,
                        master,
                        ambient,
                        random_position,
                        position,
                        next,
                    });
                }
            }
        }
    }

    fn add_loop(&mut self, wave: &str, ambient: bool, volume: f32, level: f32, pitch: i32, at: [f64; 3]) {
        let found = self.loops.iter().rposition(|l| {
            l.id != self.loop_id && l.pitch == pitch && l.wave.eq_ignore_ascii_case(wave) && l.ambient == ambient
        });
        let slot = match found {
            Some(i) => i,
            None => {
                self.serial += 1;
                self.loops.push(Loop {
                    wave: wave.to_owned(),
                    pitch,
                    ambient,
                    position: at,
                    soundlevel: level,
                    current: if ambient { 0.0 } else { POSITIONED_START_VOLUME },
                    target: 0.0,
                    serial: self.serial,
                    id: 0,
                });
                self.loops.len() - 1
            }
        };
        let l = &mut self.loops[slot];
        l.target = volume;
        l.position = at;
        l.soundlevel = level;
        l.id = self.loop_id;
    }

    pub fn update(&mut self, dt: f64, fade_time: f64) -> Vec<Fired> {
        let dt = dt.clamp(0.0, 1.0);
        self.now += dt;
        let amount = if fade_time > 0.0 { (dt / fade_time) as f32 } else { dt as f32 };
        for l in &mut self.loops {
            if l.current != l.target {
                l.current = if l.current < l.target {
                    (l.current + amount).min(l.target)
                } else {
                    (l.current - amount).max(l.target)
                };
            }
        }
        self.loops.retain(|l| !(l.target == 0.0 && l.current == 0.0));
        let mut out = Vec::new();
        if self.now < self.next_random {
            return out;
        }
        self.next_random = self.now + 3600.0;
        for i in (0..self.randoms.len()).rev() {
            if self.now >= self.randoms[i].next {
                let fired = self.fire(i);
                out.extend(fired);
                let gap = f64::from(self.rng.interval(self.randoms[i].time));
                self.randoms[i].next = self.now + gap;
            }
            if self.randoms[i].next < self.next_random {
                self.next_random = self.randoms[i].next;
            }
        }
        out
    }

    fn fire(&mut self, i: usize) -> Option<Fired> {
        let r = self.randoms[i].clone();
        let wave = r.waves[self.rng.below(r.waves.len())].clone();
        let volume = r.master * self.rng.interval(r.volume);
        if r.ambient {
            let pitch = self.rng.interval(r.pitch) as i32;
            return Some(Fired { wave, volume, pitch, soundlevel: soundlevel::NONE, place: Place::Ambient });
        }
        let level = self.rng.interval(r.soundlevel).trunc();
        let pitch = self.rng.interval(r.pitch) as i32;
        let place = if r.random_position {
            Place::AroundListener(self.rng.unit() * 360.0 - 180.0)
        } else {
            Place::At(r.position)
        };
        Some(Fired { wave, volume, pitch, soundlevel: level, place })
    }
}

pub fn around(eye: [f64; 3], yaw_deg: f64, angle_deg: f32) -> [f64; 3] {
    let yaw = yaw_deg.to_radians();
    let forward = [yaw.cos(), yaw.sin()];
    let right = [yaw.sin(), -yaw.cos()];
    let (s, c) = f64::from(angle_deg).to_radians().sin_cos();
    [
        eye[0] + RANDOM_RADIUS * (c * right[0] + s * forward[0]),
        eye[1] + RANDOM_RADIUS * (c * right[1] + s * forward[1]),
        eye[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Library, Origin};

    fn lib(text: &str) -> Library {
        let mut l = Library::default();
        l.add_file("x.txt", text, Origin::Game, false);
        l
    }

    #[test]
    fn loops_fade_linearly_over_the_fade_time_and_are_reused() {
        let l = lib(r#"
"a" { "dsp" "5" "playlooping" { "volume" "0.9" "pitch" "100" "wave" "x.wav" } "playlooping" { "volume" "0.3" "wave" "y.wav" } }
"b" { "playlooping" { "volume" "0.6" "pitch" "100" "wave" "x.wav" } }
"#);
        let mut c = Client::new(1);
        c.start(1, "a", &l.flatten("a"), &[]);
        assert_eq!(c.dsp.room, Some(5));
        assert_eq!(c.loops.len(), 2);
        assert!(c.loops.iter().all(|l| l.current == 0.0), "ambient loops start silent");
        c.update(0.75, FADE_TIME);
        c.update(0.75, FADE_TIME);
        let x = c.loops.iter().find(|l| l.wave == "sound/x.wav").unwrap();
        assert!((x.current - 0.5).abs() < 1e-5, "1.5 s of a 3 s fade adds 0.5 of volume: {}", x.current);
        let y = c.loops.iter().find(|l| l.wave == "sound/y.wav").unwrap();
        assert!((y.current - 0.3).abs() < 1e-6, "the fade stops at the target");
        for _ in 0..3 {
            c.update(1.0, FADE_TIME);
        }
        c.start(2, "b", &l.flatten("b"), &[]);
        assert_eq!(c.dsp.room, Some(5), "a soundscape without dsp keeps the room effect");
        assert_eq!(c.loops.len(), 2, "x.wav at the same pitch is reused, not restarted");
        let x = c.loops.iter().find(|l| l.wave == "sound/x.wav").unwrap();
        assert_eq!((x.current, x.target), (0.9, 0.6));
        c.update(0.3, FADE_TIME);
        let y = c.loops.iter().find(|l| l.wave == "sound/y.wav").unwrap();
        assert!((y.current - 0.2).abs() < 1e-5);
        c.update(1.0, FADE_TIME);
        assert_eq!(c.loops.len(), 1, "y.wav faded out and was removed");
        assert!((c.loops[0].current - 0.6).abs() < 1e-6);
    }

    #[test]
    fn random_sounds_follow_the_client_timing() {
        let l = lib(
            r#""r" { "playrandom" { "time" "2,4" "volume" "0.5" "pitch" "90,110" "rndwave" { "wave" "a.wav" "wave" "b.wav" } } }"#,
        );
        let mut c = Client::new(3);
        c.start(1, "r", &l.flatten("r"), &[]);
        let mut times = Vec::new();
        let mut t = 0.0;
        for _ in 0..6000 {
            t += 0.01;
            for f in c.update(0.01, FADE_TIME) {
                assert_eq!(f.place, Place::Ambient);
                assert!((f.volume - 0.5).abs() < 1e-6);
                assert!((90..=110).contains(&f.pitch));
                times.push(t);
            }
        }
        assert!(times[0] >= 0.99 && times[0] <= 2.02, "first sound after half an interval: {}", times[0]);
        for w in times.windows(2) {
            assert!((1.98..=4.02).contains(&(w[1] - w[0])));
        }
    }

    #[test]
    fn positions_and_random_positions_like_the_engine() {
        let l = lib(
            r#""p" { "playlooping" { "volume" "1" "wave" "a.wav" "position" "0" } "playlooping" { "volume" "1" "wave" "b.wav" "position" "1" } "playrandom" { "time" "0.1" "volume" "1" "pitch" "100" "position" "random" "rndwave" { "wave" "c.wav" } } }"#,
        );
        let mut c = Client::new(5);
        c.start(1, "p", &l.flatten("p"), &[Some([10.0, 0.0, 0.0]), None]);
        assert_eq!(c.loops.len(), 1, "a missing position target suppresses the sound");
        assert_eq!(c.loops[0].current, POSITIONED_START_VOLUME);
        assert!(!c.loops[0].ambient);
        let fired = (0..100).flat_map(|_| c.update(0.05, FADE_TIME)).next().unwrap();
        let Place::AroundListener(a) = fired.place else { panic!() };
        let p = around([0.0, 0.0, 64.0], 0.0, a);
        let d = (p[0] * p[0] + p[1] * p[1]).sqrt();
        assert!((d - RANDOM_RADIUS).abs() < 1e-6, "random sounds are 36 units from the eyes");
        assert_eq!(p[2], 64.0);
    }
}
