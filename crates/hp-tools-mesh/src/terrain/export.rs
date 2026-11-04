use hp_editor::{Document, FaceRef};
use hp_vmf::multiblend::MultiBlend;

use super::image_io::{write_heights, write_splat};

pub const MAX_EXPORT_SIDE: usize = 8192;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportOptions {
    pub path: String,
    pub splat: String,
    pub min_height: Option<f64>,
    pub max_height: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportReport {
    pub width: usize,
    pub height: usize,
    pub min_height: f64,
    pub max_height: f64,
    pub size: (f64, f64),
    pub min: (f64, f64),
    pub faces: usize,
    pub splat: bool,
}

pub struct Raster {
    pub width: usize,
    pub height: usize,
    pub heights: Vec<f64>,
    pub weights: Vec<[f64; 4]>,
    pub min: (f64, f64),
    pub size: (f64, f64),
}

pub fn rasterize(doc: &Document, faces: &[FaceRef]) -> Option<Raster> {
    struct Part<'a> {
        mesh: &'a hp_geom::DispMesh,
        weights: Vec<[f64; 4]>,
    }
    let mut parts: Vec<Part<'_>> = Vec::new();
    let (mut lo, mut hi) = ((f64::INFINITY, f64::INFINITY), (f64::NEG_INFINITY, f64::NEG_INFINITY));
    let mut spacing = f64::INFINITY;
    for f in faces {
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(disp)) =
            (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
        else {
            continue;
        };
        let n = mesh.row_len;
        if n < 2 || mesh.positions.len() != n * n {
            continue;
        }
        for p in &mesh.positions {
            lo = (lo.0.min(p.x), lo.1.min(p.y));
            hi = (hi.0.max(p.x), hi.1.max(p.y));
        }
        if let (Some(a), Some(b)) = (mesh.base_positions.first(), mesh.base_positions.get(n - 1)) {
            spacing = spacing.min(a.distance(*b) / (n - 1) as f64);
        }
        let weights = MultiBlend::of_or_alphas(disp).map(|m| m.weights).unwrap_or_default();
        parts.push(Part { mesh, weights });
    }
    let size = (hi.0 - lo.0, hi.1 - lo.1);
    if parts.is_empty() || !(spacing.is_finite() && spacing > 1e-3) || size.0 < 1e-3 || size.1 < 1e-3 {
        return None;
    }
    let width = ((size.0 / spacing).round() as usize + 1).clamp(2, MAX_EXPORT_SIDE);
    let height = ((size.1 / spacing).round() as usize + 1).clamp(2, MAX_EXPORT_SIDE);
    let (dx, dy) = (size.0 / (width - 1) as f64, size.1 / (height - 1) as f64);
    let mut heights = vec![f64::NAN; width * height];
    let mut weights = vec![[1.0, 0.0, 0.0, 0.0]; width * height];
    for part in &parts {
        let m = part.mesh;
        for tri in m.indices.chunks_exact(3) {
            let idx = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let (Some(a), Some(b), Some(c)) =
                (m.positions.get(idx[0]), m.positions.get(idx[1]), m.positions.get(idx[2]))
            else {
                continue;
            };
            let denom = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
            if denom.abs() < 1e-12 {
                continue;
            }
            let (tx0, tx1) = (a.x.min(b.x).min(c.x), a.x.max(b.x).max(c.x));
            let (ty0, ty1) = (a.y.min(b.y).min(c.y), a.y.max(b.y).max(c.y));
            let i0 = (((tx0 - lo.0) / dx - 1e-6).ceil().max(0.0)) as usize;
            let i1 = (((tx1 - lo.0) / dx + 1e-6).floor().min((width - 1) as f64)) as usize;
            let j0 = (((hi.1 - ty1) / dy - 1e-6).ceil().max(0.0)) as usize;
            let j1 = (((hi.1 - ty0) / dy + 1e-6).floor().min((height - 1) as f64)) as usize;
            for j in j0..=j1 {
                let py = hi.1 - j as f64 * dy;
                for i in i0..=i1 {
                    let px = lo.0 + i as f64 * dx;
                    let l1 = ((b.y - c.y) * (px - c.x) + (c.x - b.x) * (py - c.y)) / denom;
                    let l2 = ((c.y - a.y) * (px - c.x) + (a.x - c.x) * (py - c.y)) / denom;
                    let l3 = 1.0 - l1 - l2;
                    if l1 < -1e-9 || l2 < -1e-9 || l3 < -1e-9 {
                        continue;
                    }
                    let z = l1 * a.z + l2 * b.z + l3 * c.z;
                    let slot = j * width + i;
                    if heights.get(slot).is_some_and(|old| old.is_nan() || z > *old) {
                        heights[slot] = z;
                        if let (Some(wa), Some(wb), Some(wc)) =
                            (part.weights.get(idx[0]), part.weights.get(idx[1]), part.weights.get(idx[2]))
                        {
                            weights[slot] = std::array::from_fn(|k| l1 * wa[k] + l2 * wb[k] + l3 * wc[k]);
                        }
                    }
                }
            }
        }
    }
    fill_holes(&mut heights, &mut weights, width, height);
    Some(Raster { width, height, heights, weights, min: lo, size })
}

fn fill_holes(heights: &mut [f64], weights: &mut [[f64; 4]], width: usize, height: usize) {
    for _ in 0..64 {
        let snapshot = heights.to_vec();
        let wsnap = weights.to_vec();
        let mut changed = false;
        for j in 0..height {
            for i in 0..width {
                let slot = j * width + i;
                if !snapshot.get(slot).is_some_and(|v| v.is_nan()) {
                    continue;
                }
                let mut sum = 0.0;
                let mut w = [0.0; 4];
                let mut count = 0.0;
                for (di, dj) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (ni, nj) = (i as i64 + di, j as i64 + dj);
                    if ni < 0 || nj < 0 || ni >= width as i64 || nj >= height as i64 {
                        continue;
                    }
                    let ns = nj as usize * width + ni as usize;
                    if let Some(v) = snapshot.get(ns).filter(|v| !v.is_nan()) {
                        sum += v;
                        count += 1.0;
                        if let Some(nw) = wsnap.get(ns) {
                            for k in 0..4 {
                                w[k] += nw[k];
                            }
                        }
                    }
                }
                if count > 0.0 {
                    heights[slot] = sum / count;
                    weights[slot] = w.map(|x| x / count);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let floor = heights.iter().copied().filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min);
    for v in heights.iter_mut().filter(|v| v.is_nan()) {
        *v = if floor.is_finite() { floor } else { 0.0 };
    }
}

pub fn export(
    state: &hp_app::state::AppState,
    faces: &[FaceRef],
    opts: &ExportOptions,
) -> Result<ExportReport, String> {
    let raster = rasterize(&state.doc, faces).ok_or("the selected displacements have no surface to export")?;
    let path = super::resolve_path(state, &opts.path)?;
    let data_lo = raster.heights.iter().copied().fold(f64::INFINITY, f64::min);
    let data_hi = raster.heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let lo = opts.min_height.unwrap_or(data_lo);
    let hi = opts.max_height.unwrap_or(data_hi);
    if !(lo.is_finite() && hi.is_finite()) || hi < lo {
        return Err("the height range must have min at most max".to_owned());
    }
    let span = hi - lo;
    let values: Vec<u16> = raster
        .heights
        .iter()
        .map(|z| if span > 1e-9 { ((z - lo) / span).clamp(0.0, 1.0) * 65535.0 } else { 0.0 }.round() as u16)
        .collect();
    write_heights(&path, raster.width, raster.height, &values)?;
    let splat = !opts.splat.trim().is_empty();
    if splat {
        let rgba: Vec<u8> =
            raster.weights.iter().flat_map(|w| w.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)).collect();
        write_splat(&super::resolve_path(state, &opts.splat)?, raster.width, raster.height, &rgba)?;
    }
    Ok(ExportReport {
        width: raster.width,
        height: raster.height,
        min_height: lo,
        max_height: hi,
        size: raster.size,
        min: raster.min,
        faces: faces.len(),
        splat,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use glam::DVec2;
    use hp_app::testing::test_state;

    use super::*;
    use crate::disp::commands::targets;
    use crate::terrain::generate::{GenOptions, generate};
    use crate::terrain::image_io::{RawSpec, load_heightmap};
    use crate::terrain::import::{ImportOptions, import};
    use crate::terrain::{TexMode, TileSetup};

    fn temp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("hp_terrain_exp_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sorted_heights(state: &hp_app::state::AppState) -> Vec<f64> {
        let mut out = Vec::new();
        for f in state.doc.disp_faces() {
            if let Some(m) = state.doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) {
                out.extend(m.positions.iter().map(|p| p.z));
            }
        }
        out.sort_by(f64::total_cmp);
        out
    }

    #[test]
    fn export_then_import_gives_the_same_terrain() {
        let d = temp("round");
        let mut state = test_state();
        let g = GenOptions {
            setup: TileSetup {
                size: DVec2::new(1024.0, 1024.0),
                tile_size: 512.0,
                tile_power: 3,
                h_max: 400.0,
                centered: false,
                ..TileSetup::default()
            },
            auto_texture: true,
            ..GenOptions::default()
        };
        generate(&mut state, &g).unwrap();
        let before = sorted_heights(&state);
        let faces = targets(&state.doc);
        assert_eq!(faces.len(), 4);
        let opts = ExportOptions {
            path: d.join("h.png").to_string_lossy().into_owned(),
            splat: d.join("s.png").to_string_lossy().into_owned(),
            ..ExportOptions::default()
        };
        let report = export(&state, &faces, &opts).unwrap();
        assert_eq!((report.width, report.height), (17, 17));
        assert!(report.splat);
        let f = load_heightmap(&d.join("h.png"), &RawSpec::default()).unwrap();
        assert_eq!((f.width, f.height), (17, 17));

        let mut fresh = test_state();
        let mut o = ImportOptions {
            path: opts.path.clone(),
            setup: TileSetup {
                size: DVec2::new(report.size.0, report.size.1),
                origin: glam::DVec3::new(report.min.0, report.min.1, 0.0),
                centered: false,
                tile_size: 512.0,
                tile_power: 3,
                h_min: report.min_height,
                h_max: report.max_height,
                ..TileSetup::default()
            },
            ..ImportOptions::default()
        };
        o.tex.mode = TexMode::Splat;
        o.tex.splat = opts.splat.clone();
        import(&mut fresh, &o).unwrap();
        let after = sorted_heights(&fresh);
        assert_eq!(before.len(), after.len());
        let tolerance = (report.max_height - report.min_height) / 65535.0 + 1e-6;
        for (a, b) in before.iter().zip(&after) {
            assert!((a - b).abs() <= tolerance, "{a} {b}");
        }
    }

    #[test]
    fn export_writes_a_raw_and_rejects_a_bad_range() {
        let d = temp("raw");
        let mut state = test_state();
        let mut g = GenOptions::default();
        g.setup.size = DVec2::new(512.0, 512.0);
        g.setup.tile_size = 512.0;
        g.setup.tile_power = 2;
        generate(&mut state, &g).unwrap();
        let faces = targets(&state.doc);
        let raw = ExportOptions { path: d.join("h.raw").to_string_lossy().into_owned(), ..ExportOptions::default() };
        let r = export(&state, &faces, &raw).unwrap();
        assert_eq!(std::fs::metadata(d.join("h.raw")).unwrap().len(), (r.width * r.height * 2) as u64);
        let bad = ExportOptions { min_height: Some(10.0), max_height: Some(5.0), ..raw };
        assert!(export(&state, &faces, &bad).is_err());
        assert!(rasterize(&state.doc, &[]).is_none());
    }
}
