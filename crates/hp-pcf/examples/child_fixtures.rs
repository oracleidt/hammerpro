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
    for (name, cp, lock) in [("initial", 1, false), ("follow", 1, true), ("five", 5, true)] {
        let root = make(&mut doc, &format!("hp_child_{name}"), 0, [10.0, 0.0, 0.0], [30.0, 0.0, 0.0], false)?;
        let child = make(&mut doc, &format!("hp_child_{name}_child"), cp, [100.0, 10.0, 0.0], [0.0; 3], lock)?;
        doc.add_operator_with_params(
            root,
            Category::Operators,
            "Set child control points from particle positions",
            attrs(vec![
                ("# of control points to set", Value::Int(1)),
                ("First control point to set", Value::Int(cp)),
                ("first particle to copy", Value::Int(0)),
                ("Group ID to affect", Value::Int(0)),
            ]),
        )?;
        doc.add_child(root, child, 0.0)?;
    }
    std::fs::write(output, doc.to_bytes()?)?;
    Ok(())
}
