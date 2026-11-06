use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};

fn attrs(values: Vec<(&str, Value)>) -> Vec<(String, Value)> {
    values.into_iter().map(|(key, value)| (key.to_owned(), value)).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os().nth(1).ok_or("expected output PCF path")?;
    let mut doc = Document::new();
    for name in ["gravity", "drag", "fixed_force", "pull", "twist"] {
        let system = doc.create_system(&format!("hp_probe_{name}"))?;
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
            attrs(vec![("lifetime_min", Value::Float(10.0)), ("lifetime_max", Value::Float(10.0))]),
        )?;
        if name == "drag" {
            doc.add_operator_with_params(
                system,
                Category::Initializers,
                "Velocity Random",
                attrs(vec![
                    ("speed_in_local_coordinate_system_min", Value::Vec3([40.0, 0.0, 0.0])),
                    ("speed_in_local_coordinate_system_max", Value::Vec3([40.0, 0.0, 0.0])),
                ]),
            )?;
        }
        doc.add_operator_with_params(
            system,
            Category::Operators,
            "Movement Basic",
            attrs(vec![
                ("gravity", Value::Vec3(if name == "gravity" { [0.0, 0.0, -100.0] } else { [0.0; 3] })),
                ("drag", Value::Float(if name == "drag" { 0.1 } else { 0.0 })),
            ]),
        )?;
        let force = match name {
            "fixed_force" => Some((
                "random force",
                vec![("min force", Value::Vec3([0.0, 10.0, 0.0])), ("max force", Value::Vec3([0.0, 10.0, 0.0]))],
            )),
            "pull" => Some((
                "Pull towards control point",
                vec![
                    ("control point number", Value::Int(0)),
                    ("amount of force", Value::Float(10.0)),
                    ("falloff power", Value::Float(0.0)),
                ],
            )),
            "twist" => Some((
                "twist around axis",
                vec![("twist axis", Value::Vec3([0.0, 0.0, 1.0])), ("amount of force", Value::Float(10.0))],
            )),
            _ => None,
        };
        if let Some((function, values)) = force {
            doc.add_operator_with_params(system, Category::Forces, function, attrs(values))?;
        }
        doc.add_operator(system, Category::Renderers, "render_animated_sprites")?;
    }
    std::fs::write(output, doc.to_bytes()?)?;
    println!("Created five single-particle native conformance fixtures");
    Ok(())
}
