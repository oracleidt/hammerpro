use crate::Material;
use crate::info::ShaderKind;
use crate::value::parse_f32;

pub const FOUR_WAY_LAYERS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FourWayBlend {
    pub lum: [[f32; 2]; FOUR_WAY_LAYERS],
    pub blend: [[f32; 2]; FOUR_WAY_LAYERS],
    pub lum_blend_factor: [f32; FOUR_WAY_LAYERS],
    pub uv_scale: [[f32; 2]; FOUR_WAY_LAYERS],
    pub modulate_2x: [bool; FOUR_WAY_LAYERS],
}

impl Default for FourWayBlend {
    fn default() -> Self {
        Self {
            lum: [[0.0, 1.0]; FOUR_WAY_LAYERS],
            blend: [[0.0, 1.0]; FOUR_WAY_LAYERS],
            lum_blend_factor: [0.0; FOUR_WAY_LAYERS],
            uv_scale: [[1.0, 1.0]; FOUR_WAY_LAYERS],
            modulate_2x: [false; FOUR_WAY_LAYERS],
        }
    }
}

pub fn parse_vec2(s: &str) -> Option<[f32; 2]> {
    let t = s.trim();
    let inner = t.strip_prefix('[').map_or(t, |r| r.strip_suffix(']').unwrap_or(r));
    let mut out = [0.0f32; 2];
    let mut count = 0usize;
    for part in inner.split_ascii_whitespace() {
        let v = parse_f32(part)?;
        if let Some(slot) = out.get_mut(count) {
            *slot = v;
        }
        count += 1;
    }
    match count {
        1 => Some([out[0]; 2]),
        n if n >= 2 => Some(out),
        _ => None,
    }
}

impl Material {
    pub fn four_way_blend(&self) -> Option<FourWayBlend> {
        if self.shader_kind() != ShaderKind::Lightmapped4WayBlend {
            return None;
        }
        let mut out = FourWayBlend::default();
        let finite = |v: Option<f32>, default: f32| v.filter(|x| x.is_finite()).unwrap_or(default);
        for layer in 0..FOUR_WAY_LAYERS {
            let n = layer + 1;
            let lum = &mut out.lum[layer];
            lum[0] = finite(self.get_f32(&format!("$texture{n}_lumstart")), 0.0);
            lum[1] = finite(self.get_f32(&format!("$texture{n}_lumend")), 1.0);
            if let Some(uv) = self.get(&format!("$texture{n}_uvscale")).and_then(parse_vec2)
                && uv.iter().all(|v| v.is_finite())
            {
                out.uv_scale[layer] = uv;
            }
            if layer == 0 {
                continue;
            }
            let blend = &mut out.blend[layer];
            blend[0] = finite(self.get_f32(&format!("$texture{n}_blendstart")), 0.0);
            blend[1] = finite(self.get_f32(&format!("$texture{n}_blendend")), 1.0);
            out.lum_blend_factor[layer] = finite(self.get_f32(&format!("$lumblendfactor{n}")), 0.0);
            out.modulate_2x[layer] = self.get_i32(&format!("$texture{n}_blendmode")) == Some(1);
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]

    use super::*;

    #[test]
    fn reads_every_layer_parameter() {
        let m = crate::parse(
            r#"Lightmapped_4WayBlend {
                "$basetexture" "a/1" "$basetexture2" "a/2" "$basetexture3" "a/3" "$basetexture4" "a/4"
                "$texture1_lumstart" "0.1" "$texture1_lumend" "0.5"
                "$texture2_lumstart" "0.2" "$texture2_lumend" "0.6"
                "$texture2_blendstart" "0.8" "$texture2_blendend" "0.9"
                "$texture3_blendstart" ".75" "$texture3_blendend" "1"
                "$texture4_lumend" "0.05" "$texture4_blendmode" "1"
                "$texture2_uvscale" "[2 3]" "$texture3_uvscale" "0.5" "$texture4_uvscale" "[x 1]"
                "$lumblendfactor2" "0.5" "$lumblendfactor3" "0.7" "$lumblendfactor4" "nan"
            }"#,
        )
        .unwrap();
        let f = m.four_way_blend().unwrap();
        assert_eq!(f.lum, [[0.1, 0.5], [0.2, 0.6], [0.0, 1.0], [0.0, 0.05]]);
        assert_eq!(f.blend, [[0.0, 1.0], [0.8, 0.9], [0.75, 1.0], [0.0, 1.0]]);
        assert_eq!(f.lum_blend_factor, [0.0, 0.5, 0.7, 0.0]);
        assert_eq!(f.uv_scale, [[1.0, 1.0], [2.0, 3.0], [0.5, 0.5], [1.0, 1.0]]);
        assert_eq!(f.modulate_2x, [false, false, false, true]);
    }

    #[test]
    fn other_shaders_have_no_four_way_blend() {
        let wvt = crate::parse(r#"WorldVertexTransition { "$basetexture" "a/1" "$texture2_blendstart" "1" }"#).unwrap();
        assert_eq!(wvt.four_way_blend(), None);
        let plain = crate::parse(r#"Lightmapped_4WayBlend { "$basetexture" "a/1" }"#).unwrap();
        assert_eq!(plain.four_way_blend(), Some(FourWayBlend::default()));
        assert_eq!(parse_vec2("[1 2 3]"), Some([1.0, 2.0]));
        assert_eq!(parse_vec2("[]"), None);
    }
}
