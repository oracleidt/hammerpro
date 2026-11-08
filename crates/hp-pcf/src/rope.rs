use crate::{Operator, Particle, Points};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rope {
    pub subdivisions: usize,
    pub texel_size: f32,
    pub scroll: f32,
    pub offset: f32,
    pub control_points: [i32; 2],
    pub scale_texture: bool,
    pub scale_scroll: bool,
    pub scale_offset: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub tangent: [f32; 3],
    pub position: [f32; 3],
    pub width: f32,
    pub color: [f32; 4],
    pub v: f32,
}

impl Rope {
    pub fn spline_controls(particles: &[Particle], index: usize) -> [[f32; 4]; 4] {
        let point = |i: usize| {
            let p = &particles[i];
            [p.pos[0], p.pos[1], p.pos[2], p.radius]
        };
        let b = point(index);
        let c = point(index + 1);
        let a = if index == 0 { std::array::from_fn(|i| b[i] - (c[i] - b[i])) } else { point(index - 1) };
        let d =
            if index + 2 == particles.len() { std::array::from_fn(|i| c[i] + (c[i] - b[i])) } else { point(index + 2) };
        [a, b, c, d]
    }

    pub(crate) fn compile(op: &Operator) -> Self {
        let texel_size = op.float("texel_size").unwrap_or(4.0);
        Self {
            subdivisions: op.int("subdivision_count").unwrap_or(3).clamp(1, 256) as usize,
            texel_size: if texel_size > 0.0 { texel_size } else { 1.0 },
            scroll: op.float("texture_scroll_rate").unwrap_or(0.0),
            offset: op.float("texture_offset").unwrap_or(0.0),
            control_points: [
                op.int("scale CP start").unwrap_or(-1).min(63),
                op.int("scale CP end").unwrap_or(-1).min(63),
            ],
            scale_texture: op.bool("scale texture by CP distance").unwrap_or(false),
            scale_scroll: op.bool("scale scroll by CP distance").unwrap_or(false),
            scale_offset: op.bool("scale offset by CP distance").unwrap_or(false),
        }
    }

    pub fn points(
        &self,
        particles: &[Particle],
        texture_height: f32,
        time: f32,
        controls: Option<&Points>,
        spline: bool,
    ) -> Vec<Point> {
        if particles.len() < 2 {
            return Vec::new();
        }
        let height = texture_height.max(1.0);
        let mut scale = (height * self.texel_size).recip();
        let mut scroll_rate = self.scroll;
        let mut offset = if spline { self.offset / height } else { self.offset };
        if self.control_points.iter().all(|i| *i >= 0)
            && let Some(controls) = controls
        {
            let a = controls.get(self.control_points[0]);
            let b = controls.get(self.control_points[1]);
            let d = std::array::from_fn::<_, 3, _>(|i| a[i] - b[i]);
            let distance = ((d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]).sqrt();
            if self.scale_texture {
                scale = (distance * self.texel_size + f32::EPSILON).recip();
            }
            if self.scale_scroll {
                if spline {
                    scroll_rate *= distance / height * if self.scale_texture { scale } else { self.texel_size };
                } else {
                    offset *= distance / height;
                }
            }
            if self.scale_offset {
                if spline {
                    offset *= distance / height + 1.0;
                } else {
                    offset += distance / height * self.offset;
                }
            }
        }
        let scroll = time * scroll_rate + offset;
        let (scale, start) = if spline { (self.texel_size, self.offset + scroll) } else { (scale, scroll * scale) };
        let color = |p: &Particle| [p.color[0], p.color[1], p.color[2], p.alpha * p.alpha2].map(byte);
        let mut result = Vec::with_capacity((particles.len() - 1) * self.subdivisions + 1);
        let mut distance = 0.0;
        let mut spline_v = start;
        result.push(Point {
            tangent: std::array::from_fn(|i| particles[1].pos[i] - particles[0].pos[i]),
            position: particles[0].pos,
            width: particles[0].radius,
            color: color(&particles[0]),
            v: start,
        });
        for index in 0..particles.len() - 1 {
            let a = &particles[index];
            let b = &particles[index + 1];
            let before = if spline && index == 0 {
                std::array::from_fn(|i| 2.0 * a.pos[i] - b.pos[i])
            } else {
                particles[index.saturating_sub(1)].pos
            };
            let after = if spline && index + 2 == particles.len() {
                std::array::from_fn(|i| 2.0 * b.pos[i] - a.pos[i])
            } else {
                particles[(index + 2).min(particles.len() - 1)].pos
            };
            let before_width = if spline && index == 0 {
                2.0 * a.radius - b.radius
            } else {
                particles[index.saturating_sub(1)].radius
            };
            let after_width = if spline && index + 2 == particles.len() {
                2.0 * b.radius - a.radius
            } else {
                particles[(index + 2).min(particles.len() - 1)].radius
            };
            let delta = std::array::from_fn::<_, 3, _>(|i| b.pos[i] - a.pos[i]);
            let length = ((delta[1] * delta[1] + delta[0] * delta[0]) + delta[2] * delta[2]).sqrt();
            let end_v = start + (distance + length) * scale;
            let start_v = start + distance * scale;
            let spline_step = length * (self.texel_size * (self.subdivisions as f32).recip());
            let mut cpu_v = start_v;
            let cpu_step = (end_v - start_v) * (self.subdivisions as f32).recip();
            let ac = color(a);
            let bc = color(b);
            for step in 1..=self.subdivisions {
                let t = step as f32 / self.subdivisions as f32;
                let position = if step == self.subdivisions { b.pos } else { catmull(before, a.pos, b.pos, after, t) };
                spline_v += spline_step;
                cpu_v += cpu_step;
                result.push(Point {
                    tangent: tangent(before, a.pos, b.pos, after, t),
                    position,
                    width: if spline {
                        catmull([before_width; 3], [a.radius; 3], [b.radius; 3], [after_width; 3], t)[0]
                    } else {
                        a.radius + (b.radius - a.radius) * t
                    },
                    color: std::array::from_fn(|i| byte(ac[i] + (bc[i] - ac[i]) * t)),
                    v: if spline {
                        spline_v
                    } else if step == self.subdivisions {
                        end_v
                    } else {
                        cpu_v
                    },
                });
            }
            distance += length;
        }
        result
    }
}

fn byte(value: f32) -> f32 {
    (value * 255.0).round_ties_even().rem_euclid(256.0) / 255.0
}

fn catmull(a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], t: f32) -> [f32; 3] {
    let square = t * t;
    let cube = square * t;
    std::array::from_fn(|i| {
        ((-0.5 * a[i] + 1.5 * b[i]) - 1.5 * c[i] + 0.5 * d[i]) * cube
            + ((a[i] - 2.5 * b[i]) + 2.0 * c[i] - 0.5 * d[i]) * square
            + (-0.5 * a[i] + 0.5 * c[i]) * t
            + b[i]
    })
}

fn tangent(a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| {
        let q = (2.0 * a[i] - 5.0 * b[i]) + 4.0 * c[i] - d[i];
        let cubic = (3.0 * b[i] - a[i]) - 3.0 * c[i] + d[i];
        0.5 * ((c[i] - a[i]) + t * (q + t * cubic) + t * (q + 2.0 * (t * cubic)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Plan, Sim, SystemDef};
    use std::sync::Arc;

    #[test]
    fn rope_uvs_match_native_vertex_builders_for_texture_scroll_and_all_control_point_flags() {
        for (spline, source) in [
            (true, include_str!("../tests/data/rope_gpu_native.json")),
            (false, include_str!("../tests/data/rope_cpu_native.json")),
        ] {
            let reference: serde_json::Value = serde_json::from_str(source).unwrap();
            let plan = Plan::new(&SystemDef { initial_particles: 1, ..Default::default() });
            let mut sim = Sim::new(Arc::new(plan), 1);
            sim.step_at(0.0, &Points::at([0.0; 3]), &[], 1);
            let mut checked = 0;
            for case in reference["cases"].as_array().unwrap() {
                let flags = case["flags"].as_u64().unwrap();
                let mut rope = Rope::compile(&Operator::default());
                rope.texel_size = case["texel_size"].as_f64().unwrap() as f32;
                rope.scroll = case["scroll"].as_f64().unwrap() as f32;
                rope.offset = case["offset"].as_f64().unwrap() as f32;
                rope.control_points = [3, 5];
                rope.scale_texture = flags & 1 != 0;
                rope.scale_scroll = flags & 2 != 0;
                rope.scale_offset = flags & 4 != 0;
                let mut controls = Points::at([0.0; 3]);
                controls.pos.resize(6, [0.0; 3]);
                for (input, index) in [3, 5].into_iter().enumerate() {
                    controls.pos[index] =
                        std::array::from_fn(|axis| case["control_points"][input][axis].as_f64().unwrap() as f32);
                }
                let mut particles = vec![sim.particles[0]; 3];
                for (index, particle) in particles.iter_mut().enumerate() {
                    particle.pos = std::array::from_fn(|axis| case["positions"][index][axis].as_f64().unwrap() as f32);
                }
                let points =
                    rope.points(&particles, 16.0, case["time"].as_f64().unwrap() as f32, Some(&controls), spline);
                assert_eq!(points.len(), 7);
                for (index, point) in points.iter().enumerate() {
                    let expected =
                        case[if spline { "parameters" } else { "uv" }][index * 2][1].as_f64().unwrap() as f32;
                    assert_eq!(
                        point.v.to_bits(),
                        expected.to_bits(),
                        "texel {} flags {flags}, vertex {index}: {} != {expected}",
                        rope.texel_size,
                        point.v
                    );
                    checked += 1;
                }
            }
            assert_eq!(checked, 168);
        }
    }
}
