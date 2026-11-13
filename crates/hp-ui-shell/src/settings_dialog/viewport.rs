use egui::{RichText, Ui};
use hp_app::i18n::TrTable;
use hp_app::settings::{
    CtrlWheel, DisplayUnits, FLY_RESPONSE_MAX, GRID_STEP_MAX, GRID_STEP_MIN, NavScheme, ViewSettings, WorkflowSettings,
};
use hp_app::{AppState, t, tf};

use crate::common::heading;
use crate::help::{scheme_controls, scheme_label_key};

fn units_label_key(units: DisplayUnits) -> &'static str {
    match units {
        DisplayUnits::Units => "shell.view.units_units",
        DisplayUnits::Inches => "shell.view.units_inches",
        DisplayUnits::FeetInches => "shell.view.units_feet",
        DisplayUnits::Meters => "shell.view.units_meters",
    }
}

fn workflow_section(ui: &mut Ui, w: &mut WorkflowSettings) -> bool {
    let mut changed = false;
    heading(ui, t!("shell.view.controls"));
    egui::Grid::new("hp_shell_view_controls")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .min_col_width(super::label_column(ui))
        .show(ui, |ui| {
            ui.label(t!("shell.view.scheme")).on_hover_text(t!("shell.view.scheme_tip"));
            egui::ComboBox::from_id_salt("hp_shell_view_scheme")
                .selected_text(t!(scheme_label_key(w.nav_scheme)))
                .width(180.0)
                .show_ui(ui, |ui| {
                    for s in NavScheme::ALL {
                        changed |= ui.selectable_value(&mut w.nav_scheme, s, t!(scheme_label_key(s))).changed();
                    }
                });
            ui.end_row();
            ui.label("");
            ui.vertical(|ui| {
                let weak = ui.visuals().weak_text_color();
                for (input, desc) in scheme_controls(w.nav_scheme) {
                    ui.label(RichText::new(tf!("shell.view.control_line", t!(input), t!(desc))).small().color(weak));
                }
            });
            ui.end_row();
            ui.label(t!("shell.view.invert_x"));
            changed |= ui.checkbox(&mut w.invert_mouse_x, "").changed();
            ui.end_row();
            ui.label(t!("shell.view.camera_smoothing")).on_hover_text(t!("shell.view.camera_smoothing_tip"));
            changed |=
                ui.checkbox(&mut w.camera_smoothing, "").on_hover_text(t!("shell.view.camera_smoothing_tip")).changed();
            ui.end_row();
            ui.label(t!("shell.view.fly_response")).on_hover_text(t!("shell.view.fly_response_tip"));
            changed |= ui
                .add_enabled(
                    w.camera_smoothing,
                    egui::Slider::new(&mut w.fly_response, 0.0..=FLY_RESPONSE_MAX).fixed_decimals(2).suffix(" s"),
                )
                .changed();
            ui.end_row();
            ui.label(t!("shell.view.ctrl_wheel"));
            egui::ComboBox::from_id_salt("hp_shell_view_ctrl_wheel")
                .selected_text(match w.ctrl_wheel {
                    CtrlWheel::GridSize => t!("shell.view.ctrl_wheel_grid"),
                    CtrlWheel::ZoomAll2d => t!("shell.view.ctrl_wheel_zoom"),
                })
                .width(180.0)
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(&mut w.ctrl_wheel, CtrlWheel::GridSize, t!("shell.view.ctrl_wheel_grid"))
                        .changed();
                    changed |= ui
                        .selectable_value(&mut w.ctrl_wheel, CtrlWheel::ZoomAll2d, t!("shell.view.ctrl_wheel_zoom"))
                        .changed();
                });
            ui.end_row();
            ui.label(t!("shell.view.link_zoom"));
            changed |= ui.checkbox(&mut w.link_2d_zoom, "").changed();
            ui.end_row();
            ui.label(t!("shell.view.drag_autopan")).on_hover_text(t!("shell.view.drag_autopan_tip"));
            changed |= ui.checkbox(&mut w.drag_autopan, "").on_hover_text(t!("shell.view.drag_autopan_tip")).changed();
            ui.end_row();
            ui.label(t!("shell.view.grid_step")).on_hover_text(t!("shell.view.grid_step_tip"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut w.grid_step_factor)
                        .range(GRID_STEP_MIN..=GRID_STEP_MAX)
                        .speed(0.05)
                        .max_decimals(2)
                        .prefix("× "),
                )
                .changed();
            ui.end_row();
            ui.label(t!("shell.view.units"));
            egui::ComboBox::from_id_salt("hp_shell_view_units")
                .selected_text(t!(units_label_key(w.units)))
                .width(180.0)
                .show_ui(ui, |ui| {
                    for u in DisplayUnits::ALL {
                        changed |= ui.selectable_value(&mut w.units, u, t!(units_label_key(u))).changed();
                    }
                });
            ui.end_row();
        });
    changed
}

fn grid_mode_row(ui: &mut Ui, state: &mut AppState) {
    use hp_app::Action;
    let modes = [
        (hp_app::settings::GridMode::Global, Action::GridModeGlobal),
        (hp_app::settings::GridMode::PerView, Action::GridModePerView),
        (hp_app::settings::GridMode::Custom, Action::GridModeCustom),
        (hp_app::settings::GridMode::Selection, Action::GridModeSelection),
    ];
    egui::Grid::new("hp_shell_view_grid_mode")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .min_col_width(super::label_column(ui))
        .show(ui, |ui| {
            ui.label(t!("shell.view.grid_mode")).on_hover_text(t!("shell.view.grid_mode_tip"));
            ui.horizontal(|ui| {
                let current = state.settings.grid.mode;
                let label = modes.iter().find(|(m, _)| *m == current).map_or("", |(_, a)| a.label());
                egui::ComboBox::from_id_salt("hp_shell_view_grid_mode_combo")
                    .selected_text(label)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (mode, action) in modes {
                            let enabled = action.enabled(state);
                            if ui
                                .add_enabled(enabled, egui::Button::selectable(current == mode, action.label()))
                                .clicked()
                            {
                                crate::grids::run_grid_action(state, action);
                            }
                        }
                    });
                if ui.button(Action::GridManager.label()).clicked() {
                    state.ui.show_grid_manager = true;
                }
            });
            ui.end_row();
        });
}

pub(super) fn page(ui: &mut Ui, state: &mut AppState) {
    let mut changed = workflow_section(ui, &mut state.settings.workflow);
    grid_mode_row(ui, state);
    let v = &mut state.settings.view;
    heading(ui, t!("shell.view.camera"));
    egui::Grid::new("hp_shell_view_camera")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .min_col_width(super::label_column(ui))
        .show(ui, |ui| {
            ui.label(t!("shell.view.fov"));
            changed |= ui.add(egui::Slider::new(&mut v.fov_deg, 40.0..=120.0).suffix("°").fixed_decimals(0)).changed();
            ui.end_row();
            ui.label(t!("shell.view.camera_speed"));
            changed |= ui
                .add(egui::DragValue::new(&mut v.camera_speed).range(50.0..=20000.0).speed(10.0).suffix(" u/s"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.view.fast_multiplier"))
                .on_hover_text(tf!("shell.view.fast_multiplier_tip", hp_app::i18n::tr("key.shift")));
            changed |= ui
                .add(egui::DragValue::new(&mut v.camera_fast_multiplier).range(1.0..=20.0).speed(0.1).suffix("×"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.view.mouse_sensitivity"));
            changed |= ui.add(egui::Slider::new(&mut v.mouse_sensitivity, 0.02..=1.0).fixed_decimals(2)).changed();
            ui.end_row();
            ui.label(t!("shell.view.invert_y"));
            changed |= ui.checkbox(&mut v.invert_mouse_y, "").changed();
            ui.end_row();
            ui.label(t!("shell.view.keyboard_layout")).on_hover_text(t!("shell.view.keyboard_layout_tip"));
            egui::ComboBox::from_id_salt("keyboard_layout")
                .selected_text(match v.keyboard_layout {
                    hp_app::settings::KeyboardLayout::Auto => t!("shell.view.layout_auto"),
                    hp_app::settings::KeyboardLayout::Azerty => "AZERTY",
                    hp_app::settings::KeyboardLayout::Qwerty => "QWERTY",
                })
                .show_ui(ui, |ui| {
                    for (l, label) in [
                        (hp_app::settings::KeyboardLayout::Auto, t!("shell.view.layout_auto")),
                        (hp_app::settings::KeyboardLayout::Azerty, "AZERTY"),
                        (hp_app::settings::KeyboardLayout::Qwerty, "QWERTY"),
                    ] {
                        changed |= ui.selectable_value(&mut v.keyboard_layout, l, label).changed();
                    }
                });
            ui.end_row();
            ui.label(t!("shell.view.far_plane"));
            changed |= ui.add(egui::DragValue::new(&mut v.far_plane).range(2048.0..=262144.0).speed(64.0)).changed();
            ui.end_row();
            ui.label(t!("shell.view.zoom_to_cursor"));
            changed |= ui.checkbox(&mut v.zoom_to_cursor, "").changed();
            ui.end_row();
            ui.label(t!("shell.view.hover_outline"));
            changed |=
                ui.checkbox(&mut v.hover_outline, "").on_hover_text(t!("shell.view.hover_outline_tip")).changed();
            ui.end_row();
        });
    heading(ui, t!("shell.view.rendering"));
    egui::Grid::new("hp_shell_view_render")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .min_col_width(super::label_column(ui))
        .show(ui, |ui| {
            ui.label(t!("shell.view.msaa")).on_hover_text(t!("shell.view.msaa_tip"));
            changed |= ui.checkbox(&mut v.msaa, "").changed();
            ui.end_row();
            ui.label(t!("shell.view.show_3d_grid"));
            changed |= ui.checkbox(&mut v.show_3d_grid, "").changed();
            ui.end_row();
            ui.label(t!("shell.view.grid_highlight"));
            changed |= ui.add(egui::DragValue::new(&mut v.grid_highlight_every).range(2..=64)).changed();
            ui.end_row();
            ui.label(t!("shell.view.grid_1024"));
            changed |= ui.checkbox(&mut v.show_1024_grid, "").changed();
            ui.end_row();
        });
    heading(ui, t!("shell.view.colors"));
    egui::Grid::new("hp_shell_view_colors")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .min_col_width(super::label_column(ui))
        .show(ui, |ui| {
            ui.label(t!("shell.view.background_2d"));
            changed |= ui.color_edit_button_srgb(&mut v.background_2d).changed();
            ui.end_row();
            ui.label(t!("shell.view.background_3d"));
            changed |= ui.color_edit_button_srgb(&mut v.background_3d).changed();
            ui.end_row();
            ui.label(t!("shell.view.selection_color"));
            changed |= ui.color_edit_button_srgb(&mut v.selection_color).changed();
            ui.end_row();
        });
    if changed {
        state.settings_dirty = true;
    }
}

pub(super) fn reset(state: &mut AppState) {
    let s = &mut state.settings;
    let keep = s.view.clone();
    s.view = ViewSettings {
        render_mode: keep.render_mode,
        show_models: keep.show_models,
        show_helpers: keep.show_helpers,
        show_tool_textures: keep.show_tool_textures,
        show_fps: keep.show_fps,
        ..ViewSettings::default()
    };
    let d = WorkflowSettings::default();
    let w = &mut s.workflow;
    w.nav_scheme = d.nav_scheme;
    w.invert_mouse_x = d.invert_mouse_x;
    w.fly_response = d.fly_response;
    w.camera_smoothing = d.camera_smoothing;
    w.ctrl_wheel = d.ctrl_wheel;
    w.link_2d_zoom = d.link_2d_zoom;
    w.drag_autopan = d.drag_autopan;
    w.grid_step_factor = d.grid_step_factor;
    w.units = d.units;
}

pub(crate) const TR: TrTable = &[
    ("shell.view.camera", "Camera", "Caméra"),
    ("shell.view.fov", "Field of view", "Champ de vision"),
    ("shell.view.camera_speed", "Camera speed", "Vitesse de la caméra"),
    ("shell.view.fast_multiplier", "Fast speed multiplier", "Multiplicateur de vitesse rapide"),
    ("shell.view.fast_multiplier_tip", "Applied while {0} is held", "Appliqué tant que {0} est enfoncée"),
    ("shell.view.mouse_sensitivity", "Mouse sensitivity", "Sensibilité de la souris"),
    ("shell.view.invert_y", "Invert mouse Y", "Inverser l'axe Y de la souris"),
    ("shell.view.keyboard_layout", "Camera keys", "Touches de la caméra"),
    (
        "shell.view.keyboard_layout_tip",
        "Auto: by key position (ZQSD on an AZERTY keyboard, WASD on QWERTY). AZERTY or QWERTY: by key label. Down: Q on QWERTY, A on AZERTY; up: E or Space. Mouselook (Hammer's Z): the key at the Z position in Auto (W on AZERTY), Z with QWERTY, W with AZERTY. Ctrl does not fly: Ctrl + click adds to or removes from the selection.",
        "Auto : selon la position des touches (ZQSD sur un clavier AZERTY, WASD en QWERTY). AZERTY ou QWERTY : selon les lettres des touches. Descendre : A en AZERTY, Q en QWERTY ; monter : E ou Espace. Vue souris (le Z de Hammer) : la touche à la place du Z QWERTY en Auto (W en AZERTY), Z avec QWERTY, W avec AZERTY. Ctrl ne fait pas voler : Ctrl + clic ajoute ou retire de la sélection.",
    ),
    ("shell.view.layout_auto", "Auto (keyboard)", "Auto (clavier)"),
    ("shell.view.far_plane", "View distance", "Distance d'affichage"),
    ("shell.view.zoom_to_cursor", "Zoom toward the cursor", "Zoomer vers le curseur"),
    ("shell.view.hover_outline", "Outline objects under the mouse", "Contour des objets sous la souris"),
    (
        "shell.view.hover_outline_tip",
        "Off: only the selected objects are highlighted, when you click them",
        "Désactivé : seuls les objets sélectionnés sont mis en évidence, quand vous cliquez dessus",
    ),
    ("shell.view.rendering", "Rendering", "Rendu"),
    ("shell.view.msaa", "Antialiasing (MSAA)", "Anticrénelage (MSAA)"),
    ("shell.view.msaa_tip", "Smoother edges, uses more GPU memory", "Bords plus lisses, utilise plus de mémoire GPU"),
    ("shell.view.show_3d_grid", "Grid in the 3D view", "Grille dans la vue 3D"),
    ("shell.view.grid_highlight", "Highlight grid lines every", "Accentuer les lignes de grille toutes les"),
    ("shell.view.grid_1024", "Show 1024 unit lines", "Afficher les lignes de 1024 unités"),
    ("shell.view.colors", "Colors", "Couleurs"),
    ("shell.view.background_2d", "2D background", "Fond 2D"),
    ("shell.view.background_3d", "3D background", "Fond 3D"),
    ("shell.view.selection_color", "Selection", "Sélection"),
    ("shell.view.controls", "Controls", "Contrôles"),
    ("shell.view.control_line", "{0}: {1}", "{0} : {1}"),
    ("shell.view.scheme", "Camera controls", "Contrôles de la caméra"),
    (
        "shell.view.scheme_tip",
        "Mouse and keyboard controls of the views, like Hammer Pro, Hammer or Hammer++",
        "Contrôles des vues à la souris et au clavier, comme Hammer Pro, Hammer ou Hammer++",
    ),
    ("shell.view.scheme_hammerpro", "Hammer Pro", "Hammer Pro"),
    ("shell.view.scheme_hammer", "Hammer", "Hammer"),
    ("shell.view.scheme_hammerpp", "Hammer++ (middle mouse)", "Hammer++ (bouton du milieu)"),
    ("shell.view.ctl_mmb", "Middle mouse drag", "Bouton du milieu + glisser"),
    (
        "shell.view.ctl_mmb_look",
        "Look around and fly with WASD (3D view)",
        "Regarder autour et voler avec ZQSD (vue 3D)",
    ),
    ("shell.view.ctl_space_left", "Space + left drag", "Espace + glisser clic gauche"),
    ("shell.view.ctl_space_left_desc", "Look around (3D view)", "Regarder autour (vue 3D)"),
    ("shell.view.ctl_space_right", "Space + right drag", "Espace + glisser clic droit"),
    ("shell.view.ctl_space_right_desc", "Pan (3D view)", "Faire défiler (vue 3D)"),
    ("shell.view.ctl_both", "Left + right drag", "Clic gauche + clic droit + glisser"),
    ("shell.view.ctl_both_desc", "Move forward and strafe (3D view)", "Avancer et se décaler (vue 3D)"),
    ("shell.view.ctl_arrows_3d", "Arrow keys", "Flèches"),
    (
        "shell.view.ctl_arrows_3d_desc",
        "Look around (3D view, nothing selected)",
        "Regarder autour (vue 3D, rien de sélectionné)",
    ),
    ("shell.view.ctl_arrows_2d", "Arrow keys", "Flèches"),
    (
        "shell.view.ctl_arrows_2d_desc",
        "Scroll the 2D view (nothing selected)",
        "Faire défiler la vue 2D (rien de sélectionné)",
    ),
    ("shell.view.ctl_digits", "1 to 9", "1 à 9"),
    ("shell.view.ctl_digits_desc", "Zoom presets of the 2D view", "Zooms prédéfinis de la vue 2D"),
    ("shell.view.invert_x", "Invert mouse X", "Inverser l'axe X de la souris"),
    ("shell.view.fly_response", "Camera acceleration time", "Temps d'accélération de la caméra"),
    ("shell.view.camera_smoothing", "Smooth 3D camera", "Caméra 3D fluide"),
    (
        "shell.view.camera_smoothing_tip",
        "On: the 3D camera speeds up and slows down smoothly when you fly, and glides toward a teammate you follow. Off: it starts and stops the frame you press or release a movement key, and jumps straight to a followed teammate. Mouse look, the wheel, orbit and trackpad gestures always follow the input directly. The acceleration time is kept for when smoothing is turned on again.",
        "Activé : la caméra 3D accélère et ralentit en douceur en vol, et glisse vers le coéquipier suivi. Désactivé : elle démarre et s'arrête à l'image où vous appuyez sur une touche de déplacement ou la relâchez, et saute directement sur le coéquipier suivi. La vue à la souris, la molette, l'orbite et les gestes du pavé tactile suivent toujours directement l'entrée. Le temps d'accélération est conservé pour la réactivation.",
    ),
    (
        "shell.view.fly_response_tip",
        "Time the flying camera takes to reach its speed (0: at once)",
        "Temps que met la caméra en vol pour atteindre sa vitesse (0 : immédiat)",
    ),
    ("shell.view.ctrl_wheel", "Ctrl + mouse wheel in 2D", "Ctrl + molette en 2D"),
    ("shell.view.ctrl_wheel_grid", "Changes the grid size", "Change la taille de la grille"),
    ("shell.view.ctrl_wheel_zoom", "Zooms every 2D view", "Zoome toutes les vues 2D"),
    ("shell.view.link_zoom", "Zoom the 2D views together", "Zoomer les vues 2D ensemble"),
    (
        "shell.view.drag_autopan",
        "Scroll the 2D view when dragging to its edge",
        "Faire défiler la vue 2D en glissant vers son bord",
    ),
    (
        "shell.view.drag_autopan_tip",
        "Like Hammer: while you drag something in a 2D view (a selection, a handle, a new block, a box selection), bringing the cursor near or past the edge of the view scrolls it that way, faster the farther the cursor goes. What you drag keeps following the cursor.",
        "Comme Hammer : pendant que vous faites glisser quelque chose dans une vue 2D (une sélection, une poignée, un nouveau bloc, une sélection par cadre), approcher le curseur du bord de la vue ou le dépasser la fait défiler dans ce sens, d'autant plus vite que le curseur va loin. Ce que vous faites glisser suit toujours le curseur.",
    ),
    ("shell.view.grid_step", "Grid size step", "Pas de la taille de grille"),
    ("shell.view.grid_mode", "Grid", "Grille"),
    (
        "shell.view.grid_mode_tip",
        "Which grid the tools snap to: one for every view (Hammer), one per view, a custom grid or the grid of the selection",
        "La grille sur laquelle les outils s'alignent : une pour toutes les vues (Hammer), une par vue, une grille personnalisée ou celle de la sélection",
    ),
    (
        "shell.view.grid_step_tip",
        "Factor of the grid smaller and grid larger keys (2 halves and doubles)",
        "Facteur des touches de grille plus fine et plus large (2 divise et multiplie par deux)",
    ),
    ("shell.view.units", "Size units", "Unités des tailles"),
    ("shell.view.units_units", "Units", "Unités"),
    ("shell.view.units_inches", "Inches", "Pouces"),
    ("shell.view.units_feet", "Feet and inches", "Pieds et pouces"),
    ("shell.view.units_meters", "Meters", "Mètres"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_renders_for_every_scheme_and_resets() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        for scheme in NavScheme::ALL {
            state.settings.workflow.nav_scheme = scheme;
            hp_app::testing::run_ui(&mut state, page);
            for (input, desc) in scheme_controls(scheme) {
                assert!(hp_app::i18n::has_key(input) && hp_app::i18n::has_key(desc), "{input} {desc}");
            }
            assert!(hp_app::i18n::has_key(scheme_label_key(scheme)));
        }
        state.settings.workflow.invert_mouse_x = true;
        state.settings.workflow.units = DisplayUnits::Meters;
        state.settings.workflow.keymap_preset = hp_app::actions::KeymapPreset::Hammer;
        reset(&mut state);
        assert!(!state.settings.workflow.invert_mouse_x);
        assert_eq!(state.settings.workflow.units, DisplayUnits::Units);
        assert_eq!(
            state.settings.workflow.keymap_preset,
            hp_app::actions::KeymapPreset::Hammer,
            "the shortcut preset belongs to the keyboard page"
        );
        for u in DisplayUnits::ALL {
            assert!(hp_app::i18n::has_key(units_label_key(u)));
        }
    }
}
