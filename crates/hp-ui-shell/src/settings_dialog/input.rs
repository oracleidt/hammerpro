use egui::RichText;
use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::{TRACKPAD_SPEED_MAX, TRACKPAD_SPEED_MIN, TrackpadMode, TrackpadScroll3d};
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(InputPage)]
}

pub fn mode_label_key(mode: TrackpadMode) -> &'static str {
    match mode {
        TrackpadMode::Auto => "shell.input.mode_auto",
        TrackpadMode::On => "shell.input.mode_on",
        TrackpadMode::Off => "shell.input.mode_off",
    }
}

pub fn scroll_label_key(scroll: TrackpadScroll3d) -> &'static str {
    match scroll {
        TrackpadScroll3d::Orbit => "shell.input.scroll_orbit",
        TrackpadScroll3d::Move => "shell.input.scroll_move",
    }
}

pub const GESTURES: [(&str, &str); 9] = [
    ("shell.input.g_scroll", "shell.input.g_scroll_desc"),
    ("shell.input.g_scroll_3d", "shell.input.g_scroll_3d_desc"),
    ("shell.input.g_pinch", "shell.input.g_pinch_desc"),
    ("shell.input.g_alt_scroll", "shell.input.g_alt_scroll_desc"),
    ("shell.input.g_shift_scroll", "shell.input.g_shift_scroll_desc"),
    ("shell.input.g_space_drag", "shell.input.g_space_drag_desc"),
    ("shell.input.g_space_shift_drag", "shell.input.g_space_shift_drag_desc"),
    ("shell.input.g_tap", "shell.input.g_tap_desc"),
    ("shell.input.g_two_finger_tap", "shell.input.g_two_finger_tap_desc"),
];

struct InputPage;

impl SettingsPage for InputPage {
    fn id(&self) -> &'static str {
        "input"
    }

    fn label(&self) -> String {
        t!("shell.input.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "🖱"
    }

    fn order(&self) -> i32 {
        page_order::KEYBOARD + 5
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let detected = state.ui.trackpad_detected;
        let s = &mut state.settings.input;
        let mut changed = false;
        let weak = ui.visuals().weak_text_color();
        heading(ui, t!("shell.input.trackpad"));
        ui.label(RichText::new(t!("shell.input.intro")).color(weak));
        ui.add_space(4.0);
        egui::Grid::new("hp_shell_input_trackpad")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .min_col_width(super::label_column(ui))
            .show(ui, |ui| {
                ui.label(t!("shell.input.mode")).on_hover_text(t!("shell.input.mode_tip"));
                hp_app::widgets::choice_row(ui, |ui| {
                    for mode in TrackpadMode::ALL {
                        changed |= ui.radio_value(&mut s.trackpad_mode, mode, t!(mode_label_key(mode))).changed();
                    }
                });
                ui.end_row();
                ui.label("");
                let status = match (s.trackpad_mode, detected) {
                    (TrackpadMode::Auto, true) => t!("shell.input.auto_trackpad"),
                    (TrackpadMode::Auto, false) => t!("shell.input.auto_mouse"),
                    (TrackpadMode::On, _) => t!("shell.input.on_note"),
                    (TrackpadMode::Off, _) => t!("shell.input.off_note"),
                };
                ui.add(egui::Label::new(RichText::new(status).small().color(weak)).wrap());
                ui.end_row();
                ui.label(t!("shell.input.scroll_3d")).on_hover_text(t!("shell.input.scroll_3d_tip"));
                hp_app::widgets::choice_row(ui, |ui| {
                    for scroll in TrackpadScroll3d::ALL {
                        changed |=
                            ui.radio_value(&mut s.trackpad_scroll_3d, scroll, t!(scroll_label_key(scroll))).changed();
                    }
                });
                ui.end_row();
                for (key, value) in [
                    ("shell.input.pan_speed", &mut s.trackpad_pan_speed),
                    ("shell.input.zoom_speed", &mut s.trackpad_zoom_speed),
                    ("shell.input.orbit_speed", &mut s.trackpad_orbit_speed),
                ] {
                    ui.label(t!(key));
                    changed |= ui
                        .add(
                            egui::Slider::new(value, TRACKPAD_SPEED_MIN..=TRACKPAD_SPEED_MAX)
                                .logarithmic(true)
                                .fixed_decimals(2)
                                .suffix("×"),
                        )
                        .changed();
                    ui.end_row();
                }
            });
        heading(ui, t!("shell.input.gestures"));
        let active = s.trackpad_mode.active(detected);
        if !active {
            ui.label(RichText::new(t!("shell.input.gestures_off")).small().color(weak));
        }
        egui::Grid::new("hp_shell_input_gestures")
            .num_columns(2)
            .spacing([16.0, 6.0])
            .min_col_width(super::label_column(ui))
            .show(ui, |ui| {
                for (gesture, desc) in GESTURES {
                    ui.label(RichText::new(t!(gesture)).strong());
                    ui.add(egui::Label::new(t!(desc)).wrap());
                    ui.end_row();
                }
            });
        if changed {
            state.settings_dirty = true;
        }
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.input.page", "Input", "Saisie"),
    ("shell.input.trackpad", "Trackpad", "Pavé tactile"),
    (
        "shell.input.intro",
        "Trackpad mode makes the views easy to use without a mouse: two finger scrolling moves the view instead of zooming, pinching zooms, and Space + drag replaces the middle mouse button.",
        "Le mode pavé tactile rend les vues faciles à utiliser sans souris : le défilement à deux doigts déplace la vue au lieu de zoomer, le pincement zoome et Espace + glisser remplace le bouton du milieu.",
    ),
    ("shell.input.mode", "Trackpad mode", "Mode pavé tactile"),
    (
        "shell.input.mode_tip",
        "Auto: on as soon as a trackpad gesture is seen, off again at the first mouse wheel notch. On: always (every scroll pans). Off: the mouse behaviour, exactly as before.",
        "Auto : activé dès qu'un geste de pavé tactile est vu, désactivé au premier cran de molette. Activé : toujours (tout défilement déplace). Désactivé : le comportement souris, exactement comme avant.",
    ),
    ("shell.input.mode_auto", "Auto", "Auto"),
    ("shell.input.mode_on", "On", "Activé"),
    ("shell.input.mode_off", "Off", "Désactivé"),
    (
        "shell.input.auto_trackpad",
        "Detected: a trackpad. The trackpad gestures are on.",
        "Détecté : un pavé tactile. Les gestes du pavé tactile sont actifs.",
    ),
    (
        "shell.input.auto_mouse",
        "Detected: a mouse wheel. The trackpad gestures turn on at the first trackpad scroll or pinch.",
        "Détecté : une molette de souris. Les gestes du pavé tactile s'activent au premier défilement ou pincement sur le pavé.",
    ),
    (
        "shell.input.on_note",
        "Every scroll is read as a trackpad gesture, even a mouse wheel.",
        "Tout défilement est lu comme un geste de pavé tactile, même une molette de souris.",
    ),
    (
        "shell.input.off_note",
        "The wheel zooms and the middle button pans, as with a mouse.",
        "La molette zoome et le bouton du milieu déplace, comme avec une souris.",
    ),
    ("shell.input.scroll_3d", "Two finger scroll in 3D", "Défilement à deux doigts en 3D"),
    (
        "shell.input.scroll_3d_tip",
        "Orbit turns the camera around the point at the center of the 3D view. Move slides the camera sideways and up or down.",
        "Orbite fait tourner la caméra autour du point au centre de la vue 3D. Déplacer fait glisser la caméra sur les côtés et vers le haut ou le bas.",
    ),
    ("shell.input.scroll_orbit", "Orbit", "Orbite"),
    ("shell.input.scroll_move", "Move", "Déplacer"),
    ("shell.input.pan_speed", "Scroll speed", "Vitesse de défilement"),
    ("shell.input.zoom_speed", "Pinch zoom speed", "Vitesse du zoom par pincement"),
    ("shell.input.orbit_speed", "Orbit speed", "Vitesse d'orbite"),
    ("shell.input.gestures", "Trackpad gestures", "Gestes du pavé tactile"),
    (
        "shell.input.gestures_off",
        "The trackpad gestures are off right now: the mouse controls apply.",
        "Les gestes du pavé tactile sont désactivés pour l'instant : les contrôles souris s'appliquent.",
    ),
    ("shell.input.g_scroll", "Two finger scroll (2D)", "Défilement à deux doigts (2D)"),
    ("shell.input.g_scroll_desc", "Pan the view", "Déplacer la vue"),
    ("shell.input.g_scroll_3d", "Two finger scroll (3D)", "Défilement à deux doigts (3D)"),
    (
        "shell.input.g_scroll_3d_desc",
        "Orbit or move the camera (setting above)",
        "Orbiter ou déplacer la caméra (réglage ci-dessus)",
    ),
    ("shell.input.g_pinch", "Pinch, or Ctrl + scroll", "Pincer, ou Ctrl + défilement"),
    (
        "shell.input.g_pinch_desc",
        "Zoom the 2D view, move the 3D camera forward or back",
        "Zoomer la vue 2D, avancer ou reculer la caméra 3D",
    ),
    ("shell.input.g_alt_scroll", "Alt + scroll", "Alt + défilement"),
    (
        "shell.input.g_alt_scroll_desc",
        "Acts as the mouse wheel (tool options such as the block count, otherwise zoom)",
        "Agit comme la molette (options d'outil comme le nombre de blocs, sinon zoom)",
    ),
    ("shell.input.g_shift_scroll", "Shift + scroll", "Maj + défilement"),
    ("shell.input.g_shift_scroll_desc", "Acts as Shift + mouse wheel", "Agit comme Maj + molette"),
    ("shell.input.g_space_drag", "Space + drag", "Espace + glisser"),
    ("shell.input.g_space_drag_desc", "Pan a 2D view, look around in 3D", "Déplacer une vue 2D, regarder autour en 3D"),
    ("shell.input.g_space_shift_drag", "Space + Shift + drag (3D)", "Espace + Maj + glisser (3D)"),
    ("shell.input.g_space_shift_drag_desc", "Pan the 3D camera", "Déplacer la caméra 3D"),
    ("shell.input.g_tap", "Tap", "Toucher"),
    (
        "shell.input.g_tap_desc",
        "Click; a little finger motion still counts as a click",
        "Clic ; un léger mouvement du doigt compte encore comme un clic",
    ),
    ("shell.input.g_two_finger_tap", "Two finger tap", "Toucher à deux doigts"),
    ("shell.input.g_two_finger_tap_desc", "Right click (context menu)", "Clic droit (menu contextuel)"),
];
