use serde::{Deserialize, Serialize};

use crate::{Sound, db_to_gain};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edit {
    pub gain_db: f32,
    pub start: f64,
    pub end: Option<f64>,
    pub fade_in: f64,
    pub fade_out: f64,
    pub looped: bool,
    pub loop_start: f64,
    pub mono: bool,
    pub rate: Option<u32>,
}

impl Default for Edit {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            start: 0.0,
            end: None,
            fade_in: 0.0,
            fade_out: 0.0,
            looped: false,
            loop_start: 0.0,
            mono: false,
            rate: None,
        }
    }
}

impl Edit {
    pub fn from_sound(sound: &Sound) -> Self {
        Self {
            looped: sound.looped,
            loop_start: if sound.rate == 0 { 0.0 } else { f64::from(sound.loop_start) / f64::from(sound.rate) },
            ..Self::default()
        }
    }

    pub fn range_frames(&self, sound: &Sound) -> (usize, usize) {
        let rate = f64::from(sound.rate.max(1));
        let frames = sound.frames();
        let finite = |v: f64, d: f64| if v.is_finite() { v } else { d };
        let start = ((finite(self.start, 0.0).max(0.0) * rate).round() as usize).min(frames);
        let end = match self.end {
            Some(e) => ((finite(e, 0.0).max(0.0) * rate).round() as usize).min(frames),
            None => frames,
        };
        (start, end.max(start))
    }
}

pub fn normalize_gain_db(sound: &Sound, edit: &Edit, target_db: f32) -> f32 {
    let (a, b) = edit.range_frames(sound);
    let ch = usize::from(sound.channels.max(1));
    let peak = sound.samples[a * ch..b * ch].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    if peak <= 1e-6 { 0.0 } else { target_db - 20.0 * peak.log10() }
}

pub fn apply(sound: &Sound, edit: &Edit) -> Sound {
    let ch = usize::from(sound.channels.max(1));
    let (a, b) = edit.range_frames(sound);
    let mut out = Sound {
        rate: sound.rate,
        channels: sound.channels,
        samples: sound.samples[a * ch..b * ch].to_vec(),
        looped: edit.looped,
        loop_start: 0,
    };
    if edit.mono && out.channels == 2 {
        out.samples = out.samples.chunks_exact(2).map(|p| (p[0] + p[1]) * 0.5).collect();
        out.channels = 1;
    }
    let ch = usize::from(out.channels.max(1));
    let frames = out.frames();
    let rate = f64::from(out.rate.max(1));
    let gain = db_to_gain(if edit.gain_db.is_finite() { edit.gain_db } else { 0.0 });
    let fade_in = (edit.fade_in.max(0.0) * rate) as usize;
    let fade_out = (edit.fade_out.max(0.0) * rate) as usize;
    for f in 0..frames {
        let mut g = gain;
        if fade_in > 0 && f < fade_in {
            g *= f as f32 / fade_in as f32;
        }
        if fade_out > 0 && frames - f <= fade_out {
            g *= (frames - f) as f32 / fade_out as f32;
        }
        for s in &mut out.samples[f * ch..(f + 1) * ch] {
            *s = (*s * g).clamp(-1.0, 1.0);
        }
    }
    if let Some(target) = edit.rate.filter(|r| *r > 0 && *r != out.rate) {
        out = resample(&out, target);
    }
    out.loop_start = if edit.looped && edit.loop_start.is_finite() {
        ((edit.loop_start.max(0.0) * f64::from(out.rate)).round() as usize).min(out.frames().saturating_sub(1)) as u32
    } else {
        0
    };
    out
}

fn resample(sound: &Sound, rate: u32) -> Sound {
    let ch = usize::from(sound.channels.max(1));
    let frames = sound.frames();
    let ratio = f64::from(sound.rate) / f64::from(rate);
    let out_frames = ((frames as f64 / ratio).round() as usize).max(1);
    let mut samples = Vec::with_capacity(out_frames * ch);
    for i in 0..out_frames {
        let t = i as f64 * ratio;
        for c in 0..ch {
            let v = if ratio > 1.0 {
                let from = t.floor() as usize;
                let to = ((t + ratio).ceil() as usize).min(frames).max(from + 1);
                let mut sum = 0.0;
                let mut n = 0.0;
                for f in from..to.min(frames) {
                    sum += sound.samples[f * ch + c];
                    n += 1.0;
                }
                if n > 0.0 { sum / n } else { 0.0 }
            } else {
                let f0 = (t.floor() as usize).min(frames - 1);
                let f1 = (f0 + 1).min(frames - 1);
                let k = (t - f0 as f64) as f32;
                sound.samples[f0 * ch + c] * (1.0 - k) + sound.samples[f1 * ch + c] * k
            };
            samples.push(v);
        }
    }
    Sound { rate, channels: sound.channels, samples, looped: sound.looped, loop_start: 0 }
}

pub fn peaks(sound: &Sound, bins: usize) -> Vec<(f32, f32)> {
    let bins = bins.max(1);
    let ch = usize::from(sound.channels.max(1));
    let frames = sound.frames();
    if frames == 0 {
        return vec![(0.0, 0.0); bins];
    }
    (0..bins)
        .map(|i| {
            let from = i * frames / bins;
            let to = ((i + 1) * frames / bins).max(from + 1).min(frames);
            let mut lo = 0.0_f32;
            let mut hi = 0.0_f32;
            for f in from..to {
                let mut v = 0.0;
                for c in 0..ch {
                    v += sound.samples[f * ch + c];
                }
                v /= ch as f32;
                lo = lo.min(v);
                hi = hi.max(v);
            }
            (lo, hi)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, encode_wav};

    fn tone(frames: usize) -> Sound {
        Sound {
            rate: 22_050,
            channels: 1,
            samples: (0..frames).map(|i| (i as f32 * 0.05).sin() * 0.5).collect(),
            looped: false,
            loop_start: 0,
        }
    }

    #[test]
    fn gain_trim_and_fades() {
        let s = tone(22_050);
        let edit = Edit { gain_db: 6.0, start: 0.25, end: Some(0.75), fade_in: 0.1, ..Edit::default() };
        let out = apply(&s, &edit);
        assert_eq!(out.frames(), 11_025);
        assert!(out.samples[0].abs() < 1e-6);
        assert!(out.peak() > 0.5);
        let louder = apply(&s, &Edit { gain_db: 40.0, ..Edit::default() });
        assert!(louder.peak() <= 1.0);
    }

    #[test]
    fn normalize_reaches_target() {
        let s = tone(4_000);
        let edit = Edit::default();
        let db = normalize_gain_db(&s, &edit, -1.0);
        let out = apply(&s, &Edit { gain_db: db, ..edit });
        assert!((out.peak() - 10.0_f32.powf(-1.0 / 20.0)).abs() < 0.01);
    }

    #[test]
    fn wav_round_trip_with_loop() {
        let s = tone(2_000);
        let edit = Edit { looped: true, loop_start: 0.01, ..Edit::default() };
        let out = apply(&s, &edit);
        let bytes = encode_wav(&out);
        let back = decode(&bytes, "wav").unwrap();
        assert_eq!(back.rate, 22_050);
        assert_eq!(back.channels, 1);
        assert_eq!(back.frames(), 2_000);
        assert!(back.looped);
        assert_eq!(back.loop_start, out.loop_start);
        assert!((back.samples[100] - out.samples[100]).abs() < 1.0 / 16_000.0);
        let plain = decode(&encode_wav(&apply(&s, &Edit::default())), "wav").unwrap();
        assert!(!plain.looped);
    }

    #[test]
    fn mono_and_resample() {
        let s = Sound { rate: 44_100, channels: 2, samples: vec![0.5; 44_100 * 2], looped: false, loop_start: 0 };
        let out = apply(&s, &Edit { mono: true, rate: Some(22_050), ..Edit::default() });
        assert_eq!((out.channels, out.rate), (1, 22_050));
        assert_eq!(out.frames(), 22_050);
        assert!((out.samples[10] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn peaks_cover_the_sound() {
        let p = peaks(&tone(1_000), 10);
        assert_eq!(p.len(), 10);
        assert!(p.iter().any(|(lo, hi)| *lo < -0.1 && *hi > 0.1));
    }

    #[test]
    fn garbage_is_refused() {
        assert!(decode(b"not a sound at all", "wav").is_err());
    }
}
