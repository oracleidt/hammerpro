use crate::{BakedLightBaseline, LightKind, PreviewLight, Renderer, lights, live_lighting};

pub fn light_pattern_value(pattern: &str, time: f64) -> f32 {
    if pattern.is_empty() {
        return 1.0;
    }
    let frame = if time.is_finite() { (time.max(0.0) * 10.0).floor() as usize } else { 0 };
    f32::from(pattern.as_bytes()[frame % pattern.len()].clamp(b'a', b'z') - b'a') / 12.0
}

impl BakedLightBaseline {
    pub(crate) fn at_time(&self, time: f64) -> Vec<crate::gpu::LightGpu> {
        self.lights
            .iter()
            .enumerate()
            .map(|(index, light)| {
                let scale = self.patterns.get(index).map_or(1.0, |pattern| light_pattern_value(pattern, time));
                let mut result = *light;
                for channel in 0..3 {
                    result.color[channel] *= scale;
                }
                result
            })
            .collect()
    }
}

impl Renderer {
    pub fn set_animated_lights(&mut self, input: &[PreviewLight], patterns: &[String], ambient: [f32; 3], time: f64) {
        let packed = lights::pack_lights(input);
        let mut directional = Vec::new();
        let mut local = Vec::new();
        for (index, light) in input.iter().enumerate() {
            let target = if matches!(light.kind, LightKind::Directional { .. }) {
                if directional.len() >= lights::MAX_DIRECTIONAL {
                    continue;
                }
                &mut directional
            } else {
                if local.len() + lights::MAX_DIRECTIONAL >= lights::MAX_LIGHTS {
                    continue;
                }
                &mut local
            };
            target.push(patterns.get(index).cloned().unwrap_or_default());
        }
        directional.extend(local);
        let baseline = std::sync::Arc::new(BakedLightBaseline { lights: packed.lights, patterns: directional });
        let current = lights::Packed { lights: baseline.at_time(time), n_dir: packed.n_dir, left_out: packed.left_out };
        self.unanimated_lights = Some(baseline);
        self.light_time = time;
        let previous = self.live_light_baseline().map(|b| b.at_time(time));
        self.upload_lights(live_lighting::combine(current, previous.as_deref()), ambient);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_patterns_step_at_ten_hertz_and_keep_normal_brightness() {
        assert_eq!(light_pattern_value("", 4.0), 1.0);
        assert_eq!(light_pattern_value("m", 4.0), 1.0);
        assert_eq!(light_pattern_value("amz", 0.099), 0.0);
        assert_eq!(light_pattern_value("amz", 0.1), 1.0);
        assert_eq!(light_pattern_value("amz", 0.2), 25.0 / 12.0);
        assert_eq!(light_pattern_value("amz", 0.31), 0.0);
    }
}
