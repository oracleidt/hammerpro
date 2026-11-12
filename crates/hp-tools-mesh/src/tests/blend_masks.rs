use hp_vmf::multiblend::MultiBlend;
use image::{ImageBuffer, Luma};

use super::*;
use crate::terrain::TileSetup;
use crate::terrain::image_io::{Channel, OutFormat, RawSpec, read_pixels, write_gray, write_rgba};
use crate::terrain::import::{ImportOptions, import};
use crate::terrain::masks::{
    Layer, Levels, MaskExportOptions, MaskOptions, MaskSource, MaskTarget, UNDO_MASKS, WeightMode, coverage_to_rows,
    export_masks, import_masks,
};

const N: usize = 9;

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("hp_masks_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn terrain(dir: &std::path::Path) -> AppState {
    let flat: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::from_pixel(N as u32, N as u32, Luma([0]));
    let path = dir.join("flat.png");
    flat.save(&path).unwrap();
    let mut state = test_state();
    let o = ImportOptions {
        path: path.to_string_lossy().into_owned(),
        setup: TileSetup {
            size: DVec2::new(512.0, 512.0),
            tile_size: 256.0,
            tile_power: 2,
            h_min: 0.0,
            h_max: 64.0,
            centered: false,
            ..TileSetup::default()
        },
        ..ImportOptions::default()
    };
    import(&mut state, &o).unwrap();
    state
}

fn faces(state: &AppState) -> Vec<FaceRef> {
    state.doc.disp_faces()
}

fn pixel(p: DVec3) -> usize {
    let i = (p.x / 64.0).round() as usize;
    let row = N - 1 - (p.y / 64.0).round() as usize;
    row * N + i
}

fn each_vertex(state: &AppState, mut f: impl FnMut(usize, &hp_vmf::DispInfo, usize)) {
    for face in faces(state) {
        let node = state.doc.scene().solid(face.solid).unwrap();
        let mesh = node.disp_mesh(face.side).unwrap();
        let disp = node.solid.sides[face.side].disp.as_deref().unwrap();
        for (k, p) in mesh.positions.iter().enumerate() {
            f(pixel(*p), disp, k);
        }
    }
}

fn path(dir: &std::path::Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

fn coverage(px: usize) -> [f64; 4] {
    let raw = [(px % 5) as f64, (px % 3) as f64, ((px / 9) % 4) as f64, if px.is_multiple_of(7) { 3.0 } else { 0.0 }];
    let sum: f64 = raw.iter().sum();
    if sum > 0.0 { raw.map(|v| v / sum) } else { [1.0, 0.0, 0.0, 0.0] }
}

#[test]
fn a_gray_mask_sets_the_alpha_of_every_vertex_in_one_undo_step() {
    let d = temp("alpha");
    let mut state = terrain(&d);
    let values: Vec<u16> = (0..N * N).map(|k| (k * 800) as u16).collect();
    let mask: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::from_raw(N as u32, N as u32, values.clone()).unwrap();
    mask.save(d.join("a.png")).unwrap();
    let steps = undo_steps(&state);
    let opts = MaskOptions { files: vec![path(&d, "a.png")], target: MaskTarget::Alpha, ..MaskOptions::default() };
    let all = faces(&state);
    let r = import_masks(&mut state, &all, &opts, UNDO_MASKS).unwrap();
    assert_eq!((r.faces, r.alpha_faces, r.four_way_faces), (4, 4, 0));
    assert_eq!(undo_steps(&state), steps + 1);
    let mut checked = 0;
    each_vertex(&state, |px, disp, k| {
        let expect = 255.0 * f64::from(values[px]) / 65535.0;
        assert!((disp.alphas[k] - expect).abs() < 1e-9, "{} {expect}", disp.alphas[k]);
        assert!(MultiBlend::of(disp).is_none());
        checked += 1;
    });
    assert_eq!(checked, 4 * 25);
    assert!(state.doc.undo());
    each_vertex(&state, |_, disp, k| assert_eq!(disp.alphas[k], 0.0));
}

#[test]
fn float_rgba_masks_become_4way_rows_and_export_back_exactly() {
    let d = temp("fourway");
    let mut state = terrain(&d);
    let cover: Vec<[f64; 4]> = (0..N * N).map(|px| coverage(px).map(|v| f64::from(v as f32))).collect();
    write_rgba(&d.join("splat.exr"), N, N, &cover, OutFormat::Exr).unwrap();
    let opts =
        MaskOptions { files: vec![path(&d, "splat.exr")], target: MaskTarget::FourWay, ..MaskOptions::default() };
    let all = faces(&state);
    let r = import_masks(&mut state, &all, &opts, UNDO_MASKS).unwrap();
    assert_eq!(r.four_way_faces, 4);
    each_vertex(&state, |px, disp, k| {
        let mb = MultiBlend::of(disp).unwrap();
        let sum: f64 = cover[px].iter().sum();
        let want = coverage_to_rows(cover[px].map(|v| v / sum));
        for (a, b) in mb.weights[k].iter().zip(want) {
            assert!((a - b).abs() <= 5e-7, "{:?} {want:?}", mb.weights[k]);
        }
        assert!((disp.alphas[k] - 255.0 * (1.0 - cover[px][0] / sum)).abs() < 1e-6);
    });
    let out = MaskExportOptions {
        path: path(&d, "back.exr"),
        alpha_path: path(&d, "alpha.png"),
        layer_paths: [path(&d, "t1.r32"), String::new(), String::new(), path(&d, "t4.tif")],
        ..MaskExportOptions::default()
    };
    let rep = export_masks(&state, &all, &out).unwrap();
    assert_eq!((rep.width, rep.height, rep.files.len()), (N, N, 4));
    let back = read_pixels(&d.join("back.exr"), &RawSpec::default()).unwrap();
    for (c, name) in [Channel::R, Channel::G, Channel::B, Channel::A].iter().zip(0..4) {
        let plane = back.channel(*c);
        for (px, c) in cover.iter().enumerate() {
            let sum: f64 = c.iter().sum();
            assert!((plane.data[px] - c[name] / sum).abs() < 3e-6, "{px} {name}");
        }
    }
    let t1 = read_pixels(&d.join("t1.r32"), &RawSpec::default()).unwrap();
    assert!((t1.data[10] - back.channel(Channel::R).data[10]).abs() < 1e-7);
    let t4 = read_pixels(&d.join("t4.tif"), &RawSpec::default()).unwrap();
    assert_eq!(t4.bits, 16);
    let alpha = read_pixels(&d.join("alpha.png"), &RawSpec::default()).unwrap();
    assert!((alpha.data[11] - (1.0 - back.channel(Channel::R).data[11])).abs() < 1.0 / 65535.0 + 1e-6);
    let direct =
        MaskExportOptions { path: path(&d, "rows.png"), weights: WeightMode::Direct, ..MaskExportOptions::default() };
    export_masks(&state, &all, &direct).unwrap();
    let rows = read_pixels(&d.join("rows.png"), &RawSpec::default()).unwrap();
    assert_eq!(rows.bits, 16);
}

#[test]
fn direct_mode_writes_the_weights_as_given_and_auto_follows_existing_rows() {
    let d = temp("direct");
    let mut state = terrain(&d);
    let cover: Vec<[f64; 4]> = (0..N * N).map(|_| [0.0, 0.5, 0.5, 0.0]).collect();
    write_rgba(&d.join("s.png"), N, N, &cover, OutFormat::Png16).unwrap();
    let all = faces(&state);
    let first = all[..1].to_vec();
    let opts = MaskOptions {
        files: vec![path(&d, "s.png")],
        target: MaskTarget::FourWay,
        weights: WeightMode::Direct,
        ..MaskOptions::default()
    };
    import_masks(&mut state, &first, &opts, UNDO_MASKS).unwrap();
    let auto = MaskOptions { target: MaskTarget::Auto, weights: WeightMode::Coverage, ..opts };
    let r = import_masks(&mut state, &all, &auto, UNDO_MASKS).unwrap();
    assert_eq!((r.four_way_faces, r.alpha_faces), (1, 3), "rows mark the 4-way face, the rest get the alpha");
    let node = state.doc.scene().solid(first[0].solid).unwrap();
    let mb = MultiBlend::of(node.solid.sides[first[0].side].disp.as_deref().unwrap()).unwrap();
    assert!(mb.weights.iter().all(|w| (w[1] - 1.0).abs() < 1e-4 && (w[2] - 0.5).abs() < 1e-4), "{:?}", mb.weights[0]);
}

#[test]
fn channels_levels_and_an_explicit_extent_line_up() {
    let d = temp("levels");
    let mut state = terrain(&d);
    let values: Vec<f64> = (0..N * N).map(|k| (k % N) as f64 / (N - 1) as f64).collect();
    write_gray(&d.join("ramp.pfm"), N, N, &values, OutFormat::Pfm).unwrap();
    let opts = MaskOptions {
        files: vec![String::new(), path(&d, "ramp.pfm")],
        target: MaskTarget::Alpha,
        alpha: Layer { source: MaskSource::image(1, Channel::R), levels: Levels { invert: true, ..Levels::default() } },
        extent: Some((DVec2::new(0.0, 0.0), DVec2::new(1024.0, 512.0))),
        ..MaskOptions::default()
    };
    let all = faces(&state);
    import_masks(&mut state, &all, &opts, UNDO_MASKS).unwrap();
    for face in &all {
        let node = state.doc.scene().solid(face.solid).unwrap();
        let mesh = node.disp_mesh(face.side).unwrap();
        let disp = node.solid.sides[face.side].disp.as_deref().unwrap();
        for (k, p) in mesh.positions.iter().enumerate() {
            let expect = 255.0 * (1.0 - p.x / 1024.0);
            assert!((disp.alphas[k] - expect).abs() < 1e-6, "{p:?} {}", disp.alphas[k]);
        }
    }
    let bad = MaskOptions { files: vec![], ..MaskOptions::default() };
    assert!(import_masks(&mut state, &all, &bad, UNDO_MASKS).is_err());
    assert!(import_masks(&mut state, &[], &opts, UNDO_MASKS).is_err());
}

#[test]
fn splat_texturing_of_the_heightmap_import_uses_the_same_coverage() {
    let d = temp("splat");
    let cover: Vec<[f64; 4]> = (0..N * N).map(|_| [0.0, 0.5, 0.5, 0.0]).collect();
    write_rgba(&d.join("s.tif"), N, N, &cover, OutFormat::Tiff16).unwrap();
    let flat: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::from_pixel(N as u32, N as u32, Luma([0]));
    flat.save(d.join("h.png")).unwrap();
    let mut state = test_state();
    let mut o = ImportOptions {
        path: path(&d, "h.png"),
        setup: TileSetup {
            size: DVec2::new(512.0, 512.0),
            tile_size: 512.0,
            tile_power: 2,
            centered: false,
            ..TileSetup::default()
        },
        ..ImportOptions::default()
    };
    o.tex.mode = crate::terrain::TexMode::Splat;
    o.tex.splat = path(&d, "s.tif");
    let steps = undo_steps(&state);
    import(&mut state, &o).unwrap();
    assert_eq!(undo_steps(&state), steps + 1, "terrain and blend in one step");
    each_vertex(&state, |_, disp, k| {
        let w = MultiBlend::of(disp).unwrap().weights[k];
        assert!((w[1] - 1.0).abs() < 1e-4 && (w[2] - 0.5).abs() < 1e-4, "{w:?}");
    });
}

struct Mats;

impl hp_bspc::MaterialSource for Mats {
    fn facts(&self, name: &str) -> hp_bspc::MaterialFacts {
        hp_bspc::materials::facts_by_name(name)
    }
}

#[test]
fn the_compiled_bsp_carries_the_imported_alpha_and_4way_rows() {
    let d = temp("bsp");
    let mut state = terrain(&d);
    let cover: Vec<[f64; 4]> = (0..N * N).map(coverage).collect();
    write_rgba(&d.join("s.exr"), N, N, &cover, OutFormat::Exr).unwrap();
    let values: Vec<f64> = (0..N * N).map(|k| k as f64 / (N * N) as f64).collect();
    write_gray(&d.join("a.exr"), N, N, &values, OutFormat::Exr).unwrap();
    let all = faces(&state);
    let (four, wvt) = all.split_at(2);
    let fw = MaskOptions { files: vec![path(&d, "s.exr")], target: MaskTarget::FourWay, ..MaskOptions::default() };
    import_masks(&mut state, four, &fw, UNDO_MASKS).unwrap();
    let alpha = MaskOptions { files: vec![path(&d, "a.exr")], target: MaskTarget::Alpha, ..MaskOptions::default() };
    import_masks(&mut state, wvt, &alpha, UNDO_MASKS).unwrap();
    let map = state.doc.to_map();
    let out =
        hp_bspc::compile(&map, &Mats, &hp_bspc::props::NoModels, &hp_bspc::CompileOptions::default(), &mut |_| {})
            .unwrap();
    let problems = hp_bspc::validate::validate(&out.bsp);
    assert!(problems.is_empty(), "{problems:?}");
    let bsp = hp_bsp::Bsp::from_vec(out.bsp).unwrap();
    let blend = bsp.lump(hp_bsp::lump::DISP_MULTIBLEND).unwrap().into_owned();
    assert_eq!(blend.len(), 2 * 25 * hp_bspc::disp::MULTIBLEND_ENTRY_SIZE, "rows for the two 4-way tiles only");
    let back = hp_decompile::decompile(&bsp, &hp_decompile::Options::default()).unwrap();
    let source: Vec<&hp_vmf::DispInfo> =
        map.world.solids.iter().flat_map(|s| &s.sides).filter_map(|s| s.disp.as_deref()).collect();
    let decompiled: Vec<&hp_vmf::DispInfo> =
        back.map.world.solids.iter().flat_map(|s| &s.sides).filter_map(|s| s.disp.as_deref()).collect();
    assert_eq!(decompiled.len(), 4);
    let mut rows = 0;
    let mut alphas = 0;
    for disp in &decompiled {
        let twin = source
            .iter()
            .find(|s| (s.start_position - disp.start_position).length() < 0.5)
            .expect("same displacement in the map");
        match (MultiBlend::of(twin), MultiBlend::of(disp)) {
            (Some(a), Some(b)) => {
                for (x, y) in a.weights.iter().flatten().zip(b.weights.iter().flatten()) {
                    assert!((x - y).abs() < 1e-5, "{x} {y}");
                }
                rows += 1;
            }
            (None, None) => {
                for (x, y) in twin.alphas.iter().zip(&disp.alphas) {
                    assert!((x - y).abs() < 1e-3, "{x} {y}");
                }
                assert!(disp.alphas.iter().any(|a| *a > 100.0));
                alphas += 1;
            }
            other => panic!("rows lost or invented: {:?}", (other.0.is_some(), other.1.is_some())),
        }
    }
    assert_eq!((rows, alphas), (2, 2));
}
