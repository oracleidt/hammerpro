use std::sync::Arc;

use egui::{Button, Color32, RichText, Ui};
use hp_app::state::AppState;
use hp_app::widgets::{grid_row, section};
use hp_app::{Severity, t, tf};

use super::stamps::{StampImage, StampShape};
use super::{ErodeKind, NoiseStyle, PlateauLevel, StampBlend, TerraOp, mask};
use crate::disp::{DisplacementTool, PanelAction};

const THUMB_CELLS: usize = 28;
const THUMB_SIZE: f32 = 84.0;
const IMAGE_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff", "exr", "pfm", "r16", "r32", "raw"];

pub(crate) fn op_color(op: TerraOp, invert: bool) -> Color32 {
    match (op, invert) {
        (TerraOp::Erode, _) => Color32::from_rgb(214, 160, 96),
        (TerraOp::Terrace, _) => Color32::from_rgb(236, 206, 86),
        (TerraOp::Noise, false) => Color32::from_rgb(110, 196, 255),
        (TerraOp::Noise, true) => Color32::from_rgb(90, 120, 255),
        (TerraOp::Stamp, false) => Color32::from_rgb(255, 160, 60),
        (TerraOp::Stamp, true) => Color32::from_rgb(120, 140, 255),
        (TerraOp::Cliff, false) => Color32::from_rgb(255, 96, 96),
        (TerraOp::Cliff, true) => hp_app::theme::VIEW_SUCCESS,
        (TerraOp::Plateau, _) => Color32::from_rgb(200, 220, 120),
        (TerraOp::Clone, _) => Color32::from_rgb(120, 232, 170),
        (TerraOp::Mask, false) => Color32::from_rgb(255, 84, 48),
        (TerraOp::Mask, true) => Color32::WHITE,
    }
}

pub(crate) fn op_label(op: TerraOp, invert: bool) -> &'static str {
    match (op, invert) {
        (TerraOp::Noise, true) => t!("tf.label.noise_down"),
        (TerraOp::Stamp, true) => t!("tf.label.stamp_dig"),
        (TerraOp::Cliff, true) => t!("tf.label.soften"),
        (TerraOp::Plateau, true) => t!("tf.label.fill"),
        (TerraOp::Mask, true) => t!("tf.label.unmask"),
        (op, _) => t!(op.label_key()),
    }
}

pub(crate) fn hint(op: TerraOp) -> &'static str {
    match op {
        TerraOp::Erode => t!("tf.hint.erode"),
        TerraOp::Terrace => t!("tf.hint.terrace"),
        TerraOp::Noise => t!("tf.hint.noise"),
        TerraOp::Stamp => t!("tf.hint.stamp"),
        TerraOp::Cliff => t!("tf.hint.cliff"),
        TerraOp::Plateau => t!("tf.hint.plateau"),
        TerraOp::Clone => t!("tf.hint.clone"),
        TerraOp::Mask => t!("tf.hint.mask"),
    }
}

fn pick_image(current: &str) -> Option<String> {
    let mut dialog = rfd::FileDialog::new().add_filter(t!("terrain.filter_images"), IMAGE_EXTENSIONS);
    if let Some(dir) = std::path::Path::new(current).parent().filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_file().map(|p| p.to_string_lossy().into_owned())
}

fn thumbnail(ui: &mut Ui, tf: &super::TerraformSettings) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(THUMB_SIZE, THUMB_SIZE), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, Color32::from_gray(18));
    let cell = THUMB_SIZE / THUMB_CELLS as f32;
    let mut values = Vec::with_capacity(THUMB_CELLS * THUMB_CELLS);
    for y in 0..THUMB_CELLS {
        for x in 0..THUMB_CELLS {
            let u = (x as f64 + 0.5) / THUMB_CELLS as f64 * 2.0 - 1.0;
            let v = 1.0 - (y as f64 + 0.5) / THUMB_CELLS as f64 * 2.0;
            let (s, inside) = super::apply::stamp_height(tf.stamp_shape, tf, u, v, 1.0);
            values.push(s * inside.max(0.0));
        }
    }
    for (i, s) in values.iter().enumerate() {
        let (x, y) = (i % THUMB_CELLS, i / THUMB_CELLS);
        let r = egui::Rect::from_min_size(
            rect.min + egui::vec2(x as f32 * cell, y as f32 * cell),
            egui::vec2(cell + 0.5, cell + 0.5),
        );
        let c = if *s >= 0.0 {
            let g = (40.0 + 215.0 * s.clamp(0.0, 1.0)) as u8;
            Color32::from_rgb(g, (f64::from(g) * 0.92) as u8, (f64::from(g) * 0.78) as u8)
        } else {
            let b = (40.0 + 180.0 * (-s).clamp(0.0, 1.0)) as u8;
            Color32::from_rgb(30, 50, b)
        };
        painter.rect_filled(r, 0.0, c);
    }
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Inside);
}

impl DisplacementTool {
    pub(in crate::disp) fn terraform_ui(&mut self, ui: &mut Ui, state: &mut AppState, actions: &mut Vec<PanelAction>) {
        section(ui, t!("tf.section"));
        ui.horizontal_wrapped(|ui| {
            for op in TerraOp::ALL {
                let text = RichText::new(t!(op.label_key())).color(op_color(op, false));
                if ui.add(Button::selectable(self.terraform.op == op, text)).on_hover_text(t!(op.tip_key())).clicked() {
                    self.terraform.op = op;
                    self.cursor = None;
                }
            }
        });
        let tfs = &mut self.terraform;
        let strength_row = |ui: &mut Ui, v: &mut f64| {
            grid_row(ui, t!("disp.strength"), |ui| {
                ui.add(egui::Slider::new(v, 0.0..=1.0).max_decimals(2)).on_hover_text(t!("tf.strength_tip"));
            });
        };
        let mut import = false;
        hp_app::widgets::form(ui, "hp_disp_terraform", |ui| match tfs.op {
            TerraOp::Erode => {
                grid_row(ui, t!("tf.erode.kind"), |ui| {
                    ui.selectable_value(&mut tfs.erode_kind, ErodeKind::Hydraulic, t!("tf.erode.hydraulic"))
                        .on_hover_text(t!("tf.erode.hydraulic_tip"));
                    ui.selectable_value(&mut tfs.erode_kind, ErodeKind::Thermal, t!("tf.erode.thermal"))
                        .on_hover_text(t!("tf.erode.thermal_tip"));
                });
                grid_row(ui, t!("disp.strength"), |ui| {
                    ui.add(egui::Slider::new(&mut tfs.erode_strength, 0.0..=1.0).max_decimals(2))
                        .on_hover_text(t!("tf.erode.strength_tip"));
                });
                let rate_label = if tfs.erode_kind == ErodeKind::Hydraulic {
                    t!("tf.erode.droplets")
                } else {
                    t!("tf.erode.passes")
                };
                grid_row(ui, rate_label, |ui| {
                    ui.add(
                        egui::Slider::new(&mut tfs.erode_rate, 1.0..=10000.0)
                            .logarithmic(true)
                            .max_decimals(0)
                            .suffix(t!("disp.per_second")),
                    )
                    .on_hover_text(t!("tf.erode.rate_tip"));
                });
                match tfs.erode_kind {
                    ErodeKind::Hydraulic => {
                        grid_row(ui, t!("tf.erode.capacity"), |ui| {
                            ui.add(egui::Slider::new(&mut tfs.erode_capacity, 0.0..=32.0).max_decimals(1))
                                .on_hover_text(t!("tf.erode.capacity_tip"));
                        });
                        grid_row(ui, t!("tf.erode.deposition"), |ui| {
                            ui.add(egui::Slider::new(&mut tfs.erode_deposition, 0.0..=1.0).max_decimals(2))
                                .on_hover_text(t!("tf.erode.deposition_tip"));
                        });
                        grid_row(ui, t!("tf.erode.evaporation"), |ui| {
                            ui.add(egui::Slider::new(&mut tfs.erode_evaporation, 0.0..=0.5).max_decimals(3))
                                .on_hover_text(t!("tf.erode.evaporation_tip"));
                        });
                        grid_row(ui, t!("tf.erode.inertia"), |ui| {
                            ui.add(egui::Slider::new(&mut tfs.erode_inertia, 0.0..=0.95).max_decimals(2))
                                .on_hover_text(t!("tf.erode.inertia_tip"));
                        });
                    }
                    ErodeKind::Thermal => {
                        grid_row(ui, t!("tf.erode.talus"), |ui| {
                            ui.add(egui::Slider::new(&mut tfs.erode_talus, 1.0..=89.0).max_decimals(0).suffix(" deg"))
                                .on_hover_text(t!("tf.erode.talus_tip"));
                        });
                    }
                }
            }
            TerraOp::Terrace => {
                grid_row(ui, t!("tf.terrace.step"), |ui| {
                    ui.add(egui::DragValue::new(&mut tfs.terrace_step).speed(1.0).range(1.0..=4096.0))
                        .on_hover_text(t!("tf.terrace.step_tip"));
                });
                grid_row(ui, t!("tf.terrace.sharpness"), |ui| {
                    ui.add(egui::Slider::new(&mut tfs.terrace_sharpness, 0.0..=1.0).max_decimals(2))
                        .on_hover_text(t!("tf.terrace.sharpness_tip"));
                });
                grid_row(ui, t!("tf.terrace.offset"), |ui| {
                    ui.add(egui::DragValue::new(&mut tfs.terrace_offset).speed(1.0).range(-4096.0..=4096.0))
                        .on_hover_text(t!("tf.terrace.offset_tip"));
                });
                strength_row(ui, &mut tfs.strength);
            }
            TerraOp::Noise => {
                grid_row(ui, t!("tf.noise.style"), |ui| {
                    for (s, key, tip) in [
                        (NoiseStyle::Fbm, "tf.noise.fbm", "tf.noise.fbm_tip"),
                        (NoiseStyle::Ridged, "tf.noise.ridged", "tf.noise.ridged_tip"),
                        (NoiseStyle::Billow, "tf.noise.billow", "tf.noise.billow_tip"),
                    ] {
                        ui.selectable_value(&mut tfs.noise_style, s, t!(key)).on_hover_text(t!(tip));
                    }
                });
                grid_row(ui, t!("tf.noise.amplitude"), |ui| {
                    ui.add(egui::DragValue::new(&mut tfs.noise_amplitude).speed(0.5).range(0.0..=4096.0))
                        .on_hover_text(t!("tf.noise.amplitude_tip"));
                });
                grid_row(ui, t!("tf.noise.scale"), |ui| {
                    ui.add(egui::Slider::new(&mut tfs.noise_scale, 1.0..=65536.0).logarithmic(true).max_decimals(0))
                        .on_hover_text(t!("tf.noise.scale_tip"));
                });
                grid_row(ui, t!("tf.noise.octaves"), |ui| {
                    ui.add(egui::Slider::new(&mut tfs.noise_octaves, 1..=8)).on_hover_text(t!("tf.noise.octaves_tip"));
                });
                grid_row(ui, t!("tf.noise.roughness"), |ui| {
                    ui.add(egui::Slider::new(&mut tfs.noise_roughness, 0.05..=1.0).max_decimals(2))
                        .on_hover_text(t!("tf.noise.roughness_tip"));
                });
                grid_row(ui, t!("tf.noise.seed"), |ui| {
                    ui.add(egui::DragValue::new(&mut tfs.noise_seed).range(0..=u64::from(u32::MAX)))
                        .on_hover_text(t!("tf.noise.seed_tip"));
                    if ui.small_button(t!("tf.noise.new_seed")).clicked() {
                        tfs.noise_seed = (tfs
                            .noise_seed
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1_442_695_040_888_963_407)
                            >> 33)
                            % u64::from(u32::MAX);
                    }
                });
                strength_row(ui, &mut tfs.strength);
            }
            TerraOp::Stamp => {
                grid_row(ui, t!("tf.stamp.shape"), |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            for s in StampShape::ALL {
                                if s == StampShape::Image && tfs.stamp_image.is_none() {
                                    continue;
                                }
                                ui.selectable_value(&mut tfs.stamp_shape, s, t!(s.label_key()));
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            if ui.button(t!("tf.stamp.import")).on_hover_text(t!("tf.stamp.import_tip")).clicked() {
                                import = true;
                            }
                            if let Some(name) = std::path::Path::new(&tfs.stamp_image_path).file_name() {
                                ui.weak(name.to_string_lossy());
                            }
                        });
                    });
                });
                grid_row(ui, t!("tf.stamp.preview"), |ui| thumbnail(ui, tfs));
                grid_row(ui, t!("tf.stamp.rotation"), |ui| {
                    ui.add(egui::Slider::new(&mut tfs.stamp_rotation, -180.0..=180.0).max_decimals(0).suffix(" deg"))
                        .on_hover_text(t!("tf.stamp.rotation_tip"));
                });
                grid_row(ui, t!("tf.stamp.scale"), |ui| {
                    ui.add(
                        egui::Slider::new(&mut tfs.stamp_scale, 0.1..=8.0)
                            .logarithmic(true)
                            .max_decimals(2)
                            .suffix(" x"),
                    )
                    .on_hover_text(t!("tf.stamp.scale_tip"));
                });
                grid_row(ui, t!("tf.stamp.height"), |ui| {
                    ui.add(egui::DragValue::new(&mut tfs.stamp_height).speed(1.0).range(-8192.0..=8192.0))
                        .on_hover_text(t!("tf.stamp.height_tip"));
                });
                grid_row(ui, t!("tf.stamp.blend"), |ui| {
                    for (b, key, tip) in [
                        (StampBlend::Add, "tf.stamp.add", "tf.stamp.add_tip"),
                        (StampBlend::Max, "tf.stamp.max", "tf.stamp.max_tip"),
                        (StampBlend::Min, "tf.stamp.min", "tf.stamp.min_tip"),
                        (StampBlend::Replace, "tf.stamp.replace", "tf.stamp.replace_tip"),
                    ] {
                        ui.selectable_value(&mut tfs.stamp_blend, b, t!(key)).on_hover_text(t!(tip));
                    }
                });
            }
            TerraOp::Cliff => {
                grid_row(ui, t!("disp.strength"), |ui| {
                    ui.add(egui::Slider::new(&mut tfs.cliff_strength, 0.0..=1.0).max_decimals(2))
                        .on_hover_text(t!("tf.cliff.strength_tip"));
                });
            }
            TerraOp::Plateau => {
                grid_row(ui, t!("tf.plateau.level"), |ui| {
                    ui.selectable_value(&mut tfs.plateau_level, PlateauLevel::Stroke, t!("tf.plateau.stroke"))
                        .on_hover_text(t!("tf.plateau.stroke_tip"));
                    ui.selectable_value(&mut tfs.plateau_level, PlateauLevel::Fixed, t!("tf.plateau.fixed"))
                        .on_hover_text(t!("tf.plateau.fixed_tip"));
                });
                if tfs.plateau_level == PlateauLevel::Fixed {
                    grid_row(ui, t!("tf.plateau.height"), |ui| {
                        ui.add(egui::DragValue::new(&mut tfs.plateau_height).speed(1.0).range(-16384.0..=16384.0))
                            .on_hover_text(t!("tf.plateau.height_tip"));
                    });
                }
                strength_row(ui, &mut tfs.strength);
            }
            TerraOp::Clone => {
                grid_row(ui, t!("tf.clone.source"), |ui| match tfs.clone_source {
                    Some(p) => {
                        ui.label(format!("{:.0} {:.0} {:.0}", p.x, p.y, p.z));
                        if ui.small_button(t!("common.clear")).clicked() {
                            tfs.clone_source = None;
                        }
                    }
                    None => {
                        ui.colored_label(hp_app::theme::warning(), t!("tf.clone.no_source"));
                    }
                });
                grid_row(ui, t!("tf.clone.rotation"), |ui| {
                    ui.add(egui::Slider::new(&mut tfs.clone_rotation, -180.0..=180.0).max_decimals(0).suffix(" deg"))
                        .on_hover_text(t!("tf.clone.rotation_tip"));
                });
                grid_row(ui, t!("tf.clone.height"), |ui| {
                    ui.add(egui::DragValue::new(&mut tfs.clone_height).speed(0.5).range(-8192.0..=8192.0))
                        .on_hover_text(t!("tf.clone.height_tip"));
                });
                strength_row(ui, &mut tfs.strength);
            }
            TerraOp::Mask => {
                strength_row(ui, &mut tfs.strength);
            }
        });
        if import && let Some(path) = pick_image(&self.terraform.stamp_image_path) {
            match StampImage::load(std::path::Path::new(&path)) {
                Ok(img) => {
                    let text = tf!("tf.stamp.imported", img.width, img.height);
                    self.terraform.stamp_image = Some(Arc::new(img));
                    self.terraform.stamp_image_path = path;
                    self.terraform.stamp_shape = StampShape::Image;
                    state.notify(Severity::Success, text);
                }
                Err(e) => state.notify(Severity::Warning, tf!("tf.stamp.import_failed", e)),
            }
        }
        if self.terraform.stamp_shape == StampShape::Image && self.terraform.stamp_image.is_none() {
            self.terraform.restore_image();
            if self.terraform.stamp_image.is_none() && self.terraform.op == TerraOp::Stamp {
                ui.colored_label(hp_app::theme::warning(), t!("tf.stamp.no_image"));
            }
        }
        let masked = mask::masked_faces(&state.doc);
        if self.terraform.op == TerraOp::Mask || masked > 0 {
            ui.add_space(2.0);
            let targets = crate::disp::commands::targets(&state.doc);
            let scope = if targets.is_empty() {
                t!("tf.mask.scope_all").to_owned()
            } else {
                tf!("tf.mask.scope_selected", targets.len())
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tf!("tf.mask.count", masked)).weak());
                ui.checkbox(&mut self.terraform.mask_show, t!("tf.mask.show")).on_hover_text(t!("tf.mask.show_tip"));
            });
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(t!("tf.mask.invert"))
                    .on_hover_text(format!("{} ({scope})", t!("tf.mask.invert_tip")))
                    .clicked()
                {
                    actions.push(PanelAction::Mask(MaskCommand::Invert));
                }
                if ui
                    .add_enabled(masked > 0, Button::new(t!("tf.mask.clear")))
                    .on_hover_text(format!("{} ({scope})", t!("tf.mask.clear_tip")))
                    .clicked()
                {
                    actions.push(PanelAction::Mask(MaskCommand::Clear));
                }
                if ui
                    .button(t!("tf.mask.fill"))
                    .on_hover_text(format!("{} ({scope})", t!("tf.mask.fill_tip")))
                    .clicked()
                {
                    actions.push(PanelAction::Mask(MaskCommand::Fill));
                }
            });
        }
        if self.terraform.op != TerraOp::Mask {
            self.axis_row_form(ui);
        }
    }

    fn axis_row_form(&mut self, ui: &mut Ui) {
        hp_app::widgets::form(ui, "hp_disp_terraform_axis", |ui| {
            self.axis_row(ui, false);
            grid_row(ui, t!("disp.sculpt.smoothing"), |ui| {
                ui.add(egui::Slider::new(&mut self.brush.smooth, 0.0..=1.0).max_decimals(2))
                    .on_hover_text(t!("tf.smoothing_tip"));
            });
        });
        if self.brush.axis == crate::disp::PaintAxis::Custom {
            hp_app::widgets::vec3_edit(ui, &mut self.brush.custom_axis, 0.01);
        }
    }

    pub(crate) fn run_mask_on(&mut self, state: &mut AppState, cmd: MaskCommand, all: bool) -> usize {
        let mut faces = if all { Vec::new() } else { crate::disp::commands::targets(&state.doc) };
        if faces.is_empty() {
            faces = state.doc.disp_faces();
        }
        let result = match cmd {
            MaskCommand::Clear => mask::clear_masks(&mut state.doc, &faces),
            MaskCommand::Invert => mask::invert_masks(&mut state.doc, &faces),
            MaskCommand::Fill => mask::fill_masks(&mut state.doc, &faces),
        };
        match result {
            Ok(n) => {
                state.notify(Severity::Info, tf!("tf.mask.done", n));
                n
            }
            Err(e) => {
                state.notify(Severity::Warning, tf!("disp.failed", crate::common::edit_error_text(&e)));
                0
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaskCommand {
    Clear,
    Invert,
    Fill,
}
