use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::{RichText, Ui};
use hp_app::{AppState, t, tf, theme};
use hp_editor::ObjRef;
use hp_light::rad_doc::{Pass, RadDoc, TexLightLine};

#[derive(Debug, Default)]
pub struct RadEditor {
    path: Option<PathBuf>,
    stamp: Option<SystemTime>,
    doc: RadDoc,
    dirty: bool,
    model: String,
    material: String,
    error: Option<String>,
}

fn stamp_of(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok().and_then(|m| m.modified().ok())
}

fn selected_face_materials(state: &AppState) -> Vec<String> {
    let scene = state.doc.scene();
    let mut out: Vec<String> = Vec::new();
    for f in state.doc.selected_faces() {
        let Some(m) = scene.solid(f.solid).and_then(|n| n.solid.sides.get(f.side)).map(|s| s.material.clone()) else {
            continue;
        };
        if !m.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&m)) {
            out.push(m);
        }
    }
    out
}

fn selected_models(state: &AppState) -> Vec<String> {
    let scene = state.doc.scene();
    let mut out: Vec<String> = Vec::new();
    for o in state.doc.selected_objects() {
        let ObjRef::Entity(id) = o else { continue };
        let Some(model) = scene.entity(id).and_then(|n| n.entity.get("model")).map(str::trim) else { continue };
        if model.to_ascii_lowercase().ends_with(".mdl") && !out.iter().any(|m| m.eq_ignore_ascii_case(model)) {
            out.push(model.to_owned());
        }
    }
    out
}

fn model_entry(model: &str) -> String {
    let m = model.trim().replace('\\', "/");
    match m.get(..7) {
        Some(head) if head.eq_ignore_ascii_case("models/") => m[7..].to_owned(),
        _ => m,
    }
}

impl RadEditor {
    fn load(&mut self, path: Option<PathBuf>) {
        let stamp = path.as_deref().and_then(stamp_of);
        if self.path == path && (self.dirty || self.stamp == stamp) {
            return;
        }
        self.doc = path
            .as_deref()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| RadDoc::parse(&String::from_utf8_lossy(&b)))
            .unwrap_or_default();
        self.path = path;
        self.stamp = stamp;
        self.dirty = false;
        self.error = None;
    }

    fn save(&mut self) {
        let Some(p) = self.path.clone() else { return };
        match std::fs::write(&p, self.doc.text()) {
            Ok(()) => {
                self.dirty = false;
                self.stamp = stamp_of(&p);
                self.error = None;
            }
            Err(e) => self.error = Some(format!("{}: {e}", p.display())),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let map = state.doc.path().map(Path::to_path_buf);
        let chosen = state
            .settings
            .compile
            .chosen_rad_file()
            .and_then(|f| hp_app::settings::resolve_rad_path(f, map.as_deref()));
        self.load(chosen.clone());
        let Some(path) = chosen else {
            ui.label(RichText::new(t!("lighting.radedit_none")).small().weak());
            return;
        };
        if !path.is_file() && !self.dirty {
            if ui.button(t!("lighting.radedit_create")).on_hover_text(path.display().to_string()).clicked() {
                self.dirty = true;
                self.save();
            }
            return;
        }
        let mut changed = false;
        ui.label(RichText::new(t!("lighting.radedit_texlights")).strong());
        let mut remove: Option<usize> = None;
        let lights = self.doc.texlights();
        egui::Grid::new("hp_rad_texlights").num_columns(4).spacing([6.0, 3.0]).show(ui, |ui| {
            for (i, light) in lights.iter().enumerate() {
                let mut l = light.clone();
                let label = match l.pass {
                    Pass::Both => l.material.clone(),
                    p => format!("{}: {}", p.name(), l.material),
                };
                ui.add(egui::Label::new(RichText::new(label).monospace()).truncate());
                let to_u8 = |v: f32| v.clamp(0.0, 255.0).round() as u8;
                let mut rgb = [to_u8(l.color[0]), to_u8(l.color[1]), to_u8(l.color[2])];
                let mut edited =
                    ui.color_edit_button_srgb(&mut rgb).on_hover_text(t!("lighting.radedit_color_tip")).changed();
                edited |= ui
                    .add(egui::DragValue::new(&mut l.brightness).range(0.0..=100_000.0).speed(1.0))
                    .on_hover_text(t!("lighting.radedit_brightness_tip"))
                    .changed();
                ui.horizontal(|ui| {
                    if l.hdr.is_some() {
                        ui.label(RichText::new("HDR").small().weak()).on_hover_text(t!("lighting.radedit_hdr_tip"));
                    }
                    if ui.small_button("x").on_hover_text(t!("lighting.radedit_remove")).clicked() {
                        remove = Some(i);
                    }
                });
                if edited {
                    l.color = rgb.map(f32::from);
                    self.doc.set_texlight(l);
                    changed = true;
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove.and_then(|i| lights.get(i)) {
            self.doc.remove_texlight(&i.material, Some(i.pass));
            changed = true;
        }
        let faces = selected_face_materials(state);
        ui.horizontal_wrapped(|ui| {
            let resp = ui
                .add_enabled(!faces.is_empty(), egui::Button::new(t!("lighting.radedit_add_face")))
                .on_hover_text(t!("lighting.radedit_add_face_tip"));
            let resp =
                if faces.is_empty() { resp.on_disabled_hover_text(t!("lighting.radedit_add_face_none")) } else { resp };
            if resp.clicked() {
                for m in &faces {
                    if !lights.iter().any(|l| l.material.eq_ignore_ascii_case(m)) {
                        self.doc.set_texlight(TexLightLine {
                            pass: Pass::Both,
                            material: m.to_ascii_lowercase(),
                            color: [255.0, 255.0, 255.0],
                            brightness: 200.0,
                            hdr: None,
                        });
                        changed = true;
                    }
                }
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new(t!("lighting.radedit_models")).strong())
            .on_hover_text(t!("lighting.radedit_models_tip"));
        let models = self.doc.force_texture_shadow();
        for m in &models {
            ui.horizontal(|ui| {
                if ui.small_button("x").on_hover_text(t!("lighting.radedit_remove")).clicked() {
                    self.doc.remove_force_texture_shadow(m);
                    changed = true;
                }
                ui.label(RichText::new(m).monospace());
            });
        }
        let picked = selected_models(state);
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.model).desired_width(180.0).hint_text("props/fence.mdl"));
            if ui.button(t!("lighting.radedit_add")).clicked()
                && self.doc.add_force_texture_shadow(&model_entry(&self.model))
            {
                self.model.clear();
                changed = true;
            }
            let resp = ui
                .add_enabled(!picked.is_empty(), egui::Button::new(t!("lighting.radedit_add_props")))
                .on_hover_text(t!("lighting.radedit_add_props_tip"));
            if resp.clicked() {
                for m in &picked {
                    changed |= self.doc.add_force_texture_shadow(&model_entry(m));
                }
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new(t!("lighting.radedit_noshadow")).strong())
            .on_hover_text(t!("lighting.radedit_noshadow_tip"));
        for m in &self.doc.no_shadow() {
            ui.horizontal(|ui| {
                if ui.small_button("x").on_hover_text(t!("lighting.radedit_remove")).clicked() {
                    self.doc.remove_no_shadow(m);
                    changed = true;
                }
                ui.label(RichText::new(m).monospace());
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.material).desired_width(180.0).hint_text("foliage/leaves"));
            if ui.button(t!("lighting.radedit_add")).clicked() && self.doc.add_no_shadow(&self.material) {
                self.material.clear();
                changed = true;
            }
        });
        if changed {
            self.dirty = true;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(self.dirty, egui::Button::new(t!("lighting.radedit_save"))).clicked() {
                self.save();
            }
            if ui.add_enabled(self.dirty, egui::Button::new(t!("lighting.radedit_revert"))).clicked() {
                self.dirty = false;
                self.stamp = None;
                self.load(Some(path.clone()));
            }
            if self.dirty {
                ui.label(RichText::new(t!("lighting.radedit_unsaved")).small().color(theme::warning()));
            }
        });
        if let Some(e) = &self.error {
            ui.label(RichText::new(tf!("lighting.radedit_save_failed", e)).small().color(theme::warning()));
        }
    }
}
