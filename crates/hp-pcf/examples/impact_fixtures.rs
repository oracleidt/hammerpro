use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};
fn attrs(v: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    v.into_iter().map(|(k, v)| (k.into(), v)).collect()
}
fn make(
    doc: &mut Document,
    name: &str,
    cp: i32,
    offset: [f32; 3],
    speed: [f32; 3],
    lock: bool,
) -> Result<usize, Box<dyn std::error::Error>> {
    let id = doc.create_system(name)?;
    doc.set_attrs(
        id,
        attrs(vec![
            ("max_particles", Value::Int(1)),
            ("radius", Value::Float(1.0)),
            ("material", Value::Str("particle/fire".into())),
            ("bounding_box_min", Value::Vec3([0.0; 3])),
            ("bounding_box_max", Value::Vec3([0.0; 3])),
        ]),
    )?;
    doc.add_operator_with_params(
        id,
        Category::Emitters,
        "emit_instantaneously",
        attrs(vec![("num_to_emit", Value::Int(1))]),
    )?;
    doc.add_operator_with_params(
        id,
        Category::Initializers,
        "Position Within Box Random",
        attrs(vec![
            ("control point number", Value::Int(cp)),
            ("min", Value::Vec3(offset)),
            ("max", Value::Vec3(offset)),
        ]),
    )?;
    doc.add_operator_with_params(
        id,
        Category::Initializers,
        "Lifetime Random",
        attrs(vec![("lifetime_min", Value::Float(10.0)), ("lifetime_max", Value::Float(10.0))]),
    )?;
    doc.add_operator_with_params(
        id,
        Category::Initializers,
        "Velocity Random",
        attrs(vec![
            ("speed_in_local_coordinate_system_min", Value::Vec3(speed)),
            ("speed_in_local_coordinate_system_max", Value::Vec3(speed)),
        ]),
    )?;
    doc.add_operator(id, Category::Operators, "Movement Basic")?;
    if lock {
        doc.add_operator_with_params(
            id,
            Category::Operators,
            "Movement Lock to Control Point",
            attrs(vec![
                ("control_point_number", Value::Int(cp)),
                ("start_fadeout_min", Value::Float(1.0)),
                ("start_fadeout_max", Value::Float(1.0)),
                ("end_fadeout_min", Value::Float(1.0)),
                ("end_fadeout_max", Value::Float(1.0)),
            ]),
        )?;
    }
    doc.add_operator(id, Category::Renderers, "render_animated_sprites")?;
    Ok(id)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os().nth(1).ok_or("expected output PCF path")?;
    let mut doc = Document::new();
    for name in [
        "floor",
        "offset",
        "miss",
        "stopped",
        "upward",
        "bias_half",
        "bias_double",
        "miss_offset",
        "miss_bias_half",
        "miss_bias_double",
    ] {
        let speed = if name == "stopped" {
            0.0
        } else if name == "upward" {
            160.0
        } else {
            -160.0
        };
        let root = make(&mut doc, &format!("hp_impact_{name}"), 0, [10.0, 0.0, 0.0], [0.0, 0.0, speed], false)?;
        doc.add_operator_with_params(
            root,
            Category::Initializers,
            "Lifetime from Time to Impact",
            attrs(vec![
                ("trace offset", Value::Float(if name == "offset" || name == "miss_offset" { -400.0 } else { 0.0 })),
                (
                    "maximum trace length",
                    Value::Float(if name.starts_with("miss") {
                        16.0
                    } else if name == "offset" {
                        900.0
                    } else {
                        256.0
                    }),
                ),
                ("trace collision group", Value::Str("DEBRIS".into())),
                (
                    "bias distance",
                    Value::Vec3([
                        1.0,
                        1.0,
                        if name.ends_with("bias_half") {
                            0.5
                        } else if name.ends_with("bias_double") {
                            2.0
                        } else {
                            1.0
                        },
                    ]),
                ),
                ("maximum points to cache", Value::Int(64)),
                ("trace recycle tolerance", Value::Float(64.0)),
                ("collide with water", Value::Bool(true)),
            ]),
        )?;
        doc.add_operator(root, Category::Operators, "Lifespan Decay")?;
    }
    std::fs::write(output, doc.to_bytes()?)?;
    Ok(())
}
