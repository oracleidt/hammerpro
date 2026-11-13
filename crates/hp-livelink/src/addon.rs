use std::path::{Path, PathBuf};

use crate::exchange::write_atomic;
use crate::protocol::{DATA_DIR, INJECT_FILE};

pub const LUA: &str = include_str!("../../../assets/livelink/hammerpro_livelink.lua");
pub const ADDON: &str = "hammerpro_livelink";
pub const LUA_NAME: &str = "hammerpro_livelink.lua";

pub fn addon_dir(root: &Path) -> PathBuf {
    root.join("garrysmod").join("addons").join(ADDON)
}

pub fn lua_file(root: &Path) -> PathBuf {
    addon_dir(root).join("lua").join("autorun").join(LUA_NAME)
}

pub fn data_dir(root: &Path) -> PathBuf {
    root.join("garrysmod").join("data").join(DATA_DIR)
}

pub fn is_installed(root: &Path) -> bool {
    std::fs::read_to_string(lua_file(root)).is_ok_and(|t| t == LUA)
}

pub fn is_present(root: &Path) -> bool {
    lua_file(root).is_file()
}

pub fn install(root: &Path) -> std::io::Result<()> {
    if !root.join("garrysmod").is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "garrysmod folder not found"));
    }
    write_atomic(&lua_file(root), LUA.as_bytes())?;
    write_atomic(&data_dir(root).join(INJECT_FILE), LUA.as_bytes())
}

pub fn inject_command() -> String {
    format!("lua_run RunString(file.Read('{DATA_DIR}/{INJECT_FILE}','DATA') or '','{ADDON}')")
}

fn remove_file(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn prune(from: Option<&Path>, top: &Path) {
    let mut dir = from.map(Path::to_path_buf);
    while let Some(d) = dir {
        if !d.starts_with(top) || std::fs::remove_dir(&d).is_err() || d == top {
            break;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
}

pub fn remove(root: &Path) -> std::io::Result<()> {
    let file = lua_file(root);
    remove_file(&file)?;
    prune(file.parent(), &addon_dir(root));
    let data = data_dir(root);
    if let Ok(list) = std::fs::read_dir(&data) {
        for entry in list.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let ours = name.ends_with(".json") || name.ends_with(".txt") || name.ends_with(".tmp");
            if ours && path.is_file() {
                remove_file(&path)?;
            }
        }
    }
    let _ = std::fs::remove_dir(&data);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_livelink_addon_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("garrysmod")).expect("scratch");
        dir
    }

    #[test]
    fn install_and_remove_touch_only_link_files() {
        let root = scratch("install");
        assert!(!is_installed(&root) && !is_present(&root));
        install(&root).expect("install");
        assert!(is_installed(&root));
        assert_eq!(std::fs::read_to_string(data_dir(&root).join(INJECT_FILE)).expect("copy"), LUA);
        let user = addon_dir(&root).join("lua").join("mine.lua");
        std::fs::write(&user, "print(1)").expect("write");
        std::fs::write(lua_file(&root), "old").expect("old");
        assert!(!is_installed(&root) && is_present(&root), "an old copy is not the current addon");
        remove(&root).expect("remove");
        assert!(user.is_file());
        assert!(!data_dir(&root).exists());
        std::fs::remove_file(&user).expect("cleanup");
        install(&root).expect("again");
        remove(&root).expect("remove");
        assert!(!addon_dir(&root).exists());
        remove(&root).expect("twice");
        assert!(install(&root.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_inject_command_reads_the_data_copy() {
        let cmd = inject_command();
        assert!(cmd.starts_with("lua_run RunString(file.Read('hammerpro_livelink/addon.txt','DATA')"));
        assert!(!cmd.contains('"'), "the console strips double quotes");
    }

    #[test]
    fn the_addon_script_is_complete() {
        for needle in [
            "HAMMERPRO_LIVELINK",
            "session.json",
            "cmd.json",
            "game.json",
            "ents.json",
            "util.AddNetworkString",
            "EntityKeyValue",
            "ProjectedTexture",
            "DynamicLight",
            "ClientsideModel",
            "halo.Add",
            "point_servercommand",
            "runpassedcode",
        ] {
            assert!(LUA.contains(needle), "{needle}");
        }
        assert!(!LUA.contains("--"), "no comments in the addon");
        assert!(!LUA.contains('\u{2014}') && !LUA.contains('\u{2013}'));
    }
}
