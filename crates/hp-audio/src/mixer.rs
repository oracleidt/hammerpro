use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::Sound;
use crate::dsp::Dsp;

pub const MIX_RATE: u32 = 44_100;
const SWING_SECONDS: f32 = 0.08;
const FADE_OUT_SECONDS: f32 = 0.6;
const MAX_VOICES: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceParams {
    pub gain: f32,
    pub pan: f32,
    pub pitch: f32,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self { gain: 1.0, pan: 0.0, pitch: 1.0 }
    }
}

#[derive(Clone, Debug)]
pub struct LoopVoice {
    pub key: u64,
    pub sound: Arc<Sound>,
    pub params: VoiceParams,
}

struct Voice {
    key: Option<u64>,
    sound: Arc<Sound>,
    pos: f64,
    looped: bool,
    target: VoiceParams,
    gl: f32,
    gr: f32,
    alive: bool,
    done: bool,
}

#[derive(Default)]
pub struct Mixer {
    voices: Vec<Voice>,
    master: f32,
    muted: bool,
    room: Dsp,
    player: Dsp,
    effect_volume: f32,
    capture: Option<(Vec<f32>, usize)>,
    captured: Option<Vec<f32>>,
}

fn clean(p: VoiceParams) -> VoiceParams {
    let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
    VoiceParams {
        gain: fin(p.gain, 0.0).clamp(0.0, 4.0),
        pan: fin(p.pan, 0.0).clamp(-1.0, 1.0),
        pitch: fin(p.pitch, 1.0).clamp(0.05, 4.0),
    }
}

fn pan_gains(p: VoiceParams, stereo: bool) -> (f32, f32) {
    if stereo {
        (p.gain * (1.0 - p.pan).min(1.0), p.gain * (1.0 + p.pan).min(1.0))
    } else {
        let a = (p.pan + 1.0) * std::f32::consts::FRAC_PI_4;
        (p.gain * a.cos(), p.gain * a.sin())
    }
}

fn approach(current: f32, target: f32, step: f32) -> f32 {
    if (target - current).abs() <= step { target } else { current + step * (target - current).signum() }
}

impl Mixer {
    pub fn new() -> Self {
        Self {
            voices: Vec::new(),
            master: 1.0,
            muted: false,
            room: Dsp::new(),
            player: Dsp::new(),
            effect_volume: 1.0,
            capture: None,
            captured: None,
        }
    }

    pub fn set_dsp(&mut self, room: i32, player: i32, effect_volume: f32, fade_seconds: f32) {
        self.room.set(room, fade_seconds);
        self.player.set(player, fade_seconds);
        self.effect_volume = if effect_volume.is_finite() { effect_volume.clamp(0.0, 2.0) } else { 1.0 };
    }

    pub fn dsp(&self) -> (i32, i32) {
        (self.room.index(), self.player.index())
    }

    pub fn start_capture(&mut self, seconds: f32) {
        let frames = (seconds.clamp(0.05, 120.0) * MIX_RATE as f32) as usize;
        self.capture = Some((Vec::with_capacity(frames * 2), frames));
        self.captured = None;
    }

    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }

    pub fn take_capture(&mut self) -> Option<Vec<f32>> {
        self.captured.take()
    }

    pub fn set_master(&mut self, volume: f32, muted: bool) {
        self.master = if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 1.0 };
        self.muted = muted;
    }

    pub fn set_looping(&mut self, wanted: &[LoopVoice]) {
        for v in &mut self.voices {
            if v.key.is_some() {
                v.alive = false;
            }
        }
        for w in wanted {
            let params = clean(w.params);
            if let Some(v) =
                self.voices.iter_mut().find(|v| v.key == Some(w.key) && !v.done && Arc::ptr_eq(&v.sound, &w.sound))
            {
                v.target = params;
                v.alive = true;
                continue;
            }
            if self.voices.len() >= MAX_VOICES || w.sound.frames() == 0 || w.sound.rate == 0 {
                continue;
            }
            self.voices.push(Voice {
                key: Some(w.key),
                sound: Arc::clone(&w.sound),
                pos: 0.0,
                looped: true,
                target: params,
                gl: 0.0,
                gr: 0.0,
                alive: true,
                done: false,
            });
        }
    }

    pub fn one_shot(&mut self, sound: Arc<Sound>, params: VoiceParams) -> bool {
        if self.voices.len() >= MAX_VOICES || sound.frames() == 0 || sound.rate == 0 {
            return false;
        }
        let params = clean(params);
        let (gl, gr) = pan_gains(params, sound.channels >= 2);
        self.voices.push(Voice {
            key: None,
            sound,
            pos: 0.0,
            looped: false,
            target: params,
            gl,
            gr,
            alive: true,
            done: false,
        });
        true
    }

    pub fn clear(&mut self) {
        for v in &mut self.voices {
            v.alive = false;
        }
    }

    pub fn voices(&self) -> usize {
        self.voices.iter().filter(|v| v.alive && !v.done).count()
    }

    pub fn is_silent(&self) -> bool {
        self.voices.is_empty()
    }

    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let frames = out.len() / 2;
        if frames == 0 {
            return;
        }
        let master = if self.muted { 0.0 } else { self.master };
        let swing = frames as f32 / (MIX_RATE as f32 * SWING_SECONDS);
        let fade = frames as f32 / (MIX_RATE as f32 * FADE_OUT_SECONDS);
        for v in &mut self.voices {
            let stereo = v.sound.channels >= 2;
            let (tl, tr) = if v.alive { pan_gains(v.target, stereo) } else { (0.0, 0.0) };
            let step = if v.alive { swing } else { fade };
            let (nl, nr) = (approach(v.gl, tl, step), approach(v.gr, tr, step));
            let ch = usize::from(v.sound.channels.max(1));
            let total = v.sound.frames();
            let loop_start =
                if v.sound.looped { (v.sound.loop_start as usize).min(total.saturating_sub(1)) } else { 0 };
            let advance = f64::from(v.sound.rate) * f64::from(v.target.pitch) / f64::from(MIX_RATE);
            let samples = &v.sound.samples;
            for f in 0..frames {
                if v.pos >= total as f64 {
                    if v.looped && total > loop_start {
                        let span = (total - loop_start) as f64;
                        v.pos = loop_start as f64 + (v.pos - total as f64) % span;
                    } else {
                        v.done = true;
                        break;
                    }
                }
                let t = f as f32 / frames as f32;
                let gl = (v.gl + (nl - v.gl) * t) * master;
                let gr = (v.gr + (nr - v.gr) * t) * master;
                let i = v.pos as usize;
                let frac = (v.pos - i as f64) as f32;
                let j = if i + 1 < total {
                    i + 1
                } else if v.looped {
                    loop_start
                } else {
                    i
                };
                let at = |frame: usize, c: usize| samples.get(frame * ch + c.min(ch - 1)).copied().unwrap_or(0.0);
                let l = at(i, 0) + (at(j, 0) - at(i, 0)) * frac;
                let r = if ch >= 2 { at(i, 1) + (at(j, 1) - at(i, 1)) * frac } else { l };
                out[f * 2] += l * gl;
                out[f * 2 + 1] += r * gr;
                v.pos += advance;
            }
            v.gl = nl;
            v.gr = nr;
        }
        self.voices.retain(|v| !v.done && (v.alive || v.gl > 0.0 || v.gr > 0.0));
        self.room.process(out, self.effect_volume);
        self.player.process(out, self.effect_volume);
        for s in out.iter_mut() {
            *s = soft_clip(*s);
        }
        if let Some((buf, frames)) = &mut self.capture {
            let take = (*frames * 2).saturating_sub(buf.len()).min(out.len());
            buf.extend_from_slice(&out[..take]);
            if buf.len() >= *frames * 2 {
                self.captured = self.capture.take().map(|(b, _)| b);
            }
        }
    }
}

fn soft_clip(x: f32) -> f32 {
    if !x.is_finite() {
        0.0
    } else if x.abs() <= 0.8 {
        x
    } else {
        let s = x.signum();
        let over = (x.abs() - 0.8) / 0.2;
        s * (0.8 + 0.2 * over.tanh())
    }
}

fn global() -> &'static Arc<Mutex<Mixer>> {
    static MIXER: OnceLock<Arc<Mutex<Mixer>>> = OnceLock::new();
    MIXER.get_or_init(|| Arc::new(Mutex::new(Mixer::new())))
}

fn lock() -> MutexGuard<'static, Mixer> {
    global().lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn ambience_set(voices: &[LoopVoice]) {
    lock().set_looping(voices);
    if !voices.is_empty() {
        imp::wake(global());
    }
}

pub fn ambience_fire(sound: Arc<Sound>, params: VoiceParams) -> bool {
    let ok = lock().one_shot(sound, params);
    if ok {
        imp::wake(global());
    }
    ok
}

pub fn ambience_master(volume: f32, muted: bool) {
    lock().set_master(volume, muted);
}

pub fn ambience_stop() {
    lock().clear();
}

pub fn ambience_voices() -> usize {
    lock().voices()
}

pub fn ambience_dsp(room: i32, player: i32, effect_volume: f32, fade_seconds: f32) {
    lock().set_dsp(room, player, effect_volume, fade_seconds);
}

pub fn ambience_capture(seconds: f32) {
    lock().start_capture(seconds);
    imp::wake(global());
}

pub fn ambience_capturing() -> bool {
    lock().capturing()
}

pub fn ambience_take_capture() -> Option<Vec<f32>> {
    lock().take_capture()
}

pub fn ambience_available() -> bool {
    imp::available()
}

#[cfg(windows)]
mod imp {
    use std::num::NonZero;
    use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use super::{MIX_RATE, Mixer};

    const BLOCK_FRAMES: usize = 512;
    const RELEASE_AFTER: Duration = Duration::from_secs(10);

    static SENDER: OnceLock<Option<Mutex<Sender<()>>>> = OnceLock::new();

    struct MixSource {
        mixer: Arc<Mutex<Mixer>>,
        buf: Vec<f32>,
        at: usize,
    }

    impl Iterator for MixSource {
        type Item = f32;

        fn next(&mut self) -> Option<f32> {
            if self.at >= self.buf.len() {
                let mut m = self.mixer.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                m.render(&mut self.buf);
                self.at = 0;
            }
            let s = self.buf[self.at];
            self.at += 1;
            Some(s)
        }
    }

    impl rodio::Source for MixSource {
        fn current_span_len(&self) -> Option<usize> {
            None
        }

        fn channels(&self) -> rodio::ChannelCount {
            NonZero::new(2).unwrap_or(NonZero::<u16>::MIN)
        }

        fn sample_rate(&self) -> rodio::SampleRate {
            NonZero::new(MIX_RATE).unwrap_or(NonZero::<u32>::MIN)
        }

        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    pub(super) fn available() -> bool {
        true
    }

    pub(super) fn wake(mixer: &Arc<Mutex<Mixer>>) {
        let sender = SENDER.get_or_init(|| {
            let (tx, rx) = channel();
            let mixer = Arc::clone(mixer);
            match std::thread::Builder::new().name("hp-ambience".to_owned()).spawn(move || run(&mixer, &rx)) {
                Ok(_) => Some(Mutex::new(tx)),
                Err(e) => {
                    log::warn!("could not start the soundscape playback thread: {e}");
                    None
                }
            }
        });
        if let Some(tx) = sender.as_ref().and_then(|s| s.lock().ok()) {
            let _ = tx.send(());
        }
    }

    fn silent(mixer: &Arc<Mutex<Mixer>>) -> bool {
        mixer.lock().map(|m| m.is_silent() && !m.capturing()).unwrap_or(true)
    }

    fn run(mixer: &Arc<Mutex<Mixer>>, rx: &Receiver<()>) {
        let mut output: Option<rodio::MixerDeviceSink> = None;
        let mut idle_since: Option<Instant> = None;
        let mut failed_at: Option<Instant> = None;
        loop {
            let quiet = silent(mixer);
            if !quiet {
                idle_since = None;
                let retry = failed_at.is_none_or(|t| t.elapsed() > Duration::from_secs(5));
                if output.is_none() && retry {
                    match rodio::DeviceSinkBuilder::open_default_sink() {
                        Ok(mut stream) => {
                            stream.log_on_drop(false);
                            stream.mixer().add(MixSource {
                                mixer: Arc::clone(mixer),
                                buf: vec![0.0; BLOCK_FRAMES * 2],
                                at: BLOCK_FRAMES * 2,
                            });
                            output = Some(stream);
                            failed_at = None;
                        }
                        Err(e) => {
                            log::warn!("no audio output for the soundscapes: {e}");
                            failed_at = Some(Instant::now());
                        }
                    }
                }
            } else if output.is_some() {
                let since = *idle_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= RELEASE_AFTER {
                    output = None;
                    idle_since = None;
                }
            }
            let received = if quiet && output.is_none() {
                rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
            } else {
                rx.recv_timeout(Duration::from_millis(100))
            };
            if matches!(received, Err(RecvTimeoutError::Disconnected)) {
                break;
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::sync::{Arc, Mutex};

    use super::Mixer;

    pub(super) fn available() -> bool {
        false
    }

    pub(super) fn wake(_mixer: &Arc<Mutex<Mixer>>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, channels: u16, frames: usize, value: f32) -> Arc<Sound> {
        Arc::new(Sound {
            rate,
            channels,
            samples: vec![value; frames * usize::from(channels)],
            looped: false,
            loop_start: 0,
        })
    }

    fn rms(buf: &[f32], channel: usize) -> f32 {
        let n = buf.len() / 2;
        (buf.iter().skip(channel).step_by(2).map(|s| s * s).sum::<f32>() / n as f32).sqrt()
    }

    #[test]
    fn looping_voices_ramp_in_pan_and_fade_out() {
        let mut m = Mixer::new();
        let s = tone(22_050, 1, 1000, 0.5);
        m.set_looping(&[LoopVoice {
            key: 1,
            sound: Arc::clone(&s),
            params: VoiceParams { gain: 1.0, pan: -1.0, pitch: 1.0 },
        }]);
        let mut buf = vec![0.0; 2 * 8192];
        m.render(&mut buf);
        assert!(buf[0].abs() < 0.05, "starts from silence");
        m.render(&mut buf);
        assert!(rms(&buf, 0) > 0.45, "left {}", rms(&buf, 0));
        assert!(rms(&buf, 1) < 0.01, "panned hard left");
        assert_eq!(m.voices(), 1);
        m.set_looping(&[]);
        assert_eq!(m.voices(), 0);
        for _ in 0..8 {
            m.render(&mut buf);
        }
        assert!(m.is_silent(), "the voice faded out and was removed");
        assert!(rms(&buf, 0) < 1e-6);
    }

    #[test]
    fn the_same_key_keeps_its_voice_and_one_shots_end() {
        let mut m = Mixer::new();
        let s = tone(44_100, 2, 4410, 0.25);
        let v = LoopVoice { key: 9, sound: Arc::clone(&s), params: VoiceParams::default() };
        m.set_looping(std::slice::from_ref(&v));
        m.set_looping(std::slice::from_ref(&v));
        assert_eq!(m.voices(), 1);
        assert!(m.one_shot(tone(11_025, 1, 1102, 0.5), VoiceParams { gain: 0.5, pan: 0.0, pitch: 2.0 }));
        assert_eq!(m.voices(), 2);
        let mut buf = vec![0.0; 2 * 44_100];
        m.render(&mut buf);
        assert_eq!(m.voices(), 1, "a 0.1 s one shot at double pitch is over after 1 s");
        m.set_master(0.5, true);
        m.render(&mut buf);
        assert!(buf.iter().all(|s| *s == 0.0), "muted");
        assert!(!m.one_shot(tone(0, 1, 10, 0.1), VoiceParams::default()), "no rate, not played");
    }

    #[test]
    fn loud_mixes_are_clipped_softly() {
        assert_eq!(soft_clip(0.5), 0.5);
        assert!(soft_clip(10.0) <= 1.0 && soft_clip(10.0) > 0.95);
        assert!(soft_clip(-10.0) >= -1.0);
        assert_eq!(soft_clip(f32::NAN), 0.0);
    }

    fn sine(hz: f32, amp: f32, seconds: f32) -> Arc<Sound> {
        let n = (seconds * 44_100.0) as usize;
        let samples = (0..n).map(|i| (i as f32 * hz * std::f32::consts::TAU / 44_100.0).sin() * amp).collect();
        Arc::new(Sound { rate: 44_100, channels: 1, samples, looped: true, loop_start: 0 })
    }

    fn settled(m: &mut Mixer, seconds: f32) -> Vec<f32> {
        let mut warm = vec![0.0; 2 * 8192];
        m.render(&mut warm);
        let mut buf = vec![0.0; 2 * (seconds * 44_100.0) as usize];
        m.render(&mut buf);
        buf
    }

    #[test]
    fn levels_follow_the_gain_with_an_equal_power_center() {
        let mut m = Mixer::new();
        let s = sine(1000.0, 0.5, 1.0);
        m.set_looping(&[LoopVoice { key: 1, sound: s, params: VoiceParams { gain: 0.5, pan: 0.0, pitch: 1.0 } }]);
        let buf = settled(&mut m, 0.5);
        let expected = 0.5 * 0.5 / 2f32.sqrt() * std::f32::consts::FRAC_1_SQRT_2;
        for c in 0..2 {
            let r = rms(&buf, c);
            assert!((r - expected).abs() < expected * 0.02, "channel {c}: {r} expected {expected}");
        }
    }

    #[test]
    fn panning_splits_the_power_by_angle() {
        let mut m = Mixer::new();
        let s = sine(500.0, 0.5, 1.0);
        m.set_looping(&[LoopVoice { key: 1, sound: s, params: VoiceParams { gain: 1.0, pan: 0.5, pitch: 1.0 } }]);
        let buf = settled(&mut m, 0.5);
        let ratio = rms(&buf, 0) / rms(&buf, 1);
        let a = 1.5 * std::f32::consts::FRAC_PI_4;
        assert!((ratio - a.cos() / a.sin()).abs() < 0.01, "left / right {ratio}");
        let power = rms(&buf, 0).powi(2) + rms(&buf, 1).powi(2);
        let centered = 2.0 * (0.5 / 2f32.sqrt() * std::f32::consts::FRAC_1_SQRT_2).powi(2);
        assert!((power - centered).abs() < centered * 0.03, "the total power does not depend on the pan");
    }

    #[test]
    fn pitch_changes_the_frequency() {
        let mut m = Mixer::new();
        m.set_looping(&[LoopVoice {
            key: 1,
            sound: sine(400.0, 0.5, 1.0),
            params: VoiceParams { gain: 1.0, pan: 0.0, pitch: 1.5 },
        }]);
        let buf = settled(&mut m, 1.0);
        let crossings =
            buf.chunks_exact(2).collect::<Vec<_>>().windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
        assert!((crossings as i32 - 600).abs() <= 2, "400 Hz at pitch 1.5 is 600 Hz: {crossings}");
    }

    #[test]
    fn the_room_effect_leaves_a_tail_and_captures_record_the_output() {
        let mut m = Mixer::new();
        m.set_dsp(25, 0, 1.0, 0.0);
        assert_eq!(m.dsp(), (25, 0));
        assert!(m.one_shot(sine(800.0, 0.5, 0.1), VoiceParams::default()));
        m.start_capture(1.0);
        let mut buf = vec![0.0; 2 * 22_050];
        m.render(&mut buf);
        assert!(m.capturing());
        m.render(&mut buf);
        assert!(!m.capturing());
        let rec = m.take_capture().expect("one second recorded");
        assert_eq!(rec.len(), 2 * 44_100);
        let tail = &rec[2 * 22_050..];
        assert!(rms(tail, 0) > 1e-4, "the cavern still rings after the sound ended: {}", rms(tail, 0));
        let mut dry = Mixer::new();
        assert!(dry.one_shot(sine(800.0, 0.5, 0.1), VoiceParams::default()));
        let mut a = vec![0.0; 2 * 22_050];
        dry.render(&mut a);
        dry.render(&mut a);
        assert!(rms(&a, 0) < 1e-7, "without a room effect the sound simply stops");
    }
}
