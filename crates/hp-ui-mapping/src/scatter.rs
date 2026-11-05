use egui::{RichText, Ui};
use hp_app::scatter::{
    self, Area, DEFAULT_MIN_BLEND, MAX_COUNT, MAX_ITEMS, ScatterError, ScatterItem, ScatterPlan, ScatterSpec,
};
use hp_app::{AppState, Severity, ViewKind, t, tf, theme};
use hp_editor::Id;

use crate::dialogs::MapDialog;
use crate::util::{centered_pos, escape_pressed, icon_title};

const CLASSES: [&str; 3] = ["prop_static", "prop_dynamic", "prop_physics_override"];

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(ScatterDialog::default())]
}

#[derive(Clone, Debug, PartialEq)]
struct Row {
    model: String,
    class: String,
    texture: String,
    min_blend: f64,
    weight: f64,
}

impl Default for Row {
    fn default() -> Self {
        Self {
            model: String::new(),
            class: CLASSES[0].to_owned(),
            texture: String::new(),
            min_blend: DEFAULT_MIN_BLEND,
            weight: 1.0,
        }
    }
}

pub(crate) struct ScatterDialog {
    rows: Vec<Row>,
    whole_map: bool,
    count: u32,
    spacing: f64,
    seed: u32,
    random_yaw: bool,
    align: bool,
    max_slope: f64,
    scale: (f64, f64),
    offset: (f64, f64),
    snap: bool,
    result: Option<Result<(ScatterSpec, ScatterPlan), String>>,
    message: Option<(bool, String)>,
    retry: bool,
}

impl Default for ScatterDialog {
    fn default() -> Self {
        Self {
            rows: vec![Row::default()],
            whole_map: false,
            count: 100,
            spacing: 32.0,
            seed: 1,
            random_yaw: true,
            align: false,
            max_slope: 45.0,
            scale: (1.0, 1.0),
            offset: (0.0, 0.0),
            snap: false,
            result: None,
            message: None,
            retry: false,
        }
    }
}

fn targets(state: &AppState) -> Vec<Id> {
    state.doc.selected_solid_ids()
}

impl ScatterDialog {
    fn build(&self, state: &AppState) -> Result<ScatterSpec, String> {
        if self.rows.iter().any(|r| r.model.trim().is_empty()) {
            return Err(t!("scatter.need_asset").to_owned());
        }
        let items: Vec<ScatterItem> = self
            .rows
            .iter()
            .map(|r| {
                let mut item = ScatterItem::prop(&r.model);
                item.classname.clone_from(&r.class);
                item.weight = r.weight;
                item.texture = Some(r.texture.trim().to_owned()).filter(|t| !t.is_empty());
                item.min_blend = r.min_blend;
                item
            })
            .collect();
        let ids = targets(state);
        let (area, targets) = if self.whole_map {
            let b = state
                .doc
                .scene()
                .solids()
                .values()
                .map(|n| n.bounds)
                .reduce(|a, b| a.union(&b))
                .ok_or_else(|| t!("scatter.targets_none").to_owned())?;
            (Area::Box(b), None)
        } else {
            let b = ids
                .iter()
                .filter_map(|id| state.doc.scene().solid(*id).map(|n| n.bounds))
                .reduce(|a, b| a.union(&b))
                .ok_or_else(|| t!("scatter.targets_none").to_owned())?;
            (Area::Box(b), Some(ids))
        };
        let mut spec = ScatterSpec::new(items, self.count as usize, area);
        spec.targets = targets;
        spec.spacing = self.spacing;
        spec.seed = u64::from(self.seed);
        spec.random_yaw = self.random_yaw;
        spec.align = self.align;
        spec.max_slope = self.max_slope;
        spec.scale = (self.scale.0, self.scale.1.max(self.scale.0));
        spec.offset = (self.offset.0, self.offset.1.max(self.offset.0));
        if self.snap {
            spec.grid = Some(state.grid_frame(ViewKind::Top));
        }
        Ok(spec)
    }

    fn preview(&mut self, state: &mut AppState) {
        self.retry = false;
        self.message = None;
        self.result = Some(self.build(state).and_then(|spec| match scatter::plan_in_state(state, &spec) {
            Ok(plan) => Ok((spec, plan)),
            Err(ScatterError::Loading(_)) => {
                self.retry = true;
                Err(t!("scatter.loading").to_owned())
            }
            Err(e) => Err(e.to_string()),
        }));
    }

    fn generate(&mut self, state: &mut AppState) {
        self.preview(state);
        let Some(Ok((spec, plan))) = self.result.take() else { return };
        match scatter::commit(state, &spec, &plan) {
            Ok(ids) => {
                let text = tf!("scatter.placed", ids.len());
                state.log(Severity::Info, text.clone());
                self.message = Some((true, text));
            }
            Err(e) => self.message = Some((false, tf!("scatter.failed", e))),
        }
        self.result = None;
    }

    fn surface_textures(state: &mut AppState) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let mut materials: Vec<String> = Vec::new();
        for id in targets(state) {
            if let Some(n) = state.doc.scene().solid(id) {
                for s in &n.solid.sides {
                    let m = crate::util::normalize_material(&s.material);
                    if !materials.contains(&m) {
                        materials.push(m);
                    }
                }
            }
        }
        for m in materials.into_iter().take(64) {
            if let Some(info) = state.assets.material_info(&m) {
                for tex in [info.base_texture.clone(), info.base_texture2.clone()].into_iter().flatten() {
                    let tex = crate::util::normalize_material(&tex);
                    if !names.contains(&tex) {
                        names.push(tex);
                    }
                }
            }
        }
        names
    }

    fn rows_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let textures = Self::surface_textures(state);
        let mut remove: Option<usize> = None;
        for (i, row) in self.rows.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t!("scatter.model"));
                    ui.add(
                        egui::TextEdit::singleline(&mut row.model)
                            .hint_text(t!("scatter.model_hint"))
                            .desired_width(250.0),
                    );
                    let class_box = egui::ComboBox::from_id_salt("class")
                        .width(140.0)
                        .selected_text(row.class.clone())
                        .show_ui(ui, |ui| {
                            for c in CLASSES {
                                ui.selectable_value(&mut row.class, c.to_owned(), c);
                            }
                        });
                    class_box.response.on_hover_text(t!("scatter.class"));
                    if self_remove_button(ui) {
                        remove = Some(i);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(t!("scatter.texture")).on_hover_text(t!("scatter.texture_tip"));
                    ui.add(
                        egui::TextEdit::singleline(&mut row.texture)
                            .hint_text(t!("scatter.texture_hint"))
                            .desired_width(180.0),
                    )
                    .on_hover_text(t!("scatter.texture_tip"));
                    egui::ComboBox::from_id_salt("pick").width(24.0).selected_text("v").show_ui(ui, |ui| {
                        ui.label(RichText::new(t!("scatter.pick_texture")).weak());
                        if ui.selectable_label(row.texture.is_empty(), "-").clicked() {
                            row.texture.clear();
                        }
                        for tex in &textures {
                            if ui.selectable_label(&row.texture == tex, tex).clicked() {
                                row.texture.clone_from(tex);
                            }
                        }
                    });
                    ui.label(t!("scatter.min_blend")).on_hover_text(t!("scatter.min_blend_tip"));
                    ui.add(egui::Slider::new(&mut row.min_blend, 0.0..=1.0).fixed_decimals(2));
                    ui.label(t!("scatter.weight"));
                    ui.add(egui::DragValue::new(&mut row.weight).speed(0.1).range(0.001..=1000.0));
                });
                ui.separator();
            });
        }
        if let Some(i) = remove
            && self.rows.len() > 1
        {
            self.rows.remove(i);
        }
    }

    fn add_selected_props(&mut self, state: &AppState) {
        for id in state.doc.selected_entity_ids() {
            let Some(n) = state.doc.scene().entity(id) else { continue };
            let Some(model) = n.entity.get("model").map(str::trim).filter(|m| m.to_ascii_lowercase().ends_with(".mdl"))
            else {
                continue;
            };
            if self.rows.len() >= MAX_ITEMS || self.rows.iter().any(|r| r.model.eq_ignore_ascii_case(model)) {
                continue;
            }
            let row = Row { model: model.to_owned(), ..Row::default() };
            match self.rows.iter_mut().find(|r| r.model.trim().is_empty()) {
                Some(empty) => *empty = row,
                None => self.rows.push(row),
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, state: &mut AppState) -> bool {
        let mut close = false;
        ui.label(RichText::new(t!("scatter.targets")).strong());
        ui.checkbox(&mut self.whole_map, t!("scatter.whole_map"));
        let n = targets(state).len();
        if self.whole_map {
            ui.label(RichText::new(" ").weak());
        } else if n > 0 {
            ui.label(RichText::new(tf!("scatter.targets_n", n)).weak());
        } else {
            ui.label(RichText::new(t!("scatter.targets_none")).color(theme::warning()));
        }
        ui.separator();
        ui.label(RichText::new(t!("scatter.assets")).strong());
        egui::ScrollArea::vertical().max_height(260.0).auto_shrink([true, true]).show(ui, |ui| {
            self.rows_ui(ui, state);
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(self.rows.len() < MAX_ITEMS, egui::Button::new(t!("scatter.add"))).clicked() {
                self.rows.push(Row::default());
            }
            if ui.button(t!("scatter.from_props")).on_hover_text(t!("scatter.from_props_tip")).clicked() {
                self.add_selected_props(state);
            }
        });
        ui.separator();
        egui::Grid::new("scatter_params").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(t!("scatter.count"));
            ui.add(egui::DragValue::new(&mut self.count).range(1..=MAX_COUNT as u32));
            ui.end_row();
            ui.label(t!("scatter.spacing")).on_hover_text(t!("scatter.spacing_tip"));
            ui.add(egui::DragValue::new(&mut self.spacing).speed(1.0).range(0.0..=4096.0));
            ui.end_row();
            ui.label(t!("scatter.seed"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.seed));
                if ui.button(t!("scatter.reroll")).clicked() {
                    self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                }
            });
            ui.end_row();
            ui.label(t!("scatter.slope"));
            ui.add(egui::DragValue::new(&mut self.max_slope).range(0.0..=90.0).suffix(" deg"));
            ui.end_row();
            ui.label(t!("scatter.scale")).on_hover_text(t!("scatter.scale_tip"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.scale.0).speed(0.01).range(0.01..=100.0));
                ui.add(egui::DragValue::new(&mut self.scale.1).speed(0.01).range(0.01..=100.0));
            });
            ui.end_row();
            ui.label(t!("scatter.offset")).on_hover_text(t!("scatter.offset_tip"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.offset.0).speed(0.5).range(-4096.0..=4096.0));
                ui.add(egui::DragValue::new(&mut self.offset.1).speed(0.5).range(-4096.0..=4096.0));
            });
            ui.end_row();
        });
        ui.checkbox(&mut self.random_yaw, t!("scatter.yaw"));
        ui.checkbox(&mut self.align, t!("scatter.align"));
        ui.checkbox(&mut self.snap, t!("scatter.snap")).on_hover_text(t!("scatter.snap_tip"));
        ui.separator();
        self.result_ui(ui);
        ui.horizontal(|ui| {
            if ui.button(t!("scatter.preview")).clicked() {
                self.preview(state);
            }
            let ready = !self.rows.is_empty() && (self.whole_map || n > 0);
            if ui
                .add_enabled(ready, egui::Button::new(RichText::new(t!("scatter.generate")).strong()))
                .on_hover_text(t!("scatter.generate_tip"))
                .clicked()
            {
                self.generate(state);
            }
            if ui.button(t!("common.close")).clicked() {
                close = true;
            }
        });
        close
    }

    fn result_ui(&self, ui: &mut Ui) {
        if let Some((ok, text)) = &self.message {
            let color = if *ok { theme::success() } else { theme::error() };
            ui.label(RichText::new(text).color(color));
        }
        match &self.result {
            Some(Ok((spec, plan))) => {
                ui.label(RichText::new(tf!("scatter.result", plan.placements.len(), plan.asked)).strong());
                for (i, n) in plan.per_item.iter().enumerate() {
                    let name = spec
                        .items
                        .get(i)
                        .and_then(|it| it.keyvalues.iter().find(|(k, _)| k == "model"))
                        .map_or("", |(_, v)| v.as_str());
                    ui.label(RichText::new(tf!("scatter.per_item", name, n)).weak().small());
                }
                ui.label(
                    RichText::new(tf!(
                        "scatter.rejected",
                        plan.missed,
                        plan.too_steep,
                        plan.too_close,
                        plan.wrong_texture
                    ))
                    .weak()
                    .small(),
                );
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(theme::warning()));
            }
            None => {}
        }
    }
}

fn self_remove_button(ui: &mut Ui) -> bool {
    ui.small_button("x").on_hover_text(t!("scatter.remove")).clicked()
}

impl MapDialog for ScatterDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.ui.show_scatter {
            return;
        }
        if self.retry {
            self.preview(state);
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        let mut open = true;
        let mut close = false;
        let response = hp_app::chrome::Window::new(icon_title("✨", t!("scatter.title")))
            .id(egui::Id::unique("hp_scatter_window"))
            .default_pos(centered_pos(ctx, egui::vec2(560.0, 520.0)))
            .default_width(560.0)
            .open(&mut open)
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| close = self.body(ui, state));
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if !open || close {
            state.ui.show_scatter = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use hp_app::testing::{run_ctx, test_state};
    use hp_editor::ObjRef;

    use super::*;

    #[test]
    fn the_dialog_previews_then_generates_in_one_step() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        let mut d = ScatterDialog::default();
        run_ctx(&mut state, |ctx, s| d.show(ctx, s));
        state.ui.show_scatter = true;
        run_ctx(&mut state, |ctx, s| d.show(ctx, s));
        assert!(state.ui.show_scatter);

        d.preview(&mut state);
        assert!(matches!(d.result, Some(Err(_))), "an asset without a model is refused");
        d.rows[0].model = "models/props/a.mdl".to_owned();
        d.whole_map = true;
        d.count = 20;
        d.preview(&mut state);
        let Some(Ok((_, plan))) = &d.result else { panic!("{:?}", d.result) };
        assert!(!plan.placements.is_empty());
        run_ctx(&mut state, |ctx, s| d.show(ctx, s));

        let before = state.doc.stats().entities;
        let (undo, _) = state.doc.history_len();
        d.generate(&mut state);
        assert!(state.doc.stats().entities > before);
        assert_eq!(state.doc.history_len().0, undo + 1);
        assert!(d.message.as_ref().is_some_and(|m| m.0));

        d.whole_map = false;
        state.doc.clear_selection();
        d.preview(&mut state);
        assert!(matches!(d.result, Some(Err(_))), "no surface selected");

        let prop = state
            .doc
            .create_point_entity(
                "prop_static",
                glam::DVec3::new(0.0, 0.0, 8.0),
                &[("model".to_owned(), "models/props/b.mdl".to_owned())],
            )
            .expect("prop");
        state.doc.set_selection(&[ObjRef::Entity(prop)]);
        d.rows[0].model.clear();
        d.add_selected_props(&state);
        assert_eq!(d.rows[0].model, "models/props/b.mdl");
        d.add_selected_props(&state);
        assert_eq!(d.rows.len(), 1, "a model is added once");

        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        run_ctx(&mut state, |ctx, s| d.show(ctx, s));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        state.ui.show_scatter = false;
        run_ctx(&mut state, |ctx, s| d.show(ctx, s));
    }
}
