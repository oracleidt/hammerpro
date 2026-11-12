use std::collections::HashMap;

use glam::DVec2;
use hp_app::state::AppState;
use hp_editor::{Document, FaceRef};
use hp_geom::HeightField;
use hp_vmf::DispInfo;
use hp_vmf::multiblend::MultiBlend;

use super::export::rasterize;
use super::image_io::{Channel, OutFormat, Pixels, RawSpec, finite_field, read_pixels, write_gray, write_rgba};
use super::{PixelGrid, orient, resolve_path, sample_field};

pub const UNDO_MASKS: &str = "Import blend masks";
pub const MAX_MASK_FILES: usize = 4;
pub const FOUR_WAY_SHADER: &str = "lightmapped_4wayblend";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaskTarget {
    #[default]
    Auto,
    Alpha,
    FourWay,
}

impl MaskTarget {
    pub const NAMES: [&'static str; 3] = ["auto", "alpha", "four_way"];

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Alpha => "alpha",
            Self::FourWay => "four_way",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Auto, Self::Alpha, Self::FourWay].into_iter().find(|t| t.name() == name)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeightMode {
    #[default]
    Coverage,
    Direct,
}

impl WeightMode {
    pub const NAMES: [&'static str; 2] = ["coverage", "direct"];

    pub fn name(self) -> &'static str {
        match self {
            Self::Coverage => "coverage",
            Self::Direct => "direct",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Coverage, Self::Direct].into_iter().find(|t| t.name() == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskSource {
    None,
    One,
    Rest,
    Image { file: usize, channel: Channel },
}

impl MaskSource {
    pub fn image(file: usize, channel: Channel) -> Self {
        Self::Image { file, channel }
    }

    pub fn text(self) -> String {
        match self {
            Self::None => "none".to_owned(),
            Self::One => "one".to_owned(),
            Self::Rest => "rest".to_owned(),
            Self::Image { file, channel } => format!("{}.{}", file + 1, channel.name()),
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim().to_ascii_lowercase();
        match t.as_str() {
            "none" | "zero" | "0" | "" => return Some(Self::None),
            "one" | "1" | "full" => return Some(Self::One),
            "rest" | "remainder" => return Some(Self::Rest),
            _ => {}
        }
        if let Some((file, ch)) = t.split_once('.') {
            let file: usize = file.trim().parse().ok()?;
            if !(1..=MAX_MASK_FILES).contains(&file) {
                return None;
            }
            return Channel::from_name(ch).map(|channel| Self::Image { file: file - 1, channel });
        }
        Channel::from_name(&t).map(|channel| Self::Image { file: 0, channel })
    }

    fn file(self) -> Option<usize> {
        match self {
            Self::Image { file, .. } => Some(file),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Levels {
    pub invert: bool,
    pub black: f64,
    pub white: f64,
    pub gamma: f64,
    pub contrast: f64,
}

impl Default for Levels {
    fn default() -> Self {
        Self { invert: false, black: 0.0, white: 1.0, gamma: 1.0, contrast: 1.0 }
    }
}

impl Levels {
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    pub fn apply(&self, v: f64) -> f64 {
        let v = if v.is_finite() { v } else { 0.0 };
        let span = self.white - self.black;
        let mut t = if span.abs() > 1e-9 {
            (v - self.black) / span
        } else if v >= self.black {
            1.0
        } else {
            0.0
        };
        t = t.clamp(0.0, 1.0);
        if (self.gamma - 1.0).abs() > 1e-12 {
            t = t.powf(1.0 / self.gamma.max(1e-3));
        }
        if (self.contrast - 1.0).abs() > 1e-12 {
            t = ((t - 0.5) * self.contrast + 0.5).clamp(0.0, 1.0);
        }
        if self.invert { 1.0 - t } else { t }
    }

    pub fn validate(&self, what: &str) -> Result<(), String> {
        let ok = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !(ok(self.black, -1.0, 2.0) && ok(self.white, -1.0, 2.0)) {
            return Err(format!("{what}: black and white must be between -1 and 2"));
        }
        if !ok(self.gamma, 0.05, 20.0) {
            return Err(format!("{what}: gamma must be between 0.05 and 20"));
        }
        if !ok(self.contrast, 0.0, 20.0) {
            return Err(format!("{what}: contrast must be between 0 and 20"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub source: MaskSource,
    pub levels: Levels,
}

impl Layer {
    pub fn of(source: MaskSource) -> Self {
        Self { source, levels: Levels::default() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaskOptions {
    pub files: Vec<String>,
    pub raw: RawSpec,
    pub target: MaskTarget,
    pub alpha: Layer,
    pub layers: [Layer; 4],
    pub normalize: bool,
    pub weights: WeightMode,
    pub extent: Option<(DVec2, DVec2)>,
    pub flip_x: bool,
    pub flip_y: bool,
    pub grid: PixelGrid,
    pub bilinear: bool,
}

impl Default for MaskOptions {
    fn default() -> Self {
        Self {
            files: vec![String::new()],
            raw: RawSpec::default(),
            target: MaskTarget::Auto,
            alpha: Layer::of(MaskSource::image(0, Channel::Luma)),
            layers: [Channel::R, Channel::G, Channel::B, Channel::A].map(|c| Layer::of(MaskSource::image(0, c))),
            normalize: true,
            weights: WeightMode::Coverage,
            extent: None,
            flip_x: false,
            flip_y: false,
            grid: PixelGrid::Corner,
            bilinear: true,
        }
    }
}

impl MaskOptions {
    pub fn used_files(&self) -> Vec<usize> {
        let alpha = (self.target != MaskTarget::FourWay).then_some(&self.alpha);
        let layers = if self.target == MaskTarget::Alpha { &self.layers[..0] } else { &self.layers[..] };
        let mut out: Vec<usize> = alpha.into_iter().chain(layers.iter()).filter_map(|l| l.source.file()).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.files.len() > MAX_MASK_FILES {
            return Err(format!("at most {MAX_MASK_FILES} mask files"));
        }
        for f in self.used_files() {
            if self.files.get(f).is_none_or(|p| p.trim().is_empty()) {
                return Err(format!("a channel uses mask file {} which is not given", f + 1));
            }
        }
        self.alpha.levels.validate("alpha")?;
        for (i, l) in self.layers.iter().enumerate() {
            l.levels.validate(&format!("texture {}", i + 1))?;
        }
        if let Some((min, size)) = self.extent
            && !(min.is_finite() && size.is_finite() && size.min_element() >= 1.0)
        {
            return Err("the mask extent must have a size of at least 1 unit".to_owned());
        }
        Ok(())
    }
}

enum Plane {
    Zero,
    One,
    Rest,
    Field(HeightField),
}

pub struct Prepared {
    alpha: Plane,
    alpha_levels: Levels,
    layers: Vec<(Plane, Levels)>,
    pub images: Vec<(usize, usize)>,
    pub described: Vec<String>,
    grid: PixelGrid,
    bilinear: bool,
}

pub fn load_files(state: &AppState, opts: &MaskOptions) -> Result<Vec<Option<Pixels>>, String> {
    opts.validate()?;
    let used = opts.used_files();
    let mut out = Vec::with_capacity(opts.files.len());
    for (i, f) in opts.files.iter().enumerate() {
        if used.contains(&i) {
            out.push(Some(read_pixels(&resolve_path(state, f)?, &opts.raw)?));
        } else {
            out.push(None);
        }
    }
    Ok(out)
}

pub fn load_all(state: &AppState, files: &[String], raw: &RawSpec) -> Result<Vec<Option<Pixels>>, String> {
    let mut out = Vec::with_capacity(files.len());
    for f in files {
        if f.trim().is_empty() {
            out.push(None);
        } else {
            out.push(Some(read_pixels(&resolve_path(state, f)?, raw)?));
        }
    }
    Ok(out)
}

pub fn prepare(files: &[Option<Pixels>], opts: &MaskOptions) -> Result<Prepared, String> {
    opts.validate()?;
    let mut cache: HashMap<(usize, &'static str), HeightField> = HashMap::new();
    let mut plane = |source: MaskSource| -> Result<Plane, String> {
        Ok(match source {
            MaskSource::None => Plane::Zero,
            MaskSource::One => Plane::One,
            MaskSource::Rest => Plane::Rest,
            MaskSource::Image { file, channel } => {
                let key = (file, channel.name());
                if let Some(f) = cache.get(&key) {
                    return Ok(Plane::Field(f.clone()));
                }
                let px = files
                    .get(file)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| format!("mask file {} is not loaded", file + 1))?;
                let field = orient(finite_field(px.channel(channel)), opts.flip_x, opts.flip_y);
                cache.insert(key, field.clone());
                Plane::Field(field)
            }
        })
    };
    let alpha = if opts.target == MaskTarget::FourWay { Plane::Zero } else { plane(opts.alpha.source)? };
    let mut layers = Vec::with_capacity(4);
    for l in &opts.layers {
        let p = if opts.target == MaskTarget::Alpha { Plane::Zero } else { plane(l.source)? };
        layers.push((p, l.levels));
    }
    let images = files.iter().flatten().map(|p| (p.width, p.height)).collect();
    let described = files.iter().flatten().map(Pixels::describe).collect();
    Ok(Prepared {
        alpha,
        alpha_levels: opts.alpha.levels,
        layers,
        images,
        described,
        grid: opts.grid,
        bilinear: opts.bilinear,
    })
}

impl Prepared {
    fn sample(&self, plane: &Plane, levels: &Levels, u: f64, v: f64) -> f64 {
        match plane {
            Plane::Zero | Plane::Rest => levels.apply(0.0),
            Plane::One => levels.apply(1.0),
            Plane::Field(f) => levels.apply(sample_field(f, u, v, self.grid, self.bilinear)),
        }
    }

    pub fn alpha(&self, u: f64, v: f64) -> f64 {
        self.sample(&self.alpha, &self.alpha_levels, u, v).clamp(0.0, 1.0)
    }

    pub fn coverage(&self, u: f64, v: f64, normalize: bool) -> [f64; 4] {
        let mut c = [0.0; 4];
        let mut rest = Vec::new();
        for (i, (plane, levels)) in self.layers.iter().enumerate() {
            if matches!(plane, Plane::Rest) {
                rest.push(i);
            } else if let Some(slot) = c.get_mut(i) {
                *slot = self.sample(plane, levels, u, v).clamp(0.0, 1.0);
            }
        }
        if !rest.is_empty() {
            let left = (1.0 - c.iter().sum::<f64>()).clamp(0.0, 1.0) / rest.len() as f64;
            for i in rest {
                let levels = self.layers.get(i).map(|l| l.1).unwrap_or_default();
                if let Some(slot) = c.get_mut(i) {
                    *slot = levels.apply(left).clamp(0.0, 1.0);
                }
            }
        }
        if normalize {
            let sum: f64 = c.iter().sum();
            if sum > 1e-9 {
                c = c.map(|x| x / sum);
            } else {
                c = [1.0, 0.0, 0.0, 0.0];
            }
        }
        c
    }
}

pub fn coverage_to_rows(c: [f64; 4]) -> [f64; 4] {
    let ratio = |a: f64, b: f64| if b > 1e-12 { (a / b).clamp(0.0, 1.0) } else { 0.0 };
    let s12 = c[0] + c[1];
    let s123 = s12 + c[2];
    [c[0].clamp(0.0, 1.0), ratio(c[1], s12), ratio(c[2], s123), c[3].clamp(0.0, 1.0)]
}

pub fn rows_to_coverage(b: [f64; 4]) -> [f64; 4] {
    let f = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    let (b2, b3, b4) = (f(b[1]), f(b[2]), f(b[3]));
    [(1.0 - b2) * (1.0 - b3) * (1.0 - b4), b2 * (1.0 - b3) * (1.0 - b4), b3 * (1.0 - b4), b4]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceKind {
    Alpha,
    FourWay,
}

pub fn face_bounds(doc: &Document, faces: &[FaceRef]) -> Option<(DVec2, DVec2)> {
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for f in faces {
        let Some(m) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) else { continue };
        for p in &m.positions {
            lo = lo.min(p.truncate());
            hi = hi.max(p.truncate());
        }
    }
    let size = hi - lo;
    (lo.is_finite() && size.min_element() > 1e-3).then_some((lo, size))
}

pub fn face_kinds(state: &mut AppState, faces: &[FaceRef], target: MaskTarget) -> Vec<FaceKind> {
    let mut shaders: HashMap<String, bool> = HashMap::new();
    let mut out = Vec::with_capacity(faces.len());
    for f in faces {
        let kind = match target {
            MaskTarget::Alpha => FaceKind::Alpha,
            MaskTarget::FourWay => FaceKind::FourWay,
            MaskTarget::Auto => {
                let side = state.doc.scene().solid(f.solid).and_then(|n| n.solid.sides.get(f.side));
                let rows = side.and_then(|s| s.disp.as_deref()).is_some_and(hp_vmf::multiblend::has_multiblend);
                let material = side.map(|s| s.material.to_ascii_lowercase()).unwrap_or_default();
                let four = match shaders.get(&material) {
                    Some(v) => *v,
                    None => {
                        let v = state
                            .assets
                            .material_info(&material)
                            .is_some_and(|i| i.shader.eq_ignore_ascii_case(FOUR_WAY_SHADER));
                        shaders.insert(material, v);
                        v
                    }
                };
                if four || rows { FaceKind::FourWay } else { FaceKind::Alpha }
            }
        };
        out.push(kind);
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaskReport {
    pub faces: usize,
    pub alpha_faces: usize,
    pub four_way_faces: usize,
    pub vertices: usize,
    pub images: Vec<(usize, usize)>,
    pub described: Vec<String>,
    pub min: (f64, f64),
    pub size: (f64, f64),
}

pub fn compute(
    doc: &Document,
    faces: &[FaceRef],
    kinds: &[FaceKind],
    prepared: &Prepared,
    opts: &MaskOptions,
    extent: (DVec2, DVec2),
) -> (Vec<(FaceRef, DispInfo)>, MaskReport) {
    let (min, size) = extent;
    let mut report = MaskReport { min: (min.x, min.y), size: (size.x, size.y), ..MaskReport::default() };
    let mut updates = Vec::with_capacity(faces.len());
    let mut seen = std::collections::HashSet::new();
    for (f, kind) in faces.iter().zip(kinds) {
        if !seen.insert(*f) {
            continue;
        }
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(old)) =
            (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
        else {
            continue;
        };
        let n = old.row_len();
        let count = n * n;
        if mesh.positions.len() != count || old.alphas.len() != count {
            continue;
        }
        let mut disp = old.clone();
        let uv = |k: usize| {
            let p = mesh.positions.get(k).map_or(DVec2::ZERO, |p| p.truncate());
            (((p.x - min.x) / size.x).clamp(0.0, 1.0), ((p.y - min.y) / size.y).clamp(0.0, 1.0))
        };
        match kind {
            FaceKind::Alpha => {
                for (k, a) in disp.alphas.iter_mut().enumerate() {
                    let (u, v) = uv(k);
                    *a = 255.0 * prepared.alpha(u, v);
                }
                report.alpha_faces += 1;
            }
            FaceKind::FourWay => {
                let mut mb = MultiBlend::of(&disp).unwrap_or_else(|| MultiBlend::new(count));
                for k in 0..count {
                    let (u, v) = uv(k);
                    let c = prepared.coverage(u, v, opts.normalize);
                    if let Some(w) = mb.weights.get_mut(k) {
                        *w = match opts.weights {
                            WeightMode::Coverage => coverage_to_rows(c),
                            WeightMode::Direct => c,
                        };
                    }
                    if let Some(a) = disp.alphas.get_mut(k) {
                        *a = (255.0 * (1.0 - c[0])).clamp(0.0, 255.0);
                    }
                }
                if !mb.write_to(&mut disp) {
                    continue;
                }
                report.four_way_faces += 1;
            }
        }
        report.vertices += count;
        report.faces += 1;
        updates.push((*f, disp));
    }
    report.images.clone_from(&prepared.images);
    report.described.clone_from(&prepared.described);
    (updates, report)
}

pub fn import_masks(
    state: &mut AppState,
    faces: &[FaceRef],
    opts: &MaskOptions,
    label: &str,
) -> Result<MaskReport, String> {
    if faces.is_empty() {
        return Err("select the displacements to paint (or give their faces)".to_owned());
    }
    let files = load_files(state, opts)?;
    let prepared = prepare(&files, opts)?;
    apply_prepared(state, faces, opts, &prepared, label)
}

pub fn apply_prepared(
    state: &mut AppState,
    faces: &[FaceRef],
    opts: &MaskOptions,
    prepared: &Prepared,
    label: &str,
) -> Result<MaskReport, String> {
    let extent = match opts.extent {
        Some(e) => e,
        None => face_bounds(&state.doc, faces).ok_or("the displacements have no area seen from above")?,
    };
    let kinds = face_kinds(state, faces, opts.target);
    let (updates, report) = compute(&state.doc, faces, &kinds, prepared, opts, extent);
    if report.faces == 0 {
        return Err("none of the faces is a displacement".to_owned());
    }
    state.doc.set_disp_data(label, updates).map_err(|e| crate::common::edit_error_text(&e))?;
    Ok(report)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaskExportOptions {
    pub path: String,
    pub alpha_path: String,
    pub layer_paths: [String; 4],
    pub weights: WeightMode,
    pub float: bool,
}

impl MaskExportOptions {
    pub fn paths(&self) -> Vec<&str> {
        std::iter::once(self.path.as_str())
            .chain(std::iter::once(self.alpha_path.as_str()))
            .chain(self.layer_paths.iter().map(String::as_str))
            .filter(|p| !p.trim().is_empty())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaskExportReport {
    pub width: usize,
    pub height: usize,
    pub min: (f64, f64),
    pub size: (f64, f64),
    pub faces: usize,
    pub files: Vec<String>,
}

pub fn export_masks(state: &AppState, faces: &[FaceRef], opts: &MaskExportOptions) -> Result<MaskExportReport, String> {
    if opts.paths().is_empty() {
        return Err("give at least one output file".to_owned());
    }
    let raster = rasterize(&state.doc, faces).ok_or("the selected displacements have no surface to export")?;
    let (w, h) = (raster.width, raster.height);
    let cover: Vec<[f64; 4]> = raster
        .weights
        .iter()
        .map(|b| match opts.weights {
            WeightMode::Coverage => rows_to_coverage(*b),
            WeightMode::Direct => b.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }),
        })
        .collect();
    let mut files = Vec::new();
    if !opts.path.trim().is_empty() {
        let p = resolve_path(state, &opts.path)?;
        write_rgba(&p, w, h, &cover, OutFormat::for_path(&p, opts.float)?)?;
        files.push(p.display().to_string());
    }
    if !opts.alpha_path.trim().is_empty() {
        let p = resolve_path(state, &opts.alpha_path)?;
        write_gray(&p, w, h, &raster.alphas, OutFormat::for_path(&p, opts.float)?)?;
        files.push(p.display().to_string());
    }
    for (k, path) in opts.layer_paths.iter().enumerate() {
        if path.trim().is_empty() {
            continue;
        }
        let p = resolve_path(state, path)?;
        let plane: Vec<f64> = cover.iter().map(|c| c[k]).collect();
        write_gray(&p, w, h, &plane, OutFormat::for_path(&p, opts.float)?)?;
        files.push(p.display().to_string());
    }
    Ok(MaskExportReport { width: w, height: h, min: raster.min, size: raster.size, faces: faces.len(), files })
}

pub fn preview_colors(prepared: &Prepared, opts: &MaskOptions, side: usize, four_way: bool) -> Vec<[f32; 3]> {
    const TINTS: [[f64; 3]; 4] = [[0.36, 0.62, 0.25], [0.55, 0.40, 0.24], [0.55, 0.55, 0.58], [0.95, 0.95, 0.97]];
    let side = side.max(2);
    let mut out = Vec::with_capacity(side * side);
    for j in 0..side {
        for i in 0..side {
            let u = i as f64 / (side - 1) as f64;
            let v = 1.0 - j as f64 / (side - 1) as f64;
            if four_way {
                let raw = prepared.coverage(u, v, opts.normalize);
                let c = match opts.weights {
                    WeightMode::Coverage => raw,
                    WeightMode::Direct => rows_to_coverage(raw),
                };
                let mut rgb = [0.0f64; 3];
                for (k, tint) in TINTS.iter().enumerate() {
                    for (o, t) in rgb.iter_mut().zip(tint) {
                        *o += c[k] * t;
                    }
                }
                out.push(rgb.map(|x| x.clamp(0.0, 1.0) as f32));
            } else {
                let a = prepared.alpha(u, v) as f32;
                out.push([a, a, a]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;

    #[test]
    fn sources_parse_and_print() {
        assert_eq!(MaskSource::parse("2.a"), Some(MaskSource::image(1, Channel::A)));
        assert_eq!(MaskSource::parse("G"), Some(MaskSource::image(0, Channel::G)));
        assert_eq!(MaskSource::parse("rest"), Some(MaskSource::Rest));
        assert_eq!(MaskSource::parse("none"), Some(MaskSource::None));
        assert_eq!(MaskSource::parse("5.r"), None);
        assert_eq!(MaskSource::parse("1.q"), None);
        assert_eq!(MaskSource::image(3, Channel::Luma).text(), "4.l");
        for s in ["1.r", "2.g", "3.b", "4.a", "1.l", "one", "rest", "none"] {
            assert_eq!(MaskSource::parse(s).unwrap().text(), s);
        }
    }

    #[test]
    fn levels_invert_clip_and_shape() {
        let l = Levels { black: 0.25, white: 0.75, ..Levels::default() };
        assert_eq!(l.apply(0.0), 0.0);
        assert_eq!(l.apply(0.5), 0.5);
        assert_eq!(l.apply(1.0), 1.0);
        assert_eq!(Levels { invert: true, ..Levels::default() }.apply(0.25), 0.75);
        assert!((Levels { gamma: 2.0, ..Levels::default() }.apply(0.25) - 0.5).abs() < 1e-12);
        assert_eq!(Levels { contrast: 2.0, ..Levels::default() }.apply(0.75), 1.0);
        assert_eq!(Levels::default().apply(f64::NAN), 0.0);
        assert!(Levels { gamma: 0.0, ..Levels::default() }.validate("x").is_err());
        assert!(Levels::default().is_identity());
    }

    #[test]
    fn coverage_and_rows_convert_both_ways() {
        for c in [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.25, 0.25, 0.25, 0.25],
            [0.0, 0.5, 0.5, 0.0],
            [0.1, 0.2, 0.3, 0.4],
        ] {
            let back = rows_to_coverage(coverage_to_rows(c));
            for (a, b) in c.iter().zip(back) {
                assert!((a - b).abs() < 1e-12, "{c:?} {back:?}");
            }
        }
        assert_eq!(coverage_to_rows([0.0, 0.5, 0.5, 0.0]), [0.0, 1.0, 0.5, 0.0]);
        assert_eq!(coverage_to_rows([0.0, 0.0, 1.0, 0.0]), [0.0, 0.0, 1.0, 0.0]);
    }
}
