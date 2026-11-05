use serde_json::{Value, json};

use crate::graph::Graph;

#[derive(Clone, Copy, Debug)]
pub struct PresetInfo {
    pub id: &'static str,
    pub description: &'static str,
}

pub const PRESETS: &[PresetInfo] = &[
    PresetInfo { id: "blank", description: "The image straight to a base texture output." },
    PresetInfo { id: "tint", description: "Colors an image with a tint." },
    PresetInfo {
        id: "normal_from_height",
        description: "A base texture and a normal map made from the brightness of the image.",
    },
    PresetInfo { id: "tile_blend", description: "Two copies of an image mixed by noise, to break up repetition." },
    PresetInfo { id: "seamless", description: "Makes an image tile without a seam." },
    PresetInfo { id: "color_grade", description: "Hue, brightness, contrast and levels." },
    PresetInfo { id: "grunge", description: "Dirt: noise overlaid on the image." },
    PresetInfo {
        id: "material_set",
        description: "A base texture, a normal map and a reflection mask from one image.",
    },
];

struct Builder {
    g: Graph,
}

impl Builder {
    fn node(&mut self, kind: &str, x: f32, y: f32, params: &[(&str, Value)]) -> Result<u32, String> {
        self.g.add_with(kind, [x * 230.0, y * 170.0], params)
    }

    fn link(&mut self, from: u32, out: &str, to: u32, input: &str) -> Result<(), String> {
        self.g.connect(from, out, to, input)
    }
}

fn build(id: &str, source: &str) -> Result<Graph, String> {
    let mut b = Builder { g: Graph::new() };
    let src = json!(source);
    match id {
        "blank" => {
            let t = b.node("texture", 0.0, 0.0, &[("source", src)])?;
            let o = b.node("output", 1.0, 0.0, &[])?;
            b.link(t, "rgba", o, "color")?;
        }
        "tint" => {
            let t = b.node("texture", 0.0, 0.0, &[("source", src)])?;
            let tint = b.node("tint", 1.0, 0.0, &[("amount", json!(1.0))])?;
            let o = b.node("output", 2.0, 0.0, &[])?;
            b.link(t, "rgba", tint, "color")?;
            b.link(tint, "color", o, "color")?;
        }
        "normal_from_height" => {
            let t = b.node("texture", 0.0, 0.0, &[("source", src)])?;
            let base = b.node("output", 3.0, -0.5, &[])?;
            let gray = b.node("grayscale", 1.0, 1.0, &[])?;
            let blur = b.node("blur", 2.0, 1.0, &[("radius", json!(1.0))])?;
            let n = b.node("height_to_normal", 3.0, 1.0, &[("strength", json!(8.0))])?;
            let out = b.node("output", 4.0, 1.0, &[("target", json!("normalmap"))])?;
            b.link(t, "rgba", base, "color")?;
            b.link(t, "rgba", gray, "color")?;
            b.link(gray, "color", blur, "color")?;
            b.link(blur, "color", n, "value")?;
            b.link(n, "color", out, "color")?;
        }
        "tile_blend" => {
            let a = b.node("texture", 0.0, 0.0, &[("source", src.clone())])?;
            let uv = b.node("uv", 0.0, 1.5, &[])?;
            let tile =
                b.node("tiling_offset", 1.0, 1.5, &[("tiling", json!([1.7, 1.7])), ("offset", json!([0.31, 0.17]))])?;
            let rot = b.node("rotate_uv", 2.0, 1.5, &[("angle", json!(37.0))])?;
            let c = b.node("texture", 3.0, 1.5, &[("source", src)])?;
            let hsv = b.node("hsv", 4.0, 1.5, &[("value", json!(0.9))])?;
            let noise = b.node("noise", 0.0, 3.0, &[("scale", json!(3)), ("octaves", json!(4)), ("seed", json!(7))])?;
            let contrast = b.node("brightness_contrast", 1.0, 3.0, &[("contrast", json!(0.7))])?;
            let mix = b.node("lerp", 5.0, 0.5, &[])?;
            let o = b.node("output", 6.0, 0.5, &[])?;
            b.link(uv, "uv", tile, "uv")?;
            b.link(tile, "uv", rot, "uv")?;
            b.link(rot, "uv", c, "uv")?;
            b.link(c, "rgba", hsv, "color")?;
            b.link(noise, "value", contrast, "color")?;
            b.link(a, "rgba", mix, "a")?;
            b.link(hsv, "color", mix, "b")?;
            b.link(contrast, "color", mix, "t")?;
            b.link(mix, "result", o, "color")?;
        }
        "seamless" => {
            let t = b.node("texture", 0.0, 0.0, &[("source", src)])?;
            let s = b.node("make_seamless", 1.0, 0.0, &[("width", json!(0.25))])?;
            let o = b.node("output", 2.0, 0.0, &[])?;
            b.link(t, "rgba", s, "color")?;
            b.link(s, "color", o, "color")?;
        }
        "color_grade" => {
            let t = b.node("texture", 0.0, 0.0, &[("source", src)])?;
            let hsv = b.node("hsv", 1.0, 0.0, &[("saturation", json!(1.1))])?;
            let bc = b.node("brightness_contrast", 2.0, 0.0, &[("contrast", json!(0.1))])?;
            let lv = b.node("levels", 3.0, 0.0, &[("gamma", json!(1.0))])?;
            let o = b.node("output", 4.0, 0.0, &[])?;
            b.link(t, "rgba", hsv, "color")?;
            b.link(hsv, "color", bc, "color")?;
            b.link(bc, "color", lv, "color")?;
            b.link(lv, "color", o, "color")?;
        }
        "grunge" => {
            let t = b.node("texture", 0.0, 0.0, &[("source", src)])?;
            let n1 = b.node("noise", 0.0, 1.5, &[("scale", json!(5)), ("octaves", json!(5)), ("seed", json!(3))])?;
            let n2 =
                b.node("noise", 0.0, 3.0, &[("kind", json!("voronoi")), ("scale", json!(12)), ("seed", json!(9))])?;
            let mixn = b.node("multiply", 1.0, 2.2, &[])?;
            let ramp = b.node("gradient_map", 2.0, 2.2, &[])?;
            let bl = b.node("blend", 3.0, 0.5, &[("mode", json!("overlay"))])?;
            let o = b.node("output", 4.0, 0.5, &[])?;
            b.link(n1, "value", mixn, "a")?;
            b.link(n2, "value", mixn, "b")?;
            b.link(mixn, "result", ramp, "value")?;
            b.link(t, "rgba", bl, "base")?;
            b.link(ramp, "color", bl, "layer")?;
            b.g.set_input_default(bl, "opacity", &[0.6]).map_err(|e| e.to_string())?;
            b.link(bl, "color", o, "color")?;
        }
        "material_set" => {
            let t = b.node("texture", 0.0, 0.0, &[("source", src)])?;
            let base = b.node("output", 3.0, -0.5, &[])?;
            let gray = b.node("grayscale", 1.0, 1.0, &[])?;
            let n = b.node("height_to_normal", 2.0, 1.0, &[("strength", json!(6.0))])?;
            let nout = b.node("output", 3.0, 1.0, &[("target", json!("normalmap"))])?;
            let lv = b.node("levels", 2.0, 2.2, &[("black", json!(0.25)), ("white", json!(0.9))])?;
            let mout = b.node("output", 3.0, 2.2, &[("target", json!("envmapmask"))])?;
            b.link(t, "rgba", base, "color")?;
            b.link(t, "rgba", gray, "color")?;
            b.link(gray, "color", n, "value")?;
            b.link(n, "color", nout, "color")?;
            b.link(gray, "color", lv, "color")?;
            b.link(lv, "color", mout, "color")?;
        }
        _ => return Err(format!("unknown preset \"{id}\"")),
    }
    Ok(b.g)
}

pub fn preset(id: &str, source: &str) -> Option<Graph> {
    build(&id.trim().to_ascii_lowercase(), source).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use hp_vtf::RgbaImage;

    use super::*;
    use crate::{EvalOptions, ImageCache, ImageLoader, Severity, evaluate};

    struct Mem;

    impl ImageLoader for Mem {
        fn load(&self, _: &str, _: u32) -> Result<RgbaImage, String> {
            let px: Vec<u8> = (0..16 * 16).flat_map(|i| [(i % 16 * 16) as u8, (i / 16 * 16) as u8, 90, 255]).collect();
            Ok(RgbaImage { width: 16, height: 16, pixels: px })
        }
    }

    #[test]
    fn every_preset_is_valid_and_renders() {
        let mut outputs: HashMap<&str, usize> = HashMap::new();
        for p in PRESETS {
            let g = preset(p.id, "a.png").unwrap_or_else(|| panic!("{}", p.id));
            let bad: Vec<_> = g.validate().into_iter().filter(|d| d.severity != Severity::Info).collect();
            assert!(bad.is_empty(), "{}: {bad:?}", p.id);
            let opts = EvalOptions { width: 32, height: 32, outputs: true, ..EvalOptions::default() };
            let e = evaluate(&g, &Mem, &ImageCache::new(), &opts);
            assert!(e.diagnostics.is_empty(), "{}: {:?}", p.id, e.diagnostics);
            assert_eq!(e.outputs.len(), g.output_ids().len(), "{}", p.id);
            outputs.insert(p.id, e.outputs.len());
            assert_eq!(Graph::parse(&g.to_json()).unwrap().0, g);
        }
        assert_eq!(outputs["material_set"], 3);
        assert!(preset("nope", "").is_none());
        assert!(preset("TINT", "").is_some());
    }
}
