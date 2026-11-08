use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{Align, ComboBox, DragValue, Id, Label, Layout, Popup, Response, RichText, Shape, Ui, vec2};
use hp_app::actions::key_label;
use hp_app::runtime::AssetKind;
use hp_app::state::RightTab;
use hp_app::{Severity, t, tf, theme};
use hp_fgd::{ClassKind, Property, PropertyType};

use crate::cx::{Cx, ModelPick, push_recent_model, recent_models, set_model_pick};
use crate::edit::Op;
use crate::names::MapNames;
use crate::pick::{PickPurpose, ViewPicks, entities_matching_target, mark, start_asset_pick};
use crate::sounds::{self, GameSound};
use crate::values::{
    Color255, Tri, choice_index, fmt_num_rounded, format_angles, format_axis, format_color1, format_vec3,
    is_missing_model, is_valid_float, is_valid_integer, parse_angles, parse_axis, parse_bool, parse_color1,
    parse_pitch, parse_vec3, preview_material_name, sky_names, sound_key_name,
};
use crate::view::{Agg, EntityView, PropRow, Target};
use crate::widgets::{
    CLICK, CommitAction, CompleteMode, FieldOpts, PickerItem, PickerOpts, TextBuffers, autocomplete_keys,
    autocomplete_popup, empty_note, fit_scroll_content, icon, picker_popup, picker_popup_with, search_box, side_button,
    thumbnail, yaw_dial,
};

pub(crate) const PICK_GLYPH: &str = "⌖";
pub(crate) const MARK_GLYPH: &str = "◉";
pub(crate) const BROWSE_GLYPH: &str = "…";

const BUTTON_W: f32 = 22.0;

const MISSING_ICON_W: f32 = 16.0;

const MIN_ANGLE_FIELD_W: f32 = 32.0;

struct FileBrowse {
    rx: Receiver<Option<PathBuf>>,
    target: Target,
    key: String,
}

pub(crate) struct SmartEdit {
    salt: Id,
    pub buffers: TextBuffers,
    pub filter: String,
    pub show_keys: bool,
    pub update_references: bool,
    new_key: String,
    new_value: String,
    browse: Option<FileBrowse>,
    skies: Option<(usize, usize, Vec<String>)>,
    pub picks: ViewPicks,
}

impl SmartEdit {
    pub fn new(salt: &str) -> SmartEdit {
        SmartEdit {
            salt: Id::unique(("hp_ui_entities.smartedit", salt)),
            buffers: TextBuffers::default(),
            filter: String::new(),
            show_keys: false,
            update_references: true,
            new_key: String::new(),
            new_value: String::new(),
            browse: None,
            skies: None,
            picks: ViewPicks::new(salt),
        }
    }

    fn field_id(&self, key: &str) -> Id {
        self.salt.with(("field", key.to_ascii_lowercase()))
    }

    #[cfg(test)]
    pub fn test_field_id(&self, key: &str) -> Id {
        self.field_id(key)
    }

    pub fn begin_frame(&mut self) {
        self.buffers.begin_frame();
    }

    pub fn end_frame(&mut self, ctx: &egui::Context, map_path: Option<&Path>, ops: &mut Vec<Op>) {
        self.buffers.end_frame(ctx);
        for (action, text) in self.buffers.take_commits() {
            if let Some(op) = action.into_op(text, self.update_references) {
                ops.push(op);
            }
        }
        if let Some(b) = &self.browse {
            match b.rx.try_recv() {
                Ok(picked) => {
                    if let Some(path) = picked {
                        ops.push(Op::SetKey {
                            target: b.target.clone(),
                            key: b.key.clone(),
                            value: instance_path(map_path, &path),
                            merge: false,
                        });
                    }
                    self.browse = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
                Err(TryRecvError::Disconnected) => self.browse = None,
            }
        }
    }

    pub fn poll_picks(&mut self, cx: &mut Cx, ops: &mut Vec<Op>) {
        self.picks.poll(cx.doc, cx.pick, ops);
    }

    pub fn name_prompt_ui(&mut self, ui: &mut Ui, ops: &mut Vec<Op>) {
        let Some(prompt) = self.picks.prompt.as_mut() else { return };
        let mut done = false;
        let mut accept = false;
        egui::Frame::new()
            .fill(theme::accent_bg())
            .stroke(egui::Stroke::new(1.0, theme::accent()))
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                let text = tf!("entities.name_prompt", prompt.class, prompt.key);
                ui.add(Label::new(RichText::new(text).color(theme::accent())).wrap());
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 2.0 * 70.0).max(60.0);
                    let edit = ui.add(egui::TextEdit::singleline(&mut prompt.name).desired_width(w));
                    if std::mem::take(&mut prompt.focus) {
                        edit.request_focus();
                    }
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        accept = true;
                    }
                    let valid = is_valid_name(&prompt.name);
                    if ui.add_enabled(valid, egui::Button::new(t!("common.ok"))).clicked() {
                        accept = true;
                    }
                    if ui.button(t!("common.cancel")).clicked() {
                        done = true;
                    }
                });
            });
        if accept && is_valid_name(&prompt.name) {
            ops.push(Op::NameAndSet {
                named: prompt.entity,
                name: prompt.name.trim().to_owned(),
                target: prompt.target.clone(),
                key: prompt.key.clone(),
            });
            done = true;
        }
        if done {
            self.picks.prompt = None;
        }
        ui.add_space(4.0);
    }

    fn pick_button(&mut self, ui: &mut Ui, cx: &mut Cx, target: &Target, key: &str, purpose: PickPurpose, tip: &str) {
        let waiting = self.picks.is_waiting(cx.pick, key);
        let resp = side_button(ui, icon(PICK_GLYPH), waiting).on_hover_text(tip);
        if resp.clicked() {
            if waiting {
                self.picks.cancel(cx.pick, key);
            } else {
                let purpose = match purpose {
                    PickPurpose::Faces { .. } => PickPurpose::Faces { add: ui.input(|i| i.modifiers.command) },
                    p => p,
                };
                self.picks.start(cx.pick, target.clone(), key, purpose);
            }
        }
    }

    fn browse_button(
        &mut self,
        ui: &mut Ui,
        cx: &mut Cx,
        view: &EntityView,
        row: &PropRow,
        kind: AssetKind,
        tip: &str,
    ) {
        let waiting = cx
            .runtime
            .entities
            .asset_pick
            .as_ref()
            .is_some_and(|p| p.result.is_none() && p.kind == kind && p.key.eq_ignore_ascii_case(&row.key));
        let resp = side_button(ui, BROWSE_GLYPH, waiting).on_hover_text(tip);
        if !resp.clicked() {
            return;
        }
        if waiting {
            cx.runtime.entities.asset_pick = None;
            return;
        }
        let class = if view.target.is_world() {
            "worldspawn".to_owned()
        } else {
            view.single_class().map_or_else(|| tf!("entities.n_classes", view.classes.len()), str::to_owned)
        };
        let label = format!("{class} ({})", row.key);
        start_asset_pick(&mut cx.runtime.entities.asset_pick, kind, &view.target, &row.key, label, row.agg.text());
        show_browser_for(cx, kind);
    }

    fn preview_particle_button(&mut self, ui: &mut Ui, cx: &mut Cx, view: &EntityView, row: &PropRow) {
        let open = cx.runtime.lab.particle_preview.open;
        let resp = side_button(ui, "▶", open).on_hover_text(t!("entities.preview_particles"));
        if !resp.clicked() {
            return;
        }
        if open {
            cx.runtime.lab.particle_preview.open = false;
            let waiting = cx.runtime.entities.asset_pick.as_ref().is_some_and(|p| {
                p.result.is_none() && p.kind == AssetKind::Particle && p.key.eq_ignore_ascii_case(&row.key)
            });
            if waiting {
                cx.runtime.entities.asset_pick = None;
            }
            return;
        }
        let class = view.single_class().map_or_else(|| tf!("entities.n_classes", view.classes.len()), str::to_owned);
        let label = format!("{class} ({})", row.key);
        let current = row.agg.text();
        start_asset_pick(
            &mut cx.runtime.entities.asset_pick,
            AssetKind::Particle,
            &view.target,
            &row.key,
            label,
            current,
        );
        let preview = &mut cx.runtime.lab.particle_preview;
        preview.open = true;
        let current = current.trim();
        if !current.is_empty() {
            preview.selected = Some(current.to_owned());
        }
    }

    pub fn properties(&mut self, ui: &mut Ui, cx: &mut Cx, view: &EntityView, names: &MapNames, ops: &mut Vec<Op>) {
        fit_scroll_content(ui);
        ui.horizontal(|ui| {
            let keys_tip = t!("entities.show_keys_tip");
            if ui
                .add(egui::Button::selectable(self.show_keys, t!("entities.show_keys")))
                .on_hover_text(keys_tip)
                .clicked()
            {
                self.show_keys = !self.show_keys;
            }
            search_box(ui, &mut self.filter, t!("entities.filter_props"));
        });
        ui.add_space(2.0);
        let query = self.filter.trim().to_ascii_lowercase();
        let mut shown = 0usize;
        for row in &view.props {
            if !query.is_empty() {
                let label = row.prop.as_ref().map_or("", |p| p.label()).to_ascii_lowercase();
                if !row.key.to_ascii_lowercase().contains(&query) && !label.contains(&query) {
                    continue;
                }
            }
            ui.push_id(row.key.to_ascii_lowercase(), |ui| self.row(ui, cx, view, names, row, shown, ops));
            shown += 1;
        }
        if shown == 0 {
            empty_note(ui, if query.is_empty() { t!("entities.no_properties") } else { t!("entities.no_match") });
        }
        if let Target::Entities(ids) = &view.target
            && let [id] = ids.as_slice()
            && let Some(a) = crate::instance_ui::instance_section(ui, cx.doc, *id, ops)
        {
            cx.request(a);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        ui: &mut Ui,
        cx: &mut Cx,
        view: &EntityView,
        names: &MapNames,
        row: &PropRow,
        index: usize,
        ops: &mut Vec<Op>,
    ) {
        let full = ui.available_width();
        let spacing = ui.spacing().item_spacing.x;
        let label_w = (full * 0.4).clamp(70.0, 200.0);
        let widget_w = (full - label_w - spacing).max(60.0);
        let row_h = ui.spacing().interact_size.y;
        let bg = ui.painter().add(Shape::Noop);
        let inner = ui.horizontal(|ui| {
            let label = ui
                .allocate_ui_with_layout(vec2(label_w, row_h), Layout::left_to_right(Align::Center), |ui| {
                    ui.set_min_width(label_w);
                    let marker = if row.modified { "*" } else { " " };
                    ui.add_sized(vec2(8.0, row_h), Label::new(RichText::new(marker).color(theme::accent()).strong()));
                    let text = match &row.prop {
                        Some(p) if !self.show_keys => p.label().to_owned(),
                        _ => row.key.clone(),
                    };
                    let rich = if row.prop.is_none() { RichText::new(text).italics() } else { RichText::new(text) };
                    ui.add(Label::new(rich).truncate().show_tooltip_when_elided(false).sense(CLICK))
                })
                .inner;
            let class = if view.classes.len() == 1 { view.classes.first().map(String::as_str) } else { None };
            let label = label.on_hover_ui(|ui| prop_tooltip(ui, row, class));
            label.context_menu(|ui| {
                if let Some(p) = &row.prop
                    && ui.button(t!("entities.reset_default")).clicked()
                {
                    ops.push(Op::ResetKey {
                        target: view.target.clone(),
                        key: row.key.clone(),
                        default: p.default_value(),
                    });
                    ui.close();
                }
                if ui.add_enabled(row.agg.value().is_some(), egui::Button::new(t!("entities.copy_value"))).clicked() {
                    ui.ctx().copy_text(row.agg.text().to_owned());
                    ui.close();
                }
                if ui.button(t!("entities.copy_key")).clicked() {
                    ui.ctx().copy_text(row.key.clone());
                    ui.close();
                }
                if ui.add_enabled(row.agg != Agg::Absent, egui::Button::new(t!("entities.remove_key"))).clicked() {
                    ops.push(Op::RemoveKey { target: view.target.clone(), key: row.key.clone() });
                    ui.close();
                }
                if row.prop.as_ref().is_some_and(|p| p.kind == PropertyType::TargetSource) && !view.target.is_world() {
                    ui.separator();
                    ui.checkbox(&mut self.update_references, t!("entities.update_refs"))
                        .on_hover_text(t!("entities.update_refs_tip"));
                }
            });
            ui.allocate_ui_with_layout(vec2(widget_w, row_h), Layout::left_to_right(Align::Center), |ui| {
                ui.set_width(widget_w);
                self.value_widget(ui, cx, view, names, row, ops);
            });
        });
        if index % 2 == 1 {
            let rect = inner.response.rect.expand2(vec2(2.0, 1.0));
            ui.painter().set(bg, Shape::rect_filled(rect, 2.0, crate::widgets::stripe_color(ui)));
        }
    }

    fn value_widget(
        &mut self,
        ui: &mut Ui,
        cx: &mut Cx,
        view: &EntityView,
        names: &MapNames,
        row: &PropRow,
        ops: &mut Vec<Op>,
    ) {
        let target = &view.target;
        let id = self.field_id(&row.key);
        let prop = row.prop.as_ref();
        let mut kind = prop.map_or(PropertyType::String, |p| p.kind.clone());
        if target.is_world() && row.key.eq_ignore_ascii_case("detailmaterial") {
            kind = PropertyType::Material;
        }
        if prop.is_none() && row.key.eq_ignore_ascii_case("model") && is_studio_model_value(&row.agg) {
            kind = PropertyType::Studio;
        }
        let display = row.agg.text();
        if prop.is_some_and(|p| p.readonly) {
            ui.add(Label::new(RichText::new(display).weak()).truncate().selectable(true));
            return;
        }
        let default = prop.and_then(|p| p.default.as_deref()).unwrap_or("");
        let hint: &str = if row.agg.is_mixed() { t!("common.multiple_values") } else { default };
        let w = ui.available_width();
        let spacing = ui.spacing().item_spacing.x;
        let set_key = || CommitAction::SetKey { target: target.clone(), key: row.key.clone() };
        let push_set = |ops: &mut Vec<Op>, value: String, merge: bool| {
            ops.push(Op::SetKey { target: target.clone(), key: row.key.clone(), value, merge });
        };
        let effective: Option<&str> = match &row.agg {
            Agg::Value(v) => Some(v),
            Agg::Absent => prop.and_then(|p| p.default.as_deref()),
            Agg::Mixed => None,
        };

        match kind {
            PropertyType::Choices | PropertyType::String if row.key.eq_ignore_ascii_case("soundscape") => {
                let text_w = (w - BUTTON_W - spacing).max(40.0);
                self.buffers.field(ui, id, display, FieldOpts { hint, width: text_w, ..FieldOpts::default() }, set_key);
                let btn = side_button(ui, icon("▾"), false).on_hover_text(t!("entities.pick_soundscape"));
                if picker_is_open(&btn) {
                    let mut values: Vec<String> = prop
                        .into_iter()
                        .flat_map(|property| property.choices.iter().map(|choice| choice.value.clone()))
                        .collect();
                    if let Some(names) = crate::catalog::soundscape_names(ui.ctx(), cx.assets) {
                        values.extend(names.iter().cloned());
                    }
                    values.sort_unstable_by_key(|name| name.to_ascii_lowercase());
                    values.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
                    if let Some(value) =
                        picker_popup(&btn, true, values.len(), |index| PickerItem { name: &values[index], detail: "" })
                    {
                        push_set(ops, value, false);
                    }
                }
            }
            PropertyType::Boolean => {
                let tri = match effective {
                    Some(v) => {
                        if parse_bool(v) {
                            Tri::On
                        } else {
                            Tri::Off
                        }
                    }
                    None => Tri::Mixed,
                };
                let mut checked = tri == Tri::On;
                let resp = ui.add(egui::Checkbox::new(&mut checked, "").indeterminate(tri == Tri::Mixed));
                if resp.clicked() {
                    push_set(ops, if tri.toggled() { "1" } else { "0" }.to_owned(), false);
                }
            }
            PropertyType::Choices if prop.is_some_and(|p| !p.choices.is_empty()) => {
                let Some(p) = prop else { return };
                let current = effective.and_then(|v| choice_index(&p.choices, v));
                let selected_text = match (&row.agg, current) {
                    (Agg::Mixed, _) => t!("common.multiple_values").to_owned(),
                    (_, Some(i)) => choice_label(&p.choices[i]).to_owned(),
                    (_, None) => display.to_owned(),
                };
                ComboBox::from_id_salt(id.with("choices")).width(w).selected_text(selected_text).truncate().show_ui(
                    ui,
                    |ui| {
                        for (i, c) in p.choices.iter().enumerate() {
                            let resp = ui.selectable_label(current == Some(i), choice_label(c)).on_hover_text(&c.value);
                            if resp.clicked() {
                                push_set(ops, c.value.clone(), false);
                            }
                        }
                    },
                );
            }
            PropertyType::Integer | PropertyType::Float => {
                let integer = kind == PropertyType::Integer;
                let live = self.buffers.live_text(id).unwrap_or(display);
                let valid = if integer { is_valid_integer(live) } else { is_valid_float(live) };
                let color = (!valid).then_some(theme::error());
                let opts = FieldOpts { hint, width: w, color, ..FieldOpts::default() };
                let resp = self.buffers.field(ui, id, display, opts, || CommitAction::SetNumber {
                    target: target.clone(),
                    key: row.key.clone(),
                    integer,
                });
                if !valid {
                    resp.on_hover_text(if integer {
                        t!("entities.invalid_integer")
                    } else {
                        t!("entities.invalid_number")
                    });
                }
            }
            PropertyType::Color255 => {
                let parsed = effective.and_then(Color255::parse);
                let mut rgb = parsed.map_or([128, 128, 128], |c| c.to_srgb());
                let tip = if parsed.is_some_and(|c| c.is_unset()) {
                    t!("entities.color_unset")
                } else {
                    t!("entities.pick_color")
                };
                let btn = ui.color_edit_button_srgb(&mut rgb).on_hover_text(tip);
                if btn.changed() {
                    let base = parsed.unwrap_or_else(|| {
                        let brightness = Color255::parse(default).and_then(|c| c.brightness);
                        Color255 { rgb: [255.0; 3], brightness }
                    });
                    push_set(ops, base.with_srgb(rgb).format(), true);
                }
                if let Some(c) = parsed
                    && let Some(mut b) = c.brightness
                {
                    let resp = ui
                        .add_sized(
                            vec2(48.0, ui.spacing().interact_size.y),
                            DragValue::new(&mut b).speed(1.0).max_decimals(2),
                        )
                        .on_hover_text(t!("entities.brightness"));
                    if resp.changed() {
                        push_set(ops, Color255 { brightness: Some(b.max(0.0)), ..c }.format(), true);
                    }
                }
                let rest = ui.available_width();
                self.buffers.field(ui, id, display, FieldOpts { hint, width: rest, ..FieldOpts::default() }, set_key);
            }
            PropertyType::Color1 => {
                let parsed = effective.and_then(parse_color1);
                let mut rgb = parsed.unwrap_or([0.5, 0.5, 0.5]);
                if ui.color_edit_button_rgb(&mut rgb).on_hover_text(t!("entities.pick_color")).changed() {
                    push_set(ops, format_color1(rgb), true);
                }
                let rest = ui.available_width();
                self.buffers.field(ui, id, display, FieldOpts { hint, width: rest, ..FieldOpts::default() }, set_key);
            }
            PropertyType::Studio => {
                let missing = match &row.agg {
                    Agg::Value(v) if is_missing_model(cx.assets.fs(), v) => Some(v.trim()),
                    _ => None,
                };
                let warn_w = if missing.is_some() { MISSING_ICON_W + spacing } else { 0.0 };
                if let Some(path) = missing {
                    let tip = tf!("entities.model_missing", path);
                    let warn = Label::new(icon("⚠").color(theme::warning()));
                    ui.add_sized(vec2(MISSING_ICON_W, ui.spacing().interact_size.y), warn).on_hover_text(tip);
                }
                let text_w = (w - 2.0 * (BUTTON_W + spacing) - warn_w).max(40.0);
                let color = missing.map(|_| theme::warning());
                let opts = FieldOpts { hint, width: text_w, color, ..FieldOpts::default() };
                let resp = self.buffers.field(ui, id, display, opts, set_key);
                if let Some(path) = missing {
                    resp.on_hover_text(tf!("entities.model_missing", path));
                }
                let browse = side_button(ui, BROWSE_GLYPH, false);
                if browse.on_hover_text(t!("entities.browse_models")).clicked() {
                    let class = view.single_class().unwrap_or("");
                    set_model_pick(
                        ui.ctx(),
                        Some(ModelPick {
                            target: target.clone(),
                            key: row.key.clone(),
                            label: format!("{class} ({})", row.key),
                        }),
                    );
                    cx.show_models_tab();
                }
                crate::widgets::side_menu_button(ui, icon("▾"), |ui| {
                    ui.set_max_width(380.0);
                    let recent = recent_models(ui.ctx());
                    if recent.is_empty() {
                        ui.label(RichText::new(t!("entities.no_recent_models")).weak());
                    }
                    for m in recent {
                        if ui.button(&m).clicked() {
                            push_recent_model(ui.ctx(), &m);
                            push_set(ops, m, false);
                            ui.close();
                        }
                    }
                })
                .on_hover_text(t!("entities.recent_models"));
            }
            PropertyType::Material | PropertyType::Decal | PropertyType::Sprite => {
                let row_h = ui.spacing().interact_size.y;
                let name = preview_material_name(effective.unwrap_or(""));
                let thumb = cx.thumbs.get(&name).copied();
                if !name.is_empty() {
                    thumbnail(ui, thumb, row_h).on_hover_ui(|ui| {
                        thumbnail(ui, thumb, 128.0);
                        ui.label(&name);
                    });
                }
                let used = if name.is_empty() { 0.0 } else { row_h + spacing };
                let text_w = (w - used - 2.0 * (BUTTON_W + spacing)).max(40.0);
                self.buffers.field(ui, id, display, FieldOpts { hint, width: text_w, ..FieldOpts::default() }, set_key);
                let tip = tf!("entities.use_current_material", cx.current_material);
                let enabled = !cx.current_material.is_empty();
                let use_btn = ui.add_enabled_ui(enabled, |ui| side_button(ui, icon("◩"), false)).inner;
                if use_btn.on_hover_text(tip).clicked() {
                    let value = if kind == PropertyType::Sprite {
                        format!("{}.vmt", cx.current_material)
                    } else {
                        cx.current_material.to_owned()
                    };
                    push_set(ops, value, false);
                }
                let asset = match kind {
                    PropertyType::Decal => AssetKind::Decal,
                    PropertyType::Sprite => AssetKind::Sprite,
                    _ => AssetKind::Material,
                };
                self.browse_button(ui, cx, view, row, asset, t!("entities.browse_materials"));
            }
            PropertyType::Sound => {
                let game = sounds::game_sounds(ui.ctx(), cx.assets);
                let game: &[GameSound] = game.as_deref().map_or(&[], Vec::as_slice);
                let text_w = (w - 3.0 * (BUTTON_W + spacing)).max(40.0);
                self.buffers.field(ui, id, display, FieldOpts { hint, width: text_w, ..FieldOpts::default() }, set_key);
                let btn = side_button(ui, icon("♪"), false).on_hover_text(t!("entities.browse_sounds"));
                if picker_is_open(&btn) {
                    let files = cx.assets.sounds();
                    let assets = cx.assets;
                    let n = game.len();
                    let name_of = |i: usize| -> &str {
                        match game.get(i) {
                            Some(g) => &g.name,
                            None => files.get(i - n).map_or("", |s| sound_key_name(&s.name)),
                        }
                    };
                    let volume = cx.settings.browser.volume();
                    let preview = |i: usize| {
                        sounds::play(assets, name_of(i), game, volume);
                    };
                    let opts = PickerOpts {
                        allow_custom: true,
                        search_details: true,
                        preview: Some(&preview),
                        preview_tip: t!("entities.play_sound"),
                        min_width: 440.0,
                    };
                    let picked = picker_popup_with(&btn, opts, n + files.len(), |i| PickerItem {
                        name: name_of(i),
                        detail: game.get(i).map_or("", |g| g.wave.as_str()),
                    });
                    if let Some(s) = picked {
                        push_set(ops, s, false);
                    }
                }
                let current = effective.unwrap_or("").trim();
                let playing = !current.is_empty() && sounds::is_playing(current);
                let playable = sounds::preview_file(current, game).is_some_and(|f| sounds::can_preview(&f));
                let glyph = if playing { "■" } else { "▶" };
                let play = ui.add_enabled_ui(playable || playing, |ui| side_button(ui, icon(glyph), false)).inner;
                let tip = if playing { t!("entities.stop_sound") } else { t!("entities.play_sound") };
                let play = play.on_hover_text(tip).on_disabled_hover_text(t!("entities.play_unavailable"));
                if play.clicked() {
                    if playing {
                        sounds::stop();
                    } else if !sounds::play(cx.assets, current, game, cx.settings.browser.volume()) {
                        cx.notify(Severity::Warning, tf!("entities.play_failed", current));
                    }
                }
                if playing {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                }
                self.browse_button(ui, cx, view, row, AssetKind::Sound, t!("entities.browse_sound_browser"));
            }
            PropertyType::ParticleSystem => {
                let text_w = (w - 3.0 * (BUTTON_W + spacing)).max(40.0);
                self.buffers.field(ui, id, display, FieldOpts { hint, width: text_w, ..FieldOpts::default() }, set_key);
                let btn = side_button(ui, icon("▾"), false).on_hover_text(t!("entities.pick_particle"));
                if picker_is_open(&btn) {
                    let systems = cx.particles.map_or(&[][..], |c| c.systems.as_slice());
                    let opts = PickerOpts {
                        allow_custom: true,
                        search_details: true,
                        min_width: 360.0,
                        ..PickerOpts::default()
                    };
                    let picked = picker_popup_with(&btn, opts, systems.len(), |i| PickerItem {
                        name: systems.get(i).map_or("", |s| s.name.as_str()),
                        detail: systems.get(i).map_or("", |s| s.file.as_str()),
                    });
                    if let Some(p) = picked {
                        push_set(ops, p, false);
                    }
                }
                self.browse_button(ui, cx, view, row, AssetKind::Particle, t!("entities.browse_particles"));
                self.preview_particle_button(ui, cx, view, row);
            }
            PropertyType::Scene | PropertyType::Script => {
                let text_w = (w - BUTTON_W - spacing).max(40.0);
                self.buffers.field(ui, id, display, FieldOpts { hint, width: text_w, ..FieldOpts::default() }, set_key);
                let scene = kind == PropertyType::Scene;
                let tip = if scene { t!("entities.pick_scene") } else { t!("entities.pick_script") };
                let btn = side_button(ui, icon("▾"), false).on_hover_text(tip);
                if picker_is_open(&btn) {
                    let files = if scene {
                        crate::catalog::scene_files(ui.ctx(), cx.assets)
                    } else {
                        crate::catalog::script_files(ui.ctx(), cx.assets)
                    };
                    let files: &[String] = files.as_deref().map_or(&[], Vec::as_slice);
                    let picked = picker_popup(&btn, true, files.len(), |i| PickerItem {
                        name: files.get(i).map_or("", String::as_str),
                        detail: "",
                    });
                    if let Some(p) = picked {
                        push_set(ops, p, false);
                    }
                }
            }
            PropertyType::ScriptList => {
                let text_w = (w - BUTTON_W - spacing).max(40.0);
                self.buffers.field(ui, id, display, FieldOpts { hint, width: text_w, ..FieldOpts::default() }, set_key);
                let btn = side_button(ui, icon("+"), false).on_hover_text(t!("entities.add_script"));
                if picker_is_open(&btn) {
                    let files = crate::catalog::script_files(ui.ctx(), cx.assets);
                    let files: &[String] = files.as_deref().map_or(&[], Vec::as_slice);
                    let picked = picker_popup(&btn, true, files.len(), |i| PickerItem {
                        name: files.get(i).map_or("", String::as_str),
                        detail: "",
                    });
                    if let Some(p) = picked {
                        let base = effective.unwrap_or("");
                        push_set(ops, crate::catalog::append_to_list(base, &p), false);
                    }
                }
            }
            PropertyType::SideList => {
                let text_w = (w - BUTTON_W - spacing).max(40.0);
                let count = effective.map_or(0, |v| hp_editor::side_index::parse_side_list(v).len());
                let opts = FieldOpts { hint, width: text_w, ..FieldOpts::default() };
                self.buffers.field(ui, id, display, opts, set_key).on_hover_text(tf!("entities.sides_count", count));
                let tip = t!("entities.pick_faces");
                self.pick_button(ui, cx, target, &row.key, PickPurpose::Faces { add: false }, tip);
            }
            PropertyType::TargetSource => {
                let action = || CommitAction::Targetname { target: target.clone() };
                let opts = TargetOpts { validate: false, dropdown: false, mode: CompleteMode::Suggest };
                self.target_field(ui, id, display, hint, w, names, names.names(), opts, action);
            }
            PropertyType::TargetDestination | PropertyType::TargetNameOrClass | PropertyType::FilterClass => {
                let filter = kind == PropertyType::FilterClass;
                let opts = TargetOpts { validate: true, dropdown: filter, mode: CompleteMode::Complete };
                let field_w = (w - 2.0 * (BUTTON_W + spacing)).max(40.0);
                let list = if filter { names.filters() } else { names.names() };
                self.target_field(ui, id, display, hint, field_w, names, list, opts, set_key);
                self.pick_button(ui, cx, target, &row.key, PickPurpose::Target, t!("entities.pick_entity"));
                let current = effective.unwrap_or("").trim();
                let can_mark = !current.is_empty() && !current.starts_with('!') && names.target_exists(current);
                let mark_btn = ui.add_enabled_ui(can_mark, |ui| side_button(ui, icon(MARK_GLYPH), false)).inner;
                let doc: &hp_editor::Document = cx.doc;
                let mark_btn = mark_btn
                    .on_hover_ui(|ui| {
                        ui.label(tf!("entities.mark_tip", entities_matching_target(doc, current).len()));
                    })
                    .on_disabled_hover_text(t!("entities.mark_none"));
                if mark_btn.clicked() {
                    let marked = entities_matching_target(cx.doc, current);
                    mark(cx.doc, cx.requested_actions, &marked);
                }
            }
            PropertyType::AngleNegativePitch => {
                let aim = !target.is_world();
                let w = if aim { (w - BUTTON_W - spacing).max(40.0) } else { w };
                match effective.and_then(parse_pitch) {
                    Some(mut p) => {
                        let h = ui.spacing().interact_size.y;
                        let resp = ui.add_sized(
                            vec2(w, h),
                            DragValue::new(&mut p)
                                .speed(1.0)
                                .range(-90.0..=90.0)
                                .clamp_existing_to_range(false)
                                .max_decimals(2)
                                .suffix("°")
                                .min_size(vec2(w.min(40.0), h)),
                        );
                        if resp.on_hover_text(t!("entities.pitch_negative_tip")).changed() {
                            push_set(ops, fmt_num_rounded(p, 4), true);
                        }
                    }
                    None => {
                        let live = self.buffers.live_text(id).unwrap_or(display);
                        let color = (!is_valid_float(live)).then_some(theme::error());
                        let opts = FieldOpts { hint, width: w, color, ..FieldOpts::default() };
                        let resp = self.buffers.field(ui, id, display, opts, || CommitAction::SetNumber {
                            target: target.clone(),
                            key: row.key.clone(),
                            integer: false,
                        });
                        if color.is_some() {
                            resp.on_hover_text(t!("entities.invalid_number"));
                        }
                    }
                }
                if aim {
                    let purpose = PickPurpose::Aim { negative_pitch: true };
                    self.pick_button(ui, cx, target, &row.key, purpose, t!("entities.pick_aim"));
                }
            }
            PropertyType::Angle => {
                let parsed = effective.and_then(parse_angles);
                let dial = ui.spacing().interact_size.y + 2.0;
                let room = (w - dial - 3.0 * spacing - (BUTTON_W + spacing)) / 3.0;
                let aim = !target.is_world() && room >= MIN_ANGLE_FIELD_W;
                let w = if aim { w - BUTTON_W - spacing } else { w };
                match parsed {
                    Some(mut a) => {
                        let dial = ui.spacing().interact_size.y + 2.0;
                        let dv_w = ((w - dial - 3.0 * spacing) / 3.0).max(16.0);
                        let mut changed = false;
                        let tips = [t!("entities.pitch"), t!("entities.yaw"), t!("entities.roll")];
                        for (i, tip) in tips.iter().enumerate() {
                            let resp = ui.add_sized(
                                vec2(dv_w, ui.spacing().interact_size.y),
                                DragValue::new(&mut a[i])
                                    .speed(1.0)
                                    .max_decimals(2)
                                    .min_size(vec2(dv_w, ui.spacing().interact_size.y)),
                            );
                            changed |= resp.on_hover_text(*tip).changed();
                        }
                        let (dial_resp, yaw) = yaw_dial(ui, Some(a.y), true);
                        dial_resp.on_hover_text(t!("entities.yaw_dial"));
                        if let Some(y) = yaw
                            && (y - a.y).abs() > 1e-9
                        {
                            a.y = y;
                            changed = true;
                        }
                        if changed {
                            push_set(ops, format_angles(a), true);
                        }
                    }
                    None => {
                        let dial = ui.spacing().interact_size.y + 2.0;
                        let opts = FieldOpts { hint, width: (w - dial - spacing).max(40.0), ..FieldOpts::default() };
                        self.buffers.field(ui, id, display, opts, set_key);
                        let (_, yaw) = yaw_dial(ui, None, true);
                        if let Some(y) = yaw {
                            push_set(ops, format_angles(glam::DVec3::new(0.0, y, 0.0)), true);
                        }
                    }
                }
                if aim {
                    let purpose = PickPurpose::Aim { negative_pitch: false };
                    self.pick_button(ui, cx, target, &row.key, purpose, t!("entities.pick_aim"));
                }
            }
            PropertyType::Axis => match effective.and_then(parse_axis) {
                Some(mut points) => {
                    let dv_w = ((w - 2.0 * spacing) / 3.0).max(16.0);
                    let mut changed = false;
                    ui.vertical(|ui| {
                        for (n, point) in points.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                let axes =
                                    [("X", theme::text_weak()), ("Y", theme::text_weak()), ("Z", theme::text_weak())];
                                for (i, (axis, color)) in axes.iter().enumerate() {
                                    let h = ui.spacing().interact_size.y;
                                    let label = format!("{axis}{} ", n + 1);
                                    let resp = ui.add_sized(
                                        vec2(dv_w, h),
                                        DragValue::new(&mut point[i])
                                            .min_size(vec2(dv_w, h))
                                            .speed(1.0)
                                            .max_decimals(3)
                                            .prefix(RichText::new(label).color(*color)),
                                    );
                                    changed |= resp.changed();
                                }
                            });
                        }
                    });
                    if changed {
                        push_set(ops, format_axis(points), true);
                    }
                }
                None => {
                    let opts = FieldOpts { hint, width: w, ..FieldOpts::default() };
                    self.buffers.field(ui, id, display, opts, set_key).on_hover_text(t!("entities.axis_tip"));
                }
            },
            PropertyType::Origin | PropertyType::Vector | PropertyType::VecLine => {
                match effective.and_then(parse_vec3) {
                    Some(mut v) => {
                        let dv_w = ((w - 2.0 * spacing) / 3.0).max(16.0);
                        let mut changed = false;
                        let axes = [("X", theme::text_weak()), ("Y", theme::text_weak()), ("Z", theme::text_weak())];
                        for (i, (axis, color)) in axes.iter().enumerate() {
                            let resp = ui.add_sized(
                                vec2(dv_w, ui.spacing().interact_size.y),
                                DragValue::new(&mut v[i])
                                    .min_size(vec2(dv_w, ui.spacing().interact_size.y))
                                    .speed(1.0)
                                    .max_decimals(3)
                                    .prefix(RichText::new(format!("{axis} ")).color(*color)),
                            );
                            changed |= resp.changed();
                        }
                        if changed {
                            push_set(ops, format_vec3(v), true);
                        }
                    }
                    None => {
                        self.buffers.field(
                            ui,
                            id,
                            display,
                            FieldOpts { hint, width: w, ..FieldOpts::default() },
                            set_key,
                        );
                    }
                }
            }
            PropertyType::NpcClass | PropertyType::PointEntityClass => {
                let text_w = (w - BUTTON_W - spacing).max(40.0);
                self.buffers.field(ui, id, display, FieldOpts { hint, width: text_w, ..FieldOpts::default() }, set_key);
                let btn = side_button(ui, icon("▾"), false);
                if picker_is_open(&btn)
                    && let Some(fgd) = cx.fgd
                {
                    let classes: Vec<&str> = if kind == PropertyType::NpcClass {
                        fgd.class_names_where(|c| c.kind == ClassKind::Npc)
                    } else {
                        fgd.point_classes().iter().map(String::as_str).collect()
                    };
                    let picked = picker_popup(&btn, true, classes.len(), |i| {
                        let name = classes.get(i).copied().unwrap_or("");
                        PickerItem { name, detail: fgd.class(name).map_or("", |c| c.description.as_str()) }
                    });
                    if let Some(c) = picked {
                        push_set(ops, c, false);
                    }
                }
            }
            PropertyType::InstanceFile => {
                let text_w = (w - BUTTON_W - spacing).max(40.0);
                self.buffers.field(ui, id, display, FieldOpts { hint, width: text_w, ..FieldOpts::default() }, set_key);
                let busy = self.browse.is_some();
                let btn = ui.add_enabled_ui(!busy, |ui| side_button(ui, BROWSE_GLYPH, false)).inner;
                if btn.on_hover_text(t!("entities.browse_instance")).clicked() {
                    let start = cx.doc.path().and_then(Path::parent).map(Path::to_path_buf);
                    self.browse = Some(spawn_vmf_browse(ui.ctx(), start, target.clone(), row.key.clone()));
                }
            }
            _ if target.is_world() && row.key.eq_ignore_ascii_case("skyname") => {
                let text_w = (w - BUTTON_W - spacing).max(40.0);
                let opts = FieldOpts { hint, width: text_w, ..FieldOpts::default() };
                let resp = self.buffers.field(ui, id, display, opts, set_key);
                let sky = effective.unwrap_or("").trim().to_ascii_lowercase();
                if !sky.is_empty() && !resp.has_focus() {
                    let hdr = self.sky_list(cx).iter().any(|s| *s == format!("{sky}_hdr"));
                    resp.on_hover_ui(|ui| sky_preview(ui, cx, &sky, hdr));
                }
                let btn = side_button(ui, icon("▾"), false).on_hover_text(t!("entities.pick_sky"));
                if picker_is_open(&btn) {
                    let skies = self.sky_list(cx);
                    let picked = picker_popup(&btn, true, skies.len(), |i| PickerItem {
                        name: skies.get(i).map_or("", String::as_str),
                        detail: "",
                    });
                    if let Some(s) = picked {
                        push_set(ops, s, false);
                    }
                }
            }
            _ => {
                self.buffers.field(ui, id, display, FieldOpts { hint, width: w, ..FieldOpts::default() }, set_key);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn target_field(
        &mut self,
        ui: &mut Ui,
        id: Id,
        display: &str,
        hint: &str,
        width: f32,
        names: &MapNames,
        list: &[String],
        opts: TargetOpts,
        action: impl FnOnce() -> CommitAction + Clone,
    ) {
        let keys = autocomplete_keys(ui, id, opts.mode);
        let live = self.buffers.live_text(id).unwrap_or(display).to_owned();
        let exists = !opts.validate || live.trim().is_empty() || names.target_exists(&live);
        let color = (!exists).then_some(theme::error());
        let spacing = ui.spacing().item_spacing.x;
        let text_w = if opts.dropdown { (width - BUTTON_W - spacing).max(40.0) } else { width };
        let field_opts = FieldOpts { hint, width: text_w, color, ..FieldOpts::default() };
        let resp = self.buffers.field(ui, id, display, field_opts, action.clone());
        let resp = if exists { resp } else { resp.on_hover_text(t!("entities.target_missing")) };
        if resp.has_focus() || resp.lost_focus() {
            let typed = self.buffers.live_text(id).unwrap_or(display).to_owned();
            let candidates: Vec<&str> = list.iter().map(String::as_str).collect();
            if let Some(pick) = autocomplete_popup(ui, &resp, &typed, &candidates, keys, opts.mode) {
                self.buffers.commit_now(id, pick, action.clone()());
                ui.memory_mut(|m| m.surrender_focus(id));
            }
        }
        if opts.dropdown {
            let btn = side_button(ui, icon("▾"), false);
            if picker_is_open(&btn) {
                let picked = picker_popup(&btn, false, list.len(), |i| PickerItem {
                    name: list.get(i).map_or("", String::as_str),
                    detail: "",
                });
                if let Some(p) = picked {
                    self.buffers.commit_now(id, p, action());
                }
            }
        }
    }

    fn sky_list(&mut self, cx: &Cx) -> Vec<String> {
        let materials = cx.assets.materials();
        let key = (std::sync::Arc::as_ptr(&materials) as usize, materials.len());
        match &self.skies {
            Some((p, n, list)) if (*p, *n) == key => list.clone(),
            _ => {
                let list = sky_names(&materials);
                self.skies = Some((key.0, key.1, list.clone()));
                list
            }
        }
    }

    pub fn flags(&mut self, ui: &mut Ui, view: &EntityView, ops: &mut Vec<Op>) {
        fit_scroll_content(ui);
        let Target::Entities(ids) = &view.target else {
            empty_note(ui, t!("entities.no_flags"));
            return;
        };
        if view.flags.is_empty() {
            empty_note(ui, t!("entities.no_flags"));
            return;
        }
        for group in &view.flags {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&group.label).strong());
                let id = self.field_id(&group.key);
                let display = group.agg.text();
                let hint = if group.agg.is_mixed() { t!("common.multiple_values") } else { "0" };
                let live = self.buffers.live_text(id).unwrap_or(display);
                let color = (!is_valid_integer(live)).then_some(theme::error());
                let opts = FieldOpts { hint, width: 90.0, color, ..FieldOpts::default() };
                self.buffers
                    .field(ui, id, display, opts, || CommitAction::SetNumber {
                        target: view.target.clone(),
                        key: group.key.clone(),
                        integer: true,
                    })
                    .on_hover_text(t!("entities.flags_value_tip"));
            });
            ui.separator();
            for bit in &group.bits {
                let mut checked = bit.state == Tri::On;
                let label =
                    if bit.label.is_empty() { tf!("entities.flag_unknown", bit.bit) } else { bit.label.clone() };
                let mut text = RichText::new(label);
                if bit.label.is_empty() {
                    text = text.italics();
                }
                let resp = ui.add(egui::Checkbox::new(&mut checked, text).indeterminate(bit.state == Tri::Mixed));
                let mut tip = tf!("entities.flag_bit", bit.bit);
                if bit.default_on {
                    tip.push_str(" · ");
                    tip.push_str(t!("entities.flag_default_on"));
                }
                if resp.on_hover_text(tip).clicked() {
                    ops.push(Op::SetFlag {
                        ids: ids.clone(),
                        key: group.key.clone(),
                        bit: bit.bit,
                        on: bit.state.toggled(),
                    });
                }
            }
            ui.add_space(6.0);
        }
    }

    pub fn raw(&mut self, ui: &mut Ui, view: &EntityView, ops: &mut Vec<Op>) {
        fit_scroll_content(ui);
        let full = ui.available_width();
        let spacing = ui.spacing().item_spacing.x;
        let key_w = (full * 0.38).clamp(70.0, 200.0);
        let value_w = (full - key_w - BUTTON_W - 2.0 * spacing).max(60.0);
        let grid = egui::Grid::new(self.salt.with("raw")).num_columns(3).striped(true).min_col_width(0.0);
        grid.spacing(vec2(spacing, 3.0)).show(ui, |ui| {
            ui.label(RichText::new(t!("entities.key")).strong());
            ui.label(RichText::new(t!("common.value")).strong());
            ui.label("");
            ui.end_row();
            left_label(ui, key_w, RichText::new("classname").weak());
            let class_text = match view.classes.as_slice() {
                [one] => one.clone(),
                _ => t!("common.multiple_values").to_owned(),
            };
            left_label(ui, value_w, RichText::new(class_text).weak());
            ui.label("");
            ui.end_row();
            for row in &view.raw {
                let key_id = self.salt.with(("raw_key", row.key.to_ascii_lowercase()));
                self.buffers.field(ui, key_id, &row.key, FieldOpts { width: key_w, ..FieldOpts::default() }, || {
                    CommitAction::RenameKey { target: view.target.clone(), old: row.key.clone() }
                });
                let value_id = self.salt.with(("raw_value", row.key.to_ascii_lowercase()));
                let hint = if row.agg.is_mixed() { t!("common.multiple_values") } else { "" };
                let opts = FieldOpts { hint, width: value_w, ..FieldOpts::default() };
                self.buffers.field(ui, value_id, row.agg.text(), opts, || CommitAction::SetKey {
                    target: view.target.clone(),
                    key: row.key.clone(),
                });
                let remove = side_button(ui, "×", false);
                if remove.on_hover_text(t!("entities.remove_key")).clicked() {
                    ops.push(Op::RemoveKey { target: view.target.clone(), key: row.key.clone() });
                }
                ui.end_row();
            }
            let key_resp = ui.add(
                egui::TextEdit::singleline(&mut self.new_key).hint_text(t!("entities.new_key")).desired_width(key_w),
            );
            let value_resp = ui.add(
                egui::TextEdit::singleline(&mut self.new_value)
                    .hint_text(t!("entities.new_value"))
                    .desired_width(value_w),
            );
            let enter =
                (key_resp.lost_focus() || value_resp.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let can_add = is_addable_key(&self.new_key);
            let add = ui.add_enabled_ui(can_add, |ui| side_button(ui, "+", false)).inner;
            if (add.on_hover_text(tf!("entities.add_key", key_label(egui::Key::Enter))).clicked() || enter) && can_add {
                ops.push(Op::SetKey {
                    target: view.target.clone(),
                    key: self.new_key.trim().to_owned(),
                    value: std::mem::take(&mut self.new_value),
                    merge: false,
                });
                self.new_key.clear();
                if enter {
                    key_resp.request_focus();
                }
            }
            ui.end_row();
        });
    }
}

#[derive(Clone, Copy)]
struct TargetOpts {
    validate: bool,
    dropdown: bool,
    mode: CompleteMode,
}

pub(crate) fn picker_is_open(button: &Response) -> bool {
    button.clicked() || Popup::is_id_open(&button.ctx, Popup::default_response_id(button))
}

fn is_valid_name(name: &str) -> bool {
    !name.trim().is_empty() && !name.contains('"')
}

pub(crate) fn show_browser_for(cx: &mut Cx, kind: AssetKind) {
    let ui_settings = &mut cx.settings.ui;
    match kind {
        AssetKind::Material | AssetKind::Decal | AssetKind::Sprite => {
            if !ui_settings.show_texture_browser {
                ui_settings.show_texture_browser = true;
                *cx.settings_dirty = true;
            }
            cx.ui.right_tab = RightTab::Textures;
            cx.ui.focus_texture_search = true;
        }
        AssetKind::Sound => {
            if !cx.settings.browser.show_sound_browser && !cx.runtime.entities.sound_window {
                cx.settings.browser.show_sound_browser = true;
                *cx.settings_dirty = true;
            }
            if !cx.runtime.entities.sound_window {
                cx.ui.right_tab = RightTab::Package(crate::sound_browser::TAB_ID);
            }
        }
        AssetKind::Particle => {
            if !cx.settings.browser.show_particle_browser && !cx.runtime.entities.particle_window {
                cx.settings.browser.show_particle_browser = true;
                *cx.settings_dirty = true;
            }
            if !cx.runtime.entities.particle_window {
                cx.ui.right_tab = RightTab::Package(crate::particles::TAB_ID);
            }
        }
    }
}

fn sky_preview(ui: &mut Ui, cx: &Cx, sky: &str, hdr: bool) {
    ui.label(RichText::new(sky).strong());
    egui::Grid::new("hp_ui_entities.sky_faces").spacing(vec2(2.0, 2.0)).show(ui, |ui| {
        for row in [["", "up", "", ""], ["lf", "ft", "rt", "bk"], ["", "dn", "", ""]] {
            for face in row {
                if face.is_empty() {
                    ui.allocate_exact_size(vec2(SKY_THUMB, SKY_THUMB), egui::Sense::hover());
                } else {
                    let name = format!("skybox/{sky}{face}");
                    thumbnail(ui, cx.thumbs.get(&name).copied(), SKY_THUMB).on_hover_text(face);
                }
            }
            ui.end_row();
        }
    });
    let (text, color) = if hdr {
        (t!("entities.sky_hdr"), theme::success())
    } else {
        (t!("entities.sky_ldr_only"), ui.visuals().weak_text_color())
    };
    ui.label(RichText::new(text).small().color(color));
}

const SKY_THUMB: f32 = 48.0;

pub(crate) const SKY_FACES: [&str; 6] = ["rt", "lf", "bk", "ft", "up", "dn"];

fn left_label(ui: &mut Ui, width: f32, text: RichText) {
    let h = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(vec2(width, h), Layout::left_to_right(Align::Center), |ui| {
        ui.set_width(width);
        ui.add_space(4.0);
        ui.add(Label::new(text).truncate());
    });
}

fn choice_label(c: &hp_fgd::Choice) -> &str {
    if c.display.trim().is_empty() { &c.value } else { &c.display }
}

fn is_studio_model_value(agg: &Agg) -> bool {
    matches!(agg, Agg::Value(v) if v.trim().to_ascii_lowercase().ends_with(".mdl"))
}

pub(crate) fn is_addable_key(key: &str) -> bool {
    let k = key.trim();
    !k.is_empty() && !k.eq_ignore_ascii_case("id") && !k.eq_ignore_ascii_case("classname") && !k.contains('"')
}

fn prop_tooltip(ui: &mut Ui, row: &PropRow, class: Option<&str>) {
    ui.set_max_width(340.0);
    match &row.prop {
        Some(p) => {
            ui.label(RichText::new(p.label()).strong());
            ui.label(RichText::new(format!("{} ({})", p.name, p.kind)).monospace().weak());
            if !p.description.trim().is_empty() {
                ui.label(p.description.trim());
            }
            if let Some(d) = p.default_value().filter(|d| !d.is_empty()) {
                ui.label(RichText::new(tf!("entities.default_value", d)).weak());
            }
            if let Some(note) = class.and_then(|c| crate::max_distance::key_note(c, &row.key)) {
                ui.label(RichText::new(note).italics());
            }
        }
        None => {
            ui.label(RichText::new(&row.key).strong().monospace());
            ui.label(RichText::new(t!("entities.unknown_key")).weak());
        }
    }
}

pub(crate) fn thumb_names(view: &EntityView) -> Vec<String> {
    let mut out = Vec::new();
    for row in &view.props {
        let is_material = row.prop.as_ref().is_some_and(|p: &Property| {
            matches!(p.kind, PropertyType::Material | PropertyType::Decal | PropertyType::Sprite)
        }) || (view.target.is_world() && row.key.eq_ignore_ascii_case("detailmaterial"));
        if !is_material {
            continue;
        }
        let value = match &row.agg {
            Agg::Value(v) => Some(v.as_str()),
            Agg::Absent => row.prop.as_ref().and_then(|p| p.default.as_deref()),
            Agg::Mixed => None,
        };
        if let Some(v) = value {
            let name = preview_material_name(v);
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    if view.target.is_world()
        && let Some(sky) = view.props.iter().find(|r| r.key.eq_ignore_ascii_case("skyname")).and_then(|r| r.agg.value())
    {
        let sky = sky.trim().to_ascii_lowercase();
        if !sky.is_empty() && !sky.contains(['/', '\\']) {
            out.extend(SKY_FACES.iter().map(|f| format!("skybox/{sky}{f}")));
        }
    }
    out
}

pub(crate) fn view_has_kind(view: &EntityView, kind: &PropertyType) -> bool {
    view.props.iter().any(|r| r.prop.as_ref().is_some_and(|p| p.kind == *kind))
}

pub(crate) fn instance_path(map_path: Option<&Path>, picked: &Path) -> String {
    let rel = map_path.and_then(Path::parent).and_then(|dir| picked.strip_prefix(dir).ok());
    rel.unwrap_or(picked).to_string_lossy().replace('\\', "/")
}

fn spawn_vmf_browse(ctx: &egui::Context, start: Option<PathBuf>, target: Target, key: String) -> FileBrowse {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    let title = t!("entities.browse_instance").to_owned();
    let filter = t!("dialog.vmf_filter").to_owned();
    let spawned = std::thread::Builder::new().name("hp-instance-browse".to_owned()).spawn(move || {
        let mut dialog = rfd::AsyncFileDialog::new().set_title(title).add_filter(filter, &["vmf"]);
        if let Some(dir) = start {
            dialog = dialog.set_directory(dir);
        }
        let picked = pollster::block_on(dialog.pick_file()).map(|h| h.path().to_path_buf());
        let _ = tx.send(picked);
        ctx.request_repaint();
    });
    if let Err(e) = spawned {
        log::warn!("could not start the file dialog thread: {e}");
    }
    FileBrowse { rx, target, key }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_paths_are_relative_to_the_map() {
        let map = Path::new("C:/maps/test.vmf");
        assert_eq!(instance_path(Some(map), Path::new("C:/maps/instances/door.vmf")), "instances/door.vmf");
        assert_eq!(instance_path(Some(map), Path::new("D:/other/door.vmf")), "D:/other/door.vmf");
        assert_eq!(instance_path(None, Path::new("D:/other/door.vmf")), "D:/other/door.vmf");
    }

    #[test]
    fn addable_keys() {
        assert!(is_addable_key(" targetname "));
        assert!(!is_addable_key(""));
        assert!(!is_addable_key("ID"));
        assert!(!is_addable_key("classname"));
        assert!(!is_addable_key("a\"b"));
    }

    #[test]
    fn thumbnails_for_material_keys() {
        let state = hp_app::testing::test_state();
        let fgd = crate::test_fgd::test_fgd();
        let mut doc = state.doc;
        let id = doc
            .create_point_entity(
                "env_sprite",
                glam::DVec3::ZERO,
                &[("model".to_owned(), "sprites/Glow01.spr".to_owned())],
            )
            .unwrap();
        let names = MapNames::build(&doc);
        let view = EntityView::build(&doc, Target::Entities(vec![id]), Some(&fgd), &names);
        assert_eq!(thumb_names(&view), ["sprites/glow01"]);
        let world = EntityView::build(&doc, Target::World, None, &names);
        let mut expected = vec!["detail/detailsprites".to_owned()];
        expected.extend(SKY_FACES.iter().map(|f| format!("skybox/sky_day01_01{f}")));
        assert_eq!(thumb_names(&world), expected, "the six faces of the sky");
    }
}
