use egui::{RichText, Ui};
use hp_app::runtime::{BakeKind, BakePhase, BudgetSummary, LightingRuntime};
use hp_app::settings::{LightingQuality, LightingSetting, RenderModeSetting};
use hp_app::{Action, AppState, Severity, ToolId, t, tf, theme};
use hp_editor::luxels::{ColorCorrectionPlus, LightPlus};
use hp_editor::{FaceRef, SelectOp};

use crate::dialogs::MapDialog;
use crate::util::plural;

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    Vec::new()
}

const TOP_ROWS: usize = 12;

pub fn fmt_decimal(v: f64, digits: usize) -> String {
    let s = format!("{v:.digits$}");
    if hp_app::i18n::lang() == hp_app::i18n::Lang::Fr { s.replace('.', ",") } else { s }
}

pub fn fmt_count(n: u64) -> String {
    let digits = n.to_string();
    let sep = if hp_app::i18n::lang() == hp_app::i18n::Lang::Fr { ' ' } else { ',' };
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

pub fn fmt_megabytes(bytes: u64) -> String {
    tf!("lighting.megabytes", fmt_decimal(bytes as f64 / (1024.0 * 1024.0), 1))
}

pub fn phase_text(phase: BakePhase) -> String {
    match phase {
        BakePhase::Preparing => t!("lighting.phase_preparing").to_owned(),
        BakePhase::Compiling => t!("lighting.phase_compiling").to_owned(),
        BakePhase::Direct => t!("lighting.phase_direct").to_owned(),
        BakePhase::Bounce => t!("lighting.phase_bounce").to_owned(),
        BakePhase::Finishing => t!("lighting.phase_finishing").to_owned(),
    }
}

pub fn budget_color(fraction: f64) -> egui::Color32 {
    if fraction > 1.0 {
        theme::error()
    } else if fraction > 0.75 {
        theme::warning()
    } else {
        theme::accent()
    }
}

#[derive(Debug, Default)]
pub struct LightingTab {
    cc_edit: [Option<String>; 4],
    vrad: bool,
}

impl LightingTab {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        state.runtime.lighting.budget_wanted = true;
        egui::ScrollArea::vertical().id_salt("hp_lighting_scroll").auto_shrink([false, false]).show(ui, |ui| {
            self.bake_section(ui, state);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.vrad, false, t!("lighting.preview"));
                ui.selectable_value(&mut self.vrad, true, "VRAD");
            });
            if self.vrad {
                self.vrad_section(ui, state);
                self.options_section(ui, state);
                return;
            }
            ui.add_space(6.0);
            color_correction_section(ui, state, &mut self.cc_edit);
            ui.add_space(6.0);
            budget_section(ui, state);
            ui.add_space(6.0);
            stats_section(ui, &state.runtime.lighting);
        });
    }

    fn bake_section(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.label(RichText::new(t!("lighting.preview")).strong());
        let mode = state.settings.view.render_mode;
        ui.horizontal(|ui| {
            for (m, action) in [
                (RenderModeSetting::Baked, Action::RenderBaked),
                (RenderModeSetting::LightmapGrid, Action::RenderLightmapGrid),
            ] {
                if ui.add(egui::Button::selectable(mode == m, action.label())).clicked() {
                    state.request(if mode == m { Action::RenderTextured } else { action });
                }
            }
        });
        ui.horizontal(|ui| {
            let baking = state.runtime.lighting.progress.is_some();
            if ui.button(format!("☀ {}", Action::BakeLighting.label())).clicked() {
                state.request(Action::BakeLighting);
            }
            if ui
                .button(Action::BakeAroundCamera.label())
                .on_hover_text(tf!("lighting.around_tip", state.doc.light_plus().bake_dist.round()))
                .clicked()
            {
                state.request(Action::BakeAroundCamera);
            }
            if ui.add_enabled(baking, egui::Button::new(format!("⏹ {}", Action::StopBake.label()))).clicked() {
                state.request(Action::StopBake);
            }
        });
        quality_row(ui, state);
        let rt = &state.runtime.lighting;
        if let Some(p) = rt.progress {
            let kind = match p.kind {
                BakeKind::Full => t!("lighting.kind_full"),
                BakeKind::AroundCamera => t!("lighting.kind_around"),
                BakeKind::Update => t!("lighting.kind_update"),
                BakeKind::Refine => t!("lighting.kind_refine"),
            };
            let text = tf!("lighting.progress", kind, phase_text(p.phase), fmt_decimal(f64::from(p.elapsed_s), 1));
            ui.add(hp_app::widgets::progress_bar(p.fraction.clamp(0.0, 1.0)).text(text));
        } else if let Some(last) = rt.last {
            let text = if last.cancelled {
                t!("lighting.last_stopped").to_owned()
            } else {
                tf!(
                    plural(last.faces, "lighting.last_done_one", "lighting.last_done"),
                    fmt_count(last.faces as u64),
                    fmt_decimal(f64::from(last.seconds), 1)
                )
            };
            ui.label(RichText::new(text).weak());
            if last.density < 0.999 {
                ui.label(
                    RichText::new(tf!("lighting.reduced", fmt_decimal(f64::from(last.density), 2)))
                        .color(theme::warning())
                        .small(),
                );
            }
            if last.atlas_full > 0 {
                ui.label(RichText::new(tf!("lighting.atlas_full", last.atlas_full)).color(theme::warning()).small());
            }
        } else if !rt.has_bake {
            ui.label(RichText::new(t!("lighting.not_baked")).weak());
        }
        if state.runtime.lighting.no_lights {
            ui.label(RichText::new(format!("⚠ {}", t!("lighting.no_lights"))).color(theme::warning()));
        }
        if state.runtime.lighting.stale && state.runtime.lighting.progress.is_none() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("⚠ {}", t!("lighting.stale"))).color(theme::warning()));
                if ui.small_button(t!("lighting.rebake")).clicked() {
                    state.request(Action::BakeLighting);
                }
            });
        }
        let r = state.runtime.lighting.relight;
        if r.jobs > 0 && !r.full {
            let text = tf!(
                "lighting.relight_stats",
                r.changed_lights,
                fmt_count(r.relit_faces as u64),
                fmt_decimal(f64::from(r.seconds) * 1000.0, 0),
                fmt_decimal(f64::from(r.turnaround_seconds) * 1000.0, 0)
            );
            ui.label(RichText::new(text).weak().small());
        }
        lights_section(ui, state);
    }

    fn options_section(&mut self, ui: &mut Ui, state: &mut AppState) {
        let current = state.doc.light_plus();
        let mut o = current.clone();
        ui.label(RichText::new(t!("lighting.options")).strong());
        egui::Grid::new("hp_lighting_options").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(t!("lighting.brightness"));
            ui.add(egui::Slider::new(&mut o.brightness_scale, 0.0..=4.0).step_by(0.05));
            ui.end_row();
            ui.label(t!("lighting.bounced"));
            ui.checkbox(&mut o.bounced, "").on_hover_text(t!("lighting.bounced_tip"));
            ui.end_row();
            ui.label(t!("lighting.dynamic"));
            ui.checkbox(&mut o.incremental, "").on_hover_text(t!("lighting.dynamic_tip"));
            ui.end_row();
            ui.label(t!("lighting.delay"));
            ui.add_enabled(
                o.incremental,
                egui::DragValue::new(&mut o.incremental_delay).range(0.0..=60.0).speed(0.05).suffix(" s"),
            );
            ui.end_row();
            ui.label(t!("lighting.bake_dist"));
            ui.add(egui::DragValue::new(&mut o.bake_dist).range(64.0..=65536.0).speed(8.0));
            ui.end_row();
        });
        egui::CollapsingHeader::new(t!("lighting.advanced")).id_salt("hp_lighting_advanced").show(ui, |ui| {
            egui::Grid::new("hp_lighting_advanced_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label(t!("lighting.sun_samples")).on_hover_text(t!("lighting.quick_tip"));
                ui.add(egui::DragValue::new(&mut o.samples_sun).range(1..=256).speed(0.2));
                ui.end_row();
                ui.label(t!("lighting.ambient_samples")).on_hover_text(t!("lighting.quick_tip"));
                ui.add(egui::DragValue::new(&mut o.samples_ambient).range(16..=5184).speed(1.0));
                ui.end_row();
                ui.label(t!("lighting.supersample")).on_hover_text(t!("lighting.quick_tip"));
                ui.checkbox(&mut o.supersample, "");
                ui.end_row();
                ui.label(t!("lighting.debug"));
                ui.checkbox(&mut o.debug, "").on_hover_text(t!("lighting.debug_tip"));
                ui.end_row();
            });
            if ui.button(t!("lighting.reset")).clicked() {
                o = LightPlus::default();
            }
        });
        if o != current
            && let Err(e) = state.doc.set_light_plus(&o)
        {
            state.notify(Severity::Warning, tf!("lighting.options_failed", e));
        }
    }

    fn vrad_section(&mut self, ui: &mut Ui, state: &mut AppState) {
        let current = state.doc.light_plus();
        let mut options = current.clone();
        ui.label(egui::RichText::new(t!("lighting.vrad_parameters")).strong());
        ui.checkbox(&mut options.keep_bake, t!("lighting.keep_bake")).on_hover_text(t!("lighting.keep_bake_tip"));
        ui.horizontal(|ui| {
            ui.label(t!("lighting.vrad_bounces"));
            ui.add_enabled(options.bounced, egui::DragValue::new(&mut options.bounce_count).range(0..=100));
        });
        let compile = &mut state.settings.compile;
        let mut changed = ui.checkbox(&mut compile.static_prop_shadows, t!("lighting.vrad_prop_shadows")).changed();
        changed |= ui.checkbox(&mut compile.static_prop_lighting, t!("lighting.vrad_prop_lighting")).changed();
        if changed {
            state.settings_dirty = true;
        }
        ui.checkbox(&mut options.prop_polys, t!("lighting.vrad_prop_polys"));
        ui.checkbox(&mut options.texture_shadows, t!("lighting.vrad_texture_shadows"));
        ui.checkbox(&mut options.no_disp_shadows, t!("lighting.vrad_no_disp_shadows"));
        ui.checkbox(&mut options.fast_ambient, t!("lighting.vrad_fast_ambient"));
        ui.separator();
        ui.label(egui::RichText::new(t!("lighting.vrad_entities")).strong());
        let mut entities: Vec<_> = state
            .doc
            .scene()
            .entities()
            .values()
            .map(|node| &node.entity)
            .filter(|entity| {
                matches!(
                    entity.classname.to_ascii_lowercase().as_str(),
                    "light_environment" | "shadow_control" | "env_tonemap_controller"
                )
            })
            .collect();
        entities.sort_unstable_by_key(|entity| entity.id);
        if entities.is_empty() {
            ui.label(egui::RichText::new(t!("lighting.vrad_no_entities")).weak());
        }
        for entity in entities {
            let mut enabled = options.uses_entity(entity.id);
            let label = format!("{} #{} {}", entity.classname, entity.id, entity.get("targetname").unwrap_or(""));
            if ui.checkbox(&mut enabled, label).changed() {
                if enabled {
                    options.excluded_entities.retain(|id| *id != entity.id);
                } else {
                    options.excluded_entities.push(entity.id);
                }
                options.excluded_entities.sort_unstable();
                options.excluded_entities.dedup();
            }
        }
        ui.label(egui::RichText::new(t!("lighting.vrad_geometry")).weak().small());
        if options != current
            && let Err(error) = state.doc.set_light_plus(&options)
        {
            state.notify(Severity::Warning, tf!("lighting.options_failed", error));
        }
    }
}

fn lights_section(ui: &mut Ui, state: &mut AppState) {
    egui::CollapsingHeader::new(t!("lighting.lights_heading")).id_salt("hp_lighting_lights").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for action in [Action::SoloSelectedLights, Action::MuteSelectedLights, Action::UnmuteAllLights] {
                if ui.add_enabled(action.enabled(state), egui::Button::new(action.label())).clicked() {
                    state.request(action);
                }
            }
        });
        let muted = state.doc.light_plus().excluded_entities.len();
        if muted > 0 {
            ui.label(RichText::new(tf!("lighting.muted_count", muted)).color(theme::warning()).small());
        }
        if ui.button(Action::MeasureLightCosts.label()).clicked() {
            state.runtime.lighting.costs = None;
            state.request(Action::MeasureLightCosts);
        }
        if state.runtime.lighting.costs_wanted && state.runtime.lighting.costs.is_none() {
            ui.label(RichText::new(t!("lighting.costs_pending")).weak().small());
        }
        let Some(mut sorted) = state.runtime.lighting.costs.clone() else { return };
        sorted.sort_by_key(|c| std::cmp::Reverse(c.luxels));
        let mut pick = None;
        egui::Grid::new("hp_lighting_costs").num_columns(3).spacing([8.0, 2.0]).show(ui, |ui| {
            for c in sorted.iter().take(12) {
                let name = match c.entity {
                    Some(id) => format!("{} #{id}", c.classname),
                    None => c.classname.clone(),
                };
                if ui.small_button(name).clicked() {
                    pick = c.entity;
                }
                ui.label(RichText::new(tf!("lighting.cost_luxels", fmt_count(c.luxels))).small());
                ui.label(RichText::new(tf!("lighting.cost_faces", fmt_count(c.faces as u64))).weak().small());
                ui.end_row();
            }
        });
        if let Some(id) = pick {
            state.doc.select(&[hp_editor::ObjRef::Entity(id)], SelectOp::Replace);
        }
    });
}

fn quality_row(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.label(t!("lighting.quality")).on_hover_text(t!("lighting.quality_tip"));
        let q = &mut state.settings.lighting.quality;
        let name = |q: LightingQuality| match q {
            LightingQuality::Draft => t!("lighting.quality_draft"),
            LightingQuality::Normal => t!("lighting.quality_normal"),
            LightingQuality::Final => t!("lighting.quality_final"),
        };
        let before = *q;
        egui::ComboBox::from_id_salt("hp_lighting_quality").selected_text(name(*q)).show_ui(ui, |ui| {
            for v in [LightingQuality::Draft, LightingQuality::Normal, LightingQuality::Final] {
                ui.selectable_value(q, v, name(v));
            }
        });
        if *q != before {
            state.settings_dirty = true;
        }
        let (range, tip) = if matches!(state.settings.compile.lighting, LightingSetting::Ldr) {
            ("LDR", t!("lighting.ldr_tip"))
        } else {
            ("HDR", t!("lighting.hdr_tip"))
        };
        ui.label(RichText::new(range).weak()).on_hover_text(tip);
    });
}

fn entity_lookups(state: &AppState) -> ColorCorrectionPlus {
    let mut cc = ColorCorrectionPlus::default();
    let mut slots = cc.slots.iter_mut();
    for n in state.doc.scene().entities().values() {
        let e = &n.entity;
        if !e.classname.eq_ignore_ascii_case("color_correction") {
            continue;
        }
        let Some(file) = e.get("filename").map(str::trim).filter(|f| !f.is_empty()) else { continue };
        let Some(slot) = slots.next() else { break };
        slot.name = file.chars().filter(|c| *c != '"').take(260).collect();
        slot.weight = e
            .get("maxweight")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .map_or(1.0, |v| v.clamp(0.0, 1.0));
    }
    cc
}

fn color_correction_section(ui: &mut Ui, state: &mut AppState, edits: &mut [Option<String>; 4]) {
    let current = state.doc.color_correction_plus();
    let mut cc = current.clone();
    egui::CollapsingHeader::new(t!("lighting.cc")).id_salt("hp_lighting_cc").default_open(current.is_active()).show(
        ui,
        |ui| {
            ui.label(RichText::new(t!("lighting.cc_help")).weak().small());
            for (slot, edit) in cc.slots.iter_mut().zip(edits.iter_mut()) {
                let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
                ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().slider_width = 80.0;
                    ui.add(egui::Slider::new(&mut slot.weight, 0.0..=1.0).step_by(0.05));
                    let text = edit.get_or_insert_with(|| slot.name.clone());
                    let width = (ui.available_width() - ui.spacing().item_spacing.x).max(60.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(text).desired_width(width).hint_text("materials/correction/....raw"),
                    );
                    if r.lost_focus() {
                        slot.name = text.trim().chars().filter(|c| *c != '"').take(260).collect();
                        *edit = None;
                    } else if !r.has_focus() {
                        *edit = None;
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui.button(t!("lighting.cc_from_map")).on_hover_text(t!("lighting.cc_from_map_tip")).clicked() {
                    cc = entity_lookups(state);
                }
                if ui.button(t!("lighting.cc_clear")).clicked() {
                    cc = ColorCorrectionPlus::default();
                }
            });
        },
    );
    if cc != current
        && let Err(e) = state.doc.set_color_correction_plus(&cc)
    {
        state.notify(Severity::Warning, tf!("lighting.options_failed", e));
    }
}

fn budget_section(ui: &mut Ui, state: &mut AppState) {
    ui.label(RichText::new(t!("lighting.budget")).strong());
    let Some(b) = state.runtime.lighting.budget.clone() else {
        ui.label(RichText::new(t!("lighting.budget_computing")).weak());
        return;
    };
    budget_gauge(ui, &b, 260.0);
    ui.label(
        RichText::new(tf!("lighting.budget_detail", fmt_count(b.faces as u64), fmt_count(b.luxels))).weak().small(),
    );
    let tiny = state.settings.lighting.tiny_scale;
    if b.tiny_faces > 0 {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tf!(plural(b.tiny_faces, "lighting.tiny_one", "lighting.tiny"), b.tiny_faces, tiny))
                    .color(theme::warning()),
            );
            if ui.small_button(t!("lighting.select")).clicked() {
                state.request(Action::SelectTinyLightmapFaces);
            }
        });
    }
    if b.wide_faces > 0 {
        ui.label(RichText::new(tf!(plural(b.wide_faces, "lighting.wide_one", "lighting.wide"), b.wide_faces)).small());
    }
    if b.oversized_displacements > 0 {
        ui.label(
            RichText::new(tf!("lighting.oversized_disps", b.oversized_displacements)).color(theme::error()).small(),
        );
    }
    if b.top.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(t!("lighting.top_faces")).id_salt("hp_lighting_top").show(ui, |ui| {
        egui::Grid::new("hp_lighting_top_grid").num_columns(4).striped(true).show(ui, |ui| {
            ui.label(RichText::new(t!("lighting.col_face")).small().weak());
            ui.label(RichText::new(t!("lighting.col_scale")).small().weak());
            ui.label(RichText::new(t!("lighting.col_luxels")).small().weak());
            ui.label(RichText::new(t!("lighting.col_size")).small().weak());
            ui.end_row();
            for e in b.top.iter().take(TOP_ROWS) {
                let label = if e.displacement {
                    tf!("lighting.face_disp", e.solid, e.side)
                } else {
                    tf!("lighting.face", e.solid, e.side)
                };
                if ui.link(label).on_hover_text(t!("lighting.face_tip")).clicked() {
                    select_face(state, e.solid, e.side);
                }
                ui.label(fmt_decimal(e.scale, 0));
                ui.label(format!("{} x {}", e.dims[0], e.dims[1]));
                ui.label(fmt_megabytes(e.bytes));
                ui.end_row();
            }
        });
    });
}

fn select_face(state: &mut AppState, solid: u32, side: usize) {
    if state.doc.scene().solid(solid).is_none() {
        return;
    }
    state.doc.select_faces(&[FaceRef::new(solid, side)], SelectOp::Replace);
    state.set_tool(ToolId::Texture);
    if let Some(n) = state.doc.scene().solid(solid)
        && let Some(f) = n.geom.face_for_side(side)
    {
        crate::util::center_views_on(state, f.center());
    }
}

pub fn budget_gauge(ui: &mut Ui, b: &BudgetSummary, width: f32) -> egui::Response {
    let fraction = b.fraction();
    let text = tf!("lighting.budget_gauge", fmt_megabytes(b.bytes), fmt_megabytes(b.limit), (fraction * 100.0).round());
    let color = budget_color(fraction);
    let r = ui.vertical(|ui| {
        ui.label(RichText::new(text).color(if fraction > 0.75 { color } else { ui.visuals().text_color() }));
        ui.add(
            hp_app::widgets::progress_bar(fraction.clamp(0.0, 1.0) as f32)
                .fill(color)
                .desired_width(width)
                .desired_height(6.0),
        );
    });
    r.response.on_hover_text(t!("lighting.budget_tip"))
}

fn stats_section(ui: &mut Ui, rt: &LightingRuntime) {
    let s = rt.stats;
    if s.faces == 0 {
        return;
    }
    ui.label(
        RichText::new(tf!(
            "lighting.stats",
            fmt_count(s.baked_faces as u64),
            fmt_count(s.faces as u64),
            fmt_megabytes(s.gpu_bytes),
            fmt_count(s.triangles as u64)
        ))
        .weak()
        .small(),
    );
    if s.scope_radius > 0.0 {
        let follows = if s.scope_auto { t!("lighting.scope_follows") } else { "" };
        ui.label(RichText::new(tf!("lighting.scope", s.scope_radius.round(), follows)).weak().small());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::runtime::{BakeProgress, BakeSummary, BudgetEntry};

    #[test]
    fn formats() {
        let _lock = crate::lang_lock();
        hp_app::register_translations();
        crate::register_translations();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(fmt_count(1234567), "1,234,567");
        assert_eq!(fmt_count(12), "12");
        assert_eq!(fmt_megabytes(5 * 1024 * 1024 + 300_000), "5.3 MB");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        assert_eq!(fmt_count(1234567), "1 234 567");
        assert_eq!(fmt_megabytes(1024 * 1024 / 2), "0,5 Mo");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(budget_color(2.0), theme::error());
        assert_eq!(budget_color(0.1), theme::accent());
    }

    #[test]
    fn tab_requests_actions_and_edits_options() {
        let _lock = crate::lang_lock();
        hp_app::register_translations();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut tab = LightingTab::default();
        hp_app::testing::run_ui(&mut state, |ui, state| tab.ui(ui, state));
        assert!(state.runtime.lighting.budget_wanted);
        state.runtime.lighting.progress = Some(BakeProgress {
            kind: BakeKind::Full,
            phase: BakePhase::Bounce,
            fraction: 0.5,
            elapsed_s: 2.0,
            faces: 10,
        });
        state.runtime.lighting.last = Some(BakeSummary { faces: 3, seconds: 1.0, density: 0.5, ..Default::default() });
        state.runtime.lighting.budget = Some(BudgetSummary {
            faces: 6,
            luxels: 100,
            bytes: 400,
            limit: 1 << 24,
            tiny_faces: 2,
            wide_faces: 1,
            oversized_displacements: 1,
            top: vec![BudgetEntry { solid: 1, side: 0, bytes: 400, scale: 2.0, dims: [4, 4], displacement: false }],
            revision: 1,
        });
        hp_app::testing::run_ui(&mut state, |ui, state| tab.ui(ui, state));
        let mut o = state.doc.light_plus();
        o.brightness_scale = 2.0;
        state.doc.set_light_plus(&o).unwrap();
        hp_app::testing::run_ui(&mut state, |ui, state| tab.ui(ui, state));
        assert_eq!(state.doc.light_plus().brightness_scale, 2.0);
        let kv = [
            ("filename".to_owned(), "materials/correction/test.raw".to_owned()),
            ("maxweight".to_owned(), "0.5".to_owned()),
        ];
        state.doc.create_point_entity("color_correction", glam::DVec3::ZERO, &kv).unwrap();
        let cc = entity_lookups(&state);
        assert_eq!(cc.slots[0].name, "materials/correction/test.raw");
        assert_eq!(cc.slots[0].weight, 0.5);
        assert!(cc.slots[1].name.is_empty());
        select_face(&mut state, 999_999, 0);
        assert!(state.doc.selection().faces.is_empty());
    }
}
