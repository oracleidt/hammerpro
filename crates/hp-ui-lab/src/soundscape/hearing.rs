use std::collections::HashMap;
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, unbounded};
use glam::DVec3;
use hp_app::AppState;
use hp_app::runtime::soundscape::{ActiveSoundscape, PreviewRequest};
use hp_audio::{LoopVoice, Sound, VoiceParams};
use hp_soundscape::library::Library;
use hp_soundscape::rules::{Emitter, TriggerList, decide};
use hp_soundscape::schedule::{Client, FADE_TIME, Fired, MAX_POSITIONS, Place, around, audible_pitch};
use hp_soundscape::{game_sounds::GameSounds, soundlevel};

use super::world::{self, Ambient, LibraryCache, Trigger};
use crate::media;

const MAX_INFLIGHT: usize = 4;
const AMBIENT_KEY: u64 = 0xA000_0000_0000_0000;

enum Slot {
    Pending,
    Ready(Arc<Sound>),
    Missing,
}

pub struct SoundCache {
    slots: HashMap<String, Slot>,
    tx: Sender<(String, Option<Arc<Sound>>)>,
    rx: Receiver<(String, Option<Arc<Sound>>)>,
    inflight: usize,
    generation: Option<u64>,
}

impl Default for SoundCache {
    fn default() -> Self {
        let (tx, rx) = unbounded();
        Self { slots: HashMap::new(), tx, rx, inflight: 0, generation: None }
    }
}

impl SoundCache {
    fn poll(&mut self, state: &AppState) {
        let generation = state.assets.generation();
        if self.generation != Some(generation) {
            self.slots.retain(|_, s| matches!(s, Slot::Pending));
            self.generation = Some(generation);
        }
        while let Ok((path, sound)) = self.rx.try_recv() {
            self.inflight = self.inflight.saturating_sub(1);
            self.slots.insert(path, sound.map_or(Slot::Missing, Slot::Ready));
        }
    }

    pub fn get(&mut self, state: &AppState, path: &str) -> Option<Arc<Sound>> {
        let key = path.to_ascii_lowercase();
        match self.slots.get(&key) {
            Some(Slot::Ready(s)) => return Some(Arc::clone(s)),
            Some(Slot::Pending | Slot::Missing) => return None,
            None => {}
        }
        if self.inflight >= MAX_INFLIGHT {
            return None;
        }
        let Some(bytes) = media::read_game(state, &key) else {
            self.slots.insert(key, Slot::Missing);
            return None;
        };
        self.slots.insert(key.clone(), Slot::Pending);
        self.inflight += 1;
        let tx = self.tx.clone();
        let spawned = std::thread::Builder::new().name("hp-soundscape-decode".into()).spawn(move || {
            let sound = hp_audio::decode(&bytes, &key).ok().map(Arc::new);
            let _ = tx.send((key, sound));
        });
        if spawned.is_err() {
            self.inflight = self.inflight.saturating_sub(1);
        }
        None
    }

    pub fn is_missing(&self, path: &str) -> bool {
        matches!(self.slots.get(&path.to_ascii_lowercase()), Some(Slot::Missing))
    }

    pub fn preload(&mut self, path: &str, sound: Arc<Sound>) {
        self.slots.insert(path.to_ascii_lowercase(), Slot::Ready(sound));
    }

    pub fn loaded(&self, path: &str) -> Option<Arc<Sound>> {
        match self.slots.get(&path.to_ascii_lowercase()) {
            Some(Slot::Ready(s)) => Some(Arc::clone(s)),
            _ => None,
        }
    }
}

pub trait Sink: Send {
    fn loops(&mut self, voices: &[LoopVoice]);
    fn fire(&mut self, sound: Arc<Sound>, params: VoiceParams) -> bool;
    fn master(&mut self, volume: f32, mute: bool);
    fn dsp(&mut self, room: i32, player: i32, effect_volume: f32, fade_seconds: f32);
    fn stop(&mut self);
    fn capture(&mut self, seconds: f32);
    fn take_capture(&mut self) -> Option<Vec<f32>>;
    fn available(&self) -> bool;
}

pub struct DeviceSink;

impl Sink for DeviceSink {
    fn loops(&mut self, voices: &[LoopVoice]) {
        hp_audio::mixer::ambience_set(voices);
    }
    fn fire(&mut self, sound: Arc<Sound>, params: VoiceParams) -> bool {
        hp_audio::mixer::ambience_fire(sound, params)
    }
    fn master(&mut self, volume: f32, mute: bool) {
        hp_audio::mixer::ambience_master(volume, mute);
    }
    fn dsp(&mut self, room: i32, player: i32, effect_volume: f32, fade_seconds: f32) {
        hp_audio::mixer::ambience_dsp(room, player, effect_volume, fade_seconds);
    }
    fn stop(&mut self) {
        hp_audio::mixer::ambience_stop();
    }
    fn capture(&mut self, seconds: f32) {
        hp_audio::mixer::ambience_capture(seconds);
    }
    fn take_capture(&mut self) -> Option<Vec<f32>> {
        hp_audio::mixer::ambience_take_capture()
    }
    fn available(&self) -> bool {
        hp_audio::mixer::ambience_available()
    }
}

pub struct MixerSink(pub Arc<std::sync::Mutex<hp_audio::mixer::Mixer>>);

impl MixerSink {
    fn with<R>(&self, f: impl FnOnce(&mut hp_audio::mixer::Mixer) -> R) -> R {
        let mut m = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut m)
    }
}

impl Sink for MixerSink {
    fn loops(&mut self, voices: &[LoopVoice]) {
        self.with(|m| m.set_looping(voices));
    }
    fn fire(&mut self, sound: Arc<Sound>, params: VoiceParams) -> bool {
        self.with(|m| m.one_shot(sound, params))
    }
    fn master(&mut self, volume: f32, mute: bool) {
        self.with(|m| m.set_master(volume, mute));
    }
    fn dsp(&mut self, room: i32, player: i32, effect_volume: f32, fade_seconds: f32) {
        self.with(|m| m.set_dsp(room, player, effect_volume, fade_seconds));
    }
    fn stop(&mut self) {
        self.with(hp_audio::mixer::Mixer::clear);
    }
    fn capture(&mut self, seconds: f32) {
        self.with(|m| m.start_capture(seconds));
    }
    fn take_capture(&mut self) -> Option<Vec<f32>> {
        self.with(hp_audio::mixer::Mixer::take_capture)
    }
    fn available(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AmbientState {
    on: bool,
    volume: f32,
    pitch: f32,
    fade: f32,
    fade_rate: f32,
}

#[derive(Clone, Debug, PartialEq)]
enum Mode {
    Off,
    Hear,
    Preview(String),
}

pub struct Hearing {
    pub sounds: SoundCache,
    library: LibraryCache,
    emitters: Option<(u64, Arc<Vec<Emitter>>)>,
    ambients: Option<(u64, Arc<Vec<Ambient>>)>,
    triggers: Option<(u64, Arc<Vec<Trigger>>)>,
    game_sounds: Option<(u64, Arc<GameSounds>)>,
    client: Client,
    server: Option<u32>,
    params: Option<(u32, String)>,
    by_trigger: bool,
    touch: TriggerList,
    mode: Mode,
    preview_volume: f32,
    sink: Box<dyn Sink>,
    recording: Option<(std::path::PathBuf, f32)>,
    ambient_state: HashMap<u32, AmbientState>,
    enabled: HashMap<u32, bool>,
    spawned: bool,
}

impl Default for Hearing {
    fn default() -> Self {
        Self::with_sink(Box::new(DeviceSink))
    }
}

pub fn pan_of(listener: DVec3, yaw_deg: f64, at: DVec3) -> f32 {
    let d = at - listener;
    let flat = glam::DVec2::new(d.x, d.y);
    let len = flat.length();
    if len < 1.0 {
        return 0.0;
    }
    let yaw = yaw_deg.to_radians();
    let right = glam::DVec2::new(yaw.sin(), -yaw.cos());
    (flat.dot(right) / len).clamp(-1.0, 1.0) as f32
}

fn rms(buf: &[f32], channel: usize) -> f32 {
    let n = (buf.len() / 2).max(1);
    (buf.iter().skip(channel).step_by(2).map(|s| s * s).sum::<f32>() / n as f32).sqrt()
}

type Listener = Option<(DVec3, f64)>;

impl Hearing {
    pub fn with_sink(sink: Box<dyn Sink>) -> Self {
        Self {
            sounds: SoundCache::default(),
            library: LibraryCache::default(),
            emitters: None,
            ambients: None,
            triggers: None,
            game_sounds: None,
            client: Client::new(0x50_55_4e_44),
            server: None,
            params: None,
            by_trigger: false,
            touch: TriggerList::default(),
            mode: Mode::Off,
            preview_volume: 1.0,
            sink,
            recording: None,
            ambient_state: HashMap::new(),
            enabled: HashMap::new(),
            spawned: false,
        }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    fn emitters(&mut self, state: &AppState) -> Arc<Vec<Emitter>> {
        let rev = state.doc.revision();
        match &self.emitters {
            Some((r, e)) if *r == rev => Arc::clone(e),
            _ => {
                let e = Arc::new(world::collect_emitters(state));
                self.emitters = Some((rev, Arc::clone(&e)));
                e
            }
        }
    }

    fn triggers(&mut self, state: &AppState) -> Arc<Vec<Trigger>> {
        let rev = state.doc.revision();
        match &self.triggers {
            Some((r, t)) if *r == rev => Arc::clone(t),
            _ => {
                let t = Arc::new(world::collect_triggers(state));
                self.triggers = Some((rev, Arc::clone(&t)));
                t
            }
        }
    }

    fn ambients(&mut self, state: &AppState) -> Arc<Vec<Ambient>> {
        let rev = state.doc.revision();
        match &self.ambients {
            Some((r, a)) if *r == rev => Arc::clone(a),
            _ => {
                let a = Arc::new(world::collect_ambients(state));
                self.ambients = Some((rev, Arc::clone(&a)));
                a
            }
        }
    }

    fn game_sounds(&mut self, state: &AppState) -> Arc<GameSounds> {
        let generation = state.assets.generation();
        match &self.game_sounds {
            Some((g, s)) if *g == generation => Arc::clone(s),
            _ => {
                let s = Arc::new(world::game_sounds(state));
                self.game_sounds = Some((generation, Arc::clone(&s)));
                s
            }
        }
    }

    fn stop(&mut self, state: &mut AppState) {
        if self.mode != Mode::Off || self.client.playing.is_some() {
            self.sink.stop();
            self.sink.dsp(0, 0, 1.0, 0.0);
        }
        self.mode = Mode::Off;
        self.client.clear();
        self.server = None;
        self.params = None;
        self.by_trigger = false;
        self.touch.reset();
        self.ambient_state.clear();
        self.enabled.clear();
        self.spawned = false;
        let rt = &mut state.runtime.lab.soundscape;
        rt.active = None;
        rt.hearing_voices = 0;
        rt.hearing_ambient = 0;
        rt.preview = None;
        rt.dsp = (0, 0);
        rt.triggers.clear();
    }

    pub fn tick(&mut self, state: &mut AppState, dt: f32) {
        self.sounds.poll(state);
        self.record(state);
        let inputs = std::mem::take(&mut state.runtime.lab.soundscape.io);
        let settings = state.settings.lab.soundscape;
        self.sink.master(settings.volume.clamp(0.0, 1.0), settings.mute);
        match state.runtime.lab.soundscape.preview_request.take() {
            Some(PreviewRequest::Play { name, volume }) => {
                self.stop(state);
                self.preview_volume = volume.clamp(0.0, 1.0);
                self.mode = Mode::Preview(name.clone());
                state.runtime.lab.soundscape.preview = Some(name);
            }
            Some(PreviewRequest::Stop) => {
                if matches!(self.mode, Mode::Preview(_)) {
                    self.stop(state);
                }
            }
            None => {}
        }
        let hear = settings.hearing && state.runtime.lab.player.active;
        let wanted = match &self.mode {
            Mode::Preview(n) => Mode::Preview(n.clone()),
            _ if hear => Mode::Hear,
            _ => Mode::Off,
        };
        if wanted == Mode::Off {
            if self.mode != Mode::Off {
                self.stop(state);
            }
            return;
        }
        if wanted == Mode::Hear && self.mode != Mode::Hear {
            self.stop(state);
        }
        self.mode = wanted;
        world::refresh_props(state);
        let library = self.library.get(state);
        let dt = f64::from(dt.clamp(0.0, 1.0));
        match self.mode.clone() {
            Mode::Preview(name) => self.preview(state, &library, &name, dt),
            Mode::Hear => {
                self.inputs(state, &inputs);
                self.hear(state, &library, dt);
            }
            Mode::Off => {}
        }
    }

    fn spawn_ambients(&mut self, state: &mut AppState) {
        if self.spawned {
            return;
        }
        self.spawned = true;
        let ambients = self.ambients(state);
        for a in ambients.iter() {
            let on = !a.start_silent;
            self.ambient_state
                .insert(a.id, AmbientState { on, volume: a.volume, pitch: a.pitch, fade: 1.0, fade_rate: 0.0 });
            if on && !a.looped {
                self.play_once(state, a);
            }
        }
    }

    fn play_once(&mut self, state: &AppState, a: &Ambient) {
        let sounds = self.game_sounds(state);
        let Some(path) = world::ambient_wave(&a.message, &sounds) else { return };
        let Some(sound) = self.sounds.get(state, &path) else { return };
        let st = self.ambient_state.get(&a.id).copied();
        let volume = st.map_or(a.volume, |s| s.volume);
        let pitch = st.map_or(a.pitch, |s| s.pitch);
        let p = state.runtime.lab.player.clone();
        let ear = hp_ui_viewports::player::eye(&p);
        let at = DVec3::from_array(a.origin);
        let gain = soundlevel::gain(volume, a.soundlevel, ear.distance(at) as f32);
        let pan = if a.everywhere { 0.0 } else { pan_of(ear, state.cameras.cam3d.yaw_deg, at) };
        if gain > 0.0 && pitch >= 1.0 {
            self.sink.fire(sound, VoiceParams { gain, pan, pitch: pitch / 100.0 });
        }
    }

    fn inputs(&mut self, state: &mut AppState, inputs: &[(u32, String, String)]) {
        self.spawn_ambients(state);
        let ambients = self.ambients(state);
        for (id, input, param) in inputs {
            let value = param.trim().parse::<f32>().ok().filter(|v| v.is_finite());
            if let Some(a) = ambients.iter().find(|a| a.id == *id) {
                let mut st = self.ambient_state.get(id).copied().unwrap_or(AmbientState {
                    on: false,
                    volume: a.volume,
                    pitch: a.pitch,
                    fade: 1.0,
                    fade_rate: 0.0,
                });
                let start = match input.as_str() {
                    "playsound" => {
                        st.on = true;
                        true
                    }
                    "stopsound" => {
                        st.on = false;
                        false
                    }
                    "togglesound" => {
                        st.on = !st.on;
                        st.on
                    }
                    "volume" => {
                        if let Some(v) = value {
                            st.volume = (v / 10.0).clamp(0.0, 1.0);
                        }
                        false
                    }
                    "pitch" => {
                        if let Some(v) = value {
                            st.pitch = v.clamp(1.0, 255.0);
                        }
                        false
                    }
                    "fadein" => {
                        st.on = true;
                        st.fade = 0.0;
                        st.fade_rate = 1.0 / value.unwrap_or(0.0).max(0.01);
                        true
                    }
                    "fadeout" => {
                        st.fade_rate = -1.0 / value.unwrap_or(0.0).max(0.01);
                        false
                    }
                    _ => false,
                };
                self.ambient_state.insert(*id, st);
                if start && !a.looped {
                    self.play_once(state, a);
                    if let Some(s) = self.ambient_state.get_mut(id) {
                        s.on = false;
                    }
                }
            } else {
                let current = self.enabled.get(id).copied();
                let base = self
                    .emitters
                    .as_ref()
                    .and_then(|(_, e)| e.iter().find(|e| e.id == *id).map(|e| e.enabled))
                    .unwrap_or(true);
                let now = current.unwrap_or(base);
                let next = match input.as_str() {
                    "enable" => true,
                    "disable" => false,
                    "toggleenabled" => !now,
                    _ => now,
                };
                self.enabled.insert(*id, next);
            }
        }
    }

    fn record(&mut self, state: &mut AppState) {
        use hp_app::runtime::soundscape::RecordState;
        if let Some(req) = state.runtime.lab.soundscape.record_request.take() {
            self.sink.capture(req.seconds);
            self.recording = Some((req.path.clone(), req.seconds));
            state.runtime.lab.soundscape.record = Some(RecordState::Recording { path: req.path, seconds: req.seconds });
        }
        let Some((path, seconds)) = self.recording.clone() else { return };
        let Some(samples) = self.sink.take_capture() else { return };
        self.recording = None;
        let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let levels = [rms(&samples, 0), rms(&samples, 1)];
        let sound = Sound { rate: hp_audio::mixer::MIX_RATE, channels: 2, samples, looped: false, loop_start: 0 };
        let bytes = hp_audio::encode_wav(&sound);
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, &bytes))
            .map_err(|e| format!("{}: {e}", path.display()));
        state.runtime.lab.soundscape.record = Some(match result {
            Ok(()) => RecordState::Written { path, seconds, peak, rms: levels },
            Err(e) => RecordState::Failed(e),
        });
    }

    fn restart_if_changed(&mut self, library: &Library, emitters: &[Emitter]) {
        let found = self.server.and_then(|id| emitters.iter().find(|e| e.id == id));
        let now = found.map(|e| (e.id, e.soundscape.to_ascii_lowercase()));
        if now == self.params {
            return;
        }
        self.params.clone_from(&now);
        let Some(e) = found else { return };
        let Some(entry) = library.get(&e.soundscape) else { return };
        let name = entry.block.name.trim().to_owned();
        let flat = library.flatten(&name);
        let mut positions: Vec<Option<[f64; 3]>> = vec![None; MAX_POSITIONS];
        for (i, _, at) in &e.positions {
            if let Some(slot) = positions.get_mut(usize::from(*i)) {
                *slot = *at;
            }
        }
        self.client.start(e.id, &name, &flat, &positions);
    }

    fn hear(&mut self, state: &mut AppState, library: &Library, dt: f64) {
        let p = state.runtime.lab.player.clone();
        let ear = hp_ui_viewports::player::eye(&p);
        let yaw = state.cameras.cam3d.yaw_deg;
        self.spawn_ambients(state);
        let mut emitters = self.emitters(state);
        if !self.enabled.is_empty() {
            let mut list = (*emitters).clone();
            for e in &mut list {
                if let Some(on) = self.enabled.get(&e.id) {
                    e.enabled = *on;
                }
            }
            emitters = Arc::new(list);
        }
        let triggers = self.triggers(state);
        let (min, max) = world::player_box(p.feet, p.crouched);
        let now_touching = world::touching(&triggers, min, max);
        let before = self.server;
        self.server = self.touch.step(&world::trigger_links(&triggers), &now_touching, self.server);
        if self.server != before {
            self.by_trigger = self.server.is_some();
        }
        let previous = self.server.and_then(|id| emitters.iter().position(|e| e.id == id));
        let sight = world::sight_fn(state);
        let d = decide(&emitters, ear.to_array(), previous, |e, to| {
            sight(DVec3::from_array(e.origin), DVec3::from_array(to))
        });
        drop(sight);
        let chosen = d.active.and_then(|i| emitters.get(i)).map(|e| e.id);
        if chosen != self.server {
            self.by_trigger = false;
        }
        self.server = chosen;
        self.restart_if_changed(library, &emitters);
        let fired = self.client.update(dt, FADE_TIME);
        let dsp = self.client.dsp;
        self.sink.dsp(dsp.room.unwrap_or(0), dsp.player.unwrap_or(0), dsp.volume.unwrap_or(1.0), FADE_TIME as f32);
        let mut voices = Vec::new();
        self.loop_voices(state, Some((ear, yaw)), 1.0, &mut voices);
        let scape_voices = voices.len();
        let mut ambient_count = 0;
        if state.settings.lab.soundscape.ambient_generic {
            let ambients = self.ambients(state);
            let sounds = self.game_sounds(state);
            for a in ambients.iter().filter(|a| a.looped) {
                let Some(st) = self.ambient_state.get_mut(&a.id) else { continue };
                if st.fade_rate != 0.0 {
                    st.fade = (st.fade + st.fade_rate * dt as f32).clamp(0.0, 1.0);
                    if st.fade <= 0.0 {
                        st.on = false;
                        st.fade = 1.0;
                        st.fade_rate = 0.0;
                    } else if st.fade >= 1.0 {
                        st.fade_rate = 0.0;
                    }
                }
                let st = *st;
                if !st.on {
                    continue;
                }
                let Some(path) = world::ambient_wave(&a.message, &sounds) else { continue };
                let at = DVec3::from_array(a.origin);
                let g = soundlevel::gain(st.volume * st.fade, a.soundlevel, ear.distance(at) as f32);
                if g < 0.002 {
                    continue;
                }
                let Some(sound) = self.sounds.get(state, &path) else { continue };
                let pan = if a.everywhere { 0.0 } else { pan_of(ear, yaw, at) };
                let params = VoiceParams { gain: g, pan, pitch: st.pitch / 100.0 };
                voices.push(LoopVoice { key: AMBIENT_KEY | u64::from(a.id), sound, params });
                ambient_count += 1;
            }
        }
        self.play_fired(state, fired, Some((ear, yaw)), 1.0);
        self.sink.loops(&voices);
        let rt = &mut state.runtime.lab.soundscape;
        rt.active = self.server.and_then(|id| emitters.iter().find(|e| e.id == id)).map(|e| ActiveSoundscape {
            entity: e.id,
            class: e.kind.class().to_owned(),
            name: e.soundscape.clone(),
            kept: d.kept_previous,
            dsp: dsp.room,
            by_trigger: self.by_trigger,
        });
        rt.hearing_voices = scape_voices;
        rt.hearing_ambient = ambient_count;
        rt.dsp = (dsp.room.unwrap_or(0), dsp.player.unwrap_or(0));
        rt.triggers = now_touching;
        rt.hearing_note = (!self.sink.available()).then(|| hp_app::t!("soundscape.hear.no_device").to_owned());
    }

    fn preview(&mut self, state: &mut AppState, library: &Library, name: &str, dt: f64) {
        if self.client.playing.is_none() {
            let flat = library.flatten(name);
            self.client.start(0, name, &flat, &vec![Some([0.0; 3]); MAX_POSITIONS]);
        }
        let fired = self.client.update(dt, FADE_TIME);
        let dsp = self.client.dsp;
        self.sink.dsp(dsp.room.unwrap_or(0), dsp.player.unwrap_or(0), dsp.volume.unwrap_or(1.0), 0.0);
        let mut voices = Vec::new();
        let volume = self.preview_volume;
        self.loop_voices(state, None, volume, &mut voices);
        self.play_fired(state, fired, None, volume);
        self.sink.loops(&voices);
        let rt = &mut state.runtime.lab.soundscape;
        rt.hearing_voices = voices.len();
        rt.dsp = (dsp.room.unwrap_or(0), dsp.player.unwrap_or(0));
    }

    fn loop_voices(&mut self, state: &AppState, listener: Listener, volume: f32, out: &mut Vec<LoopVoice>) {
        let loops = self.client.loops.clone();
        for l in loops {
            if !audible_pitch(l.pitch) {
                continue;
            }
            let (gain, pan) = match (l.ambient, listener) {
                (false, Some((ear, yaw))) => {
                    let at = DVec3::from_array(l.position);
                    let g = soundlevel::gain(l.current.min(1.0), l.soundlevel, ear.distance(at) as f32);
                    (g, pan_of(ear, yaw, at))
                }
                _ => (l.current.clamp(0.0, 1.0), 0.0),
            };
            let gain = gain * volume;
            if gain <= 0.0 {
                continue;
            }
            if let Some(sound) = self.sounds.get(state, &l.wave) {
                let params = VoiceParams { gain, pan, pitch: l.pitch as f32 / 100.0 };
                out.push(LoopVoice { key: l.key(), sound, params });
            }
        }
    }

    fn play_fired(&mut self, state: &AppState, fired: Vec<Fired>, listener: Listener, volume: f32) {
        for f in fired {
            if !audible_pitch(f.pitch) || f.volume <= 0.0 {
                continue;
            }
            let Some(sound) = self.sounds.get(state, &f.wave) else { continue };
            let (gain, pan) = match (&f.place, listener) {
                (Place::Ambient, _) => (f.volume.clamp(0.0, 1.0), 0.0),
                (Place::At(p), Some((ear, yaw))) => {
                    let at = DVec3::from_array(*p);
                    (soundlevel::gain(f.volume, f.soundlevel, ear.distance(at) as f32), pan_of(ear, yaw, at))
                }
                (Place::AroundListener(a), Some((ear, yaw))) => {
                    let at = DVec3::from_array(around(ear.to_array(), yaw, *a));
                    (soundlevel::gain(f.volume, f.soundlevel, ear.distance(at) as f32), pan_of(ear, yaw, at))
                }
                (Place::AroundListener(a), None) => (f.volume.clamp(0.0, 1.0), f64::from(*a).to_radians().cos() as f32),
                (Place::At(_), None) => (f.volume.clamp(0.0, 1.0), 0.0),
            };
            let gain = gain * volume;
            if gain > 0.0 {
                self.sink.fire(sound, VoiceParams { gain, pan, pitch: f.pitch as f32 / 100.0 });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use hp_audio::mixer::Mixer;

    fn box_solid(id: u32, min: [f64; 3], max: [f64; 3], material: &str) -> String {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let planes = [
            ([x0, y1, z1], [x1, y1, z1], [x1, y0, z1]),
            ([x0, y0, z0], [x1, y0, z0], [x1, y1, z0]),
            ([x0, y1, z1], [x0, y0, z1], [x0, y0, z0]),
            ([x1, y1, z0], [x1, y0, z0], [x1, y0, z1]),
            ([x1, y1, z1], [x0, y1, z1], [x0, y1, z0]),
            ([x1, y0, z0], [x0, y0, z0], [x0, y0, z1]),
        ];
        let mut out = format!("solid\n{{\n\"id\" \"{id}\"\n");
        for (i, (a, b, c)) in planes.iter().enumerate() {
            let f = |p: &[f64; 3]| format!("({} {} {})", p[0], p[1], p[2]);
            out.push_str(&format!(
                "side\n{{\n\"id\" \"{}\"\n\"plane\" \"{} {} {}\"\n\"material\" \"{material}\"\n\"uaxis\" \"[1 0 0 0] 0.25\"\n\"vaxis\" \"[0 -1 0 0] 0.25\"\n\"rotation\" \"0\"\n\"lightmapscale\" \"16\"\n\"smoothing_groups\" \"0\"\n}}\n",
                id * 10 + i as u32,
                f(a),
                f(b),
                f(c)
            ));
        }
        out.push_str("}\n");
        out
    }

    fn state_with(world: &str, entities: &str) -> AppState {
        let mut state = hp_app::testing::test_state();
        let text = format!(
            "versioninfo\n{{\n\"editorversion\" \"400\"\n}}\nworld\n{{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n{world}}}\n{entities}"
        );
        state.doc = hp_editor::Document::from_map(hp_vmf::parse(&text).expect("test map"));
        state
    }

    fn point(id: u32, class: &str, origin: [f64; 3], keys: &[(&str, &str)]) -> String {
        let mut out = format!(
            "entity\n{{\n\"id\" \"{id}\"\n\"classname\" \"{class}\"\n\"origin\" \"{} {} {}\"\n",
            origin[0], origin[1], origin[2]
        );
        for (k, v) in keys {
            out.push_str(&format!("\"{k}\" \"{v}\"\n"));
        }
        out.push_str("}\n");
        out
    }

    fn library(text: &str) -> Library {
        world::build_library(&[], Some("scripts/soundscapes_t.txt"), &hp_soundscape::script::parse_file(text))
    }

    fn sine(hz: f32, seconds: f32) -> Arc<Sound> {
        let n = (seconds * 44_100.0) as usize;
        let samples = (0..n).map(|i| (i as f32 * hz * std::f32::consts::TAU / 44_100.0).sin() * 0.5).collect();
        Arc::new(Sound { rate: 44_100, channels: 1, samples, looped: true, loop_start: 0 })
    }

    fn channel_rms(buf: &[f32], c: usize) -> f32 {
        rms(buf, c)
    }

    fn walk(h: &mut Hearing, state: &mut AppState, lib: &Library, feet: [f64; 3], seconds: f64) {
        state.runtime.lab.player.feet = DVec3::from_array(feet);
        let steps = (seconds / 0.05).round().max(1.0) as usize;
        for _ in 0..steps {
            h.hear(state, lib, 0.05);
        }
    }

    const SCAPES: &str = r#"
"t.a" { "dsp" "25" "playlooping" { "volume" "0.9" "pitch" "100" "wave" "a.wav" } }
"t.b" { "dsp" "1" "playlooping" { "volume" "0.6" "pitch" "100" "wave" "b.wav" } }
"t.pos" { "playlooping" { "volume" "1" "pitch" "100" "wave" "p.wav" "position" "0" "soundlevel" "SNDLVL_75dB" } }
"t.trig" { "playlooping" { "volume" "1" "pitch" "100" "wave" "t.wav" } }
"#;

    fn hearing(state: &mut AppState) -> (Hearing, Arc<Mutex<Mixer>>) {
        let mixer = Arc::new(Mutex::new(Mixer::new()));
        let mut h = Hearing::with_sink(Box::new(MixerSink(Arc::clone(&mixer))));
        for w in ["a", "b", "p", "t"] {
            h.sounds.preload(&format!("sound/{w}.wav"), sine(500.0, 1.0));
        }
        state.settings.lab.soundscape.hearing = true;
        state.runtime.lab.player.active = true;
        h.mode = Mode::Hear;
        (h, mixer)
    }

    fn render(mixer: &Arc<Mutex<Mixer>>, seconds: f32) -> Vec<f32> {
        let mut buf = vec![0.0; 2 * (seconds * 44_100.0) as usize];
        mixer.lock().unwrap().render(&mut buf);
        buf
    }

    #[test]
    fn switching_crossfades_and_sets_the_room_effect() {
        let ents = [
            point(10, "env_soundscape", [0.0, 0.0, 64.0], &[("soundscape", "t.a"), ("radius", "512")]),
            point(11, "env_soundscape", [2000.0, 0.0, 64.0], &[("soundscape", "t.b"), ("radius", "512")]),
        ]
        .concat();
        let mut state = state_with("", &ents);
        let lib = library(SCAPES);
        let (mut h, mixer) = hearing(&mut state);
        walk(&mut h, &mut state, &lib, [100.0, 0.0, 0.0], 3.0);
        assert_eq!(state.runtime.lab.soundscape.active.as_ref().map(|a| a.name.as_str()), Some("t.a"));
        assert_eq!(mixer.lock().unwrap().dsp(), (25, 0), "the room effect of the soundscape is applied");
        render(&mixer, 0.1);
        let level_a = channel_rms(&render(&mixer, 0.2), 0);
        let expected = 0.9 * 0.5 / 2f32.sqrt() * std::f32::consts::FRAC_1_SQRT_2;
        assert!(level_a > expected * 0.9, "a.wav at 0.9 after its 3 s fade: {level_a} expected about {expected}");
        walk(&mut h, &mut state, &lib, [1900.0, 0.0, 0.0], 0.05);
        assert_eq!(state.runtime.lab.soundscape.active.as_ref().map(|a| a.name.as_str()), Some("t.b"));
        let a = h.client().loops.iter().find(|l| l.wave == "sound/a.wav").map(|l| l.current).unwrap();
        let b = h.client().loops.iter().find(|l| l.wave == "sound/b.wav").map(|l| l.current).unwrap();
        assert!((a - (0.9 - 0.05 / 3.0)).abs() < 1e-4, "a fades out at 1 volume unit per 3 s: {a}");
        assert!((b - 0.05 / 3.0).abs() < 1e-4, "b fades in: {b}");
        walk(&mut h, &mut state, &lib, [1900.0, 0.0, 0.0], 1.45);
        let b = h.client().loops.iter().find(|l| l.wave == "sound/b.wav").map(|l| l.current).unwrap();
        assert!((b - 0.5).abs() < 1e-3, "1.5 s into the fade b is at 0.5: {b}");
        assert_eq!(mixer.lock().unwrap().dsp(), (1, 0));
        walk(&mut h, &mut state, &lib, [1000.0, 0.0, 0.0], 3.0);
        let act = state.runtime.lab.soundscape.active.clone().unwrap();
        assert_eq!(act.name, "t.b", "out of every radius the current soundscape stays");
        assert!(act.kept);
        assert!(h.client().loops.iter().all(|l| l.wave != "sound/a.wav"), "a.wav faded out and stopped");
    }

    #[test]
    fn positioned_sounds_attenuate_and_pan_numerically() {
        let ents = [
            point(
                10,
                "env_soundscape",
                [0.0, 0.0, 64.0],
                &[("soundscape", "t.pos"), ("radius", "-1"), ("position0", "spot")],
            ),
            point(12, "info_target", [0.0, 600.0, 64.0], &[("targetname", "spot")]),
        ]
        .concat();
        let mut state = state_with("", &ents);
        let lib = library(SCAPES);
        let (mut h, mixer) = hearing(&mut state);
        state.cameras.cam3d.yaw_deg = 0.0;
        walk(&mut h, &mut state, &lib, [0.0, 0.0, 0.0], 30.0);
        render(&mixer, 0.5);
        let buf = render(&mixer, 0.5);
        let (l, r) = (channel_rms(&buf, 0), channel_rms(&buf, 1));
        assert!(r < l * 0.02, "the spot at +Y is on the left when looking down +X: L {l} R {r}");
        let gain = soundlevel::gain(1.0, 75.0, 600.0);
        let expected = gain * 0.5 / 2f32.sqrt();
        assert!((l - expected).abs() < expected * 0.03, "distance gain at 600 units: {l} expected {expected}");
        walk(&mut h, &mut state, &lib, [0.0, -1200.0, 0.0], 0.05);
        render(&mixer, 0.5);
        let far = channel_rms(&render(&mixer, 0.5), 0);
        let expected_far = soundlevel::gain(1.0, 75.0, 1800.0) * 0.5 / 2f32.sqrt();
        assert!((far - expected_far).abs() < expected_far * 0.05, "three times further: {far} expected {expected_far}");
        assert!(far < l * 0.5);
    }

    #[test]
    fn trigger_soundscape_activates_its_triggerable_like_the_engine() {
        let trig = format!(
            "entity\n{{\n\"id\" \"20\"\n\"classname\" \"trigger_soundscape\"\n\"soundscape\" \"tss\"\n{}}}\n",
            box_solid(21, [-64.0, -64.0, 0.0], [64.0, 64.0, 128.0], "TOOLS/TOOLSTRIGGER")
        );
        let ents = [
            point(10, "env_soundscape", [3000.0, 0.0, 64.0], &[("soundscape", "t.a"), ("radius", "256")]),
            point(
                11,
                "env_soundscape_triggerable",
                [0.0, 2000.0, 64.0],
                &[("soundscape", "t.trig"), ("radius", "0"), ("targetname", "tss")],
            ),
            trig,
        ]
        .concat();
        let mut state = state_with("", &ents);
        let lib = library(SCAPES);
        let (mut h, _mixer) = hearing(&mut state);
        walk(&mut h, &mut state, &lib, [3000.0, 100.0, 0.0], 0.2);
        assert_eq!(state.runtime.lab.soundscape.active.as_ref().map(|a| a.name.as_str()), Some("t.a"));
        walk(&mut h, &mut state, &lib, [0.0, 0.0, 0.0], 0.2);
        let act = state.runtime.lab.soundscape.active.clone().unwrap();
        assert_eq!(act.name, "t.trig", "entering the trigger makes its env_soundscape_triggerable current");
        assert!(act.by_trigger);
        assert_eq!(state.runtime.lab.soundscape.triggers, [20]);
        assert_eq!(h.client().playing.as_ref().map(|p| p.1.as_str()), Some("t.trig"));
        walk(&mut h, &mut state, &lib, [500.0, 0.0, 0.0], 0.2);
        assert!(state.runtime.lab.soundscape.active.is_none(), "leaving clears the current soundscape on the server");
        assert_eq!(
            h.client().playing.as_ref().map(|p| p.1.as_str()),
            Some("t.trig"),
            "the client keeps playing until a new soundscape starts"
        );
        walk(&mut h, &mut state, &lib, [3000.0, 0.0, 0.0], 0.2);
        assert_eq!(state.runtime.lab.soundscape.active.as_ref().map(|a| a.name.as_str()), Some("t.a"));
    }

    #[test]
    fn a_soundscape_standing_on_a_floor_starts_in_solid_and_sees_nobody() {
        let floor = box_solid(2, [-256.0, -256.0, -16.0], [256.0, 256.0, 0.0], "DEV/DEV_MEASUREGENERIC01B");
        let ents = [
            point(10, "env_soundscape", [0.0, 0.0, 0.0], &[("soundscape", "t.a"), ("radius", "512")]),
            point(11, "env_soundscape", [0.0, 0.0, 1.0], &[("soundscape", "t.b"), ("radius", "512")]),
        ]
        .concat();
        let state = state_with(&floor, &ents);
        assert!(
            world::start_solid(&state, DVec3::new(0.0, 0.0, 0.0)),
            "on the top face counts as inside, like the engine"
        );
        assert!(!world::start_solid(&state, DVec3::new(0.0, 0.0, 1.0)));
        let sight = world::sight_fn(&state);
        assert!(!sight(DVec3::new(0.0, 0.0, 0.0), DVec3::new(100.0, 0.0, 64.0)));
        assert!(sight(DVec3::new(0.0, 0.0, 1.0), DVec3::new(100.0, 0.0, 64.0)));
        assert!(!sight(DVec3::new(0.0, 0.0, 64.0), DVec3::new(0.0, 0.0, -100.0)), "the floor blocks");
    }

    #[test]
    fn clips_and_triggers_do_not_block_but_water_and_glass_do() {
        assert!(!world::blocks_sight("tools/toolsplayerclip"));
        assert!(!world::blocks_sight("TOOLS/TOOLSTRIGGER"));
        assert!(!world::blocks_sight("tools/toolsblocklight"));
        assert!(world::blocks_sight("tools/toolsnodraw"));
        assert!(world::blocks_sight("nature/water_canals_water2"));
        assert!(world::blocks_sight("glass/glasswindow007a"));
    }

    #[test]
    fn panning_follows_the_view() {
        let ear = DVec3::ZERO;
        assert!(pan_of(ear, 0.0, DVec3::new(0.0, -100.0, 0.0)) > 0.99);
        assert!(pan_of(ear, 0.0, DVec3::new(0.0, 100.0, 0.0)) < -0.99);
        assert!(pan_of(ear, 90.0, DVec3::new(100.0, 0.0, 0.0)) > 0.99);
        assert_eq!(pan_of(ear, 0.0, DVec3::new(0.0, 0.0, 50.0)), 0.0);
    }

    #[test]
    fn leaving_the_player_view_stops_everything() {
        let mut state =
            state_with("", &point(10, "env_soundscape", [0.0, 0.0, 64.0], &[("soundscape", "t.a"), ("radius", "-1")]));
        let (mut h, _mixer) = hearing(&mut state);
        h.tick(&mut state, 0.05);
        state.runtime.lab.player.active = false;
        h.tick(&mut state, 0.05);
        assert!(state.runtime.lab.soundscape.active.is_none());
        assert!(h.client().playing.is_none());
    }

    #[test]
    fn recordings_are_written_as_wav() {
        let dir = std::env::temp_dir().join(format!("hp-ss-rec-{}", std::process::id()));
        let mut state = state_with("", "");
        let (mut h, mixer) = hearing(&mut state);
        let path = dir.join("rec.wav");
        state.runtime.lab.soundscape.record_request =
            Some(hp_app::runtime::soundscape::RecordRequest { path: path.clone(), seconds: 0.25 });
        h.record(&mut state);
        mixer.lock().unwrap().one_shot(sine(300.0, 0.5), VoiceParams::default());
        render(&mixer, 0.3);
        h.record(&mut state);
        match &state.runtime.lab.soundscape.record {
            Some(hp_app::runtime::soundscape::RecordState::Written { peak, rms, .. }) => {
                assert!(*peak > 0.3 && rms[0] > 0.1, "{peak} {rms:?}");
            }
            other => panic!("{other:?}"),
        }
        let back = hp_audio::decode(&std::fs::read(&path).unwrap(), "rec.wav").unwrap();
        assert_eq!((back.rate, back.channels, back.frames()), (44_100, 2, 11_025));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inputs_of_the_player_view_drive_ambient_generic_and_soundscapes() {
        let ents = [
            point(10, "env_soundscape", [0.0, 0.0, 64.0], &[("soundscape", "t.a"), ("radius", "512")]),
            point(11, "env_soundscape", [300.0, 0.0, 64.0], &[("soundscape", "t.b"), ("radius", "512")]),
            point(
                12,
                "ambient_generic",
                [0.0, 0.0, 64.0],
                &[("message", "p.wav"), ("radius", "1250"), ("health", "10"), ("spawnflags", "16")],
            ),
        ]
        .concat();
        let mut state = state_with("", &ents);
        let lib = library(SCAPES);
        let (mut h, _mixer) = hearing(&mut state);
        walk(&mut h, &mut state, &lib, [50.0, 0.0, 0.0], 0.1);
        assert_eq!(state.runtime.lab.soundscape.active.as_ref().map(|a| a.entity), Some(10));
        assert_eq!(state.runtime.lab.soundscape.hearing_ambient, 0, "Start Silent");
        h.inputs(&mut state, &[(12, "playsound".to_owned(), String::new()), (10, "disable".to_owned(), String::new())]);
        walk(&mut h, &mut state, &lib, [50.0, 0.0, 0.0], 0.1);
        assert_eq!(state.runtime.lab.soundscape.hearing_ambient, 1, "PlaySound starts the ambient_generic");
        assert_eq!(
            state.runtime.lab.soundscape.active.as_ref().map(|a| a.entity),
            Some(11),
            "the disabled soundscape lets the other one in"
        );
        h.inputs(
            &mut state,
            &[(12, "stopsound".to_owned(), String::new()), (10, "toggleenabled".to_owned(), String::new())],
        );
        walk(&mut h, &mut state, &lib, [50.0, 0.0, 0.0], 0.1);
        assert_eq!(state.runtime.lab.soundscape.hearing_ambient, 0);
        assert_eq!(state.runtime.lab.soundscape.active.as_ref().map(|a| a.entity), Some(10));
    }
}
