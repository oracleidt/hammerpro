use std::collections::HashMap;
use std::path::Path;

use egui::{RichText, Ui};
use hp_app::{AppState, Severity, t, tf};
use hp_editor::FaceRef;
use hp_vtf::{Refusal, TexelKind, TexelMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    Base,
    Bump,
    Detail,
    Base2,
    Bump2,
    EnvMask,
    BlendMod,
}

impl Layer {
    pub(crate) const ALL: [Layer; 7] =
        [Layer::Base, Layer::Base2, Layer::Bump, Layer::Bump2, Layer::Detail, Layer::EnvMask, Layer::BlendMod];

    pub fn transform_key(self) -> &'static str {
        match self {
            Layer::Base => "$basetexturetransform",
            Layer::Bump => "$bumptransform",
            Layer::Detail => "$detailtexturetransform",
            Layer::Base2 => "$basetexturetransform2",
            Layer::Bump2 => "$bumptransform2",
            Layer::EnvMask => "$envmapmasktransform",
            Layer::BlendMod => "$blendmasktransform",
        }
    }

    pub(crate) fn texture_keys(self) -> &'static [&'static str] {
        match self {
            Layer::Base => &["$basetexture"],
            Layer::Bump => &["$bumpmap", "$normalmap"],
            Layer::Detail => &["$detail"],
            Layer::Base2 => &["$basetexture2"],
            Layer::Bump2 => &["$bumpmap2"],
            Layer::EnvMask => &["$envmapmask"],
            Layer::BlendMod => &["$blendmodulatetexture"],
        }
    }

    pub(crate) fn bake_keys(self) -> (&'static str, &'static str) {
        match self {
            Layer::Base => ("%hpbasetexturesource", "%hpbasetexturetransform"),
            Layer::Bump => ("%hpbumpsource", "%hpbumptransform"),
            Layer::Detail => ("%hpdetailsource", "%hpdetailtransform"),
            Layer::Base2 => ("%hpbasetexture2source", "%hpbasetexture2transform"),
            Layer::Bump2 => ("%hpbumpmap2source", "%hpbumpmap2transform"),
            Layer::EnvMask => ("%hpenvmapmasksource", "%hpenvmapmasktransform"),
            Layer::BlendMod => ("%hpblendmodulatesource", "%hpblendmodulatetransform"),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Layer::Base => t!("tex.uvl.base"),
            Layer::Bump => t!("tex.uvl.bump"),
            Layer::Detail => t!("tex.uvl.detail"),
            Layer::Base2 => t!("tex.uvl.base2"),
            Layer::Bump2 => t!("tex.uvl.bump2"),
            Layer::EnvMask => t!("tex.uvl.envmask"),
            Layer::BlendMod => t!("tex.uvl.blendmod"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2 {
    pub m: [[f64; 2]; 2],
    pub t: [f64; 2],
}

impl Affine2 {
    pub const IDENTITY: Affine2 = Affine2 { m: [[1.0, 0.0], [0.0, 1.0]], t: [0.0, 0.0] };

    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        [self.m[0][0] * p[0] + self.m[0][1] * p[1] + self.t[0], self.m[1][0] * p[0] + self.m[1][1] * p[1] + self.t[1]]
    }

    pub fn inverse(&self) -> Option<Affine2> {
        let det = self.m[0][0] * self.m[1][1] - self.m[0][1] * self.m[1][0];
        if !(det.is_finite() && det.abs() > 1e-12) {
            return None;
        }
        let m = [[self.m[1][1] / det, -self.m[0][1] / det], [-self.m[1][0] / det, self.m[0][0] / det]];
        let t = [-(m[0][0] * self.t[0] + m[0][1] * self.t[1]), -(m[1][0] * self.t[0] + m[1][1] * self.t[1])];
        Some(Affine2 { m, t })
    }

    pub fn after(&self, o: &Affine2) -> Affine2 {
        let a = &self.m;
        let b = &o.m;
        Affine2 {
            m: [
                [a[0][0] * b[0][0] + a[0][1] * b[1][0], a[0][0] * b[0][1] + a[0][1] * b[1][1]],
                [a[1][0] * b[0][0] + a[1][1] * b[1][0], a[1][0] * b[0][1] + a[1][1] * b[1][1]],
            ],
            t: self.apply(o.t),
        }
    }

    pub fn source_scaled(&self, s: [f64; 2]) -> Affine2 {
        Affine2 {
            m: [[self.m[0][0] * s[0], self.m[0][1] * s[1]], [self.m[1][0] * s[0], self.m[1][1] * s[1]]],
            t: [self.t[0] * s[0], self.t[1] * s[1]],
        }
    }

    pub fn source_unscaled(&self, s: [f64; 2]) -> Option<Affine2> {
        if s[0].abs() < 1e-9 || s[1].abs() < 1e-9 {
            return None;
        }
        Some(self.source_scaled([1.0 / s[0], 1.0 / s[1]]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub center: [f64; 2],
    pub scale: [f64; 2],
    pub rotate: f64,
    pub translate: [f64; 2],
}

impl Default for Transform {
    fn default() -> Self {
        Self { center: [0.5, 0.5], scale: [1.0, 1.0], rotate: 0.0, translate: [0.0, 0.0] }
    }
}

impl Transform {
    pub fn parse(text: &str) -> Transform {
        let source = hp_vmt::TextureTransform::parse(text);
        Self {
            center: source.center.map(f64::from),
            scale: source.scale.map(f64::from),
            rotate: f64::from(source.rotate),
            translate: source.translate.map(f64::from),
        }
    }

    pub fn format(&self) -> String {
        format!(
            "center {} {} scale {} {} rotate {} translate {} {}",
            num_text(self.center[0]),
            num_text(self.center[1]),
            num_text(self.scale[0]),
            num_text(self.scale[1]),
            num_text(self.rotate),
            num_text(self.translate[0]),
            num_text(self.translate[1])
        )
    }

    pub fn affine(&self) -> Affine2 {
        let (s, c) = self.rotate.to_radians().sin_cos();
        let m = [[c * self.scale[0], -s * self.scale[1]], [s * self.scale[0], c * self.scale[1]]];
        let mc =
            [m[0][0] * self.center[0] + m[0][1] * self.center[1], m[1][0] * self.center[0] + m[1][1] * self.center[1]];
        Affine2 { m, t: [self.center[0] + self.translate[0] - mc[0], self.center[1] + self.translate[1] - mc[1]] }
    }

    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        self.affine().apply(p)
    }

    pub fn inverse(&self, q: [f64; 2]) -> Option<[f64; 2]> {
        if self.scale[0].abs() < 1e-9 || self.scale[1].abs() < 1e-9 {
            return None;
        }
        self.affine().inverse().map(|inv| inv.apply(q))
    }

    pub fn from_affine(a: &Affine2, center: [f64; 2], rotate: f64) -> Option<Transform> {
        let (s, c) = rotate.to_radians().sin_cos();
        let sx = c * a.m[0][0] + s * a.m[1][0];
        let sy = -s * a.m[0][1] + c * a.m[1][1];
        if !(sx.is_finite() && sy.is_finite() && sx.abs() > 1e-6 && sy.abs() > 1e-6) {
            return None;
        }
        let mut t = Transform { center, scale: [sx, sy], rotate, translate: [0.0, 0.0] };
        let at = t.affine().t;
        t.translate = [a.t[0] - at[0], a.t[1] - at[1]];
        (t.translate[0].is_finite() && t.translate[1].is_finite()).then_some(t)
    }
}

fn num_text(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_owned();
    if s == "-0" { "0".to_owned() } else { s }
}

pub fn parse_detail_scale(text: Option<&str>) -> [f64; 2] {
    let nums: Vec<f64> = text
        .unwrap_or("")
        .split(|c: char| c.is_whitespace() || matches!(c, '[' | ']' | '{' | '}'))
        .filter_map(|w| w.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .collect();
    match nums.as_slice() {
        [a, b, ..] => [*a, *b],
        [a] => [*a, *a],
        [] => [4.0, 4.0],
    }
}

pub fn format_detail_scale(s: [f64; 2]) -> String {
    if (s[0] - s[1]).abs() < 1e-9 { num_text(s[0]) } else { format!("[{} {}]", num_text(s[0]), num_text(s[1])) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    Lightmapped,
    VertexLit,
    Water,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Support {
    Honoured,
    FollowsBase,
    NotDrawn,
    Unknown,
}

#[derive(Clone, Debug)]
pub(crate) struct LayerModel {
    pub layer: Layer,
    pub shader: String,
    pub family: Family,
    pub support: Support,
    pub key: &'static str,
    pub keeps_fast_path: bool,
    pub texture: Option<String>,
    pub texture_key: Option<&'static str>,
    pub source: Option<String>,
    pub ssbump: bool,
    pub base: Transform,
    pub values: LayerValues,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayerValues {
    pub tr: Transform,
    pub detail_scale: [f64; 2],
}

impl LayerModel {
    pub(crate) fn of(mat: &hp_vmt::Material, layer: Layer) -> LayerModel {
        let base_name = mat.shader_base_name();
        let family = match base_name.as_str() {
            "lightmappedgeneric" | "worldvertextransition" => Family::Lightmapped,
            "vertexlitgeneric" | "unlitgeneric" => Family::VertexLit,
            "water" | "lightmappedreflective" => Family::Water,
            _ => Family::Other,
        };
        let has = |k: &str| mat.get(k).is_some_and(|v| !v.trim().is_empty());
        let on = |k: &str| mat.get(k).is_some_and(|v| v.trim().parse::<f64>().is_ok_and(|n| n != 0.0));
        let identity = |k: &str| mat.get(k).is_none_or(|v| Transform::parse(v) == Transform::default());
        let bump = has("$bumpmap");
        let bump2 = bump && has("$bumpmap2");
        let detail = has("$detail");
        let base2 = has("$basetexture2");
        let bump_mask =
            bump2 && has("$bumpmask") && !on("$selfillum") && !detail && !base2 && !on("$basetexturenoenvmap");
        let masked = on("$maskedblending");
        let fast_path = base2 && has("$blendmodulatetexture") && !masked;
        let own = layer.transform_key();
        let (support, key, keeps_fast_path) = match (family, layer) {
            (_, Layer::Base) => (Support::Honoured, own, false),
            (Family::Lightmapped, Layer::Bump) if bump_mask => (Support::Honoured, "$bumptransform", false),
            (Family::Lightmapped, Layer::Bump2) if !bump => (Support::NotDrawn, "$bumptransform", false),
            (Family::Lightmapped, Layer::Bump2) if bump_mask => (Support::Honoured, "$bumptransform2", false),
            (Family::Lightmapped, Layer::Bump2) if detail => (Support::FollowsBase, "$bumptransform", false),
            (Family::Lightmapped, Layer::Bump2) if fast_path => (Support::FollowsBase, "$bumptransform", true),
            (Family::Lightmapped, Layer::Bump2) => (Support::Honoured, "$bumptransform", false),
            (Family::Lightmapped, Layer::Bump | Layer::Detail | Layer::Base2) => (Support::FollowsBase, own, false),
            (Family::Lightmapped, Layer::EnvMask) if bump2 => (Support::NotDrawn, own, false),
            (Family::Lightmapped, Layer::EnvMask) if fast_path => (Support::FollowsBase, own, true),
            (Family::Lightmapped, Layer::EnvMask) => (Support::Honoured, own, false),
            (Family::Lightmapped, Layer::BlendMod) if !base2 => (Support::NotDrawn, own, false),
            (Family::Lightmapped, Layer::BlendMod) if masked && !identity("$bumptransform") => {
                (Support::Honoured, own, false)
            }
            (Family::Lightmapped, Layer::BlendMod) => (Support::FollowsBase, own, false),
            (Family::VertexLit, Layer::Bump | Layer::EnvMask) => (Support::FollowsBase, own, false),
            (Family::VertexLit, Layer::Detail) | (Family::Water, Layer::Bump) => (Support::Honoured, own, false),
            _ => (Support::Unknown, own, false),
        };
        let (texture_key, texture) = layer
            .texture_keys()
            .iter()
            .find_map(|k| mat.get(k).filter(|v| !v.trim().is_empty()).map(|v| (*k, v.trim().to_owned())))
            .unzip();
        let (source_key, baked_key) = layer.bake_keys();
        let source = mat
            .get(source_key)
            .map(str::trim)
            .filter(|s| !s.is_empty() && layer != Layer::Base)
            .filter(|s| {
                texture.as_deref().is_some_and(|t| {
                    is_baked_name(t) || hp_vmt::normalize_texture_path(t) == hp_vmt::normalize_texture_path(s)
                })
            })
            .map(str::to_owned);
        let base = if layer == Layer::BlendMod {
            Transform::default()
        } else {
            mat.get("$basetexturetransform").map(Transform::parse).unwrap_or_default()
        };
        let tr = match (&source, support) {
            (Some(_), _) => mat.get(baked_key).map(Transform::parse).unwrap_or_default(),
            (None, Support::FollowsBase) => base,
            (None, _) => mat.get(key).map(Transform::parse).unwrap_or_default(),
        };
        LayerModel {
            layer,
            shader: mat.shader.clone(),
            family,
            support,
            key,
            keeps_fast_path,
            texture,
            texture_key,
            source,
            ssbump: on("$ssbump"),
            base,
            values: LayerValues { tr, detail_scale: parse_detail_scale(mat.get("$detailscale")) },
        }
    }

    pub(crate) fn out_scale(&self, v: &LayerValues) -> [f64; 2] {
        if self.layer == Layer::Detail { v.detail_scale } else { [1.0, 1.0] }
    }

    pub(crate) fn written(&self, v: &LayerValues) -> Affine2 {
        v.tr.affine().source_scaled(self.out_scale(v))
    }

    pub(crate) fn game(&self, v: &LayerValues) -> Affine2 {
        match self.support {
            Support::FollowsBase => self.base.affine().source_scaled(self.out_scale(v)),
            _ => self.written(v),
        }
    }

    pub(crate) fn honoured(&self) -> bool {
        matches!(self.support, Support::Honoured | Support::Unknown)
    }

    pub(crate) fn editable(&self) -> bool {
        self.support != Support::NotDrawn
    }

    pub(crate) fn bakes(&self) -> bool {
        self.layer != Layer::Base && self.support == Support::FollowsBase && self.texture.is_some()
    }

    pub(crate) fn edit_texture(&self) -> Option<&str> {
        if self.bakes() { self.source.as_deref().or(self.texture.as_deref()) } else { self.texture.as_deref() }
    }

    pub(crate) fn kind(&self) -> TexelKind {
        match self.layer {
            Layer::Bump if self.ssbump => TexelKind::SsBump,
            Layer::Bump => TexelKind::Normal,
            _ => TexelKind::Color,
        }
    }

    pub(crate) fn bake_map(&self, v: &LayerValues) -> Result<TexelMap, Refusal> {
        let inv = self.game(v).inverse().ok_or(Refusal::Fraction)?;
        let a = self.written(v).after(&inv);
        let map = TexelMap::from_affine(a.m, a.t)?;
        map.check(self.kind())?;
        Ok(map)
    }

    pub(crate) fn solve(&self, a: &Affine2, v: &LayerValues, rotate: f64) -> Option<Transform> {
        let a = a.source_unscaled(self.out_scale(v))?;
        Transform::from_affine(&a, v.tr.center, rotate)
    }

    pub(crate) fn anchored(&self, v: &LayerValues, mut tr: Transform, p: [f64; 2], q: [f64; 2]) -> Transform {
        let w = self.written(&LayerValues { tr, ..*v }).apply(p);
        let s = self.out_scale(v);
        for i in 0..2 {
            if s[i].abs() > 1e-9 {
                tr.translate[i] += (q[i] - w[i]) / s[i];
            }
        }
        tr
    }

    pub(crate) fn snap_translate(
        &self,
        v: &LayerValues,
        from: [f64; 2],
        mut tr: Transform,
        size: [u32; 2],
    ) -> Transform {
        let s = self.out_scale(v);
        for i in 0..2 {
            let n = f64::from(size[i].max(1));
            if s[i].abs() > 1e-9 {
                let d = (tr.translate[i] - from[i]) * s[i];
                tr.translate[i] = from[i] + (d * n).round() / n / s[i];
            }
        }
        tr
    }

    pub(crate) fn support_text(&self) -> String {
        let shader = self.shader.as_str();
        let key = self.key;
        match (self.support, self.family, self.layer) {
            (Support::Honoured, _, Layer::Bump2) if key == "$bumptransform" => tf!("tex.uvl.bump2_ok", shader),
            (Support::Honoured, _, _) => tf!("tex.uvl.ok", shader, key),
            (Support::Unknown, _, _) => tf!("tex.uvl.unknown", shader, key),
            (Support::NotDrawn, _, Layer::EnvMask) => tf!("tex.uvl.envmask_bump2", shader),
            (Support::NotDrawn, _, _) => tf!("tex.uvl.not_drawn", shader),
            (Support::FollowsBase, _, _) if self.keeps_fast_path => tf!("tex.uvl.fast_path", shader, key),
            (Support::FollowsBase, Family::Lightmapped, Layer::Bump) => tf!("tex.uvl.lmg_bump", shader),
            (Support::FollowsBase, Family::Lightmapped, Layer::Bump2) => tf!("tex.uvl.bump2_detail", shader),
            (Support::FollowsBase, _, Layer::Detail) => tf!("tex.uvl.lmg_detail", shader),
            (Support::FollowsBase, _, Layer::Base2) => tf!("tex.uvl.lmg_base2", shader),
            (Support::FollowsBase, _, Layer::BlendMod) => tf!("tex.uvl.blendmod_face", shader),
            (Support::FollowsBase, _, _) => tf!("tex.uvl.follows_base", shader, key),
        }
    }
}

fn is_baked_name(texture: &str) -> bool {
    let t = hp_vmt::normalize_texture_path(texture);
    (t.starts_with("hp_variants/") || t.contains("/hp_variants/"))
        && t.rsplit('/')
            .next()
            .and_then(|leaf| leaf.rsplit_once("_bk"))
            .is_some_and(|(_, h)| h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub(crate) fn refusal_text(r: Refusal) -> String {
    match r {
        Refusal::Angle => t!("tex.uvl.bake_angle").to_owned(),
        Refusal::Fraction => t!("tex.uvl.bake_fraction").to_owned(),
        Refusal::TooMany => tf!("tex.uvl.bake_too_many", hp_vtf::MAX_REPEAT),
        Refusal::SsBump => t!("tex.uvl.bake_ssbump").to_owned(),
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub(crate) fn baked_name(source: &str, kind: TexelKind, map: &TexelMap, content: u64) -> String {
    let key = format!("{source}|{kind:?}|{:?}|{:.6} {:.6}|{content:016x}", map.m, map.t[0], map.t[1]);
    let leaf = source.rsplit('/').next().unwrap_or(source);
    format!("hp_variants/{leaf}_bk{:08x}", fnv(key.as_bytes()) as u32)
}

fn bake_texture(
    state: &AppState,
    model: &LayerModel,
    v: &LayerValues,
    dir: &Path,
    layout: &hp_fs::CustomLayout,
) -> Result<(String, bool), String> {
    let missing = || t!("tex.uvl.no_texture").to_owned();
    let source = model.edit_texture().map(hp_vmt::normalize_texture_path).ok_or_else(missing)?;
    let fs = state.assets.fs().ok_or_else(missing)?;
    let raw = fs.load_texture_raw(&source);
    let mut kind = model.kind();
    if kind == TexelKind::Normal
        && raw.as_deref().and_then(|b| hp_vtf::read_info(b).ok()).is_some_and(|i| i.has_flag(hp_vtf::flags::SSBUMP))
    {
        kind = TexelKind::SsBump;
    }
    let map = model.bake_map(v).and_then(|m| m.check(kind).map(|()| m)).map_err(refusal_text)?;
    if map.is_identity() {
        return Ok((source, false));
    }
    let image = match &raw {
        Some(_) => None,
        None => {
            let tex = fs.load_texture(&source, None).map_err(|e| e.to_string())?;
            Some(hp_vtf::RgbaImage::new(tex.width, tex.height, tex.rgba).map_err(|e| e.to_string())?)
        }
    };
    let content = match (&raw, &image) {
        (Some(b), _) => fnv(b),
        (None, Some(img)) => fnv(&img.pixels),
        (None, None) => 0,
    };
    let (path, name) = crate::folders::asset_target(dir, layout, &baked_name(&source, kind, &map, content), "vtf");
    if path.is_file() {
        return Ok((name, false));
    }
    let bytes = match (raw, image) {
        (Some(b), _) => hp_vtf::remap_vtf(&b, &map, kind),
        (None, Some(img)) => hp_vtf::remap_image(&img, &map, kind).and_then(|out| {
            let flags = if kind == TexelKind::Normal { hp_vtf::flags::NORMAL } else { 0 };
            let opts = hp_vtf::EncodeOptions {
                flags,
                mip_filter: hp_vtf::mip_filter_for(kind),
                ..hp_vtf::EncodeOptions::default()
            };
            hp_vtf::encode(&out, &opts)
        }),
        (None, None) => return Err(missing()),
    }
    .map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok((name, true))
}

fn bake_params(
    state: &AppState,
    model: &LayerModel,
    v: &LayerValues,
    dir: &Path,
    layout: &hp_fs::CustomLayout,
) -> Result<(Vec<(String, String)>, bool), String> {
    let (name, wrote) = bake_texture(state, model, v, dir, layout)?;
    let source = model.edit_texture().map(hp_vmt::normalize_texture_path).unwrap_or_default();
    let key = model.texture_key.unwrap_or(model.layer.texture_keys()[0]);
    let (source_key, baked_key) = model.layer.bake_keys();
    Ok((vec![(key.to_owned(), name), (source_key.to_owned(), source), (baked_key.to_owned(), v.tr.format())], wrote))
}

#[derive(Clone)]
pub(crate) struct LayerTexture {
    pub handle: egui::TextureHandle,
    pub size: [u32; 2],
}

#[derive(Clone, Debug)]
struct Pending {
    disk: LayerValues,
    seen: LayerValues,
    edit: LayerValues,
    undo: Vec<LayerValues>,
    redo: Vec<LayerValues>,
}

type Key = (String, Layer);

pub struct LayerEditor {
    pending: HashMap<Key, Pending>,
    anchor: Option<(Key, LayerValues)>,
    textures: crate::uv_texture::Textures,
    pub(crate) show_base: bool,
    pub(crate) opacity: f32,
    pub(crate) keep_layers: bool,
}

impl Default for LayerEditor {
    fn default() -> Self {
        Self {
            pending: HashMap::new(),
            anchor: None,
            textures: crate::uv_texture::Textures::default(),
            show_base: true,
            opacity: 0.7,
            keep_layers: true,
        }
    }
}

pub(crate) fn resolved_material(state: &AppState, material: &str) -> Option<hp_vmt::Material> {
    let mat = state.assets.fs()?.material(material)?;
    Some(mat.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, state.assets.material_hdr()))
}

pub(crate) fn layers_of(mat: &hp_vmt::Material) -> Vec<Layer> {
    Layer::ALL
        .iter()
        .copied()
        .filter(|l| {
            *l == Layer::Base || l.texture_keys().iter().any(|k| mat.get(k).is_some_and(|v| !v.trim().is_empty()))
        })
        .collect()
}

impl LayerEditor {
    pub(crate) fn sync(&mut self, mat: &hp_vmt::Material, material: &str, layer: Layer) -> (LayerModel, LayerValues) {
        let model = LayerModel::of(mat, layer);
        let key = (material.to_owned(), layer);
        let now = model.values;
        let p = self.pending.entry(key).or_insert_with(|| Pending {
            disk: now,
            seen: now,
            edit: now,
            undo: Vec::new(),
            redo: Vec::new(),
        });
        if p.seen != now {
            p.seen = now;
            if p.edit == p.disk {
                p.edit = now;
            }
            p.disk = now;
        }
        (model, p.edit)
    }

    pub(crate) fn change(&mut self, material: &str, layer: Layer, v: LayerValues) {
        let key = (material.to_owned(), layer);
        let Some(before) = self.pending.get(&key).map(|p| p.edit) else { return };
        if before == v {
            return;
        }
        if self.anchor.as_ref().is_none_or(|(k, _)| *k != key) {
            self.settle(false);
            self.anchor = Some((key.clone(), before));
        }
        if let Some(p) = self.pending.get_mut(&key) {
            p.edit = v;
        }
    }

    pub(crate) fn settle(&mut self, pointer_down: bool) {
        if pointer_down {
            return;
        }
        if let Some((key, before)) = self.anchor.take()
            && let Some(p) = self.pending.get_mut(&key)
            && p.edit != before
        {
            p.undo.push(before);
            if p.undo.len() > 200 {
                p.undo.remove(0);
            }
            p.redo.clear();
        }
    }

    fn undo(&mut self, key: &Key) {
        self.settle(false);
        if let Some(p) = self.pending.get_mut(key)
            && let Some(v) = p.undo.pop()
        {
            p.redo.push(p.edit);
            p.edit = v;
        }
    }

    fn redo(&mut self, key: &Key) {
        self.settle(false);
        if let Some(p) = self.pending.get_mut(key)
            && let Some(v) = p.redo.pop()
        {
            p.undo.push(p.edit);
            p.edit = v;
        }
    }

    pub(crate) fn texture(&mut self, ctx: &egui::Context, state: &AppState, name: &str) -> Option<LayerTexture> {
        let fs = state.assets.fs()?;
        self.textures.get(ctx, fs, state.assets.generation(), name)
    }

    pub(crate) fn panel(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        material: &str,
        model: &LayerModel,
        faces: &[FaceRef],
    ) {
        let layer = model.layer;
        let key = (material.to_owned(), layer);
        let Some(p) = self.pending.get(&key).cloned() else { return };
        let color = if model.honoured() { ui.visuals().weak_text_color() } else { ui.visuals().warn_fg_color };
        ui.label(RichText::new(model.support_text()).small().color(color));
        if model.bakes() {
            ui.label(RichText::new(t!("tex.uvl.bake_note")).small());
        }
        let note = if layer == Layer::Base2 { t!("tex.uvl.base2_3d") } else { t!("tex.uvl.no_3d") };
        ui.label(RichText::new(note).small().weak());
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.show_base, t!("tex.uvl.show_base"));
            if self.show_base {
                ui.add(egui::Slider::new(&mut self.opacity, 0.1..=1.0).show_value(false))
                    .on_hover_text(t!("tex.uvl.opacity"));
            }
        });
        let mut v = p.edit;
        egui::Grid::new("hp_uv_layer_grid").num_columns(3).show(ui, |ui| {
            ui.label(t!("tex.uvl.scale"));
            ui.add(egui::DragValue::new(&mut v.tr.scale[0]).speed(0.01).prefix("U "));
            ui.add(egui::DragValue::new(&mut v.tr.scale[1]).speed(0.01).prefix("V "));
            ui.end_row();
            ui.label(t!("tex.uvl.shift"));
            ui.add(egui::DragValue::new(&mut v.tr.translate[0]).speed(0.005).prefix("U "));
            ui.add(egui::DragValue::new(&mut v.tr.translate[1]).speed(0.005).prefix("V "));
            ui.end_row();
            ui.label(t!("tex.uvl.rotate"));
            ui.add(egui::DragValue::new(&mut v.tr.rotate).speed(1.0).suffix(" deg"));
            ui.end_row();
            ui.label(t!("tex.uvl.center"));
            ui.add(egui::DragValue::new(&mut v.tr.center[0]).speed(0.005).prefix("U "));
            ui.add(egui::DragValue::new(&mut v.tr.center[1]).speed(0.005).prefix("V "));
            ui.end_row();
            if layer == Layer::Detail {
                ui.label(t!("tex.uvl.detail_scale")).on_hover_text(t!("tex.uvl.detail_scale_tip"));
                ui.add(egui::DragValue::new(&mut v.detail_scale[0]).speed(0.05).prefix("U "));
                ui.add(egui::DragValue::new(&mut v.detail_scale[1]).speed(0.05).prefix("V "));
                ui.end_row();
            }
        });
        let finite = |x: f64| x.is_finite();
        if v != p.edit
            && [v.tr.scale, v.tr.translate, v.tr.center, v.detail_scale].iter().flatten().copied().all(finite)
            && v.tr.rotate.is_finite()
        {
            self.change(material, layer, v);
        }
        let p = self.pending.get(&key).cloned().unwrap_or(p);
        let changes = changes(&p.disk, &p.edit, layer, model.key);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!p.undo.is_empty(), egui::Button::new(t!("tex.uvl.undo")).small()).clicked() {
                self.undo(&key);
            }
            if ui.add_enabled(!p.redo.is_empty(), egui::Button::new(t!("tex.uvl.redo")).small()).clicked() {
                self.redo(&key);
            }
            if ui
                .add_enabled(!changes.is_empty(), egui::Button::new(t!("tex.uvl.revert")).small())
                .on_hover_text(t!("tex.uvl.revert_tip"))
                .clicked()
            {
                let disk = p.disk;
                self.change(material, layer, disk);
                self.settle(false);
            }
            if !changes.is_empty() {
                ui.label(RichText::new(t!("tex.uvl.pending")).small().color(ui.visuals().warn_fg_color));
            }
        });
        let refusal = if model.bakes() { model.bake_map(&p.edit).err().map(refusal_text) } else { None };
        if let Some(r) = &refusal {
            ui.label(RichText::new(r).small().color(ui.visuals().error_fg_color));
        }
        ui.label(RichText::new(t!("tex.uvl.apply_to")).small().strong());
        ui.horizontal_wrapped(|ui| {
            let ok = refusal.is_none();
            let faces_tip =
                if model.bakes() { t!("tex.uvl.apply_faces_bake_tip") } else { t!("tex.uvl.apply_faces_tip") };
            if ui
                .add_enabled(ok, egui::Button::new(tf!("tex.uvl.apply_faces", faces.len())))
                .on_hover_text(faces_tip)
                .clicked()
                && apply_variant(state, layer, &p.disk, &p.edit, faces)
            {
                self.settle(false);
                self.pending.remove(&key);
                self.shown_result(model.bakes());
            }
            let material_tip =
                if model.bakes() { t!("tex.uvl.apply_material_bake_tip") } else { t!("tex.uvl.apply_material_tip") };
            if ui
                .add_enabled(ok && !changes.is_empty(), egui::Button::new(t!("tex.uvl.apply_material")))
                .on_hover_text(material_tip)
                .clicked()
                && apply_material(state, material, model, &p.edit, &changes)
            {
                self.applied(&key, model.bakes());
            }
        });
    }

    fn applied(&mut self, key: &Key, baked: bool) {
        self.settle(false);
        if let Some(p) = self.pending.get_mut(key) {
            p.disk = p.edit;
        }
        self.shown_result(baked);
    }

    fn shown_result(&mut self, baked: bool) {
        if baked {
            self.textures.clear();
        }
    }
}

fn changes(disk: &LayerValues, edit: &LayerValues, layer: Layer, key: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if edit.tr != disk.tr {
        out.push((key.to_owned(), edit.tr.format()));
    }
    if layer == Layer::Detail && edit.detail_scale != disk.detail_scale {
        out.push(("$detailscale".to_owned(), format_detail_scale(edit.detail_scale)));
    }
    out
}

fn variant_origin(state: &AppState, material: &str) -> (String, Vec<(String, String)>) {
    if is_variant_name(material)
        && let Some(text) = state.assets.fs().and_then(|fs| fs.read_text(&format!("materials/{material}.vmt")))
        && let Ok(m) = hp_vmt::parse(&text)
        && m.shader.eq_ignore_ascii_case("patch")
        && let Some(include) = m.get("include")
    {
        let origin = hp_vmt::normalize_material_name(include);
        let keys = m
            .other_blocks
            .iter()
            .filter(|b| b.name.eq_ignore_ascii_case("insert") || b.name.eq_ignore_ascii_case("replace"))
            .flat_map(|b| b.props.iter().cloned())
            .fold(Vec::<(String, String)>::new(), |mut acc, (k, v)| {
                merge_param(&mut acc, &k, &v);
                acc
            });
        if !origin.is_empty() {
            return (origin, keys);
        }
    }
    (material.to_owned(), Vec::new())
}

fn merge_param(list: &mut Vec<(String, String)>, key: &str, value: &str) {
    match list.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
        Some(slot) => value.clone_into(&mut slot.1),
        None => list.push((key.to_owned(), value.to_owned())),
    }
}

pub(crate) fn variant_text(origin: &str, params: &[(String, String)], origin_mat: Option<&hp_vmt::Material>) -> String {
    fn nested(block: &hp_vmt::Block, params: &[(String, String)]) -> Option<hp_vmt::Block> {
        let mut out = hp_vmt::Block::new(block.name.clone());
        out.props = params.iter().filter(|(k, _)| block.get(k).is_some()).cloned().collect();
        out.children = block.children.iter().filter_map(|c| nested(c, params)).collect();
        (!out.props.is_empty() || !out.children.is_empty()).then_some(out)
    }
    fn emit(lines: &mut Vec<String>, block: &hp_vmt::Block, depth: usize) {
        let tab = "\t".repeat(depth);
        lines.push(format!("{tab}\"{}\"", block.name));
        lines.push(format!("{tab}{{"));
        for (k, v) in &block.props {
            lines.push(format!("{tab}\t\"{k}\" \"{v}\""));
        }
        for c in &block.children {
            emit(lines, c, depth + 1);
        }
        lines.push(format!("{tab}}}"));
    }
    let sections: Vec<hp_vmt::Block> = origin_mat
        .map(|m| {
            m.other_blocks
                .iter()
                .filter(|b| !["insert", "replace", "proxies"].iter().any(|n| b.name.eq_ignore_ascii_case(n)))
                .filter_map(|b| nested(b, params))
                .collect()
        })
        .unwrap_or_default();
    let mut lines = vec!["\"patch\"".to_owned(), "{".to_owned(), format!("\t\"include\" \"materials/{origin}.vmt\"")];
    for name in ["insert", "replace"] {
        let mut block = hp_vmt::Block::new(name);
        block.props = params.to_vec();
        if name == "insert" {
            block.children.clone_from(&sections);
        }
        emit(&mut lines, &block, 1);
    }
    lines.push("}".to_owned());
    lines.push(String::new());
    lines.join("\n")
}

pub(crate) fn variant_name(origin: &str, params: &[(String, String)]) -> String {
    let mut sorted: Vec<String> = params.iter().map(|(k, v)| format!("{}={v}", k.to_ascii_lowercase())).collect();
    sorted.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in format!("{origin}|{}", sorted.join("|")).bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    let leaf = origin.rsplit('/').next().unwrap_or(origin);
    format!("hp_variants/{leaf}_{:08x}", h as u32)
}

fn custom_target_of(state: &AppState) -> Result<(std::path::PathBuf, hp_fs::CustomLayout), String> {
    state
        .settings
        .game
        .custom_texture_dirs
        .iter()
        .filter(|dir| dir.enabled)
        .find_map(|dir| {
            let layout = state.assets.fs().and_then(|fs| fs.custom_layout_of_path(&dir.path))?;
            Some((dir.path.clone(), layout))
        })
        .ok_or_else(|| t!("tex.uvl.no_custom").to_owned())
}

fn layer_params(
    state: &AppState,
    material: &str,
    layer: Layer,
    disk: &LayerValues,
    edit: &LayerValues,
    target: &(std::path::PathBuf, hp_fs::CustomLayout),
) -> Result<(Vec<(String, String)>, bool), String> {
    let model = resolved_material(state, material).map(|m| LayerModel::of(&m, layer));
    let key = model.as_ref().map_or(layer.transform_key(), |m| m.key);
    let mut params = changes(disk, edit, layer, key);
    match model.filter(LayerModel::bakes) {
        Some(model) => {
            params.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
            let (baked, wrote) = bake_params(state, &model, edit, &target.0, &target.1)
                .map_err(|e| tf!("tex.uvl.bake_failed", material, e))?;
            params.extend(baked);
            Ok((params, wrote))
        }
        None => {
            if !params.iter().any(|(k, _)| k.eq_ignore_ascii_case(key)) {
                params.push((key.to_owned(), edit.tr.format()));
            }
            Ok((params, false))
        }
    }
}

fn apply_variant(
    state: &mut AppState,
    layer: Layer,
    disk: &LayerValues,
    edit: &LayerValues,
    faces: &[FaceRef],
) -> bool {
    match variant_core(state, layer, disk, edit, faces, "Layer variant", None) {
        Ok(done) => {
            state.notify(Severity::Success, tf!("tex.uvl.variant_done", done.name, done.faces));
            true
        }
        Err(e) => {
            if let Some(e) = e {
                state.notify(Severity::Error, e);
            }
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantDone {
    pub name: String,
    pub variants: Vec<String>,
    pub faces: usize,
    pub baked: bool,
}

fn variant_core(
    state: &mut AppState,
    layer: Layer,
    disk: &LayerValues,
    edit: &LayerValues,
    faces: &[FaceRef],
    label: &str,
    merge: Option<&str>,
) -> Result<VariantDone, Option<String>> {
    let target = custom_target_of(state).map_err(Some)?;
    let (dir_path, layout) = target.clone();
    let mut groups: Vec<(String, Vec<FaceRef>)> = Vec::new();
    for f in faces {
        let Some(side) = state.doc.face_side(*f) else { continue };
        let m = crate::util::normalize_material(&side.material);
        match groups.iter_mut().find(|(g, _)| *g == m) {
            Some((_, list)) => list.push(*f),
            None => groups.push((m, vec![*f])),
        }
    }
    let mut made: Vec<(String, Vec<FaceRef>)> = Vec::new();
    let mut baked = false;
    for (material, list) in groups {
        let params = match layer_params(state, &material, layer, disk, edit, &target) {
            Ok((params, wrote)) => {
                baked |= wrote;
                params
            }
            Err(e) => {
                if let Some(fs) = state.assets.fs() {
                    fs.refresh();
                }
                return Err(Some(e));
            }
        };
        let (origin, mut all) = variant_origin(state, &material);
        for (k, v) in &params {
            merge_param(&mut all, k, v);
        }
        let (path, new_name) = crate::folders::vmt_target(&dir_path, &layout, &variant_name(&origin, &all));
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return Err(Some(tf!("tex.me.failed", new_name, e)));
        }
        let origin_mat = state.assets.fs().and_then(|fs| fs.material(&origin));
        if let Err(e) = std::fs::write(&path, variant_text(&origin, &all, origin_mat.as_deref())) {
            return Err(Some(tf!("tex.me.failed", new_name, e)));
        }
        made.push((new_name, list));
    }
    if let Some(fs) = state.assets.fs() {
        fs.refresh();
    }
    if made.is_empty() {
        return Err(None);
    }
    match merge {
        Some(tag) => state.doc.begin_merge(label, tag),
        None => state.doc.begin(label),
    }
    let mut count = 0;
    let mut failed = None;
    for (name, list) in &made {
        state.assets.invalidate_material(name);
        match state.doc.apply_material(list, name) {
            Ok(_) => count += list.len(),
            Err(e) => {
                failed = Some(e);
                break;
            }
        }
    }
    state.doc.commit();
    if let Some(e) = failed {
        return Err(Some(tf!("tex.tool.apply_failed", e)));
    }
    Ok(VariantDone {
        name: made[0].0.clone(),
        variants: made.iter().map(|(n, _)| n.clone()).collect(),
        faces: count,
        baked,
    })
}

fn apply_material(
    state: &mut AppState,
    material: &str,
    model: &LayerModel,
    edit: &LayerValues,
    changes: &[(String, String)],
) -> bool {
    match material_core(state, material, model, edit, changes) {
        Ok(done) => {
            if let Some(b) = &done.backup {
                state.log(Severity::Info, tf!("tex.me.backup_made", b.display()));
            }
            state.notify(Severity::Success, tf!("tex.uvl.material_done", material));
            true
        }
        Err(e) => {
            state.notify(Severity::Error, e);
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialDone {
    pub file: std::path::PathBuf,
    pub backup: Option<std::path::PathBuf>,
    pub params: Vec<(String, String)>,
    pub baked: bool,
}

fn material_core(
    state: &mut AppState,
    material: &str,
    model: &LayerModel,
    edit: &LayerValues,
    changes: &[(String, String)],
) -> Result<MaterialDone, String> {
    let vpath = format!("materials/{material}.vmt");
    let Some((path, mut text)) = state.assets.fs().and_then(|fs| Some((fs.disk_path(&vpath)?, fs.read_text(&vpath)?)))
    else {
        return Err(t!("tex.uvl.not_loose").to_owned());
    };
    let mut params = changes.to_vec();
    let mut wrote = false;
    if model.bakes() {
        let target = custom_target_of(state)?;
        params.retain(|(k, _)| !k.eq_ignore_ascii_case(model.key));
        let (baked, w) = bake_params(state, model, edit, &target.0, &target.1)
            .map_err(|e| tf!("tex.uvl.bake_failed", material, e))?;
        params.extend(baked);
        wrote = w;
    }
    let resolved = state.assets.fs().and_then(|fs| fs.material(material));
    for (k, v) in &params {
        let set = match &resolved {
            Some(resolved) => hp_vmt::set_param_with_fallbacks_in_text(&text, k, v, resolved),
            None => hp_vmt::set_param_everywhere_in_text(&text, k, v),
        };
        text = set.map_err(|e| tf!("tex.me.failed", material, e))?;
    }
    let w = crate::matedit::write_with_backup(&path, &text).map_err(|e| tf!("tex.me.failed", material, e))?;
    if let Some(fs) = state.assets.fs() {
        if wrote {
            fs.refresh();
        } else if let Some(id) = fs.material_source(material) {
            fs.refresh_sources(&[id]);
        }
    }
    state.assets.invalidate_material(material);
    Ok(MaterialDone { file: path, backup: w.backup, params, baked: wrote })
}

impl LayerEditor {
    pub(crate) fn keep_in_place(
        &mut self,
        state: &mut AppState,
        face: FaceRef,
        a: &Affine2,
        tag: &str,
    ) -> (Vec<Layer>, Vec<(Layer, String)>) {
        let (mut kept, mut failed) = (Vec::new(), Vec::new());
        let Some(inv) = a.inverse().filter(|_| self.keep_layers) else { return (kept, failed) };
        if (0..2).all(|i| (0..2).all(|j| (a.m[i][j] - Affine2::IDENTITY.m[i][j]).abs() < 1e-9) && a.t[i].abs() < 1e-9) {
            return (kept, failed);
        }
        let Some(material) = state.doc.face_side(face).map(|s| crate::util::normalize_material(&s.material)) else {
            return (kept, failed);
        };
        let Some(mat) = resolved_material(state, &material) else { return (kept, failed) };
        for layer in layers_of(&mat) {
            let model = LayerModel::of(&mat, layer);
            if layer == Layer::Base || !model.editable() {
                continue;
            }
            let key = (material.clone(), layer);
            let waiting = self.pending.get(&key).filter(|p| p.edit != p.disk).map(|p| p.edit);
            let applied = model.source.is_some() || (is_variant_name(&material) && model.values.tr != model.base);
            let from = match (waiting, applied) {
                (Some(v), _) => v,
                (None, true) => model.values,
                (None, false) => continue,
            };
            let Some(to) = compensated(&model, &from, &inv) else { continue };
            if waiting.is_some() {
                self.settle(false);
                if let Some(p) = self.pending.get_mut(&key) {
                    p.undo.push(p.edit);
                    p.redo.clear();
                    p.edit = to;
                }
                kept.push(layer);
                continue;
            }
            if model.bakes()
                && let Err(r) = model.bake_map(&to)
            {
                failed.push((layer, refusal_text(r)));
                continue;
            }
            match variant_core(state, layer, &model.values, &to, &[face], "Layer variant", Some(tag)) {
                Ok(_) => kept.push(layer),
                Err(Some(e)) => failed.push((layer, e)),
                Err(None) => {}
            }
        }
        (kept, failed)
    }
}

fn is_variant_name(material: &str) -> bool {
    let m = hp_vmt::normalize_material_name(material);
    m.starts_with("hp_variants/") || m.contains("/hp_variants/")
}

fn compensated(model: &LayerModel, from: &LayerValues, inv: &Affine2) -> Option<LayerValues> {
    let w = model.written(from).after(inv);
    let u = w.source_unscaled(model.out_scale(from))?;
    let angle = u.m[1][0].atan2(u.m[0][0]).to_degrees();
    let close = |a: &Affine2, b: &Affine2| {
        (0..2).all(|i| (0..2).all(|j| (a.m[i][j] - b.m[i][j]).abs() < 1e-6) && (a.t[i] - b.t[i]).abs() < 1e-6)
    };
    [from.tr.rotate, angle, angle + 180.0]
        .into_iter()
        .map(|r| {
            let r = r.rem_euclid(360.0);
            if r > 180.0 { r - 360.0 } else { r }
        })
        .filter_map(|r| Transform::from_affine(&u, from.tr.center, r))
        .find(|t| close(&t.affine(), &u))
        .map(|tr| LayerValues { tr, ..*from })
}

pub(crate) fn wraps_cleanly(state: &AppState, material: &str) -> bool {
    let Some(mat) = resolved_material(state, material) else { return true };
    layers_of(&mat).into_iter().all(|layer| {
        let model = LayerModel::of(&mat, layer);
        if !model.editable() {
            return true;
        }
        let a = model.game(&model.values);
        a.m.iter().flatten().all(|v| (v - v.round()).abs() < 1e-6)
    })
}

impl Layer {
    pub fn mcp_name(self) -> &'static str {
        match self {
            Layer::Base => "base",
            Layer::Bump => "bump",
            Layer::Detail => "detail",
            Layer::Base2 => "base2",
            Layer::Bump2 => "bump2",
            Layer::EnvMask => "envmask",
            Layer::BlendMod => "blendmod",
        }
    }

    pub fn from_mcp_name(name: &str) -> Option<Layer> {
        Layer::ALL.into_iter().find(|l| l.mcp_name() == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerReport {
    pub layer: Layer,
    pub shader: String,
    pub support: &'static str,
    pub support_text: String,
    pub texture: Option<String>,
    pub texture_key: Option<&'static str>,
    pub source: Option<String>,
    pub transform_key: &'static str,
    pub transform: Transform,
    pub detail_scale: [f64; 2],
    pub bakes: bool,
}

pub fn mcp_layers(state: &AppState, material: &str) -> Option<Vec<LayerReport>> {
    let mat = resolved_material(state, material)?;
    Some(
        layers_of(&mat)
            .into_iter()
            .map(|layer| {
                let m = LayerModel::of(&mat, layer);
                LayerReport {
                    layer,
                    shader: m.shader.clone(),
                    support: match m.support {
                        Support::Honoured => "honoured",
                        Support::FollowsBase => "follows_base",
                        Support::NotDrawn => "not_drawn",
                        Support::Unknown => "unknown",
                    },
                    support_text: if layer == Layer::Base { String::new() } else { m.support_text() },
                    texture: m.texture.clone(),
                    texture_key: m.texture_key,
                    source: m.source.clone(),
                    transform_key: m.key,
                    transform: m.values.tr,
                    detail_scale: m.values.detail_scale,
                    bakes: m.bakes(),
                }
            })
            .collect(),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayerEdit {
    pub center: Option<[f64; 2]>,
    pub scale: Option<[f64; 2]>,
    pub rotate: Option<f64>,
    pub translate: Option<[f64; 2]>,
    pub detail_scale: Option<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerTarget {
    Faces(Vec<FaceRef>),
    Material,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerApplied {
    Variants(VariantDone),
    Material(MaterialDone),
}

pub fn mcp_apply_layer(
    state: &mut AppState,
    material: &str,
    layer: Layer,
    edit: &LayerEdit,
    target: &LayerTarget,
    label: &str,
) -> Result<LayerApplied, String> {
    if layer == Layer::Base {
        return Err("the base layer is the face mapping itself: edit it with the face texture tools".to_owned());
    }
    let mat =
        resolved_material(state, material).ok_or_else(|| format!("no material {material} in the game content"))?;
    if !layers_of(&mat).contains(&layer) {
        return Err(format!("{material} has no {} layer ({} is not set)", layer.mcp_name(), layer.texture_keys()[0]));
    }
    let model = LayerModel::of(&mat, layer);
    let disk = model.values;
    let mut v = disk;
    if let Some(c) = edit.center {
        v.tr.center = c;
    }
    if let Some(s) = edit.scale {
        v.tr.scale = s;
    }
    if let Some(r) = edit.rotate {
        v.tr.rotate = r;
    }
    if let Some(t) = edit.translate {
        v.tr.translate = t;
    }
    if let Some(d) = edit.detail_scale {
        if layer != Layer::Detail {
            return Err("detail_scale is for the detail layer".to_owned());
        }
        v.detail_scale = d;
    }
    let finite = [v.tr.center, v.tr.scale, v.tr.translate, v.detail_scale].iter().flatten().all(|x| x.is_finite());
    if !finite || !v.tr.rotate.is_finite() || v.tr.scale.iter().any(|s| s.abs() < 1e-6) {
        return Err("the values must be finite and the scale not zero".to_owned());
    }
    if model.bakes() {
        model.bake_map(&v).map_err(refusal_text)?;
    }
    match target {
        LayerTarget::Faces(faces) => {
            if faces.is_empty() {
                return Err("no face to apply to".to_owned());
            }
            if state.doc.in_transaction() {
                return Err("another edit is in progress".to_owned());
            }
            variant_core(state, layer, &disk, &v, faces, label, None)
                .map(LayerApplied::Variants)
                .map_err(|e| e.unwrap_or_else(|| "the faces do not exist".to_owned()))
        }
        LayerTarget::Material => {
            let changes = changes(&disk, &v, layer, model.key);
            if changes.is_empty() && !model.bakes() {
                return Err("nothing changes: the material has these values already".to_owned());
            }
            material_core(state, material, &model, &v, &changes).map(LayerApplied::Material)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn transforms_parse_format_and_invert() {
        let t = Transform::parse("center .5 .5 scale 2 1 rotate 90 translate 0.25 0");
        assert_eq!(t.scale, [2.0, 1.0]);
        assert_eq!(t.rotate, 90.0);
        let p = [0.3, 0.7];
        let q = t.apply(p);
        let back = t.inverse(q).unwrap();
        assert!(close(back, p));
        assert_eq!(Transform::parse(&t.format()), t);
        assert_eq!(
            Transform::parse("scale 3"),
            Transform::default(),
            "the native parser ignores incomplete expressions"
        );
        assert_eq!(
            Transform::parse("center .5 .5 scale 3 3 rotate 0 translate 0 0"),
            Transform { scale: [3.0, 3.0], ..Transform::default() }
        );
    }

    #[test]
    fn affine_matches_the_engine_matrix_and_solves_back() {
        let t = Transform { center: [0.25, 0.5], scale: [2.0, 0.5], rotate: 30.0, translate: [0.1, -0.2] };
        let p = [0.7, 0.2];
        let (s, c) = 30f64.to_radians().sin_cos();
        let (x, y) = ((p[0] - 0.25) * 2.0, (p[1] - 0.5) * 0.5);
        let want = [c * x - s * y + 0.25 + 0.1, s * x + c * y + 0.5 - 0.2];
        assert!(close(t.apply(p), want));
        let a = t.affine();
        assert!(close(a.inverse().unwrap().apply(a.apply(p)), p));
        let back = Transform::from_affine(&a, t.center, t.rotate).unwrap();
        assert!(close(back.scale, t.scale) && close(back.translate, t.translate));
        let d = a.source_scaled([4.0, 2.0]);
        assert!(close(d.source_unscaled([4.0, 2.0]).unwrap().apply(p), a.apply(p)));
        assert!(close(Transform::default().affine().source_scaled([4.0, 4.0]).apply(p), [p[0] * 4.0, p[1] * 4.0]));
    }

    #[test]
    fn detail_scale_values() {
        assert_eq!(parse_detail_scale(Some("4")), [4.0, 4.0]);
        assert_eq!(parse_detail_scale(Some("[2 3]")), [2.0, 3.0]);
        assert_eq!(parse_detail_scale(None), [4.0, 4.0]);
        assert_eq!(format_detail_scale([4.0, 4.0]), "4");
        assert_eq!(format_detail_scale([2.0, 3.5]), "[2 3.5]");
    }

    #[test]
    fn editing_material_uvs_updates_dx_hdr_overrides_and_preserves_the_source_backup() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/uv-layer-overrides").join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ));
        std::fs::create_dir_all(root.join("materials/fixture")).unwrap();
        let source = r#"// authored UVs
VertexLitGeneric {
    "$basetexture" "fixture/color" "$detail" "fixture/detail"
    "$basetexturetransform" "scale 1 1" "$detailtexturetransform" "scale 1 1" "$detailscale" "4"
    VertexLitGeneric_DX8 { "$basetexturetransform" "scale 2 2" "$detailtexturetransform" "scale 2 2" "$detailscale" "8" }
    VertexLitGeneric_DX9 { // preserve this comment
        "$basetexturetransform" "scale 3 3" "$detailtexturetransform" "scale 3 3" "$detailscale" "12"
    }
    VertexLitGeneric_DX9_HDR { "$basetexturetransform" "scale 5 5" "$detailtexturetransform" "scale 5 5" "$detailscale" "20" }
    Proxies { TextureScroll { textureScrollVar "$basetexturetransform" textureScrollRate 0.2 } }
}"#;
        for (index, (layer, patch)) in
            [(Layer::Base, false), (Layer::Detail, false), (Layer::Base, true), (Layer::Detail, true)]
                .into_iter()
                .enumerate()
        {
            let name = format!("fixture/edited{index}");
            let path = root.join(format!("materials/{name}.vmt"));
            let base = root.join(format!("materials/fixture/base{index}.vmt"));
            let authored = if patch {
                std::fs::write(&base, source).unwrap();
                format!(
                    r#"// authored UVs
Patch {{
    include "materials/fixture/base{index}.vmt"
    replace {{ // preserve this comment
        "$basetexturetransform" "scale 11 11"
        VertexLitGeneric_DX9 {{ "$detailtexturetransform" "scale 13 13" "$detailscale" "24" }}
    }}
}}"#
                )
            } else {
                source.to_owned()
            };
            std::fs::write(&path, &authored).unwrap();
            let fs = hp_fs::GameFs::empty();
            fs.add_game_dir(&root).unwrap();
            let mut state = hp_app::testing::test_state();
            state.assets = Box::new(hp_app::testing::MockAssets::with_fs(fs));
            let model = LayerModel::of(&resolved_material(&state, &name).unwrap(), layer);
            assert!(!model.bakes());
            let edit = LayerValues {
                tr: Transform { scale: [7.0, 9.0], translate: [0.25, -0.5], ..Default::default() },
                detail_scale: [16.0, 16.0],
            };
            let changes = changes(&model.values, &edit, layer, model.key);
            let done = material_core(&mut state, &name, &model, &edit, &changes).unwrap();
            let material = state.assets.fs().unwrap().material(&name).unwrap();
            for (dx, hdr) in [(80, false), (95, false), (95, true)] {
                let effective = material.resolve_fallbacks(dx, hdr);
                for (key, value) in &changes {
                    assert_eq!(effective.get(key), Some(value.as_str()), "{layer:?} DX{dx} HDR{hdr}: {key}");
                }
            }
            assert_eq!(std::fs::read_to_string(done.backup.unwrap()).unwrap(), authored);
            let saved = std::fs::read_to_string(path).unwrap();
            assert!(saved.contains("// authored UVs") && saved.contains("// preserve this comment"));
            assert!(material.proxies.is_some());
            if patch {
                assert_eq!(std::fs::read_to_string(base).unwrap(), source, "included materials are never edited");
            } else {
                assert!(saved.contains("textureScrollRate 0.2"));
            }
        }
    }

    fn mat(text: &str) -> hp_vmt::Material {
        hp_vmt::parse(text).unwrap().resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false)
    }

    #[test]
    fn shaders_decide_which_transforms_count() {
        let lmg = mat(
            r#""LightmappedGeneric" { "$basetexture" "a/b" "LightmappedGeneric_DX9" { "$bumpmap" "a/b_normal" } "$detail" "d/d" }"#,
        );
        assert_eq!(layers_of(&lmg), vec![Layer::Base, Layer::Bump, Layer::Detail]);
        let bump = LayerModel::of(&lmg, Layer::Bump);
        assert_eq!(bump.support, Support::FollowsBase);
        assert_eq!(bump.texture.as_deref(), Some("a/b_normal"));
        let v = LayerValues { tr: Transform { scale: [2.0, 2.0], ..Transform::default() }, ..bump.values };
        assert_eq!(bump.game(&v), Affine2::IDENTITY);
        assert_ne!(bump.written(&v), Affine2::IDENTITY);
        let detail = LayerModel::of(&lmg, Layer::Detail);
        assert_eq!(detail.support, Support::FollowsBase);
        assert!(close(detail.game(&detail.values).apply([0.5, 0.25]), [2.0, 1.0]));
        let vlg = mat(r#""VertexLitGeneric" { "$basetexture" "a" "$detail" "d" "$detailscale" "2" }"#);
        let d = LayerModel::of(&vlg, Layer::Detail);
        assert_eq!(d.support, Support::Honoured);
        let v = LayerValues { tr: Transform { translate: [0.25, 0.0], ..Transform::default() }, ..d.values };
        assert!(close(d.game(&v).apply([0.0, 0.0]), [0.5, 0.0]));
        let water = mat(r#""Water" { "$normalmap" "w/n" }"#);
        assert_eq!(LayerModel::of(&water, Layer::Bump).support, Support::Honoured);
        let mask = mat(r#""LightmappedGeneric" { "$basetexture" "a" "$envmapmask" "m" }"#);
        assert_eq!(LayerModel::of(&mask, Layer::EnvMask).support, Support::Honoured);
        let wvt = mat(
            r#""WorldVertexTransition" { "$basetexture" "a" "$basetexture2" "b" "$bumpmap" "n" "$bumpmap2" "n2" "$envmapmask" "m" }"#,
        );
        assert_eq!(LayerModel::of(&wvt, Layer::Base2).support, Support::FollowsBase);
        assert_eq!(LayerModel::of(&wvt, Layer::EnvMask).support, Support::NotDrawn);
    }

    #[test]
    fn solving_and_anchoring_keep_points() {
        let m = mat(r#""VertexLitGeneric" { "$basetexture" "a" "$detail" "d" "$detailscale" "[4 2]" }"#);
        let model = LayerModel::of(&m, Layer::Detail);
        let v =
            LayerValues { tr: Transform { rotate: 90.0, scale: [2.0, 1.0], ..Transform::default() }, ..model.values };
        let w = model.written(&v);
        let back = model.solve(&w, &v, 90.0).unwrap();
        assert!(close(back.scale, v.tr.scale) && close(back.translate, v.tr.translate));
        let p = [0.3, 0.6];
        let moved = model.anchored(&v, v.tr, p, [w.apply(p)[0] + 0.5 / 256.0, w.apply(p)[1]]);
        let snapped = model.snap_translate(&v, v.tr.translate, moved, [256, 256]);
        let d = model.written(&LayerValues { tr: snapped, ..v }).apply(p);
        let texels = (d[0] - w.apply(p)[0]) * 256.0;
        assert!((texels - texels.round()).abs() < 1e-6);
        let a = model.anchored(&v, Transform { scale: [3.0, 3.0], ..v.tr }, p, w.apply(p));
        assert!(close(model.written(&LayerValues { tr: a, ..v }).apply(p), w.apply(p)));
    }

    #[test]
    fn ignored_transforms_are_baked() {
        let lmg = mat(
            r#""LightmappedGeneric" { "$basetexture" "a/b" "LightmappedGeneric_DX9" { "$bumpmap" "a/b_normal" } "$detail" "d/d" "$detailscale" "4" }"#,
        );
        let bump = LayerModel::of(&lmg, Layer::Bump);
        assert!(bump.bakes());
        assert_eq!(bump.kind(), TexelKind::Normal);
        assert_eq!(bump.texture_key, Some("$bumpmap"));
        assert_eq!(bump.edit_texture(), Some("a/b_normal"));
        let with = |tr: Transform| LayerValues { tr, ..bump.values };
        assert!(bump.bake_map(&bump.values).unwrap().is_identity());
        let q = bump.bake_map(&with(Transform { rotate: 90.0, ..Transform::default() })).unwrap();
        assert_eq!(q.m, [[0, -1], [1, 0]]);
        assert!(q.t[0].abs() < 1e-9 && (q.t[1]).abs() < 1e-9, "{:?}", q.t);
        let r = bump.bake_map(&with(Transform { scale: [2.0, -1.0], translate: [0.25, 0.0], ..Transform::default() }));
        assert_eq!(r.unwrap().m, [[2, 0], [0, -1]]);
        assert_eq!(
            bump.bake_map(&with(Transform { scale: [1.5, 1.0], ..Transform::default() })),
            Err(Refusal::Fraction)
        );
        assert_eq!(bump.bake_map(&with(Transform { rotate: 30.0, ..Transform::default() })), Err(Refusal::Angle));
        let detail = LayerModel::of(&lmg, Layer::Detail);
        assert!(detail.bakes());
        let d = detail
            .bake_map(&LayerValues { tr: Transform { scale: [2.0, 2.0], ..Transform::default() }, ..detail.values });
        assert_eq!(d.unwrap().m, [[2, 0], [0, 2]]);
        let ss = mat(r#""LightmappedGeneric" { "$basetexture" "a" "$bumpmap" "n" "$ssbump" "1" }"#);
        let ssm = LayerModel::of(&ss, Layer::Bump);
        assert_eq!(ssm.kind(), TexelKind::SsBump);
        let turn = LayerValues { tr: Transform { rotate: 90.0, ..Transform::default() }, ..ssm.values };
        assert_eq!(ssm.bake_map(&turn), Err(Refusal::SsBump));
        let water = mat(r#""Water" { "$normalmap" "w/n" }"#);
        assert!(!LayerModel::of(&water, Layer::Bump).bakes());
    }

    #[test]
    fn baked_layers_edit_from_their_original() {
        let tr = Transform { rotate: 90.0, ..Transform::default() };
        let text = format!(
            r#""LightmappedGeneric" {{ "$basetexture" "a/b" "$bumpmap" "custom/x/hp_variants/b_normal_bk0123abcd" "%hpbumpsource" "a/b_normal" "%hpbumptransform" "{}" }}"#,
            tr.format()
        );
        let m = LayerModel::of(&mat(&text), Layer::Bump);
        assert_eq!(m.source.as_deref(), Some("a/b_normal"));
        assert_eq!(m.edit_texture(), Some("a/b_normal"));
        assert_eq!(m.values.tr, tr);
        let hand = text.replace("custom/x/hp_variants/b_normal_bk0123abcd", "other/normal");
        let m = LayerModel::of(&mat(&hand), Layer::Bump);
        assert_eq!(m.source, None);
        assert_eq!(m.edit_texture(), Some("other/normal"));
        assert_eq!(m.values.tr, Transform::default());
        let q = TexelMap { m: [[0, -1], [1, 0]], t: [0.0, 0.0] };
        let a = baked_name("a/b_normal", TexelKind::Normal, &q, 7);
        assert_eq!(a, baked_name("a/b_normal", TexelKind::Normal, &q, 7));
        assert_ne!(a, baked_name("a/b_normal", TexelKind::Normal, &q, 8));
        assert_ne!(a, baked_name("a/b_normal", TexelKind::Normal, &TexelMap::IDENTITY, 7));
        assert!(a.starts_with("hp_variants/b_normal_bk") && is_baked_name(&a), "{a}");
        assert!(!is_baked_name("hp_variants/b_normal"));
    }

    #[test]
    fn variants_are_stable_patches() {
        let params = vec![("$bumptransform".to_owned(), "center 0.5 0.5 scale 2 2 rotate 0 translate 0 0".to_owned())];
        let a = variant_name("gm_construct/wall_top", &params);
        assert_eq!(a, variant_name("gm_construct/wall_top", &params));
        assert!(a.starts_with("hp_variants/wall_top_"));
        let text = variant_text("gm_construct/wall_top", &params, None);
        let m = hp_vmt::parse(&text).unwrap();
        assert!(m.shader.eq_ignore_ascii_case("patch"));
        assert_eq!(m.get("include"), Some("materials/gm_construct/wall_top.vmt"));
        let wall =
            r#""LightmappedGeneric" { "$basetexture" "plaster/p" "LightmappedGeneric_DX9" { "$bumpmap" "c/n" } }"#;
        let origin = hp_vmt::parse(wall).unwrap();
        let baked = vec![("$bumpmap".to_owned(), "hp_variants/n_bk0123abcd".to_owned())];
        let text = variant_text("gm_construct/wall_top", &baked, Some(&origin));
        let patch = hp_vmt::parse(&text).unwrap();
        let resolved = hp_vmt::resolve_patch(&patch, &|_| Some(wall.to_owned())).unwrap();
        assert_eq!(
            resolved.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false).get("$bumpmap"),
            Some("hp_variants/n_bk0123abcd"),
            "{text}"
        );
        let mut changed = LayerValues { tr: Transform::default(), detail_scale: [4.0, 4.0] };
        let disk = changed;
        assert!(changes(&disk, &changed, Layer::Detail, "$detailtexturetransform").is_empty());
        changed.detail_scale = [8.0, 8.0];
        assert_eq!(
            changes(&disk, &changed, Layer::Detail, "$detailtexturetransform"),
            vec![("$detailscale".to_owned(), "8".to_owned())]
        );
    }

    #[test]
    fn blend_layers_follow_the_shader() {
        let mud = mat(
            r#""WorldVertexTransition" { "$basetexture" "n/mud1" "$basetexture2" "n/mud2" "$bumpmap" "n/mud1_normal" "$bumpmap2" "n/mud2_normal" "$normalmapalphaenvmapmask" 1 "$envmap" "env_cubemap" }"#,
        );
        assert_eq!(layers_of(&mud), vec![Layer::Base, Layer::Base2, Layer::Bump, Layer::Bump2]);
        let base2 = LayerModel::of(&mud, Layer::Base2);
        let bump = LayerModel::of(&mud, Layer::Bump);
        let bump2 = LayerModel::of(&mud, Layer::Bump2);
        assert!(base2.bakes() && bump.bakes());
        assert_eq!(bump.kind(), TexelKind::Normal);
        assert_eq!(base2.kind(), TexelKind::Color);
        assert_eq!((bump2.support, bump2.key), (Support::Honoured, "$bumptransform"));
        assert!(!bump2.bakes() && bump2.editable());
        assert_eq!(bump2.texture.as_deref(), Some("n/mud2_normal"));
        assert_eq!(bump2.layer.bake_keys().0, "%hpbumpmap2source");
        let moved = LayerValues { tr: Transform { translate: [0.25, 0.0], ..Transform::default() }, ..bump2.values };
        assert_eq!(changes(&bump2.values, &moved, Layer::Bump2, bump2.key)[0].0, "$bumptransform");
        assert_eq!(bump.key, "$bumptransform");
        assert!(bump.bake_map(&LayerValues { tr: moved.tr, ..bump.values }).is_ok());

        let detail = mat(
            r#""WorldVertexTransition" { "$basetexture" "a" "$basetexture2" "b" "$bumpmap" "n" "$bumpmap2" "n2" "$detail" "d" "$detailscale" ".1" }"#,
        );
        assert_eq!(layers_of(&detail), vec![Layer::Base, Layer::Base2, Layer::Bump, Layer::Bump2, Layer::Detail]);
        assert!(LayerModel::of(&detail, Layer::Bump2).bakes());
        assert!(LayerModel::of(&detail, Layer::Detail).bakes());

        let modulate = mat(
            r#""WorldVertexTransition" { "$basetexture" "a" "$basetexture2" "b" "$bumpmap" "n" "$bumpmap2" "n2" "$blendmodulatetexture" "m" "$basetexturetransform" "center .5 .5 scale 2 2 rotate 0 translate 0 0" }"#,
        );
        assert_eq!(layers_of(&modulate).last(), Some(&Layer::BlendMod));
        let bm = LayerModel::of(&modulate, Layer::BlendMod);
        assert!(bm.bakes());
        assert_eq!(bm.base, Transform::default(), "the blend mask ignores $basetexturetransform");
        assert_eq!(bm.values.tr, Transform::default());
        let b2 = LayerModel::of(&modulate, Layer::Bump2);
        assert!(b2.bakes() && b2.keeps_fast_path);
        let base2 = LayerModel::of(&modulate, Layer::Base2);
        assert_eq!(base2.values.tr.scale, [2.0, 2.0]);
        assert!(base2.bake_map(&base2.values).unwrap().is_identity());
        let masked = mat(
            r#""WorldVertexTransition" { "$basetexture" "a" "$basetexture2" "b" "$blendmodulatetexture" "m" "$maskedblending" 1 "$bumptransform" "center .5 .5 scale 1 1 rotate 0 translate .5 0" }"#,
        );
        let bm = LayerModel::of(&masked, Layer::BlendMod);
        assert_eq!((bm.support, bm.key), (Support::Honoured, "$blendmasktransform"));

        let mask = mat(
            r#""LightmappedGeneric" { "$basetexture" "a" "$bumpmap" "n" "$bumpmap2" "n2" "$bumpmask" "k" "$bumptransform2" "center .5 .5 scale 2 2 rotate 0 translate 0 0" }"#,
        );
        assert_eq!(LayerModel::of(&mask, Layer::Bump).key, "$bumptransform");
        assert!(LayerModel::of(&mask, Layer::Bump).honoured());
        let b2 = LayerModel::of(&mask, Layer::Bump2);
        assert_eq!((b2.support, b2.key, b2.values.tr.scale), (Support::Honoured, "$bumptransform2", [2.0, 2.0]));
        let lone = mat(r#""WorldVertexTransition" { "$basetexture" "a" "$basetexture2" "b" "$bumpmap2" "n2" }"#);
        assert!(!LayerModel::of(&lone, Layer::Bump2).editable());
        assert_eq!(Layer::from_mcp_name("bump2"), Some(Layer::Bump2));
        assert_eq!(Layer::from_mcp_name("blendmod"), Some(Layer::BlendMod));
    }

    #[test]
    fn layers_stay_in_place_when_the_face_moves() {
        let m = mat(
            r#""WorldVertexTransition" { "$basetexture" "a" "$basetexture2" "b" "$bumpmap" "n" "$bumpmap2" "n2" }"#,
        );
        let p = [0.3, 0.7];
        for layer in [Layer::Bump, Layer::Bump2, Layer::Base2] {
            let model = LayerModel::of(&m, layer);
            let from = LayerValues {
                tr: Transform { translate: [0.25, 0.5], rotate: 90.0, ..Transform::default() },
                ..model.values
            };
            for a in [
                Affine2 { m: [[1.0, 0.0], [0.0, 1.0]], t: [0.25, -0.5] },
                Affine2 { m: [[0.0, -1.0], [1.0, 0.0]], t: [1.0, 0.0] },
                Affine2 { m: [[-1.0, 0.0], [0.0, 2.0]], t: [0.0, 0.0] },
            ] {
                let to = compensated(&model, &from, &a.inverse().unwrap()).expect("a transform");
                let before = model.written(&from).apply(p);
                let after = model.written(&to).apply(a.apply(p));
                assert!(close(before, after), "{layer:?} {a:?}: {before:?} {after:?}");
            }
        }
        let half = mat(r#""LightmappedGeneric" { "$basetexture" "a" "$detail" "d" "$detailscale" ".5" }"#);
        let d = LayerModel::of(&half, Layer::Detail);
        assert!(d.game(&d.values).m.iter().flatten().any(|v| (v - v.round()).abs() > 1e-6));
    }
}
