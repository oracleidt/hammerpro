use std::sync::Arc;

use crate::dmx::{Element, Value};
use crate::sheet::{FULL_UV, Sheet, parse_sheet, sheet_from_vtf};
use crate::sim::{Points, RenderKind};
use crate::{Child, Effect, Operator, Plan, Sim, SystemDef};

fn op(function: &str, attrs: Vec<(&str, Value)>) -> Operator {
    Operator {
        function: function.to_owned(),
        params: Element {
            kind: "DmeParticleOperator".into(),
            name: function.to_owned(),
            attrs: attrs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        },
    }
}

fn fl(v: f32) -> Value {
    Value::Float(v)
}

fn system(name: &str, emitters: Vec<Operator>, inits: Vec<Operator>, ops: Vec<Operator>) -> SystemDef {
    SystemDef {
        name: name.to_owned(),
        material: "particle/test".to_owned(),
        emitters,
        initializers: inits,
        operators: ops,
        renderers: vec![op("render_animated_sprites", vec![])],
        ..Default::default()
    }
}

fn burst(n: i32) -> Operator {
    op("emit_instantaneously", vec![("num_to_emit", Value::Int(n))])
}

fn run(def: &SystemDef, points: &Points, seconds: f32) -> Sim {
    let mut sim = Sim::new(Arc::new(Plan::new(def)), 11);
    let steps = (seconds / 0.05).round() as usize;
    for _ in 0..steps.max(1) {
        sim.step_at(0.05, points, &[], 500);
    }
    sim
}

struct FloorTrace;

#[test]
fn delayed_bursts_keep_native_birth_dates_across_frame_boundaries() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/instant_birth_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/instant_birth_native_samples.json")).unwrap();
    for case in cases {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let mut definition = definitions.iter().find(|def| def.name == name).unwrap().clone();
        definition.initializers.retain(|op| op.function != "Remap Scalar to Vector");
        definition.initializers.push(op(
            "Remap Initial Scalar",
            vec![
                ("input field", Value::Int(8)),
                ("input minimum", fl(0.0)),
                ("input maximum", fl(16.0)),
                ("output field", Value::Int(3)),
                ("output minimum", fl(0.0)),
                ("output maximum", fl(16.0)),
            ],
        ));
        let born = case["born"].as_f64().unwrap() as f32;
        for dt in [0.016, 0.033, 0.05, 0.1] {
            let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 17);
            let mut time = 0.0;
            while time < 1.1 {
                time += dt;
                sim.step(dt, [0.0; 3], 1);
                assert_eq!(sim.particles.len(), usize::from(time >= born), "{name} dt{dt} time{time}");
                if let Some(particle) = sim.particles.first() {
                    assert_eq!(particle.radius.to_bits(), born.to_bits(), "{name} dt{dt}");
                    assert!((particle.age - (time - born)).abs() < 0.000002, "{name} dt{dt} age{}", particle.age);
                }
            }
        }
    }
}

#[test]
fn long_lifetimes_and_pre_age_match_native_without_an_editor_time_cap() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/long_life_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/long_life_native_samples.json")).unwrap();
    for case in cases {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let mut definition = definitions.iter().find(|def| def.name == name).unwrap().clone();
        definition.initializers.retain(|op| op.function != "Remap Scalar to Vector");
        let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
        sim.step(0.0, [0.0; 3], 1);
        let expected = -case["born"].as_f64().unwrap() as f32;
        assert_eq!(sim.particles[0].age, expected, "{name}");
        assert_eq!(sim.particles[0].life, expected * 2.0, "{name}");
    }
}

#[test]
fn alpha2_stays_independent_and_combines_with_native_render_list_rounding() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/alpha2_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/alpha2_native_samples.json")).unwrap();
    assert_eq!(cases.len(), 30);
    for case in cases {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let definition = definitions.iter().find(|def| def.name == name).unwrap();
        let mut sim = Sim::new(Arc::new(Plan::new(definition)), 0);
        sim.step(0.0, [0.0; 3], 1);
        let particle = &sim.particles[0];
        assert_eq!(particle.alpha.to_bits(), (case["primary"].as_f64().unwrap() as f32).to_bits(), "{name}");
        assert_eq!(particle.alpha2.to_bits(), (case["secondary"].as_f64().unwrap() as f32).to_bits(), "{name}");
        assert_eq!(particle.opacity().to_bits(), (case["opacity"].as_f64().unwrap() as f32).to_bits(), "{name}");
        assert_eq!(particle.life, 10000.0, "alpha initialization must preserve the configured lifetime");
        let expected = case["pixel"][1].as_u64().unwrap();
        assert_eq!(particle.opacity() > 0.01, expected > 0, "{name}");
    }
}

#[test]
fn alpha2_remaps_and_ramps_preserve_primary_alpha_and_long_lifetimes() {
    let definition = system(
        "alpha2-ramp",
        vec![burst(1)],
        vec![
            op("Lifetime Random", vec![("lifetime_min", fl(4000.0)), ("lifetime_max", fl(4000.0))]),
            op("Alpha Random", vec![("alpha_min", Value::Int(128)), ("alpha_max", Value::Int(128))]),
            op(
                "Remap Noise to Scalar",
                vec![("output field", Value::Int(16)), ("output minimum", fl(0.25)), ("output maximum", fl(0.25))],
            ),
        ],
        vec![op("Ramp Scalar Linear Simple", vec![("ramp field", Value::Int(16)), ("ramp rate", fl(1.0))])],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
    sim.step(0.0, [0.0; 3], 1);
    sim.step(0.05, [0.0; 3], 1);
    assert_eq!(sim.particles[0].alpha, 128.0 / 255.0);
    assert!((sim.particles[0].alpha2 - 0.3).abs() < 1e-6);
    assert_eq!(sim.particles[0].life, 4000.0);
}

#[test]
fn native_initializer_passes_preserve_pre_age_for_both_authored_orders() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/preage_order_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/preage_order_native_samples.json")).unwrap();
    for case in cases {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let mut definition = definitions.iter().find(|def| def.name == name).unwrap().clone();
        definition.initializers.retain(|op| op.function != "Remap Scalar to Vector");
        let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
        sim.step(0.0, [0.0; 3], 1);
        assert_eq!(sim.particles[0].age, -case["born"].as_f64().unwrap() as f32, "{name}");
    }
}

#[test]
fn base_attributes_are_initialized_once_then_modified_in_authored_order() {
    let definition = system(
        "initializer-passes",
        vec![burst(1)],
        vec![
            op(
                "Remap Initial Scalar",
                vec![
                    ("input field", Value::Int(3)),
                    ("input minimum", fl(0.0)),
                    ("input maximum", fl(10.0)),
                    ("output field", Value::Int(3)),
                    ("output minimum", fl(100.0)),
                    ("output maximum", fl(200.0)),
                ],
            ),
            op("Radius Random", vec![("radius_min", fl(2.0)), ("radius_max", fl(2.0))]),
            op("Radius Random", vec![("radius_min", fl(16.0)), ("radius_max", fl(16.0))]),
        ],
        vec![],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
    sim.step(0.0, [0.0; 3], 1);
    assert_eq!(sim.particles[0].radius, 120.0);
}

#[test]
fn scalar_noise_world_clock_reaches_the_simulation_environment() {
    struct Clock {
        ticks: [u32; 3],
        calls: std::cell::Cell<usize>,
    }
    impl crate::environment::Environment for Clock {
        fn time_milliseconds(&self) -> u32 {
            let index = self.calls.get();
            self.calls.set(index + 1);
            self.ticks[index]
        }
    }
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/noise_initializers_native_math.json")).unwrap();
    let case = cases
        .iter()
        .find(|c| {
            c["kind"] == "remap" && c["field"] == 3 && c["world_scale"] != 0 && c["expected"].as_f64().unwrap() > 0.0
        })
        .unwrap();
    let read = |name: &str| case[name].as_f64().unwrap() as f32;
    let vector = |name: &str| std::array::from_fn(|axis| case[name][axis].as_f64().unwrap() as f32);
    let definition = system(
        "world-noise",
        vec![burst(1)],
        vec![
            op(
                "Position Within Box Random",
                vec![("min", Value::Vec3(vector("position"))), ("max", Value::Vec3(vector("position")))],
            ),
            op(
                "Remap Noise to Scalar",
                vec![
                    ("output field", Value::Int(3)),
                    ("output minimum", fl(read("min"))),
                    ("output maximum", fl(read("max"))),
                    ("time noise coordinate scale", fl(read("time_scale"))),
                    ("spatial noise coordinate scale", fl(read("spatial_scale"))),
                    ("time coordinate offset", fl(read("born") + read("time_offset"))),
                    ("spatial coordinate offset", Value::Vec3(vector("spatial_offset"))),
                    ("world time noise coordinate scale", fl(read("world_scale"))),
                    ("absolute value", Value::Bool(case["absolute"].as_bool().unwrap())),
                    ("invert absolute value", Value::Bool(case["invert"].as_bool().unwrap())),
                ],
            ),
        ],
        vec![],
    );
    let world = Clock {
        ticks: std::array::from_fn(|axis| case["ticks"][axis].as_u64().unwrap() as u32),
        calls: std::cell::Cell::new(0),
    };
    let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
    sim.step_in(0.0, &Points::at([0.0; 3]), &[], 1, &world);
    assert_eq!(world.calls.get(), 3);
    assert_eq!(sim.particles[0].radius.to_bits(), read("expected").to_bits());
}

#[test]
fn scalar_noise_initializers_match_native_pixels_and_creation_times() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/noise_initializers_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/noise_initializers_native_samples.json")).unwrap();
    for case in cases {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let mut definition = definitions.iter().find(|def| def.name == name).unwrap().clone();
        definition.initializers.retain(|op| op.function != "Remap Scalar to Vector");
        let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
        sim.step(0.0, [0.0; 3], 1);
        assert_eq!(sim.particles.len(), 1, "{name}");
        let particle = &sim.particles[0];
        if case["kind"] == "preage" {
            let expected = case["measured_born"].as_f64().unwrap() as f32;
            assert!((particle.age + expected).abs() < 0.0001, "{name}: {} vs {}", particle.age, -expected);
        } else {
            let gray = ((128.0f32 / 255.0 + 0.055) / 1.055).powf(2.4);
            let linear = gray * (128.0 / 255.0) * particle.alpha;
            let pixel =
                (if linear <= 0.0031308 { linear * 12.92 } else { 1.055 * linear.powf(1.0 / 2.4) - 0.055 }) * 255.0;
            let expected = case["pixel"][0].as_f64().unwrap() as f32;
            assert!((pixel.round() - expected).abs() <= 1.0, "{name}: {pixel} vs {expected}");
        }
    }
}

#[test]
fn pre_age_changes_creation_time_before_later_initializers_and_can_reach_full_lifetime() {
    let definition = system(
        "full-pre-age",
        vec![burst(1)],
        vec![
            op("Lifetime Random", vec![("lifetime_min", fl(10.0)), ("lifetime_max", fl(10.0))]),
            op("Lifetime Pre-Age Noise", vec![("start age minimum", fl(1.0)), ("start age maximum", fl(1.0))]),
            op(
                "Remap Initial Scalar",
                vec![
                    ("input field", Value::Int(8)),
                    ("input minimum", fl(-10.0)),
                    ("input maximum", fl(0.0)),
                    ("output field", Value::Int(3)),
                    ("output minimum", fl(1.0)),
                    ("output maximum", fl(11.0)),
                ],
            ),
        ],
        vec![],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
    sim.step(0.0, [0.0; 3], 1);
    let particle = &sim.particles[0];
    assert_eq!(particle.age, 10.0);
    assert_eq!(particle.radius, 1.0);
}

#[test]
fn velocity_noise_matches_native_trajectories_and_uses_the_selected_control_point() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/velocity_noise_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/velocity_noise_native_samples.json")).unwrap();
    for case in cases {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let definition = definitions.iter().find(|def| def.name == name).unwrap();
        let mut sim = Sim::new(Arc::new(Plan::new(definition)), 0);
        sim.step(0.0, [0.0; 3], 1);
        for axis in 0..3 {
            let expected = case["velocity"][axis].as_f64().unwrap() as f32;
            assert!((sim.particles[0].vel[axis] - expected).abs() < 1e-5, "{name} axis {axis}");
        }
    }
    let definition = system(
        "local-noise",
        vec![burst(1)],
        vec![op(
            "Velocity Noise",
            vec![
                ("Control Point Number", Value::Int(1)),
                ("Apply Velocity in Local Space (0/1)", Value::Bool(true)),
                ("output minimum", Value::Vec3([10.0, 20.0, 30.0])),
                ("output maximum", Value::Vec3([10.0, 20.0, 30.0])),
            ],
        )],
        vec![],
    );
    let mut points = Points::at([100.0; 3]);
    points.orientations = vec![points.axes, [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]];
    let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
    sim.step_at(0.0, &points, &[], 1);
    assert_eq!(sim.particles[0].vel, [-20.0, 10.0, 30.0]);
}

#[test]
fn spatial_noise_matches_native_scalar_and_vector_samples() {
    for (file, reference, vector) in [
        (
            include_bytes!("../tests/data/noise_scalar_cases.pcf").as_slice(),
            include_str!("../tests/data/noise_scalar_native.json"),
            false,
        ),
        (
            include_bytes!("../tests/data/noise_vector_cases.pcf").as_slice(),
            include_str!("../tests/data/noise_vector_native.json"),
            true,
        ),
    ] {
        let definitions = crate::parse_file(file).unwrap();
        let cases: Vec<serde_json::Value> = serde_json::from_str(reference).unwrap();
        for case in cases {
            let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
            let definition = definitions.iter().find(|def| def.name == name).unwrap();
            for seed in [0, 17, u32::MAX] {
                let mut sim = Sim::new(Arc::new(Plan::new(definition)), seed);
                sim.step(0.0, [0.0; 3], 1);
                for _ in 0..3 {
                    let particle = &sim.particles[0];
                    if vector {
                        for axis in 0..3 {
                            let expected = case["color"][axis].as_f64().unwrap() as f32;
                            assert_eq!(particle.color[axis].to_bits(), expected.to_bits(), "{name} channel {axis}");
                        }
                    } else {
                        let expected = case["alpha"].as_f64().unwrap() as f32;
                        assert_eq!(particle.alpha.to_bits(), expected.to_bits(), "{name}");
                    }
                    // Spatial noise remains stationary when only age and the collection seed change.
                    sim.step(0.05, [0.0; 3], 1);
                }
            }
        }
    }
}

#[test]
fn additive_noise_integrates_current_fields_and_vector_defaults_to_color() {
    let mut definition = system(
        "additive-noise",
        vec![burst(1)],
        vec![],
        vec![
            op(
                "Noise Scalar",
                vec![
                    ("output field", Value::Int(7)),
                    ("output minimum", fl(0.2)),
                    ("output maximum", fl(0.2)),
                    ("additive", Value::Bool(true)),
                ],
            ),
            op(
                "Noise Scalar",
                vec![
                    ("output field", Value::Int(3)),
                    ("output minimum", fl(10.0)),
                    ("output maximum", fl(10.0)),
                    ("additive", Value::Bool(true)),
                ],
            ),
            op(
                "Noise Vector",
                vec![
                    ("output minimum", Value::Vec3([0.2; 3])),
                    ("output maximum", Value::Vec3([0.2; 3])),
                    ("additive", Value::Bool(true)),
                ],
            ),
        ],
    );
    definition.color = [0, 0, 0, 0];
    definition.radius = 16.0;
    let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
    sim.step(0.0, [0.0; 3], 1);
    for frame in 1..=5 {
        sim.step(0.05, [0.0; 3], 1);
        let particle = &sim.particles[0];
        assert!((particle.alpha - frame as f32 * 0.01).abs() < 1e-6);
        assert!((particle.radius - (16.0 + frame as f32 * 0.5)).abs() < 1e-6);
        assert!(particle.color.iter().all(|value| (*value - frame as f32 * 0.01).abs() < 1e-6));
        assert_eq!(particle.pos, [0.0; 3]);
    }
}

#[test]
fn rotation_initializers_match_native_constants_signs_and_zero_exponents() {
    let mut definitions = crate::parse_file(include_bytes!("../tests/data/rotation_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/rotation_native_samples.json")).unwrap();
    for definition in &mut definitions {
        let name = definition.name.strip_prefix("hp_spritecard_").unwrap();
        let case = cases.iter().find(|case| case["name"] == name).unwrap();
        let speed = definition.initializers.iter().any(|op| op.function == "Rotation Speed Random");
        definition.initializers.retain(|op| op.function != "Remap Scalar to Vector");
        if speed {
            definition.operators.push(op("Rotation Basic", vec![]));
        }
        let plan = Arc::new(Plan::new(definition));
        assert!(plan.unsupported.is_empty());
        let mut sim = Sim::new(plan, 17);
        sim.step(0.0, [0.0, 0.0, 64.0], 100);
        assert_eq!(sim.particles.len(), definition.max_particles as usize);
        if speed {
            sim.step(0.05, [0.0, 0.0, 64.0], 100);
        }
        let mut low = 0.0f32;
        let mut high = 0.0f32;
        for particle in &sim.particles {
            let value = particle.rotation / if speed { 0.05 } else { 1.0 };
            low = low.min(value);
            high = high.max(value);
        }
        assert!((low - case["low"].as_f64().unwrap() as f32).abs() < 0.00001, "{name}: low{low}");
        assert!((high - case["high"].as_f64().unwrap() as f32).abs() < 0.00001, "{name}: high{high}");
    }
}

#[test]
fn child_control_point_groups_match_native_matching_and_nonmatching_groups() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/group_cases.pcf")).unwrap();
    let plans: std::collections::HashMap<_, _> =
        definitions.iter().map(|def| (def.name.clone(), Arc::new(Plan::new(def)))).collect();
    assert!(plans.values().all(|plan| plan.unsupported.is_empty()));
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/group_native_samples.json")).unwrap();
    let baseline = &cases.iter().find(|case| case["name"] == "baseline").unwrap()["samples"];
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let mut effect = Effect::new(&format!("hp_spritecard_{name}"), &|name| plans.get(name).cloned(), 17).unwrap();
        effect.step(0.0, [0.0, 0.0, 64.0], 10);
        for (index, sample) in case["samples"].as_array().unwrap().iter().enumerate() {
            let dt = sample["dt"].as_f64().unwrap() as f32;
            for _ in 0..if index == 0 { 4 } else { 5 } {
                effect.step(dt, [0.0, 0.0, 64.0], 10);
            }
            let maximum = effect.particles().map(|(_, part)| part.pos[0]).reduce(f32::max).unwrap();
            let parent = effect.sims[0].1.particles[0].pos[0];
            let expected = (sample["high"][0].as_f64().unwrap() - baseline[index]["high"][0].as_f64().unwrap()) as f32;
            let actual = maximum - parent;
            assert!((actual - expected).abs() < 0.002, "{name} sample{index}: child offset{actual} native{expected}");
        }
    }
}

#[test]
fn missing_position_initializers_match_native_world_origin_instead_of_control_point() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/position_native_samples.json")).unwrap();
    for case in cases {
        let vector = |value: &serde_json::Value| std::array::from_fn(|axis| value[axis].as_f64().unwrap() as f32);
        let cp = vector(&case["control_point"]);
        let mut initializers = Vec::new();
        if !case["offset"].is_null() {
            let offset = vector(&case["offset"]);
            initializers.push(op(
                "Position Within Box Random",
                vec![("min", Value::Vec3(offset)), ("max", Value::Vec3(offset))],
            ));
        }
        let def = system(case["name"].as_str().unwrap(), vec![burst(1)], initializers, vec![]);
        let mut sim = Sim::new(Arc::new(Plan::new(&def)), 17);
        sim.step(0.0, cp, 1);
        assert_eq!(sim.particles.len(), 1);
        let position = sim.particles[0].pos;
        let high = vector(&case["bounds_high"]);
        assert_eq!(position, std::array::from_fn(|axis| cp[axis] + high[axis]), "{}", case["name"]);
        assert!(case["pixel"][0].as_u64().unwrap() > 100, "native particle was rendered");
    }
}

impl crate::environment::Environment for FloorTrace {
    fn trace(&self, start: [f32; 3], end: [f32; 3], radius: f32, _: bool) -> Option<crate::environment::Trace> {
        let a = start[2] - radius;
        let b = end[2] - radius;
        if a >= 0.0 && b < 0.0 {
            Some(crate::environment::Trace { fraction: a / (a - b), normal: [0.0, 0.0, 1.0], start_solid: false })
        } else {
            None
        }
    }
}

#[test]
fn traced_collisions_bounce_after_movement_and_preserve_the_slide_velocity() {
    let mut def = system("bounce", vec![burst(1)], vec![], vec![op("Movement Basic", vec![])]);
    def.radius = 0.25;
    def.constraints.push(op("Collision via traces", vec![("amount of bounce", fl(1.0)), ("amount of slide", fl(1.0))]));
    let plan = Arc::new(Plan::new(&def));
    assert!(plan.unsupported.is_empty());
    let mut sim = Sim::new(plan, 17);
    sim.step(0.0, [0.0, 0.0, 1.0], 10);
    sim.particles[0].vel = [10.0, 0.0, -40.0];
    sim.step_in(0.1, &Points::at([0.0, 0.0, 1.0]), &[], 10, &FloorTrace);
    let particle = sim.particles[0];
    assert!(particle.pos[0] > 1.0);
    assert!(particle.pos[2] >= particle.radius);
    assert!((particle.vel[0] - 12.425356).abs() < 0.005);
    assert!((particle.vel[2] - 9.701425).abs() < 0.005);
    def.constraints[0] = op("Collision via traces", vec![("collision mode", Value::Int(1))]);
    assert_eq!(Plan::new(&def).unsupported, ["Collision via traces"]);
}

#[test]
fn plane_and_distance_constraints_apply_to_the_position_after_movement() {
    let def = system(
        "plane",
        vec![burst(1)],
        vec![],
        vec![
            op("Movement Basic", vec![]),
            op(
                "Prevent passing through a plane",
                vec![("global normal", Value::Bool(true)), ("global origin", Value::Bool(true))],
            ),
        ],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&def)), 17);
    sim.step(0.0, [0.0, 0.0, 1.0], 10);
    sim.particles[0].vel = [10.0, 0.0, -40.0];
    sim.step(0.1, [0.0; 3], 10);
    assert_eq!(sim.particles[0].pos, [1.0, 0.0, 0.0]);
    assert_eq!(sim.particles[0].vel, [10.0, 0.0, 0.0]);
    let mut def = def;
    def.operators[1] = op("Constrain Distance to Control Point", vec![("maximum distance", fl(0.5))]);
    let mut sim = Sim::new(Arc::new(Plan::new(&def)), 17);
    sim.step(0.0, [0.0; 3], 10);
    sim.particles[0].vel = [100.0, 0.0, 0.0];
    sim.step(0.1, [0.0; 3], 10);
    assert_eq!(sim.particles[0].pos, [0.5, 0.0, 0.0]);
}

#[test]
fn spatial_culling_removes_particles_even_without_lifespan_decay() {
    let def = system(
        "cull",
        vec![burst(1)],
        vec![],
        vec![op("Movement Basic", vec![]), op("Cull when crossing sphere", vec![("Cull Distance", fl(1.0))])],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&def)), 17);
    sim.step(0.0, [0.0; 3], 10);
    sim.particles[0].vel = [100.0, 0.0, 0.0];
    sim.step(0.1, [0.0; 3], 10);
    assert!(sim.particles.is_empty());
}

#[test]
fn random_culling_matches_native_lifetime_distributions_and_window_limits() {
    let captures: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/cull_native_samples.json")).unwrap();
    for case in captures {
        let def = system(
            "random cull",
            vec![burst(1)],
            vec![op("Lifetime Random", vec![("lifetime_min", fl(2.0)), ("lifetime_max", fl(2.0))])],
            vec![
                op(
                    "Cull Random",
                    vec![
                        ("Cull Start Time", fl(case["start"].as_f64().unwrap() as f32)),
                        ("Cull End Time", fl(case["end"].as_f64().unwrap() as f32)),
                        ("Cull Time Exponent", fl(case["exponent"].as_f64().unwrap() as f32)),
                        ("Cull Percentage", fl(case["percentage"].as_f64().unwrap() as f32)),
                    ],
                ),
                op("Lifespan Decay", vec![]),
            ],
        );
        let plan = Arc::new(Plan::new(&def));
        assert!(plan.unsupported.is_empty());
        let mut total = 0.0;
        for seed in 1..=2048 {
            let mut sim = Sim::new(Arc::clone(&plan), seed);
            let mut last = 0.0;
            for _ in 0..81 {
                sim.step(0.05, [0.0; 3], 1);
                let Some(particle) = sim.particles.first() else {
                    break;
                };
                last = particle.age / particle.life;
            }
            total += last;
        }
        let samples = case["normalized_last_visible_ages"].as_array().unwrap();
        let native = samples.iter().map(|v| v.as_f64().unwrap()).sum::<f64>() / samples.len() as f64;
        let preview = f64::from(total / 2048.0);
        assert!((native - preview).abs() < 0.08, "{}: native {native}, preview {preview}", case["name"]);
    }
    let def = system(
        "exact cull endpoint",
        vec![burst(1)],
        vec![],
        vec![op(
            "Cull Random",
            vec![("Cull Start Time", fl(0.5)), ("Cull End Time", fl(0.5)), ("Cull Percentage", fl(1.0))],
        )],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&def)), 17);
    sim.step(0.0, [0.0; 3], 1);
    assert_eq!(sim.particles.len(), 1);
    sim.particles[0].age = 0.5;
    sim.step(0.0, [0.0; 3], 1);
    assert!(sim.particles.is_empty());
}

#[test]
fn movement_drag_and_forces_match_captured_garrys_mod_particle_positions() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/operator_cases.pcf")).unwrap();
    let captures: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/operator_native_samples.json")).unwrap();
    let vector = |value: &serde_json::Value| {
        let v = value.as_array().unwrap();
        [v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32, v[2].as_f64().unwrap() as f32]
    };
    for definition in definitions {
        let name = definition.name.strip_prefix("hp_probe_").unwrap();
        let plan = Arc::new(Plan::new(&definition));
        assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
        let mut sim = Sim::new(plan, 17);
        for sample in captures["cases"][name].as_array().unwrap() {
            let lo = vector(&sample["lo"]);
            let hi = vector(&sample["hi"]);
            if (0..3).any(|axis| lo[axis] > hi[axis]) {
                continue;
            }
            sim.step(sample["dt"].as_f64().unwrap() as f32, [0.0, 0.0, 64.0], 1);
            let expected = [0, 1, 2].map(|axis| if lo[axis].abs() > hi[axis].abs() { lo[axis] } else { hi[axis] });
            let position = sim.particles[0].pos;
            let actual = [position[0], position[1], position[2] - 64.0];
            let error = actual.iter().zip(expected).map(|(a, b)| (a - b).powi(2)).sum::<f32>().sqrt();
            assert!(error < 0.001, "{name}, {}: actual {actual:?}, native {expected:?}", sim.time());
        }
    }
}

#[test]
fn control_point_motion_warp_and_collisions_match_native_particle_samples() {
    struct NativeFloor;
    impl crate::environment::Environment for NativeFloor {
        fn trace(&self, start: [f32; 3], end: [f32; 3], radius: f32, _: bool) -> Option<crate::environment::Trace> {
            let a = start[2] - radius;
            let b = end[2] - radius;
            (a >= 0.0 && b < 0.0).then(|| crate::environment::Trace {
                fraction: ((a - 0.03125) / (a - b)).clamp(0.0, 1.0),
                normal: [0.0, 0.0, 1.0],
                start_solid: false,
            })
        }
    }
    let definitions = crate::parse_file(include_bytes!("../tests/data/spatial_cases.pcf")).unwrap();
    let captures: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/spatial_native_samples.json")).unwrap();
    let vector = |value: &serde_json::Value| {
        let v = value.as_array().unwrap();
        [v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32, v[2].as_f64().unwrap() as f32]
    };
    for definition in definitions {
        let name = definition.name.strip_prefix("hp_spatial_").unwrap();
        let plan = Arc::new(Plan::new(&definition));
        assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
        let mut sim = Sim::new(plan, 17);
        let mut points = Points::at([0.0, 0.0, 64.0]);
        for (frame, sample) in captures["cases"][name].as_array().unwrap().iter().enumerate() {
            let lo = vector(&sample["lo"]);
            let hi = vector(&sample["hi"]);
            if (0..3).all(|axis| lo[axis] <= hi[axis]) {
                sim.step_in(sample["dt"].as_f64().unwrap() as f32, &points, &[], 1, &NativeFloor);
                let expected = [0, 1, 2].map(|axis| {
                    if (lo[axis] - (points.get(0)[axis] - [0.0, 0.0, 64.0][axis])).abs() < 0.0001 {
                        hi[axis]
                    } else {
                        lo[axis]
                    }
                });
                let p = sim.particles[0].pos;
                let actual = [p[0], p[1], p[2] - 64.0];
                let error = actual.iter().zip(expected).map(|(a, b)| (a - b).powi(2)).sum::<f32>().sqrt();
                assert!(error < 0.01, "{name}, frame {frame}: actual {actual:?}, native {expected:?}, error {error}");
            }
            points = Points::with_angles(
                vector(&sample["cp"]["position"]),
                [0.0, sample["cp"]["yaw"].as_f64().unwrap() as f32, 0.0],
            );
        }
    }
}

#[test]
fn scalar_lifetimes_and_inherited_velocity_match_native_particle_samples() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/scalar_cases.pcf")).unwrap();
    let captures: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/scalar_native_samples.json")).unwrap();
    let frames = captures["cases"]["inherit_velocity"].as_array().unwrap();
    for definition in definitions {
        let name = definition.name.strip_prefix("hp_scalar_").unwrap();
        let plan = Arc::new(Plan::new(&definition));
        assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
        let mut sim = Sim::new(plan, 17);
        for (frame, timing) in frames.iter().enumerate().skip(1) {
            let mut points = Points::at([0.0, 0.0, 64.0]);
            points.pos.extend([
                [if name == "inherit_velocity" { (frame - 1) as f32 * 2.0 } else { 0.0 }, 0.0, 64.0],
                [0.25, 0.0, 0.0],
            ]);
            points.orientations = vec![points.axes, points.axes, Points::with_angles([0.0; 3], [0.0, 90.0, 0.0]).axes];
            sim.step_at(timing["dt"].as_f64().unwrap() as f32, &points, &[], 1);
            let native = captures["cases"][name].as_array().unwrap().get(frame);
            let native_alive = native.is_some_and(|sample| {
                sample["hi"][0].as_f64().unwrap() >= if name == "inherit_velocity" { 90.0 } else { 10.0 }
            });
            assert_eq!(!sim.particles.is_empty(), native_alive, "{name}, frame {frame}, time {}", sim.time());
            if native_alive {
                let expected = native.unwrap()["hi"][0].as_f64().unwrap() as f32;
                let actual = sim.particles[0].pos[0];
                assert!((actual - expected).abs() < 0.005, "{name}, frame {frame}: actual {actual}, native {expected}");
            }
        }
    }
}

#[test]
fn scalar_remaps_keep_initial_and_current_values_distinct() {
    for (mode, expected) in
        [("output is scalar of initial random range", 0.5), ("output is scalar of current value", 0.125)]
    {
        let def = system(
            "remap",
            vec![burst(1)],
            vec![],
            vec![op(
                "Remap Distance to Control Point to Scalar",
                vec![
                    ("output field", Value::Int(7)),
                    ("output minimum", fl(0.5)),
                    ("output maximum", fl(0.5)),
                    (mode, Value::Bool(true)),
                ],
            )],
        );
        let mut sim = Sim::new(Arc::new(Plan::new(&def)), 17);
        for _ in 0..3 {
            sim.step(0.05, [0.0; 3], 1);
        }
        assert!((sim.particles[0].alpha - expected).abs() < 1e-6, "{mode}: {}", sim.particles[0].alpha);
    }
    let def = system(
        "los",
        vec![burst(1)],
        vec![],
        vec![op("Remap Distance to Control Point to Scalar", vec![("ensure line of sight", Value::Bool(true))])],
    );
    assert_eq!(Plan::new(&def).unsupported, ["Remap Distance to Control Point to Scalar"]);
}

#[test]
fn distance_lock_matches_nine_native_offset_and_radius_combinations() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/distance_cases.pcf")).unwrap();
    let capture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/distance_native_samples.json")).unwrap();
    for definition in definitions {
        let name = definition.name.strip_prefix("hp_distance_").unwrap();
        let plan = Arc::new(Plan::new(&definition));
        assert!(plan.unsupported.is_empty());
        let mut sim = Sim::new(plan, 17);
        let mut points = Points::at([0.0, 0.0, 64.0]);
        for (frame, sample) in capture["cases"][name].as_array().unwrap().iter().enumerate() {
            if frame > 0 {
                sim.step_at(sample["dt"].as_f64().unwrap() as f32, &points, &[], 1);
                let expected = sample["hi"][0].as_f64().unwrap() as f32;
                assert!(
                    (sim.particles[0].pos[0] - expected).abs() < 0.0001,
                    "{name}, frame {frame}: actual {}, native {expected}",
                    sim.particles[0].pos[0]
                );
            }
            points.pos[0][0] = sample["cp"]["position"][0].as_f64().unwrap() as f32;
        }
    }
}

#[test]
fn child_control_points_match_native_initial_and_following_particles() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/child_cases.pcf")).unwrap();
    let plans: std::collections::HashMap<String, Arc<Plan>> =
        definitions.iter().map(|d| (d.name.clone(), Arc::new(Plan::new(d)))).collect();
    assert!(plans.values().all(|p| p.unsupported.is_empty()));
    let captures: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/child_native_samples.json")).unwrap();
    for name in ["initial", "follow", "five"] {
        let mut effect = Effect::new(&format!("hp_child_{name}"), &|name| plans.get(name).cloned(), 17).unwrap();
        for (frame, sample) in captures["cases"][name].as_array().unwrap().iter().enumerate().skip(1) {
            effect.step(sample["dt"].as_f64().unwrap() as f32, [0.0, 0.0, 64.0], 10);
            let expected = sample["hi"][0].as_f64().unwrap() as f32;
            let actual = effect.particles().map(|(_, p)| p.pos[0]).reduce(f32::max).unwrap();
            assert!((actual - expected).abs() < 0.002, "{name}, frame {frame}: actual {actual}, native {expected}");
        }
    }
}

#[test]
fn traced_control_points_propagate_through_child_generations() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/cpimpact_cases.pcf")).unwrap();
    let mut root = definitions.into_iter().find(|def| def.name == "hp_cpimpact_up").unwrap();
    let mut child = root.clone();
    child.name = "child".into();
    child.operators.retain(|op| op.function != "Set Control Point to Impact Point");
    let mut grandchild = child.clone();
    grandchild.name = "grandchild".into();
    child.children.push(Child { system: grandchild.name.clone(), delay: 0.0 });
    root.children.push(Child { system: child.name.clone(), delay: 0.0 });
    let plans: std::collections::HashMap<_, _> =
        [root, child, grandchild].into_iter().map(|def| (def.name.clone(), Arc::new(Plan::new(&def)))).collect();
    let mut effect = Effect::new("hp_cpimpact_up", &|name| plans.get(name).cloned(), 17).unwrap();
    let mut points = Points::at([0.0, 0.0, 64.0]);
    points.pos.push([0.0, 0.0, 64.0]);
    for frame in 0..12 {
        if frame == 6 {
            points.pos[1][2] = 96.0;
        }
        effect.step_at(1.0 / 30.0, &points, 10);
        let particles: Vec<_> = effect.particles().collect();
        assert_eq!(particles.len(), 3);
        let expected = if frame < 6 { 192.0 } else { 224.0 };
        assert!((particles[0].1.pos[2] - expected).abs() < 0.001);
        for (_, particle) in &particles[1..] {
            assert_eq!(particle.pos, particles[0].1.pos);
        }
    }
}

#[test]
fn particle_radii_extend_rays_and_preserve_native_contact_damping() {
    struct NativeFloor;
    impl crate::environment::Environment for NativeFloor {
        fn trace(&self, start: [f32; 3], end: [f32; 3], radius: f32, _: bool) -> Option<crate::environment::Trace> {
            assert_eq!(radius, 0.0);
            (end[2] < 0.0).then(|| crate::environment::Trace {
                fraction: ((start[2] - 0.03125) / (start[2] - end[2])).clamp(0.0, 1.0),
                normal: [0.0, 0.0, 1.0],
                start_solid: false,
            })
        }
    }
    let definitions = crate::parse_file(include_bytes!("../tests/data/radius_cases.pcf")).unwrap();
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/radius_native_samples.json")).unwrap();
    for def in definitions {
        let name = def.name.strip_prefix("hp_radius_").unwrap();
        let mut sim = Sim::new(Arc::new(Plan::new(&def)), 17);
        for (index, sample) in data["cases"][name].as_array().unwrap().iter().enumerate().skip(1) {
            sim.step_in(sample["dt"].as_f64().unwrap() as f32, &Points::at([0.0, 0.0, 64.0]), &[], 1, &NativeFloor);
            let expected = sample["lo"][2].as_f64().unwrap() as f32 + 64.0;
            let actual = sim.particles[0].pos[2];
            assert!((actual - expected).abs() < 0.00001, "{name}, frame {index}: actual {actual}, native {expected}");
        }
    }
}

#[test]
fn impact_lifetimes_match_native_hits_misses_biases_and_stationary_particles() {
    struct NativeFloor;
    impl crate::environment::Environment for NativeFloor {
        fn trace(&self, start: [f32; 3], end: [f32; 3], radius: f32, _: bool) -> Option<crate::environment::Trace> {
            assert_eq!(radius, 0.0);
            (start[2] >= 0.0 && end[2] < 0.0).then(|| crate::environment::Trace {
                fraction: ((start[2] - 0.03125) / (start[2] - end[2])).clamp(0.0, 1.0),
                normal: [0.0, 0.0, 1.0],
                start_solid: false,
            })
        }
    }
    let definitions = crate::parse_file(include_bytes!("../tests/data/impact_cases.pcf")).unwrap();
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/impact_native_samples.json")).unwrap();
    let frames = data["cases"]["upward"].as_array().unwrap();
    for def in definitions {
        let name = def.name.strip_prefix("hp_impact_").unwrap();
        let plan = Arc::new(Plan::new(&def));
        assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
        let mut sim = Sim::new(plan, 17);
        for (frame, sample) in frames.iter().enumerate().skip(1) {
            sim.step_in(sample["dt"].as_f64().unwrap() as f32, &Points::at([0.0, 0.0, 64.0]), &[], 1, &NativeFloor);
            let alive = frame < data["cases"][name].as_array().unwrap().len();
            assert_eq!(!sim.particles.is_empty(), alive, "{name}, frame {frame}, time {}", sim.time());
        }
    }
}

#[test]
fn traced_control_points_match_native_offsets_and_update_intervals() {
    struct NativeWorld;
    impl crate::environment::Environment for NativeWorld {
        fn trace(&self, start: [f32; 3], end: [f32; 3], radius: f32, _: bool) -> Option<crate::environment::Trace> {
            assert_eq!(radius, 0.0);
            let mut best: Option<crate::environment::Trace> = None;
            for (axis, plane, normal) in [(2, 0.0, [0.0, 0.0, 1.0]), (0, 1024.0, [-1.0, 0.0, 0.0])] {
                let crossing = if axis == 2 {
                    start[axis] >= plane && end[axis] < plane
                } else {
                    start[axis] <= plane && end[axis] > plane
                };
                if !crossing {
                    continue;
                }
                let approach = (start[axis] - end[axis]).abs();
                let fraction = ((plane - start[axis]) / (end[axis] - start[axis]) - 0.03125 / approach).clamp(0.0, 1.0);
                if best.is_none_or(|old| fraction < old.fraction) {
                    best = Some(crate::environment::Trace { fraction, normal, start_solid: false });
                }
            }
            best
        }
    }
    let definitions = crate::parse_file(include_bytes!("../tests/data/cpimpact_cases.pcf")).unwrap();
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/cpimpact_native_samples.json")).unwrap();
    for def in definitions {
        let name = def.name.strip_prefix("hp_cpimpact_").unwrap();
        let plan = Arc::new(Plan::new(&def));
        assert!(plan.unsupported.is_empty());
        let mut sim = Sim::new(plan, 17);
        for (frame, sample) in data["cases"][name].as_array().unwrap().iter().enumerate().skip(1) {
            let mut points = Points::at([0.0, 0.0, 64.0]);
            points.pos.push([0.0, 0.0, 64.0 + (frame - 1) as f32 * 2.0]);
            sim.step_in(sample["dt"].as_f64().unwrap() as f32, &points, &[], 1, &NativeWorld);
            let expected =
                [sample["hi"][0].as_f64().unwrap() as f32, 0.0, sample["lo"][2].as_f64().unwrap() as f32 + 64.0];
            let actual = sim.particles[0].pos;
            let error = actual.iter().zip(expected).map(|(a, b)| (a - b).powi(2)).sum::<f32>().sqrt();
            assert!(error < 0.001, "{name}, frame {frame}: actual {actual:?}, native {expected:?}");
        }
    }
}

#[test]
fn local_speeds_turn_with_the_angles() {
    let def = system(
        "s",
        vec![burst(20)],
        vec![op(
            "Position Within Sphere Random",
            vec![
                ("speed_in_local_coordinate_system_min", Value::Vec3([100.0, 0.0, 0.0])),
                ("speed_in_local_coordinate_system_max", Value::Vec3([100.0, 0.0, 0.0])),
            ],
        )],
        vec![op("Movement Basic", vec![])],
    );
    let sim = run(&def, &Points::with_angles([0.0; 3], [0.0, 90.0, 0.0]), 0.5);
    assert_eq!(sim.particles.len(), 20);
    for p in &sim.particles {
        assert!(p.vel[1] > 99.0 && p.vel[0].abs() < 1e-3, "{:?}", p.vel);
        assert!(p.pos[1] > 30.0);
    }
    let down = Points::with_angles([0.0; 3], [90.0, 0.0, 0.0]);
    assert!((down.axes[0][2] + 1.0).abs() < 1e-5);
    assert!((Points::at([0.0; 3]).axes[1][1] - 1.0).abs() < 1e-5);
}

#[test]
fn rotation_spin_sequence_and_flip() {
    let def = system(
        "s",
        vec![burst(50)],
        vec![
            op("Rotation Random", vec![("rotation_offset_min", fl(10.0)), ("rotation_offset_max", fl(20.0))]),
            op("Sequence Random", vec![("sequence_min", Value::Int(2)), ("sequence_max", Value::Int(4))]),
            op("Rotation Yaw Flip Random", vec![("Flip Percentage", fl(0.5))]),
        ],
        vec![op("Rotation Spin Roll", vec![("spin_rate_degrees", Value::Int(90))])],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 1.0);
    let mut flipped = 0;
    for p in &sim.particles {
        let deg = p.rotation.to_degrees();
        assert!(deg > 95.0 && deg < 115.0, "{deg}");
        assert!((2..=4).contains(&p.sequence));
        flipped += usize::from(p.yaw.cos() < 0.0);
    }
    assert!(flipped > 5 && flipped < 45, "{flipped}");
}

#[test]
fn radius_scale_windows_keep_their_last_value() {
    let def = system(
        "s",
        vec![burst(1)],
        vec![op("Lifetime Random", vec![("lifetime_min", fl(10.0)), ("lifetime_max", fl(10.0))])],
        vec![
            op(
                "Radius Scale",
                vec![
                    ("start_time", fl(0.0)),
                    ("end_time", fl(0.2)),
                    ("radius_start_scale", fl(1.0)),
                    ("radius_end_scale", fl(2.0)),
                ],
            ),
            op(
                "Radius Scale",
                vec![
                    ("start_time", fl(0.5)),
                    ("end_time", fl(1.0)),
                    ("radius_start_scale", fl(2.0)),
                    ("radius_end_scale", fl(4.0)),
                ],
            ),
        ],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 3.3);
    assert!((sim.particles[0].radius - 10.0).abs() < 0.3, "{}", sim.particles[0].radius);
    let sim = run(&def, &Points::at([0.0; 3]), 7.5);
    assert!((sim.particles[0].radius - 15.0).abs() < 0.5, "{}", sim.particles[0].radius);
}

#[test]
fn fades_oscillations_and_ramps() {
    let def = system(
        "s",
        vec![burst(10)],
        vec![op("Lifetime Random", vec![("lifetime_min", fl(2.0)), ("lifetime_max", fl(2.0))])],
        vec![
            op("Alpha Fade In Simple", vec![("proportional fade in time", fl(0.5))]),
            op("Alpha Fade Out Simple", vec![("proportional fade out time", fl(0.25))]),
            op(
                "Oscillate Scalar",
                vec![
                    ("oscillation field", Value::Int(3)),
                    ("oscillation rate min", fl(20.0)),
                    ("oscillation rate max", fl(20.0)),
                    ("oscillation frequency min", fl(0.25)),
                    ("oscillation frequency max", fl(0.25)),
                ],
            ),
            op(
                "Ramp Scalar Linear Simple",
                vec![("ramp field", Value::Int(4)), ("ramp rate", fl(2.0)), ("end time", fl(1.0))],
            ),
            op("Lifespan Decay", vec![]),
        ],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 0.25);
    let p = sim.particles[0];
    assert!((p.alpha - 0.25).abs() < 0.06, "{}", p.alpha);
    let sim = run(&def, &Points::at([0.0; 3]), 1.0);
    let p = sim.particles[0];
    assert!((p.alpha - 1.0).abs() < 1e-3);
    assert!((p.radius - 5.0).abs() > 0.5, "the radius oscillates: {}", p.radius);
    assert!((p.rotation - 2.0).abs() < 0.15, "ramped: {}", p.rotation);
    let sim = run(&def, &Points::at([0.0; 3]), 1.9);
    assert!(sim.particles[0].alpha < 0.3);
    let sim = run(&def, &Points::at([0.0; 3]), 2.2);
    assert!(sim.particles.is_empty());
}

#[test]
fn paths_rings_constraints_and_control_points() {
    let points = Points { pos: vec![[0.0; 3], [200.0, 0.0, 0.0]], ..Points::at([0.0; 3]) };
    let path = system(
        "s",
        vec![burst(40)],
        vec![op(
            "Position Along Path Sequential",
            vec![
                ("start control point number", Value::Int(0)),
                ("end control point number", Value::Int(1)),
                ("particles to map from start to end", Value::Int(40)),
            ],
        )],
        vec![],
    );
    let sim = run(&path, &points, 0.05);
    let xs: Vec<f32> = sim.particles.iter().map(|p| p.pos[0]).collect();
    assert!(xs.iter().all(|x| (0.0..=200.0).contains(x)));
    assert!(xs.windows(2).all(|w| w[1] > w[0]), "start to end in order");
    let ring = system(
        "s",
        vec![burst(30)],
        vec![op("Position Along Ring", vec![("initial radius", fl(50.0))])],
        vec![op("Constrain distance to control point", vec![("maximum distance", fl(30.0))])],
    );
    let sim = run(&ring, &Points::at([0.0; 3]), 0.05);
    for p in &sim.particles {
        let d = (p.pos[0] * p.pos[0] + p.pos[1] * p.pos[1] + p.pos[2] * p.pos[2]).sqrt();
        assert!((d - 30.0).abs() < 0.1, "constrained to 30: {d}");
        assert!(p.pos[2].abs() < 1e-3);
    }
    assert_eq!(points.get(7), [0.0; 3]);
    assert_eq!(points.get(-1), [0.0; 3]);
}

#[test]
fn children_start_on_the_particles_of_their_parent() {
    let parent = SystemDef {
        children: vec![Child { system: "child".into(), delay: 0.0 }],
        ..system(
            "parent",
            vec![burst(5)],
            vec![op(
                "Position Within Box Random",
                vec![("min", Value::Vec3([500.0, 0.0, 0.0])), ("max", Value::Vec3([500.0, 0.0, 0.0]))],
            )],
            vec![],
        )
    };
    let child = system(
        "child",
        vec![op("emit_continuously", vec![("emission_rate", fl(100.0))])],
        vec![op("Position From Parent Particles", vec![])],
        vec![],
    );
    let plans = [Arc::new(Plan::new(&parent)), Arc::new(Plan::new(&child))];
    let lookup = |n: &str| plans.iter().find(|p| p.name.eq_ignore_ascii_case(n)).cloned();
    let mut fx = Effect::new("parent", &lookup, 1).expect("effect");
    assert!(fx.missing.is_empty());
    for _ in 0..10 {
        fx.step_at(0.05, &Points::at([0.0; 3]), 500);
    }
    let kids = &fx.sims[1].1.particles;
    assert!(!kids.is_empty());
    assert!(kids.iter().all(|p| (p.pos[0] - 500.0).abs() < 1e-3));
    let lonely = SystemDef {
        children: vec![Child { system: "Nobody".into(), delay: 0.0 }],
        ..system("x", vec![burst(1)], vec![], vec![])
    };
    let plan = Arc::new(Plan::new(&lonely));
    let only = |n: &str| (n == "x").then(|| plan.clone());
    assert_eq!(Effect::new("x", &only, 1).expect("fx").missing, vec!["Nobody".to_owned()]);
}

#[test]
fn renderers_lifetimes_from_sequences_and_animation_cycles() {
    let mut trail = system("t", vec![burst(1)], vec![], vec![]);
    trail.renderers = vec![op("render_sprite_trail", vec![("max length", fl(64.0)), ("min length", fl(2.0))])];
    let plan = Plan::new(&trail);
    assert_eq!(plan.render.kind, RenderKind::Trail);
    assert_eq!((plan.render.min_len, plan.render.max_len), (2.0, 64.0));
    let mut rope = system("r", vec![burst(1)], vec![], vec![]);
    rope.renderers = vec![op("render_rope", vec![])];
    assert_eq!(Plan::new(&rope).render.kind, RenderKind::Rope);
    let mut models = system("m", vec![burst(1)], vec![], vec![]);
    models.renderers = vec![op("render_models", vec![])];
    let plan = Plan::new(&models);
    assert!(!plan.draws);
    assert_eq!(plan.unsupported, vec!["render_models".to_owned()]);
    let seq = system(
        "q",
        vec![burst(4)],
        vec![
            op("Sequence Random", vec![("sequence_min", Value::Int(1)), ("sequence_max", Value::Int(1))]),
            op("lifetime from sequence", vec![("Frames Per Second", fl(10.0))]),
        ],
        vec![],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&seq)), 1);
    sim.set_sequence_frames(vec![4, 25]);
    sim.step(0.01, [0.0; 3], 10);
    assert!(sim.particles.iter().all(|p| (p.life - 2.5).abs() < 1e-4));
    let mut fps = system("f", vec![burst(1)], vec![], vec![]);
    fps.renderers = vec![op(
        "render_animated_sprites",
        vec![("animation rate", fl(20.0)), ("use animation rate as FPS", Value::Bool(true))],
    )];
    let plan = Plan::new(&fps);
    let mut p = sim.particles[0];
    p.age = 1.0;
    assert!((plan.cycle(&p, 40.0) - 0.5).abs() < 1e-5);
    let mut fit = system("g", vec![burst(1)], vec![], vec![]);
    fit.renderers = vec![op("render_animated_sprites", vec![("animation_fit_lifetime", Value::Bool(true))])];
    assert!((Plan::new(&fit).cycle(&p, 40.0) - 0.1).abs() < 1e-5, "the native sprite renderer ignores fit-lifetime");
    assert!((Plan::new(&seq).cycle(&p, 40.0) - 0.1).abs() < 1e-5, "default rate 0.1");
}

#[test]
fn noise_emitters_remaps_and_warm_up() {
    let def = system(
        "n",
        vec![op("emit noise", vec![("emission minimum", fl(10.0)), ("emission maximum", fl(30.0))])],
        vec![
            op("Lifetime Random", vec![("lifetime_min", fl(100.0)), ("lifetime_max", fl(100.0))]),
            op(
                "remap initial scalar",
                vec![
                    ("input field", Value::Int(1)),
                    ("input minimum", fl(0.0)),
                    ("input maximum", fl(200.0)),
                    ("output field", Value::Int(3)),
                    ("output minimum", fl(0.0)),
                    ("output maximum", fl(40.0)),
                ],
            ),
            op(
                "Remap Noise to Scalar",
                vec![("output field", Value::Int(7)), ("output minimum", fl(0.2)), ("output maximum", fl(0.4))],
            ),
        ],
        vec![],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 2.0);
    let n = sim.particles.len();
    assert!((20..=60).contains(&n), "between 10 and 30 a second: {n}");
    for p in &sim.particles {
        assert!((p.radius - 20.0).abs() < 1e-3, "life 100 maps to radius 20: {}", p.radius);
        assert!(p.alpha >= 0.2 - 1e-4 && p.alpha <= 0.4 + 1e-4);
    }
    let fx = Effect::new("n", &|_| Some(Arc::new(Plan::new(&def))), 1).expect("fx");
    assert!(fx.continuous());
    let once =
        Effect::new("b", &|_| Some(Arc::new(Plan::new(&system("b", vec![burst(2)], vec![], vec![])))), 1).expect("fx");
    assert!(!once.continuous());
}

type RawSequence<'a> = (u32, bool, &'a [(f32, [f32; 4])]);

fn sheet_bytes(version: u32, sequences: &[RawSequence<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&(sequences.len() as u32).to_le_bytes());
    for (number, clamp, frames) in sequences {
        out.extend_from_slice(&number.to_le_bytes());
        out.extend_from_slice(&u32::from(*clamp).to_le_bytes());
        out.extend_from_slice(&(frames.len() as u32).to_le_bytes());
        out.extend_from_slice(&(frames.len() as f32).to_le_bytes());
        for (duration, uv) in *frames {
            out.extend_from_slice(&duration.to_le_bytes());
            for _ in 0..if version > 0 { 4 } else { 1 } {
                for c in uv {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
    }
    out
}

#[test]
fn sprite_sheet_frames() {
    let a = [0.0, 0.0, 0.5, 0.5];
    let b = [0.5, 0.0, 1.0, 0.5];
    let c = [0.0, 0.5, 0.5, 1.0];
    let bytes = sheet_bytes(0, &[(0, false, &[(1.0, a), (1.0, b)]), (3, true, &[(1.0, a), (3.0, c)])]);
    let sheet = parse_sheet(&bytes).expect("sheet");
    assert_eq!(sheet.sequences.len(), 2);
    assert_eq!(sheet.frame_uv(0, 0.25), a);
    assert_eq!(sheet.frame_uv(0, 0.75), b);
    assert_eq!(sheet.frame_uv(0, 1.25), a);
    assert_eq!(sheet.frame_uv(3, 0.2), a);
    assert_eq!(sheet.frame_uv(3, 0.3), c, "durations weigh the frames");
    assert_eq!(sheet.frame_uv(3, 7.0), c);
    assert_eq!(sheet.frame_count(5), 2);
    assert_eq!(sheet.frame_uv(0, f32::NAN), a);
    assert_eq!(Sheet::default().frame_uv(0, 0.5), FULL_UV);
    let v1 = parse_sheet(&sheet_bytes(1, &[(0, false, &[(1.0, b)])])).expect("v1");
    assert_eq!(v1.frame_uv(0, 0.5), b);
    for n in 0..bytes.len() {
        let _ = parse_sheet(&bytes[..n]);
    }
    let mut huge = bytes.clone();
    huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(parse_sheet(&huge).is_none());
    let mut vtf = vec![0u8; 88];
    vtf[..4].copy_from_slice(b"VTF\0");
    vtf[4..8].copy_from_slice(&7u32.to_le_bytes());
    vtf[8..12].copy_from_slice(&3u32.to_le_bytes());
    vtf[68..72].copy_from_slice(&1u32.to_le_bytes());
    vtf[80..84].copy_from_slice(&[0x10, 0, 0, 0]);
    vtf[84..88].copy_from_slice(&88u32.to_le_bytes());
    vtf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    vtf.extend_from_slice(&bytes);
    assert_eq!(sheet_from_vtf(&vtf), Some(sheet));
    for n in 0..vtf.len() {
        let _ = sheet_from_vtf(&vtf[..n]);
    }
    vtf[8..12].copy_from_slice(&2u32.to_le_bytes());
    assert!(sheet_from_vtf(&vtf).is_none());
    assert!(sheet_from_vtf(b"nope").is_none());
}

#[test]
fn real_sprite_sheets() {
    let dir = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod/sourceengine");
    if !dir.is_dir() {
        return;
    }
    let loose = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod/materials/particle");
    let Ok(entries) = std::fs::read_dir(loose) else {
        return;
    };
    let mut found = 0;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("vtf"))
            && let Ok(bytes) = std::fs::read(&p)
            && let Some(sheet) = sheet_from_vtf(&bytes)
        {
            found += 1;
            assert!(sheet.sequences.iter().all(|s| !s.frames.is_empty()));
        }
    }
    eprintln!("{found} loose particle textures with a sheet");
}

#[test]
fn pre_aged_particles_start_part_way_through_their_life() {
    let def = system(
        "p",
        vec![burst(3)],
        vec![
            op("Lifetime Pre-Age Noise", vec![("start age minimum", fl(0.5)), ("start age maximum", fl(0.5))]),
            op("Lifetime Random", vec![("lifetime_min", fl(2.0)), ("lifetime_max", fl(2.0))]),
        ],
        vec![op("Lifespan Decay", vec![])],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&def)), 1);
    sim.step(0.01, [0.0; 3], 10);
    assert!(sim.particles.iter().all(|p| (p.age - 1.01).abs() < 1e-4), "{:?}", sim.particles[0].age);
    assert!(Plan::new(&def).unsupported.is_empty());
}

#[test]
fn random_fades_and_color_fades_ease_in_and_out_by_default() {
    let fade = |ease: Option<bool>| {
        let mut attrs = vec![
            ("fade in time min", fl(1.0)),
            ("fade in time max", fl(1.0)),
            ("proportional 0/1", Value::Bool(false)),
        ];
        if let Some(e) = ease {
            attrs.push(("ease in and out", Value::Bool(e)));
        }
        system(
            "s",
            vec![burst(4)],
            vec![op("Lifetime Random", vec![("lifetime_min", fl(4.0)), ("lifetime_max", fl(4.0))])],
            vec![op("Alpha Fade In Random", attrs), op("Lifespan Decay", vec![])],
        )
    };
    let quarter = |ease| run(&fade(ease), &Points::at([0.0; 3]), 0.25).particles[0].alpha;
    assert!((quarter(Some(false)) - 0.25).abs() < 0.03, "linear");
    assert!((quarter(None) - 0.156).abs() < 0.03, "eased by default: {}", quarter(None));
    assert!((quarter(Some(true)) - quarter(None)).abs() < 1e-4);
    let def = system(
        "c",
        vec![burst(2)],
        vec![
            op("Lifetime Random", vec![("lifetime_min", fl(2.0)), ("lifetime_max", fl(2.0))]),
            op(
                "Color Random",
                vec![("color1", Value::Color([0, 0, 0, 255])), ("color2", Value::Color([0, 0, 0, 255]))],
            ),
        ],
        vec![op("Color Fade", vec![("color_fade", Value::Color([255, 255, 255, 255]))]), op("Lifespan Decay", vec![])],
    );
    let quarter_color = run(&def, &Points::at([0.0; 3]), 0.5).particles[0].color[0];
    assert!((quarter_color - 0.156).abs() < 0.04, "eased: {quarter_color}");
}

#[test]
fn frame_blend_gives_both_frames_and_the_fraction() {
    use crate::sheet::{SheetFrame, SheetSequence};
    let frame = |x: f32| SheetFrame { duration: 1.0, uv: [x, 0.0, x + 0.25, 1.0] };
    let looping = Sheet {
        sequences: vec![SheetSequence {
            number: 0,
            clamp: false,
            frames: vec![frame(0.0), frame(0.25), frame(0.5)],
            total: 3.0,
        }],
    };
    let (a, b, f) = looping.frame_blend(0, 0.5);
    assert_eq!(a, looping.frame_uv(0, 0.5));
    assert_eq!(b[0], 0.5);
    assert!((f - 0.5).abs() < 1e-4, "{f}");
    let (_, wrapped, _) = looping.frame_blend(0, 0.9);
    assert_eq!(wrapped[0], 0.0, "a looping sequence blends the last frame into the first");
    let clamped = Sheet { sequences: vec![SheetSequence { clamp: true, ..looping.sequences[0].clone() }] };
    let (last, same, f) = clamped.frame_blend(0, 1.0);
    assert_eq!(last, same);
    assert_eq!(f, 0.0);
}

#[test]
fn second_sequences_and_field_thirteen_do_not_overwrite_the_first_sequence() {
    let mut def = system(
        "two sequences",
        vec![burst(1)],
        vec![
            op("Sequence Random", vec![("sequence_min", Value::Int(3)), ("sequence_max", Value::Int(3))]),
            op("Sequence Two Random", vec![("sequence_min", Value::Int(7)), ("sequence_max", Value::Int(7))]),
        ],
        vec![],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 0.05);
    assert!(sim.plan().unsupported.is_empty());
    assert_eq!((sim.particles[0].sequence, sim.particles[0].sequence2), (3, 7));
    def.initializers.push(op(
        "Remap Control Point to Scalar",
        vec![("output field", Value::Int(13)), ("output minimum", fl(11.0)), ("output maximum", fl(11.0))],
    ));
    let sim = run(&def, &Points::at([0.0; 3]), 0.05);
    assert_eq!((sim.particles[0].sequence, sim.particles[0].sequence2), (3, 11));
}

#[test]
fn trace_requirements_distinguish_brush_only_traces_from_prop_collisions() {
    let mut def = system("requirements", vec![burst(1)], vec![], vec![op("Movement Basic", vec![])]);
    assert_eq!(Plan::new(&def).trace_requirement(), None);
    def.constraints.push(op("Collision via traces", vec![("brush only", Value::Bool(true))]));
    assert_eq!(Plan::new(&def).trace_requirement(), Some(false));
    def.constraints.push(op("Collision via traces", vec![]));
    assert_eq!(Plan::new(&def).trace_requirement(), Some(true));
    def.constraints.clear();
    def.operators.push(op("Set Control Point to Impact Point", vec![]));
    assert_eq!(Plan::new(&def).trace_requirement(), Some(true));
}

#[test]
fn sprite_animation_clocks_match_native_rendered_pixels_including_lifetime_and_fps_options() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/spritecard_native_samples.json")).unwrap();
    let srgb = |value: f32| {
        let value = if value <= 0.0031308 { value * 12.92 } else { 1.055 * value.powf(1.0 / 2.4) - 0.055 };
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    for case in
        cases.into_iter().filter(|case| case["sequence"] == 2 && case["dual"] == false && case["maximum"][0] == false)
    {
        let mut def = system("clock", vec![burst(1)], vec![], vec![]);
        def.renderers = vec![op(
            "render_animated_sprites",
            vec![
                ("animation rate", fl(case["rate"].as_f64().unwrap() as f32)),
                ("second sequence animation rate", fl(case["rate2"].as_f64().unwrap() as f32)),
                ("animation_fit_lifetime", Value::Bool(case["fit"].as_bool().unwrap())),
                ("use animation rate as FPS", Value::Bool(case["fps"].as_bool().unwrap())),
            ],
        )];
        let plan = Arc::new(Plan::new(&def));
        let mut sim = Sim::new(Arc::clone(&plan), 17);
        sim.step(0.0, [0.0; 3], 1);
        let mut particle = sim.particles[0];
        particle.life = case["life"].as_f64().unwrap() as f32;
        for sample in case["samples"].as_array().unwrap() {
            particle.age = sample["age"].as_f64().unwrap() as f32;
            let frame = plan.cycle(&particle, 2.0).rem_euclid(1.0) * 2.0;
            let blue = if frame < 1.0 { frame } else { 2.0 - frame };
            let alpha = (64.0 + 128.0 * blue) / 255.0;
            let actual = [srgb((1.0 - blue) * alpha), 0, srgb(blue * alpha)];
            for (channel, value) in actual.into_iter().enumerate() {
                let expected = sample["pixel"][channel].as_u64().unwrap() as u8;
                assert!(
                    value.abs_diff(expected) <= 3,
                    "{} age {}: {actual:?} versus {}",
                    case["name"],
                    particle.age,
                    sample["pixel"]
                );
            }
        }
    }
}

#[test]
fn imported_numeric_types_are_not_coerced_into_native_boolean_or_integer_fields() {
    let wrong = op("render_animated_sprites", vec![("use animation rate as FPS", Value::Int(1))]);
    assert_eq!(wrong.bool("use animation rate as FPS"), None);
    let correct = op("render_animated_sprites", vec![("use animation rate as FPS", Value::Bool(true))]);
    assert_eq!(correct.bool("use animation rate as FPS"), Some(true));
    let wrong = op("Rotation Spin Roll", vec![("spin_rate_degrees", Value::Float(90.0))]);
    assert_eq!(wrong.float("spin_rate_degrees"), None);
    let correct = op("Rotation Spin Roll", vec![("spin_rate_degrees", Value::Int(90))]);
    assert_eq!(correct.float("spin_rate_degrees"), Some(90.0));
}

#[test]
fn yaw_flipping_adds_half_a_turn_without_losing_the_existing_tilt() {
    for (chance, expected) in
        [(0.0, std::f32::consts::FRAC_PI_4), (1.0, std::f32::consts::FRAC_PI_4 + std::f32::consts::PI)]
    {
        let def = system(
            "yaw",
            vec![burst(1)],
            vec![
                op("Rotation Yaw Random", vec![("yaw_offset_min", fl(45.0)), ("yaw_offset_max", fl(45.0))]),
                op("Rotation Yaw Flip Random", vec![("Flip Percentage", fl(chance))]),
            ],
            vec![],
        );
        let sim = run(&def, &Points::at([0.0; 3]), 0.0);
        assert!((sim.particles[0].yaw - expected).abs() < 0.00001);
        assert!(!sim.particles[0].flip);
    }
}

#[test]
fn yaw_spinning_uses_its_native_rate_keys_without_changing_roll() {
    let def = system(
        "yaw",
        vec![burst(1)],
        vec![],
        vec![op("Rotation Spin Yaw", vec![("yaw_rate_degrees", Value::Int(90))])],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 0.5);
    assert!((sim.particles[0].yaw - std::f32::consts::FRAC_PI_4).abs() < 0.00001);
    assert_eq!(sim.particles[0].rotation, 0.0);
}

#[test]
fn color_lighting_matches_native_pixels_for_each_tint_and_blend_mode() {
    struct Lighting {
        particle: [f32; 3],
        controls: [[f32; 3]; 2],
        origin: [f32; 3],
    }
    impl crate::environment::Environment for Lighting {
        fn lighting(&self, position: [f32; 3]) -> [f32; 3] {
            if position[0] < self.origin[0] - 100.0 {
                self.controls[1]
            } else if position[0] > self.origin[0] + 5.0 {
                self.particle
            } else {
                self.controls[0]
            }
        }
    }
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/color_light_native_samples.json")).unwrap();
    let read = |value: &serde_json::Value| std::array::from_fn(|axis| value[axis].as_f64().unwrap() as f32);
    let world = Lighting {
        particle: read(&reference["lighting"]),
        controls: [read(&reference["control_lighting"][0]), read(&reference["control_lighting"][1])],
        origin: read(&reference["origin"]),
    };
    let mut points = Points::at(world.origin);
    points.pos.push([world.origin[0] - 256.0, world.origin[1], world.origin[2]]);
    let definitions = crate::parse_file(include_bytes!("../tests/data/color_light_cases.pcf")).unwrap();
    for case in reference["cases"].as_array().unwrap() {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let plan = Arc::new(Plan::new(definitions.iter().find(|def| def.name == name).unwrap()));
        assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
        let mut sim = Sim::new(plan, 17);
        sim.step_in(0.0, &points, &[], 1, &world);
        let actual = gray_sprite_pixel(sim.particles[0].color);
        for (channel, value) in actual.into_iter().enumerate() {
            let expected = case["pixel"][channel].as_u64().unwrap() as u8;
            assert!(value.abs_diff(expected) <= 1, "{name}: {actual:?} vs {}", case["pixel"]);
        }
    }
}

fn gray_sprite_pixel(color: [f32; 3]) -> [u8; 3] {
    // Native SpriteCard vertex colors use gamma 2.2; the texture uses sRGB.
    let texture = ((128.0f32 / 255.0 + 0.055) / 1.055).powf(2.4);
    color.map(|channel| {
        let linear = channel.max(0.0).powf(2.2) * texture * (128.0 / 255.0);
        let encoded = if linear <= 0.0031308 { linear * 12.92 } else { 1.055 * linear.powf(1.0 / 2.4) - 0.055 };
        (encoded.clamp(0.0, 1.0) * 255.0).round() as u8
    })
}

#[test]
fn color_fades_match_native_pixels_at_measured_particle_ages() {
    let reference: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/color_fade_native_samples.json")).unwrap();
    let definitions = crate::parse_file(include_bytes!("../tests/data/color_fade_cases.pcf")).unwrap();
    for case in reference {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let plan = Arc::new(Plan::new(definitions.iter().find(|def| def.name == name).unwrap()));
        assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
        let mut sim = Sim::new(plan, 17);
        sim.step(0.0, [0.0; 3], 1);
        for sample in case["samples"].as_array().unwrap() {
            let age = sample["age"].as_f64().unwrap() as f32;
            while age - sim.particles[0].age > 0.000001 {
                sim.step((age - sim.particles[0].age).min(0.05), [0.0; 3], 1);
            }
            let actual = gray_sprite_pixel(sim.particles[0].color);
            for (channel, value) in actual.into_iter().enumerate() {
                let expected = sample["pixel"][channel].as_u64().unwrap() as u8;
                assert!(value.abs_diff(expected) <= 1, "{name} age {age}: {actual:?} vs {}", sample["pixel"]);
            }
        }
    }
}

#[test]
fn native_particle_caps_and_sequential_float_steps_are_preserved() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/cap_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/cap_native_samples.json")).unwrap();
    let mut points = Points::at([0.0, 0.0, 64.0]);
    points.pos.push([10000.0, 0.0, 64.0]);
    for case in cases {
        let count = case["count"].as_u64().unwrap() as usize;
        let name = format!("hp_spritecard_cap{count}");
        let plan = Arc::new(Plan::new(definitions.iter().find(|d| d.name == name).unwrap()));
        assert!(plan.unsupported.is_empty());
        let mut sim = Sim::new(plan, 17);
        sim.step_at(0.0, &points, &[], 10000);
        assert_eq!(sim.particles.len(), count.min(5000));
        let maximum = sim.particles.iter().map(|p| p.pos[0]).max_by(f32::total_cmp).unwrap();
        let expected = case["maximum"].as_f64().unwrap() as f32;
        assert!((maximum - expected).abs() < 0.001, "{name}: {maximum} vs {expected}");
        sim.restart();
        sim.step_at(0.0, &points, &[], 10000);
        assert_eq!(sim.particles[0].pos[0], 0.0);
    }
}

#[test]
fn replacing_the_lighting_context_refreshes_new_particles_without_restarting_existing_ones() {
    struct Lighting(std::cell::Cell<f32>);
    impl crate::environment::Environment for Lighting {
        fn lighting(&self, _: [f32; 3]) -> [f32; 3] {
            [self.0.get(); 3]
        }
    }
    let def = system(
        "color cache",
        vec![op("emit_continuously", vec![("emission_rate", fl(4.0))])],
        vec![op("Color Random", vec![("tint_perc", fl(1.0))])],
        vec![],
    );
    let world = Lighting(std::cell::Cell::new(0.0));
    let points = Points::at([0.0; 3]);
    let mut sim = Sim::new(Arc::new(Plan::new(&def)), 17);
    for _ in 0..3 {
        sim.step_in(0.1, &points, &[], 20, &world);
    }
    world.0.set(1.0);
    for _ in 0..3 {
        sim.step_in(0.1, &points, &[], 20, &world);
    }
    assert_eq!(sim.particles.len(), 2);
    assert!(sim.particles.iter().all(|p| p.color == [0.0; 3]));
    let age = sim.particles[0].age;
    sim.invalidate_lighting();
    for _ in 0..3 {
        sim.step_in(0.1, &points, &[], 20, &world);
    }
    assert_eq!(sim.particles.len(), 3);
    assert_eq!(sim.particles[2].color, [1.0; 3]);
    assert_eq!(sim.particles[0].color, [0.0; 3]);
    assert!(sim.particles[0].age > age);
}

#[test]
fn sequential_path_edge_cases_match_native_zero_fractional_and_bouncing_bounds() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/path_edge_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/path_edge_native_samples.json")).unwrap();
    let mut points = Points::at([0.0, 0.0, 64.0]);
    points.pos.push([10000.0, 0.0, 64.0]);
    for case in cases {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let mut sim = Sim::new(Arc::new(Plan::new(definitions.iter().find(|d| d.name == name).unwrap())), 17);
        sim.step_at(0.0, &points, &[], 100);
        assert_eq!(sim.particles.len(), 8);
        let actual = sim.particles.iter().map(|p| p.pos[0]).max_by(f32::total_cmp).unwrap();
        let expected = case["max"][0].as_f64().unwrap() as f32;
        assert!((actual - expected).abs() < 0.001, "{name}: {actual} vs {expected}");
    }
}

#[test]
fn named_environment_culling_and_lifetime_limits_match_native_precipitation_and_blockers() {
    use crate::environment::{Environment, RayEnvironment, RayHit};
    struct Scene {
        precipitation: [[f32; 3]; 2],
        blocker: Option<[[f32; 3]; 2]>,
    }
    impl Environment for Scene {
        fn trace_environment(
            &self,
            environment: RayEnvironment,
            start: [f32; 3],
            direction: [f32; 3],
            maximum: f32,
        ) -> Option<RayHit> {
            let bounds = match environment {
                RayEnvironment::Precipitation => self.precipitation,
                RayEnvironment::PrecipitationBlocker => self.blocker?,
            };
            let mut result = None;
            for axis in 0..3 {
                if direction[axis] == 0.0 {
                    continue;
                }
                for side in 0..2 {
                    let distance = (bounds[side][axis] - start[axis]) / direction[axis];
                    if distance < 0.0 || distance > maximum {
                        continue;
                    }
                    let point: [f32; 3] = std::array::from_fn(|axis| start[axis] + distance * direction[axis]);
                    if (0..3)
                        .any(|axis| point[axis] < bounds[0][axis] - 0.0001 || point[axis] > bounds[1][axis] + 0.0001)
                    {
                        continue;
                    }
                    if result.is_none_or(|old: RayHit| distance < old.distance) {
                        let mut normal = [0.0; 3];
                        normal[axis] = if side == 0 { 1.0 } else { -1.0 };
                        result = Some(RayHit { distance, normal });
                    }
                }
            }
            result
        }
    }
    let mut definitions = crate::parse_file(include_bytes!("../tests/data/precip_cases.pcf")).unwrap();
    for definition in &mut definitions {
        definition.initializers.retain(|op| !op.function.eq_ignore_ascii_case("Remap Scalar to Vector"));
    }
    let contexts: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/precip_native_samples.json")).unwrap();
    let bounds = |value: &serde_json::Value| {
        std::array::from_fn(|side| std::array::from_fn(|axis| value[side][axis].as_f64().unwrap() as f32))
    };
    for context in contexts {
        let world = Scene {
            precipitation: bounds(&context["precipitation"]),
            blocker: (!context["blocker"].is_null()).then(|| bounds(&context["blocker"])),
        };
        for case in context["cases"].as_array().unwrap() {
            let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
            let plan = Arc::new(Plan::new(definitions.iter().find(|d| d.name == name).unwrap()));
            assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
            let mut sim = Sim::new(plan, 17);
            sim.step_in(0.025, &Points::at([0.0, 0.0, 64.0]), &[], 100, &world);
            assert_eq!(!sim.particles.is_empty(), case["alive"].as_bool().unwrap(), "{} {name}", context["name"]);
            if let Some(particle) = sim.particles.first() {
                let expected = case["life"].as_f64().unwrap() as f32;
                assert!(
                    (particle.life - expected).abs() < 0.00001,
                    "{} {name}: {} vs {expected}",
                    context["name"],
                    particle.life
                );
            }
        }
    }
}

#[test]
fn sequential_path_curves_match_native_bulge_orientation_and_unclamped_midpoints() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/path_curve_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/path_curve_native_samples.json")).unwrap();
    let mut points = Points::at([0.0; 3]);
    points.pos.push([100.0, 0.0, 0.0]);
    points.axes = [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]];
    for case in cases {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let plan = Arc::new(Plan::new(definitions.iter().find(|d| d.name == name).unwrap()));
        assert!(plan.unsupported.is_empty());
        let mut sim = Sim::new(plan, 17);
        sim.step_at(0.0, &points, &[], 100);
        assert_eq!(sim.particles.len(), 3);
        for axis in 0..3 {
            let low = sim.particles.iter().map(|p| p.pos[axis]).min_by(f32::total_cmp).unwrap();
            let high = sim.particles.iter().map(|p| p.pos[axis]).max_by(f32::total_cmp).unwrap();
            assert!(
                (low - case["minimum"][axis].as_f64().unwrap() as f32).abs() < 0.0001,
                "{name} axis{axis} low={low}"
            );
            assert!(
                (high - case["maximum"][axis].as_f64().unwrap() as f32).abs() < 0.0001,
                "{name} axis{axis} high={high}"
            );
        }
    }
}

#[test]
fn cp_hierarchy_parents_highest_point_and_biased_volume_match_native_distributions() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/hierarchy_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/hierarchy_native_samples.json")).unwrap();
    for case in cases {
        let short = case["name"].as_str().unwrap();
        let name = format!("hp_spritecard_{short}");
        let mut points = Points::at([0.0; 3]);
        points.parents = vec![0, 0, 1];
        points.pos = match short {
            "capsame" => vec![[100.0, 0.0, 0.0]; 3],
            "capbias" => vec![[0.0; 3]; 3],
            _ => vec![[0.0; 3], [100.0, 0.0, 0.0], [300.0, 0.0, 0.0]],
        };
        if short == "capparent" {
            points.parents[1] = 2;
        }
        let plan = Arc::new(Plan::new(definitions.iter().find(|d| d.name == name).unwrap()));
        assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
        let mut sim = Sim::new(plan, 17);
        sim.step_at(0.0, &points, &[], 5000);
        assert_eq!(sim.particles.len(), 5000);
        for axis in 0..3 {
            let low = sim.particles.iter().map(|p| p.pos[axis]).fold(0.0, f32::min);
            let high = sim.particles.iter().map(|p| p.pos[axis]).fold(0.0, f32::max);
            let expected_low = case["minimum"][axis].as_f64().unwrap() as f32;
            let expected_high = case["maximum"][axis].as_f64().unwrap() as f32;
            // A different collection seed changes the extrema within the same uniform volume.
            assert!((low - expected_low).abs() < 0.15, "{name} axis{axis}: {low} vs {expected_low}");
            assert!((high - expected_high).abs() < 0.15, "{name} axis{axis}: {high} vs {expected_high}");
        }
    }
}

#[test]
fn model_hitbox_positions_and_bone_lock_match_native_centers_and_animated_transforms() {
    use crate::environment::{Environment, ModelHitbox};
    struct Scene {
        bones: Vec<[[f32; 4]; 3]>,
        phase: std::cell::Cell<usize>,
        queries: std::cell::Cell<usize>,
    }
    impl Environment for Scene {
        fn model_hitboxes(&self, object: u32, set: &str) -> Option<Arc<[ModelHitbox]>> {
            assert_eq!(object, 7);
            self.queries.set(self.queries.get() + 1);
            let bounds = if set == "effects" {
                vec![(0, [4.0, 14.0, 24.0], [16.0, 26.0, 36.0]), (1, [96.0, -4.0, -4.0], [104.0, 4.0, 4.0])]
            } else {
                vec![(0, [-16.0; 3], [16.0; 3])]
            };
            Some(
                bounds
                    .into_iter()
                    .map(|(bone, minimum, maximum)| {
                        let mut transform = self.bones[bone];
                        if self.phase.get() == 2 {
                            let x = transform[0];
                            transform[0] = transform[1].map(|value| -value);
                            transform[1] = x;
                        }
                        if (1..=2).contains(&self.phase.get()) {
                            transform[0][3] += 100.0;
                        }
                        ModelHitbox { minimum, maximum, transform }
                    })
                    .collect::<Vec<_>>()
                    .into(),
            )
        }
    }
    let definitions = crate::parse_file(include_bytes!("../tests/data/model_position_cases.pcf")).unwrap();
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/model_position_native_samples.json")).unwrap();
    let world = Scene {
        bones: serde_json::from_value(reference["bones"].clone()).unwrap(),
        phase: std::cell::Cell::new(0),
        queries: std::cell::Cell::new(0),
    };
    for case in reference["cases"].as_array().unwrap() {
        let short = case["name"].as_str().unwrap();
        let name = format!("hp_spritecard_{short}");
        let plan = Arc::new(Plan::new(definitions.iter().find(|d| d.name == name).unwrap()));
        assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
        let mut points = Points::at([17.0, 19.0, 23.0]);
        if short != "capnone" {
            points.objects = vec![Some(7)];
        }
        world.phase.set(0);
        world.queries.set(0);
        let mut sim = Sim::new(plan, 17);
        sim.step_in(0.025, &points, &[], 100, &world);
        assert_eq!(sim.particles.len(), 1);
        assert_eq!(world.queries.get(), usize::from(short != "capnone"));
        for (phase, sample) in case["samples"].as_array().unwrap().iter().enumerate() {
            if phase > 0 {
                for _ in 0..100 {
                    sim.step_in(0.025, &points, &[], 100, &world);
                }
                world.phase.set(phase);
                sim.step_in(0.025, &points, &[], 100, &world);
            }
            let particle = &sim.particles[0];
            for axis in 0..3 {
                let low = particle.pos[axis].min(points.pos[0][axis]);
                let high = particle.pos[axis].max(points.pos[0][axis]);
                let expected_low = sample["minimum"][axis].as_f64().unwrap() as f32;
                let expected_high = sample["maximum"][axis].as_f64().unwrap() as f32;
                assert!(
                    (low - expected_low).abs() < 0.0001,
                    "{short} phase{phase} axis{axis} low={low} vs {expected_low}"
                );
                assert!(
                    (high - expected_high).abs() < 0.0001,
                    "{short} phase{phase} axis{axis} high={high} vs {expected_high}"
                );
            }
        }
    }
}

#[test]
fn missing_control_points_match_native_parented_and_unparented_effect_defaults() {
    let definitions = crate::parse_file(include_bytes!("../tests/data/control_point_defaults.pcf")).unwrap();
    let contexts: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/data/control_point_defaults_native.json")).unwrap();
    for context in contexts {
        let points = if context["unparented"].as_bool().unwrap() {
            Points::unparented([64.0, 128.0, 256.0])
        } else {
            Points::at([64.0, 128.0, 256.0])
        };
        for case in context["cases"].as_array().unwrap() {
            let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
            let mut sim = Sim::new(Arc::new(Plan::new(definitions.iter().find(|d| d.name == name).unwrap())), 17);
            sim.step_at(0.0, &points, &[], 1);
            assert_eq!(sim.particles.len(), 1);
            let expected: [f32; 3] = serde_json::from_value(case["position"].clone()).unwrap();
            assert_eq!(sim.particles[0].pos, expected, "{name} unparented={}", context["unparented"]);
        }
    }
}
