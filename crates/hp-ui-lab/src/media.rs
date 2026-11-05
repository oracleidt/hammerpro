use std::path::{Path, PathBuf};

use hp_app::AppState;

pub const SOUND_PREFIX: &str = "sound";
pub const MAX_SOURCE_BYTES: usize = 128 << 20;

pub fn map_root(state: &AppState) -> Option<PathBuf> {
    let dir = state.doc.path()?.parent()?;
    if dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("maps")) {
        return dir.parent().map(Path::to_path_buf).filter(|p| p.is_dir());
    }
    None
}

pub fn default_root(state: &AppState) -> Option<PathBuf> {
    map_root(state).or_else(|| hp_ui_entities::model_import::content_root(state))
}

pub fn clean_rel(name: &str, folder: &str, ext: &str) -> Result<String, String> {
    let mut parts: Vec<&str> = Vec::new();
    let normalized = name.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Err("the file name is empty".to_owned());
    }
    if normalized.contains(':') || normalized.starts_with('/') {
        return Err(format!("\"{name}\" must be a path relative to the content folder"));
    }
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(format!("\"{name}\" must not contain \"..\"")),
            p if p.chars().any(|c| c.is_control() || "<>\"|?*".contains(c)) => {
                return Err(format!("\"{name}\" has characters a file name cannot hold"));
            }
            p => parts.push(p),
        }
    }
    let first_ok = {
        let folder_parts: Vec<&str> = folder.split('/').collect();
        parts.len() > folder_parts.len() && folder_parts.iter().zip(&parts).all(|(a, b)| a.eq_ignore_ascii_case(b))
    };
    let mut rel: Vec<String> = Vec::new();
    if !first_ok {
        rel.extend(folder.split('/').map(str::to_owned));
    }
    rel.extend(parts.iter().map(|p| (*p).to_owned()));
    let last = rel.pop().unwrap_or_default();
    let stem = match last.rfind('.') {
        Some(i) if i > 0 => &last[..i],
        _ => last.as_str(),
    };
    if stem.is_empty() {
        return Err("the file name is empty".to_owned());
    }
    rel.push(format!("{stem}.{ext}"));
    Ok(rel.join("/"))
}

pub fn is_external(src: &str) -> bool {
    let p = Path::new(src.trim());
    p.is_absolute() || src.trim().chars().nth(1) == Some(':')
}

pub fn game_sound_path(src: &str) -> String {
    let s = src.trim().replace('\\', "/");
    let s = s.trim_start_matches('/');
    if s.to_ascii_lowercase().starts_with("sound/") { s.to_owned() } else { format!("{SOUND_PREFIX}/{s}") }
}

pub fn read_source(
    state: &AppState,
    src: &str,
    game_path: impl Fn(&str) -> String,
) -> Result<(Vec<u8>, String), String> {
    let bytes = if is_external(src) {
        let path = PathBuf::from(src.trim());
        let len = std::fs::metadata(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?.len();
        if len > MAX_SOURCE_BYTES as u64 {
            return Err(format!("{} is too big to edit", path.display()));
        }
        std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?
    } else {
        let rel = game_path(src);
        if rel.contains("..") {
            return Err(format!("\"{src}\" must not contain \"..\""));
        }
        read_game(state, &rel)
            .ok_or_else(|| format!("\"{rel}\" was not found in the game content or the project folders"))?
    };
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err("the file is too big to edit".to_owned());
    }
    Ok((bytes, src.trim().to_owned()))
}

pub fn roots(state: &AppState) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: Option<PathBuf>| {
        if let Some(p) = p.filter(|p| !out.contains(p)) {
            out.push(p);
        }
    };
    push(map_root(state));
    push(state.doc.path().and_then(|p| p.parent()).map(Path::to_path_buf));
    push(hp_ui_entities::model_import::content_root(state));
    out
}

pub fn read_game(state: &AppState, rel: &str) -> Option<Vec<u8>> {
    if let Some(fs) = state.assets.fs() {
        if let Some(b) = fs.read(rel) {
            return Some(b);
        }
        if let Some(b) = fs.read(&rel.to_ascii_lowercase()) {
            return Some(b);
        }
    }
    roots(state).iter().find_map(|r| std::fs::read(r.join(rel)).ok())
}

pub fn read_lut(state: &AppState, name: &str) -> Option<Vec<u8>> {
    let name = name.trim().replace('\\', "/");
    if name.is_empty() || name.contains("..") || name.contains(':') || name.starts_with('/') {
        return None;
    }
    read_game(state, &name).filter(|b| hp_colorgrade::is_raw(b))
}

pub fn write_file(root: &Path, rel: &str, bytes: &[u8], overwrite: bool) -> Result<PathBuf, String> {
    let path = root.join(rel);
    if path.exists() && !overwrite {
        return Err(format!("{} already exists (set overwrite to replace it)", path.display()));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    std::fs::write(&path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

pub fn resolve_root(state: &AppState, requested: Option<&str>) -> Result<PathBuf, String> {
    match requested.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => {
            let p = PathBuf::from(r);
            if !p.is_absolute() {
                return Err("root must be an absolute folder path".to_owned());
            }
            if !p.is_dir() {
                return Err(format!("{} is not a folder", p.display()));
            }
            Ok(p)
        }
        None => default_root(state).ok_or_else(|| {
            "no output folder: set the game folder in the settings, save the map in a maps folder, or pass root"
                .to_owned()
        }),
    }
}

pub fn is_sound_ext(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    [".wav", ".mp3", ".ogg"].iter().any(|e| v.ends_with(e))
}

pub fn map_sounds(state: &AppState) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, node) in state.doc.scene().entities() {
        for (_, v) in &node.entity.props {
            if !is_sound_ext(v) {
                continue;
            }
            let v = v.trim().trim_start_matches(|c: char| "*#@<>^)(}{$!?&~".contains(c));
            if v.is_empty() || v.contains(':') {
                continue;
            }
            out.push(game_sound_path(v));
        }
    }
    out.sort_unstable_by_key(|s| s.to_ascii_lowercase());
    out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    out
}

pub fn pick_file(
    filter: String,
    ext: &'static [&'static str],
    start: Option<PathBuf>,
    ctx: &egui::Context,
) -> crossbeam_channel::Receiver<Option<PathBuf>> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let mut dialog = rfd::FileDialog::new().add_filter(filter, ext);
    if let Some(s) = start.filter(|s| s.is_dir()) {
        dialog = dialog.set_directory(s);
    }
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("hp-media-pick".into()).spawn(move || {
        let _ = tx.send(dialog.pick_file());
        ctx.request_repaint();
    });
    if spawned.is_err() {
        log::warn!("cannot open the file dialog");
    }
    rx
}

pub fn pick_folder(start: Option<PathBuf>, ctx: &egui::Context) -> crossbeam_channel::Receiver<Option<PathBuf>> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let mut dialog = rfd::FileDialog::new();
    if let Some(s) = start.filter(|s| s.is_dir()) {
        dialog = dialog.set_directory(s);
    }
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("hp-media-pick".into()).spawn(move || {
        let _ = tx.send(dialog.pick_folder());
        ctx.request_repaint();
    });
    if spawned.is_err() {
        log::warn!("cannot open the folder dialog");
    }
    rx
}

pub fn poll<T>(slot: &mut Option<crossbeam_channel::Receiver<T>>) -> Option<T> {
    let rx = slot.as_ref()?;
    match rx.try_recv() {
        Ok(v) => {
            *slot = None;
            Some(v)
        }
        Err(crossbeam_channel::TryRecvError::Empty) => None,
        Err(crossbeam_channel::TryRecvError::Disconnected) => {
            *slot = None;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_names_are_cleaned() {
        assert_eq!(clean_rel("boom", "sound", "wav").unwrap(), "sound/boom.wav");
        assert_eq!(clean_rel("sound/fx/boom.mp3", "sound", "wav").unwrap(), "sound/fx/boom.wav");
        assert_eq!(clean_rel("Sound\\fx\\a", "sound", "wav").unwrap(), "Sound/fx/a.wav");
        assert_eq!(
            clean_rel("cc_warm.raw", "materials/correction", "raw").unwrap(),
            "materials/correction/cc_warm.raw"
        );
        assert_eq!(
            clean_rel("materials/correction/x", "materials/correction", "raw").unwrap(),
            "materials/correction/x.raw"
        );
        assert!(clean_rel("../x", "sound", "wav").is_err());
        assert!(clean_rel("C:/x", "sound", "wav").is_err());
        assert!(clean_rel("/x", "sound", "wav").is_err());
        assert!(clean_rel("", "sound", "wav").is_err());
        assert!(clean_rel("a|b", "sound", "wav").is_err());
    }

    #[test]
    fn sound_paths() {
        assert_eq!(game_sound_path("ambient/wind.wav"), "sound/ambient/wind.wav");
        assert_eq!(game_sound_path("/sound/ambient/wind.wav"), "sound/ambient/wind.wav");
        assert!(is_external("C:\\x\\y.wav"));
        assert!(!is_external("ambient/wind.wav"));
        assert!(is_sound_ext("a/B.WAV"));
    }

    #[test]
    fn writing_respects_overwrite() {
        let dir = std::env::temp_dir().join(format!("hp-media-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = write_file(&dir, "sound/a/b.wav", b"x", false).unwrap();
        assert!(p.is_file());
        assert!(write_file(&dir, "sound/a/b.wav", b"y", false).is_err());
        write_file(&dir, "sound/a/b.wav", b"y", true).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"y");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
