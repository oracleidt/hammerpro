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
    for name in ["down", "up", "offset", "up_offset", "oblique"] {
        let root = make(&mut doc, &format!("hp_cpimpact_{name}"), 4, [10.0, 0.0, 0.0], [0.0; 3], true)?;
        doc.add_operator_with_params(
            root,
            Category::Operators,
            "Set Control Point to Impact Point",
            attrs(vec![
                ("trace collision group", Value::Str("DEBRIS".into())),
                ("Trace Update Rate", Value::Float(0.1)),
                (
                    "Trace Direction Override",
                    Value::Vec3(if name.starts_with("up") {
                        [0.0, 0.0, 1.0]
                    } else if name == "oblique" {
                        [1.0, 0.0, -0.03]
                    } else {
                        [0.0, 0.0, -1.0]
                    }),
                ),
                ("Control Point to Set", Value::Int(4)),
                ("Control Point to Trace From", Value::Int(1)),
                (
                    "Max Trace Length",
                    Value::Float(if name.starts_with("up") {
                        128.0
                    } else if name == "oblique" {
                        2048.0
                    } else {
                        1024.0
                    }),
                ),
                (
                    "Offset End Point Amount",
                    Value::Float(if name.ends_with("offset") || name == "oblique" { 16.0 } else { 0.0 }),
                ),
            ]),
        )?;
    }
    std::fs::write(output, doc.to_bytes()?)?;
    Ok(())
}
