use super::*;
use crate::dmx::{Element, Value};
use serde_json::Value as Json;

fn number(v: &Json) -> f32 {
    v.as_f64().unwrap() as f32
}
fn vector(v: &Json) -> V3 {
    std::array::from_fn(|i| number(&v[i]))
}
fn op(name: &str, params: Vec<(&str, Value)>) -> Operator {
    Operator {
        function: name.into(),
        params: Element { attrs: params.into_iter().map(|(k, v)| (k.into(), v)).collect(), ..Default::default() },
    }
}
fn particle() -> Particle {
    let definition = SystemDef { max_particles: 1, initial_particles: 1, ..Default::default() };
    let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 1);
    sim.step(0.02, [0.0; 3], 1);
    sim.particles.pop().expect("initialized reference particle")
}

#[test]
fn count_remapping_matches_native_batches_values_and_persistent_counters() {
    let data: Json = serde_json::from_str(include_str!("../../../tests/data/initializer_count_native.json")).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 960);
    let template = particle();
    for (index, case) in cases.iter().enumerate() {
        let field = case["field"].as_i64().unwrap() as i32;
        let mut counter = case["counter"].as_i64().unwrap() as i32;
        let operator = op(
            "Remap Particle Count to Scalar",
            vec![
                ("output field", Value::Int(field)),
                ("input minimum", Value::Int(case["input_range"][0].as_i64().unwrap() as i32)),
                ("input maximum", Value::Int(case["input_range"][1].as_i64().unwrap() as i32)),
                ("output minimum", Value::Float(number(&case["output_range"][0]))),
                ("output maximum", Value::Float(number(&case["output_range"][1]))),
                ("output is scalar of initial random range", Value::Bool(case["multiply"].as_bool().unwrap())),
                ("only active within specified input range", Value::Bool(case["within"].as_bool().unwrap())),
            ],
        );
        let remap = Count::compile(&operator).unwrap();
        let mut particles: Vec<_> = case["current"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let mut p = template;
                let value = number(value);
                match remap.output {
                    Field::Alpha => p.base_alpha = value,
                    Field::Alpha2 => p.alpha2 = value,
                    _ => set_field(&mut p, remap.output, value, false),
                }
                p
            })
            .collect();
        remap.initialize(&mut particles, &mut counter);
        assert_eq!(counter, case["final_counter"].as_i64().unwrap() as i32, "case {index}");
        for (p, target) in particles.iter().zip(case["expected"].as_array().unwrap()) {
            assert_eq!(get_field(p, remap.output).to_bits(), number(target).to_bits(), "case {index}");
        }
    }
}

#[test]
fn vector_remapping_matches_native_fields_windows_transforms_and_previous_positions() {
    let data: Json = serde_json::from_str(include_str!("../../../tests/data/initializer_vector_native.json")).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1200);
    let template = particle();
    for (index, case) in cases.iter().enumerate() {
        let field = case["field"].as_i64().unwrap() as i32;
        let cp = case["cp"].as_i64().unwrap() as usize;
        let operator = op(
            "Remap Scalar to Vector",
            vec![
                ("input field", Value::Int(10)),
                ("output field", Value::Int(field)),
                ("input minimum", Value::Float(number(&case["input_range"][0]))),
                ("input maximum", Value::Float(number(&case["input_range"][1]))),
                ("output minimum", Value::Vec3(vector(&case["output_range"][0]))),
                ("output maximum", Value::Vec3(vector(&case["output_range"][1]))),
                ("output is scalar of initial random range", Value::Bool(case["multiply"].as_bool().unwrap())),
                ("emitter lifetime start time (seconds)", Value::Float(number(&case["window"][0]))),
                ("emitter lifetime end time (seconds)", Value::Float(number(&case["window"][1]))),
                ("control_point_number", Value::Int(cp as i32)),
                ("use local system", Value::Bool(case["local"].as_bool().unwrap())),
            ],
        );
        let remap = Vector::compile(&operator).unwrap();
        let mut points = Points::at([0.0; 3]);
        points.pos.resize(64, [0.0; 3]);
        points.pos[cp] = vector(&case["new"]);
        let axes = std::array::from_fn(|i| vector(&case["axes"][i]));
        points.axes = axes;
        points.orientations.resize(64, axes);
        let mut previous = points.clone();
        previous.pos[cp] = vector(&case["old"]);
        let positions = control::Positions {
            current: &points,
            previous: &previous,
            time: number(&case["time"]),
            dt: number(&case["dt"]),
        };
        for particle_index in 0..case["inputs"].as_array().unwrap().len() {
            let mut p = template;
            p.born = number(&case["born"][particle_index]);
            p.trail = number(&case["inputs"][particle_index]);
            p.previous_pos = vector(&case["previous"][particle_index]);
            let current = vector(&case["current"][particle_index]);
            match field {
                0 => p.pos = current,
                2 => p.previous_pos = current,
                6 => p.base_color = current,
                15 => p.hitbox_relative = current,
                _ => unreachable!(),
            }
            remap.initialize(&mut p, &positions);
            let actual = match field {
                0 => p.pos,
                2 => p.previous_pos,
                6 => p.base_color,
                15 => p.hitbox_relative,
                _ => unreachable!(),
            };
            assert_eq!(
                actual.map(f32::to_bits),
                vector(&case["expected"][particle_index]).map(f32::to_bits),
                "case {index}/{particle_index}"
            );
            if field == 0 {
                assert_eq!(
                    p.previous_pos.map(f32::to_bits),
                    vector(&case["expected_previous"][particle_index]).map(f32::to_bits),
                    "previous case {index}/{particle_index}"
                );
            }
        }
    }
}
