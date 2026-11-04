use egui::{Pos2, Rect, Shape, Stroke, Ui};
use hp_app::t;
use hp_app::tool::ToolOption;
use hp_geom::{MAX_DOOR_ARCH_SEGMENTS, MAX_STAIR_ANGLE, MAX_STAIR_STEPS, Primitive, PrimitiveParams, RiseDirection};

use crate::block::{BlockOptions, param};

pub(crate) fn is_structure(kind: Primitive) -> bool {
    matches!(
        kind,
        Primitive::Stairs | Primitive::SpiralStairs | Primitive::Ramp | Primitive::Pillar | Primitive::Doorway
    )
}

pub(crate) fn apply(o: &BlockOptions, p: &mut PrimitiveParams) {
    p.stair_steps = o.stair_steps;
    p.stair_step_height = o.stair_step_height;
    p.stair_open = o.stair_open;
    p.spiral_inner = o.spiral_inner;
    p.spiral_angle = o.spiral_angle;
    p.spiral_start = o.spiral_start;
    p.ramp_angle = o.ramp_angle;
    p.pillar_base = o.pillar_base;
    p.pillar_capital = o.pillar_capital;
    p.pillar_inset = o.pillar_inset;
    p.door_width = o.door_width;
    p.door_height = o.door_height;
    p.door_thickness = o.door_thickness;
    p.door_arch = o.door_arch;
    p.door_arch_radius = o.door_arch_radius;
    p.door_arch_segments = o.door_arch_segments;
    p.direction = o.direction;
}

fn units(v: &mut f64, max: f64) -> egui::DragValue<'_> {
    egui::DragValue::new(v).range(0.0..=max).speed(1.0)
}

fn flag(ui: &mut Ui, grid: bool, label: &str, tip: &str, value: &mut bool) {
    if grid {
        ui.label("");
    }
    ui.checkbox(value, t!(label)).on_hover_text(t!(tip));
    if grid {
        ui.end_row();
    }
}

fn direction_key(d: RiseDirection) -> &'static str {
    match d {
        RiseDirection::PlusY => "create.struct.dir_plus_y",
        RiseDirection::MinusY => "create.struct.dir_minus_y",
        RiseDirection::PlusX => "create.struct.dir_plus_x",
        RiseDirection::MinusX => "create.struct.dir_minus_x",
        RiseDirection::Longest => "create.struct.dir_longest",
    }
}

fn direction_ui(ui: &mut Ui, o: &mut BlockOptions, grid: bool) {
    ui.label(t!("create.struct.direction")).on_hover_text(t!("create.struct.direction_tip"));
    egui::ComboBox::from_id_salt("hp_block_direction").selected_text(t!(direction_key(o.direction))).show_ui(
        ui,
        |ui| {
            for d in RiseDirection::ALL {
                ui.selectable_value(&mut o.direction, d, t!(direction_key(d)));
            }
        },
    );
    if grid {
        ui.end_row();
    }
}

pub(crate) fn params_ui(ui: &mut Ui, o: &mut BlockOptions, grid: bool) {
    let row = |ui: &mut Ui, label: &str, tip: &str, value: egui::DragValue<'_>| {
        param(ui, label, tip, value);
        if grid {
            ui.end_row();
        }
    };
    match o.kind {
        Primitive::Stairs | Primitive::SpiralStairs => {
            let steps = egui::DragValue::new(&mut o.stair_steps).range(1..=MAX_STAIR_STEPS).speed(0.1);
            row(ui, "create.struct.steps", "create.struct.steps_tip", steps);
            row(
                ui,
                "create.struct.step_height",
                "create.struct.step_height_tip",
                units(&mut o.stair_step_height, 1024.0),
            );
            if o.kind == Primitive::SpiralStairs {
                row(ui, "create.struct.inner", "create.struct.inner_tip", units(&mut o.spiral_inner, 16384.0));
                let turn = egui::DragValue::new(&mut o.spiral_angle)
                    .range(-MAX_STAIR_ANGLE..=MAX_STAIR_ANGLE)
                    .speed(1.0)
                    .suffix("°");
                row(ui, "create.struct.turn", "create.struct.turn_tip", turn);
                let start = egui::DragValue::new(&mut o.spiral_start).range(-360.0..=360.0).speed(1.0).suffix("°");
                row(ui, "create.block.start_angle", "create.block.start_angle_tip", start);
            }
            direction_ui(ui, o, grid);
            flag(ui, grid, "create.struct.open", "create.struct.open_tip", &mut o.stair_open);
            if !o.stair_open && o.kind == Primitive::SpiralStairs {
                o.spiral_angle = o.spiral_angle.clamp(-360.0, 360.0);
            }
        }
        Primitive::Ramp => {
            let slope = egui::DragValue::new(&mut o.ramp_angle).range(1.0..=89.0).speed(0.5).suffix("°");
            row(ui, "create.struct.slope", "create.struct.slope_tip", slope);
            direction_ui(ui, o, grid);
        }
        Primitive::Pillar => {
            row(ui, "create.struct.base", "create.struct.base_tip", units(&mut o.pillar_base, 16384.0));
            row(ui, "create.struct.capital", "create.struct.capital_tip", units(&mut o.pillar_capital, 16384.0));
            row(ui, "create.struct.inset", "create.struct.inset_tip", units(&mut o.pillar_inset, 16384.0));
        }
        Primitive::Doorway => {
            row(ui, "create.struct.door_width", "create.struct.door_width_tip", units(&mut o.door_width, 65536.0));
            row(ui, "create.struct.door_height", "create.struct.door_height_tip", units(&mut o.door_height, 65536.0));
            row(ui, "create.struct.thickness", "create.struct.thickness_tip", units(&mut o.door_thickness, 65536.0));
            flag(ui, grid, "create.struct.arch", "create.struct.arch_tip", &mut o.door_arch);
            if o.door_arch {
                let radius = units(&mut o.door_arch_radius, 65536.0);
                row(ui, "create.struct.arch_radius", "create.struct.arch_radius_tip", radius);
                let segs = egui::DragValue::new(&mut o.door_arch_segments).range(2..=MAX_DOOR_ARCH_SEGMENTS).speed(0.1);
                row(ui, "create.struct.arch_segments", "create.struct.arch_segments_tip", segs);
            }
        }
        _ => {}
    }
}

pub(crate) fn set_option(o: &mut BlockOptions, key: &str, value: &str) -> Option<bool> {
    let num = value.parse::<f64>().ok().filter(|v| v.is_finite());
    let on = match value.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" => Some(true),
        "off" | "false" | "0" => Some(false),
        _ => None,
    };
    let set = |slot: &mut f64, v: Option<f64>| v.map(|v| *slot = v).is_some();
    Some(match key {
        "steps" => {
            value.parse::<u32>().ok().filter(|v| (1..=MAX_STAIR_STEPS).contains(v)).map(|v| o.stair_steps = v).is_some()
        }
        "step_height" => set(&mut o.stair_step_height, num.filter(|v| (0.0..=1024.0).contains(v))),
        "open" => on.map(|v| o.stair_open = v).is_some(),
        "inner" => set(&mut o.spiral_inner, num.filter(|v| *v >= 0.0)),
        "spiral_angle" => set(&mut o.spiral_angle, num.filter(|v| v.abs() <= MAX_STAIR_ANGLE && *v != 0.0)),
        "spiral_start" => set(&mut o.spiral_start, num.filter(|v| v.abs() <= 360.0)),
        "slope" => set(&mut o.ramp_angle, num.filter(|v| (1.0..=89.0).contains(v))),
        "base" => set(&mut o.pillar_base, num.filter(|v| *v >= 0.0)),
        "capital" => set(&mut o.pillar_capital, num.filter(|v| *v >= 0.0)),
        "inset" => set(&mut o.pillar_inset, num.filter(|v| *v >= 0.0)),
        "door_width" => set(&mut o.door_width, num.filter(|v| *v >= 0.0)),
        "door_height" => set(&mut o.door_height, num.filter(|v| *v >= 0.0)),
        "thickness" => set(&mut o.door_thickness, num.filter(|v| *v >= 0.0)),
        "door_arch" => on.map(|v| o.door_arch = v).is_some(),
        "door_arch_radius" => set(&mut o.door_arch_radius, num.filter(|v| *v >= 0.0)),
        "door_arch_segments" => value
            .parse::<u32>()
            .ok()
            .filter(|v| (2..=MAX_DOOR_ARCH_SEGMENTS).contains(v))
            .map(|v| o.door_arch_segments = v)
            .is_some(),
        "direction" => RiseDirection::from_id(value).map(|d| o.direction = d).is_some(),
        _ => return None,
    })
}

pub(crate) fn option_values(o: &BlockOptions, num: &dyn Fn(f64) -> String) -> Vec<ToolOption> {
    let on = |b: bool| if b { "on" } else { "off" };
    vec![
        ToolOption::new("steps", o.stair_steps.to_string(), "stairs and spiral stairs: number of steps (1 to 256)"),
        ToolOption::new(
            "step_height",
            num(o.stair_step_height),
            "stairs and spiral stairs: height of a step in units, 0 uses steps instead",
        ),
        ToolOption::new(
            "open",
            on(o.stair_open),
            "on: open steps (slabs two steps thick, spiral stairs up to 3600 degrees), off: solid steps down to the base",
        ),
        ToolOption::new("inner", num(o.spiral_inner), "spiral stairs: inner radius in units (0 for pie steps)"),
        ToolOption::new(
            "spiral_angle",
            num(o.spiral_angle),
            "spiral stairs: total turn in degrees, negative turns the other way (over 360 only with open)",
        ),
        ToolOption::new("spiral_start", num(o.spiral_start), "spiral stairs: start angle in degrees"),
        ToolOption::new("slope", num(o.ramp_angle), "ramp: slope angle in degrees (1 to 89)"),
        ToolOption::new("base", num(o.pillar_base), "pillar: height of the base in units (0 for none)"),
        ToolOption::new("capital", num(o.pillar_capital), "pillar: height of the capital in units (0 for none)"),
        ToolOption::new(
            "inset",
            num(o.pillar_inset),
            "pillar: how much the shaft is narrower than the base and capital",
        ),
        ToolOption::new("door_width", num(o.door_width), "doorway: opening width in units (0 is half the wall)"),
        ToolOption::new("door_height", num(o.door_height), "doorway: opening height in units (0 is three quarters)"),
        ToolOption::new("thickness", num(o.door_thickness), "doorway: wall thickness in units (0 fills the box depth)"),
        ToolOption::new("door_arch", on(o.door_arch), "doorway: on gives the opening a round top"),
        ToolOption::new(
            "door_arch_radius",
            num(o.door_arch_radius),
            "doorway arch: radius in units, at least half the opening, 0 is exactly half (a semicircle)",
        ),
        ToolOption::new(
            "door_arch_segments",
            o.door_arch_segments.to_string(),
            "doorway arch: number of segments of the round top (2 to 64)",
        ),
        ToolOption::new(
            "direction",
            o.direction.id(),
            "stairs, spiral stairs and ramp: rise toward +y (default), -y, +x, -x or longest (along the longest side of the box)",
        ),
    ]
}

pub(crate) fn paint_icon(painter: &egui::Painter, r: Rect, kind: Primitive, color: egui::Color32) -> bool {
    let stroke = Stroke::new(1.3, color);
    let p = |x: f32, y: f32| Pos2::new(r.left() + x * r.width(), r.top() + y * r.height());
    let mut shapes = Vec::new();
    match kind {
        Primitive::Stairs => {
            shapes.push(Shape::closed_line(
                vec![
                    p(0.0, 1.0),
                    p(0.0, 0.75),
                    p(0.3, 0.75),
                    p(0.3, 0.5),
                    p(0.6, 0.5),
                    p(0.6, 0.25),
                    p(0.9, 0.25),
                    p(0.9, 0.0),
                    p(1.0, 0.0),
                    p(1.0, 1.0),
                ],
                stroke,
            ));
        }
        Primitive::SpiralStairs => {
            shapes.push(Shape::closed_line(vec![p(0.0, 0.7), p(0.5, 0.45), p(1.0, 0.7), p(0.5, 0.95)], stroke));
            shapes.push(Shape::line(vec![p(0.0, 0.7), p(0.0, 0.45), p(0.5, 0.2), p(1.0, 0.45), p(1.0, 0.7)], stroke));
            shapes.push(Shape::line(vec![p(0.5, 0.45), p(0.5, 0.0)], stroke));
        }
        Primitive::Ramp => {
            shapes.push(Shape::closed_line(vec![p(0.0, 1.0), p(1.0, 1.0), p(1.0, 0.1)], stroke));
            shapes.push(Shape::line(vec![p(0.3, 1.0), p(0.3, 0.75)], stroke));
        }
        Primitive::Pillar => {
            shapes.push(Shape::closed_line(vec![p(0.05, 0.0), p(0.95, 0.0), p(0.95, 0.15), p(0.05, 0.15)], stroke));
            shapes.push(Shape::closed_line(vec![p(0.3, 0.15), p(0.7, 0.15), p(0.7, 0.85), p(0.3, 0.85)], stroke));
            shapes.push(Shape::closed_line(vec![p(0.05, 0.85), p(0.95, 0.85), p(0.95, 1.0), p(0.05, 1.0)], stroke));
        }
        Primitive::Doorway => {
            shapes.push(Shape::closed_line(
                vec![
                    p(0.0, 1.0),
                    p(0.0, 0.0),
                    p(1.0, 0.0),
                    p(1.0, 1.0),
                    p(0.7, 1.0),
                    p(0.7, 0.3),
                    p(0.3, 0.3),
                    p(0.3, 1.0),
                ],
                stroke,
            ));
        }
        _ => return false,
    }
    painter.extend(shapes);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_parse_and_report() {
        let mut o = BlockOptions::default();
        assert_eq!(set_option(&mut o, "steps", "12"), Some(true));
        assert_eq!(o.stair_steps, 12);
        assert_eq!(set_option(&mut o, "steps", "0"), Some(false));
        assert_eq!(set_option(&mut o, "slope", "45"), Some(true));
        assert_eq!(set_option(&mut o, "slope", "90"), Some(false));
        assert_eq!(set_option(&mut o, "open", "on"), Some(true));
        assert!(o.stair_open);
        assert_eq!(set_option(&mut o, "spiral_angle", "-720"), Some(true));
        assert_eq!(set_option(&mut o, "spiral_angle", "0"), Some(false));
        assert_eq!(set_option(&mut o, "sides", "8"), None);
        let values = option_values(&o, &|v| v.to_string());
        assert!(values.iter().any(|v| v.key == "slope" && v.value == "45"));
        assert_eq!(values.len(), 17);
        assert_eq!(set_option(&mut o, "direction", "-x"), Some(true));
        assert_eq!(o.direction, RiseDirection::MinusX);
        assert_eq!(set_option(&mut o, "direction", "up"), Some(false));
        assert_eq!(set_option(&mut o, "door_arch", "on"), Some(true));
        assert_eq!(set_option(&mut o, "door_arch_segments", "1"), Some(false));
        assert_eq!(set_option(&mut o, "door_arch_segments", "12"), Some(true));
        assert!(o.door_arch && o.door_arch_segments == 12);
    }
}
