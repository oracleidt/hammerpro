//! Source trail geometry. Keep the CPU and SpriteCard arithmetic paths separate:
//! they differ in normalization, fade-at-zero handling and operation order.

use crate::sim::{Particle, Render};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Cpu,
    Spline,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub head: [f32; 3],
    pub delta: [f32; 3],
    /// Full ribbon width, unlike the radius of an animated sprite.
    pub width: f32,
}

struct Input {
    head: [f32; 3],
    previous: [f32; 3],
    age: f32,
    trail: f32,
    radius: f32,
    alpha: f32,
}

impl Render {
    pub fn trail_segment(&self, particle: &Particle, dt: f32, path: Path) -> Option<Segment> {
        segment(
            self,
            Input {
                head: particle.pos,
                previous: particle.previous_position(),
                age: particle.age,
                trail: particle.trail,
                radius: particle.radius,
                alpha: particle.opacity(),
            },
            if self.ignore_dt || dt == 0.0 { 1.0 } else { 1.0 / dt },
            path,
        )
    }
}

fn segment(render: &Render, input: Input, dt_scale: f32, path: Path) -> Option<Segment> {
    let d: [f32; 3] = std::array::from_fn(|i| input.previous[i] - input.head[i]);
    let squared = (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2];
    let (inverse, raw) = match path {
        Path::Cpu => {
            let fade = if input.age >= render.len_fade_in { 1.0 } else { input.age / render.len_fade_in };
            let length = squared.sqrt();
            (1.0 / length, (fade * input.trail) * (length * dt_scale))
        }
        Path::Spline => {
            let mut fade = input.age / render.len_fade_in;
            if fade > 1.0 {
                fade = 1.0;
            }
            let inverse = if squared > 0.0 { wide::f32x4::splat(squared).recip_sqrt().to_array()[0] } else { 0.0 };
            (inverse, (inverse * input.trail) * ((dt_scale * fade) * squared))
        }
    };
    // Source tests before applying minimum length: a stopped particle must not
    // acquire an artificial trail merely because the minimum length is nonzero.
    if input.alpha == 0.0 || raw <= 0.0 || raw.is_nan() {
        return None;
    }
    let length = render.min_len.max(render.max_len.min(raw));
    let delta = d.map(|v| (v * inverse) * length);
    let width = if render.constrain_radius { input.radius.min(length) } else { input.radius };
    Some(Segment { head: input.head, delta, width })
}

impl Segment {
    /// Four Catmull-Rom control points. The visible segment runs between 1 and 2.
    pub fn controls(self) -> [[f32; 4]; 4] {
        let positions = [
            std::array::from_fn(|i| self.head[i] - self.delta[i]),
            self.head,
            std::array::from_fn(|i| self.head[i] + self.delta[i]),
            std::array::from_fn(|i| self.head[i] + (self.delta[i] + self.delta[i])),
        ];
        positions.map(|p| [p[0], p[1], p[2], self.width])
    }

    /// Native CPU ribbon vertices, useful to project bounds for any camera.
    pub fn cpu_vertices(self, eye: [f32; 3]) -> [[f32; 3]; 4] {
        let v: [f32; 3] = std::array::from_fn(|i| self.head[i] - eye[i]);
        let d = self.delta;
        let cross = [v[1] * d[2] - v[2] * d[1], v[2] * d[0] - v[0] * d[2], v[0] * d[1] - v[1] * d[0]];
        let [x, y, z] = cross;
        let squared = (z * z + y * y) + (x * x + 1e-10);
        let inverse = wide::f32x4::splat(squared).recip_sqrt().to_array()[0];
        let inverse = ((3.0 - (inverse * squared) * inverse) * inverse) * 0.5;
        let side = cross.map(|v| v * inverse);
        let plus = std::array::from_fn(|i| self.head[i] + (self.width * 0.5) * side[i]);
        let minus = std::array::from_fn(|i| self.head[i] + (self.width * -0.5) * side[i]);
        [plus, minus, std::array::from_fn(|i| minus[i] + d[i]), std::array::from_fn(|i| plus[i] + d[i])]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn array<const N: usize>(value: &Value) -> [f32; N] {
        std::array::from_fn(|i| value[i].as_f64().unwrap() as f32)
    }

    #[test]
    fn collection_timesteps_and_native_trail_parameters_reach_the_renderer() {
        use crate::dmx::Value as DmxValue;
        use crate::{Effect, Operator, Plan, SystemDef};
        use std::sync::Arc;
        let mut renderer = Operator { function: "render_sprite_trail".into(), ..Default::default() };
        renderer.params.attrs = vec![
            ("constrain radius to length".into(), DmxValue::Bool(false)),
            ("ignore delta time".into(), DmxValue::Bool(true)),
            ("length fade in time".into(), DmxValue::Float(-1.0)),
        ];
        let plan =
            Arc::new(Plan::new(&SystemDef { initial_particles: 1, renderers: vec![renderer], ..Default::default() }));
        assert!(!plan.render.constrain_radius);
        assert!(plan.render.ignore_dt);
        assert_eq!(plan.render.len_fade_in, -1.0);
        let mut effect = Effect::new("trail", &|_| Some(plan.clone()), 17).unwrap();
        effect.step(0.02, [0.0; 3], 10);
        assert_eq!(effect.systems_with_dt().next().unwrap().2, 0.02);
        let p = &mut effect.sims[0].1.particles[0];
        assert_eq!(p.previous_position(), p.pos);
        p.vel = [20.0, 0.0, 0.0];
        assert_ne!(p.previous_position(), p.pos, "public velocity edits keep their displacement history");
        let mut render = plan.render;
        render.len_fade_in = 0.0;
        p.age = 1.0;
        p.trail = 1.0;
        for path in [Path::Cpu, Path::Spline] {
            let ignored = render.trail_segment(p, 0.02, path).unwrap();
            assert_eq!(Some(ignored), render.trail_segment(p, 0.0, path));
            render.ignore_dt = false;
            let timed = render.trail_segment(p, 0.02, path).unwrap();
            for axis in 0..3 {
                assert!((timed.delta[axis] - ignored.delta[axis] * 50.0).abs() < 0.0001);
            }
            render.ignore_dt = true;
        }
        effect.step(0.0, [0.0; 3], 10);
        assert_eq!(
            effect.systems_with_dt().next().unwrap().2,
            0.0,
            "a zero-DT render must not reuse the previous nonzero timestep"
        );
        effect.sims[0].1.restart();
        assert_eq!(effect.systems_with_dt().next().unwrap().2, 0.0);
    }

    #[test]
    fn native_cpu_and_spline_draw_decisions_and_geometry_match_float_bits() {
        for (path, fixture) in [
            (Path::Cpu, include_str!("../tests/data/native_trail_cpu.json")),
            (Path::Spline, include_str!("../tests/data/native_trail_gpu.json")),
        ] {
            let data: Value = serde_json::from_str(fixture).unwrap();
            let cases = data["cases"].as_array().unwrap();
            assert_eq!(cases.len(), 360);
            let mut checked = 0;
            for (index, case) in cases.iter().enumerate() {
                let f = |name: &str| case[name].as_f64().unwrap() as f32;
                let render = Render {
                    min_len: f("min_len"),
                    max_len: f("max_len"),
                    len_fade_in: f("fade"),
                    constrain_radius: case["constrain"].as_bool().unwrap(),
                    ..Default::default()
                };
                let input = Input {
                    head: array(&case["position"]),
                    previous: array(&case["previous"]),
                    age: f("time") - f("born"),
                    trail: f("trail"),
                    radius: f("radius"),
                    alpha: f("alpha"),
                };
                let result = segment(&render, input, f("dt_scale"), path);
                assert_eq!(result.is_some(), case["drawn"].as_bool().unwrap(), "{path:?} case {index}");
                if let Some(segment) = result {
                    match path {
                        Path::Cpu => {
                            for (vertex, actual) in segment.cpu_vertices(array(&data["eye"])).into_iter().enumerate() {
                                let expected = array::<3>(&case["expected"][vertex]);
                                assert_eq!(
                                    actual.map(f32::to_bits),
                                    expected.map(f32::to_bits),
                                    "CPU case {index}, vertex {vertex}"
                                );
                                checked += 3;
                            }
                        }
                        Path::Spline => {
                            for (point, actual) in segment.controls().into_iter().enumerate() {
                                let expected = array::<4>(&case["expected"][point]);
                                assert_eq!(
                                    actual.map(f32::to_bits),
                                    expected.map(f32::to_bits),
                                    "spline case {index}, point {point}"
                                );
                                checked += 4;
                            }
                        }
                    }
                }
            }
            assert_eq!(checked, if path == Path::Cpu { 1656 } else { 1776 });
        }
    }
}
