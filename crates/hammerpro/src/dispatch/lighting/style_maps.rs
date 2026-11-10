use std::collections::HashMap;

use hp_bsp::Bsp;
use hp_render::lightmap::LightmapRect;

#[derive(Debug)]
pub(super) struct SourceFace {
    layers: Vec<(String, Vec<[f32; 3]>)>,
}

#[derive(Debug)]
pub(super) struct AnimatedFace {
    face: u32,
    rect: LightmapRect,
    layers: Vec<(String, Vec<[f32; 3]>)>,
    last: Vec<f32>,
    packed: Vec<u32>,
    hdr: bool,
}

fn patterns(bsp: &Bsp) -> HashMap<u8, String> {
    entity_patterns(&bsp.entities().unwrap_or_default())
}

fn entity_patterns(entities: &[hp_bsp::Entity]) -> HashMap<u8, String> {
    let mut result: HashMap<_, _> = (0..255u8)
        .map(|style| {
            (
                style,
                if style == 0 {
                    String::new()
                } else {
                    hp_editor::limits::world_light::default_pattern(style as usize).to_owned()
                },
            )
        })
        .collect();
    for entity in entities {
        if !entity.classname().starts_with("light") {
            continue;
        }
        let Some(style) = entity.get("style").and_then(|s| s.parse::<u8>().ok()) else { continue };
        if let Some(pattern) = entity
            .get("pattern")
            .filter(|p| !p.is_empty() && p.len() <= 256 && p.bytes().all(|c| c.is_ascii_lowercase()))
        {
            result.insert(style, pattern.into());
        } else if let Some(default) =
            entity.get("defaultstyle").and_then(|s| s.parse::<usize>().ok()).filter(|s| *s > 0)
        {
            result.insert(style, hp_editor::limits::world_light::default_pattern(default).into());
        }
    }
    result
}

pub(super) fn has_animation(bsp: &Bsp) -> bool {
    let patterns = patterns(bsp);
    bsp.entities().unwrap_or_default().iter().any(|entity| {
        entity.classname().starts_with("light")
            && entity
                .get("style")
                .and_then(|s| s.parse::<u8>().ok())
                .and_then(|s| patterns.get(&s))
                .is_some_and(|p| needs_preview(p))
    })
}

pub(super) fn needs_preview(pattern: &str) -> bool {
    pattern.bytes().any(|value| value != b'm')
}

pub(super) fn patterns_of(entities: &[hp_bsp::Entity]) -> HashMap<u8, String> {
    entity_patterns(entities)
}

impl SourceFace {
    pub(super) fn new(layers: Vec<(String, Vec<[f32; 3]>)>) -> Self {
        Self { layers }
    }

    pub(super) fn bind(self, face: u32, rect: LightmapRect, hdr: bool) -> AnimatedFace {
        let count = (rect.w * rect.h * if rect.bumped { 4 } else { 1 }) as usize;
        AnimatedFace { face, rect, layers: self.layers, last: Vec::new(), packed: vec![0; count], hdr }
    }
}

impl AnimatedFace {
    pub(super) fn resource_probes(&self, probes: &mut hp_app::runtime::primitives::ResourceProbes) {
        let class = hp_app::runtime::primitives::PrimitiveClass::Lighting;
        probes.vec(&self.layers, class);
        probes.vec(&self.last, class);
        probes.vec(&self.packed, class);
        for (pattern, image) in &self.layers {
            probes.string(pattern, class);
            probes.vec(image, class);
        }
    }

    pub(super) fn face_id(&self) -> u32 {
        self.face
    }

    pub(super) fn animated(&self) -> bool {
        self.layers.iter().any(|(pattern, _)| pattern.bytes().any(|value| Some(value) != pattern.bytes().next()))
    }
    pub(super) fn sample(&mut self, time: f64) -> bool {
        if self.last.len() == self.layers.len()
            && self
                .layers
                .iter()
                .zip(&self.last)
                .all(|((pattern, _), previous)| hp_render::light_pattern_value(pattern, time) == *previous)
        {
            return false;
        }
        self.last.clear();
        self.last.extend(self.layers.iter().map(|(pattern, _)| hp_render::light_pattern_value(pattern, time)));
        for (index, out) in self.packed.iter_mut().enumerate() {
            let mut sum = [0.0; 3];
            for ((_, image), weight) in self.layers.iter().zip(&self.last) {
                if let Some(color) = image.get(index) {
                    for channel in 0..3 {
                        sum[channel] += color[channel] * weight;
                    }
                }
            }
            if !self.hdr {
                sum = hp_editor::limits::world_light::ldr_lightmap(sum);
            }
            *out = hp_light::pack::pack_rg11b10(sum);
        }
        true
    }

    pub(super) fn write(&self) -> (u32, LightmapRect, &[u32]) {
        (self.face, self.rect, &self.packed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_light_style_preserves_appearance_after_compiler_assigns_a_switchable_style() {
        let mut entity = hp_bsp::Entity::default();
        entity.set("classname", "light");
        entity.set("style", "32");
        entity.set("defaultstyle", "10");
        let patterns = entity_patterns(&[entity.clone()]);
        assert_eq!(patterns[&32], hp_editor::limits::world_light::default_pattern(10));
        entity.set("pattern", "a");
        let patterns = entity_patterns(&[entity]);
        assert_eq!(patterns[&32], "a");
        assert!(needs_preview(&patterns[&32]));
        assert!(!needs_preview("mmmm"));
    }

    #[test]
    fn animated_bake_channels_mix_in_linear_space_before_ldr_conversion() {
        for hdr in [false, true] {
            let source =
                SourceFace { layers: vec![(String::new(), vec![[0.25; 3]; 4]), ("amz".into(), vec![[0.5; 3]; 4])] };
            let rect = LightmapRect { layer: 0, x: 0, y: 0, w: 1, h: 1, density: 1.0, bumped: true };
            let mut face = source.bind(0, rect, hdr);
            for (time, value) in [(0.0, 0.25), (0.1, 0.75), (0.2, 0.25 + 0.5 * 25.0 / 12.0)] {
                assert!(face.sample(time));
                let want = if hdr { [value; 3] } else { hp_editor::limits::world_light::ldr_lightmap([value; 3]) };
                assert_eq!(face.packed, vec![hp_light::pack::pack_rg11b10(want); 4]);
                assert!(!face.sample(time + 0.001));
            }
        }
    }
}
