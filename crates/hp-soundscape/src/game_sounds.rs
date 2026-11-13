use std::collections::HashMap;

use crate::script::Interval;
use crate::soundlevel;

#[derive(Clone, Debug, PartialEq)]
pub struct GameSound {
    pub name: String,
    pub waves: Vec<String>,
    pub volume: Interval,
    pub pitch: Interval,
    pub soundlevel: f32,
}

#[derive(Clone, Debug, Default)]
pub struct GameSounds {
    map: HashMap<String, GameSound>,
}

fn named_volume(v: &str) -> Option<Interval> {
    let t = v.trim();
    if t.eq_ignore_ascii_case("VOL_NORM") {
        return Some(Interval::fixed(1.0));
    }
    Interval::parse(t)
}

fn named_pitch(v: &str) -> Option<Interval> {
    let t = v.trim();
    let named = [("PITCH_NORM", 100.0), ("PITCH_LOW", 95.0), ("PITCH_HIGH", 120.0)];
    if let Some((_, p)) = named.iter().find(|(n, _)| n.eq_ignore_ascii_case(t)) {
        return Some(Interval::fixed(*p));
    }
    Interval::parse(t)
}

impl GameSounds {
    pub fn add_file(&mut self, text: &str, replace: bool) {
        let Ok(root) = hp_kv::parse(text) else { return };
        for b in &root.children {
            let mut waves: Vec<String> = b.get_all("wave").map(|w| w.trim().to_owned()).collect();
            for r in b.children_named("rndwave") {
                waves.extend(r.get_all("wave").map(|w| w.trim().to_owned()));
            }
            waves.retain(|w| !w.is_empty());
            let s = GameSound {
                name: b.name.trim().to_owned(),
                waves,
                volume: b.get("volume").and_then(named_volume).unwrap_or(Interval::fixed(1.0)),
                pitch: b.get("pitch").and_then(named_pitch).unwrap_or(Interval::fixed(100.0)),
                soundlevel: b.get("soundlevel").and_then(soundlevel::parse).unwrap_or(soundlevel::NORM),
            };
            let key = s.name.to_ascii_lowercase();
            if replace || !self.map.contains_key(&key) {
                self.map.insert(key, s);
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&GameSound> {
        self.map.get(&name.trim().to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soundscripts_resolve_to_waves() {
        let mut g = GameSounds::default();
        g.add_file(
            r#""Ambient.Wind" { "channel" "CHAN_STATIC" "volume" "VOL_NORM" "pitch" "PITCH_LOW" "soundlevel" "SNDLVL_80dB" "wave" "ambient/wind.wav" }
               "Ambient.Rnd" { "volume" "0.4,0.6" "rndwave" { "wave" "a.wav" "wave" "b.wav" } }"#,
            false,
        );
        g.add_file(r#""ambient.wind" { "wave" "other.wav" }"#, false);
        assert_eq!(g.len(), 2);
        let w = g.get("AMBIENT.WIND").unwrap();
        assert_eq!(w.waves, ["ambient/wind.wav"], "the first definition stays");
        assert_eq!(w.pitch, Interval::fixed(95.0));
        assert_eq!(w.soundlevel, 80.0);
        let r = g.get("Ambient.Rnd").unwrap();
        assert_eq!(r.waves.len(), 2);
        assert_eq!(r.volume, Interval { min: 0.4, max: 0.6 });
        g.add_file(r#""ambient.wind" { "wave" "other.wav" }"#, true);
        assert_eq!(g.get("ambient.wind").unwrap().waves, ["other.wav"]);
    }
}
