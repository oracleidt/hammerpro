use crate::Sound;

pub fn play(sound: &Sound, from: f64, to: Option<f64>, looped: bool, volume: f32) -> bool {
    imp::play(sound, from, to, looped, volume)
}

pub fn stop() {
    imp::stop();
}

pub fn is_playing() -> bool {
    imp::is_playing()
}

pub fn position() -> Option<f64> {
    imp::position()
}

#[cfg(windows)]
mod imp {
    use std::num::NonZero;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use rodio::Source;

    use crate::Sound;

    enum Cmd {
        Play { rate: u32, channels: u16, samples: Vec<f32>, looped: bool, volume: f32 },
        Stop,
    }

    static SENDER: OnceLock<Option<Mutex<Sender<Cmd>>>> = OnceLock::new();
    static PLAYING: AtomicBool = AtomicBool::new(false);
    static CLOCK: Mutex<Option<Clock>> = Mutex::new(None);

    struct Clock {
        began: Instant,
        from: f64,
        length: f64,
        looped: bool,
    }

    fn send(cmd: Cmd) -> bool {
        let sender = SENDER.get_or_init(|| {
            let (tx, rx) = channel();
            match std::thread::Builder::new().name("hp-sound-editor".to_owned()).spawn(move || run(rx)) {
                Ok(_) => Some(Mutex::new(tx)),
                Err(e) => {
                    log::warn!("could not start the sound editor playback thread: {e}");
                    None
                }
            }
        });
        sender.as_ref().and_then(|s| s.lock().ok()).is_some_and(|s| s.send(cmd).is_ok())
    }

    pub(super) fn play(sound: &Sound, from: f64, to: Option<f64>, looped: bool, volume: f32) -> bool {
        let ch = usize::from(sound.channels.max(1));
        let frames = sound.frames();
        let rate = f64::from(sound.rate.max(1));
        let a = ((from.max(0.0) * rate) as usize).min(frames);
        let b = to.map_or(frames, |t| ((t.max(0.0) * rate) as usize).min(frames)).max(a);
        if a == b || sound.channels == 0 || sound.rate == 0 {
            return false;
        }
        let samples = sound.samples[a * ch..b * ch].to_vec();
        if let Ok(mut clock) = CLOCK.lock() {
            *clock =
                Some(Clock { began: Instant::now(), from: a as f64 / rate, length: (b - a) as f64 / rate, looped });
        }
        let volume = if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 1.0 };
        PLAYING.store(true, Ordering::Relaxed);
        let ok = send(Cmd::Play { rate: sound.rate, channels: sound.channels, samples, looped, volume });
        if !ok {
            PLAYING.store(false, Ordering::Relaxed);
        }
        ok
    }

    pub(super) fn stop() {
        if SENDER.get().is_some() {
            send(Cmd::Stop);
        }
        PLAYING.store(false, Ordering::Relaxed);
    }

    pub(super) fn is_playing() -> bool {
        PLAYING.load(Ordering::Relaxed)
    }

    pub(super) fn position() -> Option<f64> {
        if !is_playing() {
            return None;
        }
        let clock = CLOCK.lock().ok()?;
        let c = clock.as_ref()?;
        let t = c.began.elapsed().as_secs_f64();
        let t = if c.looped && c.length > 0.0 { t % c.length } else { t.min(c.length) };
        Some(c.from + t)
    }

    const END_CHECK: Duration = Duration::from_millis(50);
    const RELEASE_DEVICE_AFTER: Duration = Duration::from_secs(10);

    pub(super) fn wait_for(playing: bool, device_open: bool, idle_since: Option<Instant>) -> Option<Duration> {
        if playing {
            return Some(END_CHECK);
        }
        if !device_open {
            return None;
        }
        let idle = idle_since.map_or(Duration::ZERO, |t| t.elapsed());
        Some(RELEASE_DEVICE_AFTER.saturating_sub(idle))
    }

    fn run(rx: Receiver<Cmd>) {
        let mut output: Option<rodio::MixerDeviceSink> = None;
        let mut sink: Option<rodio::Player> = None;
        let mut idle_since: Option<Instant> = None;
        loop {
            let received = match wait_for(sink.is_some(), output.is_some(), idle_since) {
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(wait) => rx.recv_timeout(wait),
            };
            if matches!(received, Err(RecvTimeoutError::Timeout))
                && sink.is_none()
                && idle_since.is_some_and(|t| t.elapsed() >= RELEASE_DEVICE_AFTER)
            {
                output = None;
                idle_since = None;
                continue;
            }
            match received {
                Ok(Cmd::Play { rate, channels, samples, looped, volume }) => {
                    if let Some(s) = sink.take() {
                        s.stop();
                    }
                    if output.is_none() {
                        match rodio::DeviceSinkBuilder::open_default_sink() {
                            Ok(mut stream) => {
                                stream.log_on_drop(false);
                                output = Some(stream);
                            }
                            Err(e) => {
                                log::warn!("no audio output for the sound editor: {e}");
                                PLAYING.store(false, Ordering::Relaxed);
                                continue;
                            }
                        }
                    }
                    let Some(stream) = &output else { continue };
                    let (Some(ch), Some(rate)) = (NonZero::new(channels), NonZero::new(rate)) else {
                        PLAYING.store(false, Ordering::Relaxed);
                        continue;
                    };
                    let buffer = rodio::buffer::SamplesBuffer::new(ch, rate, samples);
                    let s = rodio::Player::connect_new(stream.mixer());
                    s.set_volume(volume);
                    if looped {
                        s.append(buffer.repeat_infinite());
                    } else {
                        s.append(buffer);
                    }
                    sink = Some(s);
                }
                Ok(Cmd::Stop) => {
                    if let Some(s) = sink.take() {
                        s.stop();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if sink.as_ref().is_some_and(rodio::Player::empty) {
                sink = None;
                PLAYING.store(false, Ordering::Relaxed);
            }
            if sink.is_some() {
                idle_since = None;
            } else if idle_since.is_none() {
                idle_since = Some(Instant::now());
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use crate::Sound;

    pub(super) fn play(_sound: &Sound, _from: f64, _to: Option<f64>, _looped: bool, _volume: f32) -> bool {
        false
    }

    pub(super) fn stop() {}

    pub(super) fn is_playing() -> bool {
        false
    }

    pub(super) fn position() -> Option<f64> {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::time::{Duration, Instant};

    use super::imp::wait_for;

    #[test]
    fn the_playback_thread_sleeps_when_nothing_plays() {
        assert_eq!(wait_for(true, true, None), Some(Duration::from_millis(50)));
        assert_eq!(wait_for(false, false, None), None, "no device, nothing playing: block until a command");
        let just_now = wait_for(false, true, Some(Instant::now())).unwrap();
        assert!(just_now > Duration::from_secs(9) && just_now <= Duration::from_secs(10));
        let long_ago = Instant::now().checked_sub(Duration::from_secs(30)).unwrap();
        assert_eq!(wait_for(false, true, Some(long_ago)), Some(Duration::ZERO), "the device is released");
    }
}
