use std::path::{Path, PathBuf};

use crossbeam_channel::Receiver;
use hp_app::i18n::TrTable;
use hp_app::{Action, Severity, t, tf};

use super::Package;
use crate::app::HammerProApp;
use crate::export::{ExportKind, LastExport, ObjInput};
use crate::files::ViewState;

pub(crate) struct Startup;

struct Export {
    kind: ExportKind,
    path: PathBuf,
    rx: Receiver<Result<usize, String>>,
}

#[derive(Default)]
pub(crate) struct State {
    export: Option<Export>,
}

const HANDLED: &[Action] =
    &[Action::ReloadContent, Action::RebuildContentIndex, Action::ExportCopy, Action::ExportAgain, Action::ExportObj];

impl Package for Startup {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::ReloadContent => {
                let state = &mut app.state;
                state.assets.remount(&state.settings);
                state.notify(Severity::Info, t!("startup.reloading"));
            }
            Action::RebuildContentIndex => {
                let state = &mut app.state;
                hp_fs::clear_index();
                state.assets.remount(&state.settings);
                state.notify(Severity::Info, t!("startup.rebuilding_index"));
            }
            Action::ExportCopy => app.export_with_dialog(ExportKind::Vmf),
            Action::ExportObj => app.export_with_dialog(ExportKind::Obj),
            Action::ExportAgain => {
                let last: LastExport = app.state.sidecar.get();
                match (last.kind, last.path) {
                    (Some(kind), Some(path)) => app.start_export(kind, path),
                    _ => app.export_with_dialog(ExportKind::Vmf),
                }
            }
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.poll_export(ctx);
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        let view: ViewState = app.state.sidecar.get();
        if let Some(tool) = view.tool().filter(|t| app.has_tool(*t) && *t != app.state.tool) {
            app.state.set_tool(tool);
        }
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

fn export_name(stem: &str, kind: ExportKind) -> String {
    match kind {
        ExportKind::Vmf => format!("{stem}_copy.vmf"),
        ExportKind::Obj => format!("{stem}.obj"),
    }
}

impl HammerProApp {
    fn export_with_dialog(&mut self, kind: ExportKind) {
        let stem = self
            .state
            .doc
            .path()
            .or_else(|| self.state.doc.opened_from())
            .and_then(Path::file_stem)
            .map_or_else(|| "untitled".to_owned(), |s| s.to_string_lossy().into_owned());
        let name = export_name(&stem, kind);
        let picked = match kind {
            ExportKind::Vmf => crate::files::pick_save_vmf(&self.state, &name),
            ExportKind::Obj => crate::files::pick_save_obj(&self.state, &name),
        };
        if let Some(path) = picked {
            self.start_export(kind, path);
        }
    }

    fn start_export(&mut self, kind: ExportKind, path: PathBuf) {
        if self.pkg.startup.export.is_some() {
            self.state.notify(Severity::Info, t!("startup.export_busy"));
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let target = path.clone();
        let job: Box<dyn FnOnce() -> Result<usize, String> + Send> = match kind {
            ExportKind::Vmf => {
                let mut map = self.state.doc.to_map();
                crate::files::store_view(&self.state, &mut map);
                Box::new(move || crate::export::write_vmf(&map, &target).map(|()| 0))
            }
            ExportKind::Obj => {
                let input = ObjInput::from_state(&mut self.state);
                Box::new(move || crate::export::export_obj(&input, &target).map(|s| s.faces))
            }
        };
        let spawned = std::thread::Builder::new().name("hp-export".to_owned()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))
                .unwrap_or_else(|_| Err("the export stopped unexpectedly".to_owned()));
            let _ = tx.send(result);
        });
        match spawned {
            Ok(_) => self.pkg.startup.export = Some(Export { kind, path, rx }),
            Err(e) => self.state.notify(Severity::Error, tf!("startup.export_failed", path.display(), e)),
        }
    }

    fn poll_export(&mut self, ctx: &egui::Context) {
        let Some(export) = &self.pkg.startup.export else { return };
        let result = match export.rx.try_recv() {
            Ok(result) => result,
            Err(crossbeam_channel::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => Err("the export stopped unexpectedly".to_owned()),
        };
        let Some(export) = self.pkg.startup.export.take() else { return };
        let name = export.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match result {
            Ok(faces) => {
                let text = match export.kind {
                    ExportKind::Vmf => tf!("startup.exported", name),
                    ExportKind::Obj => tf!("startup.exported_obj", name, faces),
                };
                self.state.notify(Severity::Success, text);
                self.state.sidecar.set(&LastExport { kind: Some(export.kind), path: Some(export.path) });
            }
            Err(e) => self.state.notify(Severity::Error, tf!("startup.export_failed", name, e)),
        }
    }
}

pub(crate) const TR: TrTable = &[
    (
        "startup.reloading",
        "Reloading the game content and entity definitions...",
        "Rechargement du contenu du jeu et des définitions d'entités...",
    ),
    (
        "startup.rebuilding_index",
        "Rebuilding the content index: the game folders are scanned again...",
        "Reconstruction de l'index du contenu : les dossiers du jeu sont analysés à nouveau...",
    ),
    ("startup.exported", "{0} exported", "{0} exporté"),
    ("startup.exported_obj", "{0} exported: {1} faces", "{0} exporté : {1} faces"),
    ("startup.export_failed", "Could not export {0}: {1}", "Impossible d'exporter {0} : {1}"),
    ("startup.export_busy", "An export is already running", "Une exportation est déjà en cours"),
    ("startup.obj_filter", "OBJ model", "Modèle OBJ"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_are_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty());
        assert!(TR.iter().all(|(k, _, _)| k.starts_with("startup.")));
    }

    #[test]
    fn export_names() {
        assert_eq!(export_name("gm_city", ExportKind::Vmf), "gm_city_copy.vmf");
        assert_eq!(export_name("gm_city", ExportKind::Obj), "gm_city.obj");
    }
}
