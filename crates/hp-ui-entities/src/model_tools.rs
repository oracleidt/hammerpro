use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{RichText, Ui};
use hp_app::{AppState, Severity, t, tf};
use hp_compile::{CompileEvent, CompileHandle, CompilePlan, Step, StepKind};

const MAX_FILE_BYTES: usize = 512 << 20;
const EVENTS_PER_FRAME: usize = 500;
const CROWBAR_DIR: &str = "hammerpro_models";

pub(crate) fn model_files(fs: &hp_fs::GameFs, mdl: &str) -> Vec<String> {
    let (vvd, vtx) = hp_mdl::sibling_paths(mdl);
    let base = vvd.strip_suffix(".vvd").unwrap_or(&vvd).to_owned();
    let mut out = vec![mdl.to_owned(), vvd];
    out.extend(vtx);
    out.push(format!("{base}.phy"));
    out.push(format!("{base}.ani"));
    out.retain(|p| fs.exists(p));
    out
}

pub(crate) fn destination(dir: &Path, vpath: &str) -> Option<PathBuf> {
    let mut out = dir.to_path_buf();
    let mut any = false;
    for part in vpath.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(':') {
            return None;
        }
        out.push(part);
        any = true;
    }
    any.then_some(out)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Extracted {
    pub mdl: PathBuf,
    pub files: usize,
}

pub(crate) fn extract(fs: &hp_fs::GameFs, mdl: &str, dir: &Path) -> Result<Extracted, String> {
    let files = model_files(fs, mdl);
    if files.first().map(String::as_str) != Some(mdl) {
        return Err(t!("entities.inspect_not_found").to_owned());
    }
    let mut out = None;
    for f in &files {
        let dest = destination(dir, f).ok_or_else(|| tf!("entities.mt_bad_path", f))?;
        let bytes = fs.read(f).ok_or_else(|| tf!("entities.mt_unreadable", f))?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(tf!("entities.mt_unreadable", f));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&dest, &bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
        out.get_or_insert(dest);
    }
    let mdl = out.ok_or_else(|| t!("entities.inspect_not_found").to_owned())?;
    Ok(Extracted { mdl, files: files.len() })
}

pub fn studiomdl_exe(gmod_root: &Path) -> Option<PathBuf> {
    let bin = gmod_root.join("bin");
    [bin.join("win64").join("studiomdl.exe"), bin.join("studiomdl.exe")].into_iter().find(|p| p.is_file())
}

pub fn studiomdl_plan(exe: &Path, game_dir: &Path, qc: &Path) -> CompilePlan {
    let args = vec![
        "-game".to_owned(),
        game_dir.to_string_lossy().into_owned(),
        "-nop4".to_owned(),
        "-verbose".to_owned(),
        qc.to_string_lossy().into_owned(),
    ];
    let mut plan = CompilePlan::new(qc);
    plan.steps.push(Step::new(StepKind::Custom, exe, args));
    plan
}

fn severity(s: hp_compile::Severity) -> Severity {
    match s {
        hp_compile::Severity::Info => Severity::Info,
        hp_compile::Severity::Success => Severity::Success,
        hp_compile::Severity::Warning => Severity::Warning,
        hp_compile::Severity::Error => Severity::Error,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Ask {
    ExtractTo(String),
    Crowbar(String),
    Qc,
}

type ExtractResult = Result<(Extracted, Option<Result<(), String>>), String>;

enum Job {
    Extract { model: String, crowbar: bool, rx: Receiver<ExtractResult> },
    Compile { qc: PathBuf, handle: CompileHandle },
}

#[derive(Default)]
pub(crate) struct ModelTools {
    dialog: Option<(Ask, Receiver<Option<PathBuf>>)>,
    job: Option<Job>,
    studiomdl: Option<(Option<PathBuf>, Option<PathBuf>)>,
}

impl ModelTools {
    pub fn busy(&self) -> bool {
        self.dialog.is_some() || self.job.is_some()
    }

    fn studiomdl(&mut self, state: &AppState) -> Option<PathBuf> {
        let root = state.settings.game.gmod_root.clone();
        if self.studiomdl.as_ref().is_none_or(|(r, _)| *r != root) {
            let exe = root.as_deref().and_then(studiomdl_exe);
            self.studiomdl = Some((root, exe));
        }
        self.studiomdl.as_ref().and_then(|(_, exe)| exe.clone())
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState, model: &str) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx, state);
        let busy = self.busy();
        let studiomdl = self.studiomdl(state);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("action.show_shape_preview")).on_hover_text(t!("entities.mt_shape_tip")).clicked() {
                let rt = &mut state.runtime.placement;
                rt.shape_preview_model = Some(model.to_owned());
                rt.shape_preview_focus_model = true;
                if !rt.shape_preview_open {
                    state.request(hp_app::Action::ShowShapePreview);
                }
            }
            let extract = ui.add_enabled(!busy, egui::Button::new(t!("entities.mt_extract")));
            if extract.on_hover_text(t!("entities.mt_extract_tip")).clicked() {
                let title = t!("entities.mt_pick_folder").to_owned();
                self.dialog = Some((Ask::ExtractTo(model.to_owned()), spawn_dialog(&ctx, title, None)));
            }
            let crowbar = ui.add_enabled(!busy, egui::Button::new(t!("entities.mt_crowbar")));
            let crowbar = crowbar.on_hover_text(t!("entities.mt_crowbar_tip"));
            let mut change = false;
            crowbar.context_menu(|ui| {
                if ui.button(t!("entities.mt_change_crowbar")).clicked() {
                    change = true;
                    ui.close();
                }
            });
            if crowbar.clicked() || change {
                let known = state.settings.entities.crowbar_exe.clone().filter(|p| p.is_file());
                match known {
                    Some(exe) if !change => self.start_extract(&ctx, state, model, Some(exe)),
                    _ => {
                        let title = t!("entities.mt_pick_crowbar").to_owned();
                        let filter = Some((t!("entities.mt_programs").to_owned(), "exe"));
                        self.dialog = Some((Ask::Crowbar(model.to_owned()), spawn_dialog(&ctx, title, filter)));
                    }
                }
            }
            let compile = ui.add_enabled(!busy && studiomdl.is_some(), egui::Button::new(t!("entities.mt_compile")));
            let compile = compile
                .on_hover_text(t!("entities.mt_compile_tip"))
                .on_disabled_hover_text(t!("entities.mt_no_studiomdl"));
            if compile.clicked() {
                let title = t!("entities.mt_pick_qc").to_owned();
                let filter = Some((t!("entities.mt_qc_files").to_owned(), "qc"));
                self.dialog = Some((Ask::Qc, spawn_dialog(&ctx, title, filter)));
            }
        });
        let status = match &self.job {
            Some(Job::Extract { .. }) => Some(t!("entities.mt_extracting").to_owned()),
            Some(Job::Compile { qc, .. }) => Some(tf!("entities.mt_compiling", file_name(qc))),
            None => None,
        };
        if let Some(text) = status {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(RichText::new(text).weak());
                if let Some(Job::Compile { handle, .. }) = &self.job
                    && ui.small_button(t!("common.cancel")).clicked()
                {
                    handle.cancel();
                }
            });
        }
    }

    fn poll(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some((ask, rx)) = &self.dialog {
            match rx.try_recv() {
                Ok(picked) => {
                    let ask = ask.clone();
                    self.dialog = None;
                    if let Some(path) = picked {
                        self.picked(ctx, state, ask, path);
                    }
                }
                Err(TryRecvError::Disconnected) => self.dialog = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        match &mut self.job {
            Some(Job::Extract { model, crowbar, rx }) => match rx.try_recv() {
                Ok(result) => {
                    let (model, crowbar) = (model.clone(), *crowbar);
                    self.job = None;
                    report_extract(state, &model, crowbar, result);
                }
                Err(TryRecvError::Disconnected) => self.job = None,
                Err(TryRecvError::Empty) => {}
            },
            Some(Job::Compile { qc, handle }) => {
                let name = file_name(qc);
                let mut finished = None;
                for _ in 0..EVENTS_PER_FRAME {
                    match handle.events.try_recv() {
                        Ok(CompileEvent::StepStarted { command_line, .. }) => state.console.info(command_line),
                        Ok(CompileEvent::Log { line, severity: s, .. }) => state.console.push(severity(s), line),
                        Ok(CompileEvent::Finished { success, cancelled, .. }) => finished = Some((success, cancelled)),
                        Ok(_) => {}
                        Err(e) => {
                            if e.is_disconnected() {
                                finished.get_or_insert((false, handle.is_cancelled()));
                            }
                            break;
                        }
                    }
                }
                if let Some((success, cancelled)) = finished {
                    self.job = None;
                    if cancelled {
                        state.notify(Severity::Info, tf!("entities.mt_cancelled", name));
                    } else if success {
                        state.notify(Severity::Success, tf!("entities.mt_compiled", name));
                    } else {
                        state.notify(Severity::Error, tf!("entities.mt_compile_failed", name));
                    }
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
            None => {}
        }
    }

    fn picked(&mut self, ctx: &egui::Context, state: &mut AppState, ask: Ask, path: PathBuf) {
        match ask {
            Ask::ExtractTo(model) => self.start_extract_to(ctx, state, &model, path, None),
            Ask::Crowbar(model) => {
                state.settings.entities.crowbar_exe = Some(path.clone());
                state.settings_dirty = true;
                self.start_extract(ctx, state, &model, Some(path));
            }
            Ask::Qc => self.start_compile(ctx, state, path),
        }
    }

    fn start_extract(&mut self, ctx: &egui::Context, state: &mut AppState, model: &str, crowbar: Option<PathBuf>) {
        let dir = std::env::temp_dir().join(CROWBAR_DIR);
        self.start_extract_to(ctx, state, model, dir, crowbar);
    }

    fn start_extract_to(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        model: &str,
        dir: PathBuf,
        crowbar: Option<PathBuf>,
    ) {
        if crowbar.is_some() && hp_compile::sandbox::is_sandboxed() {
            state.notify(Severity::Warning, t!("entities.mt_sandboxed"));
            return;
        }
        let Some(fs) = state.assets.fs().cloned() else {
            state.notify(Severity::Warning, t!("entities.inspect_not_found"));
            return;
        };
        let (tx, rx) = channel();
        let (repaint, mdl) = (ctx.clone(), model.to_owned());
        let opens = crowbar.is_some();
        let spawned = std::thread::Builder::new().name("hp-model-extract".to_owned()).spawn(move || {
            let result = extract(&fs, &mdl, &dir).map(|done| {
                let started = crowbar.map(|exe| {
                    std::process::Command::new(&exe).arg(&done.mdl).spawn().map(drop).map_err(|e| e.to_string())
                });
                (done, started)
            });
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => self.job = Some(Job::Extract { model: model.to_owned(), crowbar: opens, rx }),
            Err(e) => state.notify(Severity::Error, tf!("entities.mt_failed", e)),
        }
    }

    fn start_compile(&mut self, ctx: &egui::Context, state: &mut AppState, qc: PathBuf) {
        if hp_compile::sandbox::is_sandboxed() {
            state.notify(Severity::Warning, t!("entities.mt_sandboxed"));
            return;
        }
        let Some(exe) = self.studiomdl(state) else {
            state.notify(Severity::Warning, t!("entities.mt_no_studiomdl"));
            return;
        };
        let root = state.settings.game.gmod_root.clone().unwrap_or_default();
        let game_dir = state
            .assets
            .fs()
            .and_then(|f| f.game_dir().map(Path::to_path_buf))
            .unwrap_or_else(|| root.join("garrysmod"));
        self.compile_with(ctx, state, &exe, &game_dir, qc);
    }

    fn compile_with(&mut self, ctx: &egui::Context, state: &mut AppState, exe: &Path, game_dir: &Path, qc: PathBuf) {
        let plan = studiomdl_plan(exe, game_dir, &qc);
        let workdir = qc.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let repaint = ctx.clone();
        let handle = hp_compile::start_with_waker(plan, &workdir, move || repaint.request_repaint());
        state.console.info(tf!("entities.mt_compiling", file_name(&qc)));
        self.job = Some(Job::Compile { qc, handle });
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn report_extract(state: &mut AppState, model: &str, crowbar: bool, result: ExtractResult) {
    match result {
        Err(e) => state.notify(Severity::Error, tf!("entities.mt_failed", e)),
        Ok((done, started)) => {
            let folder = done.mdl.parent().map(Path::display).map(|d| d.to_string()).unwrap_or_default();
            state.console.info(tf!("entities.mt_extracted", done.files, folder));
            match started {
                Some(Ok(())) => state.notify(Severity::Success, tf!("entities.mt_crowbar_started", model)),
                Some(Err(e)) => state.notify(Severity::Error, tf!("entities.mt_crowbar_failed", e)),
                None if crowbar => {}
                None => state.notify(Severity::Success, tf!("entities.mt_extracted", done.files, folder)),
            }
        }
    }
}

fn spawn_dialog(
    ctx: &egui::Context,
    title: String,
    filter: Option<(String, &'static str)>,
) -> Receiver<Option<PathBuf>> {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("hp-model-tools-dialog".to_owned()).spawn(move || {
        let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
        let picked = match filter {
            Some((name, ext)) => {
                dialog = dialog.add_filter(name, &[ext]);
                pollster::block_on(dialog.pick_file())
            }
            None => pollster::block_on(dialog.pick_folder()),
        };
        let _ = tx.send(picked.map(|h| h.path().to_path_buf()));
        ctx.request_repaint();
    });
    if let Err(e) = spawned {
        log::warn!("could not start the file dialog thread: {e}");
    }
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str) -> (PathBuf, hp_fs::GameFs) {
        let dir = std::env::temp_dir().join(format!("hp_model_tools_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let models = dir.join("game").join("models").join("zz");
        std::fs::create_dir_all(&models).expect("dir");
        for (file, bytes) in
            [("box.mdl", b"IDST".as_slice()), ("box.vvd", b"IDSV"), ("box.dx90.vtx", b"7"), ("box.phy", b"VPHY")]
        {
            std::fs::write(models.join(file), bytes).expect("write");
        }
        let fs = hp_fs::GameFs::empty();
        assert!(fs.add_game_dir(&dir.join("game")).is_ok());
        (dir, fs)
    }

    #[test]
    fn destinations_never_leave_the_folder() {
        let dir = Path::new("C:/out");
        assert_eq!(destination(dir, "models/a/b.mdl"), Some(dir.join("models").join("a").join("b.mdl")));
        assert_eq!(destination(dir, "models\\.\\b.mdl"), Some(dir.join("models").join("b.mdl")));
        for bad in ["", "/", "../x.mdl", "models/../../x.mdl", "C:/x.mdl", "models/c:x"] {
            assert_eq!(destination(dir, bad), None, "{bad}");
        }
    }

    #[test]
    fn extraction_copies_every_file_of_the_model() {
        crate::register_translations();
        let (dir, fs) = game("extract");
        assert_eq!(
            model_files(&fs, "models/zz/box.mdl"),
            ["models/zz/box.mdl", "models/zz/box.vvd", "models/zz/box.dx90.vtx", "models/zz/box.phy"]
        );
        let out = dir.join("out");
        let done = extract(&fs, "models/zz/box.mdl", &out).expect("extracted");
        assert_eq!(done.files, 4);
        assert_eq!(done.mdl, out.join("models").join("zz").join("box.mdl"));
        assert_eq!(std::fs::read(out.join("models/zz/box.phy")).ok().as_deref(), Some(b"VPHY".as_slice()));
        assert!(extract(&fs, "models/zz/none.mdl", &out).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn studiomdl_is_found_and_planned() {
        let dir = std::env::temp_dir().join(format!("hp_model_tools_bin_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin").join("win64")).expect("dir");
        assert_eq!(studiomdl_exe(&dir), None);
        std::fs::write(dir.join("bin").join("studiomdl.exe"), b"").expect("write");
        assert_eq!(studiomdl_exe(&dir), Some(dir.join("bin").join("studiomdl.exe")));
        std::fs::write(dir.join("bin").join("win64").join("studiomdl.exe"), b"").expect("write");
        assert_eq!(studiomdl_exe(&dir), Some(dir.join("bin").join("win64").join("studiomdl.exe")));
        let plan = studiomdl_plan(Path::new("S:/studiomdl.exe"), Path::new("G:/garrysmod"), Path::new("Q:/m/a.qc"));
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].kind, StepKind::Custom);
        assert_eq!(plan.steps[0].args, ["-game", "G:/garrysmod", "-nop4", "-verbose", "Q:/m/a.qc"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn finish(tools: &mut ModelTools, state: &mut AppState) {
        let ctx = egui::Context::default();
        for _ in 0..300 {
            tools.poll(&ctx, state);
            if tools.job.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the job never ended");
    }

    #[test]
    fn jobs_report_their_end() {
        crate::register_translations();
        let (dir, fs) = game("jobs");
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let mut tools = ModelTools::default();
        tools.start_extract_to(&ctx, &mut state, "models/zz/box.mdl", dir.join("a"), None);
        assert!(tools.job.is_none());
        state.assets = Box::new(crate::tests::FsAssets { inner: hp_app::testing::MockAssets::default(), fs });
        let missing = dir.join("no_crowbar.exe");
        tools.start_extract_to(&ctx, &mut state, "models/zz/box.mdl", dir.join("b"), Some(missing));
        finish(&mut tools, &mut state);
        assert!(dir.join("b/models/zz/box.vvd").is_file());
        assert_eq!(state.toasts.live().last().map(|t| t.severity), Some(Severity::Error));
        let qc = dir.join("box.qc");
        std::fs::write(&qc, "$modelname zz/box.mdl\n").expect("qc");
        tools.compile_with(&ctx, &mut state, &dir.join("no_studiomdl.exe"), &dir, qc);
        assert!(tools.busy());
        finish(&mut tools, &mut state);
        let last = state.toasts.live().last().map(|t| (t.severity, t.text.clone()));
        assert_eq!(last.map(|l| l.0), Some(Severity::Error));
        hp_app::testing::run_ui(&mut state, |ui, state| tools.ui(ui, state, "models/zz/box.mdl"));
        assert!(!tools.busy(), "drawing opens nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
