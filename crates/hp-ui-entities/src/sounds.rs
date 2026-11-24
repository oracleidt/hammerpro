use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hp_app::state::AssetsApi;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GameSound {
    pub name: String,
    pub wave: String,
    pub script: String,
    pub source: Option<u32>,
}

const MANIFEST: &str = "scripts/game_sounds_manifest.txt";

pub(crate) fn manifest_files(text: &[u8]) -> Vec<String> {
    let Ok(root) = hp_kv::parse_bytes(text) else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for block in &root.children {
        for (key, value) in &block.props {
            let key = key.to_ascii_lowercase();
            if !matches!(key.as_str(), "precache_file" | "preload_file" | "autocache_file") {
                continue;
            }
            let path = value.trim().replace('\\', "/").to_ascii_lowercase();
            if !path.is_empty() && !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

const SOUND_CHARS: &[char] = &['*', '#', '@', '>', '<', '^', ')', '}', '$', '!', '?', '&', '~', '('];

pub(crate) fn clean_wave(wave: &str) -> String {
    wave.trim().trim_start_matches(SOUND_CHARS).replace('\\', "/").to_ascii_lowercase()
}

pub(crate) fn soundscript_entries(text: &[u8]) -> Vec<GameSound> {
    let Ok(root) = hp_kv::parse_bytes(text) else { return Vec::new() };
    root.children
        .iter()
        .filter(|b| !b.name.trim().is_empty())
        .map(|b| {
            let wave = b
                .get("wave")
                .or_else(|| b.child("rndwave").and_then(|r| r.get("wave")))
                .map(clean_wave)
                .unwrap_or_default();
            GameSound { name: b.name.trim().to_owned(), wave, script: String::new(), source: None }
        })
        .collect()
}

pub(crate) fn soundscript_block(text: &[u8], name: &str) -> Option<String> {
    let root = hp_kv::parse_bytes(text).ok()?;
    let block = root.children.iter().find(|b| b.name.trim().eq_ignore_ascii_case(name.trim()))?;
    let mut out = String::new();
    write_block(block, 0, &mut out);
    Some(out)
}

fn write_block(block: &hp_kv::Block, depth: usize, out: &mut String) {
    let pad = "    ".repeat(depth);
    out.push_str(&format!("{pad}\"{}\"\n{pad}{{\n", block.name));
    for (k, v) in &block.props {
        out.push_str(&format!("{pad}    \"{k}\" \"{v}\"\n"));
    }
    for child in &block.children {
        if depth < 8 {
            write_block(child, depth + 1, out);
        }
    }
    out.push_str(&format!("{pad}}}\n"));
}

pub(crate) fn load_game_sounds(fs: &hp_fs::GameFs) -> Vec<GameSound> {
    let Some(manifest) = fs.read(MANIFEST) else { return Vec::new() };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for file in manifest_files(&manifest) {
        let Some(bytes) = fs.read(&file) else { continue };
        for mut sound in soundscript_entries(&bytes) {
            if seen.insert(sound.name.to_ascii_lowercase()) {
                sound.source =
                    (!sound.wave.is_empty()).then(|| fs.which_source(&format!("sound/{}", sound.wave))).flatten();
                sound.script.clone_from(&file);
                out.push(sound);
            }
        }
    }
    out.sort_by(|a, b| crate::names::cmp_ignore_case(&a.name, &b.name));
    out
}

struct Catalog {
    generation: u64,
    sounds: Mutex<Option<Arc<Vec<GameSound>>>>,
}

fn catalog_id() -> egui::Id {
    egui::Id::unique("hp_ui_entities.game_sounds")
}

pub(crate) fn reload_game_sounds(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Arc<Catalog>>(catalog_id()));
}

pub(crate) fn game_sounds(ctx: &egui::Context, assets: &dyn AssetsApi) -> Option<Arc<Vec<GameSound>>> {
    let fs = assets.fs()?;
    let generation = assets.generation();
    let current = ctx.data(|d| d.get_temp::<Arc<Catalog>>(catalog_id())).filter(|c| c.generation == generation);
    let catalog = match current {
        Some(c) => c,
        None => {
            let catalog = Arc::new(Catalog { generation, sounds: Mutex::new(None) });
            ctx.data_mut(|d| d.insert_temp(catalog_id(), Arc::clone(&catalog)));
            let (fs, slot, repaint) = (fs.clone(), Arc::clone(&catalog), ctx.clone());
            let spawned = std::thread::Builder::new().name("hp-game-sounds".to_owned()).spawn(move || {
                let list = Arc::new(load_game_sounds(&fs));
                if let Ok(mut s) = slot.sounds.lock() {
                    *s = Some(list);
                }
                repaint.request_repaint();
            });
            if let Err(e) = spawned {
                log::warn!("could not start the game sounds thread: {e}");
                if let Ok(mut s) = catalog.sounds.lock() {
                    *s = Some(Arc::new(Vec::new()));
                }
            }
            catalog
        }
    };
    let sounds = catalog.sounds.lock().ok().and_then(|s| s.clone());
    if sounds.is_none() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    sounds
}

pub(crate) fn is_sound_file(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    [".wav", ".mp3", ".ogg"].iter().any(|e| v.ends_with(e))
}

pub(crate) fn preview_file(value: &str, game: &[GameSound]) -> Option<String> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    if is_sound_file(v) {
        return Some(clean_wave(v));
    }
    game.iter().find(|s| s.name.eq_ignore_ascii_case(v)).map(|s| s.wave.clone()).filter(|w| !w.is_empty())
}

pub(crate) fn can_preview(file: &str) -> bool {
    cfg!(windows) && is_sound_file(file)
}

pub(crate) const MAX_PREVIEW_BYTES: usize = 64 << 20;

static NOW_PLAYING: Mutex<Option<(String, u64)>> = Mutex::new(None);

static PLAY_COUNT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn is_playing(value: &str) -> bool {
    NOW_PLAYING.lock().ok().and_then(|n| n.as_ref().map(|(v, _)| v.eq_ignore_ascii_case(value.trim()))).unwrap_or(false)
}

pub(crate) fn now_playing() -> Option<String> {
    NOW_PLAYING.lock().ok().and_then(|n| n.as_ref().map(|(v, _)| v.clone()))
}

fn finished(play: u64) {
    if let Ok(mut now) = NOW_PLAYING.lock()
        && now.as_ref().is_some_and(|(_, p)| *p == play)
    {
        *now = None;
    }
}

pub(crate) fn play(assets: &dyn AssetsApi, value: &str, game: &[GameSound], volume: f32) -> bool {
    let Some(file) = preview_file(value, game).filter(|f| can_preview(f)) else { return false };
    let Some(fs) = assets.fs() else { return false };
    let path = format!("sound/{file}");
    if !fs.exists(&path) {
        return false;
    }
    let play = PLAY_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if let Ok(mut now) = NOW_PLAYING.lock() {
        *now = Some((value.trim().to_owned(), play));
    }
    let started = player::play(fs.clone(), path, play, volume);
    if !started {
        finished(play);
    }
    started
}

pub(crate) fn set_volume(volume: f32) {
    player::set_volume(volume);
}

pub(crate) fn stop() {
    player::stop();
    if let Ok(mut now) = NOW_PLAYING.lock() {
        *now = None;
    }
}

fn clamp_volume(v: f32) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 }
}

#[cfg(windows)]
mod player {
    use std::io::Cursor;
    use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    enum Cmd {
        Play { fs: hp_fs::GameFs, path: String, play: u64, volume: f32 },
        Volume(f32),
        Stop,
    }

    static SENDER: OnceLock<Option<Mutex<Sender<Cmd>>>> = OnceLock::new();

    fn send(cmd: Cmd) -> bool {
        let sender = SENDER.get_or_init(|| {
            let (tx, rx) = channel();
            match std::thread::Builder::new().name("hp-sound-preview".to_owned()).spawn(move || run(rx)) {
                Ok(_) => Some(Mutex::new(tx)),
                Err(e) => {
                    log::warn!("could not start the sound preview thread: {e}");
                    None
                }
            }
        });
        sender.as_ref().and_then(|s| s.lock().ok()).is_some_and(|s| s.send(cmd).is_ok())
    }

    pub(super) fn play(fs: hp_fs::GameFs, path: String, play: u64, volume: f32) -> bool {
        send(Cmd::Play { fs, path, play, volume })
    }

    pub(super) fn set_volume(volume: f32) {
        if SENDER.get().is_some() {
            send(Cmd::Volume(volume));
        }
    }

    pub(super) fn stop() {
        if SENDER.get().is_some() {
            send(Cmd::Stop);
        }
    }

    const END_CHECK: Duration = Duration::from_millis(100);
    const RELEASE_DEVICE_AFTER: Duration = Duration::from_secs(10);

    pub(super) fn wait_for(playing: bool, idle: Option<Duration>) -> Option<Duration> {
        if playing {
            return Some(END_CHECK);
        }
        idle.map(|idle| RELEASE_DEVICE_AFTER.saturating_sub(idle))
    }

    fn run(rx: Receiver<Cmd>) {
        let mut output: Option<rodio::MixerDeviceSink> = None;
        let mut sink: Option<(rodio::Player, u64)> = None;
        let mut idle_since: Option<Instant> = None;
        loop {
            let idle = output.as_ref().map(|_| idle_since.get_or_insert_with(Instant::now).elapsed());
            let received = match wait_for(sink.is_some(), idle) {
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
            if received.is_ok() {
                idle_since = None;
            }
            match received {
                Ok(Cmd::Play { fs, path, play, volume }) => {
                    if let Some((s, _)) = sink.take() {
                        s.stop();
                    }
                    let Some(bytes) = fs.read(&path).filter(|b| b.len() <= super::MAX_PREVIEW_BYTES) else {
                        super::finished(play);
                        continue;
                    };
                    if output.is_none() {
                        match rodio::DeviceSinkBuilder::open_default_sink() {
                            Ok(mut stream) => {
                                stream.log_on_drop(false);
                                output = Some(stream);
                            }
                            Err(e) => {
                                log::warn!("no audio output for sound previews: {e}");
                                super::finished(play);
                                continue;
                            }
                        }
                    }
                    let Some(stream) = &output else { continue };
                    let len = bytes.len() as u64;
                    let hint = path.rsplit('.').next().unwrap_or("").to_owned();
                    let decoder = rodio::Decoder::builder()
                        .with_data(Cursor::new(bytes))
                        .with_byte_len(len)
                        .with_hint(&hint)
                        .with_seekable(true)
                        .build();
                    match decoder {
                        Ok(source) => {
                            let s = rodio::Player::connect_new(stream.mixer());
                            s.set_volume(super::clamp_volume(volume));
                            s.append(source);
                            sink = Some((s, play));
                        }
                        Err(e) => {
                            log::warn!("cannot play {path}: {e}");
                            super::finished(play);
                        }
                    }
                }
                Ok(Cmd::Volume(v)) => {
                    if let Some((s, _)) = &sink {
                        s.set_volume(super::clamp_volume(v));
                    }
                }
                Ok(Cmd::Stop) => {
                    if let Some((s, _)) = sink.take() {
                        s.stop();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if let Some((s, play)) = &sink
                && s.empty()
            {
                super::finished(*play);
                sink = None;
                idle_since = Some(Instant::now());
            }
        }
    }
}

#[cfg(not(windows))]
mod player {
    pub(super) fn play(_fs: hp_fs::GameFs, _path: String, _play: u64, _volume: f32) -> bool {
        false
    }

    pub(super) fn set_volume(_volume: f32) {}

    pub(super) fn stop() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn preview_thread_blocks_while_nothing_plays() {
        use std::time::Duration;
        assert_eq!(player::wait_for(false, None), None, "no device open: wait for the next command");
        assert!(player::wait_for(true, Some(Duration::ZERO)).is_some_and(|w| w <= Duration::from_millis(100)));
        assert_eq!(player::wait_for(false, Some(Duration::from_secs(30))), Some(Duration::ZERO), "release the device");
        assert_eq!(player::wait_for(false, Some(Duration::from_secs(4))), Some(Duration::from_secs(6)));
    }

    const MANIFEST_TEXT: &str = r#"
        game_sounds_manifest
        {
            "precache_file" "scripts/game_sounds.txt"
            "preload_file"  "scripts\game_sounds_ambient_generic.txt"
            "precache_file" "scripts/game_sounds.txt"
            "comment" "ignored"
        }
    "#;

    const SCRIPT: &str = r#"
        "Streetwar.d3_c17_13_horn"
        {
            "channel"    "CHAN_STATIC"
            "soundlevel" "SNDLVL_140dB"
            "wave"       ")ambient/alarms/klaxon1.wav"
        }
        "Weapon_Pistol.Single"
        {
            "rndwave"
            {
                "wave" "^weapons/pistol/pistol_fire2.wav"
                "wave" "^weapons/pistol/pistol_fire3.wav"
            }
        }
        "Empty.Sound" { "channel" "CHAN_AUTO" }
    "#;

    #[test]
    fn manifest_lists_scripts_once() {
        assert_eq!(
            manifest_files(MANIFEST_TEXT.as_bytes()),
            ["scripts/game_sounds.txt", "scripts/game_sounds_ambient_generic.txt"]
        );
        assert!(manifest_files(b"").is_empty());
    }

    #[test]
    fn soundscripts_give_names_and_first_waves() {
        let sounds = soundscript_entries(SCRIPT.as_bytes());
        let names: Vec<&str> = sounds.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Streetwar.d3_c17_13_horn", "Weapon_Pistol.Single", "Empty.Sound"]);
        assert_eq!(sounds[0].wave, "ambient/alarms/klaxon1.wav");
        assert_eq!(sounds[1].wave, "weapons/pistol/pistol_fire2.wav");
        assert_eq!(sounds[2].wave, "");
    }

    #[test]
    fn preview_files_resolve_game_sounds() {
        let game = soundscript_entries(SCRIPT.as_bytes());
        assert_eq!(preview_file("streetwar.D3_c17_13_horn", &game).as_deref(), Some("ambient/alarms/klaxon1.wav"));
        assert_eq!(preview_file("ambient\\wind\\wind1.wav", &game).as_deref(), Some("ambient/wind/wind1.wav"));
        assert_eq!(preview_file("Empty.Sound", &game), None);
        assert_eq!(preview_file("Unknown.Sound", &game), None);
        assert_eq!(preview_file("  ", &game), None);
        assert!(is_sound_file("music/hl2_song1.MP3"));
        assert!(!is_sound_file("Streetwar.d3_c17_13_horn"));
        assert_eq!(can_preview("music/a.mp3"), cfg!(windows));
        assert!(!can_preview("music/a.txt"));
    }

    #[test]
    fn nothing_plays_without_content() {
        let state = hp_app::testing::test_state();
        assert!(!play(state.assets.as_ref(), "ambient/wind/wind1.wav", &[], 0.5));
        assert!(!is_playing("ambient/wind/wind1.wav"));
        assert_eq!(clamp_volume(f32::NAN), 1.0);
        assert_eq!(clamp_volume(3.0), 1.0);
        stop();
    }

    #[test]
    fn real_gmod_soundscripts() {
        let root = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod");
        if !root.join("garrysmod").is_dir() {
            return;
        }
        let opts = hp_fs::MountOptions {
            mount_addons: false,
            mount_workshop: false,
            mount_cfg: false,
            ..hp_fs::MountOptions::default()
        };
        let Ok(fs) = hp_fs::GameFs::for_gmod(root, &opts) else { return };
        let t = std::time::Instant::now();
        let sounds = load_game_sounds(&fs);
        eprintln!("{} game sounds in {:?}", sounds.len(), t.elapsed());
        assert!(sounds.len() > 1000, "{}", sounds.len());
        let klaxon: Vec<&str> =
            sounds.iter().filter(|s| s.wave.contains("klaxon")).map(|s| s.name.as_str()).take(5).collect();
        eprintln!("klaxon sounds: {klaxon:?}");
        assert!(!klaxon.is_empty());
    }
}
