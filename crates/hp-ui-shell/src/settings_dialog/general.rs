use egui::{RichText, Ui};
use hp_app::i18n::{Lang, TrTable};
use hp_app::settings::{EditSettings, ThemeChoice, UiSettings, ViewSettings};
use hp_app::{AppState, t, tf};

use super::SettingsDialog;
use crate::common::heading;

impl SettingsDialog {
    pub(super) fn general_page(&mut self, ui: &mut Ui, state: &mut AppState) {
        heading(ui, t!("shell.settings.appearance"));
        egui::Grid::new("hp_shell_settings_general").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("menu.language"));
            let current = hp_app::i18n::lang();
            let mut chosen = None;
            egui::ComboBox::from_id_salt("hp_shell_lang").selected_text(current.native_name()).width(180.0).show_ui(
                ui,
                |ui| {
                    for l in Lang::ALL {
                        if ui.selectable_label(l == current, l.native_name()).clicked() {
                            chosen = Some(l);
                        }
                    }
                },
            );
            if let Some(l) = chosen
                && l != current
            {
                crate::menu::switch_language(state, l);
            }
            ui.end_row();

            ui.label(t!("shell.settings.theme"));
            ui.horizontal(|ui| {
                let mut theme = state.settings.ui.theme;
                ui.selectable_value(&mut theme, ThemeChoice::Dark, format!("🌙 {}", t!("shell.settings.theme_dark")));
                ui.selectable_value(&mut theme, ThemeChoice::Light, format!("☀ {}", t!("shell.settings.theme_light")));
                if theme != state.settings.ui.theme {
                    state.settings.ui.theme = theme;
                    state.settings_dirty = true;
                    hp_app::theme::apply(ui.ctx(), theme, state.settings.ui.ui_scale);
                }
            });
            ui.end_row();

            ui.label(t!("shell.settings.accent"));
            ui.horizontal_wrapped(|ui| {
                let current = state.settings.ui.accent_color;
                let mut chosen: Option<Option<[u8; 3]>> = None;
                if ui.selectable_label(current.is_none(), t!("shell.settings.accent_none")).clicked() {
                    chosen = Some(None);
                }
                for rgb in ACCENT_PRESETS {
                    if accent_swatch(ui, rgb, current == Some(rgb)).clicked() {
                        chosen = Some(Some(rgb));
                    }
                }
                let mut custom = current.unwrap_or(ACCENT_PRESETS[0]);
                let r = egui::color_picker::color_edit_button_srgb(ui, &mut custom);
                if r.on_hover_text(t!("shell.settings.accent_custom")).changed() {
                    chosen = Some(Some(custom));
                }
                if let Some(c) = chosen
                    && c != current
                {
                    state.settings.ui.accent_color = c;
                    state.settings_dirty = true;
                    hp_app::theme::set_accent(c);
                    hp_app::theme::apply(ui.ctx(), state.settings.ui.theme, state.settings.ui.ui_scale);
                }
            });
            ui.end_row();

            ui.label(t!("shell.settings.ui_scale"));
            ui.horizontal(|ui| {
                let mut scale = self.ui_scale_edit.unwrap_or(state.settings.ui.ui_scale);
                let r = ui.add(egui::Slider::new(&mut scale, 0.75..=2.0).step_by(0.05).fixed_decimals(2).suffix("×"));
                if r.changed() {
                    self.ui_scale_edit = Some(scale);
                }
                if (r.drag_stopped() || (r.changed() && !r.dragged())) && self.ui_scale_edit.is_some() {
                    let ctx = ui.ctx().clone();
                    self.flush_ui_scale(state, Some(&ctx));
                }
                if ui.small_button("1×").on_hover_text(t!("common.reset")).clicked() {
                    self.ui_scale_edit = Some(1.0);
                    let ctx = ui.ctx().clone();
                    self.flush_ui_scale(state, Some(&ctx));
                }
            });
            ui.end_row();

            ui.label(t!("shell.settings.show_fps"));
            if ui.checkbox(&mut state.settings.view.show_fps, "").changed() {
                state.settings_dirty = true;
            }
            ui.end_row();

            ui.label(t!("shell.settings.start_fullscreen"));
            if ui
                .checkbox(&mut state.settings.ui.start_fullscreen, "")
                .on_hover_text(t!("shell.settings.start_fullscreen_tip"))
                .changed()
            {
                state.settings_dirty = true;
            }
            ui.end_row();

            ui.label(t!("shell.settings.native_frames"));
            if ui
                .checkbox(&mut state.settings.ui.native_frames, "")
                .on_hover_text(t!("shell.settings.native_frames_tip"))
                .changed()
            {
                state.settings_dirty = true;
            }
            ui.end_row();
        });

        self.background_section(ui, state);

        heading(ui, t!("shell.settings.autosave"));
        egui::Grid::new("hp_shell_settings_autosave").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            let e = &mut state.settings.edit;
            let mut changed = false;
            ui.label(t!("shell.settings.autosave_every"));
            ui.horizontal(|ui| {
                changed |=
                    ui.add(egui::DragValue::new(&mut e.autosave_minutes).range(0..=120).suffix(" min")).changed();
                if e.autosave_minutes == 0 {
                    ui.label(RichText::new(t!("common.disabled")).small().color(ui.visuals().weak_text_color()));
                }
            });
            ui.end_row();
            ui.label(t!("shell.settings.autosave_keep"));
            changed |= ui.add(egui::DragValue::new(&mut e.autosave_keep).range(1..=100)).changed();
            ui.end_row();
            if changed {
                state.settings_dirty = true;
            }
        });
        ui.label(
            RichText::new(tf!("shell.settings.autosave_folder", hp_app::settings::Settings::autosave_dir().display()))
                .small()
                .color(ui.visuals().weak_text_color()),
        );

        heading(ui, t!("shell.settings.maps"));
        if ui
            .checkbox(&mut state.settings.workflow.open_in_new_tab, t!("shell.settings.open_in_new_tab"))
            .on_hover_text(t!("shell.settings.open_in_new_tab_tip"))
            .changed()
        {
            state.settings_dirty = true;
        }

        heading(ui, t!("menu.recent"));
        ui.horizontal(|ui| {
            ui.label(tf!("shell.settings.recent_count", state.settings.recent_files.len()));
            if ui
                .add_enabled(!state.settings.recent_files.is_empty(), egui::Button::new(t!("shell.menu.clear_recent")))
                .clicked()
            {
                state.settings.recent_files.clear();
                state.settings_dirty = true;
            }
        });
    }
}

const ACCENT_PRESETS: [[u8; 3]; 8] = [
    [66, 135, 245],
    [255, 140, 40],
    [60, 190, 100],
    [150, 100, 240],
    [235, 70, 70],
    [240, 90, 170],
    [40, 190, 210],
    [240, 200, 40],
];

fn accent_swatch(ui: &mut Ui, rgb: [u8; 3], selected: bool) -> egui::Response {
    let size = egui::vec2(18.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, color);
    let stroke = if selected {
        egui::Stroke::new(2.0, ui.visuals().strong_text_color())
    } else if response.hovered() {
        egui::Stroke::new(1.0, ui.visuals().text_color())
    } else {
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    painter.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Outside);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl SettingsDialog {
    fn background_section(&mut self, ui: &mut Ui, state: &mut AppState) {
        heading(ui, t!("shell.settings.background"));
        if let Some(rx) = &self.background_pick {
            match rx.try_recv() {
                Ok(picked) => {
                    self.background_pick = None;
                    if let Some(path) = picked {
                        let b = &mut state.settings.ui;
                        b.background_image = path.display().to_string();
                        b.background_offset = [0.0, 0.0];
                        b.background_zoom = 1.0;
                        state.settings_dirty = true;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.background_pick = None,
            }
        }
        let b = &mut state.settings.ui;
        let mut changed = false;
        egui::Grid::new("hp_shell_settings_background").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.settings.background_image"));
            ui.horizontal(|ui| {
                if ui.button(t!("shell.settings.background_browse")).clicked() && self.background_pick.is_none() {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let dialog = rfd::FileDialog::new().add_filter(
                        t!("shell.settings.background_files"),
                        &["png", "jpg", "jpeg", "bmp", "tga", "webp"],
                    );
                    let spawned = std::thread::Builder::new().name("hp-background-pick".into()).spawn(move || {
                        let _ = tx.send(dialog.pick_file());
                    });
                    if spawned.is_ok() {
                        self.background_pick = Some(rx);
                    }
                }
                if !b.background_image.is_empty() && ui.button(t!("shell.settings.background_remove")).clicked() {
                    b.background_image.clear();
                    changed = true;
                }
                let name = std::path::Path::new(&b.background_image)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| t!("shell.settings.background_none").to_owned());
                ui.label(RichText::new(name).color(ui.visuals().weak_text_color()))
                    .on_hover_text(b.background_image.as_str());
            });
            ui.end_row();
            if b.background_image.is_empty() {
                return;
            }
            ui.label(t!("shell.settings.background_opacity"));
            let max = crate::background::MAX_OPACITY;
            let mut percent = b.background_opacity.clamp(0.0, max) * 100.0;
            if ui.add(egui::Slider::new(&mut percent, 0.0..=max * 100.0).step_by(1.0).suffix(" %")).changed() {
                b.background_opacity = (percent / 100.0).clamp(0.0, max);
                changed = true;
            }
            ui.end_row();
            ui.label(t!("shell.settings.background_position"));
            ui.horizontal(|ui| {
                ui.label("X");
                changed |= ui.add(egui::DragValue::new(&mut b.background_offset[0]).speed(1.0).suffix(" px")).changed();
                ui.label("Y");
                changed |= ui.add(egui::DragValue::new(&mut b.background_offset[1]).speed(1.0).suffix(" px")).changed();
            });
            ui.end_row();
            ui.label(t!("shell.settings.background_zoom"));
            ui.horizontal(|ui| {
                changed |= ui
                    .add(
                        egui::Slider::new(&mut b.background_zoom, 0.25..=4.0)
                            .step_by(0.01)
                            .fixed_decimals(2)
                            .suffix("×"),
                    )
                    .changed();
                if ui
                    .button(t!("shell.settings.background_fit"))
                    .on_hover_text(t!("shell.settings.background_fit_tip"))
                    .clicked()
                {
                    b.background_offset = [0.0, 0.0];
                    b.background_zoom = 1.0;
                    changed = true;
                }
            });
            ui.end_row();
            ui.label("");
            changed |= ui.checkbox(&mut b.background_hide_3d, t!("shell.settings.background_hide_3d")).changed();
            ui.end_row();
        });
        if changed {
            state.settings_dirty = true;
        }
    }
}

pub(super) fn reset(state: &mut AppState) {
    let s = &mut state.settings;
    let d = UiSettings::default();
    s.ui.theme = d.theme;
    s.ui.ui_scale = d.ui_scale;
    s.ui.accent_color = d.accent_color;
    hp_app::theme::set_accent(d.accent_color);
    s.ui.background_image = d.background_image;
    s.ui.background_opacity = d.background_opacity;
    s.ui.background_offset = d.background_offset;
    s.ui.background_zoom = d.background_zoom;
    s.ui.background_hide_3d = d.background_hide_3d;
    s.ui.native_frames = d.native_frames;
    s.ui.start_fullscreen = d.start_fullscreen;
    let e = EditSettings::default();
    s.edit.autosave_minutes = e.autosave_minutes;
    s.edit.autosave_keep = e.autosave_keep;
    s.view.show_fps = ViewSettings::default().show_fps;
    s.workflow.open_in_new_tab = hp_app::settings::WorkflowSettings::default().open_in_new_tab;
}

pub(crate) const TR: TrTable = &[
    ("shell.settings.appearance", "Appearance", "Apparence"),
    ("shell.settings.theme", "Theme", "Thème"),
    ("shell.settings.theme_dark", "Dark", "Sombre"),
    ("shell.settings.theme_light", "Light", "Clair"),
    ("shell.settings.accent", "Accent color", "Couleur d'accent"),
    ("shell.settings.accent_none", "Black and white", "Noir et blanc"),
    ("shell.settings.accent_custom", "Custom color", "Couleur personnalisée"),
    ("shell.settings.ui_scale", "Interface scale", "Échelle de l'interface"),
    ("shell.settings.show_fps", "Show frame rate", "Afficher les images par seconde"),
    ("shell.settings.start_fullscreen", "Start maximized", "Démarrer agrandi"),
    (
        "shell.settings.start_fullscreen_tip",
        "Hammer Pro opens maximized in the monitor's work area. The taskbar and window buttons stay visible. F11 (View > Maximize window) restores the previous window size and position or maximizes it again. Applied at the next start.",
        "Hammer Pro s'ouvre agrandi dans la zone utile du moniteur. La barre des tâches et les boutons de fenêtre restent visibles. F11 (Affichage > Agrandir la fenêtre) restaure la taille et la position précédentes ou agrandit à nouveau la fenêtre. Appliqué au prochain démarrage.",
    ),
    ("shell.settings.native_frames", "Native window frames", "Cadres de fenêtre natifs"),
    (
        "shell.settings.native_frames_tip",
        "Use the title bars and frames of the system instead of Hammer Pro's, for remote desktop or window managers \
         that have trouble with them. Applied at once.",
        "Utilise les barres de titre et les cadres du système au lieu de ceux de Hammer Pro, pour le bureau à \
         distance ou les gestionnaires de fenêtres qui ont du mal avec eux. Appliqué tout de suite.",
    ),
    ("shell.settings.background", "Background image", "Image de fond"),
    ("shell.settings.background_image", "Image", "Image"),
    ("shell.settings.background_browse", "Choose...", "Choisir..."),
    ("shell.settings.background_remove", "Remove", "Retirer"),
    ("shell.settings.background_none", "No image", "Aucune image"),
    ("shell.settings.background_files", "Images", "Images"),
    ("shell.settings.background_opacity", "Opacity", "Opacité"),
    ("shell.settings.background_position", "Position", "Position"),
    ("shell.settings.background_zoom", "Zoom", "Zoom"),
    ("shell.settings.background_fit", "Fit", "Ajuster"),
    ("shell.settings.background_hide_3d", "Hide it over the 3D view", "Masquer sur la vue 3D"),
    (
        "shell.settings.background_fit_tip",
        "Back to covering the whole window, centered",
        "Revenir à l'image qui couvre toute la fenêtre, centrée",
    ),
    ("shell.settings.autosave", "Autosave", "Sauvegarde automatique"),
    ("shell.settings.autosave_every", "Save every", "Enregistrer toutes les"),
    ("shell.settings.autosave_keep", "Copies kept per map", "Copies conservées par map"),
    ("shell.settings.autosave_folder", "Autosaves are stored in {0}", "Les sauvegardes automatiques sont dans {0}"),
    ("shell.settings.recent_count", "Maps in the list: {0}", "Maps dans la liste : {0}"),
    ("shell.settings.maps", "Maps", "Maps"),
    ("shell.settings.open_in_new_tab", "Open maps in new tabs", "Ouvrir les maps dans de nouveaux onglets"),
    (
        "shell.settings.open_in_new_tab_tip",
        "Off: an opened map replaces the current one, after asking to save its changes.",
        "Désactivé : une map ouverte remplace la map actuelle, après avoir proposé d'enregistrer ses modifications.",
    ),
];
