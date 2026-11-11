use std::collections::{HashMap, VecDeque};

use crate::gpu::LightGpu;
use crate::lights::Packed;

fn key(light: &LightGpu) -> [u32; 21] {
    let mut key = [0; 21];
    for (dst, value) in key.iter_mut().zip(
        light
            .pos_range
            .iter()
            .chain(&light.dir)
            .chain(&light.cone)
            .chain(&light.atten)
            .chain(&light.extra)
            .chain(std::iter::once(&light.color[3])),
    ) {
        *dst = value.to_bits();
    }
    key
}

pub(crate) fn has_delta(light: &LightGpu) -> bool {
    light.delta_color[..3].iter().any(|channel| *channel != 0.0)
}

pub(crate) fn current(lights: &[LightGpu]) -> Vec<LightGpu> {
    lights
        .iter()
        .filter(|light| light.delta_color[3] < 0.5)
        .map(|light| LightGpu { delta_color: [0.0; 4], ..*light })
        .collect()
}

pub(crate) fn combine(mut packed: Packed, baseline: Option<&[LightGpu]>) -> Packed {
    let Some(baseline) = baseline else {
        for light in packed.lights.iter_mut().filter(|light| crate::lights::is_dlight(light)) {
            light.delta_color = [light.color[0], light.color[1], light.color[2], 0.0];
        }
        return packed;
    };
    let mut previous: HashMap<[u32; 21], VecDeque<LightGpu>> = HashMap::new();
    for light in baseline
        .iter()
        .filter(|light| light.color[3] < crate::lights::KIND_DIRECTIONAL && !crate::lights::is_dlight(light))
    {
        previous.entry(key(light)).or_default().push_back(*light);
    }
    for light in packed.lights.iter_mut().skip(packed.n_dir as usize) {
        let old = if crate::lights::is_dlight(light) {
            None
        } else {
            previous.get_mut(&key(light)).and_then(VecDeque::pop_front)
        };
        for channel in 0..3 {
            light.delta_color[channel] = light.color[channel] - old.map_or(0.0, |old| old.color[channel]);
        }
    }
    let mut removed: Vec<_> = previous.into_values().flatten().collect();
    removed.sort_unstable_by_key(key);
    for mut light in removed {
        if packed.lights.len() >= crate::lights::MAX_LIGHTS {
            packed.left_out += 1;
            continue;
        }
        light.delta_color = [-light.color[0], -light.color[1], -light.color[2], 1.0];
        packed.lights.push(light);
    }
    packed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LightKind, PreviewLight};

    fn light(x: f32, intensity: f32) -> PreviewLight {
        PreviewLight {
            kind: LightKind::Point,
            pos: [x, 0.0, 32.0],
            color: [1.0, 0.5, 0.25],
            intensity,
            range: 256.0,
            falloff: None,
        }
    }

    #[test]
    fn unchanged_reordered_and_duplicate_lights_do_not_double_the_bake() {
        let input = [light(0.0, 2.0), light(32.0, 3.0), light(0.0, 2.0)];
        let baseline = crate::lights::pack_lights(&input).lights;
        let changed = crate::lights::pack_lights(&[input[1], input[2], input[0]]);
        let packed = combine(changed, Some(&baseline));
        assert_eq!(packed.lights.len(), 3);
        assert!(packed.lights.iter().all(|light| light.delta_color == [0.0; 4]));
    }

    #[test]
    fn added_modified_moved_and_removed_lights_produce_signed_direct_differences() {
        let baseline = crate::lights::pack_lights(&[light(0.0, 2.0), light(32.0, 1.0)]).lights;
        let packed = combine(crate::lights::pack_lights(&[light(0.0, 3.0), light(64.0, 4.0)]), Some(&baseline));
        assert_eq!(packed.lights.len(), 3);
        assert_eq!(packed.lights[0].delta_color, [1.0, 0.5, 0.25, 0.0]);
        assert_eq!(packed.lights[1].delta_color, [4.0, 2.0, 1.0, 0.0]);
        assert_eq!(packed.lights[2].delta_color, [-1.0, -0.5, -0.25, 1.0]);
        let rebaked = current(&packed.lights);
        assert_eq!(rebaked.len(), 2);
        let packed = combine(crate::lights::Packed { lights: rebaked.clone(), n_dir: 0, left_out: 0 }, Some(&rebaked));
        assert!(packed.lights.iter().all(|light| light.delta_color == [0.0; 4]));
    }

    #[test]
    fn engine_dynamic_lights_always_light_over_the_bake() {
        let mut dynamic = light(16.0, 2.0);
        dynamic.falloff = Some(crate::LightFalloff { quadratic: 1.0, dlight: true, ..Default::default() });
        let baseline = crate::lights::pack_lights(&[light(0.0, 2.0), dynamic]).lights;
        let packed = combine(crate::lights::pack_lights(&[light(0.0, 2.0), dynamic]), Some(&baseline));
        assert_eq!(packed.lights.len(), 2, "the dynamic light of the bake is not taken away");
        assert_eq!(packed.lights[0].delta_color, [0.0; 4]);
        assert_eq!(packed.lights[1].delta_color, [2.0, 1.0, 0.5, 0.0], "never in the lightmaps");
        assert!(crate::lights::is_dlight(&packed.lights[1]));
        let gone = combine(crate::lights::pack_lights(&[light(0.0, 2.0)]), Some(&baseline));
        assert_eq!(gone.lights.len(), 1, "removing it subtracts nothing from the bake");
        let compiled = combine(crate::lights::pack_lights(&[light(0.0, 2.0), dynamic]), None);
        assert_eq!(compiled.lights[0].delta_color, [0.0; 4]);
        assert_eq!(compiled.lights[1].delta_color, [2.0, 1.0, 0.5, 0.0], "over compiled lightmaps too");
    }
}
