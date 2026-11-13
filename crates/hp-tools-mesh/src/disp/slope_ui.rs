use egui::{Button, RichText, Ui};
use glam::DVec3;
use hp_app::settings::DispWalkMode;
use hp_app::state::AppState;
use hp_app::widgets::section;
use hp_app::{Severity, t, tf};
use hp_editor::{FaceRef, SelectOp};
use hp_geom::{BlendFalloff, BlendMode, RampPivot};

use super::blend::{self, BlendOptions};
use super::slope::{self, SelectMatch, SlopeOp};
use super::{DisplacementTool, PanelAction};
use crate::common::{DocKey, point};

pub(super) type PreviewKey = (DocKey, BlendOptions, Vec<Vec<FaceRef>>);
pub(super) type PreviewCache = std::cell::RefCell<Option<(PreviewKey, Vec<(DVec3, f64)>)>>;

fn falloff_key(f: BlendFalloff) -> &'static str {
    match f {
        BlendFalloff::Linear => "disp.blend_falloff_linear",
        BlendFalloff::Smooth => "disp.blend_falloff_smooth",
        BlendFalloff::EaseIn => "disp.blend_falloff_ease_in",
        BlendFalloff::EaseOut => "disp.blend_falloff_ease_out",
    }
}

fn mode_key(m: BlendMode) -> &'static str {
    match m {
        BlendMode::Average => "disp.blend_mode_average",
        BlendMode::AOverB => "disp.blend_mode_a_over_b",
        BlendMode::BOverA => "disp.blend_mode_b_over_a",
        BlendMode::Max => "disp.blend_mode_max",
        BlendMode::Min => "disp.blend_mode_min",
    }
}

fn pivot_key(p: RampPivot) -> &'static str {
    match p {
        RampPivot::Average => "disp.ramp_pivot_average",
        RampPivot::Lowest => "disp.ramp_pivot_lowest",
        RampPivot::Highest => "disp.ramp_pivot_highest",
    }
}

fn walk_key(m: DispWalkMode) -> &'static str {
    match m {
        DispWalkMode::Source => "disp.walk_source",
        DispWalkMode::Limit => "disp.walk_limit",
        DispWalkMode::Gradient => "disp.walk_gradient",
    }
}

fn op_key(o: SelectOp) -> &'static str {
    match o {
        SelectOp::Add => "disp.slope_select_add",
        SelectOp::Remove => "disp.slope_select_remove",
        _ => "disp.slope_select_replace",
    }
}

fn row(ui: &mut Ui, label: &str, tip: &str, add: impl FnOnce(&mut Ui) -> egui::Response) {
    ui.label(label).on_hover_text(tip);
    add(ui).on_hover_text(tip);
    ui.end_row();
}

fn drag(
    ui: &mut Ui,
    value: &mut f64,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
) -> egui::Response {
    ui.add(egui::DragValue::new(value).speed(speed).range(range).max_decimals(decimals))
}

impl DisplacementTool {
    pub(super) fn blend_effective_sets(&self, state: &AppState) -> Vec<Vec<FaceRef>> {
        let [a, b] = &self.blend_sets;
        if !a.is_empty() && !b.is_empty() {
            return vec![a.clone(), b.clone()];
        }
        blend::partition(&state.doc, &super::commands::targets(&state.doc))
    }

    pub(super) fn blend_ui(&mut self, ui: &mut Ui, state: &AppState, actions: &mut Vec<PanelAction>) {
        section(ui, t!("disp.height_blend"));
        ui.label(RichText::new(t!("disp.blend_hint")).weak());
        let any = !super::commands::targets(&state.doc).is_empty();
        ui.horizontal_wrapped(|ui| {
            for (i, key) in [(0usize, "disp.blend_set_a"), (1, "disp.blend_set_b")] {
                if ui.add_enabled(any, Button::new(t!(key))).on_hover_text(t!("disp.blend_set_tip")).clicked() {
                    actions.push(PanelAction::BlendSet(i));
                }
            }
            if ui
                .add_enabled(
                    !self.blend_sets[0].is_empty() || !self.blend_sets[1].is_empty(),
                    Button::new(t!("disp.blend_clear_sets")),
                )
                .clicked()
            {
                actions.push(PanelAction::BlendClear);
            }
        });
        let sets = self.blend_effective_sets(state);
        let info = if !self.blend_sets[0].is_empty() && !self.blend_sets[1].is_empty() {
            tf!("disp.blend_sets_info", self.blend_sets[0].len(), self.blend_sets[1].len())
        } else {
            tf!("disp.blend_sets_auto", sets.len())
        };
        ui.label(RichText::new(info).weak());
        let o = &mut self.blend;
        hp_app::widgets::form(ui, "hp_disp_blend_grid", |ui| {
            row(ui, t!("disp.blend_width"), t!("disp.blend_width_tip"), |ui| {
                drag(ui, &mut o.width, 1.0, hp_geom::MIN_BLEND_WIDTH..=hp_geom::MAX_BLEND_WIDTH, 1)
            });
            row(ui, t!("disp.blend_falloff"), t!("disp.blend_falloff_tip"), |ui| {
                egui::ComboBox::from_id_salt("hp_disp_blend_falloff")
                    .selected_text(t!(falloff_key(o.falloff)))
                    .show_ui(ui, |ui| {
                        for f in BlendFalloff::ALL {
                            ui.selectable_value(&mut o.falloff, f, t!(falloff_key(f)));
                        }
                    })
                    .response
            });
            row(ui, t!("disp.blend_mode"), t!("disp.blend_mode_tip"), |ui| {
                egui::ComboBox::from_id_salt("hp_disp_blend_mode")
                    .selected_text(t!(mode_key(o.mode)))
                    .show_ui(ui, |ui| {
                        for m in BlendMode::ALL {
                            ui.selectable_value(&mut o.mode, m, t!(mode_key(m)));
                        }
                    })
                    .response
            });
        });
        ui.checkbox(&mut o.match_edges, t!("disp.blend_match_edges")).on_hover_text(t!("disp.blend_match_edges_tip"));
        ui.checkbox(&mut o.alpha, t!("disp.blend_alpha")).on_hover_text(t!("disp.blend_alpha_tip"));
        ui.checkbox(&mut o.match_powers, t!("disp.blend_match_powers"))
            .on_hover_text(t!("disp.blend_match_powers_tip"));
        ui.checkbox(&mut self.blend_preview, t!("disp.blend_preview")).on_hover_text(t!("disp.blend_preview_tip"));
        if ui
            .add_enabled(sets.len() >= 2, Button::new(t!("disp.blend_run")))
            .on_hover_text(t!("disp.blend_run_tip"))
            .clicked()
        {
            actions.push(PanelAction::Blend);
        }
    }

    pub(super) fn slope_ui(&mut self, ui: &mut Ui, state: &mut AppState, any: bool, actions: &mut Vec<PanelAction>) {
        section(ui, t!("disp.slope"));
        ui.label(RichText::new(t!("disp.slope_hint")).weak());
        self.slope_view_ui(ui, state);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(any, Button::new(t!("disp.slope_measure")))
                .on_hover_text(t!("disp.slope_measure_tip"))
                .clicked()
            {
                actions.push(PanelAction::SlopeMeasure);
            }
            if let Some(m) = &self.measured {
                ui.label(
                    RichText::new(tf!(
                        "disp.slope_measured",
                        format!("{:.1}", m.stats.min),
                        format!("{:.1}", m.stats.max),
                        format!("{:.1}", m.stats.mean)
                    ))
                    .weak(),
                );
            }
        });
        if let Some(m) = self.measured.filter(|m| m.fitted) {
            ui.label(
                RichText::new(tf!("disp.slope_fit", format!("{:.1}", m.angle), format!("{:.0}", m.direction))).weak(),
            );
        }
        let o = &mut self.slope;
        hp_app::widgets::collapsing(ui, t!("disp.slope_limit")).id_salt("hp_disp_slope_limit").show(ui, |ui| {
            hp_app::widgets::form(ui, "hp_disp_slope_limit_grid", |ui| {
                row(ui, t!("disp.slope_max_angle"), t!("disp.slope_max_angle_tip"), |ui| {
                    drag(ui, &mut o.max_angle, 0.25, 1.0..=89.0, 2)
                });
                row(ui, t!("disp.slope_terrace_step"), t!("disp.slope_terrace_step_tip"), |ui| {
                    drag(ui, &mut o.terrace_step, 1.0, 1.0..=4096.0, 1)
                });
                row(ui, t!("disp.slope_terrace_strength"), t!("disp.slope_terrace_strength_tip"), |ui| {
                    drag(ui, &mut o.terrace_strength, 0.01, 0.0..=1.0, 2)
                });
            });
            ui.checkbox(&mut o.terrace, t!("disp.slope_terrace")).on_hover_text(t!("disp.slope_terrace_tip"));
        });
        if ui
            .add_enabled(any, Button::new(t!("disp.slope_limit_run")))
            .on_hover_text(t!("disp.slope_limit_tip"))
            .clicked()
        {
            actions.push(PanelAction::Slope(SlopeOp::Limit));
        }
        hp_app::widgets::collapsing(ui, t!("disp.slope_ramp")).id_salt("hp_disp_slope_ramp").show(ui, |ui| {
            hp_app::widgets::form(ui, "hp_disp_slope_ramp_grid", |ui| {
                row(ui, t!("disp.slope_ramp_angle"), t!("disp.slope_ramp_angle_tip"), |ui| {
                    drag(ui, &mut o.ramp_angle, 0.25, 0.0..=89.0, 2)
                });
                row(ui, t!("disp.slope_ramp_direction"), t!("disp.slope_ramp_direction_tip"), |ui| {
                    drag(ui, &mut o.ramp_direction, 1.0, 0.0..=360.0, 1)
                });
                row(ui, t!("disp.slope_ramp_pivot"), t!("disp.slope_ramp_pivot_tip"), |ui| {
                    egui::ComboBox::from_id_salt("hp_disp_ramp_pivot")
                        .selected_text(t!(pivot_key(o.ramp_pivot)))
                        .show_ui(ui, |ui| {
                            for p in [RampPivot::Average, RampPivot::Lowest, RampPivot::Highest] {
                                ui.selectable_value(&mut o.ramp_pivot, p, t!(pivot_key(p)));
                            }
                        })
                        .response
                });
                row(ui, t!("disp.slope_strength"), t!("disp.slope_strength_tip"), |ui| {
                    drag(ui, &mut o.ramp_strength, 0.01, 0.0..=1.0, 2)
                });
                row(ui, t!("disp.slope_detail"), t!("disp.slope_detail_tip"), |ui| {
                    drag(ui, &mut o.ramp_detail, 0.01, 0.0..=1.0, 2)
                });
            });
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(any, Button::new(t!("disp.slope_ramp_run")))
                .on_hover_text(t!("disp.slope_ramp_tip"))
                .clicked()
            {
                actions.push(PanelAction::Slope(SlopeOp::Ramp));
            }
            let fitted = self.measured.is_some_and(|m| m.fitted);
            if ui
                .add_enabled(fitted, Button::new(t!("disp.slope_ramp_take")))
                .on_hover_text(t!("disp.slope_ramp_take_tip"))
                .clicked()
            {
                actions.push(PanelAction::SlopeTake);
            }
        });
        let o = &mut self.slope;
        hp_app::widgets::collapsing(ui, t!("disp.slope_uniform")).id_salt("hp_disp_slope_uniform").show(ui, |ui| {
            hp_app::widgets::form(ui, "hp_disp_slope_uniform_grid", |ui| {
                row(ui, t!("disp.slope_strength"), t!("disp.slope_strength_tip"), |ui| {
                    drag(ui, &mut o.uniform_strength, 0.01, 0.0..=1.0, 2)
                });
                row(ui, t!("disp.slope_detail"), t!("disp.slope_detail_tip"), |ui| {
                    drag(ui, &mut o.uniform_detail, 0.01, 0.0..=1.0, 2)
                });
            });
        });
        if ui
            .add_enabled(any, Button::new(t!("disp.slope_uniform_run")))
            .on_hover_text(t!("disp.slope_uniform_tip"))
            .clicked()
        {
            actions.push(PanelAction::Slope(SlopeOp::Uniform));
        }
        let o = &mut self.slope;
        hp_app::widgets::collapsing(ui, t!("disp.slope_path")).id_salt("hp_disp_slope_path").show(ui, |ui| {
            ui.label(RichText::new(tf!("disp.slope_path_points", o.path.len())).weak());
            hp_app::widgets::form(ui, "hp_disp_slope_path_grid", |ui| {
                row(ui, t!("disp.slope_path_width"), t!("disp.slope_path_width_tip"), |ui| {
                    drag(ui, &mut o.path_width, 1.0, 0.001..=65536.0, 1)
                });
                row(ui, t!("disp.blend_falloff"), t!("disp.blend_falloff_tip"), |ui| {
                    egui::ComboBox::from_id_salt("hp_disp_path_falloff")
                        .selected_text(t!(falloff_key(o.path_falloff)))
                        .show_ui(ui, |ui| {
                            for f in BlendFalloff::ALL {
                                ui.selectable_value(&mut o.path_falloff, f, t!(falloff_key(f)));
                            }
                        })
                        .response
                });
                row(ui, t!("disp.slope_strength"), t!("disp.slope_strength_tip"), |ui| {
                    drag(ui, &mut o.path_strength, 0.01, 0.0..=1.0, 2)
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.pick_path, t!("disp.slope_path_pick"))
                    .on_hover_text(t!("disp.slope_path_pick_tip"));
                if ui.button(t!("disp.slope_path_entities")).on_hover_text(t!("disp.slope_path_entities_tip")).clicked()
                {
                    actions.push(PanelAction::PathFromEntities);
                }
                if ui.add_enabled(!o.path.is_empty(), Button::new(t!("disp.slope_path_clear"))).clicked() {
                    actions.push(PanelAction::PathClear);
                }
            });
        });
        let ready = any && self.slope.path.len() >= 2;
        if ui
            .add_enabled(ready, Button::new(t!("disp.slope_path_run")))
            .on_hover_text(t!("disp.slope_path_tip"))
            .clicked()
        {
            actions.push(PanelAction::Slope(SlopeOp::Path));
        }
        let o = &mut self.slope;
        hp_app::widgets::collapsing(ui, t!("disp.slope_smooth")).id_salt("hp_disp_slope_smooth").show(ui, |ui| {
            hp_app::widgets::form(ui, "hp_disp_slope_smooth_grid", |ui| {
                row(ui, t!("disp.erosion_iterations"), t!("disp.slope_smooth_passes_tip"), |ui| {
                    ui.add(egui::DragValue::new(&mut o.smooth_passes).range(1..=1000))
                });
                row(ui, t!("disp.strength"), t!("disp.slope_strength_tip"), |ui| {
                    drag(ui, &mut o.smooth_strength, 0.01, 0.0..=1.0, 2)
                });
            });
        });
        if ui
            .add_enabled(any, Button::new(t!("disp.slope_smooth_run")))
            .on_hover_text(t!("disp.slope_smooth_tip"))
            .clicked()
        {
            actions.push(PanelAction::Slope(SlopeOp::Smooth));
        }
        let o = &mut self.slope;
        hp_app::widgets::collapsing(ui, t!("disp.slope_select")).id_salt("hp_disp_slope_select").show(ui, |ui| {
            hp_app::widgets::form(ui, "hp_disp_slope_select_grid", |ui| {
                row(ui, t!("disp.slope_max_angle"), t!("disp.slope_select_angle_tip"), |ui| {
                    drag(ui, &mut o.select_angle, 0.25, 0.0..=90.0, 2)
                });
                row(ui, t!("disp.slope_select_which"), t!("disp.slope_select_which_tip"), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut o.select_above, true, t!("disp.slope_select_above"));
                        ui.selectable_value(&mut o.select_above, false, t!("disp.slope_select_below"));
                    })
                    .response
                });
                row(ui, t!("disp.slope_select_match"), t!("disp.slope_select_match_tip"), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut o.select_match, SelectMatch::Any, t!("disp.slope_select_any"));
                        ui.selectable_value(&mut o.select_match, SelectMatch::All, t!("disp.slope_select_all"));
                    })
                    .response
                });
                row(ui, t!("disp.slope_select_op"), t!("disp.slope_select_op_tip"), |ui| {
                    egui::ComboBox::from_id_salt("hp_disp_slope_select_op")
                        .selected_text(t!(op_key(o.select_op)))
                        .show_ui(ui, |ui| {
                            for op in [SelectOp::Replace, SelectOp::Add, SelectOp::Remove] {
                                ui.selectable_value(&mut o.select_op, op, t!(op_key(op)));
                            }
                        })
                        .response
                });
            });
        });
        if ui.button(t!("disp.slope_select_run")).on_hover_text(t!("disp.slope_select_tip")).clicked() {
            actions.push(PanelAction::SlopeSelect);
        }
    }

    fn slope_view_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let r = &mut state.settings.render;
        let mut changed =
            ui.checkbox(&mut r.disp_walkable, t!("disp.walk_show")).on_hover_text(t!("disp.walk_show_tip")).changed();
        ui.horizontal_wrapped(|ui| {
            for m in DispWalkMode::ALL {
                changed |= ui
                    .add(Button::selectable(r.disp_walk_mode == m, t!(walk_key(m))))
                    .on_hover_text(t!("disp.walk_mode_tip"))
                    .clicked()
                    .then(|| r.disp_walk_mode = m)
                    .is_some();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("disp.walk_angle"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut r.disp_walk_angle)
                        .speed(0.1)
                        .range(1.0..=89.0)
                        .max_decimals(2)
                        .suffix("°"),
                )
                .on_hover_text(t!("disp.walk_angle_tip"))
                .changed();
            if ui.small_button(t!("disp.walk_angle_reset")).on_hover_text(t!("disp.walk_angle_reset_tip")).clicked() {
                r.disp_walk_angle = hp_app::settings::DEFAULT_WALK_ANGLE;
                changed = true;
            }
        });
        if changed {
            state.settings_dirty = true;
        }
    }

    pub(super) fn run_menu_action(&mut self, state: &mut AppState, action: hp_app::Action) -> bool {
        let panel = match action {
            hp_app::Action::DispBlendHeights => PanelAction::Blend,
            hp_app::Action::DispLimitSlope => PanelAction::Slope(SlopeOp::Limit),
            hp_app::Action::DispUniformSlope => PanelAction::Slope(SlopeOp::Uniform),
            hp_app::Action::DispSelectBySlope => PanelAction::SlopeSelect,
            _ => return false,
        };
        self.run_slope(state, panel);
        true
    }

    pub(super) fn run_slope(&mut self, state: &mut AppState, action: PanelAction) {
        match action {
            PanelAction::Blend => {
                let sets = self.blend_effective_sets(state);
                blend::apply(state, &sets, &self.blend);
            }
            PanelAction::BlendSet(i) => {
                let faces = super::commands::targets(&state.doc);
                if let Some(slot) = self.blend_sets.get_mut(i) {
                    *slot = faces;
                }
            }
            PanelAction::BlendClear => self.blend_sets = [Vec::new(), Vec::new()],
            PanelAction::Slope(op) => {
                slope::apply(state, op, &self.slope);
                self.measured = None;
            }
            PanelAction::SlopeMeasure => {
                let faces = super::commands::targets(&state.doc);
                self.measured = slope::measure(&state.doc, &faces);
            }
            PanelAction::SlopeTake => {
                if let Some(m) = self.measured.filter(|m| m.fitted) {
                    self.slope.ramp_angle = m.angle.clamp(0.0, 89.0);
                    self.slope.ramp_direction = m.direction;
                }
            }
            PanelAction::SlopeSelect => {
                slope::select_by_slope(state, &self.slope);
            }
            PanelAction::PathFromEntities => {
                let path = slope::path_from_entities(&state.doc);
                if path.len() < 2 {
                    state.notify(Severity::Info, t!("disp.slope_path_entities_need"));
                } else {
                    self.slope.path = path;
                }
            }
            PanelAction::PathClear => self.slope.path.clear(),
            _ => {}
        }
    }

    pub(super) fn pick_path_point(&mut self, state: &mut AppState, view: &hp_app::ViewInfo, pos: egui::Pos2) {
        let (o, d) = view.ray(pos);
        let Some(hit) = super::pick_disp(&state.doc, &hp_geom::Ray::new(o, d)) else {
            state.notify(Severity::Info, t!("disp.slope_path_miss"));
            return;
        };
        if self.slope.path.len() >= 256 {
            state.notify(Severity::Info, t!("disp.slope_path_full"));
            return;
        }
        self.slope.path.push(hit.point);
    }

    pub(super) fn slope_overlay(&self, state: &AppState, out: &mut hp_render::Overlay) {
        if self.blend_preview {
            let sets = self.blend_effective_sets(state);
            let key: PreviewKey = (DocKey::of(&state.doc), self.blend.clone(), sets.clone());
            let mut cache = self.preview_cache.borrow_mut();
            if cache.as_ref().is_none_or(|(k, _)| *k != key) {
                *cache = Some((key, blend::preview(&state.doc, &sets, &self.blend)));
            }
            if let Some((_, dots)) = cache.as_ref() {
                for (p, w) in dots {
                    let w = *w as f32;
                    point(out, *p, [0.25 + 0.75 * w, 0.55 + 0.2 * w, 1.0 - 0.8 * w, 0.9], 3.0 + 3.0 * w);
                }
            }
        }
        let path = &self.slope.path;
        let color = [1.0, 0.8, 0.2, 1.0];
        for w in path.windows(2) {
            if let [a, b] = w {
                crate::common::line_on_top(out, *a, *b, color);
            }
        }
        for p in path {
            point(out, *p + DVec3::Z * 2.0, color, 7.0);
        }
    }
}

pub const TR: hp_app::i18n::TrTable = &[
    ("disp.slope", "Slope control", "Contrôle des pentes"),
    (
        "disp.slope_hint",
        "Slopes are measured on the triangles of the selected displacements, from flat (0 degrees) to vertical (90 degrees)",
        "Les pentes sont mesurées sur les triangles des displacements sélectionnés, de l'horizontale (0 degré) à la verticale (90 degrés)",
    ),
    ("disp.walk_show", "Show slopes in the 3D view", "Afficher les pentes dans la vue 3D"),
    (
        "disp.walk_show_tip",
        "Colors displacement triangles by slope with a legend (the walkability display of the View menu)",
        "Colore les triangles des displacements selon la pente avec une légende (l'affichage de praticabilité du menu Vue)",
    ),
    ("disp.walk_source", "Source", "Source"),
    ("disp.walk_limit", "Limit", "Limite"),
    ("disp.walk_gradient", "Gradient", "Dégradé"),
    (
        "disp.walk_mode_tip",
        "Source: the three classes of the engine. Limit: green up to the angle, red above. Gradient: colors by angle with the limit in the middle",
        "Source : les trois classes du moteur. Limite : vert jusqu'à l'angle, rouge au-dessus. Dégradé : couleurs selon l'angle avec la limite au milieu",
    ),
    ("disp.walk_angle", "Walkable angle", "Angle praticable"),
    (
        "disp.walk_angle_tip",
        "Steepest walkable slope for the Limit and Gradient colors, in degrees from flat",
        "Pente praticable la plus raide pour les couleurs Limite et Dégradé, en degrés depuis l'horizontale",
    ),
    ("disp.walk_angle_reset", "45.57", "45,57"),
    (
        "disp.walk_angle_reset_tip",
        "Back to the Source walkable limit (45.57 degrees, normal Z of 0.7)",
        "Retour à la limite praticable de Source (45,57 degrés, normale Z de 0,7)",
    ),
    ("disp.slope_measure", "Measure", "Mesurer"),
    (
        "disp.slope_measure_tip",
        "Smallest, largest and mean slope of the selected displacements, and the best fit plane",
        "Pente minimale, maximale et moyenne des displacements sélectionnés, et plan d'ajustement optimal",
    ),
    ("disp.slope_measured", "Slope {0} to {1} degrees, mean {2}", "Pente de {0} à {1} degrés, moyenne {2}"),
    ("disp.slope_fit", "Best fit plane: {0} degrees toward {1}", "Plan optimal : {0} degrés vers {1}"),
    ("disp.slope_limit", "Limit steepness", "Limiter la pente"),
    ("disp.slope_max_angle", "Angle", "Angle"),
    ("disp.slope_max_angle_tip", "Steepest allowed slope, in degrees", "Pente maximale autorisée, en degrés"),
    ("disp.slope_terrace", "Terraces", "Terrasses"),
    (
        "disp.slope_terrace_tip",
        "First cut the heights into flat steps, then relax the risers between them to the angle",
        "Découpe d'abord les hauteurs en paliers plats, puis adoucit les contremarches à l'angle",
    ),
    ("disp.slope_terrace_step", "Step height", "Hauteur de palier"),
    ("disp.slope_terrace_step_tip", "Height of one terrace step, in units", "Hauteur d'un palier, en unités"),
    ("disp.slope_terrace_strength", "Terrace strength", "Force des terrasses"),
    (
        "disp.slope_terrace_strength_tip",
        "How much the heights snap to the steps (0 to 1)",
        "Part dont les hauteurs se calent sur les paliers (0 à 1)",
    ),
    ("disp.slope_limit_run", "Limit slope", "Limiter la pente"),
    (
        "disp.slope_limit_tip",
        "Relax the vertices until no triangle is steeper than the angle; the mean height is kept. One undo step",
        "Relâche les sommets jusqu'à ce qu'aucun triangle ne dépasse l'angle ; la hauteur moyenne est conservée. Une seule étape d'annulation",
    ),
    ("disp.slope_ramp", "Exact ramp", "Rampe exacte"),
    ("disp.slope_ramp_angle", "Angle", "Angle"),
    ("disp.slope_ramp_angle_tip", "Exact slope of the ramp, in degrees", "Pente exacte de la rampe, en degrés"),
    ("disp.slope_ramp_direction", "Direction", "Direction"),
    (
        "disp.slope_ramp_direction_tip",
        "Direction the ramp climbs toward, in degrees in the map plane (0 is +X, 90 is +Y)",
        "Direction vers laquelle la rampe monte, en degrés dans le plan de la carte (0 est +X, 90 est +Y)",
    ),
    ("disp.slope_ramp_pivot", "Keep", "Conserver"),
    (
        "disp.slope_ramp_pivot_tip",
        "What stays in place: the mean height, the lowest vertex or the highest vertex",
        "Ce qui reste en place : la hauteur moyenne, le sommet le plus bas ou le plus haut",
    ),
    ("disp.ramp_pivot_average", "Mean height", "Hauteur moyenne"),
    ("disp.ramp_pivot_lowest", "Lowest point", "Point le plus bas"),
    ("disp.ramp_pivot_highest", "Highest point", "Point le plus haut"),
    ("disp.slope_strength", "Strength", "Force"),
    ("disp.slope_strength_tip", "Share of the way to the result (0 to 1)", "Part du chemin vers le résultat (0 à 1)"),
    ("disp.slope_detail", "Keep relief", "Garder le relief"),
    (
        "disp.slope_detail_tip",
        "Share of the current bumps kept on top of the plane (0: a perfect plane)",
        "Part des bosses actuelles gardée au-dessus du plan (0 : un plan parfait)",
    ),
    ("disp.slope_ramp_run", "Make ramp", "Faire une rampe"),
    (
        "disp.slope_ramp_tip",
        "Turn the selected displacements into a plane of exactly this slope and direction. Sewn displacements form one plane. One undo step",
        "Transforme les displacements sélectionnés en un plan de cette pente et direction exactes. Les displacements cousus forment un seul plan. Une seule étape d'annulation",
    ),
    ("disp.slope_ramp_take", "Take measured", "Prendre la mesure"),
    (
        "disp.slope_ramp_take_tip",
        "Copy the angle and direction of the measured best fit plane",
        "Copie l'angle et la direction du plan optimal mesuré",
    ),
    ("disp.slope_uniform", "Uniform slope", "Pente uniforme"),
    ("disp.slope_uniform_run", "Make slope uniform", "Uniformiser la pente"),
    (
        "disp.slope_uniform_tip",
        "Make the slope constant over the whole selection, keeping its mean angle and direction. One undo step",
        "Rend la pente constante sur toute la sélection en gardant son angle et sa direction moyens. Une seule étape d'annulation",
    ),
    ("disp.slope_path", "Constant slope along a path", "Pente constante le long d'un chemin"),
    ("disp.slope_path_points", "{0} path point(s)", "{0} point(s) de chemin"),
    ("disp.slope_path_width", "Half width", "Demi-largeur"),
    (
        "disp.slope_path_width_tip",
        "Distance from the path where the surface is left alone, in units",
        "Distance du chemin au-delà de laquelle la surface n'est pas touchée, en unités",
    ),
    ("disp.slope_path_pick", "Pick points in the 3D view", "Placer des points dans la vue 3D"),
    (
        "disp.slope_path_pick_tip",
        "While on, a click in the 3D view adds a path point on the surface instead of selecting",
        "Activé, un clic dans la vue 3D ajoute un point de chemin sur la surface au lieu de sélectionner",
    ),
    ("disp.slope_path_entities", "From selected entities", "Depuis les entités sélectionnées"),
    (
        "disp.slope_path_entities_tip",
        "Use the origins of the selected entities, in selection order, as the path",
        "Utilise les origines des entités sélectionnées, dans l'ordre de sélection, comme chemin",
    ),
    (
        "disp.slope_path_entities_need",
        "Select at least two entities to take a path from",
        "Sélectionnez au moins deux entités pour en tirer un chemin",
    ),
    ("disp.slope_path_clear", "Clear path", "Effacer le chemin"),
    ("disp.slope_path_run", "Grade along path", "Pente le long du chemin"),
    (
        "disp.slope_path_tip",
        "The surface near the path climbs at a constant slope from the height of the first point to the height of the last. One undo step",
        "La surface près du chemin monte à pente constante de la hauteur du premier point à celle du dernier. Une seule étape d'annulation",
    ),
    (
        "disp.slope_path_miss",
        "Click on a displacement to add a path point",
        "Cliquez sur un displacement pour ajouter un point de chemin",
    ),
    ("disp.slope_path_full", "The path has 256 points already", "Le chemin a déjà 256 points"),
    (
        "disp.slope_path_need",
        "Give a path of at least two points first",
        "Donnez d'abord un chemin d'au moins deux points",
    ),
    ("disp.slope_smooth", "Smooth slopes", "Adoucir les pentes"),
    ("disp.slope_smooth_passes_tip", "Number of passes", "Nombre de passes"),
    ("disp.slope_smooth_run", "Smooth slopes", "Adoucir les pentes"),
    (
        "disp.slope_smooth_tip",
        "Even out the slope changes of the selection; sewn displacements are smoothed together. One undo step",
        "Atténue les variations de pente de la sélection ; les displacements cousus sont adoucis ensemble. Une seule étape d'annulation",
    ),
    ("disp.slope_select", "Select by slope", "Sélectionner par pente"),
    (
        "disp.slope_select_angle_tip",
        "Slope limit in degrees from flat",
        "Limite de pente en degrés depuis l'horizontale",
    ),
    ("disp.slope_select_which", "Slope", "Pente"),
    (
        "disp.slope_select_which_tip",
        "Select the displacements steeper or flatter than the angle",
        "Sélectionne les displacements plus raides ou plus plats que l'angle",
    ),
    ("disp.slope_select_above", "Above", "Au-dessus"),
    ("disp.slope_select_below", "Below", "En dessous"),
    ("disp.slope_select_match", "Match", "Critère"),
    (
        "disp.slope_select_match_tip",
        "A displacement counts when any of its triangles matches, or only when all of them do",
        "Un displacement compte si l'un de ses triangles correspond, ou seulement si tous correspondent",
    ),
    ("disp.slope_select_any", "Any triangle", "Un triangle"),
    ("disp.slope_select_all", "All triangles", "Tous les triangles"),
    ("disp.slope_select_op", "Selection", "Sélection"),
    (
        "disp.slope_select_op_tip",
        "Replace the selection, or add to it, or remove from it",
        "Remplace la sélection, ou s'y ajoute, ou en retire",
    ),
    ("disp.slope_select_replace", "Replace", "Remplacer"),
    ("disp.slope_select_add", "Add", "Ajouter"),
    ("disp.slope_select_remove", "Remove", "Retirer"),
    ("disp.slope_select_run", "Select by slope", "Sélectionner par pente"),
    (
        "disp.slope_select_tip",
        "Select the displacements with triangles above or below the angle. Among the selected displacements if some are selected, else among all of them",
        "Sélectionne les displacements dont les triangles dépassent ou restent sous l'angle. Parmi les displacements sélectionnés s'il y en a, sinon parmi tous",
    ),
    ("disp.slope_select_none", "No displacement matches that slope", "Aucun displacement ne correspond à cette pente"),
    ("disp.slope_selected", "{0} displacement(s) selected by slope.", "{0} displacement(s) sélectionné(s) par pente."),
    ("disp.slope_done", "{0} displacement(s) reshaped.", "{0} displacement(s) remodelé(s)."),
    ("disp.slope_nothing", "Nothing changed: the slope already fits.", "Rien n'a changé : la pente convient déjà."),
];
