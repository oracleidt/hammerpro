use hp_app::state::AppState;
use hp_geom::HeightField;

use super::image_io::{RawSpec, load_gray, load_heightmap, load_splat_rgba};
use super::{Source, Terrain, TexMode, Texturing, TileSetup, UNDO_IMPORT, create, orient, resolve_path};

#[derive(Clone, Debug, Default, PartialEq)]
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportReport {
    pub tiles: usize,
    pub image: (usize, usize),
    pub spacing: f64,
}

pub fn height_field(opts: &ImportOptions, state: &AppState) -> Result<HeightField, String> {
    let path = resolve_path(state, &opts.path)?;
    let loaded = load_heightmap(&path, &opts.raw)?;
    Ok(shape_field(loaded, opts))
}

pub fn shape_field(loaded: HeightField, opts: &ImportOptions) -> HeightField {
    let mut field = orient(loaded, opts.flip_x, opts.flip_y);
    if opts.invert {
        field.invert();
    }
    field.smooth(opts.smoothing);
    field
}

pub fn splat_source(opts: &ImportOptions, state: &AppState) -> Result<Source, String> {
    let channels: [HeightField; 4] = if !opts.tex.splat.trim().is_empty() {
        load_splat_rgba(&resolve_path(state, &opts.tex.splat)?)?
    } else if opts.tex.channels.iter().any(|c| !c.trim().is_empty()) {
        let mut out: Vec<HeightField> = Vec::with_capacity(4);
        for c in &opts.tex.channels {
            out.push(if c.trim().is_empty() {
                HeightField::filled(2, 2, 0.0)
            } else {
                load_gray(&resolve_path(state, c)?)?
            });
        }
        <[HeightField; 4]>::try_from(out).map_err(|_| "four splat channels are needed".to_owned())?
    } else {
        return Err("splat texturing needs a splat map or four grayscale channel images".to_owned());
    };
    Ok(Source::Splat(Box::new(channels.map(|c| orient(c, opts.flip_x, opts.flip_y)))))
}

pub fn build_terrain(opts: &ImportOptions, state: &AppState) -> Result<(Terrain, (usize, usize)), String> {
    opts.setup.validate()?;
    let field = height_field(opts, state)?;
    let image = (field.width, field.height);
    let source = match opts.tex.mode {
        TexMode::None => Source::None,
        TexMode::Auto => Source::Auto(opts.tex.auto),
        TexMode::Splat => splat_source(opts, state)?,
    };
    let terrain = Terrain {
        field,
        min: opts.setup.min(),
        size: opts.setup.size,
        h_min: opts.setup.h_min,
        h_max: opts.setup.h_max,
        skirt: opts.setup.skirt,
        spacing: opts.setup.spacing().min_element(),
        bilinear: opts.bilinear,
        source,
    };
    Ok((terrain, image))
}

pub fn import(state: &mut AppState, opts: &ImportOptions) -> Result<ImportReport, String> {
    import_as(state, opts, UNDO_IMPORT)
}

pub fn import_as(state: &mut AppState, opts: &ImportOptions, label: &str) -> Result<ImportReport, String> {
    let (terrain, image) = build_terrain(opts, state)?;
    let tiles = create(state, &opts.setup, &terrain, label)?;
    Ok(ImportReport { tiles, image, spacing: opts.setup.spacing().min_element() })
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
