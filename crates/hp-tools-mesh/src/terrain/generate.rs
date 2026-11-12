use glam::{DVec2, DVec3};
use hp_app::runtime::TerrainPreview;
use hp_app::state::AppState;
use hp_editor::{Document, FaceRef};
use hp_geom::{AutoTexture, GenParams, HydraulicParams, TerrainPreset, ThermalParams, generate_field};

use super::{Source, Terrain, TileSetup, UNDO_GENERATE, create, replace};
use crate::disp::commands::targets;

pub const PREVIEW_SIDE: usize = 65;
pub const PREVIEW_PASSES: u32 = 200;
pub const MAX_REPLACE_SIDE: usize = 1025;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ErosionOptions {
    pub thermal: bool,
    pub thermal_params: ThermalParams,
    pub hydraulic: bool,
    pub hydraulic_params: HydraulicParams,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenOptions {
    pub preset: TerrainPreset,
    pub params: GenParams,
    pub setup: TileSetup,
    pub erosion: ErosionOptions,
    pub auto_texture: bool,
    pub auto: AutoTexture,
    pub replace: bool,
}

impl Default for GenOptions {
    fn default() -> Self {
        let setup = TileSetup { h_max: 768.0, ..TileSetup::default() };
        Self {
            preset: TerrainPreset::RollingHills,
            params: GenParams::preset(TerrainPreset::RollingHills, 1),
            setup,
            erosion: ErosionOptions::default(),
            auto_texture: true,
            auto: AutoTexture::default(),
            replace: false,
        }
    }
}

impl GenOptions {
    pub fn apply_preset(&mut self, preset: TerrainPreset) {
        self.preset = preset;
        self.params = GenParams::preset(preset, self.params.seed);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenReport {
    pub tiles: usize,
    pub faces: usize,
    pub seed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent {
    pub min: DVec2,
    pub size: DVec2,
    pub spacing: f64,
    pub plane_z: f64,
}

pub fn replace_extent(doc: &Document, faces: &[FaceRef]) -> Option<Extent> {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    let mut spacing = f64::INFINITY;
    for f in faces {
        let Some(mesh) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) else { continue };
        let n = mesh.row_len;
        if n < 2 {
            continue;
        }
        for p in &mesh.base_positions {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        if let (Some(a), Some(b)) = (mesh.base_positions.first(), mesh.base_positions.get(n - 1)) {
            spacing = spacing.min(a.distance(*b) / (n - 1) as f64);
        }
    }
    let size = DVec2::new(hi.x - lo.x, hi.y - lo.y);
    (spacing.is_finite() && spacing > 1e-3 && size.min_element() > 1e-3).then_some(Extent {
        min: DVec2::new(lo.x, lo.y),
        size,
        spacing,
        plane_z: lo.z,
    })
}

pub fn terrain_for(
    opts: &GenOptions,
    min: DVec2,
    size: DVec2,
    cells: (usize, usize),
    spacing: f64,
    cap: Option<u32>,
) -> Terrain {
    let mut field = generate_field(&opts.params, cells.0, cells.1, (size.x, size.y));
    let units = opts.setup.h_max - opts.setup.h_min;
    let cell = (size.x / (cells.0 - 1).max(1) as f64, size.y / (cells.1 - 1).max(1) as f64);
    let limit = |n: u32| cap.map_or(n, |c| n.min(c));
    let thermal = opts.erosion.thermal.then(|| ThermalParams {
        iterations: limit(opts.erosion.thermal_params.iterations),
        ..opts.erosion.thermal_params
    });
    let hydraulic = opts.erosion.hydraulic.then(|| HydraulicParams {
        iterations: limit(opts.erosion.hydraulic_params.iterations),
        seed: opts.params.seed,
        ..opts.erosion.hydraulic_params
    });
    field.erode(units, cell, thermal.as_ref(), hydraulic.as_ref());
    Terrain {
        field,
        min,
        size,
        h_min: opts.setup.h_min,
        h_max: opts.setup.h_max,
        skirt: opts.setup.skirt,
        spacing,
        bilinear: true,
        grid: super::PixelGrid::Corner,
        source: if opts.auto_texture { Source::Auto(opts.auto) } else { Source::None },
    }
}

pub fn generate(state: &mut AppState, opts: &GenOptions) -> Result<GenReport, String> {
    generate_as(state, opts, UNDO_GENERATE)
}

pub fn generate_as(state: &mut AppState, opts: &GenOptions, label: &str) -> Result<GenReport, String> {
    if opts.replace {
        let faces = targets(&state.doc);
        if faces.is_empty() {
            return Err("no displacement is selected".to_owned());
        }
        let extent = replace_extent(&state.doc, &faces).ok_or("the selected displacements have no surface")?;
        let setup = TileSetup { skirt: opts.setup.skirt, ..opts.setup.clone() };
        if !(setup.h_max.is_finite() && setup.h_min.is_finite() && setup.h_max >= setup.h_min) {
            return Err("the height range must have min at most max".to_owned());
        }
        let cx = ((extent.size.x / extent.spacing).round() as usize + 1).clamp(2, MAX_REPLACE_SIDE);
        let cy = ((extent.size.y / extent.spacing).round() as usize + 1).clamp(2, MAX_REPLACE_SIDE);
        let spacing = (extent.size.x / (cx - 1) as f64).min(extent.size.y / (cy - 1) as f64);
        let terrain = terrain_for(opts, extent.min, extent.size, (cx, cy), spacing, None);
        let n = replace(state, &terrain, &faces, label)?;
        return Ok(GenReport { tiles: 0, faces: n, seed: opts.params.seed });
    }
    opts.setup.validate()?;
    let terrain = terrain_for(
        opts,
        opts.setup.min(),
        opts.setup.size,
        opts.setup.cells(),
        opts.setup.spacing().min_element(),
        None,
    );
    let n = create(state, &opts.setup, &terrain, label)?;
    Ok(GenReport { tiles: n, faces: n, seed: opts.params.seed })
}

pub fn preview(doc: &Document, opts: &GenOptions, version: u64) -> Option<TerrainPreview> {
    let (min, size, plane_z) = if opts.replace {
        let faces = targets(doc);
        let e = replace_extent(doc, &faces)?;
        (e.min, e.size, e.plane_z)
    } else {
        if opts.setup.validate().is_err() {
            return None;
        }
        (opts.setup.min(), opts.setup.size, opts.setup.plane_z())
    };
    let long = size.x.max(size.y);
    let nx = ((size.x / long * (PREVIEW_SIDE - 1) as f64).round() as usize + 1).clamp(2, PREVIEW_SIDE);
    let ny = ((size.y / long * (PREVIEW_SIDE - 1) as f64).round() as usize + 1).clamp(2, PREVIEW_SIDE);
    let spacing = (size.x / (nx - 1) as f64).min(size.y / (ny - 1) as f64);
    let terrain = terrain_for(opts, min, size, (nx, ny), spacing, Some(PREVIEW_PASSES));
    let range = opts.setup.h_max - opts.setup.h_min;
    let heights = terrain.field.data.iter().map(|v| (plane_z + range * v) as f32).collect();
    Some(TerrainPreview { version, min: [min.x, min.y], size: [size.x, size.y], nx, ny, heights })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use hp_app::testing::test_state;

    use super::*;

    fn small() -> GenOptions {
        let mut o = GenOptions::default();
        o.setup.size = DVec2::new(1024.0, 1024.0);
        o.setup.tile_size = 512.0;
        o.setup.tile_power = 2;
        o.setup.h_max = 300.0;
        o
    }

    fn heights(state: &AppState) -> Vec<f64> {
        let mut out = Vec::new();
        for f in state.doc.disp_faces() {
            if let Some(m) = state.doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) {
                out.extend(m.positions.iter().map(|p| p.z));
            }
        }
        out
    }

    #[test]
    fn the_same_seed_gives_the_same_terrain_and_another_seed_a_different_one() {
        let mut a = test_state();
        let mut b = test_state();
        let mut c = test_state();
        let o = small();
        generate(&mut a, &o).unwrap();
        generate(&mut b, &o).unwrap();
        let mut other = o.clone();
        other.params.seed = 99;
        generate(&mut c, &other).unwrap();
        assert_eq!(heights(&a), heights(&b));
        assert_ne!(heights(&a), heights(&c));
        let hs = heights(&a);
        let lo = hs.iter().copied().fold(f64::MAX, f64::min);
        let hi = hs.iter().copied().fold(f64::MIN, f64::max);
        assert!(lo >= -1e-6 && hi <= 300.0 + 1e-6 && hi - lo > 20.0, "{lo} {hi}");
    }

    #[test]
    fn generated_tiles_are_sewn_and_one_undo_step() {
        let mut state = test_state();
        let mut o = small();
        o.erosion.thermal = true;
        o.erosion.hydraulic = true;
        o.erosion.hydraulic_params.iterations = 20;
        let report = generate(&mut state, &o).unwrap();
        assert_eq!(report.tiles, 4);
        let mut seen = std::collections::HashMap::new();
        for f in state.doc.disp_faces() {
            let m = state.doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)).unwrap();
            for p in &m.positions {
                let key = ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64);
                let z = *seen.entry(key).or_insert(p.z);
                assert!((z - p.z).abs() < 1e-6, "seam at {key:?}");
            }
        }
        assert!(state.doc.undo());
        assert!(state.doc.disp_faces().is_empty());
    }

    #[test]
    fn replace_mode_regenerates_the_selected_displacements() {
        let mut state = test_state();
        let mut o = small();
        generate(&mut state, &o).unwrap();
        let before = heights(&state);
        o.replace = true;
        o.params.seed = 1234;
        let n = generate(&mut state, &o).unwrap().faces;
        assert_eq!(n, 4);
        assert_ne!(before, heights(&state));
        assert_eq!(state.doc.disp_faces().len(), 4);
        assert!(state.doc.undo());
        assert_eq!(before, heights(&state));
    }

    #[test]
    fn replace_without_selection_fails_and_preview_has_a_low_resolution() {
        let mut state = test_state();
        let mut o = small();
        o.replace = true;
        assert!(generate(&mut state, &o).is_err());
        o.replace = false;
        let p = preview(&state.doc, &o, 3).unwrap();
        assert_eq!((p.nx, p.ny), (PREVIEW_SIDE, PREVIEW_SIDE));
        assert_eq!(p.heights.len(), p.nx * p.ny);
        assert_eq!(p.version, 3);
        o.setup.size = DVec2::new(2048.0, 1024.0);
        let wide = preview(&state.doc, &o, 4).unwrap();
        assert!(wide.nx > wide.ny);
    }

    #[test]
    fn presets_all_generate() {
        for preset in TerrainPreset::ALL {
            let mut state = test_state();
            let mut o = small();
            o.apply_preset(preset);
            assert_eq!(generate(&mut state, &o).unwrap().tiles, 4, "{preset:?}");
        }
    }
}
