use std::path::PathBuf;

use crossbeam_channel::Receiver;
use glam::DVec3;
use hp_app::state::Pointfile;
use hp_app::{Action, Severity, t, tf};

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Compile;

#[derive(Default)]
pub(crate) struct State {
    pub pointfile_pick: Option<Receiver<Option<PathBuf>>>,
}

const HANDLED: &[Action] = &[
    Action::RunMap,
    Action::LoadPointfile,
    Action::UnloadPointfile,
    Action::CompileNow,
    Action::CancelCompile,
    Action::BatchCompile,
];

impl Package for Compile {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::RunMap => app.state.ui.show_compile = true,
            Action::CompileNow => app.compile.start_now(ctx, &mut app.state),
            Action::CancelCompile => app.compile.cancel_compile(&mut app.state),
            Action::BatchCompile => app.state.runtime.compile.show_batch = true,
            Action::LoadPointfile => {
                app.load_pointfile();
                if app.pkg.compile.pointfile_pick.is_some() {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
            Action::UnloadPointfile => app.state.pointfile = None,
            _ => return false,
        }
        true
    }

    fn init(&self, _app: &mut HammerProApp) {
        hp_app::extend::set_compile_options_ui(hp_ui_mapping::compile_options_ui);
        hp_app::extend::set_compilers_ui(hp_ui_mapping::compilers_settings_ui);
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.poll_pointfile_pick();
        if app.pkg.compile.pointfile_pick.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        let rt = &mut app.state.runtime.compile;
        rt.badge = None;
        rt.auto_label = None;
    }
}

impl HammerProApp {
    fn load_pointfile(&mut self) {
        let found = self.state.doc.path().and_then(|p| {
            hp_compile::find_pointfile(p).or_else(|| {
                let name = p.file_name()?;
                hp_compile::find_pointfile(&p.parent()?.join(".hammerpro_build").join(name))
            })
        });
        if let Some(path) = found {
            self.read_pointfile(path);
            return;
        }
        if self.pkg.compile.pointfile_pick.is_some() {
            return;
        }
        let mut dialog = rfd::FileDialog::new().add_filter(t!("app.pointfile_filter"), &["lin", "pts"]);
        if let Some(dir) = self.state.doc.path().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        if cfg!(target_os = "macos") {
            let _ = tx.send(dialog.pick_file());
        } else {
            std::thread::spawn(move || {
                let _ = tx.send(dialog.pick_file());
            });
        }
        self.pkg.compile.pointfile_pick = Some(rx);
    }

    fn poll_pointfile_pick(&mut self) {
        let Some(rx) = self.pkg.compile.pointfile_pick.as_ref() else { return };
        match rx.try_recv() {
            Ok(picked) => {
                self.pkg.compile.pointfile_pick = None;
                if let Some(path) = picked {
                    self.read_pointfile(path);
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => self.pkg.compile.pointfile_pick = None,
        }
    }

    fn read_pointfile(&mut self, path: PathBuf) {
        match hp_compile::read_pointfile(&path) {
            Ok(points) => {
                let points: Vec<DVec3> = points.into_iter().map(DVec3::from_array).collect();
                self.state.notify(Severity::Success, tf!("app.pointfile_loaded", points.len()));
                self.state.pointfile = Some(Pointfile { path, points, cursor: 0 });
                hp_ui_mapping::go_to_leak_start(&mut self.state);
            }
            Err(e) => self.state.notify(Severity::Error, tf!("app.pointfile_failed", e)),
        }
    }
}
