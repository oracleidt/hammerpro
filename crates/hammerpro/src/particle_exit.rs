use std::path::{Path, PathBuf};
use std::time::Duration;

use crossbeam_channel::Receiver;
use hp_app::{t, tf};
use hp_ui_lab::particle_editor::EditorSession;

use crate::app::HammerProApp;
use crate::prompts::UnsavedAnswer;

#[derive(Default)]
pub(crate) struct Controller {
    asking: bool,
    approved: Option<u64>,
    picker: Option<Receiver<Option<PathBuf>>>,
    error: Option<String>,
}

impl Controller {
    pub(crate) fn active(&self) -> bool {
        self.asking || self.picker.is_some()
    }

    fn gate_session(&mut self, session: &mut EditorSession) -> bool {
        if !session.dirty() || self.approved == Some(session.revision) {
            return true;
        }
        self.asking = true;
        session.open = true;
        false
    }
}

pub(crate) fn gate(app: &mut HammerProApp, ctx: &egui::Context) -> bool {
    app.flush_particle_edits();
    let handle = app.pkg.lab.particle_editor.clone();
    let mut session = handle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let ready = app.particle_exit.gate_session(&mut session);
    if !ready {
        ctx.request_repaint();
    }
    ready
}

fn save(app: &mut HammerProApp, controller: &mut Controller, path: &Path) -> bool {
    let handle = app.pkg.lab.particle_editor.clone();
    let mut session = handle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match session.save(path) {
        Ok(()) => {
            controller.asking = false;
            controller.error = None;
            true
        }
        Err(error) => {
            controller.error = Some(error);
            controller.asking = true;
            false
        }
    }
}

fn picker(controller: &mut Controller, source: Option<&str>) {
    let source = source.map(Path::new);
    let mut dialog = rfd::FileDialog::new().add_filter(t!("particle_exit.files"), &["pcf"]);
    if let Some(name) = source.and_then(Path::file_name) {
        dialog = dialog.set_file_name(name.to_string_lossy());
    } else {
        dialog = dialog.set_file_name("particles.pcf");
    }
    if let Some(parent) = source.filter(|p| p.is_absolute()).and_then(Path::parent).filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(parent);
    }
    let (tx, rx) = crossbeam_channel::bounded(1);
    match std::thread::Builder::new().name("hp-particle-exit-save".into()).spawn(move || {
        let _ = tx.send(dialog.save_file());
    }) {
        Ok(_) => controller.picker = Some(rx),
        Err(error) => controller.error = Some(error.to_string()),
    }
}

pub(crate) fn show(app: &mut HammerProApp, ctx: &egui::Context) {
    if !app.particle_exit.active() {
        return;
    }
    let mut controller = std::mem::take(&mut app.particle_exit);
    let mut resume = false;
    let mut cancel = false;
    let picked = controller.picker.as_ref().and_then(|rx| match rx.try_recv() {
        Ok(path) => Some(path),
        Err(crossbeam_channel::TryRecvError::Empty) => None,
        Err(crossbeam_channel::TryRecvError::Disconnected) => Some(None),
    });
    if let Some(path) = picked {
        controller.picker = None;
        if let Some(path) = path {
            resume = save(app, &mut controller, &path);
        } else {
            cancel = true;
        }
    }
    if controller.picker.is_some() {
        egui::Modal::new(egui::Id::unique("hp_particle_exit_wait")).show(ctx, |ui| {
            ui.label(t!("particle_exit.waiting"));
        });
        ctx.request_repaint_after(Duration::from_millis(100));
    } else if controller.asking && !resume && !cancel {
        let source =
            app.pkg.lab.particle_editor.lock().unwrap_or_else(std::sync::PoisonError::into_inner).source.clone();
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("hp_particle_exit")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(t!("dialog.unsaved_title"));
            ui.label(tf!("particle_exit.question", source.as_deref().unwrap_or("Untitled.pcf")));
            if let Some(error) = &controller.error {
                ui.colored_label(hp_app::theme::error(), error);
            }
            ui.horizontal(|ui| {
                if ui.button(t!("common.save")).clicked() {
                    answer = Some(UnsavedAnswer::Save);
                }
                if ui.button(t!("common.dont_save")).clicked() {
                    answer = Some(UnsavedAnswer::DontSave);
                }
                if ui.button(t!("common.cancel")).clicked() {
                    answer = Some(UnsavedAnswer::Cancel);
                }
            });
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(UnsavedAnswer::Cancel);
        }
        match answer {
            Some(UnsavedAnswer::Save) => {
                if let Some(path) = source.as_deref().map(Path::new).filter(|p| p.is_absolute()) {
                    resume = save(app, &mut controller, path);
                } else {
                    picker(&mut controller, source.as_deref());
                }
            }
            Some(UnsavedAnswer::DontSave) => {
                controller.approved = Some(
                    app.pkg.lab.particle_editor.lock().unwrap_or_else(std::sync::PoisonError::into_inner).revision,
                );
                controller.asking = false;
                resume = true;
            }
            Some(UnsavedAnswer::Cancel) => cancel = true,
            None => {}
        }
    }
    app.particle_exit = controller;
    if cancel {
        app.cancel_closing();
    } else if resume {
        app.continue_exit(ctx);
    }
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    (
        "particle_exit.question",
        "Save changes to the particle document {0} before closing Hammer Pro?",
        "Enregistrer les modifications du document de particules {0} avant de fermer Hammer Pro ?",
    ),
    ("particle_exit.files", "Particle files", "Fichiers de particules"),
    (
        "particle_exit.waiting",
        "Choose where to save the particle file, or cancel the file dialog to keep Hammer Pro open.",
        "Choisissez où enregistrer le fichier de particules, ou annulez la boîte de dialogue pour garder Hammer Pro ouvert.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discarding_one_particle_revision_cannot_discard_later_edits() {
        let mut controller = Controller::default();
        let mut session = EditorSession::default();
        assert!(controller.gate_session(&mut session));
        session.doc.create_system("first").unwrap();
        session.changed();
        assert!(!controller.gate_session(&mut session));
        assert!(controller.active());
        assert!(session.open);
        controller.approved = Some(session.revision);
        assert!(controller.gate_session(&mut session));
        session.doc.create_system("second").unwrap();
        session.changed();
        assert!(!controller.gate_session(&mut session));
    }
}
