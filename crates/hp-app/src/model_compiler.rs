use std::path::{Path, PathBuf};

use crate::settings::Settings;
use crate::{t, tf};

pub fn studiomdl_exe(gmod_root: &Path) -> Option<PathBuf> {
    let bin = gmod_root.join("bin");
    [bin.join("win64").join("studiomdl.exe"), bin.join("studiomdl.exe")].into_iter().find(|p| p.is_file())
}

pub fn custom(settings: &Settings) -> Option<&PathBuf> {
    settings.compile.model_compiler.as_ref().filter(|p| !p.as_os_str().is_empty())
}

pub fn resolve(settings: &Settings) -> Result<PathBuf, String> {
    resolve_with(settings, true)
}

pub fn resolve_configured(settings: &Settings) -> Result<PathBuf, String> {
    resolve_with(settings, false)
}

fn resolve_with(settings: &Settings, detect: bool) -> Result<PathBuf, String> {
    if let Some(custom) = custom(settings) {
        return if custom.is_file() {
            Ok(custom.clone())
        } else {
            Err(tf!("entities.mt_compiler_missing", custom.display().to_string()))
        };
    }
    settings
        .game
        .gmod_root
        .clone()
        .or_else(|| if detect { hp_fs::detect_gmod_root() } else { None })
        .as_deref()
        .and_then(studiomdl_exe)
        .ok_or_else(|| t!("entities.mt_no_studiomdl").to_owned())
}

pub fn args(game_dir: &Path, qc: &Path, extra: Vec<String>) -> Vec<String> {
    let mut args = vec!["-game".to_owned(), game_dir.to_string_lossy().into_owned(), "-nop4".to_owned()];
    if !extra.iter().any(|a| a.eq_ignore_ascii_case("-verbose")) {
        args.push("-verbose".to_owned());
    }
    args.extend(extra);
    args.push(qc.to_string_lossy().into_owned());
    args
}

pub fn name(settings: &Settings) -> String {
    match custom(settings) {
        Some(p) => p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()),
        None => "studiomdl".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_custom_compiler_wins_and_a_missing_one_is_reported() {
        let dir = std::env::temp_dir().join(format!("hp_model_compiler_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let mut s = Settings::default();
        s.game.gmod_root = Some(dir.clone());
        std::fs::write(dir.join("bin").join("studiomdl.exe"), b"x").unwrap();
        assert_eq!(resolve(&s), Ok(dir.join("bin").join("studiomdl.exe")));
        assert_eq!(name(&s), "studiomdl");
        let neko = dir.join("nekomdl.exe");
        s.compile.model_compiler = Some(neko.clone());
        assert!(resolve(&s).is_err(), "a missing custom compiler is never replaced by studiomdl");
        assert!(resolve_configured(&s).is_err());
        std::fs::write(&neko, b"x").unwrap();
        assert_eq!(resolve(&s), Ok(neko));
        assert_eq!(name(&s), "nekomdl.exe");
        s.compile.model_compiler = Some(PathBuf::new());
        assert_eq!(resolve(&s), Ok(dir.join("bin").join("studiomdl.exe")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
