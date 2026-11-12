mod common;

use glam::DVec2;
use hp_editor::Document;
use hp_editor::terrain_ops::{TerrainLayout, TerrainSurface};

fn terrain_map(tiles: u32) -> hp_vmf::Map {
    let mut doc = Document::new_empty();
    let layout = TerrainLayout {
        min: DVec2::new(-512.0, -512.0),
        size: DVec2::new(1024.0, 1024.0),
        tiles: (tiles, tiles),
        power: 3,
        plane_z: 0.0,
        thickness: 16.0,
        material: "dev/dev_measuregeneric01b".to_owned(),
        lightmap_scale: 16,
    };
    let height = |x: f64, y: f64| 40.0 + (x * 0.01).sin() * 30.0 + (y * 0.013).cos() * 20.0;
    let surface = TerrainSurface { height: &height, weights: None };
    doc.create_terrain(&layout, &surface, "Import heightmap").expect("terrain");
    doc.to_map()
}

fn compile(map: &hp_vmf::Map, opts: &hp_bspc::CompileOptions) -> (hp_bspc::CompileOutput, Vec<String>) {
    let mut log = Vec::new();
    let out =
        hp_bspc::compile(map, &common::GameMats, &hp_bspc::props::NoModels, opts, &mut |l| log.push(l.to_owned()))
            .unwrap_or_else(|e| panic!("compiles: {e}: {log:?}"));
    (out, log)
}

#[test]
fn a_terrain_without_any_entity_compiles_like_a_leaked_map_with_its_displacements() {
    let preview = hp_bspc::CompileOptions {
        vis: hp_bspc::VisLevel::Fast,
        relaxed_limits: true,
        lean_planes: true,
        preview: true,
        ..Default::default()
    };
    let full = hp_bspc::CompileOptions { leak_test: true, ..Default::default() };
    for (tiles, opts) in [(1, &preview), (2, &preview), (2, &full)] {
        let map = terrain_map(tiles);
        let (out, log) = compile(&map, opts);
        assert!(log.iter().any(|l| l == hp_bspc::compile::NO_INSIDE), "{log:?}");
        assert!(hp_bspc::validate::validate(&out.bsp).is_empty());
        let bsp = hp_bsp::Bsp::from_vec(out.bsp).expect("bsp");
        let info = bsp.lump(hp_bspc::emit::lump::DISPINFO).expect("dispinfo");
        assert_eq!(info.len() / 176, (tiles * tiles) as usize, "every tile is kept");
    }
}
