use egui::{Color32, Ui};
use hp_app::{t, theme};
use hp_shader::catalog::{NodeDef, ParamDef, ParamKind, PinTy};
use hp_shader::{Graph, MAX_STOPS};
use serde_json::{Value, json};

use super::labels;

pub struct TextureAsk {
    pub node: u32,
    pub browse: bool,
    pub current: bool,
}

fn as_f32(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

fn color_of(v: &Value) -> [f32; 4] {
    let mut c = [0.0, 0.0, 0.0, 1.0];
    if let Some(a) = v.as_array() {
        for (i, x) in a.iter().take(4).enumerate() {
            c[i] = as_f32(x);
        }
    }
    c
}

fn gradient_bar(ui: &mut Ui, stops: &[(f32, [f32; 4])]) {
    let width = ui.available_width().max(60.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 18.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, theme::palette().bg0);
    let to_col = |c: [f32; 4]| {
        Color32::from_rgba_unmultiplied(
            (c[0].clamp(0.0, 1.0) * 255.0) as u8,
            (c[1].clamp(0.0, 1.0) * 255.0) as u8,
            (c[2].clamp(0.0, 1.0) * 255.0) as u8,
            255,
        )
    };
    let mut mesh = egui::Mesh::default();
    let mut points: Vec<(f32, [f32; 4])> = Vec::new();
    if let Some(first) = stops.first()
        && first.0 > 0.0
    {
        points.push((0.0, first.1));
    }
    points.extend_from_slice(stops);
    if let Some(last) = stops.last()
        && last.0 < 1.0
    {
        points.push((1.0, last.1));
    }
    for (pos, c) in &points {
        let x = rect.left() + pos.clamp(0.0, 1.0) * rect.width();
        let color = to_col(*c);
        let i = mesh.vertices.len() as u32;
        mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(x, rect.top()), uv: egui::epaint::WHITE_UV, color });
        mesh.vertices.push(egui::epaint::Vertex {
            pos: egui::pos2(x, rect.bottom()),
            uv: egui::epaint::WHITE_UV,
            color,
        });
        if i >= 2 {
            mesh.indices.extend_from_slice(&[i - 2, i - 1, i, i - 1, i + 1, i]);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, theme::palette().border_strong), egui::StrokeKind::Inside);
    for (pos, _) in stops {
        let x = rect.left() + pos.clamp(0.0, 1.0) * rect.width();
        painter.vline(x, rect.y_range(), egui::Stroke::new(1.0, Color32::WHITE));
    }
}

fn gradient_ui(ui: &mut Ui, value: &Value) -> Option<Value> {
    let mut stops: Vec<(f32, [f32; 4])> = value
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| (s.get("pos").map_or(0.0, as_f32), s.get("color").map_or([0.0, 0.0, 0.0, 1.0], color_of)))
                .collect()
        })
        .unwrap_or_default();
    let mut changed = false;
    let mut remove: Option<usize> = None;
    ui.vertical(|ui| {
        gradient_bar(ui, &stops);
        let count = stops.len();
        for (i, (pos, color)) in stops.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                changed |= ui.add(egui::DragValue::new(pos).speed(0.005).range(0.0..=1.0).fixed_decimals(3)).changed();
                changed |= ui.color_edit_button_rgba_unmultiplied(color).changed();
                if ui
                    .add_enabled(count > 2, egui::Button::new("\u{2716}").small())
                    .on_hover_text(t!("shader.stop.remove"))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if ui.add_enabled(count < MAX_STOPS, egui::Button::new(t!("shader.stop.add"))).clicked() {
            let mut sorted = stops.clone();
            sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (mut at, mut gap) = (0, -1.0);
            for (i, w) in sorted.windows(2).enumerate() {
                if w[1].0 - w[0].0 > gap {
                    gap = w[1].0 - w[0].0;
                    at = i;
                }
            }
            if sorted.len() >= 2 {
                let (a, b) = (sorted[at], sorted[at + 1]);
                let mut c = [0.0; 4];
                for (k, slot) in c.iter_mut().enumerate() {
                    *slot = (a.1[k] + b.1[k]) * 0.5;
                }
                stops.push(((a.0 + b.0) * 0.5, c));
                changed = true;
            }
        }
    });
    if let Some(i) = remove {
        stops.remove(i);
        changed = true;
    }
    changed.then(|| {
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        Value::Array(stops.into_iter().map(|(pos, color)| json!({ "pos": pos, "color": color })).collect())
    })
}

fn param_widget(
    ui: &mut Ui,
    def: &ParamDef,
    value: &Value,
    ask: &mut Option<TextureAsk>,
    node: u32,
    is_tex: bool,
) -> Option<Value> {
    match def.kind {
        ParamKind::Float { min, max, .. } => {
            let mut v = as_f32(value);
            let r = if max - min <= 200.0 {
                ui.add(egui::Slider::new(&mut v, min..=max).clamping(egui::SliderClamping::Always))
            } else {
                ui.add(egui::DragValue::new(&mut v).speed(0.05).range(min..=max))
            };
            r.changed().then(|| json!(v))
        }
        ParamKind::Int { min, max, .. } => {
            let mut v = value.as_i64().unwrap_or(0) as i32;
            let r = if max - min <= 64 || max <= 256 {
                ui.add(egui::Slider::new(&mut v, min..=max).clamping(egui::SliderClamping::Always))
            } else {
                ui.add(egui::DragValue::new(&mut v).range(min..=max))
            };
            r.changed().then(|| json!(v))
        }
        ParamKind::Bool { .. } => {
            let mut v = value.as_bool().unwrap_or(false);
            ui.checkbox(&mut v, "").changed().then(|| json!(v))
        }
        ParamKind::Color { .. } => {
            let mut c = color_of(value);
            ui.color_edit_button_rgba_unmultiplied(&mut c).changed().then(|| json!(c))
        }
        ParamKind::Choice { options, .. } => {
            let cur = value.as_str().unwrap_or("").to_owned();
            let mut out = None;
            egui::ComboBox::from_id_salt(("hp_shader_choice", node, def.name))
                .selected_text(labels::option(&cur))
                .width(ui.available_width().min(190.0))
                .show_ui(ui, |ui| {
                    for o in options {
                        if ui.selectable_label(cur == *o, labels::option(o)).clicked() {
                            out = Some(json!(o));
                        }
                    }
                });
            out
        }
        ParamKind::Text { .. } => {
            let mut s = value.as_str().unwrap_or("").to_owned();
            let mut out = None;
            ui.vertical(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut s).desired_width(ui.available_width()));
                if r.changed() {
                    out = Some(json!(s));
                }
                if is_tex {
                    ui.horizontal(|ui| {
                        if ui.button(t!("shader.browse")).clicked() {
                            *ask = Some(TextureAsk { node, browse: true, current: false });
                        }
                        if ui.button(t!("shader.use_current")).on_hover_text(t!("shader.use_current_tip")).clicked() {
                            *ask = Some(TextureAsk { node, browse: false, current: true });
                        }
                    });
                }
            });
            out
        }
        ParamKind::Vec2 { min, max, .. } => {
            let mut v = match value.as_array() {
                Some(a) if a.len() == 2 => [as_f32(&a[0]), as_f32(&a[1])],
                _ => [0.0, 0.0],
            };
            let mut changed = false;
            ui.horizontal(|ui| {
                changed |= ui.add(egui::DragValue::new(&mut v[0]).speed(0.01).range(min..=max).prefix("x ")).changed();
                changed |= ui.add(egui::DragValue::new(&mut v[1]).speed(0.01).range(min..=max).prefix("y ")).changed();
            });
            changed.then(|| json!(v))
        }
        ParamKind::Gradient => gradient_ui(ui, value),
    }
}

pub fn node_header(ui: &mut Ui, def: &NodeDef) {
    ui.label(egui::RichText::new(labels::node_title(def.id)).strong().size(15.0));
    ui.label(egui::RichText::new(labels::node_tip(def.id)).small().color(theme::palette().text_weak));
}

pub fn node_inspector(ui: &mut Ui, graph: &mut Graph, id: u32) -> Option<TextureAsk> {
    let def: &'static NodeDef = graph.def_of(id)?;
    let mut ask: Option<TextureAsk> = None;
    let mut sets: Vec<(&'static str, Value)> = Vec::new();
    if !def.params.is_empty() {
        egui::Grid::new(("hp_shader_params", id)).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            for p in def.params {
                let Some(value) = graph.param_value(id, p.name) else { continue };
                ui.label(labels::param(p.name));
                if let Some(v) = param_widget(ui, p, &value, &mut ask, id, def.id == "texture" && p.name == "source") {
                    sets.push((p.name, v));
                }
                ui.end_row();
            }
        });
    }
    for (name, v) in sets {
        let _ = graph.set_param(id, name, v);
    }
    let mut defaults: Vec<(&'static str, Vec<f32>)> = Vec::new();
    let mut resets: Vec<&'static str> = Vec::new();
    let editable: Vec<_> =
        def.inputs.iter().filter(|p| p.ty != PinTy::Uv && graph.link_to(id, p.name).is_none()).collect();
    if !editable.is_empty() {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(t!("shader.input_defaults")).small().color(theme::palette().text_weak));
        egui::Grid::new(("hp_shader_inputs", id)).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            for pin in editable {
                let cur = graph.input_default(id, pin.name).unwrap_or(pin.default);
                ui.label(labels::pin(pin.name));
                let overridden = graph.node(id).is_some_and(|n| n.inputs.contains_key(pin.name));
                ui.horizontal(|ui| {
                    match pin.ty {
                        PinTy::Color => {
                            let mut c = cur;
                            if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
                                defaults.push((pin.name, c.to_vec()));
                            }
                        }
                        _ => {
                            let mut v = cur[0];
                            if ui.add(egui::DragValue::new(&mut v).speed(0.01)).changed() {
                                defaults.push((pin.name, vec![v]));
                            }
                        }
                    }
                    if overridden && ui.small_button(t!("shader.reset")).clicked() {
                        resets.push(pin.name);
                    }
                });
                ui.end_row();
            }
        });
    }
    for (name, v) in defaults {
        let _ = graph.set_input_default(id, name, &v);
    }
    for name in resets {
        graph.clear_input_default(id, name);
    }
    ask
}
