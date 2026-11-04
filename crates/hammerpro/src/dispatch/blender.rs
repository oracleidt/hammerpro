use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use hp_app::i18n::TrTable;
use hp_app::{Action, Severity, t, tf};
use hp_ui_entities::model_import::ModelBatchDialog;

use super::Package;
use crate::app::HammerProApp;
use crate::blender::{self, SendOptions, pull};

pub(crate) struct Bridge;

type PullAnswer = Result<pull::Outcome, String>;

#[derive(Default)]
pub(crate) struct State {
    batch: ModelBatchDialog,
    pull: Option<Receiver<PullAnswer>>,
    browse: Option<Receiver<Option<PathBuf>>>,
}

const HANDLED: &[Action] = &[
    Action::ImportModelsBatch,
    Action::SendToBlender,
    Action::PullFromBlender,
    Action::StopBlenderLink,
    Action::InstallBlenderAddon,
];

pub(crate) fn start_pull(app: &mut HammerProApp, ctx: &egui::Context) {
    if app.state.runtime.blender.pull_in_progress || app.pkg.blender.pull.is_some() {
        app.state.notify(Severity::Warning, t!("blender.busy"));
        return;
    }
    let job = match pull::job_of(&app.state) {
        Ok(j) => j,
        Err(e) => {
            app.state.notify(Severity::Warning, e);
            return;
        }
    };
    let (tx, rx) = channel();
    let repaint = ctx.clone();
    let spawned = std::thread::Builder::new().name("hp-blender-pull".into()).spawn(move || {
        let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pull::compute(job)))
            .unwrap_or_else(|_| Err("internal error".to_owned()));
        let _ = tx.send(answer);
        repaint.request_repaint();
    });
    if spawned.is_err() {
        app.state.notify(Severity::Error, tf!("blender.pull_failed", "cannot start a thread"));
        return;
    }
    app.state.runtime.blender.pull_in_progress = true;
    app.pkg.blender.pull = Some(rx);
}

fn finish_pull(app: &mut HammerProApp, answer: PullAnswer) {
    app.state.runtime.blender.pull_in_progress = false;
    let result = answer.and_then(|outcome| pull::apply(&mut app.state, outcome));
    match result {
        Ok(report) => {
            for w in &report.warnings {
                app.state.log(Severity::Warning, w.clone());
            }
            app.state.runtime.blender.note = Some(hp_app::runtime::BlenderNote { ok: true, text: report.text });
        }
        Err(e) => {
            let text = tf!("blender.pull_failed", e);
            app.state.notify(Severity::Error, text.clone());
            app.state.runtime.blender.note = Some(hp_app::runtime::BlenderNote { ok: false, text });
        }
    }
}

fn send_now(app: &mut HammerProApp) {
    let opts = SendOptions { format: None, launch: true, live: None };
    match blender::send(&mut app.state, &opts) {
        Ok(report) => {
            let severity = if report.problem.is_some() { Severity::Warning } else { Severity::Success };
            let text = app.state.runtime.blender.note.as_ref().map(|n| n.text.clone()).unwrap_or_default();
            app.state.notify(severity, text);
        }
        Err(e) => app.state.notify(Severity::Warning, e),
    }
}

fn pick_blender(app: &mut HammerProApp, ctx: &egui::Context) {
    if app.pkg.blender.browse.is_some() {
        return;
    }
    let (tx, rx) = channel();
    let repaint = ctx.clone();
    let mut dialog = rfd::FileDialog::new().set_title(t!("blender.pick_title"));
    if cfg!(windows) {
        dialog = dialog.add_filter("Blender", &["exe"]);
    }
    std::thread::spawn(move || {
        let _ = tx.send(dialog.pick_file());
        repaint.request_repaint();
    });
    app.pkg.blender.browse = Some(rx);
}

fn poll_browse(app: &mut HammerProApp) {
    let Some(rx) = &app.pkg.blender.browse else { return };
    match rx.try_recv() {
        Ok(Some(path)) => {
            app.pkg.blender.browse = None;
            if blender::find::is_blender_exe(&path) {
                app.state.settings.blender.path = path.display().to_string();
                app.state.settings_dirty = true;
                app.state.notify(Severity::Success, tf!("blender.path_saved", path.display()));
                if app.state.runtime.blender.missing_open {
                    match blender::relaunch(&mut app.state) {
                        Ok(()) => app.state.runtime.blender.missing_open = false,
                        Err(e) => app.state.notify(Severity::Error, e),
                    }
                }
            } else {
                app.state.notify(Severity::Warning, tf!("blender.wrong_exe", path.display()));
            }
        }
        Ok(None) | Err(TryRecvError::Disconnected) => app.pkg.blender.browse = None,
        Err(TryRecvError::Empty) => {}
    }
}

fn missing_dialog(app: &mut HammerProApp, ctx: &egui::Context) {
    if !app.state.runtime.blender.missing_open {
        return;
    }
    let mut open = true;
    let file = app.state.runtime.blender.link.as_ref().map(|l| l.file.display().to_string());
    let (mut browse, mut retry, mut close) = (false, false, false);
    hp_app::chrome::Window::new(t!("blender.missing_title"))
        .id(egui::Id::unique("hp_blender_missing"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(t!("blender.missing_body"));
            if let Some(f) = &file {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(tf!("blender.exported_to", f)).small().weak());
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                browse = ui.button(t!("blender.browse")).clicked();
                retry = ui.button(t!("blender.retry")).clicked();
                close = ui.button(t!("blender.close")).clicked();
            });
        });
    if browse {
        pick_blender(app, ctx);
    }
    if retry {
        match blender::relaunch(&mut app.state) {
            Ok(()) => app.state.runtime.blender.missing_open = false,
            Err(e) => app.state.notify(Severity::Warning, e),
        }
    }
    if close || !open {
        app.state.runtime.blender.missing_open = false;
    }
}

impl Package for Bridge {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::ImportModelsBatch => app.pkg.blender.batch.open(),
            Action::SendToBlender => app.state.runtime.blender.send_requested = true,
            Action::PullFromBlender => app.state.runtime.blender.pull_requested = true,
            Action::StopBlenderLink => app.state.runtime.blender.stop_requested = true,
            Action::InstallBlenderAddon => app.state.runtime.blender.install_requested = true,
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        let rt = &mut app.state.runtime.blender;
        let (send, pull_req, stop, install, browse) = (
            std::mem::take(&mut rt.send_requested),
            std::mem::take(&mut rt.pull_requested),
            std::mem::take(&mut rt.stop_requested),
            std::mem::take(&mut rt.install_requested),
            std::mem::take(&mut rt.browse_requested),
        );
        if send {
            send_now(app);
        }
        if pull_req {
            start_pull(app, ctx);
        }
        if stop && !blender::stop(&mut app.state) {
            app.state.notify(Severity::Info, t!("blender.no_link"));
        }
        if install {
            match blender::install_addon(&mut app.state) {
                Ok(_) => {
                    let text = app.state.runtime.blender.note.as_ref().map(|n| n.text.clone()).unwrap_or_default();
                    app.state.notify(Severity::Success, text);
                }
                Err(e) => app.state.notify(Severity::Error, e),
            }
        }
        if browse {
            pick_blender(app, ctx);
        }
        if std::mem::take(&mut app.state.runtime.blender.detect_requested) {
            blender::resolve_exe(&mut app.state);
        }
        poll_browse(app);
        if let Some(rx) = &app.pkg.blender.pull {
            match rx.try_recv() {
                Ok(answer) => {
                    app.pkg.blender.pull = None;
                    finish_pull(app, answer);
                }
                Err(TryRecvError::Disconnected) => {
                    app.pkg.blender.pull = None;
                    app.state.runtime.blender.pull_in_progress = false;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if app.state.runtime.blender.link.is_some() {
            ctx.request_repaint_after(blender::CHECK_EVERY);
            if app.pkg.blender.pull.is_none() && blender::watch_due(&mut app.state, std::time::Instant::now()) {
                start_pull(app, ctx);
            }
        }
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.pkg.blender.batch.show(ctx, &mut app.state);
        if let Some(p) = app.pkg.blender.batch.take_placement()
            && let Err(e) = crate::batch::place_grid(&mut app.state, &p.items, p.class, p.grid)
        {
            app.state.notify(Severity::Error, e);
        }
        missing_dialog(app, ctx);
    }

    fn translations(&self) -> TrTable {
        blender::tr::TR
    }
}

impl State {
    pub(crate) fn add_batch_inputs(&mut self, paths: Vec<PathBuf>) {
        self.batch.add_inputs(paths);
    }
}
