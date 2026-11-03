use egui::RichText;
use hp_app::extend::{SettingsPage, page_order};
use hp_app::runtime::UpdateStatus;
use hp_app::settings::UpdateSettings;
use hp_app::{AppState, t, tf};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(UpdatePage)]
}

struct UpdatePage;

impl SettingsPage for UpdatePage {
    fn id(&self) -> &'static str {
        "updates"
    }

    fn label(&self) -> String {
        t!("update.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "⟳"
    }

    fn order(&self) -> i32 {
        page_order::GENERAL + 60
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        heading(ui, t!("update.heading_settings"));
        ui.label(t!("update.intro"));
        ui.add_space(4.0);
        if ui.checkbox(&mut state.settings.update.check_on_startup, t!("update.check_on_startup")).changed() {
            state.settings_dirty = true;
        }
        ui.add_space(6.0);
        let busy = state.runtime.update.status.is_busy();
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy, egui::Button::new(t!("update.check_now"))).clicked() {
                state.runtime.update.check_requested = Some(true);
            }
            if state.runtime.update.latest.is_some()
                && matches!(state.runtime.update.status, UpdateStatus::Available | UpdateStatus::Ready)
                && ui.button(t!("update.show_update")).clicked()
            {
                state.runtime.update.dialog_open = true;
            }
        });
        ui.add_space(4.0);
        ui.label(tf!("update.current_version", state.runtime.update.current));
        status_line(ui, state);
        let skipped = state.settings.update.skipped_version.clone();
        if !skipped.trim().is_empty() {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(tf!("update.skipped", skipped));
                if ui.button(t!("update.clear_skipped")).clicked() {
                    state.settings.update.skipped_version.clear();
                    state.settings_dirty = true;
                }
            });
        }
        ui.add_space(8.0);
        ui.label(RichText::new(t!("update.note")).small().weak());
        if state.runtime.update.api_overridden {
            ui.label(RichText::new(t!("update.override_note")).small().color(ui.visuals().warn_fg_color));
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.update = UpdateSettings::default();
    }
}

fn status_line(ui: &mut egui::Ui, state: &AppState) {
    let rt = &state.runtime.update;
    let version = rt.latest.as_ref().map(|l| l.version.clone()).unwrap_or_default();
    let (text, warn) = match rt.status {
        UpdateStatus::Idle => (t!("update.status_idle").to_owned(), false),
        UpdateStatus::Checking => (t!("update.status_checking").to_owned(), false),
        UpdateStatus::UpToDate => (tf!("update.status_up_to_date", rt.latest_seen), false),
        UpdateStatus::Available => (tf!("update.status_available", version), false),
        UpdateStatus::Downloading => (tf!("update.status_downloading", version), false),
        UpdateStatus::Ready => (tf!("update.status_ready", version), false),
        UpdateStatus::Failed => {
            let why = rt.failure.as_ref().map(|f| tf!(f.key, f.detail)).unwrap_or_default();
            (tf!("update.status_failed", why), true)
        }
    };
    let color = if warn { ui.visuals().warn_fg_color } else { ui.visuals().text_color() };
    ui.label(RichText::new(text).color(color));
}
