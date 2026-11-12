use egui::Ui;
use hp_app::i18n::TrTable;
use hp_app::settings::EditSettings;
use hp_app::{AppState, t};

use crate::common::heading;

const LIGHTMAP_SCALES: [i32; 8] = [1, 2, 4, 8, 16, 32, 64, 128];

pub(super) fn page(ui: &mut Ui, state: &mut AppState) {
    let mut changed = false;
    let mut sync = false;
    let current_material = state.current_material.clone();
    let e = &mut state.settings.edit;
    heading(ui, t!("shell.edit.new_objects"));
    super::form_grid(ui, "hp_shell_edit_defaults").show(ui, |ui| {
        ui.label(t!("shell.edit.default_material"));
        ui.horizontal(|ui| {
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut e.default_material)
                        .desired_width(super::field_width(ui, 260.0, 100.0))
                        .font(egui::TextStyle::Monospace),
                )
                .changed();
            if ui
                .add_enabled(
                    !current_material.is_empty() && current_material != e.default_material,
                    egui::Button::new(t!("shell.edit.use_current")),
                )
                .on_hover_text(&current_material)
                .clicked()
            {
                e.default_material = current_material.clone();
                changed = true;
            }
        });
        ui.end_row();
        ui.label(t!("shell.edit.texture_scale"));
        changed |= ui
            .add(egui::DragValue::new(&mut e.default_texture_scale).range(0.01..=16.0).speed(0.01).max_decimals(3))
            .changed();
        ui.end_row();
        ui.label(t!("shell.edit.lightmap_scale"));
        let mut lm = e.default_lightmap_scale;
        egui::ComboBox::from_id_salt("hp_shell_lightmap").selected_text(lm.to_string()).width(80.0).show_ui(ui, |ui| {
            for s in LIGHTMAP_SCALES {
                ui.selectable_value(&mut lm, s, s.to_string());
            }
        });
        if lm != e.default_lightmap_scale {
            e.default_lightmap_scale = lm;
            changed = true;
        }
        ui.end_row();
        ui.label(t!("shell.edit.point_entity"));
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut e.default_point_entity)
                    .desired_width(220.0)
                    .font(egui::TextStyle::Monospace),
            )
            .changed();
        ui.end_row();
        ui.label(t!("shell.edit.brush_entity"));
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut e.default_brush_entity)
                    .desired_width(220.0)
                    .font(egui::TextStyle::Monospace),
            )
            .changed();
        ui.end_row();
        ui.label(t!("shell.edit.new_brush_color")).on_hover_text(t!("shell.edit.new_brush_color_tip"));
        ui.horizontal(|ui| {
            let mut custom = e.new_brush_color.is_some();
            if ui.checkbox(&mut custom, t!("shell.edit.new_brush_color_custom")).changed() {
                e.new_brush_color = custom.then_some(hp_editor::display_color::PALETTE[5]);
                changed = true;
            }
            if let Some(c) = e.new_brush_color.as_mut() {
                changed |= ui.color_edit_button_srgb(c).changed();
            }
        });
        ui.end_row();
    });
    heading(ui, t!("shell.edit.behavior"));
    super::form_grid(ui, "hp_shell_edit_behavior").show(ui, |ui| {
        ui.label(t!("shell.edit.undo_levels"));
        if ui.add(egui::DragValue::new(&mut e.undo_levels).range(10..=10000).speed(5.0)).changed() {
            changed = true;
            sync = true;
        }
        ui.end_row();
        ui.label(t!("action.toggle_texture_lock"));
        if ui.checkbox(&mut e.texture_lock, "").changed() {
            changed = true;
            sync = true;
        }
        ui.end_row();
        ui.label(t!("action.toggle_texture_scale_lock"));
        if ui.checkbox(&mut e.texture_scale_lock, "").changed() {
            changed = true;
            sync = true;
        }
        ui.end_row();
        ui.label(t!("shell.edit.keep_disps")).on_hover_text(t!("shell.edit.keep_disps_tip"));
        if ui.checkbox(&mut e.keep_displacements_on_cut, "").changed() {
            changed = true;
            sync = true;
        }
        ui.end_row();
        ui.label(t!("shell.edit.arrow_nudge")).on_hover_text(t!("shell.edit.arrow_nudge_tip"));
        changed |= ui.checkbox(&mut e.arrow_key_nudge, "").changed();
        ui.end_row();
        ui.label(t!("shell.edit.paste_offset")).on_hover_text(t!("shell.edit.paste_offset_tip"));
        changed |= ui.checkbox(&mut e.paste_offset, "").changed();
        ui.end_row();
        ui.label(t!("shell.edit.undo_selection")).on_hover_text(t!("shell.edit.undo_selection_tip"));
        if ui.checkbox(&mut e.undo_restores_selection, "").changed() {
            changed = true;
            sync = true;
        }
        ui.end_row();
    });
    if sync {
        sync_edit_options(state);
    }
    if changed {
        state.settings_dirty = true;
    }
}

pub(super) fn reset(state: &mut AppState) {
    let d = EditSettings::default();
    let e = &mut state.settings.edit;
    e.default_material = d.default_material;
    e.default_texture_scale = d.default_texture_scale;
    e.default_lightmap_scale = d.default_lightmap_scale;
    e.default_point_entity = d.default_point_entity;
    e.default_brush_entity = d.default_brush_entity;
    e.undo_levels = d.undo_levels;
    e.texture_lock = d.texture_lock;
    e.texture_scale_lock = d.texture_scale_lock;
    e.arrow_key_nudge = d.arrow_key_nudge;
    e.paste_offset = d.paste_offset;
    e.undo_restores_selection = d.undo_restores_selection;
    e.new_brush_color = d.new_brush_color;
    e.keep_displacements_on_cut = d.keep_displacements_on_cut;
    sync_edit_options(state);
}

pub fn sync_edit_options(state: &mut AppState) {
    state.apply_selection_undo_setting();
    let e = &state.settings.edit;
    let (lock, scale_lock, levels) = (e.texture_lock, e.texture_scale_lock, e.undo_levels);
    let opts = state.doc.options_mut();
    opts.texture_lock = lock;
    opts.texture_scale_lock = scale_lock;
    let mut limits = state.doc.undo_limits();
    limits.max_steps = levels.max(1) as usize;
    state.doc.set_undo_limits(limits);
}

pub(crate) const TR: TrTable = &[
    ("shell.edit.new_objects", "New objects", "Nouveaux objets"),
    ("shell.edit.default_material", "Default material", "Matériau par défaut"),
    ("shell.edit.use_current", "Use current", "Utiliser l'actuel"),
    ("shell.edit.texture_scale", "Default texture scale", "Échelle de texture par défaut"),
    ("shell.edit.lightmap_scale", "Default lightmap scale", "Échelle de lightmap par défaut"),
    ("shell.edit.point_entity", "Default point entity", "Entité ponctuelle par défaut"),
    ("shell.edit.brush_entity", "Default brush entity", "Entité de brush par défaut"),
    ("shell.edit.new_brush_color", "Color of new brushes", "Couleur des nouveaux brushes"),
    (
        "shell.edit.new_brush_color_tip",
        "Display color given to the brushes you create; off: the default look",
        "Couleur d'affichage donnée aux brushes créés ; désactivé : l'aspect par défaut",
    ),
    ("shell.edit.new_brush_color_custom", "Custom", "Personnalisée"),
    ("shell.edit.behavior", "Behavior", "Comportement"),
    ("shell.edit.undo_levels", "Undo levels", "Niveaux d'annulation"),
    ("shell.edit.keep_disps", "Keep displacements when cutting", "Garder les déplacements en coupant"),
    (
        "shell.edit.keep_disps_tip",
        "Clip, Slice, Carve and Intersect keep the displacement on every piece: exact where the cut follows the displacement grid, resampled elsewhere, seams sewn. Off: cut faces become flat brush faces again, like Hammer",
        "Découper, Trancher, Creuser et Intersection gardent le déplacement sur chaque morceau : exact quand la coupe suit la grille du déplacement, rééchantillonné ailleurs, coutures soudées. Décoché : les faces coupées redeviennent des faces de brush plates, comme dans Hammer",
    ),
    ("shell.edit.arrow_nudge", "Nudge with arrow keys", "Décaler avec les flèches"),
    (
        "shell.edit.arrow_nudge_tip",
        "Arrow keys move the selection by one grid step in 2D views",
        "Les flèches déplacent la sélection d'un pas de grille dans les vues 2D",
    ),
    ("shell.edit.paste_offset", "Offset pasted objects", "Décaler les objets collés"),
    (
        "shell.edit.paste_offset_tip",
        "Pasted objects are moved by one grid step so they do not overlap",
        "Les objets collés sont décalés d'un pas de grille pour ne pas se superposer",
    ),
    ("shell.edit.undo_selection", "Undo restores the selection", "Annuler restaure la sélection"),
    (
        "shell.edit.undo_selection_tip",
        "Selection changes are undo steps: Ctrl+Z brings back the previous selection (faces of the texture tool \
        included) and Ctrl+Y selects again. They never mark the map as modified",
        "Les changements de sélection sont des étapes d'annulation : Ctrl+Z rétablit la sélection précédente \
        (faces de l'outil de texture comprises) et Ctrl+Y la refait. Ils ne marquent jamais la carte comme modifiée",
    ),
];
