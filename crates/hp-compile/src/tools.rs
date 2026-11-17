use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{CompileError, Result};

pub const BSPC_ARG: &str = "--bspc";

pub const RAD_ARG: &str = "--rad";
pub const CUBEMAPS_ARG: &str = "--cubemaps";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolPaths {
    pub compiler: PathBuf,
    pub game_exe: Option<PathBuf>,
    pub game_dir: PathBuf,
    pub maps_dir: PathBuf,
}

impl ToolPaths {
    pub fn detect(gmod_root: &Path) -> Option<ToolPaths> {
        let compiler = std::env::current_exe().ok()?;
        Self::with_compiler(compiler, gmod_root)
    }

    pub fn with_compiler(compiler: PathBuf, gmod_root: &Path) -> Option<ToolPaths> {
        let (root, game_dir) = resolve_root(gmod_root)?;
        let mut tools = Self::for_game_dir(compiler, &game_dir);
        tools.game_exe = find_game_exe(&root);
        Some(tools)
    }

    pub fn for_game_dir(compiler: PathBuf, game_dir: &Path) -> ToolPaths {
        let game_dir = strip_trailing_separators(game_dir);
        ToolPaths { compiler, game_exe: None, maps_dir: game_dir.join("maps"), game_dir }
    }

    pub fn gmod_root(&self) -> Option<PathBuf> {
        let parent = self.game_dir.parent()?;
        (!parent.as_os_str().is_empty()).then(|| parent.to_path_buf())
    }

    pub fn missing_files(&self) -> Vec<PathBuf> {
        let mut missing = Vec::new();
        if !self.compiler.is_file() {
            missing.push(self.compiler.clone());
        }
        let gameinfo = self.game_dir.join("gameinfo.txt");
        if !gameinfo.is_file() {
            missing.push(gameinfo);
        }
        missing
    }
}

pub fn default_threads() -> u32 {
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    u32::try_from(cpus.saturating_sub(1).max(1)).unwrap_or(u32::MAX)
}

pub fn pack_files_from_dir(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let io_err = |source| CompileError::Io { path: dir.clone(), source };
        for entry in std::fs::read_dir(&dir).map_err(io_err)? {
            let entry = entry.map_err(io_err)?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let file_type = entry.file_type().map_err(io_err)?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && let Ok(rel) = path.strip_prefix(root)
            {
                let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                out.push((rel, path));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn resolve_root(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let path = strip_trailing_separators(path);
    let game_dir = path.join("garrysmod");
    if game_dir.join("gameinfo.txt").is_file() {
        return Some((path, game_dir));
    }
    if path.join("gameinfo.txt").is_file() {
        let root = path.parent()?.to_path_buf();
        return Some((root, path));
    }
    None
}

fn find_game_exe(root: &Path) -> Option<PathBuf> {
    [
        root.join("bin").join("win64").join("gmod.exe"),
        root.join("gmod.exe"),
        root.join("hl2.exe"),
        root.join("hl2_linux"),
        root.join("hl2.sh"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

pub(crate) fn strip_trailing_separators(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    let trimmed = s.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        return path.to_path_buf();
    }
    if trimmed.len() == s.len() { path.to_path_buf() } else { PathBuf::from(trimmed) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::scratch;

    const GMOD: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

    #[test]
    fn detect_real_install() {
        if !Path::new(GMOD).join("garrysmod/gameinfo.txt").is_file() {
            eprintln!("Garry's Mod not found, skipping");
            return;
        }
        let tools = ToolPaths::detect(Path::new(GMOD)).unwrap();
        assert_eq!(tools.compiler, std::env::current_exe().unwrap());
        assert!(tools.game_dir.join("gameinfo.txt").is_file());
        assert!(tools.missing_files().is_empty());
        assert_eq!(tools.gmod_root().as_deref(), Some(Path::new(GMOD)));
        let via_mod = ToolPaths::detect(&Path::new(GMOD).join("garrysmod")).unwrap();
        assert_eq!(via_mod.game_dir, tools.game_dir);
    }

    #[test]
    fn detect_fake_install() {
        let root = scratch("fake_gmod");
        let exe = root.join("hammerpro.exe");
        assert!(ToolPaths::with_compiler(exe.clone(), &root).is_none());
        let game = root.join("garrysmod");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("gameinfo.txt"), "\"GameInfo\" {}").unwrap();
        std::fs::write(root.join("hl2.exe"), b"MZ").unwrap();
        let tools = ToolPaths::with_compiler(exe.clone(), &root).unwrap();
        assert_eq!(tools.compiler, exe);
        assert_eq!(tools.game_exe, Some(root.join("hl2.exe")));
        assert_eq!(tools.maps_dir, game.join("maps"));
        assert_eq!(tools.missing_files(), vec![exe.clone()], "the compiler is not there");
        std::fs::write(&exe, b"MZ").unwrap();
        assert!(tools.missing_files().is_empty());
        std::fs::write(root.join("gmod.exe"), b"MZ").unwrap();
        assert_eq!(ToolPaths::with_compiler(exe.clone(), &root).unwrap().game_exe, Some(root.join("gmod.exe")));
        std::fs::create_dir_all(root.join("bin").join("win64")).unwrap();
        std::fs::write(root.join("bin").join("win64").join("gmod.exe"), b"MZ").unwrap();
        assert_eq!(
            ToolPaths::with_compiler(exe, &root).unwrap().game_exe,
            Some(root.join("bin").join("win64").join("gmod.exe"))
        );
    }

    #[test]
    fn trailing_separators() {
        assert_eq!(strip_trailing_separators(Path::new(r"C:\a\b\")), PathBuf::from(r"C:\a\b"));
        assert_eq!(strip_trailing_separators(Path::new("/a/b//")), PathBuf::from("/a/b"));
        assert_eq!(strip_trailing_separators(Path::new(r"C:\")), PathBuf::from(r"C:\"));
        assert_eq!(strip_trailing_separators(Path::new("/")), PathBuf::from("/"));
    }

    #[test]
    fn threads() {
        assert!(default_threads() >= 1);
    }

    #[test]
    fn pack_dir_walk() {
        let root = scratch("pack_walk");
        std::fs::create_dir_all(root.join("materials/hp")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("materials/hp/a.vmt"), "x").unwrap();
        std::fs::write(root.join("materials/hp/a.vtf"), "x").unwrap();
        std::fs::write(root.join(".git/config"), "x").unwrap();
        let files = pack_files_from_dir(&root).unwrap();
        let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(names, vec!["materials/hp/a.vmt", "materials/hp/a.vtf"]);
        assert!(files.iter().all(|f| f.1.is_file()));
        assert!(pack_files_from_dir(&root.join("nope")).is_err());
    }
}
