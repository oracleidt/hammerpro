use std::collections::HashMap;
use std::sync::Arc;

pub mod lump {
    pub const LIGHTING: usize = 8;
    pub const WORLDLIGHTS: usize = 15;
    pub const LEAF_AMBIENT_INDEX_HDR: usize = 51;
    pub const LEAF_AMBIENT_INDEX: usize = 52;
    pub const LIGHTING_HDR: usize = 53;
    pub const WORLDLIGHTS_HDR: usize = 54;
    pub const LEAF_AMBIENT_LIGHTING_HDR: usize = 55;
    pub const LEAF_AMBIENT_LIGHTING: usize = 56;
    pub const FACES_HDR: usize = 58;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightMode {
    #[default]
    Fullbright,
    Ldr,
    Hdr,
}

impl LightMode {
    pub fn name(self) -> &'static str {
        match self {
            LightMode::Fullbright => "fullbright",
            LightMode::Ldr => "ldr",
            LightMode::Hdr => "hdr",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightImage {
    pub width: u32,
    pub height: u32,
    pub styles: Vec<u8>,
    pub texels: Vec<[f32; 3]>,
    pub bumped: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WorldLight {
    pub mode: LightMode,
    pub images: Vec<LightImage>,
    pub props: PropLighting,
    pub styles: StyleTable,
    pub detail_styles: Vec<DetailStyleLight>,
}

const RGBE: usize = 4;
const MAX_FACE_LUXELS: usize = 1 << 20;
pub const MAX_STYLES: usize = 128;

pub fn rgbe_to_linear(c: [u8; 4]) -> [f32; 3] {
    let s = 2f32.powi(i32::from(c[3] as i8)) / 255.0;
    [f32::from(c[0]) * s, f32::from(c[1]) * s, f32::from(c[2]) * s]
}

pub fn rgbe_to_ambient(c: [u8; 4]) -> [f32; 3] {
    rgbe_to_linear(c).map(|v| v * 255.0)
}

pub fn ldr_lightmap(linear: [f32; 3]) -> [f32; 3] {
    linear.map(|v| {
        let v = if v.is_finite() { v } else { 0.0 };
        let i = ((v * 1024.0).round() as i32).clamp(0, 4091);
        let f = (f64::from(i) / 1024.0).powf(1.0 / 2.2);
        let byte = (f * 255.0 * 0.5).round().clamp(0.0, 255.0);
        (byte * 2.0 / 255.0) as f32
    })
}

const DEFAULT_STYLES: [&str; 13] = [
    "m",
    "mmnmmommommnonmmonqnmmo",
    "abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcba",
    "mmmmmaaaaammmmmaaaaaabcdefgabcdefg",
    "mamamamamama",
    "jklmnopqrstuvwxyzyxwvutsrqponmlkj",
    "nmonqnmomnmomomno",
    "mmmaaaabcdefgmmmmaaaammmaamm",
    "mmmaaammmaaammmabcdefaaaammmmabcdefmmmaaaa",
    "aaaaaaaazzzzzzzz",
    "mmamammmmammamamaaamammma",
    "abcdefghijklmnopqrrqponmlkjihgfedcba",
    "mmnnmmnnnmmnn",
];

const TEST_STYLE: usize = 63;
pub const FIRST_SWITCHABLE: u8 = 32;
pub const STYLE_FPS: f64 = 10.0;

pub fn default_pattern(style: usize) -> &'static str {
    DEFAULT_STYLES.get(style).copied().unwrap_or("m")
}

pub fn letter_value(c: u8) -> f32 {
    f32::from(c.saturating_sub(b'a')) * 22.0 / 264.0
}

pub fn style_value(pattern: Option<&str>, time: f64) -> f32 {
    let Some(p) = pattern.map(str::as_bytes).filter(|p| !p.is_empty()) else { return 256.0 / 264.0 };
    let frame = if time.is_finite() { (time.max(0.0) * STYLE_FPS).floor() } else { 0.0 };
    let k = (frame % p.len() as f64) as usize;
    letter_value(p[k.min(p.len() - 1)])
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightEntity<'a> {
    pub name: &'a str,
    pub style: i32,
    pub default_style: i32,
    pub start_dark: bool,
    pub pattern: &'a str,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NamedLight {
    pub name: String,
    pub style: u8,
    pub pattern: Option<String>,
    pub start_on: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightSwitch {
    pub on: bool,
    pub pattern: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LightCmd {
    On,
    Off,
    Toggle,
    Pattern(String),
}

impl LightSwitch {
    pub fn pattern_now(&self) -> &str {
        if !self.on {
            return "a";
        }
        self.pattern.as_deref().filter(|p| !p.is_empty()).unwrap_or("m")
    }

    pub fn apply(&mut self, cmd: &LightCmd) {
        match cmd {
            LightCmd::On => self.on = true,
            LightCmd::Off => self.on = false,
            LightCmd::Toggle => self.on = !self.on,
            LightCmd::Pattern(p) => {
                self.pattern = Some(p.clone());
                self.on = true;
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleTable {
    pub lights: Vec<NamedLight>,
    pub used: Vec<u8>,
}

impl StyleTable {
    pub fn from_entities<'a>(lights: impl IntoIterator<Item = LightEntity<'a>>) -> StyleTable {
        let mut table = StyleTable::default();
        for l in lights {
            let Some(style) = u8::try_from(l.style).ok().filter(|s| usize::from(*s) < MAX_STYLES) else { continue };
            if style != 0 && !table.used.contains(&style) {
                table.used.push(style);
            }
            let name = l.name.trim();
            if name.is_empty() || style < FIRST_SWITCHABLE {
                continue;
            }
            let own = l.pattern.trim();
            let pattern = if !own.is_empty() {
                Some(own.to_owned())
            } else if l.default_style > 0 {
                Some(default_pattern(l.default_style as usize).to_owned())
            } else {
                None
            };
            let light = NamedLight { name: name.to_owned(), style, pattern, start_on: !l.start_dark };
            match table.lights.iter_mut().find(|n| n.style == style) {
                Some(n) => *n = light,
                None => table.lights.push(light),
            }
        }
        table.used.sort_unstable();
        table.lights.sort_by_key(|n| n.style);
        table
    }

    pub fn initial(&self) -> Vec<LightSwitch> {
        self.lights.iter().map(|l| LightSwitch { on: l.start_on, pattern: l.pattern.clone() }).collect()
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        let name = name.trim();
        self.lights.iter().position(|l| l.name.eq_ignore_ascii_case(name))
    }

    pub fn pattern<'s>(&'s self, style: usize, switches: &'s [LightSwitch]) -> Option<&'s str> {
        if let Some(i) = self.lights.iter().position(|l| usize::from(l.style) == style) {
            return Some(match switches.get(i) {
                Some(s) => s.pattern_now(),
                None if self.lights[i].start_on => self.lights[i].pattern.as_deref().unwrap_or("m"),
                None => "a",
            });
        }
        if style == TEST_STYLE {
            return Some("a");
        }
        DEFAULT_STYLES.get(style).copied()
    }

    pub fn values(&self, time: f64, switches: &[LightSwitch]) -> [f32; MAX_STYLES] {
        std::array::from_fn(|s| style_value(self.pattern(s, switches), time))
    }

    pub fn animated(&self, style: usize) -> bool {
        style != 0
            && (self.pattern(style, &[]).is_some_and(|p| p.bytes().any(|c| Some(c) != p.bytes().next()))
                || self.lights.iter().any(|l| usize::from(l.style) == style))
    }
}

pub fn style_scales<'a>(lights: impl IntoIterator<Item = (i32, bool, Option<&'a str>)>) -> [f32; MAX_STYLES] {
    let list: Vec<(i32, bool, Option<&str>)> = lights.into_iter().collect();
    let names: Vec<String> = list.iter().map(|(s, _, _)| format!("light{s}")).collect();
    let table = StyleTable::from_entities(list.iter().zip(&names).map(|(&(style, start_dark, pattern), name)| {
        LightEntity { name, style, default_style: 0, start_dark, pattern: pattern.unwrap_or("") }
    }));
    table.values(0.0, &table.initial())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailStyleLight {
    pub model: bool,
    pub index: u32,
    pub base: [f32; 3],
    pub styles: Vec<(u8, [f32; 3])>,
}

impl DetailStyleLight {
    pub fn color(&self, values: &[f32]) -> [u8; 4] {
        let mut c = self.base.map(|v| v * values.first().copied().unwrap_or(1.0));
        for (s, l) in &self.styles {
            let v = values.get(usize::from(*s)).copied().unwrap_or(0.0);
            for k in 0..3 {
                c[k] += v * l[k];
            }
        }
        let gamma = |v: f32| (if v.is_finite() { v } else { 0.0 }).clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0;
        [gamma(c[0]).round() as u8, gamma(c[1]).round() as u8, gamma(c[2]).round() as u8, 255]
    }
}

pub fn face_lightmap(
    lighting: &[u8],
    lightofs: i32,
    styles: [u8; 4],
    w: usize,
    h: usize,
    bumped: bool,
    scales: &[f32; MAX_STYLES],
) -> Option<Vec<[f32; 3]>> {
    let start = usize::try_from(lightofs).ok()?;
    let n = w.checked_mul(h).filter(|n| (1..=MAX_FACE_LUXELS).contains(n))?;
    let per_style = n * RGBE * if bumped { 4 } else { 1 };
    let mut out = vec![[0.0f32; 3]; n];
    let mut any = false;
    for (k, &style) in styles.iter().enumerate() {
        if style == 255 {
            break;
        }
        let at = start.checked_add(k.checked_mul(per_style)?)?;
        let data = lighting.get(at..at.checked_add(n * RGBE)?)?;
        any = true;
        let scale = scales.get(usize::from(style)).copied().unwrap_or(0.0);
        if scale == 0.0 {
            continue;
        }
        for (o, c) in out.iter_mut().zip(data.chunks_exact(RGBE)) {
            let v = rgbe_to_linear([c[0], c[1], c[2], c[3]]);
            for i in 0..3 {
                o[i] += v[i] * scale;
            }
        }
    }
    any.then_some(out)
}

pub fn face_light_image(
    lighting: &[u8],
    lightofs: i32,
    styles: [u8; 4],
    w: usize,
    h: usize,
    bumped: bool,
    mode: LightMode,
) -> Option<LightImage> {
    let start = usize::try_from(lightofs).ok()?;
    let n = w.checked_mul(h).filter(|n| (1..=MAX_FACE_LUXELS).contains(n))?;
    let images = if bumped { 4 } else { 1 };
    let per_style = n * RGBE * images;
    let mut kept: Vec<u8> = Vec::new();
    let mut texels: Vec<[f32; 3]> = Vec::new();
    for (k, &style) in styles.iter().enumerate() {
        if style == 255 {
            break;
        }
        let at = start.checked_add(k.checked_mul(per_style)?)?;
        let data = lighting.get(at..at.checked_add(per_style)?)?;
        kept.push(style);
        let first = texels.len();
        texels.extend(data.chunks_exact(RGBE).map(|c| rgbe_to_linear([c[0], c[1], c[2], c[3]])));
        if bumped {
            normalize_bumped(&mut texels[first..], n);
        }
    }
    if kept.is_empty() {
        return None;
    }
    let (w, h) = (w as u32, h as u32);
    if mode == LightMode::Fullbright {
        return Some(LightImage { width: w, height: h, styles: Vec::new(), texels: vec![[1.0; 3]; n], bumped: false });
    }
    if kept == [0] {
        to_mode(&mut texels, mode);
        return Some(LightImage { width: w, height: h, styles: Vec::new(), texels, bumped });
    }
    Some(LightImage { width: w, height: h, styles: kept, texels, bumped })
}

pub fn normalize_bumped(images: &mut [[f32; 3]], n: usize) {
    if images.len() < n * 4 {
        return;
    }
    let (flat, bumps) = images.split_at_mut(n);
    let (b1, rest) = bumps.split_at_mut(n);
    let (b2, b3) = rest.split_at_mut(n);
    for j in 0..n {
        for c in 0..3 {
            let avg = (b1[j][c] + b2[j][c] + b3[j][c]) / 3.0;
            if avg > BUMP_AVERAGE_FLOOR {
                let scale = flat[j][c] / avg;
                b1[j][c] *= scale;
                b2[j][c] *= scale;
                b3[j][c] *= scale;
            } else {
                b1[j][c] = flat[j][c];
                b2[j][c] = flat[j][c];
                b3[j][c] = flat[j][c];
            }
        }
    }
}

const BUMP_AVERAGE_FLOOR: f32 = 1.0e-6;

pub fn to_mode(texels: &mut [[f32; 3]], mode: LightMode) {
    match mode {
        LightMode::Ldr => {
            for t in texels {
                *t = ldr_lightmap(*t);
            }
        }
        LightMode::Hdr => {}
        LightMode::Fullbright => texels.fill([1.0; 3]),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmitKind {
    Surface,
    #[default]
    Point,
    Spot,
    Sky,
    Quake,
    SkyAmbient,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldLightSource {
    pub origin: [f32; 3],
    pub intensity: [f32; 3],
    pub normal: [f32; 3],
    pub cluster: i32,
    pub kind: EmitKind,
    pub style: i32,
    pub stopdot: f32,
    pub stopdot2: f32,
    pub exponent: f32,
    pub radius: f32,
    pub constant: f32,
    pub linear: f32,
    pub quadratic: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LocalLight {
    pub kind: EmitKind,
    pub origin: [f32; 3],
    pub dir: [f32; 3],
    pub color: [f32; 3],
    pub attenuation: [f32; 3],
    pub cone: [f32; 3],
    pub radius: f32,
    pub style: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelLighting {
    pub ambient: [[f32; 3]; 6],
    pub lights: Vec<LocalLight>,
    pub styled: Vec<LocalLight>,
}

pub const STATIC_PROP_USE_LIGHTING_ORIGIN: u32 = 0x2;

pub fn prop_lighting_origin(flags: u32, lighting_origin: [f32; 3]) -> Option<[f32; 3]> {
    (flags & STATIC_PROP_USE_LIGHTING_ORIGIN != 0).then_some(lighting_origin)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StaticPropLight {
    pub model: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub lighting_origin: Option<[f32; 3]>,
    pub colors: Option<Arc<Vec<[u8; 4]>>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TreeLeaf {
    contents: i32,
    cluster: i16,
    min: [i16; 3],
    max: [i16; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct AmbientSample {
    cube: [[f32; 3]; 6],
    pos: [u8; 3],
}

#[derive(Clone, Debug, Default)]
pub struct PropLighting {
    planes: Vec<([f32; 3], f32)>,
    nodes: Vec<(u32, [i32; 2])>,
    leaves: Vec<TreeLeaf>,
    head: i32,
    ambient_index: Vec<(u16, u16)>,
    samples: Vec<AmbientSample>,
    leaf_cubes: Vec<[[f32; 3]; 6]>,
    pub lights: Vec<WorldLightSource>,
    sky: Vec<([f32; 3], [f32; 3])>,
    styles: Vec<f32>,
    pub static_props: Vec<StaticPropLight>,
    pvs: Option<Arc<super::portals::BspVis>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PropLumps<'a> {
    pub planes: &'a [u8],
    pub nodes: &'a [u8],
    pub leafs: &'a [u8],
    pub leaf_version: u32,
    pub models: &'a [u8],
    pub ambient_index: &'a [u8],
    pub ambient: &'a [u8],
    pub worldlights: &'a [u8],
}

const DWORLDLIGHT_SIZE: usize = 88;
const AMBIENT_SAMPLE_SIZE: usize = 28;
const MAX_LIGHTS: usize = 1 << 16;
const MAX_SAMPLES: usize = 1 << 22;
pub const MAX_LOCAL_LIGHTS: usize = 4;
const OPAQUE: i32 = 0x1 | 0x80;
const DWL_IN_AMBIENT_CUBE: i32 = 0x1;

fn i32_at(d: &[u8], o: usize) -> Option<i32> {
    d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    d.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn f32_at(d: &[u8], o: usize) -> Option<f32> {
    d.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).filter(|v| v.is_finite())
}

fn vec_at(d: &[u8], o: usize) -> Option<[f32; 3]> {
    Some([f32_at(d, o)?, f32_at(d, o + 4)?, f32_at(d, o + 8)?])
}

fn cube_at(d: &[u8], o: usize) -> Option<[[f32; 3]; 6]> {
    let mut cube = [[0.0; 3]; 6];
    for (k, c) in cube.iter_mut().enumerate() {
        let b = d.get(o + k * 4..o + k * 4 + 4)?;
        *c = rgbe_to_ambient([b[0], b[1], b[2], b[3]]);
    }
    Some(cube)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn luminance(c: [f32; 3]) -> f32 {
    c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11
}

const CUBE_DIRS: [[f32; 3]; 6] =
    [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];

impl PropLighting {
    pub fn from_lumps(l: &PropLumps<'_>, styles: &[f32; MAX_STYLES]) -> PropLighting {
        let planes = l
            .planes
            .chunks_exact(20)
            .map(|c| (vec_at(c, 0).unwrap_or([0.0, 0.0, 1.0]), f32_at(c, 12).unwrap_or(0.0)))
            .collect();
        let nodes = l
            .nodes
            .chunks_exact(32)
            .map(|c| {
                let p = i32_at(c, 0).and_then(|p| u32::try_from(p).ok()).unwrap_or(u32::MAX);
                (p, [i32_at(c, 4).unwrap_or(-1), i32_at(c, 8).unwrap_or(-1)])
            })
            .collect();
        let leaf_size = if l.leaf_version == 0 { 56 } else { 32 };
        let leaves: Vec<TreeLeaf> = l
            .leafs
            .chunks_exact(leaf_size)
            .map(|c| {
                let s = |o| i16_at(c, o).unwrap_or(0);
                TreeLeaf {
                    contents: i32_at(c, 0).unwrap_or(0),
                    cluster: s(4),
                    min: [s(8), s(10), s(12)],
                    max: [s(14), s(16), s(18)],
                }
            })
            .collect();
        let leaf_cubes = if l.leaf_version == 0 {
            l.leafs.chunks_exact(leaf_size).map(|c| cube_at(c, 32).unwrap_or_default()).collect()
        } else {
            Vec::new()
        };
        let ambient_index =
            l.ambient_index.chunks_exact(4).map(|c| (u16_at(c, 0).unwrap_or(0), u16_at(c, 2).unwrap_or(0))).collect();
        let samples = l
            .ambient
            .chunks_exact(AMBIENT_SAMPLE_SIZE)
            .take(MAX_SAMPLES)
            .map(|c| AmbientSample { cube: cube_at(c, 0).unwrap_or_default(), pos: [c[24], c[25], c[26]] })
            .collect();
        let lights = l
            .worldlights
            .chunks_exact(DWORLDLIGHT_SIZE)
            .take(MAX_LIGHTS)
            .filter_map(|c| {
                let kind = match i32_at(c, 40)? {
                    0 => EmitKind::Surface,
                    1 => EmitKind::Point,
                    2 => EmitKind::Spot,
                    3 => EmitKind::Sky,
                    4 => EmitKind::Quake,
                    5 => EmitKind::SkyAmbient,
                    _ => return None,
                };
                if i32_at(c, 76).unwrap_or(0) & DWL_IN_AMBIENT_CUBE != 0 {
                    return None;
                }
                let style = i32_at(c, 44).unwrap_or(0);
                let intensity = vec_at(c, 12)?;
                Some(WorldLightSource {
                    origin: vec_at(c, 0)?,
                    intensity,
                    normal: vec_at(c, 24).unwrap_or([0.0, 0.0, -1.0]),
                    cluster: i32_at(c, 36).unwrap_or(-1),
                    kind,
                    style,
                    stopdot: f32_at(c, 48).unwrap_or(0.0),
                    stopdot2: f32_at(c, 52).unwrap_or(0.0),
                    exponent: f32_at(c, 56).unwrap_or(0.0),
                    radius: f32_at(c, 60).unwrap_or(0.0),
                    constant: f32_at(c, 64).unwrap_or(1.0),
                    linear: f32_at(c, 68).unwrap_or(0.0),
                    quadratic: f32_at(c, 72).unwrap_or(0.0),
                })
            })
            .collect();
        PropLighting {
            planes,
            nodes,
            leaves,
            head: i32_at(l.models, 36).unwrap_or(0),
            ambient_index,
            samples,
            leaf_cubes,
            lights,
            sky: Vec::new(),
            styles: styles.to_vec(),
            static_props: Vec::new(),
            pvs: None,
        }
    }

    pub fn set_sky_faces(&mut self, faces: Vec<([f32; 3], [f32; 3])>) {
        self.sky = faces;
    }

    pub fn set_visibility(&mut self, vis: Option<Arc<super::portals::BspVis>>) {
        self.pvs = vis;
    }

    pub fn has_data(&self) -> bool {
        !self.samples.is_empty() || !self.lights.is_empty() || !self.leaf_cubes.is_empty()
    }

    fn leaf_at(&self, p: [f32; 3]) -> Option<usize> {
        let mut node = self.head;
        for _ in 0..=self.nodes.len() {
            if node < 0 {
                let leaf = usize::try_from(-1 - node).ok()?;
                return (leaf < self.leaves.len()).then_some(leaf);
            }
            let (plane, children) = *self.nodes.get(usize::try_from(node).ok()?)?;
            let (n, d) = *self.planes.get(plane as usize)?;
            node = if dot(n, p) - d >= 0.0 { children[0] } else { children[1] };
        }
        None
    }

    pub fn clear_line(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        self.clear_node(self.head, a, b, 0)
    }

    fn clear_node(&self, node: i32, a: [f32; 3], b: [f32; 3], depth: usize) -> bool {
        if depth > 256 {
            return true;
        }
        if node < 0 {
            let leaf = usize::try_from(-1 - node).ok().and_then(|l| self.leaves.get(l));
            return leaf.is_none_or(|l| l.contents & OPAQUE == 0);
        }
        let Some(&(plane, children)) = usize::try_from(node).ok().and_then(|n| self.nodes.get(n)) else { return true };
        let Some(&(n, d)) = self.planes.get(plane as usize) else { return true };
        let (da, db) = (dot(n, a) - d, dot(n, b) - d);
        if da >= 0.0 && db >= 0.0 {
            return self.clear_node(children[0], a, b, depth + 1);
        }
        if da < 0.0 && db < 0.0 {
            return self.clear_node(children[1], a, b, depth + 1);
        }
        let t = da / (da - db);
        let mid = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        let (near, far) = if da >= 0.0 { (children[0], children[1]) } else { (children[1], children[0]) };
        self.clear_node(near, a, mid, depth + 1) && self.clear_node(far, mid, b, depth + 1)
    }

    fn first_hit(&self, a: [f32; 3], dir: [f32; 3], max: f32) -> Option<[f32; 3]> {
        let at = |t: f32| [a[0] + dir[0] * t, a[1] + dir[1] * t, a[2] + dir[2] * t];
        let mut lo = 0.0f32;
        let mut step = 64.0f32;
        let mut hi = None;
        while lo < max {
            let next = (lo + step).min(max);
            if !self.clear_line(at(lo), at(next)) {
                hi = Some(next);
                break;
            }
            lo = next;
            step = (step * 1.5).min(4096.0);
        }
        let mut hi = hi?;
        for _ in 0..24 {
            let mid = (lo + hi) * 0.5;
            if self.clear_line(at(lo), at(mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(at(hi))
    }

    pub fn ambient_at(&self, p: [f32; 3]) -> [[f32; 3]; 6] {
        let Some(mut leaf) = self.leaf_at(p) else { return [[0.0; 3]; 6] };
        if !self.leaf_cubes.is_empty() {
            return self.leaf_cubes.get(leaf).copied().unwrap_or_default();
        }
        let Some(&(mut count, mut first)) = self.ambient_index.get(leaf) else { return [[0.0; 3]; 6] };
        if count == 0 && first != 0 {
            leaf = usize::from(first);
            let Some(&(c, f)) = self.ambient_index.get(leaf) else { return [[0.0; 3]; 6] };
            (count, first) = (c, f);
        }
        let Some(l) = self.leaves.get(leaf) else { return [[0.0; 3]; 6] };
        let mut out = [[0.0f32; 3]; 6];
        let mut total = 0.0f32;
        for s in self.samples.iter().skip(usize::from(first)).take(usize::from(count)) {
            let sp: [f32; 3] = std::array::from_fn(|k| {
                let (lo, hi) = (f32::from(l.min[k]), f32::from(l.max[k]));
                lo + f32::from(s.pos[k]) * (hi - lo) / 255.0
            });
            let d = sub(sp, p);
            let factor = 1.0 / (dot(d, d) + 1.0);
            total += factor;
            for (o, c) in out.iter_mut().zip(&s.cube) {
                for k in 0..3 {
                    o[k] += c[k] * factor;
                }
            }
        }
        if total > 0.0 {
            for o in &mut out {
                for v in o.iter_mut() {
                    *v /= total;
                }
            }
        }
        out
    }

    fn sees_sky(&self, p: [f32; 3], dir_to_sun: [f32; 3]) -> bool {
        let Some(hit) = self.first_hit(p, dir_to_sun, 65536.0) else { return false };
        self.sky.iter().any(|(lo, hi)| (0..3).all(|k| hit[k] >= lo[k] - 2.0 && hit[k] <= hi[k] + 2.0))
    }

    fn light_at(&self, wl: &WorldLightSource, p: [f32; 3]) -> Option<(f32, [f32; 3])> {
        match wl.kind {
            EmitKind::SkyAmbient => None,
            EmitKind::Sky => {
                let to_sun = wl.normal.map(|v| -v);
                self.sees_sky(p, to_sun).then_some((1.0, wl.normal))
            }
            EmitKind::Point | EmitKind::Spot | EmitKind::Surface | EmitKind::Quake => {
                let d = sub(p, wl.origin);
                let dist2 = dot(d, d);
                if wl.radius > 0.0 && dist2 > wl.radius * wl.radius {
                    return None;
                }
                let dist = dist2.sqrt().max(1.0);
                let dir = d.map(|v| v / dist);
                let mut ratio = match wl.kind {
                    EmitKind::Surface => {
                        let dot2 = dot(dir, wl.normal);
                        if dot2 <= 0.0 {
                            return None;
                        }
                        dot2 / dist2.max(1.0)
                    }
                    EmitKind::Quake => (wl.linear - dist).max(0.0) / wl.linear.max(1.0),
                    _ => 1.0 / (wl.constant + wl.linear * dist + wl.quadratic * dist2).max(1e-6),
                };
                if wl.kind == EmitKind::Spot {
                    let dot2 = dot(dir, wl.normal);
                    if dot2 <= wl.stopdot2 {
                        return None;
                    }
                    if dot2 <= wl.stopdot {
                        let mut mult = ((dot2 - wl.stopdot2) / (wl.stopdot - wl.stopdot2).max(1e-6)).clamp(0.0, 1.0);
                        if wl.exponent != 0.0 && wl.exponent != 1.0 {
                            mult = mult.powf(wl.exponent);
                        }
                        ratio *= mult;
                    }
                }
                if !ratio.is_finite() || ratio <= 0.0 || !self.clear_line(p, wl.origin) {
                    return None;
                }
                Some((ratio, dir))
            }
        }
    }

    pub fn model_lighting(&self, p: [f32; 3]) -> ModelLighting {
        let mut out = ModelLighting { ambient: self.ambient_at(p), ..ModelLighting::default() };
        let row = self.pvs.as_ref().and_then(|v| {
            let c = v.cluster_at(glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2])))?;
            v.visible_from(c)
        });
        let mut found: Vec<(f32, LocalLight)> = Vec::new();
        let mut styled: Vec<(f32, LocalLight)> = Vec::new();
        for wl in &self.lights {
            let sky_light = matches!(wl.kind, EmitKind::Sky | EmitKind::SkyAmbient);
            if let (false, Some(row), Ok(c)) = (sky_light, &row, usize::try_from(wl.cluster))
                && !row.get(c).copied().unwrap_or(true)
            {
                continue;
            }
            let Some((ratio, dir)) = self.light_at(wl, p) else { continue };
            let color = wl.intensity;
            let strength = luminance(color) * ratio;
            if !strength.is_finite() || strength <= 0.0 {
                continue;
            }
            let style = u8::try_from(wl.style).ok().filter(|s| usize::from(*s) < MAX_STYLES).unwrap_or(0);
            let local = match wl.kind {
                EmitKind::Sky | EmitKind::Surface => LocalLight {
                    kind: EmitKind::Sky,
                    dir,
                    color: color.map(|c| c * ratio),
                    style,
                    ..LocalLight::default()
                },
                _ => LocalLight {
                    kind: if wl.kind == EmitKind::Spot { EmitKind::Spot } else { EmitKind::Point },
                    origin: wl.origin,
                    dir: wl.normal,
                    color,
                    attenuation: if wl.kind == EmitKind::Quake {
                        [1.0, 0.0, 0.0]
                    } else {
                        [wl.constant, wl.linear, wl.quadratic]
                    },
                    cone: [wl.stopdot, wl.stopdot2, wl.exponent],
                    radius: wl.radius,
                    style,
                },
            };
            if style == 0 {
                found.push((strength, local));
            } else {
                styled.push((strength, local));
            }
        }
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        styled.sort_by(|a, b| b.0.total_cmp(&a.0));
        out.styled = styled.iter().take(MAX_LOCAL_LIGHTS).map(|(_, l)| *l).collect();
        for (strength, l) in found.into_iter().chain(styled) {
            if out.lights.len() < MAX_LOCAL_LIGHTS {
                out.lights.push(l);
                continue;
            }
            let to_light = match l.kind {
                EmitKind::Sky => l.dir.map(|v| -v),
                _ => {
                    let d = sub(l.origin, p);
                    let len = dot(d, d).sqrt().max(1e-3);
                    d.map(|v| v / len)
                }
            };
            let strength = strength * if l.style == 0 { 1.0 } else { self.style_scale(usize::from(l.style)) };
            let lum = luminance(l.color).max(1e-6);
            for (side, dir) in out.ambient.iter_mut().zip(CUBE_DIRS) {
                let t = dot(dir, to_light);
                if t > 0.0 {
                    for (s, c) in side.iter_mut().zip(l.color) {
                        *s += t * strength * c / lum;
                    }
                }
            }
        }
        out
    }

    pub fn style_scale(&self, style: usize) -> f32 {
        self.styles.get(style).copied().unwrap_or(1.0)
    }
}

pub type StaticPropRecord = (String, [f32; 3], [f32; 3], u32, [f32; 3]);

pub fn read_static_props(d: &[u8]) -> Vec<StaticPropRecord> {
    read_static_prop_entries(d).into_iter().map(|e| (e.model, e.origin, e.angles, e.flags, e.lighting_origin)).collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StaticPropEntry {
    pub model: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub flags: u32,
    pub lighting_origin: [f32; 3],
    pub skin: u32,
    pub fade: [f32; 2],
    pub leaves: Vec<u16>,
}

pub fn read_static_prop_entries_of(d: &[u8], version: u16) -> Vec<StaticPropEntry> {
    let Ok((lump, Some(_))) = hp_bsp::staticprops::StaticPropLump::parse(d, version, None) else {
        return read_static_prop_entries(d);
    };
    let names: Vec<String> = lump.names.iter().map(|n| n.to_ascii_lowercase()).collect();
    lump.props
        .iter()
        .filter_map(|p| {
            let fade = [p.fade_min, p.fade_max];
            let first = usize::from(p.first_leaf);
            Some(StaticPropEntry {
                model: names.get(usize::from(p.prop_type))?.clone(),
                origin: p.origin,
                angles: p.angles,
                flags: p.flags,
                lighting_origin: p.lighting_origin,
                skin: u32::try_from(p.skin).unwrap_or(0),
                fade: if fade.iter().all(|f| f.is_finite()) { fade } else { [0.0; 2] },
                leaves: lump
                    .leaves
                    .get(first..first.saturating_add(usize::from(p.leaf_count)))
                    .unwrap_or_default()
                    .to_vec(),
            })
        })
        .collect()
}

pub fn read_static_props_of(d: &[u8], version: u16) -> Vec<StaticPropRecord> {
    read_static_prop_entries_of(d, version)
        .into_iter()
        .map(|e| (e.model, e.origin, e.angles, e.flags, e.lighting_origin))
        .collect()
}

pub fn read_static_prop_entries(d: &[u8]) -> Vec<StaticPropEntry> {
    let count = |o: usize| i32_at(d, o).and_then(|n| usize::try_from(n).ok());
    let Some(names) = count(0).filter(|n| *n <= 1 << 16) else { return Vec::new() };
    let mut dict = Vec::with_capacity(names);
    for i in 0..names {
        let Some(raw) = d.get(4 + i * 128..4 + (i + 1) * 128) else { return Vec::new() };
        let end = raw.iter().position(|b| *b == 0).unwrap_or(128);
        dict.push(String::from_utf8_lossy(&raw[..end]).replace('\\', "/").to_ascii_lowercase());
    }
    let leaves_at = 4 + names * 128;
    let Some(leaves) = count(leaves_at).filter(|n| *n <= 1 << 24) else { return Vec::new() };
    let leaf_list: Vec<u16> = (0..leaves).filter_map(|i| u16_at(d, leaves_at + 4 + i * 2)).collect();
    let props_at = leaves_at + 4 + leaves * 2;
    let Some(props) = count(props_at).filter(|n| *n <= 1 << 20) else { return Vec::new() };
    let start = props_at + 4;
    let rest = d.len().saturating_sub(start);
    if props == 0 || rest / props < 56 {
        return Vec::new();
    }
    let size = rest / props;
    (0..props)
        .filter_map(|p| {
            let r = start + p * size;
            let model = dict.get(usize::from(u16_at(d, r + 24)?))?.clone();
            let flags = if size >= 72 { i32_at(d, r + 64)? as u32 } else { u32::from(*d.get(r + 31)?) };
            let first = usize::from(u16_at(d, r + 26)?);
            let n = usize::from(u16_at(d, r + 28)?);
            let fade = [f32_at(d, r + 36)?, f32_at(d, r + 40)?];
            Some(StaticPropEntry {
                model,
                origin: vec_at(d, r)?,
                angles: vec_at(d, r + 12)?,
                flags,
                lighting_origin: vec_at(d, r + 44)?,
                skin: u32::try_from(i32_at(d, r + 32)?).unwrap_or(0),
                fade: if fade.iter().all(|f| f.is_finite()) { fade } else { [0.0; 2] },
                leaves: leaf_list.get(first..first.saturating_add(n)).unwrap_or_default().to_vec(),
            })
        })
        .collect()
}

pub fn read_vhv(d: &[u8]) -> Option<Vec<[u8; 4]>> {
    let version = i32_at(d, 0)?;
    if version != 2 {
        return None;
    }
    let vertex_size = i32_at(d, 12)?;
    let meshes = usize::try_from(i32_at(d, 20)?).ok().filter(|n| *n <= 1 << 16)?;
    if vertex_size != 4 {
        return None;
    }
    let mut out = Vec::new();
    for m in 0..meshes {
        let h = 40 + m * 28;
        let n = usize::try_from(i32_at(d, h + 4)?).ok()?;
        let at = usize::try_from(i32_at(d, h + 8)?).ok()?;
        let data = d.get(at..at.checked_add(n.checked_mul(4)?)?)?;
        out.extend(data.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]));
        if out.len() > 1 << 24 {
            return None;
        }
    }
    Some(out)
}

pub fn find_static_prop<'a>(
    props: &'a [StaticPropLight],
    by_key: &HashMap<(String, [i32; 3]), usize>,
    model: &str,
    origin: [f32; 3],
) -> Option<&'a StaticPropLight> {
    let key = (model.replace('\\', "/").to_ascii_lowercase(), origin.map(|v| (v * 4.0).round() as i32));
    by_key.get(&key).and_then(|i| props.get(*i))
}

pub fn static_prop_index(props: &[StaticPropLight]) -> HashMap<(String, [i32; 3]), usize> {
    props.iter().enumerate().map(|(i, p)| ((p.model.clone(), p.origin.map(|v| (v * 4.0).round() as i32)), i)).collect()
}

pub fn vertex_light(c: u8) -> f32 {
    (f32::from(c) / 255.0 * 2.0).powf(2.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgbe_decodes_like_the_engine() {
        let c = rgbe_to_linear([128, 64, 0, 0]);
        assert!((c[0] - 128.0 / 255.0).abs() < 1e-6 && (c[1] - 64.0 / 255.0).abs() < 1e-6 && c[2] == 0.0);
        let d = rgbe_to_linear([200, 100, 50, (-3i8) as u8]);
        assert!((d[0] - 200.0 / 255.0 / 8.0).abs() < 1e-6);
        assert_eq!(rgbe_to_ambient([128, 64, 0, (-7i8) as u8]), [1.0, 0.5, 0.0], "ambient cubes keep the 255 factor");
        let e = rgbe_to_linear([255, 255, 255, 2]);
        assert!((e[0] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn ldr_lightmaps_are_gamma_and_overbright() {
        let one = ldr_lightmap([1.0, 0.0, 0.25]);
        assert!((one[0] - 254.0 / 255.0 * 1.0).abs() < 0.01, "{one:?}");
        assert_eq!(one[1], 0.0);
        assert!((one[2] - 0.533).abs() < 0.01, "{one:?}");
        assert!((ldr_lightmap([10.0; 3])[0] - 239.0 * 2.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn styles_add_up_with_their_values() {
        let scales = style_scales([(32, true, None), (33, false, None), (34, false, Some("a"))]);
        assert_eq!(scales[0], 1.0);
        assert_eq!(scales[32], 0.0, "initially dark");
        assert_eq!(scales[33], 1.0);
        assert_eq!(scales[34], 0.0, "custom pattern");
        assert_eq!(scales[1], 1.0, "flicker starts at m");
        assert_eq!(scales[2], 0.0, "pulse starts at a");
        let mut lighting = vec![0u8; 8];
        lighting.extend_from_slice(&[255, 0, 0, 0, 0, 255, 0, 0]);
        lighting.extend_from_slice(&[9; 24]);
        lighting.extend_from_slice(&[0, 0, 255, 0, 0, 0, 255, 0]);
        lighting.extend_from_slice(&[9; 24]);
        let lm = face_lightmap(&lighting, 8, [0, 33, 255, 255], 2, 1, true, &scales).unwrap();
        assert_eq!(lm, [[1.0, 0.0, 1.0], [0.0, 1.0, 1.0]]);
        let dark = face_lightmap(&lighting, 8, [0, 32, 255, 255], 2, 1, true, &scales).unwrap();
        assert_eq!(dark, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        assert!(face_lightmap(&lighting, 8, [0, 33, 255, 255], 20, 20, false, &scales).is_none());
        assert!(face_lightmap(&lighting, -1, [0, 255, 255, 255], 2, 1, false, &scales).is_none());
    }

    fn tree_lumps() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
        let mut planes = Vec::new();
        for v in [1.0f32, 0.0, 0.0, 0.0] {
            planes.extend_from_slice(&v.to_le_bytes());
        }
        planes.extend_from_slice(&0i32.to_le_bytes());
        let mut nodes = vec![0u8; 32];
        nodes[4..8].copy_from_slice(&(-1i32).to_le_bytes());
        nodes[8..12].copy_from_slice(&(-2i32).to_le_bytes());
        let mut leafs = vec![0u8; 64];
        for (k, v) in [0i16, 0, 0, 100, 100, 100].iter().enumerate() {
            leafs[8 + k * 2..10 + k * 2].copy_from_slice(&v.to_le_bytes());
        }
        leafs[32..36].copy_from_slice(&1i32.to_le_bytes());
        (planes, nodes, leafs, vec![0u8; 48])
    }

    fn tree() -> PropLighting {
        let (planes, nodes, leafs, models) = tree_lumps();
        let index = [2u16.to_le_bytes(), 0u16.to_le_bytes(), 0u16.to_le_bytes(), 0u16.to_le_bytes()].concat();
        let mut ambient = Vec::new();
        for (value, pos) in [(128u8, [0u8, 0, 0]), (0, [255, 255, 255])] {
            for _ in 0..6 {
                ambient.extend_from_slice(&[value, value, value, (-7i8) as u8]);
            }
            ambient.extend_from_slice(&pos);
            ambient.push(0);
        }
        let l = PropLumps {
            planes: &planes,
            nodes: &nodes,
            leafs: &leafs,
            leaf_version: 1,
            models: &models,
            ambient_index: &index,
            ambient: &ambient,
            worldlights: &[],
        };
        PropLighting::from_lumps(&l, &style_scales([]))
    }

    #[test]
    fn ambient_cubes_are_weighted_by_distance() {
        let p = tree();
        let a = p.ambient_at([0.5, 0.0, 0.0]);
        assert!(a[0][0] > 0.99, "{a:?}");
        let m = p.ambient_at([50.0, 50.0, 50.0]);
        assert!((m[4][1] - 0.5).abs() < 0.01, "{m:?}");
        assert!(p.clear_line([10.0, 0.0, 0.0], [20.0, 5.0, 0.0]));
        assert!(!p.clear_line([10.0, 0.0, 0.0], [-20.0, 5.0, 0.0]));
        let hit = p.first_hit([50.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 1000.0).unwrap();
        assert!(hit[0].abs() < 0.01, "{hit:?}");
    }

    #[test]
    fn world_lights_light_models() {
        let mut p = tree();
        p.lights = vec![WorldLightSource {
            origin: [50.0, 50.0, 90.0],
            intensity: [100.0; 3],
            kind: EmitKind::Point,
            constant: 0.0,
            linear: 0.0,
            quadratic: 1.0,
            cluster: -1,
            ..WorldLightSource::default()
        }];
        let m = p.model_lighting([50.0, 50.0, 10.0]);
        assert_eq!(m.lights.len(), 1);
        assert_eq!(m.lights[0].attenuation, [0.0, 0.0, 1.0]);
        p.lights[0].origin = [-50.0, 50.0, 90.0];
        assert!(p.model_lighting([50.0, 50.0, 10.0]).lights.is_empty());
    }

    #[test]
    fn vhv_and_static_props_parse() {
        let mut vhv = vec![0u8; 512 + 8];
        vhv[0..4].copy_from_slice(&2i32.to_le_bytes());
        vhv[12..16].copy_from_slice(&4i32.to_le_bytes());
        vhv[16..20].copy_from_slice(&2i32.to_le_bytes());
        vhv[20..24].copy_from_slice(&1i32.to_le_bytes());
        vhv[44..48].copy_from_slice(&2i32.to_le_bytes());
        vhv[48..52].copy_from_slice(&512i32.to_le_bytes());
        vhv[512..520].copy_from_slice(&[1, 2, 3, 255, 4, 5, 6, 255]);
        assert_eq!(read_vhv(&vhv), Some(vec![[1, 2, 3, 255], [4, 5, 6, 255]]));
        assert!(read_vhv(&vhv[..100]).is_none());
        assert!((vertex_light(128) - (256.0f32 / 255.0).powf(2.2)).abs() < 1e-6);
        let mut sprp = Vec::new();
        sprp.extend_from_slice(&1i32.to_le_bytes());
        let mut name = b"Models\\Props\\A.mdl".to_vec();
        name.resize(128, 0);
        sprp.extend_from_slice(&name);
        sprp.extend_from_slice(&0i32.to_le_bytes());
        sprp.extend_from_slice(&1i32.to_le_bytes());
        let mut prop = vec![0u8; 64];
        for (k, v) in [1.0f32, 2.0, 3.0].iter().enumerate() {
            prop[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
            prop[44 + k * 4..48 + k * 4].copy_from_slice(&(v + 10.0).to_le_bytes());
        }
        prop[31] = 0x40;
        sprp.extend_from_slice(&prop);
        let props = read_static_props(&sprp);
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].0, "models/props/a.mdl");
        assert_eq!(props[0].1, [1.0, 2.0, 3.0]);
        assert_eq!(props[0].3, 0x40);
        assert_eq!(props[0].4, [11.0, 12.0, 13.0]);
        let list = vec![StaticPropLight { model: props[0].0.clone(), origin: props[0].1, ..Default::default() }];
        let index = static_prop_index(&list);
        assert!(find_static_prop(&list, &index, "MODELS/props/a.mdl", [1.0, 2.0, 3.0]).is_some());
        assert!(find_static_prop(&list, &index, "models/props/a.mdl", [1.0, 2.0, 4.0]).is_none());
    }

    #[test]
    fn static_prop_entries_keep_what_the_client_draws() {
        let mut sprp = Vec::new();
        sprp.extend_from_slice(&2i32.to_le_bytes());
        for n in [&b"models/a.mdl"[..], &b"models/b.mdl"[..]] {
            let mut name = n.to_vec();
            name.resize(128, 0);
            sprp.extend_from_slice(&name);
        }
        sprp.extend_from_slice(&3i32.to_le_bytes());
        for leaf in [5u16, 6, 9] {
            sprp.extend_from_slice(&leaf.to_le_bytes());
        }
        sprp.extend_from_slice(&2i32.to_le_bytes());
        for (model, first, count, skin, fade) in [(1u16, 0u16, 2u16, 3i32, [100.0f32, 600.0]), (0, 2, 1, 0, [0.0, 0.0])]
        {
            let mut prop = vec![0u8; 72];
            prop[0..4].copy_from_slice(&8.0f32.to_le_bytes());
            prop[16..20].copy_from_slice(&90.0f32.to_le_bytes());
            prop[24..26].copy_from_slice(&model.to_le_bytes());
            prop[26..28].copy_from_slice(&first.to_le_bytes());
            prop[28..30].copy_from_slice(&count.to_le_bytes());
            prop[32..36].copy_from_slice(&skin.to_le_bytes());
            prop[36..40].copy_from_slice(&fade[0].to_le_bytes());
            prop[40..44].copy_from_slice(&fade[1].to_le_bytes());
            sprp.extend_from_slice(&prop);
        }
        let e = read_static_prop_entries(&sprp);
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].model.as_str(), e[0].skin, e[0].fade), ("models/b.mdl", 3, [100.0, 600.0]));
        assert_eq!(e[0].leaves, [5, 6]);
        assert_eq!((e[0].origin, e[0].angles), ([8.0, 0.0, 0.0], [0.0, 90.0, 0.0]));
        assert_eq!((e[1].model.as_str(), e[1].leaves.as_slice()), ("models/a.mdl", &[9u16][..]));
        assert_eq!(read_static_props(&sprp).len(), 2, "the same props");
        assert!(read_static_prop_entries(&sprp[..200]).is_empty());
        let of = read_static_prop_entries_of(&sprp, 10);
        assert_eq!(of, e);
        assert_eq!(read_static_props_of(&sprp, 10).len(), 2);
    }

    #[test]
    fn version_9_flags_are_the_byte() {
        use hp_bsp::staticprops::{StaticPropFormat, StaticPropLump, StaticPropRecord};
        let r = StaticPropRecord { flags: 0x40, diffuse_modulation: [0xff, 0, 0, 0], ..StaticPropRecord::default() };
        let lump = StaticPropLump { names: vec!["models/A.mdl".to_owned()], leaves: vec![], props: vec![r] };
        let e = read_static_prop_entries_of(&lump.to_bytes(StaticPropFormat::V9), 9);
        assert_eq!((e[0].flags, e[0].model.as_str()), (0x40, "models/a.mdl"));
    }

    #[test]
    fn style_patterns_decode_like_the_engine() {
        assert_eq!(letter_value(b'a'), 0.0);
        assert_eq!(letter_value(b'm'), 1.0);
        assert!((letter_value(b'z') - 550.0 / 264.0).abs() < 1e-6);
        assert_eq!(letter_value(b'A'), 0.0, "below 'a'");
        let p = Some("mamb");
        let at = |t: f64| style_value(p, t);
        assert_eq!([at(0.0), at(0.1), at(0.25), at(0.35), at(0.4), at(0.5)], [1.0, 0.0, 1.0, 22.0 / 264.0, 1.0, 0.0]);
        assert_eq!(style_value(None, 3.0), 256.0 / 264.0, "never set");
        assert_eq!(style_value(Some(""), 3.0), 256.0 / 264.0);
        assert_eq!(style_value(p, f64::NAN), 1.0);
        assert_eq!(default_pattern(10), "mmamammmmammamamaaamammma");
        assert_eq!(default_pattern(1), "mmnmmommommnonmmonqnmmo");
        assert_eq!(default_pattern(40), "m");
    }

    #[test]
    fn style_table_follows_the_lights() {
        let table = StyleTable::from_entities([
            LightEntity { name: "", style: 5, ..Default::default() },
            LightEntity { name: "tube", style: 32, default_style: 10, ..Default::default() },
            LightEntity { name: "blink", style: 33, pattern: " aaazzz ", ..Default::default() },
            LightEntity { name: "off", style: 34, start_dark: true, pattern: "z", ..Default::default() },
            LightEntity { name: "plain", style: 2, ..Default::default() },
            LightEntity { name: "bad", style: 400, ..Default::default() },
        ]);
        assert_eq!(table.used, [2, 5, 32, 33, 34]);
        assert_eq!(table.lights.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["tube", "blink", "off"]);
        assert_eq!(table.lights[0].pattern.as_deref(), Some("mmamammmmammamamaaamammma"));
        assert_eq!(table.find("BLINK"), Some(1));
        assert_eq!(table.find("plain"), None);
        let mut sw = table.initial();
        assert_eq!(table.pattern(33, &sw), Some("aaazzz"));
        assert_eq!(table.pattern(34, &sw), Some("a"), "starts dark");
        assert_eq!(table.pattern(63, &sw), Some("a"), "the test style");
        assert_eq!(table.pattern(20, &sw), None, "never set");
        assert_eq!(table.pattern(0, &sw), Some("m"));
        let v = table.values(0.2, &sw);
        assert_eq!((v[0], v[32], v[33], v[34], v[63]), (1.0, 0.0, 0.0, 0.0, 0.0));
        assert!(table.animated(10) && table.animated(32) && !table.animated(0) && !table.animated(20));
        sw[2].apply(&LightCmd::On);
        assert_eq!(table.values(0.0, &sw)[34], 550.0 / 264.0);
        sw[1].apply(&LightCmd::Off);
        assert_eq!(sw[1].pattern_now(), "a");
        sw[1].apply(&LightCmd::Toggle);
        assert_eq!(sw[1].pattern_now(), "aaazzz");
        sw[0].apply(&LightCmd::Off);
        sw[0].apply(&LightCmd::Pattern("zm".into()));
        assert!(sw[0].on);
        assert_eq!(table.values(0.1, &sw)[32], 1.0);
        assert_eq!(LightSwitch { on: true, pattern: None }.pattern_now(), "m");
    }

    #[test]
    fn face_images_keep_their_styles() {
        let mut lighting = vec![0u8; 4];
        lighting.extend_from_slice(&[255, 0, 0, 0, 0, 255, 0, 0]);
        lighting.extend_from_slice(&[0, 0, 255, 0, 0, 0, 255, 0]);
        let img = face_light_image(&lighting, 4, [0, 10, 255, 255], 2, 1, false, LightMode::Ldr).unwrap();
        assert_eq!(img.styles, [0, 10]);
        assert_eq!(img.texels, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]]);
        let one = face_light_image(&lighting, 4, [0, 255, 255, 255], 2, 1, false, LightMode::Ldr).unwrap();
        assert!(one.styles.is_empty());
        assert_eq!(one.texels[0], ldr_lightmap([1.0, 0.0, 0.0]));
        let hdr = face_light_image(&lighting, 4, [0, 255, 255, 255], 2, 1, false, LightMode::Hdr).unwrap();
        assert_eq!(hdr.texels[0], [1.0, 0.0, 0.0]);
        assert!(face_light_image(&lighting, 8, [0, 10, 255, 255], 2, 1, false, LightMode::Hdr).is_none());
        assert!(face_light_image(&lighting, 4, [255; 4], 2, 1, false, LightMode::Hdr).is_none());
        assert!(!one.bumped && !img.bumped);
    }

    #[test]
    fn bumped_faces_keep_their_four_images() {
        let texel = |v: u8| [v, v, v, 0u8];
        let mut lighting = vec![0u8; 4];
        for v in [128u8, 10, 20, 30, 64, 1, 2, 3] {
            lighting.extend_from_slice(&texel(v));
        }
        let img = face_light_image(&lighting, 4, [0, 10, 255, 255], 1, 1, true, LightMode::Hdr).unwrap();
        assert!(img.bumped);
        assert_eq!(img.styles, [0, 10]);
        let reds: Vec<f32> = img.texels.iter().map(|t| t[0]).collect();
        let want: Vec<f32> = [128u8, 64, 128, 192, 64, 32, 64, 96].iter().map(|&v| f32::from(v) / 255.0).collect();
        for (a, b) in reds.iter().zip(&want) {
            assert!((a - b).abs() < 1e-3, "{reds:?}");
        }
        let one = face_light_image(&lighting, 4, [0, 255, 255, 255], 1, 1, true, LightMode::Ldr).unwrap();
        assert!(one.bumped && one.styles.is_empty());
        assert_eq!(one.texels.len(), 4);
        assert_eq!(one.texels[1], ldr_lightmap(rgbe_to_linear(texel(64))));
        let full = face_light_image(&lighting, 4, [0, 255, 255, 255], 1, 1, true, LightMode::Fullbright).unwrap();
        assert!(!full.bumped && full.texels == [[1.0; 3]]);
        assert!(face_light_image(&lighting[..12], 4, [0, 255, 255, 255], 1, 1, true, LightMode::Hdr).is_none());
    }

    #[test]
    fn bumped_lightmaps_average_to_the_flat_one_per_channel() {
        let mut images = vec![[0.25, 0.5, 1.0], [1.0, 0.0, 0.5], [0.0, 0.0, 0.5], [0.0, 0.3, 0.5]];
        normalize_bumped(&mut images, 1);
        assert_eq!(images[0], [0.25, 0.5, 1.0]);
        let avg = |c: usize| (images[1][c] + images[2][c] + images[3][c]) / 3.0;
        assert!((avg(0) - 0.25).abs() < 1e-6 && (avg(1) - 0.5).abs() < 1e-6 && (avg(2) - 1.0).abs() < 1e-6);
        assert!((images[1][0] - 0.75).abs() < 1e-6 && images[2][0] == 0.0, "the ratios between the three stay");
        assert!((images[3][1] - 1.5).abs() < 1e-6 && images[1][1] == 0.0);
        let mut dark = vec![[0.5; 3], [0.0; 3], [0.0; 3], [0.0; 3]];
        normalize_bumped(&mut dark, 1);
        assert_eq!(dark[1], [0.5; 3], "no direction at all keeps the flat light");
        let mut short = vec![[1.0; 3]; 3];
        normalize_bumped(&mut short, 1);
        assert_eq!(short, vec![[1.0; 3]; 3]);
    }

    #[test]
    fn styled_lights_keep_their_style_for_the_models() {
        let mut p = tree();
        let point = |z: f32, style: i32| WorldLightSource {
            origin: [50.0, 50.0, z],
            intensity: [100.0; 3],
            kind: EmitKind::Point,
            quadratic: 1.0,
            cluster: -1,
            style,
            ..WorldLightSource::default()
        };
        p.lights = (0..5).map(|k| point(40.0 + 10.0 * k as f32, 0)).collect();
        p.lights.push(point(20.0, 32));
        let m = p.model_lighting([50.0, 50.0, 10.0]);
        assert_eq!(m.lights.len(), MAX_LOCAL_LIGHTS);
        assert!(m.lights.iter().all(|l| l.style == 0), "style 0 first, like the engine's static lights");
        assert_eq!(m.styled.len(), 1);
        assert_eq!((m.styled[0].style, m.styled[0].color), (32, [100.0; 3]), "full intensity, scaled at draw time");
        p.lights.drain(1..5);
        let m = p.model_lighting([50.0, 50.0, 10.0]);
        assert_eq!(m.lights.iter().map(|l| l.style).collect::<Vec<_>>(), [0, 32]);
    }

    #[test]
    fn the_lighting_origin_is_only_read_when_the_prop_asks_for_it() {
        assert_eq!(prop_lighting_origin(0x88, [1.0, 2.0, 3.0]), None, "ignore normals plus no self shadowing");
        assert_eq!(prop_lighting_origin(0x8a, [1.0, 2.0, 3.0]), Some([1.0, 2.0, 3.0]));
        assert_eq!(prop_lighting_origin(0, [0.0; 3]), None, "an unset field is not the world origin");
    }

    #[test]
    fn the_sun_reaches_a_prop_the_vis_row_does_not_cover() {
        let mut p = tree();
        p.set_sky_faces(vec![([-10.0, -200.0, -200.0], [10.0, 200.0, 200.0])]);
        let mut vis = Vec::new();
        vis.extend_from_slice(&2i32.to_le_bytes());
        for o in [20i32, 20, 21, 21] {
            vis.extend_from_slice(&o.to_le_bytes());
        }
        vis.push(0b0000_0001);
        vis.push(0b0000_0011);
        let (planes, nodes, leafs, models) = tree_lumps();
        let bsp = super::super::portals::BspVis::from_lumps(&planes, &nodes, &leafs, 1, &models, &vis)
            .expect("synthetic vis");
        assert_eq!(bsp.cluster_at(glam::DVec3::new(50.0, 50.0, 10.0)), Some(0));
        p.set_visibility(Some(std::sync::Arc::new(bsp)));
        let far = |kind: EmitKind| WorldLightSource {
            origin: [50.0, 50.0, 80.0],
            intensity: [10.0; 3],
            normal: [1.0, 0.0, 0.0],
            cluster: 1,
            kind,
            style: 33,
            quadratic: 1.0,
            ..WorldLightSource::default()
        };
        p.lights = vec![far(EmitKind::Sky), far(EmitKind::Point)];
        let m = p.model_lighting([50.0, 50.0, 10.0]);
        assert_eq!(m.styled.len(), 1, "the sky light is not culled by the cluster of its lump entry");
        assert_eq!(m.styled[0].kind, EmitKind::Sky);
        assert_eq!(m.styled[0].style, 33);
    }

    #[test]
    fn detail_objects_add_their_styles() {
        let d =
            DetailStyleLight { model: false, index: 3, base: [0.25, 0.0, 0.0], styles: vec![(10, [0.0, 0.25, 0.0])] };
        let mut values = vec![1.0f32; MAX_STYLES];
        let g = |v: f32| (v.powf(1.0 / 2.2) * 255.0).round() as u8;
        assert_eq!(d.color(&values), [g(0.25), g(0.25), 0, 255]);
        values[10] = 0.0;
        assert_eq!(d.color(&values), [g(0.25), 0, 0, 255]);
        values[10] = 8.0;
        assert_eq!(d.color(&values)[1], 255, "clamped");
    }
}
