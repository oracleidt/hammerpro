mod checklist;
pub(crate) mod files;
mod game_settings;
mod manifest;
pub mod mcp;
mod pipeline;
mod placement;
mod profiles;
mod publish;
mod report;
mod session;
pub mod sidecar;
mod tabs;
mod thumbnail;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use hp_app::runtime::CaptureState;
use hp_app::settings::Settings;
use hp_app::state::AssetStatus;
use hp_app::{Action, AppState, Severity, t, tf};
use hp_compile::{CompileEvent, Severity as LogSeverity, StepKind};
use hp_editor::deps_roots::{AssetRoot, RootKind as EdKind, RootSource};
use hp_fs::deps::{DepRoot, DependencyGraph, DepsCache, RootKind};
use hp_gma::AddonJson;
use image::RgbaImage;

use crate::dialogs::MapDialog;
use crate::util::{escape_pressed, icon_title};
pub(crate) use files::{safe_join, write_atomic};
pub(crate) use pipeline::KeyKinds;
use pipeline::{ReleaseEvent, ReleaseInput, ReleaseSummary, Stage};
pub(crate) use report::{filter_box, origin_name, reason_text};
pub use sidecar::ReleaseSidecar;

pub(crate) fn fmt_size(n: u64) -> String {
    pipeline::fmt_bytes(n)
}

pub(crate) fn decode_image(bytes: &[u8]) -> Result<RgbaImage, String> {
    thumbnail::decode(bytes)
}

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(ReleaseWindow::default())]
}

pub(crate) fn root_kind(kind: EdKind) -> Option<RootKind> {
    Some(match kind {
        EdKind::Material => RootKind::Material,
        EdKind::Texture => RootKind::Texture,
        EdKind::Model => RootKind::Model,
        EdKind::Sprite => RootKind::Sprite,
        EdKind::Sound => RootKind::Sound,
        EdKind::Particle => RootKind::Particle,
        EdKind::Soundscape => RootKind::Soundscape,
        EdKind::Skybox => RootKind::Skybox,
        EdKind::File => RootKind::File,
        EdKind::Instance => return None,
    })
}

pub(crate) fn root_why(source: &RootSource) -> String {
    match source {
        RootSource::Faces { count } => tf!("release.why_faces", count),
        RootSource::Key { entity, class, key } => tf!("release.why_key", class, entity, key),
        RootSource::Output { entity, class, output } => tf!("release.why_output", class, entity, output),
    }
}

pub(crate) fn key_kinds(fgd: Option<&hp_fgd::ResolvedFgd>) -> KeyKinds {
    use hp_fgd::PropertyType as P;
    let mut map: HashMap<String, Vec<(String, EdKind)>> = HashMap::new();
    if let Some(fgd) = fgd {
        for class in fgd.iter() {
            let keys: Vec<(String, EdKind)> = class
                .props
                .iter()
                .filter_map(|p| {
                    let kind = match p.kind {
                        P::Studio => EdKind::Model,
                        P::Sprite => EdKind::Sprite,
                        P::Sound => EdKind::Sound,
                        P::Decal | P::Material => EdKind::Material,
                        P::ParticleSystem => EdKind::Particle,
                        P::InstanceFile => EdKind::Instance,
                        _ => return None,
                    };
                    Some((p.name.to_ascii_lowercase(), kind))
                })
                .collect();
            if !keys.is_empty() {
                map.insert(class.name.to_ascii_lowercase(), keys);
            }
        }
    }
    Arc::new(map)
}

pub(crate) fn collect_roots(state: &AppState, kinds: &KeyKinds) -> (Vec<DepRoot>, Vec<(String, String)>) {
    let classify = |class: &str, key: &str| {
        kinds
            .get(&class.to_ascii_lowercase())
            .and_then(|keys| keys.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, kind)| *kind))
    };
    let roots: Vec<AssetRoot> = state.doc.asset_roots(&classify);
    let mut deps = Vec::with_capacity(roots.len());
    let mut instances = Vec::new();
    for r in roots {
        let why = root_why(&r.source);
        match root_kind(r.kind) {
            Some(kind) => deps.push(DepRoot { kind, name: r.name, why }),
            None => instances.push((r.name, why)),
        }
    }
    (deps, instances)
}

pub(crate) fn instance_roots(
    vmf: &Path,
    instances: &[(String, String)],
    kinds: &KeyKinds,
    cancel: &AtomicBool,
    warn: &mut dyn FnMut(String),
) -> Option<Vec<DepRoot>> {
    let mut roots = Vec::new();
    let base = vmf.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let mut queue: Vec<(PathBuf, String, usize)> =
        instances.iter().map(|(f, why)| (base.join(f.replace('\\', "/")), why.clone(), 0)).collect();
    let classify = |class: &str, key: &str| {
        kinds
            .get(&class.to_ascii_lowercase())
            .and_then(|keys| keys.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, kind)| *kind))
    };
    while let Some((path, why, depth)) = queue.pop() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if depth > 8 || !seen.insert(path.clone()) {
            continue;
        }
        let map = match hp_vmf::load(&path) {
            Ok(m) => m,
            Err(e) => {
                warn(tf!("release.instance_unreadable", path.display(), e));
                continue;
            }
        };
        let file_label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        for r in hp_editor::deps_roots::map_asset_roots(&map, &classify) {
            if r.kind == EdKind::Instance {
                let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
                queue.push((dir.join(r.name.replace('\\', "/")), tf!("release.why_instance", file_label), depth + 1));
                continue;
            }
            if let Some(kind) = root_kind(r.kind) {
                roots.push(DepRoot { kind, name: r.name, why: format!("{why} > {file_label}") });
            }
        }
    }
    Some(roots)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    #[default]
    Build,
    Report,
    Checklist,
    Addon,
    Publish,
}

pub(crate) struct Job {
    rx: Receiver<ReleaseEvent>,
    cancel: Arc<AtomicBool>,
    analyze_only: bool,
    serial: u64,
}

pub(crate) struct PublishJob {
    rx: Receiver<publish::PublishEvent>,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub(crate) struct ReleaseWindow {
    tab: Tab,
    job: Option<Job>,
    pending_build: Option<Instant>,
    log: Vec<(LogSeverity, String)>,
    stage: Option<Stage>,
    stages_done: Vec<Stage>,
    progress: f32,
    compile_step: Option<(StepKind, f32)>,
    graph: Option<Arc<DependencyGraph>>,
    summary: Option<Result<ReleaseSummary, String>>,
    cache: Option<(PathBuf, Arc<DepsCache>)>,
    side: ReleaseSidecar,
    side_serial: Option<u64>,
    thumb: Option<RgbaImage>,
    thumb_tex: Option<egui::TextureHandle>,
    thumb_dirty: bool,
    pick_image: Option<Receiver<Option<PathBuf>>>,
    image_job: Option<Receiver<Result<RgbaImage, String>>>,
    pick_folder: Option<Receiver<Option<PathBuf>>>,
    facts_job: Option<Receiver<(checklist::Facts, Option<RgbaImage>)>>,
    rows: Option<Vec<checklist::Row>>,
    facts: Option<checklist::Facts>,
    facts_key: Option<(u64, u64)>,
    confirm_session: bool,
    confirm_publish: Option<publish::PublishKind>,
    publishing: Option<PublishJob>,
    publish_log: Vec<(LogSeverity, String)>,
    report: report::ReportView,
    ignore_text: String,
}

impl MapDialog for ReleaseWindow {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.poll(ctx, state);
        if !state.runtime.content.release_open {
            return;
        }
        if let Some(tab) = state.runtime.content.release_tab.take() {
            self.tab = match tab {
                "publish" => Tab::Publish,
                "report" => Tab::Report,
                "checklist" => Tab::Checklist,
                "addon" => Tab::Addon,
                _ => Tab::Build,
            };
        }
        if matches!(state.runtime.content.capture, CaptureState::Requested | CaptureState::Capturing) {
            ctx.request_repaint();
            return;
        }
        self.sync_sidecar(state);
        let mut open = true;
        let response = hp_app::chrome::Window::new(icon_title("📦", t!("release.title")))
            .id(egui::Id::unique("hp_release_window"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(920.0, 700.0)))
            .default_size([920.0, 700.0])
            .min_size([640.0, 460.0])
            .open(&mut open)
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| self.window_ui(ui, state));
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
            && !self.confirm_session
            && self.confirm_publish.is_none()
        {
            open = false;
        }
        self.modals(ctx, state);
        if !open {
            state.runtime.content.release_open = false;
            self.pending_build = None;
        }
    }
}

impl ReleaseWindow {
    fn running(&self) -> bool {
        self.job.is_some() || self.pending_build.is_some()
    }

    fn map_of(state: &AppState) -> Option<(String, PathBuf)> {
        let vmf = state.doc.path()?;
        let vmf = std::path::absolute(vmf).unwrap_or_else(|_| vmf.to_path_buf());
        let stem = vmf.file_stem()?.to_string_lossy().to_ascii_lowercase();
        Some((stem, vmf))
    }

    fn out_root(&self, state: &AppState) -> Option<PathBuf> {
        if let Some(dir) = &self.side.output_dir {
            return Some(dir.clone());
        }
        let (map, vmf) = Self::map_of(state)?;
        Some(vmf.parent()?.join(format!("{map}_content")))
    }

    fn addon(&self, map: &str) -> AddonJson {
        AddonJson {
            title: if self.side.title.trim().is_empty() { map.to_owned() } else { self.side.title.clone() },
            kind: "map".to_owned(),
            tags: self.side.tags.clone(),
            ignore: self.side.ignore.clone(),
            description: self.side.description.clone(),
        }
    }

    fn addon_folder(&self, map: &str) -> String {
        placement::addon_folder_name(&self.addon(map).title, map)
    }

    fn sync_sidecar(&mut self, state: &mut AppState) {
        let serial = state.sidecar.serial();
        if self.side_serial == Some(serial) {
            return;
        }
        self.side_serial = Some(serial);
        self.side = state.sidecar.get();
        self.ignore_text = self.side.ignore.join("\n");
        self.thumb = None;
        self.thumb_tex = None;
        self.graph = None;
        self.summary = None;
        self.rows = None;
        self.facts_key = None;
        self.log.clear();
        self.stages_done.clear();
        self.stage = None;
        state.runtime.content.checklist = None;
        state.runtime.content.last_release = None;
    }

    fn save_sidecar(&mut self, state: &mut AppState) {
        state.sidecar.set(&self.side);
    }

    fn deps_cache(&mut self, vmf: &Path) -> Arc<DepsCache> {
        match &self.cache {
            Some((p, c)) if p == vmf => c.clone(),
            _ => {
                let c = Arc::new(DepsCache::new());
                self.cache = Some((vmf.to_path_buf(), c.clone()));
                c
            }
        }
    }

    pub(crate) fn start(&mut self, ctx: &egui::Context, state: &mut AppState, analyze_only: bool) {
        if self.job.is_some() {
            return;
        }
        if !analyze_only && (state.doc.path().is_none() || state.doc.is_dirty()) {
            state.request(if state.doc.path().is_none() { Action::SaveMapAs } else { Action::SaveMap });
            self.pending_build = Some(Instant::now());
            self.log.push((LogSeverity::Info, t!("release.waiting_save").to_owned()));
            return;
        }
        let Some((map, vmf)) = Self::map_of(state) else {
            state.notify(Severity::Warning, t!("release.err_not_saved"));
            return;
        };
        let Some(fs) = state.assets.fs().cloned() else {
            let key = if state.assets.status() == AssetStatus::Mounting {
                "release.err_mounting"
            } else {
                "release.err_no_content"
            };
            state.notify(Severity::Warning, t!(key));
            return;
        };
        let Some(out_root) = self.out_root(state) else { return };
        let kinds = key_kinds(state.assets.fgd());
        let (roots, instances) = collect_roots(state, &kinds);
        let settings = state.settings.content.clone();
        if settings.protect && settings.protect_randomize && self.side.protect_salt.is_empty() {
            let mut bytes = [0u8; 16];
            if getrandom::fill(&mut bytes).is_ok() {
                self.side.protect_salt = bytes.iter().map(|b| format!("{b:02x}")).collect();
                state.sidecar.set(&self.side);
            }
        }
        let compile = (!analyze_only && settings.compile_first).then(|| {
            let mut s = state.settings.compile.clone();
            s.preset = hp_app::settings::CompilePresetSetting::Final;
            s.lighting = hp_app::settings::LightingSetting::Both;
            s.run_bsp = true;
            s.run_vis = true;
            s.run_rad = true;
            s.pack_custom_content = false;
            s.copy_to_maps = false;
            s.launch_game = false;
            s.expert = false;
            s.set_active_compiler(hp_app::settings::compilers::BUILTIN_COMPILER);
            s.compile_cordon_only = false;
            s.progressive = false;
            s.smart_recompile = false;
            s.dev_addon = false;
            s.stop_on_leak = true;
            let mut materials: Vec<String> =
                state.doc.used_materials().into_iter().map(|(m, _)| crate::util::normalize_material(&m)).collect();
            materials.sort_unstable();
            materials.dedup();
            crate::compile::PrepInput {
                vmf: vmf.clone(),
                settings: s,
                gmod_root: fs.root().map(Path::to_path_buf).or_else(|| state.settings.game.gmod_root.clone()),
                fs: Some(fs.clone()),
                materials,
                models: Vec::new(),
                roots: Vec::new(),
                cache_dir: Settings::cache_dir().join("pack"),
                content_dir: Settings::cache_dir().join("compile_content"),
            }
        });
        let (thumb_png, icon_jpg) = match &self.thumb {
            Some(img) => (
                thumbnail::encode_png(&thumbnail::square(img, thumbnail::THUMB_SIZE)).ok(),
                thumbnail::encode_jpeg(&thumbnail::square(img, thumbnail::ICON_SIZE), 90).ok(),
            ),
            None => (None, None),
        };
        let addon = self.addon(&map);
        let input = ReleaseInput {
            addon_folder: self.addon_folder(&map),
            vmf: vmf.clone(),
            map: map.clone(),
            out_root,
            settings: settings.clone(),
            compile,
            gmod_root: fs.root().map(Path::to_path_buf).or_else(|| state.settings.game.gmod_root.clone()),
            fs,
            roots,
            instances,
            key_kinds: kinds,
            addon,
            thumb_png,
            icon_jpg,
            cache: self.deps_cache(&vmf),
            cache_dir: Settings::cache_dir().join("release").join(&map),
            game_session: !analyze_only && settings.game_session,
            nav_fingerprint: self.side.nav_fingerprint,
            workshop_id: self.side.workshop_id,
            protect_salt: self.side.protect_salt.clone(),
        };
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let repaint = ctx.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || repaint.request_repaint());
        let name = if analyze_only { "hp-release-analyze" } else { "hp-release" };
        let spawned = std::thread::Builder::new().name(name.into()).spawn(move || {
            if analyze_only {
                pipeline::analyze(input, tx, c2, wake);
            } else {
                pipeline::run(input, tx, c2, wake);
            }
        });
        match spawned {
            Ok(_) => {
                self.job = Some(Job { rx, cancel, analyze_only, serial: state.sidecar.serial() });
                self.log.clear();
                self.stages_done.clear();
                self.stage = None;
                self.progress = 0.0;
                self.compile_step = None;
                if !analyze_only {
                    self.summary = None;
                }
                state.runtime.content.progress = Some((t!("release.stage.dependencies").to_owned(), 0.0));
            }
            Err(e) => state.notify(Severity::Error, tf!("release.err_thread", e)),
        }
    }

    pub(crate) fn cancel(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
        if self.pending_build.take().is_some() {
            self.log.push((LogSeverity::Warning, t!("release.cancelled").to_owned()));
        }
    }

    fn poll(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if state.runtime.content.cancel_requested {
            state.runtime.content.cancel_requested = false;
            self.cancel();
        }
        if let Some(analyze_only) = state.runtime.content.start_requested.take() {
            state.runtime.content.release_open = true;
            self.sync_sidecar(state);
            self.start(ctx, state, analyze_only);
        }
        if std::mem::take(&mut state.runtime.content.checklist_requested) {
            self.sync_sidecar(state);
            self.refresh_facts(ctx, state, true);
        }
        if let Some(since) = self.pending_build {
            if state.doc.path().is_some() && !state.doc.is_dirty() {
                self.pending_build = None;
                self.start(ctx, state, false);
            } else if since.elapsed() > Duration::from_secs(120) {
                self.pending_build = None;
                self.log.push((LogSeverity::Error, t!("release.err_not_saved").to_owned()));
            } else {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }
        self.poll_job(state);
        self.poll_publish(state);
        self.poll_pickers(ctx, state);
        self.poll_capture(ctx, state);
        if self.job.is_some() || self.publishing.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn poll_job(&mut self, state: &mut AppState) {
        let Some(job) = &self.job else { return };
        let analyze_only = job.analyze_only;
        let same_map = job.serial == state.sidecar.serial();
        let mut events = Vec::new();
        let mut disconnected = false;
        for _ in 0..512 {
            match job.rx.try_recv() {
                Ok(e) => {
                    let done = matches!(e, ReleaseEvent::Done(_));
                    events.push(e);
                    if done {
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        let mut finished = None;
        for e in events {
            match e {
                ReleaseEvent::Stage(s) => {
                    if let Some(prev) = self.stage.replace(s)
                        && prev != s
                        && !self.stages_done.contains(&prev)
                    {
                        self.stages_done.push(prev);
                    }
                    self.progress = 0.0;
                }
                ReleaseEvent::Log(sev, text) => self.push_log(sev, text),
                ReleaseEvent::Progress(f) => self.progress = f.clamp(0.0, 1.0),
                ReleaseEvent::Graph(g) if same_map => {
                    state.runtime.content.graph = Some(g.clone());
                    self.graph = Some(g);
                    self.report.invalidate();
                    self.facts_key = None;
                }
                ReleaseEvent::NavFingerprint(fp) if same_map => {
                    self.side.nav_fingerprint = Some(fp);
                    state.sidecar.set(&self.side);
                }
                ReleaseEvent::Graph(_) | ReleaseEvent::NavFingerprint(_) => {}
                ReleaseEvent::Compile(ev) => self.compile_event(ev),
                ReleaseEvent::Done(result) => finished = Some(*result),
            }
        }
        let label = self.stage.map(Stage::label).unwrap_or_default().to_owned();
        state.runtime.content.progress = Some((label, self.progress));
        if disconnected && finished.is_none() {
            finished = Some(Err(t!("release.err_crashed").to_owned()));
        }
        let Some(result) = finished else { return };
        self.job = None;
        state.runtime.content.progress = None;
        if let Some(s) = self.stage.take()
            && !self.stages_done.contains(&s)
        {
            self.stages_done.push(s);
        }
        self.facts_key = None;
        if analyze_only {
            if let Err(e) = &result {
                self.push_log(LogSeverity::Error, e.clone());
            }
            return;
        }
        match &result {
            Ok(s) => {
                let files = s.pak_files + s.addon_files;
                if same_map {
                    self.side.last = Some(sidecar::LastRelease {
                        time: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                        files,
                        bytes: s.pak_bytes + s.addon_bytes,
                        output: s.out_root.clone(),
                    });
                    state.sidecar.set(&self.side);
                }
                state.notify(Severity::Success, tf!("release.done_toast", s.out_root.display()));
                state.log(
                    Severity::Success,
                    tf!("release.done_log", files, pipeline::fmt_bytes(s.pak_bytes + s.addon_bytes)),
                );
                if state.settings.content.open_folder {
                    open_folder(&s.out_root);
                }
            }
            Err(e) => {
                self.push_log(LogSeverity::Error, e.clone());
                state.notify(Severity::Error, e.clone());
            }
        }
        if same_map {
            state.runtime.content.last_release = Some(self.report_of(&result));
            self.summary = Some(result);
        }
    }

    fn report_of(&self, result: &Result<ReleaseSummary, String>) -> hp_app::runtime::ReleaseReport {
        let log_tail = self.log.iter().rev().take(40).rev().map(|(_, l)| l.clone()).collect();
        match result {
            Ok(s) => hp_app::runtime::ReleaseReport {
                error: None,
                out_root: Some(s.out_root.clone()),
                addon_dir: Some(s.addon_dir.clone()),
                gma: s.gma.clone(),
                files: s.pak_files + s.addon_files,
                bytes: s.pak_bytes + s.addon_bytes,
                gma_bytes: s.gma_bytes,
                duration_ms: s.duration_ms,
                protected_server_bsp: s.protected.as_ref().map(|p| p.server_bsp.clone()),
                log_tail,
            },
            Err(e) => hp_app::runtime::ReleaseReport { error: Some(e.clone()), log_tail, ..Default::default() },
        }
    }

    fn push_log(&mut self, sev: LogSeverity, text: String) {
        if self.log.len() < 100_000 {
            self.log.push((sev, text));
        }
    }

    fn compile_event(&mut self, ev: CompileEvent) {
        match ev {
            CompileEvent::StepStarted { kind, .. } => {
                self.compile_step = Some((kind, 0.0));
                self.push_log(LogSeverity::Info, tf!("release.compile_step", crate::compile::step_name(kind)));
            }
            CompileEvent::Progress { fraction, .. } => {
                if let Some((_, f)) = &mut self.compile_step {
                    *f = fraction;
                }
                self.progress = fraction;
            }
            CompileEvent::Log { line, severity, .. } => {
                if severity != LogSeverity::Info || self.log.len() < 20_000 {
                    self.push_log(severity, line);
                }
            }
            CompileEvent::Notice { notice, .. } => {
                self.push_log(notice.severity(), crate::compile::notice_text(&notice))
            }
            CompileEvent::LeakDetected { .. } => self.push_log(LogSeverity::Error, t!("release.err_leak").to_owned()),
            CompileEvent::StepFinished { success: false, .. } => {
                self.push_log(LogSeverity::Error, t!("release.err_compile").to_owned());
            }
            _ => {}
        }
    }

    fn poll_publish(&mut self, state: &mut AppState) {
        let Some(job) = &self.publishing else { return };
        let mut done = None;
        loop {
            match job.rx.try_recv() {
                Ok(publish::PublishEvent::Log(sev, line)) => {
                    if self.publish_log.len() < 50_000 {
                        self.publish_log.push((sev, line));
                    }
                }
                Ok(publish::PublishEvent::Done { success, id }) => {
                    done = Some((success, id));
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    done = Some((false, None));
                    break;
                }
            }
        }
        let Some((success, id)) = done else { return };
        self.publishing = None;
        if let Some(id) = id
            && success
        {
            self.side.workshop_id = Some(id);
            self.save_sidecar(state);
        }
        if success {
            state.notify(Severity::Success, t!("release.publish_done"));
        } else {
            state.notify(Severity::Error, t!("release.publish_failed"));
        }
    }

    fn poll_pickers(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(rx) = &self.pick_folder {
            match rx.try_recv() {
                Ok(dir) => {
                    self.pick_folder = None;
                    if let Some(dir) = dir {
                        self.side.output_dir = Some(dir);
                        self.save_sidecar(state);
                        self.facts_key = None;
                    }
                }
                Err(TryRecvError::Disconnected) => self.pick_folder = None,
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(150)),
            }
        }
        if let Some(rx) = &self.pick_image {
            match rx.try_recv() {
                Ok(path) => {
                    self.pick_image = None;
                    if let Some(path) = path {
                        let (tx, rx) = mpsc::channel();
                        let repaint = ctx.clone();
                        let _ = std::thread::Builder::new().name("hp-release-image".into()).spawn(move || {
                            let _ = tx
                                .send(thumbnail::load_file(&path).map(|i| thumbnail::square(&i, thumbnail::ICON_SIZE)));
                            repaint.request_repaint();
                        });
                        self.image_job = Some(rx);
                    }
                }
                Err(TryRecvError::Disconnected) => self.pick_image = None,
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(150)),
            }
        }
        if let Some(rx) = &self.image_job {
            match rx.try_recv() {
                Ok(Ok(img)) => {
                    self.image_job = None;
                    self.set_thumb(img);
                }
                Ok(Err(e)) => {
                    self.image_job = None;
                    state.notify(Severity::Warning, tf!("release.image_failed", e));
                }
                Err(TryRecvError::Disconnected) => self.image_job = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.facts_job {
            match rx.try_recv() {
                Ok((facts, thumb)) => {
                    self.facts_job = None;
                    if self.thumb.is_none()
                        && let Some(t) = thumb
                    {
                        self.set_thumb(t);
                    }
                    let rows = checklist::rows(&facts);
                    state.runtime.content.checklist = Some(rows.iter().map(checklist::Row::shared).collect());
                    self.rows = Some(rows);
                    self.facts = Some(facts);
                }
                Err(TryRecvError::Disconnected) => self.facts_job = None,
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    fn poll_capture(&mut self, _ctx: &egui::Context, state: &mut AppState) {
        match std::mem::take(&mut state.runtime.content.capture) {
            CaptureState::Ready(img) => {
                match thumbnail::from_rgba(img.width, img.height, img.rgba) {
                    Some(full) => self.set_thumb(thumbnail::square(&full, thumbnail::ICON_SIZE)),
                    None => state.notify(Severity::Warning, t!("release.capture_failed")),
                }
                state.runtime.content.release_open = true;
            }
            CaptureState::Failed => state.notify(Severity::Warning, t!("release.capture_failed")),
            other => state.runtime.content.capture = other,
        }
    }

    fn set_thumb(&mut self, img: RgbaImage) {
        self.thumb = Some(img);
        self.thumb_tex = None;
        self.thumb_dirty = true;
        self.facts_key = None;
    }

    fn refresh_facts(&mut self, ctx: &egui::Context, state: &AppState, force: bool) {
        let key = (state.sidecar.serial(), 0);
        if self.facts_job.is_some() || (!force && self.facts_key == Some(key)) || self.job.is_some() {
            return;
        }
        self.facts_key = Some(key);
        let map_vmf = Self::map_of(state);
        let map = map_vmf.as_ref().map(|m| m.0.clone()).unwrap_or_else(|| state.map_title().to_ascii_lowercase());
        let out_root = self.out_root(state);
        let folder = self.addon_folder(&map);
        let env_cubemaps = state
            .doc
            .scene()
            .entities()
            .values()
            .filter(|n| n.entity.classname.eq_ignore_ascii_case("env_cubemap"))
            .count();
        let mut classes: Vec<String> =
            state.doc.scene().entities().values().map(|n| n.entity.classname.to_ascii_lowercase()).collect();
        classes.sort_unstable();
        classes.dedup();
        let input = checklist::GatherInput {
            vmf: map_vmf.map(|m| m.1),
            map: map.clone(),
            out_bsp: out_root.as_ref().map(|o| o.join(&folder).join("maps").join(format!("{map}.bsp"))),
            out_icon: out_root.as_ref().map(|o| o.join(format!("{folder}_icon.jpg"))),
            thumbnail: self.thumb.is_some(),
            addon: self.addon(&map),
            fs: state.assets.fs().cloned(),
            nav_fingerprint: self.side.nav_fingerprint,
            missing_deps: self.graph.as_ref().map(|g| g.missing.len()),
            env_cubemaps,
            classes,
            gmod_root: state
                .assets
                .fs()
                .and_then(|fs| fs.root().map(Path::to_path_buf))
                .or_else(|| state.settings.game.gmod_root.clone()),
            out_gma: out_root.as_ref().map(|o| o.join(format!("{folder}.gma"))),
        };
        let thumb_path = out_root.map(|o| o.join(&folder).join("maps").join("thumb").join(format!("{map}.png")));
        let want_thumb = self.thumb.is_none();
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-release-check".into()).spawn(move || {
            let thumb = if want_thumb {
                thumb_path
                    .and_then(|p| thumbnail::load_file(&p).ok())
                    .map(|i| thumbnail::square(&i, thumbnail::ICON_SIZE))
            } else {
                None
            };
            let mut facts = checklist::gather(input);
            facts.thumbnail |= thumb.is_some();
            let _ = tx.send((facts, thumb));
            repaint.request_repaint();
        });
        if spawned.is_ok() {
            self.facts_job = Some(rx);
        }
    }

    fn window_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        if !state.settings.game.is_gmod() {
            ui.label(t!("release.gmod_only"));
            return;
        }
        if self.thumb_dirty || (self.thumb.is_some() && self.thumb_tex.is_none()) {
            self.thumb_dirty = false;
            if let Some(img) = &self.thumb {
                let small = thumbnail::square(img, thumbnail::THUMB_SIZE);
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [small.width() as usize, small.height() as usize],
                    small.as_raw(),
                );
                self.thumb_tex = Some(ui.ctx().load_texture("hp_release_thumb", color, egui::TextureOptions::LINEAR));
            }
        }
        if matches!(self.tab, Tab::Checklist | Tab::Publish) || self.rows.is_none() {
            self.refresh_facts(ui.ctx(), state, false);
        }
        crate::util::dialog_body(ui, |ui| self.body_ui(ui, state));
    }

    fn body_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let tab_before = self.tab;
        ui.horizontal(|ui| {
            for (tab, key) in [
                (Tab::Build, "release.tab_build"),
                (Tab::Report, "release.tab_report"),
                (Tab::Checklist, "release.tab_checklist"),
                (Tab::Addon, "release.tab_addon"),
                (Tab::Publish, "release.tab_publish"),
            ] {
                ui.selectable_value(&mut self.tab, tab, t!(key));
            }
            if let Some(rows) = &self.rows {
                ui.separator();
                let fails = rows.iter().filter(|r| r.status == checklist::Status::Fail).count();
                if checklist::ready(rows) {
                    ui.colored_label(hp_app::theme::success(), t!("release.ready"));
                } else {
                    ui.colored_label(hp_app::theme::error(), tf!("release.not_ready", fails));
                }
            }
        });
        ui.separator();
        if self.tab != tab_before && matches!(self.tab, Tab::Checklist | Tab::Publish) {
            self.facts_key = None;
        }
        match self.tab {
            Tab::Build => self.build_tab(ui, state),
            Tab::Report => self.report_tab(ui, state),
            Tab::Checklist => self.checklist_tab(ui, state),
            Tab::Addon => self.addon_tab(ui, state),
            Tab::Publish => self.publish_tab(ui, state),
        }
    }
}

pub(crate) fn open_folder(path: &Path) {
    hp_app::os::open_folder(path, false);
}

#[cfg(test)]
mod tests;
