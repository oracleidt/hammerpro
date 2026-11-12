use hp_app::state::AppState;
use hp_editor::EditError;
use hp_geom::HeightField;

use super::image_io::{Channel, MAX_IMAGE_SIDE, RawSpec, finite_field, read_pixels};
use super::masks::{FaceKind, Layer, MaskOptions, MaskSource, MaskTarget, WeightMode, compute, load_files, prepare};
use super::{
    PixelGrid, Source, Terrain, TexMode, Texturing, TileSetup, UNDO_IMPORT, edit_failed, orient, resolve_path,
};

pub const MAX_TILE_FILES: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeightValues {
    #[default]
    Auto,
    Unit,
    Stretch,
    Absolute,
}

impl HeightValues {
    pub const NAMES: [&'static str; 4] = ["auto", "unit", "stretch", "absolute"];

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Unit => "unit",
            Self::Stretch => "stretch",
            Self::Absolute => "absolute",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Auto, Self::Unit, Self::Stretch, Self::Absolute].into_iter().find(|v| v.name() == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportOptions {
    pub path: String,
    pub raw: RawSpec,
    pub setup: TileSetup,
    pub invert: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub smoothing: u32,
    pub bilinear: bool,
    pub tex: Texturing,
    pub values: HeightValues,
    pub value_scale: f64,
    pub grid: PixelGrid,
    pub match_pixels: bool,
    pub tiles: Vec<String>,
    pub tile_columns: u32,
    pub splat_weights: WeightMode,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            path: String::new(),
            raw: RawSpec::default(),
            setup: TileSetup::default(),
            invert: false,
            flip_x: false,
            flip_y: false,
            smoothing: 0,
            bilinear: false,
            tex: Texturing::default(),
            values: HeightValues::Auto,
            value_scale: 1.0,
            grid: PixelGrid::Corner,
            match_pixels: false,
            tiles: Vec::new(),
            tile_columns: 0,
            splat_weights: WeightMode::Coverage,
        }
    }
}

impl ImportOptions {
    pub fn has_source(&self) -> bool {
        !self.path.trim().is_empty() || self.tiles.iter().any(|t| !t.trim().is_empty())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportReport {
    pub tiles: usize,
    pub image: (usize, usize),
    pub spacing: f64,
    pub described: String,
    pub raw_range: (f64, f64),
    pub h_min: f64,
    pub h_max: f64,
    pub setup: TileSetup,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceImage {
    pub field: HeightField,
    pub float: bool,
    pub described: String,
}

fn mosaic(state: &AppState, opts: &ImportOptions) -> Result<SourceImage, String> {
    let paths: Vec<&String> = opts.tiles.iter().filter(|t| !t.trim().is_empty()).collect();
    if paths.len() > MAX_TILE_FILES {
        return Err(format!("at most {MAX_TILE_FILES} tile files"));
    }
    let count = paths.len();
    let cols = if opts.tile_columns > 0 {
        opts.tile_columns as usize
    } else {
        let side = (count as f64).sqrt().round() as usize;
        if side * side != count {
            return Err(format!("{count} tile files do not make a square grid: give the number of columns"));
        }
        side
    };
    if cols == 0 || !count.is_multiple_of(cols) {
        return Err(format!("{count} tile files do not fill rows of {cols}"));
    }
    let rows = count / cols;
    let mut fields = Vec::with_capacity(count);
    let mut float = false;
    for p in &paths {
        let px = read_pixels(&resolve_path(state, p)?, &opts.raw)?;
        float |= px.float;
        fields.push(finite_field(px.gray()));
    }
    let (w, h) = fields.first().map(|f| (f.width, f.height)).ok_or("no tile files")?;
    if fields.iter().any(|f| (f.width, f.height) != (w, h)) {
        return Err("every tile file must have the same size".to_owned());
    }
    let shared = w > 2 && h > 2 && (w - 1).is_power_of_two() && (h - 1).is_power_of_two();
    let overlap = usize::from(shared);
    let total_w = cols * (w - overlap) + overlap;
    let total_h = rows * (h - overlap) + overlap;
    if total_w > MAX_IMAGE_SIDE as usize || total_h > MAX_IMAGE_SIDE as usize {
        return Err(format!("the tiles make a {total_w} x {total_h} image, the limit is {MAX_IMAGE_SIDE} per side"));
    }
    let mut sum = vec![0.0; total_w * total_h];
    let mut hits = vec![0u32; total_w * total_h];
    for (k, f) in fields.iter().enumerate() {
        let (ox, oy) = ((k % cols) * (w - overlap), (k / cols) * (h - overlap));
        for j in 0..h {
            for i in 0..w {
                let slot = (oy + j) * total_w + ox + i;
                if let (Some(s), Some(c), Some(v)) = (sum.get_mut(slot), hits.get_mut(slot), f.data.get(j * w + i)) {
                    *s += v;
                    *c += 1;
                }
            }
        }
    }
    let data = sum.iter().zip(&hits).map(|(s, c)| if *c > 0 { s / f64::from(*c) } else { 0.0 }).collect();
    let field = HeightField::new(total_w, total_h, data).ok_or("the tiles are empty")?;
    let described = format!(
        "{cols} x {rows} tiles of {w} x {h}{} = {total_w} x {total_h}",
        if shared { " with shared edges" } else { "" }
    );
    Ok(SourceImage { field, float, described })
}

pub fn load_source(state: &AppState, opts: &ImportOptions) -> Result<SourceImage, String> {
    if opts.tiles.iter().any(|t| !t.trim().is_empty()) {
        return mosaic(state, opts);
    }
    let path = resolve_path(state, &opts.path)?;
    let px = read_pixels(&path, &opts.raw)?;
    Ok(SourceImage { field: finite_field(px.gray()), float: px.float, described: px.describe() })
}

pub fn map_values(
    raw: HeightField,
    float: bool,
    opts: &ImportOptions,
    setup: &mut TileSetup,
) -> Result<(HeightField, (f64, f64)), String> {
    let (lo, hi) = raw.min_max();
    let mode = match opts.values {
        HeightValues::Auto if !float || (lo >= 0.0 && hi <= 1.0) => HeightValues::Unit,
        HeightValues::Auto => HeightValues::Stretch,
        m => m,
    };
    let span = hi - lo;
    let stretch = |mut f: HeightField| {
        for v in &mut f.data {
            *v = if span > 1e-300 { (*v - lo) / span } else { 0.0 };
        }
        f
    };
    let field = match mode {
        HeightValues::Unit | HeightValues::Auto => raw,
        HeightValues::Stretch => stretch(raw),
        HeightValues::Absolute => {
            let scale = opts.value_scale;
            if !(scale.is_finite() && scale > 0.0) {
                return Err("the value scale must be a positive number".to_owned());
            }
            setup.h_min = lo * scale;
            setup.h_max = hi * scale;
            stretch(raw)
        }
    };
    Ok((field, (lo, hi)))
}

pub fn match_pixels(setup: &mut TileSetup, w: usize, h: usize) -> Result<(), String> {
    let per = 1usize << setup.tile_power.clamp(2, 4);
    if w < 2 || h < 2 || !(w - 1).is_multiple_of(per) || !(h - 1).is_multiple_of(per) {
        return Err(format!(
            "one vertex per pixel needs an image side of a multiple of {per} plus one (like {}), {w} x {h} is not",
            per * 64 + 1
        ));
    }
    let (tx, ty) = ((w - 1) / per, (h - 1) / per);
    setup.tile_size = setup.size.x / tx as f64;
    setup.size.y = setup.tile_size * ty as f64;
    Ok(())
}

pub struct Plan {
    pub field: HeightField,
    pub setup: TileSetup,
    pub image: (usize, usize),
    pub raw_range: (f64, f64),
    pub described: String,
}

pub fn plan(opts: &ImportOptions, state: &AppState) -> Result<Plan, String> {
    if !opts.has_source() {
        return Err("no file given".to_owned());
    }
    let src = load_source(state, opts)?;
    let image = (src.field.width, src.field.height);
    let mut setup = opts.setup.clone();
    if opts.match_pixels {
        match_pixels(&mut setup, image.0, image.1)?;
    }
    let oriented = orient(src.field, opts.flip_x, opts.flip_y);
    let (mapped, raw_range) = map_values(oriented, src.float, opts, &mut setup)?;
    let field = shape_field(mapped, opts);
    setup.validate()?;
    Ok(Plan { field, setup, image, raw_range, described: src.described })
}

pub fn height_field(opts: &ImportOptions, state: &AppState) -> Result<HeightField, String> {
    plan(opts, state).map(|p| p.field)
}

pub fn shape_field(mut field: HeightField, opts: &ImportOptions) -> HeightField {
    if opts.invert {
        field.invert();
    }
    field.smooth(opts.smoothing);
    field
}

pub fn thumb_field(loaded: HeightField, opts: &ImportOptions) -> HeightField {
    let mut field = orient(loaded, opts.flip_x, opts.flip_y);
    let (lo, hi) = field.min_max();
    if hi - lo > 1e-300 {
        for v in &mut field.data {
            *v = (*v - lo) / (hi - lo);
        }
    }
    shape_field(field, opts)
}

pub fn splat_masks(opts: &ImportOptions, setup: &TileSetup) -> Result<Option<MaskOptions>, String> {
    if opts.tex.mode != TexMode::Splat {
        return Ok(None);
    }
    let mut m = MaskOptions {
        target: MaskTarget::FourWay,
        normalize: true,
        weights: opts.splat_weights,
        extent: Some((setup.min(), setup.size)),
        flip_x: opts.flip_x,
        flip_y: opts.flip_y,
        grid: opts.grid,
        bilinear: true,
        alpha: Layer::of(MaskSource::None),
        ..MaskOptions::default()
    };
    if !opts.tex.splat.trim().is_empty() {
        m.files = vec![opts.tex.splat.clone()];
    } else if opts.tex.channels.iter().any(|c| !c.trim().is_empty()) {
        m.files = opts.tex.channels.to_vec();
        for (i, (layer, path)) in m.layers.iter_mut().zip(&opts.tex.channels).enumerate() {
            *layer =
                Layer::of(if path.trim().is_empty() { MaskSource::None } else { MaskSource::image(i, Channel::Luma) });
        }
    } else {
        return Err("splat texturing needs a splat map or four grayscale channel images".to_owned());
    }
    Ok(Some(m))
}

pub fn build_terrain(opts: &ImportOptions, state: &AppState) -> Result<(Terrain, Plan), String> {
    opts.setup.validate()?;
    let plan = plan(opts, state)?;
    let source = match opts.tex.mode {
        TexMode::None | TexMode::Splat => Source::None,
        TexMode::Auto => Source::Auto(opts.tex.auto),
    };
    let terrain = Terrain {
        field: plan.field.clone(),
        min: plan.setup.min(),
        size: plan.setup.size,
        h_min: plan.setup.h_min,
        h_max: plan.setup.h_max,
        skirt: plan.setup.skirt,
        spacing: plan.setup.spacing().min_element(),
        bilinear: opts.bilinear,
        grid: opts.grid,
        source,
    };
    Ok((terrain, plan))
}

pub fn import(state: &mut AppState, opts: &ImportOptions) -> Result<ImportReport, String> {
    import_as(state, opts, UNDO_IMPORT)
}

pub fn import_as(state: &mut AppState, opts: &ImportOptions, label: &str) -> Result<ImportReport, String> {
    let (terrain, plan) = build_terrain(opts, state)?;
    let masks = splat_masks(opts, &plan.setup)?;
    let prepared = match &masks {
        Some(m) => Some(prepare(&load_files(state, m)?, m)?),
        None => None,
    };
    let layout = plan.setup.layout();
    let tiles = state.doc.transaction(label, |doc| -> Result<usize, String> {
        let ids = terrain.run(|surface| doc.create_terrain(&layout, surface, label)).map_err(|e| edit_failed(&e))?;
        if let (Some(m), Some(p)) = (&masks, &prepared) {
            let faces = doc.disp_faces_of_solids(&ids);
            let kinds = vec![FaceKind::FourWay; faces.len()];
            let extent = m.extent.unwrap_or((plan.setup.min(), plan.setup.size));
            let (updates, _) = compute(doc, &faces, &kinds, p, m, extent);
            doc.set_disp_data(label, updates).map_err(|e: EditError| edit_failed(&e))?;
        }
        Ok(ids.len())
    })?;
    Ok(ImportReport {
        tiles,
        image: plan.image,
        spacing: plan.setup.spacing().min_element(),
        described: plan.described,
        raw_range: plan.raw_range,
        h_min: plan.setup.h_min,
        h_max: plan.setup.h_max,
        setup: plan.setup,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use glam::DVec2;
    use hp_app::testing::test_state;

    use super::*;
    use crate::terrain::image_io::write_heights;

    fn temp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("hp_terrain_imp_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn mesh_points(state: &AppState) -> Vec<glam::DVec3> {
        let mut all = Vec::new();
        for f in state.doc.disp_faces() {
            if let Some(m) = state.doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) {
                all.extend(m.positions.iter().copied());
            }
        }
        all
    }

    fn options(path: &std::path::Path) -> ImportOptions {
        ImportOptions {
            path: path.to_string_lossy().into_owned(),
            setup: TileSetup {
                size: DVec2::new(1024.0, 1024.0),
                tile_size: 512.0,
                tile_power: 2,
                h_min: 0.0,
                h_max: 256.0,
                centered: false,
                ..TileSetup::default()
            },
            ..ImportOptions::default()
        }
    }

    #[test]
    fn a_horizontal_gradient_image_gives_a_ramp() {
        let d = temp("ramp");
        let values: Vec<u16> = (0..16 * 16).map(|k| ((k % 16) as f64 / 15.0 * 65535.0).round() as u16).collect();
        let path = d.join("ramp.png");
        write_heights(&path, 16, 16, &values).unwrap();
        let mut state = test_state();
        let report = import(&mut state, &options(&path)).unwrap();
        assert_eq!(report.tiles, 4);
        let pts = mesh_points(&state);
        assert!(!pts.is_empty());
        for p in &pts {
            let expect = p.x / 1024.0 * 256.0;
            assert!((p.z - expect).abs() < 0.05, "{p:?} expected {expect}");
        }
        assert!(state.doc.undo());
        assert!(state.doc.disp_faces().is_empty());
    }

    #[test]
    fn sixteen_bit_precision_is_kept_in_the_heights() {
        let d = temp("prec");
        let values = vec![1u16, 2, 3, 4];
        let path = d.join("p.png");
        write_heights(&path, 2, 2, &values).unwrap();
        let mut state = test_state();
        let mut o = options(&path);
        o.setup.h_max = 65535.0 / 2.0;
        o.setup.size = glam::DVec2::new(512.0, 512.0);
        o.setup.tile_size = 512.0;
        import(&mut state, &o).unwrap();
        let pts = mesh_points(&state);
        let lowest = pts.iter().map(|p| p.z).fold(f64::MAX, f64::min);
        let highest = pts.iter().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!((lowest - 0.5).abs() < 0.01, "{lowest}");
        assert!((highest - 2.0).abs() < 0.01, "{highest}");
    }

    #[test]
    fn tile_seams_match_and_flips_and_invert_apply() {
        let d = temp("seam");
        let values: Vec<u16> = (0..64 * 64).map(|k| ((k * 7919) % 65535) as u16).collect();
        let path = d.join("n.png");
        write_heights(&path, 64, 64, &values).unwrap();
        let mut state = test_state();
        import(&mut state, &options(&path)).unwrap();
        let mut by_xy = std::collections::HashMap::new();
        for p in mesh_points(&state) {
            let key = ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64);
            let z = *by_xy.entry(key).or_insert(p.z);
            assert!((z - p.z).abs() < 1e-6, "seam at {key:?}");
        }
        let plain = mesh_points(&state);
        let mut s2 = test_state();
        let mut o = options(&path);
        o.invert = true;
        import(&mut s2, &o).unwrap();
        let inv = mesh_points(&s2);
        let total: f64 = plain.iter().zip(&inv).map(|(a, b)| a.z + b.z).sum::<f64>() / plain.len() as f64;
        assert!((total - 256.0).abs() < 1.0, "{total}");
    }

    #[test]
    fn flip_y_puts_the_image_top_at_the_north_edge() {
        let d = temp("flip");
        let values: Vec<u16> = (0..8 * 8).map(|k| if k / 8 == 0 { 65535 } else { 0 }).collect();
        let path = d.join("t.png");
        write_heights(&path, 8, 8, &values).unwrap();
        let mut state = test_state();
        import(&mut state, &options(&path)).unwrap();
        let pts = mesh_points(&state);
        let north = pts.iter().filter(|p| (p.y - 1024.0).abs() < 1e-6).map(|p| p.z).fold(f64::MAX, f64::min);
        let south = pts.iter().filter(|p| p.y.abs() < 1e-6).map(|p| p.z).fold(f64::MIN, f64::max);
        assert!(north > 250.0 && south < 1.0, "{north} {south}");
    }

    #[test]
    fn auto_texturing_puts_rock_on_the_cliff() {
        let d = temp("auto");
        let values: Vec<u16> = (0..16 * 16).map(|k| if k % 16 < 8 { 0 } else { 65535 }).collect();
        let path = d.join("c.png");
        write_heights(&path, 16, 16, &values).unwrap();
        let mut state = test_state();
        let mut o = options(&path);
        o.tex.mode = TexMode::Auto;
        o.bilinear = true;
        import(&mut state, &o).unwrap();
        let mut rock = 0;
        let mut grass = 0;
        for f in state.doc.disp_faces() {
            let d = state
                .doc
                .scene()
                .solid(f.solid)
                .and_then(|n| n.solid.sides.get(f.side))
                .and_then(|s| s.disp.as_deref())
                .unwrap();
            let mb = hp_vmf::multiblend::MultiBlend::of(d).unwrap();
            rock += mb.weights.iter().filter(|w| w[1] > 0.9).count();
            grass += mb.weights.iter().filter(|w| w[0] > 0.9).count();
        }
        assert!(rock > 0 && grass > rock);
    }

    #[test]
    fn splat_images_drive_the_blend_rows() {
        let d = temp("splat");
        let values = vec![0u16; 16];
        let path = d.join("flat.png");
        write_heights(&path, 4, 4, &values).unwrap();
        let splat: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
            image::ImageBuffer::from_pixel(4, 4, image::Rgba([0, 255, 0, 0]));
        splat.save(d.join("s.png")).unwrap();
        let mut state = test_state();
        let mut o = options(&path);
        o.tex.mode = TexMode::Splat;
        o.tex.splat = d.join("s.png").to_string_lossy().into_owned();
        import(&mut state, &o).unwrap();
        for f in state.doc.disp_faces() {
            let d = state
                .doc
                .scene()
                .solid(f.solid)
                .and_then(|n| n.solid.sides.get(f.side))
                .and_then(|s| s.disp.as_deref())
                .unwrap();
            let mb = hp_vmf::multiblend::MultiBlend::of(d).unwrap();
            assert!(mb.weights.iter().all(|w| w[1] > 0.99));
        }
    }

    fn wm_values(n: usize) -> Vec<f32> {
        (0..n * n)
            .map(|k| {
                let (i, j) = ((k % n) as f32, (k / n) as f32);
                -37.375 + i * 3.062_5 + (j * 0.7).sin() * 211.123_46 + (i * j) * 0.001_953_125
            })
            .collect()
    }

    fn vertex_heights(state: &AppState) -> Vec<(glam::DVec3, f64)> {
        mesh_points(state).into_iter().map(|p| (p, p.z)).collect()
    }

    #[test]
    fn an_r32_float_heightmap_keeps_its_exact_heights_in_absolute_mode() {
        let d = temp("r32abs");
        let n = 17usize;
        let values = wm_values(n);
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let path = d.join("wm.r32");
        std::fs::write(&path, bytes).unwrap();
        let mut state = test_state();
        let mut o = options(&path);
        o.values = HeightValues::Absolute;
        o.setup.tile_size = 256.0;
        o.setup.tile_power = 3;
        o.setup.origin = glam::DVec3::new(0.0, 0.0, 100.0);
        o.match_pixels = true;
        let report = import(&mut state, &o).unwrap();
        assert_eq!(report.tiles, 4);
        assert_eq!(report.image, (17, 17));
        assert!(report.described.contains("32 bit float"), "{}", report.described);
        let lo = values.iter().copied().fold(f32::MAX, f32::min);
        assert_eq!(report.h_min, f64::from(lo));
        for (p, z) in vertex_heights(&state) {
            let i = (p.x / 64.0).round() as usize;
            let row = n - 1 - (p.y / 64.0).round() as usize;
            let want = 100.0 + f64::from(values[row * n + i]);
            assert!((z - want).abs() < 1e-9, "{p:?} {want}");
        }
        let faces = crate::disp::commands::targets(&state.doc);
        let out = d.join("back.r32");
        let ex = crate::terrain::export::ExportOptions {
            path: out.to_string_lossy().into_owned(),
            ..crate::terrain::export::ExportOptions::default()
        };
        let r = crate::terrain::export::export(&state, &faces, &ex).unwrap();
        assert!(r.absolute);
        let back = super::super::image_io::read_pixels(&out, &RawSpec::default()).unwrap();
        assert_eq!((back.width, back.height), (n, n));
        for (a, b) in back.data.iter().zip(&values) {
            assert_eq!(*a as f32, *b + 100.0, "exact to the float");
        }
    }

    #[test]
    fn exr_tiff_and_pfm_heightmaps_import_the_same_terrain() {
        let d = temp("floatfmt");
        let n = 9usize;
        let values: Vec<f64> = wm_values(n).iter().map(|v| f64::from(*v)).collect();
        let mut heights: Vec<Vec<f64>> = Vec::new();
        for name in ["h.exr", "h.tif", "h.pfm"] {
            let path = d.join(name);
            let fmt = super::super::image_io::OutFormat::for_path(&path, true).unwrap();
            super::super::image_io::write_gray(&path, n, n, &values, fmt).unwrap();
            let mut state = test_state();
            let mut o = options(&path);
            o.values = HeightValues::Absolute;
            o.value_scale = 2.0;
            o.setup.size = DVec2::new(512.0, 512.0);
            o.setup.tile_size = 512.0;
            o.setup.tile_power = 3;
            import(&mut state, &o).unwrap();
            let mut zs: Vec<f64> = mesh_points(&state).iter().map(|p| p.z).collect();
            zs.sort_by(f64::total_cmp);
            heights.push(zs);
        }
        let mut want: Vec<f64> = values.iter().map(|v| v * 2.0).collect();
        want.sort_by(f64::total_cmp);
        for zs in &heights {
            assert_eq!(zs.len(), want.len());
            for (a, b) in zs.iter().zip(&want) {
                assert!((a - b).abs() < 1e-9, "{a} {b}");
            }
        }
    }

    #[test]
    fn stretch_and_unit_values_map_the_range() {
        let d = temp("stretch");
        let path = d.join("s.pfm");
        let values = vec![0.25, 0.5, 0.5, 0.75];
        super::super::image_io::write_gray(&path, 2, 2, &values, super::super::image_io::OutFormat::Pfm).unwrap();
        let state = test_state();
        let mut o = options(&path);
        o.values = HeightValues::Stretch;
        let p = plan(&o, &state).unwrap();
        assert_eq!(p.field.min_max(), (0.0, 1.0));
        assert_eq!(p.raw_range, (0.25, 0.75));
        o.values = HeightValues::Unit;
        assert_eq!(plan(&o, &state).unwrap().field.min_max(), (0.25, 0.75));
        o.values = HeightValues::Auto;
        assert_eq!(plan(&o, &state).unwrap().field.min_max(), (0.25, 0.75), "floats within 0..1 are kept");
        o.values = HeightValues::Absolute;
        o.value_scale = 0.0;
        assert!(plan(&o, &state).is_err());
    }

    #[test]
    fn match_pixels_sets_one_vertex_per_pixel_or_explains_why_not() {
        let mut s = TileSetup { size: DVec2::new(2048.0, 100.0), tile_power: 4, ..TileSetup::default() };
        match_pixels(&mut s, 1025, 513).unwrap();
        assert_eq!(s.tiles(), (64, 32));
        assert_eq!(s.cells(), (1025, 513));
        assert_eq!(s.size.y, 1024.0);
        assert!(match_pixels(&mut s, 1024, 1024).is_err());
    }

    #[test]
    fn tiled_world_machine_exports_are_stitched_on_their_shared_edges() {
        let d = temp("mosaic");
        let n = 17usize;
        let whole: Vec<f32> = wm_values(n);
        let mut paths = Vec::new();
        for ty in 0..2 {
            for tx in 0..2 {
                let tile: Vec<f32> = (0..81).map(|k| whole[(ty * 8 + k / 9) * n + tx * 8 + k % 9]).collect();
                let p = d.join(format!("t_x{tx}_y{ty}.r32"));
                std::fs::write(&p, tile.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
                paths.push(p.to_string_lossy().into_owned());
            }
        }
        let state = test_state();
        let mut o = options(&d.join("unused.png"));
        o.path.clear();
        o.tiles = paths;
        o.values = HeightValues::Absolute;
        let src = load_source(&state, &o).unwrap();
        assert_eq!((src.field.width, src.field.height), (n, n));
        assert!(src.described.contains("shared edges"));
        for (a, b) in src.field.data.iter().zip(&whole) {
            assert_eq!(*a, f64::from(*b));
        }
        o.tile_columns = 3;
        assert!(load_source(&state, &o).is_err());
    }

    #[test]
    fn bad_input_is_reported_and_changes_nothing() {
        let mut state = test_state();
        let mut o = ImportOptions::default();
        assert!(import(&mut state, &o).is_err());
        o.path = "E:/definitely/not/here.png".to_owned();
        assert!(import(&mut state, &o).is_err());
        assert!(state.doc.disp_faces().is_empty());
    }
}
