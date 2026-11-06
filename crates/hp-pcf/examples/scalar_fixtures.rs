use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};

fn attrs(values: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    values.into_iter().map(|(key, value)| (key.to_owned(), value)).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os().nth(1).ok_or("expected output PCF path")?;
    let mut doc = Document::new();
    for name in ["cp_lifetime", "distance_lifetime", "dot_lifetime", "inherit_velocity"] {
        let system = doc.create_system(&format!("hp_scalar_{name}"))?;
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
            attrs(vec![
                ("num_to_emit", Value::Int(1)),
                ("emission_start_time", Value::Float(if name == "inherit_velocity" { 0.5 } else { 0.0 })),
            ]),
        )?;
        doc.add_operator_with_params(
            system,
            Category::Initializers,
            "Position Within Box Random",
            attrs(vec![
                ("min", Value::Vec3(if name == "inherit_velocity" { [100.0, 0.0, 0.0] } else { [10.0, 0.0, 0.0] })),
                ("max", Value::Vec3(if name == "inherit_velocity" { [100.0, 0.0, 0.0] } else { [10.0, 0.0, 0.0] })),
            ]),
        )?;
        doc.add_operator_with_params(
            system,
            Category::Initializers,
            "Lifetime Random",
            attrs(vec![("lifetime_min", Value::Float(10.0)), ("lifetime_max", Value::Float(10.0))]),
        )?;
        let (category, function, mut parameters) = match name {
            "cp_lifetime" => (
                Category::Initializers,
                "Remap Control Point to Scalar",
                vec![
                    ("input control point number", Value::Int(2)),
                    ("input minimum", Value::Float(0.0)),
                    ("input maximum", Value::Float(1.0)),
                    ("output minimum", Value::Float(0.1)),
                    ("output maximum", Value::Float(0.9)),
                ],
            ),
            "distance_lifetime" => (
                Category::Operators,
                "Remap Distance to Control Point to Scalar",
                vec![
                    ("control point", Value::Int(0)),
                    ("distance minimum", Value::Float(0.0)),
                    ("distance maximum", Value::Float(20.0)),
                    ("output minimum", Value::Float(0.1)),
                    ("output maximum", Value::Float(0.5)),
                ],
            ),
            "dot_lifetime" => (
                Category::Operators,
                "Remap Dot Product to Scalar",
                vec![
                    ("first input control point", Value::Int(1)),
                    ("second input control point", Value::Int(2)),
                    ("input minimum (-1 to 1)", Value::Float(-1.0)),
                    ("input maximum (-1 to 1)", Value::Float(1.0)),
                    ("output minimum", Value::Float(0.1)),
                    ("output maximum", Value::Float(0.5)),
                ],
            ),
            _ => (
                Category::Initializers,
                "Velocity Inherit from Control Point",
                vec![("control point number", Value::Int(1)), ("velocity scale", Value::Float(0.5))],
            ),
        };
        parameters.push(("output field", Value::Int(1)));
        doc.add_operator_with_params(system, category, function, attrs(parameters))?;
        doc.add_operator(system, Category::Operators, "Movement Basic")?;
        doc.add_operator(system, Category::Operators, "Lifespan Decay")?;
        doc.add_operator(system, Category::Renderers, "render_animated_sprites")?;
    }
    std::fs::write(output, doc.to_bytes()?)?;
    println!("Created four scalar native conformance fixtures");
    Ok(())
}
