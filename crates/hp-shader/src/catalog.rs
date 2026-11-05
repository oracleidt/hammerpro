#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinTy {
    Color,
    Float,
    Uv,
}

impl PinTy {
    pub fn name(self) -> &'static str {
        match self {
            PinTy::Color => "color",
            PinTy::Float => "float",
            PinTy::Uv => "uv",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PinDef {
    pub name: &'static str,
    pub ty: PinTy,
    pub default: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub enum ParamKind {
    Float { min: f32, max: f32, default: f32 },
    Int { min: i32, max: i32, default: i32 },
    Bool { default: bool },
    Color { default: [f32; 4] },
    Choice { options: &'static [&'static str], default: usize },
    Text { default: &'static str },
    Vec2 { min: f32, max: f32, default: [f32; 2] },
    Gradient,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamDef {
    pub name: &'static str,
    pub kind: ParamKind,
}

#[derive(Clone, Copy, Debug)]
pub struct NodeDef {
    pub id: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub inputs: &'static [PinDef],
    pub outputs: &'static [PinDef],
    pub params: &'static [ParamDef],
}

impl NodeDef {
    pub fn input(&self, name: &str) -> Option<&'static PinDef> {
        self.inputs.iter().find(|p| p.name == name)
    }

    pub fn output(&self, name: &str) -> Option<&'static PinDef> {
        self.outputs.iter().find(|p| p.name == name)
    }

    pub fn param(&self, name: &str) -> Option<&'static ParamDef> {
        self.params.iter().find(|p| p.name == name)
    }

    pub fn is_output(&self) -> bool {
        self.id == "output"
    }
}

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const GRAY: [f32; 4] = [0.5, 0.5, 0.5, 1.0];

const fn fl(name: &'static str, v: f32) -> PinDef {
    PinDef { name, ty: PinTy::Float, default: [v, v, v, 1.0] }
}

const fn col(name: &'static str, default: [f32; 4]) -> PinDef {
    PinDef { name, ty: PinTy::Color, default }
}

const fn uv(name: &'static str) -> PinDef {
    PinDef { name, ty: PinTy::Uv, default: BLACK }
}

const fn pf(name: &'static str, min: f32, max: f32, default: f32) -> ParamDef {
    ParamDef { name, kind: ParamKind::Float { min, max, default } }
}

const fn pi(name: &'static str, min: i32, max: i32, default: i32) -> ParamDef {
    ParamDef { name, kind: ParamKind::Int { min, max, default } }
}

const fn pb(name: &'static str, default: bool) -> ParamDef {
    ParamDef { name, kind: ParamKind::Bool { default } }
}

const fn pc(name: &'static str, default: [f32; 4]) -> ParamDef {
    ParamDef { name, kind: ParamKind::Color { default } }
}

const fn pch(name: &'static str, options: &'static [&'static str], default: usize) -> ParamDef {
    ParamDef { name, kind: ParamKind::Choice { options, default } }
}

const fn pt(name: &'static str, default: &'static str) -> ParamDef {
    ParamDef { name, kind: ParamKind::Text { default } }
}

const fn pv(name: &'static str, min: f32, max: f32, default: [f32; 2]) -> ParamDef {
    ParamDef { name, kind: ParamKind::Vec2 { min, max, default } }
}

const fn pg(name: &'static str) -> ParamDef {
    ParamDef { name, kind: ParamKind::Gradient }
}

const fn node(
    id: &'static str,
    category: &'static str,
    description: &'static str,
    inputs: &'static [PinDef],
    outputs: &'static [PinDef],
    params: &'static [ParamDef],
) -> NodeDef {
    NodeDef { id, category, description, inputs, outputs, params }
}

pub const CATEGORIES: &[&str] = &["input", "uv", "math", "color", "filter", "procedural", "channels", "output"];

pub const WRAP_MODES: &[&str] = &["repeat", "clamp", "mirror"];
pub const FILTERS: &[&str] = &["linear", "nearest"];
pub const GRAY_MODES: &[&str] = &["luminance", "average", "lightness"];
pub const BLEND_MODES: &[&str] = &[
    "normal",
    "multiply",
    "screen",
    "overlay",
    "soft_light",
    "hard_light",
    "add",
    "subtract",
    "difference",
    "darken",
    "lighten",
    "color_dodge",
    "color_burn",
    "exclusion",
];
pub const NOISE_KINDS: &[&str] = &["perlin", "simplex", "voronoi", "white"];
pub const VORONOI_MODES: &[&str] = &["distance", "edges", "cells"];
pub const GRADIENT_KINDS: &[&str] = &["linear", "radial", "angular", "diamond"];
pub const RAMP_MODES: &[&str] = &["linear", "constant"];
pub const NORMAL_FORMATS: &[&str] = &["directx", "opengl"];
pub const TARGETS: &[&str] = &["basetexture", "normalmap", "envmapmask", "selfillummask", "custom"];
pub const FORMATS: &[&str] = &["auto", "dxt1", "dxt5", "bgra8888", "bgr888"];

const IN_UV: &[PinDef] = &[uv("uv")];
const IN_COLOR: &[PinDef] = &[col("color", GRAY)];
const IN_VALUE: &[PinDef] = &[fl("value", 0.5)];
const OUT_COLOR: &[PinDef] = &[col("color", BLACK)];
const OUT_FLOAT: &[PinDef] = &[fl("value", 0.0)];
const OUT_UV: &[PinDef] = &[uv("uv")];
const OUT_RESULT: &[PinDef] = &[col("result", BLACK)];
const OUT_RGBA: &[PinDef] = &[col("rgba", BLACK), fl("r", 0.0), fl("g", 0.0), fl("b", 0.0), fl("a", 1.0)];
const OUT_CHANNELS: &[PinDef] = &[fl("r", 0.0), fl("g", 0.0), fl("b", 0.0), fl("a", 1.0)];

const BIN_ZERO: &[PinDef] = &[col("a", BLACK), col("b", BLACK)];
const BIN_ONE: &[PinDef] = &[col("a", WHITE), col("b", WHITE)];

pub const NODES: &[NodeDef] = &[
    node(
        "texture",
        "input",
        "Samples an image: a file, or a texture of the game content.",
        IN_UV,
        OUT_RGBA,
        &[pt("source", ""), pch("wrap", WRAP_MODES, 0), pch("filter", FILTERS, 0)],
    ),
    node("color", "input", "A constant color with alpha.", &[], OUT_COLOR, &[pc("color", WHITE)]),
    node("float", "input", "A constant number.", &[], OUT_FLOAT, &[pf("value", -1000.0, 1000.0, 0.5)]),
    node("vec2", "input", "A constant 2D vector.", &[], OUT_UV, &[pv("value", -1000.0, 1000.0, [0.0, 0.0])]),
    node(
        "vec3",
        "input",
        "A constant 3D vector.",
        &[],
        OUT_COLOR,
        &[pf("x", -1000.0, 1000.0, 0.0), pf("y", -1000.0, 1000.0, 0.0), pf("z", -1000.0, 1000.0, 0.0)],
    ),
    node("uv", "input", "The texture coordinates of the image: 0 to 1 across, 0 to 1 down.", &[], OUT_UV, &[]),
    node(
        "tiling_offset",
        "uv",
        "Repeats and moves texture coordinates.",
        IN_UV,
        OUT_UV,
        &[pv("tiling", -64.0, 64.0, [1.0, 1.0]), pv("offset", -64.0, 64.0, [0.0, 0.0])],
    ),
    node(
        "rotate_uv",
        "uv",
        "Turns texture coordinates around a pivot.",
        IN_UV,
        OUT_UV,
        &[pf("angle", -360.0, 360.0, 0.0), pv("pivot", -4.0, 4.0, [0.5, 0.5])],
    ),
    node(
        "warp_uv",
        "uv",
        "Shifts texture coordinates by the red and green of a map (0.5 means no shift).",
        &[uv("uv"), col("map", GRAY)],
        OUT_UV,
        &[pf("strength", -2.0, 2.0, 0.1)],
    ),
    node("add", "math", "A + B on the color channels.", BIN_ZERO, OUT_RESULT, &[]),
    node("subtract", "math", "A - B on the color channels.", BIN_ZERO, OUT_RESULT, &[]),
    node("multiply", "math", "A * B on the color channels.", BIN_ONE, OUT_RESULT, &[]),
    node("divide", "math", "A / B on the color channels (dividing by 0 gives 0).", BIN_ONE, OUT_RESULT, &[]),
    node("min", "math", "The smaller of A and B.", BIN_ONE, OUT_RESULT, &[]),
    node("max", "math", "The larger of A and B.", BIN_ZERO, OUT_RESULT, &[]),
    node(
        "lerp",
        "math",
        "Mixes A and B: A when T is 0, B when T is 1.",
        &[col("a", BLACK), col("b", WHITE), fl("t", 0.5)],
        OUT_RESULT,
        &[],
    ),
    node(
        "clamp",
        "math",
        "Keeps a value between a minimum and a maximum.",
        &[col("value", GRAY), fl("min", 0.0), fl("max", 1.0)],
        OUT_RESULT,
        &[],
    ),
    node("saturate", "math", "Keeps a value between 0 and 1.", IN_COLOR, OUT_RESULT, &[]),
    node("power", "math", "Raises a value to a power.", &[col("value", GRAY), fl("exponent", 1.0)], OUT_RESULT, &[]),
    node("abs", "math", "The absolute value.", IN_COLOR, OUT_RESULT, &[]),
    node("one_minus", "math", "1 minus the value.", IN_COLOR, OUT_RESULT, &[]),
    node(
        "invert",
        "color",
        "Inverts the colors of an image and keeps its alpha.",
        IN_COLOR,
        OUT_COLOR,
        &[pb("invert_alpha", false)],
    ),
    node(
        "remap",
        "math",
        "Moves a range of values to another range.",
        IN_COLOR,
        OUT_RESULT,
        &[
            pf("in_min", -100.0, 100.0, 0.0),
            pf("in_max", -100.0, 100.0, 1.0),
            pf("out_min", -100.0, 100.0, 0.0),
            pf("out_max", -100.0, 100.0, 1.0),
        ],
    ),
    node(
        "smoothstep",
        "math",
        "A smooth step between two edges.",
        IN_COLOR,
        OUT_RESULT,
        &[pf("edge0", 0.0, 1.0, 0.25), pf("edge1", 0.0, 1.0, 0.75)],
    ),
    node(
        "brightness_contrast",
        "color",
        "Brightness and contrast.",
        IN_COLOR,
        OUT_COLOR,
        &[pf("brightness", -1.0, 1.0, 0.0), pf("contrast", -1.0, 1.0, 0.0)],
    ),
    node(
        "hsv",
        "color",
        "Shifts the hue and scales the saturation and the value.",
        IN_COLOR,
        OUT_COLOR,
        &[pf("hue", -180.0, 180.0, 0.0), pf("saturation", 0.0, 4.0, 1.0), pf("value", 0.0, 4.0, 1.0)],
    ),
    node(
        "tint",
        "color",
        "Multiplies an image by a tint color.",
        &[col("color", GRAY), col("tint", [1.0, 0.7, 0.45, 1.0])],
        OUT_COLOR,
        &[pf("amount", 0.0, 1.0, 1.0)],
    ),
    node(
        "grayscale",
        "color",
        "Removes the colors.",
        IN_COLOR,
        OUT_COLOR,
        &[pch("mode", GRAY_MODES, 0), pf("amount", 0.0, 1.0, 1.0)],
    ),
    node("posterize", "color", "Reduces the number of tones.", IN_COLOR, OUT_COLOR, &[pi("levels", 2, 64, 4)]),
    node(
        "levels",
        "color",
        "Black point, white point and gamma.",
        IN_COLOR,
        OUT_COLOR,
        &[pf("black", 0.0, 1.0, 0.0), pf("white", 0.0, 1.0, 1.0), pf("gamma", 0.1, 4.0, 1.0)],
    ),
    node(
        "threshold",
        "color",
        "Black and white: white where the brightness passes the threshold.",
        IN_VALUE,
        OUT_FLOAT,
        &[pf("threshold", 0.0, 1.0, 0.5), pf("softness", 0.0, 1.0, 0.0)],
    ),
    node(
        "gradient_map",
        "color",
        "Gives a color to every brightness with a color ramp.",
        IN_VALUE,
        OUT_COLOR,
        &[pg("gradient"), pch("mode", RAMP_MODES, 0)],
    ),
    node(
        "blend",
        "color",
        "Puts a layer over a base with a blend mode.",
        &[col("base", GRAY), col("layer", GRAY), fl("opacity", 1.0)],
        OUT_COLOR,
        &[pch("mode", BLEND_MODES, 1)],
    ),
    node(
        "blur",
        "filter",
        "Blurs an image (wraps around the edges).",
        IN_COLOR,
        OUT_COLOR,
        &[pf("radius", 0.0, 64.0, 4.0)],
    ),
    node(
        "sharpen",
        "filter",
        "Sharpens an image.",
        IN_COLOR,
        OUT_COLOR,
        &[pf("amount", 0.0, 8.0, 1.0), pf("radius", 0.5, 16.0, 2.0)],
    ),
    node(
        "edge_detect",
        "filter",
        "Finds the edges of the brightness of an image.",
        IN_COLOR,
        OUT_FLOAT,
        &[pf("strength", 0.0, 32.0, 4.0), pf("radius", 0.5, 16.0, 1.0)],
    ),
    node(
        "height_to_normal",
        "filter",
        "Makes a normal map from a height map.",
        IN_VALUE,
        OUT_COLOR,
        &[pf("strength", 0.0, 100.0, 8.0), pch("format", NORMAL_FORMATS, 0)],
    ),
    node(
        "normal_strength",
        "filter",
        "Makes the relief of a normal map stronger or weaker.",
        &[col("normal", [0.5, 0.5, 1.0, 1.0])],
        OUT_COLOR,
        &[pf("strength", 0.0, 8.0, 1.0)],
    ),
    node(
        "pixelate",
        "filter",
        "Makes big square pixels.",
        IN_COLOR,
        OUT_COLOR,
        &[pi("cells_x", 1, 1024, 32), pi("cells_y", 1, 1024, 32)],
    ),
    node(
        "make_seamless",
        "filter",
        "Makes an image tile without a visible seam.",
        IN_COLOR,
        OUT_COLOR,
        &[pf("width", 0.05, 0.5, 0.25)],
    ),
    node(
        "noise",
        "procedural",
        "Perlin, simplex, Voronoi or white noise; Perlin and Voronoi tile.",
        IN_UV,
        OUT_FLOAT,
        &[
            pch("kind", NOISE_KINDS, 0),
            pi("scale", 1, 256, 8),
            pi("octaves", 1, 8, 1),
            pi("lacunarity", 2, 4, 2),
            pf("gain", 0.0, 1.0, 0.5),
            pi("seed", 0, 9999, 0),
            pch("voronoi", VORONOI_MODES, 0),
            pf("jitter", 0.0, 1.0, 1.0),
        ],
    ),
    node(
        "checker",
        "procedural",
        "A checkerboard.",
        IN_UV,
        &[col("color", BLACK), fl("mask", 0.0)],
        &[pi("tiles_x", 1, 256, 8), pi("tiles_y", 1, 256, 8), pc("color_a", WHITE), pc("color_b", BLACK)],
    ),
    node(
        "gradient",
        "procedural",
        "A linear, radial, angular or diamond gradient from 0 to 1.",
        IN_UV,
        OUT_FLOAT,
        &[pch("kind", GRADIENT_KINDS, 0), pf("angle", -360.0, 360.0, 0.0)],
    ),
    node("split", "channels", "Splits a color into red, green, blue and alpha.", IN_COLOR, OUT_CHANNELS, &[]),
    node(
        "merge",
        "channels",
        "Builds a color from red, green, blue and alpha.",
        &[fl("r", 0.0), fl("g", 0.0), fl("b", 0.0), fl("a", 1.0)],
        OUT_COLOR,
        &[],
    ),
    node(
        "set_alpha",
        "channels",
        "Replaces the alpha of an image.",
        &[col("color", GRAY), fl("alpha", 1.0)],
        OUT_COLOR,
        &[],
    ),
    node(
        "output",
        "output",
        "A texture written to the game: base texture, normal map, masks. Use several for several files.",
        &[col("color", GRAY), fl("alpha", 1.0)],
        &[],
        &[
            pch("target", TARGETS, 0),
            pt("suffix", ""),
            pch("format", FORMATS, 0),
            pb("mipmaps", true),
            pb("clamp", false),
        ],
    ),
];

pub fn node_def(id: &str) -> Option<&'static NodeDef> {
    NODES.iter().find(|n| n.id == id)
}

pub fn default_suffix(target: &str) -> &'static str {
    match target {
        "normalmap" => "_normal",
        "envmapmask" => "_envmask",
        "selfillummask" => "_illum",
        "custom" => "_custom",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_categories_known() {
        for (i, n) in NODES.iter().enumerate() {
            assert!(CATEGORIES.contains(&n.category), "{}", n.id);
            assert!(NODES.iter().skip(i + 1).all(|m| m.id != n.id), "{}", n.id);
            for (j, p) in n.inputs.iter().enumerate() {
                assert!(n.inputs.iter().skip(j + 1).all(|q| q.name != p.name), "{} {}", n.id, p.name);
            }
            for (j, p) in n.outputs.iter().enumerate() {
                assert!(n.outputs.iter().skip(j + 1).all(|q| q.name != p.name), "{} {}", n.id, p.name);
            }
            for (j, p) in n.params.iter().enumerate() {
                assert!(n.params.iter().skip(j + 1).all(|q| q.name != p.name), "{} {}", n.id, p.name);
                if let ParamKind::Choice { options, default } = p.kind {
                    assert!(default < options.len());
                }
            }
        }
    }
}
