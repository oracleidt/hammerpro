use std::path::{Path, PathBuf};

pub const GMOD_APP_ID: u32 = 4000;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryFolder {
    pub(crate) path: PathBuf,
    pub(crate) apps: Vec<u32>,
}

pub(crate) fn parse_library_folders(text: &str) -> Vec<LibraryFolder> {
    let Ok(root) = hp_kv::parse(text) else {
        return Vec::new();
    };
    let Some(block) = root.child("libraryfolders") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for child in &block.children {
        let Some(path) = child.get("path") else { continue };
        let apps = child
            .child("apps")
            .map(|apps| apps.props.iter().filter_map(|(k, _)| k.trim().parse().ok()).collect())
            .unwrap_or_default();
        out.push(LibraryFolder { path: PathBuf::from(unescape_vdf(path)), apps });
    }
    for (key, value) in &block.props {
        if key.trim().parse::<u32>().is_ok() && !value.trim().is_empty() {
            out.push(LibraryFolder { path: PathBuf::from(unescape_vdf(value)), apps: Vec::new() });
        }
    }
    out
}

fn unescape_vdf(value: &str) -> String {
    value.trim().replace("\\\\", "\\")
}

#[cfg(windows)]
fn reg_query(key: &str, value: &str) -> Option<String> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let reg = std::path::PathBuf::from(std::env::var_os("SystemRoot")?).join("System32").join("reg.exe");
    let output = std::process::Command::new(reg)
        .args(["query", key, "/v", value])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_reg_output(&String::from_utf8_lossy(&output.stdout), value)
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_reg_output(stdout: &str, value: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.get(value.len()..)?;
        if !line.get(..value.len())?.eq_ignore_ascii_case(value) {
            return None;
        }
        let rest = rest.trim_start();
        let data = rest.strip_prefix("REG_SZ").or_else(|| rest.strip_prefix("REG_EXPAND_SZ"))?.trim();
        (!data.is_empty()).then(|| data.to_owned())
    })
}

fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        let from_registry = reg_query(r"HKCU\Software\Valve\Steam", "SteamPath")
            .or_else(|| reg_query(r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"))
            .or_else(|| reg_query(r"HKLM\SOFTWARE\Valve\Steam", "InstallPath"));
        roots.extend(from_registry.map(PathBuf::from));
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(pf) = std::env::var_os(var) {
                roots.push(PathBuf::from(pf).join("Steam"));
            }
        }
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
        roots.push(PathBuf::from(r"C:\Program Files\Steam"));
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            roots.push(home.join(".steam/steam"));
            roots.push(home.join(".local/share/Steam"));
            roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
            roots.push(home.join("Library/Application Support/Steam"));
        }
    }
    roots
}

fn lexical_clean(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(windows)]
fn ordinary_windows_path(path: &Path) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    let slash = u16::from(b'\\');
    if !wide.starts_with(&[slash, slash, u16::from(b'?'), slash]) {
        return path.to_path_buf();
    }
    let tail = &wide[4..];
    let unc = tail.get(..4).is_some_and(|prefix| {
        prefix.iter().zip(b"UNC\\").all(|(a, b)| *a == u16::from(*b) || *a == u16::from(b.to_ascii_lowercase()))
    });
    if unc {
        let mut normal = vec![slash, slash];
        normal.extend_from_slice(&tail[4..]);
        PathBuf::from(OsString::from_wide(&normal))
    } else if tail.len() >= 3
        && u8::try_from(tail[0]).is_ok_and(|drive| drive.is_ascii_alphabetic())
        && tail[1] == u16::from(b':')
        && (tail[2] == slash || tail[2] == u16::from(b'/'))
    {
        PathBuf::from(OsString::from_wide(tail))
    } else {
        path.to_path_buf()
    }
}

fn canonical_input(path: &Path) -> PathBuf {
    #[cfg(windows)]
    let path = ordinary_windows_path(path);
    #[cfg(not(windows))]
    let path = path.to_path_buf();
    let absolute = std::path::absolute(&path).unwrap_or(path);
    #[cfg(windows)]
    let absolute = lexical_clean(&absolute);
    absolute
}

fn canonical_existing_ancestor(path: &Path) -> PathBuf {
    let clean = lexical_clean(path);
    let mut probe = clean.as_path();
    let mut missing = Vec::new();
    loop {
        if let Ok(mut found) = std::fs::canonicalize(probe) {
            for part in missing.iter().rev() {
                found.push(part);
            }
            return found;
        }
        let Some(name) = probe.file_name() else { return clean };
        missing.push(name.to_os_string());
        let Some(parent) = probe.parent() else { return clean };
        probe = parent;
    }
}

pub(crate) fn path_key(path: &Path) -> String {
    let input = canonical_input(path);
    let canonical = std::fs::canonicalize(&input).unwrap_or_else(|_| canonical_existing_ancestor(&input));
    let mut key = canonical.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        key.make_ascii_lowercase();
        if let Some(stripped) = key.strip_prefix("//?/unc/") {
            key = format!("//{stripped}");
        } else if let Some(stripped) = key.strip_prefix("//?/") {
            key = stripped.to_owned();
        }
    }
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    key
}

pub fn detect_steam_libraries() -> Vec<PathBuf> {
    libraries_with_apps().into_iter().map(|l| l.path).collect()
}

fn libraries_with_apps() -> Vec<LibraryFolder> {
    let mut out: Vec<LibraryFolder> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let mut push = |lib: LibraryFolder, out: &mut Vec<LibraryFolder>| {
        if !lib.path.join("steamapps").is_dir() {
            return;
        }
        let key = path_key(&lib.path);
        if let Some(i) = keys.iter().position(|k| *k == key) {
            if let Some(existing) = out.get_mut(i) {
                existing.apps.extend(lib.apps);
            }
        } else {
            keys.push(key);
            out.push(lib);
        }
    };
    for root in steam_roots() {
        push(LibraryFolder { path: root.clone(), apps: Vec::new() }, &mut out);
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(bytes) = std::fs::read(&vdf) {
            for lib in parse_library_folders(&String::from_utf8_lossy(&bytes)) {
                push(lib, &mut out);
            }
        }
    }
    out
}

fn gmod_in_library(library: &Path) -> Option<PathBuf> {
    let steamapps = library.join("steamapps");
    let manifest = steamapps.join(format!("appmanifest_{GMOD_APP_ID}.acf"));
    let mut candidates = Vec::new();
    if let Ok(bytes) = std::fs::read(&manifest)
        && let Ok(kv) = hp_kv::parse(&String::from_utf8_lossy(&bytes))
        && let Some(dir) = kv.child("AppState").and_then(|s| s.get("installdir"))
        && !dir.trim().is_empty()
    {
        candidates.push(steamapps.join("common").join(dir.trim()));
    }
    candidates.push(steamapps.join("common").join("GarrysMod"));
    candidates.into_iter().find(|c| is_gmod_root(c))
}

pub(crate) fn is_gmod_root(path: &Path) -> bool {
    path.join("garrysmod").is_dir()
}

pub fn detect_gmod_root() -> Option<PathBuf> {
    let mut libraries = libraries_with_apps();
    libraries.sort_by_key(|l| !l.apps.contains(&GMOD_APP_ID));
    libraries.iter().find_map(|l| gmod_in_library(&l.path))
}

pub fn detect_workshop_dir(gmod_root: &Path) -> Option<PathBuf> {
    let steamapps = gmod_root.parent()?.parent()?;
    let dir = steamapps.join("workshop").join("content").join(GMOD_APP_ID.to_string());
    dir.is_dir().then_some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn extended_paths_are_normalized_before_canonicalization() {
        assert_eq!(canonical_input(Path::new(r"\\?\C:\root/folder/../assets/.")), PathBuf::from(r"C:\root\assets"));
        assert_eq!(
            canonical_input(Path::new(r"\\?\UnC\server\share\folder\..\assets\.")),
            PathBuf::from(r"\\server\share\assets")
        );
        assert_eq!(canonical_input(Path::new(r"\\?\C:\..\assets")), PathBuf::from(r"C:\assets"));
    }

    #[test]
    fn library_folders_current_and_old_format() {
        let text = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"228980"		"397737581"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps" { "4000" "1" "620" "2" }
	}
}"#;
        let libs = parse_library_folders(text);
        assert_eq!(libs.len(), 2);
        assert_eq!(libs[0].path, PathBuf::from(r"C:\Program Files (x86)\Steam"));
        assert_eq!(libs[1].path, PathBuf::from(r"D:\SteamLibrary"));
        assert_eq!(libs[1].apps, [4000, 620]);

        let old = "\"LibraryFolders\" { \"TimeNextStatsReport\" \"1\" \"1\" \"E:\\\\Games\\\\Steam\" }";
        let libs = parse_library_folders(old);
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0].path, PathBuf::from(r"E:\Games\Steam"));
        assert!(parse_library_folders("garbage").is_empty());
    }

    #[test]
    fn reg_output() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Valve\\Steam\r\n    SteamPath    REG_SZ    c:/program files (x86)/steam\r\n\r\n";
        assert_eq!(parse_reg_output(out, "SteamPath").as_deref(), Some("c:/program files (x86)/steam"));
        assert_eq!(parse_reg_output(out, "InstallPath"), None);
        assert_eq!(parse_reg_output("", "SteamPath"), None);
    }

    #[test]
    fn workshop_dir_requires_layout() {
        assert_eq!(detect_workshop_dir(Path::new("/")), None);
    }
}
