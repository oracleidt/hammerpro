use std::sync::OnceLock;

use crate::Sound;

const RATE: u32 = 44_100;
const ATTACK_SECONDS: f64 = 0.004;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chime {
    Success,
    Failure,
}

struct Partial {
    ratio: f64,
    amplitude: f64,
    decay: f64,
}

struct Voice {
    start: f64,
    pitch: f64,
    gain: f64,
}

fn render(voices: &[Voice], partials: &[Partial], seconds: f64, level: f32) -> Sound {
    let frames = (seconds * f64::from(RATE)) as usize;
    let fade = (0.05 * f64::from(RATE)) as usize;
    let mut samples = vec![0.0f32; frames];
    for (i, out) in samples.iter_mut().enumerate() {
        let t = i as f64 / f64::from(RATE);
        let mut v = 0.0;
        for voice in voices {
            let local = t - voice.start;
            if local < 0.0 {
                continue;
            }
            let attack = (local / ATTACK_SECONDS).min(1.0);
            for p in partials {
                let phase = std::f64::consts::TAU * voice.pitch * p.ratio * local;
                v += voice.gain * p.amplitude * (-local / p.decay).exp() * attack * phase.sin();
            }
        }
        let tail = frames.saturating_sub(i);
        let end = if tail < fade { tail as f64 / fade as f64 } else { 1.0 };
        *out = (v * end) as f32;
    }
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        let k = level / peak;
        for s in &mut samples {
            *s *= k;
        }
    }
    Sound { rate: RATE, channels: 1, samples, looped: false, loop_start: 0 }
}

pub fn synthesize(kind: Chime) -> Sound {
    match kind {
        Chime::Success => {
            let bell = [
                Partial { ratio: 1.0, amplitude: 1.0, decay: 0.28 },
                Partial { ratio: 2.0, amplitude: 0.42, decay: 0.18 },
                Partial { ratio: 3.01, amplitude: 0.16, decay: 0.11 },
                Partial { ratio: 4.2, amplitude: 0.07, decay: 0.07 },
            ];
            let voices =
                [Voice { start: 0.0, pitch: 880.0, gain: 0.8 }, Voice { start: 0.09, pitch: 1318.51, gain: 1.0 }];
            render(&voices, &bell, 0.65, 0.8)
        }
        Chime::Failure => {
            let soft = [
                Partial { ratio: 1.0, amplitude: 1.0, decay: 0.22 },
                Partial { ratio: 2.0, amplitude: 0.2, decay: 0.12 },
            ];
            let voices =
                [Voice { start: 0.0, pitch: 392.0, gain: 1.0 }, Voice { start: 0.12, pitch: 293.66, gain: 0.9 }];
            render(&voices, &soft, 0.6, 0.55)
        }
    }
}

pub fn chime(kind: Chime) -> &'static Sound {
    static SUCCESS: OnceLock<Sound> = OnceLock::new();
    static FAILURE: OnceLock<Sound> = OnceLock::new();
    match kind {
        Chime::Success => SUCCESS.get_or_init(|| synthesize(Chime::Success)),
        Chime::Failure => FAILURE.get_or_init(|| synthesize(Chime::Failure)),
    }
}

pub fn play_chime(kind: Chime, volume: f32) -> bool {
    let volume = if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 0.0 };
    if volume <= 0.0 {
        return false;
    }
    crate::play::play_overlay(chime(kind), volume)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chimes_are_short_soft_and_end_in_silence() {
        for kind in [Chime::Success, Chime::Failure] {
            let s = synthesize(kind);
            assert_eq!((s.rate, s.channels), (RATE, 1));
            assert!((0.5..=0.7).contains(&s.duration()), "{kind:?}: {}", s.duration());
            assert!(s.peak() <= 0.81 && s.peak() > 0.3, "{kind:?}: {}", s.peak());
            assert!(s.samples.last().is_some_and(|v| v.abs() < 1e-4), "{kind:?} fades out");
            assert!(s.samples[0].abs() < 1e-3, "{kind:?} starts without a click");
        }
        assert!(synthesize(Chime::Failure).peak() < synthesize(Chime::Success).peak(), "failure is softer");
        assert!(std::ptr::eq(chime(Chime::Success), chime(Chime::Success)), "rendered once");
        assert!(!play_chime(Chime::Success, 0.0), "volume 0 plays nothing");
    }
}
