use crate::mixer::MIX_RATE;

const COMBS: [usize; 4] = [1116, 1188, 1277, 1356];
const ALLPASS: [usize; 2] = [556, 441];
const SPREAD: usize = 23;
const MAX_PREDELAY: usize = 4096;
const MAX_ECHO: usize = 24_000;
const INPUT_GAIN: f32 = 0.015;
pub const LAST_STANDARD: i32 = 28;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub feedback: f32,
    pub damp: f32,
    pub wet: f32,
    pub lowpass_hz: f32,
    pub predelay_ms: f32,
    pub echo_ms: f32,
    pub echo_feedback: f32,
    pub echo_mix: f32,
    pub modulation: f32,
}

impl Preset {
    pub const OFF: Preset = Preset {
        feedback: 0.0,
        damp: 0.0,
        wet: 0.0,
        lowpass_hz: 0.0,
        predelay_ms: 0.0,
        echo_ms: 0.0,
        echo_feedback: 0.0,
        echo_mix: 0.0,
        modulation: 0.0,
    };

    fn lerp(a: &Preset, b: &Preset, t: f32) -> Preset {
        let m = |x: f32, y: f32| x + (y - x) * t;
        let lp = |x: f32| if x <= 0.0 { 20_000.0 } else { x };
        Preset {
            feedback: m(a.feedback, b.feedback),
            damp: m(a.damp, b.damp),
            wet: m(a.wet, b.wet),
            lowpass_hz: m(lp(a.lowpass_hz), lp(b.lowpass_hz)),
            predelay_ms: m(a.predelay_ms, b.predelay_ms),
            echo_ms: m(
                if a.echo_mix > 0.0 { a.echo_ms } else { b.echo_ms },
                if b.echo_mix > 0.0 { b.echo_ms } else { a.echo_ms },
            ),
            echo_feedback: m(a.echo_feedback, b.echo_feedback),
            echo_mix: m(a.echo_mix, b.echo_mix),
            modulation: m(a.modulation, b.modulation),
        }
    }

    fn is_off(&self) -> bool {
        self.wet <= 0.0 && self.echo_mix <= 0.0 && (self.lowpass_hz <= 0.0 || self.lowpass_hz >= 19_000.0)
    }
}

fn room(feedback: f32, damp: f32, wet: f32, predelay_ms: f32) -> Preset {
    Preset { feedback, damp, wet, predelay_ms, ..Preset::OFF }
}

pub fn preset(dsp: i32) -> Preset {
    let pick = |a: f32, b: f32, c: f32, i: i32| match i % 3 {
        0 => a,
        1 => b,
        _ => c,
    };
    match dsp {
        0 => Preset::OFF,
        1 => room(0.78, 0.5, 0.25, 10.0),
        2..=4 => {
            let i = dsp - 2;
            room(pick(0.80, 0.84, 0.88, i), 0.15, pick(0.30, 0.33, 0.36, i), pick(5.0, 10.0, 15.0, i))
        }
        5..=7 => {
            let i = dsp - 5;
            Preset {
                echo_ms: pick(60.0, 90.0, 130.0, i),
                echo_feedback: 0.35,
                echo_mix: 0.25,
                ..room(pick(0.82, 0.85, 0.88, i), 0.35, pick(0.30, 0.34, 0.38, i), 10.0)
            }
        }
        8..=10 => {
            let i = dsp - 8;
            room(pick(0.80, 0.84, 0.88, i), 0.4, pick(0.30, 0.34, 0.38, i), pick(10.0, 20.0, 30.0, i))
        }
        11..=13 => {
            let i = dsp - 11;
            room(pick(0.78, 0.82, 0.86, i), 0.08, pick(0.28, 0.31, 0.34, i), 8.0)
        }
        14..=16 => {
            let i = dsp - 14;
            Preset { lowpass_hz: pick(1200.0, 800.0, 500.0, i), ..room(0.75, 0.7, pick(0.20, 0.25, 0.30, i), 5.0) }
        }
        17..=19 => {
            let i = dsp - 17;
            room(pick(0.79, 0.83, 0.87, i), 0.45, pick(0.26, 0.30, 0.34, i), 12.0)
        }
        20..=22 => {
            let i = dsp - 20;
            Preset {
                echo_ms: pick(180.0, 280.0, 400.0, i),
                echo_feedback: 0.2,
                echo_mix: 0.18,
                ..room(pick(0.50, 0.55, 0.60, i), 0.5, pick(0.06, 0.08, 0.10, i), 0.0)
            }
        }
        23..=25 => {
            let i = dsp - 23;
            room(pick(0.88, 0.91, 0.94, i), 0.4, pick(0.38, 0.42, 0.46, i), pick(30.0, 50.0, 70.0, i))
        }
        26..=28 => {
            let i = dsp - 26;
            Preset {
                echo_ms: pick(20.0, 30.0, 40.0, i),
                echo_feedback: 0.6,
                echo_mix: 0.4,
                modulation: pick(0.5, 0.7, 0.9, i),
                ..room(0.8, 0.3, 0.3, 0.0)
            }
        }
        _ => room(0.78, 0.5, 0.25, 10.0),
    }
}

#[derive(Clone)]
struct Comb {
    buf: Vec<f32>,
    at: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], at: 0, store: 0.0 }
    }

    fn run(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.at];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.at] = x + self.store * feedback;
        self.at = (self.at + 1) % self.buf.len();
        out
    }
}

#[derive(Clone)]
struct Allpass {
    buf: Vec<f32>,
    at: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], at: 0 }
    }

    fn run(&mut self, x: f32) -> f32 {
        let b = self.buf[self.at];
        let out = b - x;
        self.buf[self.at] = x + b * 0.5;
        self.at = (self.at + 1) % self.buf.len();
        out
    }
}

#[derive(Clone)]
struct Line {
    buf: Vec<f32>,
    at: usize,
}

impl Line {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len], at: 0 }
    }

    fn read(&self, delay: f32) -> f32 {
        let n = self.buf.len();
        let d = delay.clamp(1.0, (n - 2) as f32);
        let i = d.floor() as usize;
        let f = d - i as f32;
        let a = self.buf[(self.at + n - i) % n];
        let b = self.buf[(self.at + n - i - 1) % n];
        a + (b - a) * f
    }

    fn write(&mut self, x: f32) {
        self.at = (self.at + 1) % self.buf.len();
        self.buf[self.at] = x;
    }
}

#[derive(Clone)]
pub struct Dsp {
    combs: [Vec<Comb>; 2],
    allpass: [Vec<Allpass>; 2],
    pre: Line,
    echo: [Line; 2],
    lp: [f32; 2],
    from: Preset,
    to: Preset,
    progress: f32,
    rate: f32,
    phase: f32,
    index: i32,
}

impl Default for Dsp {
    fn default() -> Self {
        Self::new()
    }
}

impl Dsp {
    pub fn new() -> Self {
        let combs = [0, SPREAD].map(|s| COMBS.iter().map(|c| Comb::new(c + s)).collect());
        let allpass = [0, SPREAD].map(|s| ALLPASS.iter().map(|c| Allpass::new(c + s)).collect());
        Self {
            combs,
            allpass,
            pre: Line::new(MAX_PREDELAY),
            echo: [Line::new(MAX_ECHO), Line::new(MAX_ECHO)],
            lp: [0.0; 2],
            from: Preset::OFF,
            to: Preset::OFF,
            progress: 1.0,
            rate: 1.0,
            phase: 0.0,
            index: 0,
        }
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    pub fn current(&self) -> Preset {
        Preset::lerp(&self.from, &self.to, self.progress)
    }

    pub fn set(&mut self, dsp: i32, fade_seconds: f32) {
        if dsp == self.index {
            return;
        }
        self.from = self.current();
        self.to = preset(dsp);
        self.index = dsp;
        self.progress = 0.0;
        let frames = (fade_seconds.max(0.0) * MIX_RATE as f32).max(1.0);
        self.rate = 1.0 / frames;
    }

    pub fn process(&mut self, buf: &mut [f32], effect_volume: f32) {
        if self.progress >= 1.0 && self.to.is_off() {
            return;
        }
        let effect_volume = if effect_volume.is_finite() { effect_volume.clamp(0.0, 2.0) } else { 1.0 };
        let mut p = self.current();
        let rate = MIX_RATE as f32;
        for (n, frame) in buf.chunks_exact_mut(2).enumerate() {
            if self.progress < 1.0 && n % 64 == 0 {
                self.progress = (self.progress + self.rate * 64.0).min(1.0);
                p = self.current();
            }
            let lp_hz = if p.lowpass_hz <= 0.0 { 20_000.0 } else { p.lowpass_hz };
            let lp_a = 1.0 - (-std::f32::consts::TAU * lp_hz / rate).exp();
            let input = (frame[0] + frame[1]) * 0.5;
            self.pre.write(input);
            let delayed = if p.predelay_ms > 0.0 { self.pre.read(p.predelay_ms * rate / 1000.0) } else { input };
            self.phase = (self.phase + 0.7 / rate) % 1.0;
            let wobble = 1.0 + 0.3 * p.modulation * (self.phase * std::f32::consts::TAU).sin();
            let damp = p.damp.clamp(0.0, 0.99);
            let feedback = p.feedback.clamp(0.0, 0.97);
            for (c, sample) in frame.iter_mut().enumerate() {
                self.lp[c] += lp_a * (*sample - self.lp[c]);
                let dry = self.lp[c];
                let mut rev = 0.0;
                if p.wet > 0.0 {
                    for comb in &mut self.combs[c] {
                        rev += comb.run(delayed * INPUT_GAIN, feedback, damp);
                    }
                    for ap in &mut self.allpass[c] {
                        rev = ap.run(rev);
                    }
                }
                let mut echo = 0.0;
                if p.echo_mix > 0.0 {
                    let d = (p.echo_ms * wobble * rate / 1000.0).max(2.0);
                    echo = self.echo[c].read(d);
                    self.echo[c].write(*sample + echo * p.echo_feedback.clamp(0.0, 0.9));
                } else {
                    self.echo[c].write(0.0);
                }
                *sample = dry + (rev * p.wet * 3.0 + echo * p.echo_mix) * effect_volume;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(buf: &[f32]) -> f32 {
        buf.iter().map(|s| s * s).sum::<f32>()
    }

    fn impulse_tail(dsp: i32) -> (f32, f32) {
        let mut d = Dsp::new();
        d.set(dsp, 0.0);
        let mut buf = vec![0.0; 2 * 44_100];
        buf[0] = 1.0;
        buf[1] = 1.0;
        d.process(&mut buf, 1.0);
        (energy(&buf[2 * 2205..2 * 22_050]), energy(&buf[2 * 22_050..]))
    }

    #[test]
    fn rooms_leave_a_tail_and_off_passes_through() {
        let (off, _) = impulse_tail(0);
        assert_eq!(off, 0.0, "no effect: no tail");
        let (small, _) = impulse_tail(2);
        let (big, big_late) = impulse_tail(25);
        assert!(small > 1e-6, "a metal room rings: {small}");
        assert!(big > small, "a large cavern rings longer than a small metal room: {big} {small}");
        assert!(big_late > 0.0);
        for dsp in 1..=LAST_STANDARD {
            let (tail, _) = impulse_tail(dsp);
            assert!(tail.is_finite() && tail > 0.0, "preset {dsp}");
        }
    }

    #[test]
    fn water_cuts_the_highs() {
        let tone = |hz: f32, dsp: i32| {
            let mut d = Dsp::new();
            d.set(dsp, 0.0);
            let mut buf: Vec<f32> =
                (0..2 * 22_050).map(|i| ((i / 2) as f32 * hz * std::f32::consts::TAU / 44_100.0).sin() * 0.5).collect();
            d.process(&mut buf, 0.0);
            energy(&buf[2 * 11_025..])
        };
        let high = tone(6000.0, 16) / tone(6000.0, 0);
        let low = tone(200.0, 16) / tone(200.0, 0);
        assert!(high < 0.05, "6 kHz under water: {high}");
        assert!(low > 0.8, "200 Hz under water: {low}");
    }

    #[test]
    fn presets_crossfade_over_the_fade_time() {
        let mut d = Dsp::new();
        d.set(25, 2.0);
        assert!(d.current().wet < 1e-6);
        let mut buf = vec![0.0; 2 * 44_100];
        d.process(&mut buf, 1.0);
        let half = d.current().wet;
        assert!((half - preset(25).wet * 0.5).abs() < 0.01, "half way after 1 s of 2: {half}");
        d.process(&mut buf, 1.0);
        assert!((d.current().wet - preset(25).wet).abs() < 1e-6);
        assert_eq!(preset(99), preset(1), "values past the standard presets play the generic room");
    }
}
