use egui::{RichText, Ui};
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::i18n::TrTable;
use hp_app::state::AssetStatus;
use hp_app::{AppState, Severity, t, tf};

use crate::common::severity_color;

pub(super) fn segments() -> Vec<Box<dyn StatusSegment>> {
    vec![Box::new(Content)]
}

pub(crate) fn late_text(state: &AppState) -> Option<(String, String)> {
    let late = state.assets.late_mount();
    if late == 0 || !matches!(state.assets.status(), AssetStatus::Ready) {
        return None;
    }
    let progress = state.assets.mount_progress().filter(|p| p.total > 0);
    let (done, total) = progress.as_ref().map_or((0, late), |p| (p.done.min(p.total), p.total));
    let text = tf!("shell.status.late", done, total);
    let tip = match progress.as_ref().map(|p| p.current.as_str()).filter(|c| !c.is_empty()) {
        Some(current) => format!("{}\n{current}", tf!("shell.status.late_tip", done, total)),
        None => tf!("shell.status.late_tip", done, total),
    };
    Some((text, tip))
}

struct Content;

impl StatusSegment for Content {
    fn id(&self) -> &'static str {
        "startup_content"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::CONTENT
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, cx: &mut StatusContext) {
        match state.assets.status() {
            AssetStatus::Mounting => {}
            AssetStatus::NotConfigured => {
                ui.separator();
                let r = ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("⚠ {}", t!("shell.status.no_game")))
                                .small()
                                .color(severity_color(ui.visuals(), Severity::Warning)),
                        )
                        .frame(false),
                    )
                    .on_hover_text(t!("shell.status.no_game_tip"));
                if r.clicked() {
                    cx.open_settings = Some("game");
                }
            }
            AssetStatus::Failed(msg) => {
                ui.separator();
                let r = ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("❌ {}", t!("shell.status.mount_failed")))
                                .small()
                                .color(severity_color(ui.visuals(), Severity::Error)),
                        )
                        .frame(false),
                    )
                    .on_hover_text(msg);
                if r.clicked() {
                    cx.open_settings = Some("game");
                }
            }
            AssetStatus::Ready => {
                if let Some((text, tip)) = late_text(state) {
                    ui.separator();
                    ui.label(RichText::new(text).small()).on_hover_text(tip);
                    hp_app::widgets::paced_spinner(ui, 12.0);
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                }
            }
        }
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.status.no_game", "No game content", "Aucun contenu de jeu"),
    (
        "shell.status.no_game_tip",
        "Garry's Mod was not found. Click to choose its folder.",
        "Garry's Mod est introuvable. Cliquez pour choisir son dossier.",
    ),
    ("shell.status.mount_failed", "Content failed to load", "Échec du chargement du contenu"),
    ("shell.status.opening", "Opening {0}... {1} s", "Ouverture de {0}... {1} s"),
    ("shell.status.late", "Workshop {0} / {1}", "Workshop {0} / {1}"),
    (
        "shell.status.late_tip",
        "Opening the workshop and downloaded addons in the background: {0} of {1}",
        "Ouverture des addons du workshop et téléchargés en arrière-plan : {0} sur {1}",
    ),
    ("shell.status.open_cancel", "Stop opening this map", "Arrêter l'ouverture de cette map"),
];
