use super::{Environment, Field, Operator, Particle, Points, V3, add, b, batch, f, i, lerp, scale, set_field, v};

#[derive(Clone, Debug, PartialEq)]
struct Coordinates {
    time_scale: f32,
    spatial_scale: f32,
    time_offset: f32,
    spatial_offset: V3,
}

impl Coordinates {
    fn compile(op: &Operator, time: f32, spatial: f32) -> Self {
        Self {
            time_scale: f(op, "time noise coordinate scale", time),
            spatial_scale: f(op, "spatial noise coordinate scale", spatial),
            time_offset: f(op, "time coordinate offset", 0.0),
            spatial_offset: v(op, "spatial coordinate offset", [0.0; 3]),
        }
    }

    fn at(&self, position: V3, born: f32) -> V3 {
        let time = (born + self.time_offset) * self.time_scale;
        add(scale(add(position, self.spatial_offset), self.spatial_scale), [time; 3])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Scalar {
    coordinates: Coordinates,
    min: f32,
    max: f32,
    absolute: bool,
    invert: bool,
    world_scale: f32,
    world_time: bool,
}

impl Scalar {
    pub(super) fn emitter(op: &Operator) -> Self {
        Self {
            coordinates: Coordinates {
                time_scale: f(op, "time noise coordinate scale", 0.1),
                spatial_scale: 0.0,
                time_offset: f(op, "time coordinate offset", 0.0),
                spatial_offset: [0.0; 3],
            },
            min: f(op, "emission minimum", 0.0),
            max: f(op, "emission maximum", 100.0),
            absolute: b(op, "absolute value", false),
            invert: b(op, "invert absolute value", false),
            world_scale: f(op, "world time noise coordinate scale", 0.0),
            world_time: true,
        }
    }

    pub(super) fn compile(op: &Operator, field: Field, pre_age: bool) -> Self {
        let angle = |value: f32| {
            if !pre_age && matches!(field, Field::Rotation | Field::Yaw) {
                (value as f64 * (std::f64::consts::PI / 180.0)) as f32
            } else {
                value
            }
        };
        Self {
            coordinates: Coordinates::compile(op, if pre_age { 1.0 } else { 0.1 }, if pre_age { 1.0 } else { 0.001 }),
            min: angle(f(op, if pre_age { "start age minimum" } else { "output minimum" }, 0.0)),
            max: angle(f(op, if pre_age { "start age maximum" } else { "output maximum" }, 1.0)),
            absolute: b(op, "absolute value", false),
            invert: b(op, "invert absolute value", false),
            world_scale: if pre_age { 0.0 } else { f(op, "world time noise coordinate scale", 0.0) },
            world_time: !pre_age,
        }
    }

    pub(super) fn evaluate(&self, position: V3, born: f32, world: &dyn Environment) -> f32 {
        self.sample(position, self.time_coordinates(born, world, true))
    }

    fn time_coordinates(&self, born: f32, world: &dyn Environment, include_world: bool) -> V3 {
        let time = (born + self.coordinates.time_offset) * self.coordinates.time_scale;
        if self.world_time && include_world {
            let ticks = std::array::from_fn::<_, 3, _>(|_| world.time_milliseconds() as f32 * self.world_scale);
            [time + ticks[2], time + ticks[1], time + ticks[0]]
        } else {
            [time; 3]
        }
    }

    fn sample(&self, position: V3, times: V3) -> f32 {
        let position =
            add(scale(add(position, self.coordinates.spatial_offset), self.coordinates.spatial_scale), times);
        let mut value = scalar(position);
        if self.absolute {
            value = value.abs();
        }
        if self.invert {
            value = 1.0 - value;
        }
        let factor = if self.absolute { 1.0 } else { 0.5 };
        let range = self.max - self.min;
        value * (range * factor) + ((1.0 - factor) * range + self.min)
    }

    pub(super) fn initialize_batch(
        &self,
        field: Field,
        particles: &mut [Particle],
        first: usize,
        world: &dyn Environment,
    ) {
        for span in batch::spans(first, particles.len()) {
            // Source freezes one birth time for a scalar span, or the first four lane times
            // for an entire vector span. Its vector initializer does not apply world time.
            let times = if span.vectorized {
                std::array::from_fn(|lane| self.time_coordinates(particles[span.range.start + lane].born, world, false))
            } else {
                [self.time_coordinates(particles[span.range.start].born, world, true); 4]
            };
            for (index, particle) in particles[span.range].iter_mut().enumerate() {
                let value = self.sample(particle.pos, times[if span.vectorized { index & 3 } else { 0 }]);
                if field == Field::CreationTime {
                    particle.born = value;
                } else {
                    set_field(particle, field, value, false);
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Velocity {
    coordinates: Coordinates,
    min: V3,
    max: V3,
    absolute: [bool; 3],
    invert: [bool; 3],
    cp: i32,
    local: bool,
}

impl Velocity {
    pub(super) fn compile(op: &Operator) -> Self {
        Self {
            coordinates: Coordinates::compile(op, 1.0, 0.01),
            min: v(op, "output minimum", [0.0; 3]),
            max: v(op, "output maximum", [1.0; 3]),
            absolute: v(op, "absolute value", [0.0; 3]).map(|value| value != 0.0),
            invert: v(op, "invert abs value", [0.0; 3]).map(|value| value != 0.0),
            cp: i(op, "control point number", 0).clamp(0, 63),
            local: b(op, "Apply Velocity in Local Space (0/1)", false),
        }
    }

    pub(super) fn evaluate(&self, position: V3, born: f32, points: &Points) -> V3 {
        let position = self.coordinates.at(position, born);
        // Initializers use independent offsets; the vector operator uses cumulative offsets.
        let values = [
            scalar(position),
            scalar(add(position, [100000.5, 300000.25, 9000001.0])),
            scalar(add(position, [110000.25, 310000.75, 9100000.0])),
        ];
        let velocity = std::array::from_fn(|axis| {
            let mut value = values[axis];
            if self.absolute[axis] {
                value = value.abs();
            }
            if self.invert[axis] {
                value = 1.0 - value;
            }
            let factor = if self.absolute[axis] { 1.0 } else { 0.5 };
            let range = self.max[axis] - self.min[axis];
            value * (range * factor) + ((1.0 - factor) * range + self.min[axis])
        });
        if self.local {
            let axes = points.orientation(self.cp);
            add(add(scale(axes[0], velocity[0]), scale(axes[1], velocity[1])), scale(axes[2], velocity[2]))
        } else {
            velocity
        }
    }
}

// Three independent lattice permutations followed by the native six-decimal value table.
const TABLES: &[u8; 1024] = include_bytes!("noise_tables.bin");

fn corner(x: usize, y: usize, z: usize) -> f32 {
    let xy = TABLES[256 + ((TABLES[x & 255] as usize + y) & 255)] as usize;
    let index = TABLES[512 + ((xy + z) & 255)] as usize;
    let value = TABLES[768 + index] as u32;
    ((value * 1_000_000 + 127) / 255) as f32 / 1_000_000.0
}

pub(super) fn scalar(position: V3) -> f32 {
    // Source rounds coordinates through the mantissa of (32768 + coordinate).
    // This also preserves its wrapping and reduced precision at large coordinates.
    let bits = position.map(|value| (32768.0 + value).to_bits() & 65535);
    let lattice = bits.map(|value| (value >> 8) as usize);
    let fraction = bits.map(|value| (value & 255) as f32 / 256.0);
    let [x, y, z] = lattice;
    let xy = [0, 1].map(|dz| {
        let row = [0, 1].map(|dy| lerp(corner(x, y + dy, z + dz), corner(x + 1, y + dy, z + dz), fraction[0]));
        lerp(row[0], row[1], fraction[1])
    });
    (lerp(xy[0], xy[1], fraction[2]) - 0.5) * 2.0
}

pub(super) fn vector(mut position: V3) -> V3 {
    let x = scalar(position);
    for (value, offset) in position.iter_mut().zip([100000.5, 300000.25, 9000001.0]) {
        *value += offset;
    }
    let y = scalar(position);
    for (value, offset) in position.iter_mut().zip([110000.25, 310000.75, 9100000.0]) {
        *value += offset;
    }
    [x, y, scalar(position)]
}

pub(super) fn map(value: f32, min: f32, max: f32, dt: Option<f32>) -> f32 {
    let mut amplitude = (max - min) * 0.5;
    let mut center = min + amplitude;
    if let Some(dt) = dt {
        amplitude *= dt;
        center *= dt;
    }
    value * amplitude + center
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_batch_threshold_alignment_lane_times_and_clocks_match_all_samples() {
        use crate::dmx::{Element, Value};
        use crate::{Plan, Sim, SystemDef};
        use std::sync::Arc;
        struct Clock {
            ticks: Vec<u32>,
            index: std::cell::Cell<usize>,
        }
        impl Environment for Clock {
            fn time_milliseconds(&self) -> u32 {
                let index = self.index.get();
                self.index.set(index + 1);
                self.ticks[index]
            }
        }
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/noise_batches_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 104);
        for case in cases {
            let read = |key: &str| case[key].as_f64().unwrap() as f32;
            let vector = |value: &serde_json::Value| std::array::from_fn(|axis| value[axis].as_f64().unwrap() as f32);
            let op = Operator {
                function: "Remap Noise to Scalar".into(),
                params: Element {
                    kind: "DmeParticleOperator".into(),
                    name: String::new(),
                    attrs: vec![
                        ("output field".into(), Value::Int(7)),
                        ("output minimum".into(), Value::Float(read("min"))),
                        ("output maximum".into(), Value::Float(read("max"))),
                        ("time noise coordinate scale".into(), Value::Float(read("time_scale"))),
                        ("spatial noise coordinate scale".into(), Value::Float(read("spatial_scale"))),
                        ("time coordinate offset".into(), Value::Float(read("time_offset"))),
                        ("spatial coordinate offset".into(), Value::Vec3(vector(&case["spatial_offset"]))),
                        ("absolute value".into(), Value::Bool(case["absolute"].as_bool().unwrap())),
                        ("invert absolute value".into(), Value::Bool(case["invert"].as_bool().unwrap())),
                        ("world time noise coordinate scale".into(), Value::Float(read("world_scale"))),
                    ],
                },
            };
            let total = case["positions"].as_array().unwrap().len();
            let definition = SystemDef {
                max_particles: total as u32,
                emitters: vec![Operator {
                    function: "emit_instantaneously".into(),
                    params: Element {
                        kind: String::new(),
                        name: String::new(),
                        attrs: vec![("num_to_emit".into(), Value::Int(total as i32))],
                    },
                }],
                ..Default::default()
            };
            let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
            sim.step(0.0, [0.0; 3], total);
            assert_eq!(sim.particles.len(), total);
            for (index, particle) in sim.particles.iter_mut().enumerate() {
                particle.pos = vector(&case["positions"][index]);
                particle.born = case["births"][index].as_f64().unwrap() as f32;
            }
            let world = Clock {
                ticks: case["ticks"].as_array().unwrap().iter().map(|tick| tick.as_u64().unwrap() as u32).collect(),
                index: std::cell::Cell::new(0),
            };
            let first = case["first"].as_u64().unwrap() as usize;
            let count = case["count"].as_u64().unwrap() as usize;
            Scalar::compile(&op, Field::Alpha, false).initialize_batch(
                Field::Alpha,
                &mut sim.particles[first..first + count],
                first,
                &world,
            );
            for (index, particle) in sim.particles[first..first + count].iter().enumerate() {
                let expected = case["expected"][index].as_f64().unwrap() as f32;
                assert_eq!(
                    particle.base_alpha.to_bits(),
                    expected.to_bits(),
                    "first={first} count={count} item={index}"
                );
            }
            assert_eq!(world.index.get(), case["clock_calls"].as_u64().unwrap() as usize);
        }
    }

    #[test]
    fn scalar_initializers_match_native_flags_clocks_angles_and_pre_age() {
        struct Clock {
            ticks: [u32; 3],
            calls: std::cell::Cell<usize>,
        }
        impl Environment for Clock {
            fn time_milliseconds(&self) -> u32 {
                let index = self.calls.get();
                self.calls.set(index + 1);
                self.ticks[index]
            }
        }
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/noise_initializers_native_math.json")).unwrap();
        let vector = |value: &serde_json::Value| std::array::from_fn(|axis| value[axis].as_f64().unwrap() as f32);
        for case in cases {
            let read = |name: &str| case[name].as_f64().unwrap() as f32;
            let op = Scalar {
                coordinates: Coordinates {
                    time_scale: read("time_scale"),
                    spatial_scale: read("spatial_scale"),
                    time_offset: read("time_offset"),
                    spatial_offset: vector(&case["spatial_offset"]),
                },
                min: read("min"),
                max: read("max"),
                absolute: case["absolute"].as_bool().unwrap(),
                invert: case["invert"].as_bool().unwrap(),
                world_scale: read("world_scale"),
                world_time: case["kind"] == "remap",
            };
            let world = Clock {
                ticks: std::array::from_fn(|axis| case["ticks"][axis].as_u64().unwrap() as u32),
                calls: std::cell::Cell::new(0),
            };
            let value = op.evaluate(vector(&case["position"]), read("born"), &world);
            let value = if case["kind"] == "preage" {
                read("born") - value.clamp(0.0, 1.0) * read("life")
            } else if case["field"] == 7 {
                value.clamp(0.0, 1.0)
            } else {
                value
            };
            assert_eq!(value.to_bits(), read("expected").to_bits(), "{case}");
            assert_eq!(world.calls.get(), if op.world_time { 3 } else { 0 });
        }
    }

    #[test]
    fn velocity_coordinates_flags_and_ranges_match_native_initializer() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/velocity_noise_native_math.json")).unwrap();
        let vector = |value: &serde_json::Value| std::array::from_fn(|axis| value[axis].as_f64().unwrap() as f32);
        for case in cases {
            let velocity = Velocity {
                coordinates: Coordinates {
                    time_scale: case["time_scale"].as_f64().unwrap() as f32,
                    spatial_scale: case["spatial_scale"].as_f64().unwrap() as f32,
                    time_offset: case["time_offset"].as_f64().unwrap() as f32,
                    spatial_offset: vector(&case["spatial_offset"]),
                },
                min: vector(&case["min"]),
                max: vector(&case["max"]),
                absolute: vector(&case["absolute"]).map(|value| value != 0.0),
                invert: vector(&case["invert"]).map(|value| value != 0.0),
                local: false,
                cp: 0,
            };
            let result = velocity.evaluate(
                vector(&case["position"]),
                case["born"].as_f64().unwrap() as f32,
                &Points::at([0.0; 3]),
            );
            assert_eq!(result.map(f32::to_bits), vector(&case["expected"]).map(f32::to_bits));
        }
    }

    #[test]
    fn native_lattice_interpolation_matches_all_measured_bits() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/noise_native_math.json")).unwrap();
        assert_eq!(cases.len(), 1004);
        for case in cases {
            let position = std::array::from_fn(|axis| case["position"][axis].as_f64().unwrap() as f32);
            let expected = case["expected"].as_f64().unwrap() as f32;
            assert_eq!(scalar(position).to_bits(), expected.to_bits(), "{position:?}");
        }
    }
}
