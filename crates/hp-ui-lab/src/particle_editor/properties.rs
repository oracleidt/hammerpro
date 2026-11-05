use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, vec2};
use hp_app::{t, tf};
use hp_pcf::dmx::{Dmx, Reference, Value};

pub(super) fn source_curves_ui(ui: &mut Ui, element: &hp_pcf::dmx::Element, color: [u8; 4]) -> Vec<(String, Value)> {
    let function = element.str("functionName").unwrap_or("").to_ascii_lowercase();
    if function == "color fade" {
        ui.label(egui::RichText::new(t!("pe.color_gradient")).strong());
        gradient(ui, &[color, element.color("color_fade").unwrap_or([255; 4])]);
        return Vec::new();
    }
    let fields = match function.as_str() {
        "radius scale" => {
            vec![("start_time", Some("radius_start_scale"), 0.0, 1.0), ("end_time", Some("radius_end_scale"), 1.0, 1.0)]
        }
        "alpha fade and decay" | "alpha fade and decay for tracers" => vec![
            ("start_fade_in_time", Some("start_alpha"), 0.0, 1.0),
            ("end_fade_in_time", None, 0.5, 1.0),
            ("start_fade_out_time", None, 0.5, 1.0),
            ("end_fade_out_time", Some("end_alpha"), 1.0, 0.0),
        ],
        _ => return Vec::new(),
    };
    ui.label(egui::RichText::new(t!("pe.life_curve")).strong());
    let mut points: Vec<[f32; 2]> = fields
        .iter()
        .map(|(time, value, t, v)| {
            [element.float(time).unwrap_or(*t), value.and_then(|key| element.float(key)).unwrap_or(*v)]
        })
        .collect();
    let max_time = points.iter().map(|p| p[0]).fold(1.0f32, f32::max).max(0.01);
    let max_value = points.iter().map(|p| p[1]).fold(1.0f32, f32::max).max(0.01);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width().max(120.0), 125.0), Sense::drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    for i in 1..4 {
        let x = rect.left() + rect.width() * i as f32 * 0.25;
        let y = rect.top() + rect.height() * i as f32 * 0.25;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, Color32::from_gray(50)),
        );
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0, Color32::from_gray(50)),
        );
    }
    let projected: Vec<_> = points
        .iter()
        .map(|p| {
            Pos2::new(rect.left() + p[0] / max_time * rect.width(), rect.bottom() - p[1] / max_value * rect.height())
        })
        .collect();
    for pair in projected.windows(2) {
        painter.line_segment([pair[0], pair[1]], Stroke::new(1.5, Color32::from_gray(190)));
    }
    for point in &projected {
        painter.circle_filled(*point, 4.0, Color32::from_gray(220));
    }
    ui.label(egui::RichText::new(t!("pe.curve_time_hint")).small().weak());
    let id = ui.scope_id().with("source_curve_point");
    if response.drag_started()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let index = projected
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.distance_sq(pos).total_cmp(&b.distance_sq(pos)))
            .map_or(0, |(i, _)| i);
        ui.data_mut(|data| data.insert_temp(id, index));
    }
    if response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let index = ui.data_mut(|data| data.get_temp::<usize>(id)).unwrap_or(0).min(points.len() - 1);
        let lower = if index == 0 { 0.0 } else { points[index - 1][0] };
        let upper = if index + 1 == points.len() { max_time } else { points[index + 1][0] };
        points[index][0] =
            (((pos.x - rect.left()) / rect.width()) * max_time).clamp(lower.min(upper), upper.max(lower));
        let mut edits = vec![(fields[index].0.to_owned(), Value::Float(points[index][0]))];
        if let Some(key) = fields[index].1 {
            let max = if function == "radius scale" { max_value } else { 1.0 };
            points[index][1] = ((rect.bottom() - pos.y) / rect.height() * max_value).clamp(0.0, max);
            edits.push((key.to_owned(), Value::Float(points[index][1])));
        }
        return edits;
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<usize>(id));
    }
    Vec::new()
}

pub(super) fn named_value_ui(ui: &mut Ui, name: &str, value: &mut Value, dmx: &Dmx) -> bool {
    let name = name.to_ascii_lowercase();
    match value {
        Value::Int(v) if name == "max_particles" => {
            ui.add(egui::DragValue::new(v).range(1..=100_000).clamp_existing_to_range(false)).changed()
        }
        Value::Int(v) if name == "initial_particles" || name == "num_to_emit" || name == "num_to_emit_minimum" => {
            ui.add(egui::DragValue::new(v).range(0..=100_000).clamp_existing_to_range(false)).changed()
        }
        Value::Int(v) if name.contains("control point") || name.contains("control_point") => {
            ui.add(egui::DragValue::new(v).range(0..=63).clamp_existing_to_range(false)).changed()
        }
        Value::Float(v) if name == "alpha_min" || name == "alpha_max" => {
            ui.add(egui::DragValue::new(v).speed(1.0).range(0.0..=255.0).clamp_existing_to_range(false)).changed()
        }
        Value::Float(v) if name.starts_with("lifetime_") || name.contains("duration") || name == "delay" => {
            ui.add(egui::DragValue::new(v).speed(0.05).range(0.0..=3600.0).clamp_existing_to_range(false)).changed()
        }
        Value::Float(v) if name == "radius" || name == "radius_min" || name == "radius_max" => {
            ui.add(egui::DragValue::new(v).speed(0.1).range(0.0..=100_000.0).clamp_existing_to_range(false)).changed()
        }
        _ => value_ui(ui, value, dmx),
    }
}

pub(super) fn value_ui(ui: &mut Ui, value: &mut Value, dmx: &Dmx) -> bool {
    match value {
        Value::Int(v) => ui.add(egui::DragValue::new(v).speed(1)).changed(),
        Value::Float(v) => finite_float(ui, v),
        Value::Bool(v) => ui.checkbox(v, "").changed(),
        Value::Str(v) | Value::External(v) => {
            ui.add(egui::TextEdit::singleline(v).desired_width(f32::INFINITY)).changed()
        }
        Value::Color(v) => ui.color_edit_button_srgba_unmultiplied(v).changed(),
        Value::Vec2(v) => vector(ui, v),
        Value::Vec3(v) => vector(ui, v),
        Value::Vec4(v) => vector(ui, v),
        Value::Element(v) => reference_ui(ui, v, dmx),
        Value::Elements(values) => array_ui(ui, values, None, |ui, value| reference_ui(ui, value, dmx)),
        Value::References(values) => array_ui(ui, values, Reference::Element(None), |ui, value| match value {
            Reference::Element(v) => reference_ui(ui, v, dmx),
            Reference::External(v) => ui.text_edit_singleline(v).changed(),
        }),
        Value::Ints(values) => array_ui(ui, values, 0, |ui, v| ui.add(egui::DragValue::new(v)).changed()),
        Value::Floats(values) => {
            let mut changed = array_ui(ui, values, 0.0, finite_float);
            if values.len() >= 2 {
                egui::CollapsingHeader::new(t!("pe.curve")).show(ui, |ui| {
                    changed |= curve_ui(ui, values);
                });
            }
            changed
        }
        Value::Vec3s(values) => array_ui(ui, values, [0.0; 3], vector),
        Value::Strs(values) => array_ui(ui, values, String::new(), |ui, v| ui.text_edit_singleline(v).changed()),
        Value::Raw { kind, data } => raw_ui(ui, *kind, data),
        Value::Other => {
            ui.label(t!("pe.empty_payload"));
            false
        }
    }
}

fn finite_float(ui: &mut Ui, value: &mut f32) -> bool {
    let previous = *value;
    let changed = ui
        .add(egui::DragValue::new(value).speed(0.05).range(-1.0e20..=1.0e20).clamp_existing_to_range(false))
        .changed();
    if !value.is_finite() {
        *value = previous;
        return false;
    }
    changed
}

fn vector<const N: usize>(ui: &mut Ui, value: &mut [f32; N]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for v in value {
            changed |= finite_float(ui, v);
        }
    });
    changed
}

fn reference_ui(ui: &mut Ui, value: &mut Option<usize>, dmx: &Dmx) -> bool {
    let old = *value;
    let selected = value.and_then(|i| dmx.elements.get(i)).map_or_else(|| t!("pe.none").to_owned(), |e| e.name.clone());
    egui::ComboBox::from_id_salt(ui.scope_id().with("reference")).selected_text(selected).show_ui(ui, |ui| {
        ui.selectable_value(value, None, t!("pe.none"));
        for (i, element) in dmx.elements.iter().enumerate() {
            ui.selectable_value(value, Some(i), format!("{i}: {} ({})", element.name, element.kind));
        }
    });
    *value != old
}

fn array_ui<T: Clone>(
    ui: &mut Ui,
    values: &mut Vec<T>,
    default: T,
    mut entry: impl FnMut(&mut Ui, &mut T) -> bool,
) -> bool {
    let mut changed = false;
    let mut remove = None;
    egui::ScrollArea::vertical().id_salt(ui.scope_id().with("values")).max_height(170.0).show_rows(
        ui,
        23.0,
        values.len(),
        |ui, range| {
            for i in range {
                ui.push_id(i, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(i.to_string());
                        changed |= entry(ui, &mut values[i]);
                        if ui.small_button(t!("pe.remove")).clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
        },
    );
    if let Some(i) = remove {
        values.remove(i);
        changed = true;
    }
    if ui.small_button(t!("pe.add_value")).clicked() {
        values.push(default);
        changed = true;
    }
    changed
}

fn curve_ui(ui: &mut Ui, values: &mut [f32]) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width().max(120.0), 120.0), Sense::drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    let min = values.iter().copied().fold(0.0f32, f32::min);
    let max = values.iter().copied().fold(1.0f32, f32::max).max(min + 1.0);
    let points: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            Pos2::new(
                rect.left() + i as f32 / (values.len() - 1) as f32 * rect.width(),
                rect.bottom() - (v - min) / (max - min) * rect.height(),
            )
        })
        .collect();
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(1.5, Color32::from_gray(190)));
    }
    for point in &points {
        painter.circle_filled(*point, 3.0, Color32::from_gray(220));
    }
    if response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let i = (((pos.x - rect.left()) / rect.width()) * (values.len() - 1) as f32)
            .round()
            .clamp(0.0, (values.len() - 1) as f32) as usize;
        values[i] =
            (max - (pos.y - rect.top()).clamp(0.0, rect.height()) / rect.height() * (max - min)).clamp(min, max);
        return true;
    }
    false
}

fn raw_ui(ui: &mut Ui, kind: u8, data: &mut Vec<u8>) -> bool {
    if matches!(kind, 12..=14)
        && data.len()
            == match kind {
                12 => 12,
                13 => 16,
                _ => 64,
            }
    {
        let mut values: Vec<f32> = data.chunks_exact(4).map(|v| f32::from_le_bytes(v.try_into().unwrap())).collect();
        let mut changed = false;
        for row in values.chunks_mut(4) {
            ui.horizontal(|ui| {
                for v in row {
                    changed |= finite_float(ui, v);
                }
            });
        }
        if changed {
            *data = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        }
        return changed;
    }
    if kind == 7 && data.len() == 4 {
        let mut time = i32::from_le_bytes(data.as_slice().try_into().unwrap());
        if ui.add(egui::DragValue::new(&mut time)).changed() {
            *data = time.to_le_bytes().to_vec();
            return true;
        }
        return false;
    }
    if kind == 22 && data.len() >= 4 {
        let count = i32::from_le_bytes(data[..4].try_into().unwrap());
        if count >= 0 && count as usize == (data.len() - 4) / 4 && (data.len() - 4).is_multiple_of(4) {
            let mut colors: Vec<[u8; 4]> = data[4..].chunks_exact(4).map(|v| v.try_into().unwrap()).collect();
            gradient(ui, &colors);
            if array_ui(ui, &mut colors, [255; 4], |ui, v| ui.color_edit_button_srgba_unmultiplied(v).changed()) {
                *data = (colors.len() as i32).to_le_bytes().into_iter().chain(colors.into_iter().flatten()).collect();
                return true;
            }
            return false;
        }
    }
    ui.label(tf!("pe.raw_size", kind, data.len()));
    let id = ui.scope_id().with("raw_hex");
    let mut text =
        ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| data.iter().map(|v| format!("{v:02x}")).collect());
    let response = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(2).desired_width(f32::INFINITY));
    let mut changed = false;
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let parsed = if compact.len().is_multiple_of(2) && compact.is_ascii() {
        (0..compact.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&compact[i..i + 2], 16))
            .collect::<Result<Vec<_>, _>>()
            .ok()
    } else {
        None
    };
    if response.changed()
        && let Some(parsed) = &parsed
    {
        data.clone_from(parsed);
        changed = true;
    }
    if parsed.is_none() {
        ui.label(egui::RichText::new(t!("pe.hex_invalid")).color(hp_app::theme::warning()));
    }
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    ui.label(egui::RichText::new(t!("pe.raw_hint")).weak().small());
    changed
}

fn gradient(ui: &mut Ui, values: &[[u8; 4]]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().max(100.0), 22.0), Sense::hover());
    if values.is_empty() {
        return;
    }
    for i in 0..128 {
        let t = i as f32 / 127.0 * values.len().saturating_sub(1) as f32;
        let a = values[t.floor() as usize];
        let b = values[(t.ceil() as usize).min(values.len() - 1)];
        let c: [u8; 4] = std::array::from_fn(|k| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * t.fract()) as u8);
        let cell = Rect::from_min_max(
            Pos2::new(rect.left() + rect.width() * i as f32 / 128.0, rect.top()),
            Pos2::new(rect.left() + rect.width() * (i + 1) as f32 / 128.0, rect.bottom()),
        );
        ui.painter().rect_filled(cell, 0.0, hp_app::data_colors::particle_gradient(c));
    }
}
