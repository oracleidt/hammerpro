use crate::{Operator, Particle};

#[derive(Clone, Debug, PartialEq)]
pub struct ModelRender {
    pub model: String,
    pub skin: i32,
    pub animation_rate: f32,
    pub scale_animation_rate: bool,
    pub animation_rate_field: i32,
    pub orient_z_to_normal: bool,
    pub activity: String,
}

impl ModelRender {
    pub fn animation_cycle(time: f32, rate: f32, max_frame: usize) -> f32 {
        let value = (time * rate) / max_frame.max(1) as f32;
        let integer =
            if value.is_finite() && (-2147483648.0..2147483648.0).contains(&value) { value as i32 } else { i32::MIN };
        value - integer as f32
    }
    pub(crate) fn compile(op: &Operator) -> Self {
        Self {
            model: op.params.str("sequence 0 model").unwrap_or("NONE").to_owned(),
            skin: op.int("skin number").unwrap_or(0),
            animation_rate: op.float("animation rate").unwrap_or(30.0),
            scale_animation_rate: op.bool("scale animation rate").unwrap_or(false),
            animation_rate_field: op.int("animation rate scale field").unwrap_or(10).max(0),
            orient_z_to_normal: op.bool("orient model z to normal").unwrap_or(false),
            activity: op.params.str("activity override").unwrap_or("").to_owned(),
        }
    }

    pub fn path(&self) -> String {
        format!("models/{}", self.model.replace('\\', "/"))
    }

    pub fn rate(&self, particle: &Particle) -> f32 {
        self.animation_rate
            * if self.scale_animation_rate { particle.scalar_attribute(self.animation_rate_field) } else { 1.0 }
    }

    pub fn transform(&self, particle: &Particle) -> [[f32; 4]; 4] {
        let n = normalized(particle.normal);
        let reference = || {
            let u = normalized([(1.0 - n[2]) * (n[1] * n[1]) + n[2], 0.0, -n[0]]);
            let dot = (u[0] * n[0] + u[1] * n[1]) + u[2] * n[2];
            normalized(std::array::from_fn(|axis| u[axis] - n[axis] * dot))
        };
        let mut u = reference();
        if ((u[0] * n[0] + u[1] * n[1]) + u[2] * n[2]).abs() >= 0.1 {
            u = reference();
        }
        let cross = [u[2] * n[1] - u[1] * n[2], u[0] * n[2] - u[2] * n[0], u[1] * n[0] - u[0] * n[1]];
        let basis = if self.orient_z_to_normal { [cross, u.map(|v| -v), n] } else { [n, u, cross] };
        let angle = |v: f32| (v * 57.29578) * 0.017453292;
        let (sp, cp) = angle(particle.pitch).sin_cos();
        let (sy, cy) = angle(particle.yaw).sin_cos();
        let (sr, cr) = angle(particle.rotation).sin_cos();
        let rotation = [
            [cp * cy, cp * sy, -sp],
            [(sr * sp) * cy - cr * sy, (sr * sp) * sy + cr * cy, sr * cp],
            [(cr * sp) * cy + sr * sy, (cr * sp) * sy - sr * cy, cr * cp],
        ];
        std::array::from_fn(|column| {
            if column == 3 {
                return [particle.pos[0], particle.pos[1], particle.pos[2], 1.0];
            }
            let r = rotation[column];
            std::array::from_fn(|row| {
                if row == 3 {
                    0.0
                } else {
                    ((basis[0][row] * particle.radius * r[0] + basis[1][row] * particle.radius * r[1])
                        + basis[2][row] * particle.radius * r[2])
                        + 0.0
                }
            })
        })
    }
}

fn normalized(v: [f32; 3]) -> [f32; 3] {
    let length = ((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]).sqrt();
    let inverse = 1.0 / (length + 1e-10);
    v.map(|v| v * inverse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Plan, Points, Sim, SystemDef};
    use std::sync::Arc;

    #[test]
    fn animation_rate_uses_live_attributes_instead_of_initializer_cache_or_combined_opacity() {
        let plan = Plan::new(&SystemDef { initial_particles: 1, ..Default::default() });
        let mut sim = Sim::new(Arc::new(plan), 1);
        sim.step_at(0.0, &Points::at([0.0; 3]), &[], 1);
        let mut p = sim.particles[0];
        p.alpha = 0.5;
        p.alpha2 = 0.25;
        p.color = [0.25, 0.5, 0.75];
        p.normal = [-0.25, 0.0, 1.0];
        let mut model = ModelRender::compile(&Operator::default());
        model.scale_animation_rate = true;
        for (field, expected) in [(7, 15.0), (16, 7.5), (6, 7.5), (21, -7.5)] {
            model.animation_rate_field = field;
            assert_eq!(model.rate(&p), expected, "attribute {field}");
        }
    }

    #[test]
    fn model_animation_cycle_matches_native_clock_and_signed_fraction() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/model_animation_native.json")).unwrap();
        for case in data["cases"].as_array().unwrap() {
            let actual = ModelRender::animation_cycle(
                case["time"].as_f64().unwrap() as f32,
                case["rate"].as_f64().unwrap() as f32,
                case["max_frame"].as_u64().unwrap() as usize,
            );
            let expected = case["phase"].as_f64().unwrap() as f32;
            assert_eq!(actual.to_bits(), expected.to_bits(), "{case}");
        }
    }

    #[test]
    fn transforms_match_unmodified_native_model_renderer_instructions() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/model_transform_native.json")).unwrap();
        let plan = Plan::new(&SystemDef { initial_particles: 1, radius: 1.0, ..Default::default() });
        let mut sim = Sim::new(Arc::new(plan), 1);
        sim.step_at(0.0, &Points::at([0.0; 3]), &[], 1);
        for case in data["cases"].as_array().unwrap() {
            let mut p = sim.particles[0];
            p.normal = std::array::from_fn(|a| case["normal"][a].as_f64().unwrap() as f32);
            p.rotation = case["rotation"].as_f64().unwrap() as f32;
            p.yaw = case["yaw"].as_f64().unwrap() as f32;
            p.pitch = case["pitch"].as_f64().unwrap() as f32;
            let mut renderer = ModelRender::compile(&Operator::default());
            renderer.orient_z_to_normal = case["orient"].as_bool().unwrap();
            let actual = renderer.transform(&p);
            for row in 0..3 {
                for (column, values) in actual.iter().take(3).enumerate() {
                    let expected = case["matrix"][row * 4 + column].as_f64().unwrap() as f32;
                    assert!(
                        (values[row] - expected).abs() <= 2e-6,
                        "{case}: {row},{column}: {} != {expected}",
                        values[row]
                    );
                }
            }
        }
    }
}
