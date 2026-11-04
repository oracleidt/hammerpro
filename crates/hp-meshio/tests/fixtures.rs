use std::path::{Path, PathBuf};

use glam::{DVec2, DVec3};
use hp_meshio::smd::Collision;
use hp_meshio::studio::{StudioOptions, compile};
use hp_meshio::{Alpha, ImportOptions, Model, UNITS_PER_METER};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
}

fn in_place(m: &Model) -> Model {
    m.to_source(&ImportOptions { center: false, ..ImportOptions::default() })
}

fn round_trip(m: &Model, static_prop: bool) -> (hp_mdl::Model, hp_meshio::studio::StudioFiles) {
    let names: Vec<String> =
        m.materials.iter().enumerate().map(|(i, x)| hp_meshio::smd::material_file_name(&x.name, i)).collect();
    let opts = StudioOptions {
        model: "hammerpro/fixture".to_owned(),
        cdmaterials: "models/hammerpro/fixture".to_owned(),
        static_prop,
        surface_prop: "metal".to_owned(),
        mass: 15.0,
        collision: Collision::Pieces,
        lod: hp_meshio::lod::LodOptions::default(),
    };
    let files = compile(m, &names, &opts).expect("compile");
    let loaded = hp_mdl::load(&files.mdl, &files.vvd, &files.vtx, &hp_mdl::LoadOptions::default()).expect("load");
    hp_mdl::parse_phy(files.phy.as_ref().expect("collision")).expect("collision reads");
    (loaded, files)
}

fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-3
}

#[test]
fn obj_with_mtl_textures_elsewhere_and_smoothing() {
    let m = hp_meshio::read(&fixture("crate.obj")).expect("read");
    assert_eq!(m.triangles.len(), 28);
    let wood = m.materials.iter().find(|x| x.name == "wood planks").expect("wood");
    assert_eq!(wood.texture.as_deref(), Some(fixture("textures").join("crate_wood.png").as_path()));
    let metal = m.materials.iter().find(|x| x.name == "metal").expect("metal");
    assert_eq!(metal.color, [0.2, 0.3, 0.4, 1.0]);
    let s = in_place(&m);
    for t in &s.triangles[..12] {
        let face = (t.pos[1] - t.pos[0]).cross(t.pos[2] - t.pos[0]).normalize();
        assert!(t.normal.iter().all(|n| close(*n, face)), "flat cube face");
    }
    let ring = &s.triangles[12..];
    assert!(ring.iter().any(|t| !close(t.normal[0], t.normal[1])), "smooth ring");
    let (lo, hi) = s.bounds().unwrap();
    assert!((hi.z - lo.z - UNITS_PER_METER).abs() < 1e-6);
    let (loaded, files) = round_trip(&s, true);
    assert!(loaded.is_static_prop);
    assert_eq!(loaded.materials, ["wood_planks", "metal"]);
    assert_eq!(loaded.triangle_count(), 28);
    assert_eq!(files.convex_pieces, 2, "the cube and the ring");
}

#[test]
fn binary_fbx_with_embedded_texture_and_byte_arrays() {
    let m = hp_meshio::read(&fixture("sign_binary.fbx")).expect("read");
    assert_eq!(m.triangles.len(), 2);
    let mat = &m.materials[m.triangles[0].material];
    assert_eq!(mat.name, "poster");
    let (png, ext) = mat.embedded.as_ref().expect("the texture embedded in the file");
    assert!(png.starts_with(b"\x89PNG") && ext == "png");
    assert!((m.meters_per_unit - 0.01).abs() < 1e-12, "centimeters");
    let s = in_place(&m);
    let (lo, hi) = s.bounds().unwrap();
    assert!(close(hi - lo, DVec3::new(UNITS_PER_METER, UNITS_PER_METER, 0.0)), "{lo} {hi}");
    assert!(s.triangles[0].uv.contains(&DVec2::new(0.0, 1.0)));
    let (loaded, files) = round_trip(&s, false);
    assert!(!loaded.is_static_prop);
    assert_eq!(files.convex_pieces, 1, "a flat sign still collides");
}

#[test]
fn ascii_fbx_z_up_with_pivots_and_templates() {
    let m = hp_meshio::read(&fixture("table_max.fbx")).expect("read");
    assert_eq!(m.triangles.len(), 24);
    let names: Vec<&str> = m.materials.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["oak top", "steel"]);
    assert_eq!(m.materials[0].color, [0.6, 0.4, 0.2, 1.0]);
    assert_eq!(m.materials[1].color, [0.5, 0.5, 0.5, 1.0], "the template's color");
    let s = in_place(&m);
    let (lo, hi) = s.bounds().unwrap();
    assert!(close(lo, DVec3::new(-24.0, -12.0, 0.0)) && close(hi, DVec3::new(41.0, 12.0, 30.0)), "{lo} {hi}");
    let leg: Vec<DVec3> = s.triangles.iter().filter(|t| t.material == 1).flat_map(|t| t.pos).collect();
    let (llo, lhi) = leg.iter().fold((DVec3::splat(1e9), DVec3::splat(-1e9)), |(a, b), p| (a.min(*p), b.max(*p)));
    assert!(close((llo + lhi) * 0.5, DVec3::new(40.0, -10.0, 14.0)), "{llo} {lhi}");
    for t in &s.triangles {
        let face = (t.pos[1] - t.pos[0]).cross(t.pos[2] - t.pos[0]);
        assert!(face.dot(t.normal[0]) > 0.0);
    }
    let (loaded, files) = round_trip(&s, true);
    assert_eq!(loaded.triangle_count(), 24);
    assert_eq!(files.convex_pieces, 2, "the top and the leg");
}

#[test]
fn glb_with_texture_transform_alpha_and_parents() {
    let m = hp_meshio::read(&fixture("leaf.glb")).expect("read");
    assert_eq!(m.triangles.len(), 1);
    let mat = &m.materials[0];
    assert_eq!(mat.name, "leaf");
    assert_eq!(mat.alpha, Alpha::Mask);
    assert!(mat.double_sided);
    assert_eq!(mat.color, [0.5, 1.0, 1.0, 1.0]);
    assert!(mat.embedded.as_ref().is_some_and(|(b, e)| b.starts_with(b"\x89PNG") && e == "png"));
    let t = &m.triangles[0];
    assert!(close(t.pos[0], DVec3::new(2.0, 0.0, 0.0)) && close(t.pos[1], DVec3::new(4.0, 0.0, 0.0)));
    assert!(mat.extras.has_uv_transform());
    let mut baked = m.clone();
    baked.bake_uv_transforms();
    assert!((baked.triangles[0].uv[1] - DVec2::new(2.5, 0.0)).length() < 1e-9, "{}", baked.triangles[0].uv[1]);
    let s = in_place(&m);
    let (loaded, _) = round_trip(&s, true);
    assert_eq!(loaded.triangle_count(), 1);
}
