use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_app::runtime::{BlenderLink, BlenderNote, FileStamp, file_stamp};
use hp_app::settings::{BlenderFormat, BlenderSettings};
use hp_app::{AppState, t, tf};
use hp_meshio::export::ExportFormat;

pub mod addon;
pub mod disp;
#[cfg(test)]
mod e2e;
pub mod find;
pub mod pull;
pub mod source;
pub mod tr;

pub const CHECK_EVERY: Duration = Duration::from_millis(250);
pub const SETTLE: Duration = Duration::from_millis(600);

const WORK_EXTENSIONS: &[&str] = &["glb", "gltf", "obj", "mtl", "fbx", "png", "jpg", "jpeg", "tga", "bin"];

pub fn export_format(f: BlenderFormat) -> ExportFormat {
    match f {
        BlenderFormat::Gltf => ExportFormat::Gltf,
        BlenderFormat::Obj => ExportFormat::Obj,
        BlenderFormat::Fbx => ExportFormat::Fbx,
    }
}

#[derive(Clone, Debug, Default)]
pub struct SendOptions {
    pub format: Option<BlenderFormat>,
    pub launch: bool,
    pub live: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SendReport {
    pub file: PathBuf,
    pub format: BlenderFormat,
    pub kind: &'static str,
    pub triangles: usize,
    pub materials: usize,
    pub live: bool,
    pub launched: bool,
    pub reused: bool,
    pub blender: Option<PathBuf>,
    pub problem: Option<String>,
}

pub fn work_dir(state: &AppState) -> PathBuf {
    if let Some(dir) = &state.runtime.blender.work_dir {
        return dir.clone();
    }
    if let Some(dir) = std::env::var_os("HAMMERPRO_BLENDER_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    if hp_compile::sandbox::is_sandboxed() {
        return std::env::temp_dir().join("hammerpro_sandbox_blender");
    }
    BlenderSettings::work_dir()
}

pub fn resolve_exe(state: &mut AppState) -> Option<PathBuf> {
    let found = find::resolve(&state.settings.blender.path);
    state.runtime.blender.exe = found.clone();
    state.runtime.blender.exe_checked = true;
    found
}

pub fn clean_work_dir(work: &Path) {
    let Ok(entries) = std::fs::read_dir(work) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if p.is_file() && WORK_EXTENSIONS.contains(&ext.as_str()) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

pub fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> Result<PathBuf, String> {
    let target = dir.join(name);
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(target)
}

pub fn launch(exe: &Path, startup: &Path, link: &Path) -> Result<std::process::Child, String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--python").arg(startup).env("HAMMERPRO_LINK", link);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map_err(|e| tf!("blender.launch_failed", exe.display(), e.to_string()))
}

pub fn send(state: &mut AppState, opts: &SendOptions) -> Result<SendReport, String> {
    let source = source::selection_source(state)?;
    let format = opts.format.unwrap_or(state.settings.blender.format);
    let live = opts.live.unwrap_or(state.settings.blender.live_link);
    let work = work_dir(state);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    clean_work_dir(&work);
    let files = hp_meshio::export::export(&source.model, export_format(format), &source.stem);
    let mut main = work.clone();
    for f in files.iter().skip(1) {
        write_atomic(&work, &f.name, &f.data)?;
    }
    if let Some(first) = files.first() {
        main = write_atomic(&work, &first.name, &first.data)?;
    }
    let (startup, link_file) = addon::write_working_files(&work, &main, format, true)?;
    let kind = source.target.id();
    let triangles = source.model.triangles.len();
    let materials = source.model.materials.len();
    state.runtime.blender.link = Some(BlenderLink {
        file: main.clone(),
        format,
        target: source.target,
        baseline: file_stamp(&main),
        candidate: None,
        pulls: 0,
        live,
    });
    let exe = resolve_exe(state);
    let mut launched = false;
    let mut problem = None;
    let reused = opts.launch && state.runtime.blender.blender_running();
    if opts.launch && !reused {
        match &exe {
            Some(exe) if hp_compile::sandbox::is_sandboxed() => {
                problem = Some(t!("blender.sandboxed").to_owned());
                let _ = exe;
            }
            Some(exe) => match launch(exe, &startup, &link_file) {
                Ok(child) => {
                    launched = true;
                    state.runtime.blender.launched += 1;
                    state.runtime.blender.child = Some(child);
                }
                Err(e) => problem = Some(e),
            },
            None => {
                problem = Some(t!("blender.not_found").to_owned());
                state.runtime.blender.missing_open = true;
            }
        }
    }
    let text = match (&problem, launched || reused) {
        (None, true) => tf!("blender.sent", main.display()),
        (Some(p), _) => p.clone(),
        (None, false) => tf!("blender.exported", main.display()),
    };
    state.runtime.blender.note = Some(BlenderNote { ok: problem.is_none(), text });
    Ok(SendReport { file: main, format, kind, triangles, materials, live, launched, reused, blender: exe, problem })
}

pub fn relaunch(state: &mut AppState) -> Result<(), String> {
    let work = work_dir(state);
    let startup = work.join("startup.py");
    let link = work.join("link.json");
    if !startup.is_file() {
        return Err(t!("blender.no_link").to_owned());
    }
    let exe = resolve_exe(state).ok_or_else(|| t!("blender.not_found").to_owned())?;
    let child = launch(&exe, &startup, &link)?;
    state.runtime.blender.child = Some(child);
    state.runtime.blender.missing_open = false;
    state.runtime.blender.launched += 1;
    Ok(())
}

pub fn stop(state: &mut AppState) -> bool {
    let had = state.runtime.blender.link.take().is_some();
    if had {
        let link = work_dir(state).join("link.json");
        let _ = std::fs::write(&link, "{\"version\":1,\"file\":\"\"}");
        state.runtime.blender.note = Some(BlenderNote { ok: true, text: t!("blender.stopped").to_owned() });
    }
    had
}

pub fn install_addon(state: &mut AppState) -> Result<addon::Installed, String> {
    let work = work_dir(state);
    let link = work.join("link.json");
    let exe = resolve_exe(state);
    let done = addon::install(&addon::config_roots(), exe.as_deref(), &link, &work)?;
    let first = done.files.first().map(|p| p.display().to_string()).unwrap_or_default();
    let text =
        if done.in_blender { tf!("blender.addon_installed", first) } else { tf!("blender.addon_written", first) };
    state.runtime.blender.note = Some(BlenderNote { ok: true, text });
    Ok(done)
}

pub fn poll_stamp(link: &mut BlenderLink, current: FileStamp, now: Instant) -> bool {
    if current.is_none() || current == link.baseline {
        link.candidate = None;
        return false;
    }
    match link.candidate {
        Some((seen, since)) if seen == current => {
            if now.duration_since(since) >= SETTLE {
                link.baseline = current;
                link.candidate = None;
                return true;
            }
            false
        }
        _ => {
            link.candidate = Some((current, now));
            false
        }
    }
}

pub fn watch_due(state: &mut AppState, now: Instant) -> bool {
    let rt = &mut state.runtime.blender;
    if rt.pull_in_progress {
        return false;
    }
    if rt.last_check.is_some_and(|t| now.duration_since(t) < CHECK_EVERY) {
        return false;
    }
    rt.last_check = Some(now);
    let Some(link) = rt.link.as_mut() else { return false };
    if !link.live {
        return false;
    }
    let current = file_stamp(&link.file);
    poll_stamp(link, current, now)
}

pub fn status_json(state: &AppState) -> serde_json::Value {
    let rt = &state.runtime.blender;
    let s = &state.settings.blender;
    serde_json::json!({
        "blender_found": rt.exe.is_some(),
        "blender": rt.exe.as_ref().map(|p| p.display().to_string()),
        "configured_path": s.path,
        "format": s.format.id(),
        "live_link_setting": s.live_link,
        "working_folder": work_dir(state).display().to_string(),
        "link": rt.link.as_ref().map(|l| serde_json::json!({
            "file": l.file.display().to_string(),
            "format": l.format.id(),
            "target": l.target.id(),
            "items": match &l.target {
                hp_app::runtime::BlenderTarget::Brushes { solids } => solids.len(),
                hp_app::runtime::BlenderTarget::Prop { props, .. } => props.len(),
                hp_app::runtime::BlenderTarget::Displacements { faces } => faces.len(),
            },
            "live": l.live,
            "pulls": l.pulls,
            "file_exists": l.file.is_file(),
        })),
        "pull_in_progress": rt.pull_in_progress,
        "blender_launches": rt.launched,
        "last_message": rt.note.as_ref().map(|n| serde_json::json!({ "ok": n.ok, "text": n.text })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::runtime::BlenderTarget;
    use std::time::SystemTime;

    fn link(baseline: FileStamp) -> BlenderLink {
        BlenderLink {
            file: PathBuf::from("x.glb"),
            format: BlenderFormat::Gltf,
            target: BlenderTarget::Brushes { solids: vec![1] },
            baseline,
            candidate: None,
            pulls: 0,
            live: true,
        }
    }

    #[test]
    fn a_change_must_stay_still_before_it_is_pulled() {
        let t0 = SystemTime::UNIX_EPOCH;
        let a: FileStamp = Some((t0, 10));
        let b: FileStamp = Some((t0 + Duration::from_secs(5), 30));
        let c: FileStamp = Some((t0 + Duration::from_secs(5), 64));
        let mut l = link(a);
        let now = Instant::now();
        assert!(!poll_stamp(&mut l, a, now), "unchanged file");
        assert!(!poll_stamp(&mut l, None, now), "missing file");
        assert!(!poll_stamp(&mut l, b, now), "first sight of a change");
        assert!(!poll_stamp(&mut l, c, now + SETTLE), "still growing");
        assert!(!poll_stamp(&mut l, c, now + SETTLE + Duration::from_millis(100)), "settling");
        assert!(poll_stamp(&mut l, c, now + SETTLE * 3), "settled");
        assert_eq!(l.baseline, c);
        assert!(!poll_stamp(&mut l, c, now + SETTLE * 4), "pulled once");
    }

    #[test]
    fn sending_a_selection_writes_the_files_and_the_link() {
        let mut state = hp_app::testing::test_state();
        let work = std::env::temp_dir().join(format!("hp_blender_send_{}", std::process::id()));
        state.runtime.blender.work_dir = Some(work.clone());
        let ids: Vec<hp_editor::ObjRef> =
            state.doc.scene().solids().keys().take(1).map(|id| hp_editor::ObjRef::Solid(*id)).collect();
        state.doc.select(&ids, hp_editor::SelectOp::Replace);
        let report =
            send(&mut state, &SendOptions { format: Some(BlenderFormat::Obj), launch: false, live: Some(true) })
                .expect("send");
        assert_eq!(report.kind, "brushes");
        assert!(report.file.is_file() && report.file.extension().is_some_and(|e| e == "obj"));
        assert!(report.triangles > 0 && !report.launched);
        let link = state.runtime.blender.link.clone().expect("link");
        assert_eq!(link.file, report.file);
        assert!(work.join("startup.py").is_file());
        let json = status_json(&state);
        assert_eq!(json["link"]["target"], "brushes");
        assert!(stop(&mut state));
        assert!(!stop(&mut state));
        state.doc.select(&[], hp_editor::SelectOp::Replace);
        assert!(send(&mut state, &SendOptions::default()).is_err());
        let _ = std::fs::remove_dir_all(&work);
    }
}
