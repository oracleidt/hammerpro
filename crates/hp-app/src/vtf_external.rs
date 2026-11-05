use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalEditor {
    pub label: String,
    pub path: PathBuf,
}

const EXE_NAMES: &[&str] = &[
    "VTFEdit++.exe",
    "VTFEditPlusPlus.exe",
    "VTFEdit Reloaded.exe",
    "VTFEditReloaded.exe",
    "VTFEdit-Reloaded.exe",
    "VTFEdit.exe",
    "vtfedit",
    "vtfedit++",
];

const SUB_DIRS: &[&str] = &["", "bin", "bin/x64", "bin/x86", "x64", "x86", "Release", "App"];

fn label_of(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = stem.to_ascii_lowercase();
    if lower.contains("reloaded") {
        "VTFEdit Reloaded".to_owned()
    } else if lower.contains("++") || lower.contains("plusplus") {
        "VTFEdit++".to_owned()
    } else if lower.contains("vtfedit") {
        "VTFEdit".to_owned()
    } else {
        stem
    }
}

fn is_vtf_editor_name(name: &str) -> bool {
    name.to_ascii_lowercase().contains("vtfedit")
}

fn install_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "LOCALAPPDATA", "APPDATA", "USERPROFILE"] {
        if let Some(v) = std::env::var_os(var) {
            let base = PathBuf::from(v);
            if var == "LOCALAPPDATA" {
                roots.push(base.join("Programs"));
            }
            if var == "USERPROFILE" {
                roots.push(base.join("Desktop"));
                roots.push(base.join("Downloads"));
                roots.push(base.join("Documents"));
                roots.push(base.join("Documents").join("Programs"));
            }
            roots.push(base);
        }
    }
    for drive in ["C", "D", "E"] {
        let base = PathBuf::from(format!("{drive}:/"));
        roots.push(base.join("Program Files"));
        roots.push(base.join("Program Files (x86)"));
        roots.push(base.join("Programs"));
        roots.push(base.join("Tools"));
        roots.push(base);
    }
    roots.sort();
    roots.dedup();
    roots
}

fn exes_in(dir: &Path, out: &mut Vec<PathBuf>) {
    for sub in SUB_DIRS {
        let base = if sub.is_empty() { dir.to_path_buf() } else { dir.join(sub) };
        for exe in EXE_NAMES {
            let p = base.join(exe);
            if p.is_file() {
                out.push(p);
            }
        }
    }
}

fn scan_install_dirs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in install_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten().take(4000) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_vtf_editor_name(&name) && entry.path().is_dir() {
                exes_in(&entry.path(), &mut found);
                if let Ok(children) = std::fs::read_dir(entry.path()) {
                    for child in children.flatten().take(64) {
                        if child.path().is_dir() {
                            exes_in(&child.path(), &mut found);
                        }
                    }
                }
            }
        }
    }
    found
}

fn scan_path_var() -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for exe in EXE_NAMES {
                let p = dir.join(exe);
                if p.is_file() {
                    found.push(p);
                }
            }
        }
    }
    found
}

#[cfg(windows)]
fn reg_default(key: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg")
        .args(["query", key, "/ve"])
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    parse_reg_value(&text)
}

pub fn parse_reg_value(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        for kind in ["REG_SZ", "REG_EXPAND_SZ"] {
            if let Some(at) = line.find(kind) {
                let value = line[at + kind.len()..].trim();
                if !value.is_empty() {
                    return Some(value.to_owned());
                }
            }
        }
    }
    None
}

pub fn exe_of_command(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let exe = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next()?.to_owned()
    } else {
        let lower = command.to_ascii_lowercase();
        let end = lower.find(".exe").map(|i| i + 4)?;
        command[..end].to_owned()
    };
    let exe = expand_env(&exe);
    let p = PathBuf::from(exe);
    p.is_file().then_some(p)
}

fn expand_env(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(windows)]
fn associated_program() -> Option<PathBuf> {
    let prog = reg_default("HKCR\\.vtf")?;
    let command = reg_default(&format!("HKCR\\{prog}\\shell\\open\\command"))?;
    exe_of_command(&command)
}

#[cfg(not(windows))]
fn associated_program() -> Option<PathBuf> {
    None
}

fn unique(list: Vec<ExternalEditor>) -> Vec<ExternalEditor> {
    let mut out: Vec<ExternalEditor> = Vec::new();
    for e in list {
        if !out.iter().any(|o| same_file(&o.path, &e.path)) {
            out.push(e);
        }
    }
    out
}

pub fn detect_uncached() -> Vec<ExternalEditor> {
    let mut list: Vec<ExternalEditor> = Vec::new();
    for p in scan_install_dirs().into_iter().chain(scan_path_var()) {
        list.push(ExternalEditor { label: label_of(&p), path: p });
    }
    if let Some(p) = associated_program() {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let label = if is_vtf_editor_name(&name) { label_of(&p) } else { format!("{} (.vtf)", label_of(&p)) };
        list.push(ExternalEditor { label, path: p });
    }
    unique(list)
}

static CACHE: Mutex<Option<Vec<ExternalEditor>>> = Mutex::new(None);

pub fn detect() -> Vec<ExternalEditor> {
    if let Ok(guard) = CACHE.lock()
        && let Some(list) = guard.as_ref()
    {
        return list.clone();
    }
    rescan()
}

pub fn rescan() -> Vec<ExternalEditor> {
    let list = detect_uncached();
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(list.clone());
    }
    list
}

static SCANNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn detect_if_ready() -> Option<Vec<ExternalEditor>> {
    if let Ok(guard) = CACHE.lock()
        && let Some(list) = guard.as_ref()
    {
        return Some(list.clone());
    }
    if !SCANNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let spawned = std::thread::Builder::new().name("hp-vtf-detect".into()).spawn(|| {
            rescan();
            SCANNING.store(false, std::sync::atomic::Ordering::SeqCst);
        });
        if spawned.is_err() {
            SCANNING.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    None
}

fn configured_editor(configured: &str) -> Option<ExternalEditor> {
    let configured = configured.trim().trim_matches('"');
    if configured.is_empty() {
        return None;
    }
    let p = PathBuf::from(configured);
    p.is_file().then(|| ExternalEditor { label: label_of(&p), path: p })
}

pub fn resolve(configured: &str) -> Option<ExternalEditor> {
    configured_editor(configured).or_else(|| detect().into_iter().next())
}

pub fn resolve_nonblocking(configured: &str) -> Option<ExternalEditor> {
    configured_editor(configured).or_else(|| detect_if_ready().and_then(|l| l.into_iter().next()))
}

pub fn launch(editor: &ExternalEditor, file: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new(&editor.path);
    cmd.arg(file);
    if let Some(dir) = editor.path.parent() {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.spawn().map(|_| ()).map_err(|e| format!("{}: {e}", editor.path.display()))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_text_is_parsed() {
        let text = "\r\nHKEY_CLASSES_ROOT\\.vtf\r\n    (Default)    REG_SZ    VTFEdit.File\r\n\r\n";
        assert_eq!(parse_reg_value(text).as_deref(), Some("VTFEdit.File"));
        assert_eq!(parse_reg_value("nothing"), None);
    }

    #[test]
    fn commands_give_the_program() {
        let here = std::env::current_exe().expect("exe");
        let quoted = format!("\"{}\" \"%1\"", here.display());
        assert_eq!(exe_of_command(&quoted), Some(here.clone()));
        assert_eq!(exe_of_command("\"C:/definitely/not/here.exe\" \"%1\""), None);
    }

    #[test]
    fn env_expansion() {
        let path = std::env::var("PATH").expect("PATH");
        assert_eq!(expand_env("%PATH%/x"), format!("{path}/x"));
        assert_eq!(expand_env("%HP_NOPE_NOT_SET%/x"), "%HP_NOPE_NOT_SET%/x");
        assert_eq!(expand_env("100%"), "100%");
    }

    #[test]
    fn labels() {
        assert_eq!(label_of(Path::new("C:/a/VTFEdit++.exe")), "VTFEdit++");
        assert_eq!(label_of(Path::new("C:/a/VTFEdit Reloaded.exe")), "VTFEdit Reloaded");
        assert_eq!(label_of(Path::new("C:/a/VTFEdit.exe")), "VTFEdit");
        assert_eq!(label_of(Path::new("C:/a/paint.exe")), "paint");
    }
}
