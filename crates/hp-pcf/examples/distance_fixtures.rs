use hp_pcf::dmx::Value;
use hp_pcf::editor::{Category, Document};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os().nth(1).ok_or("expected output PCF path")?;
    let mut doc = Document::parse(include_bytes!("../tests/data/spatial_cases.pcf"))?;
    let template = doc.find_system("hp_spatial_locked_range").ok_or("missing fixture")?;
    for id in doc.systems() {
        if id != template {
            doc.delete_system(id)?;
        }
    }
    for range in [10, 20, 40] {
        for offset in [5, 10, 20] {
            let id = doc.duplicate_system(template, &format!("hp_distance_r{range}_o{offset}"))?;
            for op in doc.operators(id, Category::Operators) {
                if doc
                    .element(op)
                    .and_then(|e| e.str("functionName"))
                    .is_some_and(|s| s.eq_ignore_ascii_case("Movement Lock to Control Point"))
                {
                    doc.set_attr(op, "distance fade range", Value::Float(range as f32))?;
                }
            }
            for op in doc.operators(id, Category::Initializers) {
                if doc
                    .element(op)
                    .and_then(|e| e.str("functionName"))
                    .is_some_and(|s| s.eq_ignore_ascii_case("Position Within Box Random"))
                {
                    doc.set_attrs(
                        op,
                        vec![
                            ("min".into(), Value::Vec3([offset as f32, 0.0, 0.0])),
                            ("max".into(), Value::Vec3([offset as f32, 0.0, 0.0])),
                        ],
                    )?;
                }
            }
        }
    }
    doc.delete_system(template)?;
    std::fs::write(output, doc.to_bytes()?)?;
    Ok(())
}
