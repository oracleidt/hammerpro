use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};

fn attrs(values: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    values.into_iter().map(|(key, value)| (key.to_owned(), value)).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os().nth(1).ok_or("expected output PCF path")?;
    let mut doc = Document::new();
    for name in [
        "unlocked_move",
        "locked_move",
        "locked_rotate",
        "locked_range",
        "locked_fade",
        "warp_fixed",
        "warp_transition",
        "collision_floor",
        "collision_slow",
        "collision_half",
        "collision_double",
        "collision_slide",
    ] {
        let system = doc.create_system(&format!("hp_spatial_{name}"))?;
        doc.set_attrs(
            system,
            attrs(vec![
                ("max_particles", Value::Int(1)),
                ("radius", Value::Float(1.0)),
                ("material", Value::Str("particle/fire".into())),
                ("bounding_box_min", Value::Vec3([0.0; 3])),
                ("bounding_box_max", Value::Vec3([0.0; 3])),
            ]),
        )?;
        doc.add_operator_with_params(
            system,
            Category::Emitters,
            "emit_instantaneously",
            attrs(vec![("num_to_emit", Value::Int(1))]),
        )?;
        doc.add_operator_with_params(
            system,
            Category::Initializers,
            "Position Within Box Random",
            attrs(vec![("min", Value::Vec3([10.0, 0.0, 0.0])), ("max", Value::Vec3([10.0, 0.0, 0.0]))]),
        )?;
        doc.add_operator_with_params(
            system,
            Category::Initializers,
            "Lifetime Random",
            attrs(vec![
                ("lifetime_min", Value::Float(if name == "locked_fade" { 1.0 } else { 10.0 })),
                ("lifetime_max", Value::Float(if name == "locked_fade" { 1.0 } else { 10.0 })),
            ]),
        )?;
        if name.starts_with("warp_") {
            doc.add_operator_with_params(
                system,
                Category::Initializers,
                "Position Modify Warp Random",
                attrs(vec![
                    ("control point number", Value::Int(0)),
                    ("warp min", Value::Vec3([0.5, 1.0, 1.0])),
                    ("warp max", Value::Vec3(if name == "warp_fixed" { [0.5, 1.0, 1.0] } else { [2.0, 1.0, 1.0] })),
                    (
                        "warp transition time (treats min/max as start/end sizes)",
                        Value::Float(if name == "warp_transition" { 1.0 } else { 0.0 }),
                    ),
                ]),
            )?;
        }
        if name.starts_with("collision_") {
            doc.add_operator_with_params(
                system,
                Category::Initializers,
                "Velocity Random",
                attrs(vec![
                    (
                        "speed_in_local_coordinate_system_min",
                        Value::Vec3(if name == "collision_slow" {
                            [0.0, 0.0, -80.0]
                        } else if name == "collision_slide" {
                            [80.0, 0.0, -160.0]
                        } else {
                            [0.0, 0.0, -160.0]
                        }),
                    ),
                    (
                        "speed_in_local_coordinate_system_max",
                        Value::Vec3(if name == "collision_slow" {
                            [0.0, 0.0, -80.0]
                        } else if name == "collision_slide" {
                            [80.0, 0.0, -160.0]
                        } else {
                            [0.0, 0.0, -160.0]
                        }),
                    ),
                ]),
            )?;
        }
        doc.add_operator(system, Category::Operators, "Movement Basic")?;
        if name.starts_with("locked_") {
            doc.add_operator_with_params(
                system,
                Category::Operators,
                "Movement Lock to Control Point",
                attrs(vec![
                    ("control_point_number", Value::Int(0)),
                    ("start_fadeout_min", Value::Float(if name == "locked_fade" { 0.0 } else { 1.0 })),
                    ("start_fadeout_max", Value::Float(if name == "locked_fade" { 0.0 } else { 1.0 })),
                    ("end_fadeout_min", Value::Float(if name == "locked_fade" { 0.5 } else { 1.0 })),
                    ("end_fadeout_max", Value::Float(if name == "locked_fade" { 0.5 } else { 1.0 })),
                    ("distance fade range", Value::Float(if name == "locked_range" { 20.0 } else { 0.0 })),
                ]),
            )?;
        }
        if name.starts_with("collision_") {
            doc.add_operator_with_params(
                system,
                Category::Constraints,
                "Collision via traces",
                attrs(vec![
                    ("collision mode", Value::Int(0)),
                    (
                        "amount of bounce",
                        Value::Float(if name == "collision_half" {
                            0.5
                        } else if name == "collision_double" {
                            2.0
                        } else {
                            1.0
                        }),
                    ),
                    ("amount of slide", Value::Float(if name == "collision_slide" { 0.5 } else { 0.0 })),
                    ("radius scale", Value::Float(0.0)),
                ]),
            )?;
        }
        doc.add_operator(system, Category::Renderers, "render_animated_sprites")?;
    }
    std::fs::write(output, doc.to_bytes()?)?;
    println!("Created six spatial native conformance fixtures");
    Ok(())
}
