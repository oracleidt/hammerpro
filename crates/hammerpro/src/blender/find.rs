use std::path::{Path, PathBuf};

pub fn is_blender_exe(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    stem.starts_with("blender")
}

pub fn exe_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["blender.exe", "blender-launcher.exe"]
    } else if cfg!(target_os = "macos") {
        &["Contents/MacOS/Blender", "Blender"]
    } else {
        &["blender"]
    }
}

pub fn exe_in(dir: &Path) -> Option<PathBuf> {
    exe_names().iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

pub fn version_of(name: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in name.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                out.push(cur.parse().unwrap_or(0));
                cur.clear();
            }
            if c != '.' && !out.is_empty() {
                break;
            }
        }
    }
    out
}

pub fn installs_in(parent: &Path) -> Vec<(Vec<u32>, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.to_ascii_lowercase().starts_with("blender") {
            continue;
        }
        let dir = e.path();
        if let Some(exe) = exe_in(&dir) {
            out.push((version_of(&name), exe));
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

fn best_in(parent: &Path) -> Option<PathBuf> {
    installs_in(parent).into_iter().next().map(|(_, exe)| exe)
}

pub fn standard_locations() -> Vec<PathBuf> {
    let mut parents: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(pf) = std::env::var_os(var) {
            parents.push(Path::new(&pf).join("Blender Foundation"));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        parents.push(Path::new(&local).join("Programs").join("Blender Foundation"));
        parents.push(Path::new(&local).join("Programs"));
    }
    if cfg!(target_os = "macos") {
        parents.push(PathBuf::from("/Applications"));
    }
    if cfg!(target_os = "linux") {
        parents.push(PathBuf::from("/opt"));
        parents.push(PathBuf::from("/usr/local"));
    }
    parents
}

pub fn steam_locations(libraries: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for lib in libraries {
        out.push(lib.join("steamapps").join("common"));
        out.push(lib.join("common"));
    }
    out
}

pub fn from_path_var() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| exe_in(&dir))
}

#[cfg(windows)]
fn run_reg(args: &[&str]) -> String {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg").args(args).creation_flags(0x0800_0000).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

#[cfg(not(windows))]
fn run_reg(_args: &[&str]) -> String {
    String::new()
}

pub fn quoted_exe(line: &str) -> Option<PathBuf> {
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    let p = PathBuf::from(&line[start..end]);
    is_blender_exe(&p).then_some(p)
}

pub fn from_registry() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for key in [
        "HKLM\\SOFTWARE\\Classes\\blendfile\\shell\\open\\command",
        "HKCU\\Software\\Classes\\blendfile\\shell\\open\\command",
        "HKCR\\blendfile\\shell\\open\\command",
    ] {
        for line in run_reg(&["query", key, "/ve"]).lines() {
            if let Some(p) = quoted_exe(line) {
                out.push(p);
            }
        }
    }
    for key in ["HKLM\\SOFTWARE\\BlenderFoundation", "HKCU\\SOFTWARE\\BlenderFoundation"] {
        for line in run_reg(&["query", key, "/s"]).lines() {
            if let Some(pos) = line.find("REG_SZ") {
                let value = line[pos + "REG_SZ".len()..].trim();
                let dir = PathBuf::from(value);
                if dir.is_dir()
                    && let Some(exe) = exe_in(&dir)
                {
                    out.push(exe);
                }
            }
        }
    }
    out
}

pub fn from_steam() -> Option<PathBuf> {
    let libs = hp_fs::detect_steam_libraries();
    steam_locations(&libs).into_iter().find_map(|common| {
        let dir = common.join("Blender");
        exe_in(&dir).or_else(|| best_in(&common))
    })
}

pub fn detect() -> Option<PathBuf> {
    for p in from_registry() {
        if is_blender_exe(&p) {
            return Some(p);
        }
    }
    for parent in standard_locations() {
        if let Some(exe) = best_in(&parent) {
            return Some(exe);
        }
    }
    from_steam().or_else(from_path_var)
}

pub fn resolve(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim().trim_matches('"');
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        if is_blender_exe(&p) {
            return Some(p);
        }
        if p.is_dir() {
            return exe_in(&p).or_else(|| best_in(&p));
        }
        return None;
    }
    detect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_blender_find_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    fn fake(dir: &Path, name: &str) -> PathBuf {
        let sub = dir.join(name);
        let exe = sub.join(exe_names()[0]);
        if let Some(parent) = exe.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(&exe, "x").expect("exe");
        exe
    }

    #[test]
    fn versions_sort_and_the_newest_install_wins() {
        assert_eq!(version_of("Blender 4.2"), vec![4, 2]);
        assert_eq!(version_of("Blender 3.6.13 LTS"), vec![3, 6, 13]);
        assert!(version_of("Blender").is_empty());
        let dir = scratch("installs");
        fake(&dir, "Blender 3.6");
        let newest = fake(&dir, "Blender 4.2");
        fake(&dir, "Blender 4.10");
        std::fs::create_dir_all(dir.join("Other 9.9")).expect("other");
        let list = installs_in(&dir);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].0, vec![4, 10]);
        assert_eq!(best_in(&dir).map(|p| p.to_string_lossy().contains("4.10")), Some(true));
        assert!(list.iter().any(|(_, p)| *p == newest));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_configured_path_is_checked_and_folders_are_searched() {
        let dir = scratch("configured");
        let exe = fake(&dir, "Blender 4.2");
        assert_eq!(resolve(&exe.display().to_string()), Some(exe.clone()));
        assert_eq!(resolve(&format!("\"{}\"", exe.display())), Some(exe.clone()));
        assert_eq!(resolve(&dir.join("Blender 4.2").display().to_string()), Some(exe.clone()));
        assert_eq!(resolve(&dir.display().to_string()), Some(exe.clone()));
        assert_eq!(resolve(&dir.join("nothing.exe").display().to_string()), None);
        let other = dir.join("notepad.exe");
        std::fs::write(&other, "x").expect("write");
        assert!(!is_blender_exe(&other));
        assert_eq!(resolve(&other.display().to_string()), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_lines_and_steam_folders() {
        assert_eq!(quoted_exe("no quotes here"), None);
        assert_eq!(quoted_exe("  REG_SZ  \"C:\\nowhere\\blender.exe\" \"%1\""), None);
        let libs = vec![PathBuf::from("D:/SteamLibrary")];
        assert_eq!(steam_locations(&libs).len(), 2);
        let dir = scratch("steam");
        let common = dir.join("steamapps").join("common");
        std::fs::create_dir_all(&common).expect("common");
        let exe = fake(&common, "Blender");
        let hit = steam_locations(std::slice::from_ref(&dir))
            .into_iter()
            .find_map(|c| exe_in(&c.join("Blender")).or_else(|| best_in(&c)));
        assert_eq!(hit, Some(exe));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
