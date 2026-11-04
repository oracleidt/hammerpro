use glam::{DVec2, DVec3};

use super::*;
use crate::{Material, Triangle, UpAxis};

fn cuboid(lo: DVec3, hi: DVec3, material: usize) -> Vec<Triangle> {
    let c = |i: usize| {
        DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    let quads: [([usize; 4], DVec3); 6] = [
        ([0, 2, 6, 4], -DVec3::X),
        ([1, 5, 7, 3], DVec3::X),
        ([0, 4, 5, 1], -DVec3::Y),
        ([2, 3, 7, 6], DVec3::Y),
        ([0, 1, 3, 2], -DVec3::Z),
        ([4, 6, 7, 5], DVec3::Z),
    ];
    let uv = [DVec2::new(0.0, 1.0), DVec2::new(1.0, 1.0), DVec2::new(1.0, 0.0), DVec2::new(0.0, 0.0)];
    let mut out = Vec::new();
    for (q, n) in quads {
        for k in [[0, 2, 1], [0, 3, 2]] {
            out.push(Triangle { pos: k.map(|i| c(q[i])), normal: [n; 3], uv: k.map(|i| uv[i]), material });
        }
    }
    out
}

fn table() -> Model {
    let mut triangles = cuboid(DVec3::new(-24.0, -12.0, 28.0), DVec3::new(24.0, 12.0, 30.0), 0);
    for (x, y) in [(-22.0, -10.0), (20.0, -10.0), (-22.0, 8.0), (20.0, 8.0)] {
        triangles.extend(cuboid(DVec3::new(x, y, 0.0), DVec3::new(x + 2.0, y + 2.0, 28.0), 1));
    }
    Model {
        triangles,
        materials: vec![
            Material { name: "top".to_owned(), ..Material::default() },
            Material { name: "legs".to_owned(), ..Material::default() },
        ],
        up: UpAxis::Z,
        meters_per_unit: 1.0 / crate::UNITS_PER_METER,
        objects: Vec::new(),
    }
}

fn options(static_prop: bool, collision: Collision) -> StudioOptions {
    StudioOptions {
        model: "hammerpro/table".to_owned(),
        cdmaterials: "models/hammerpro/table".to_owned(),
        static_prop,
        surface_prop: "wood".to_owned(),
        mass: 25.0,
        collision,
        lod: crate::lod::LodOptions::default(),
    }
}

fn load(f: &StudioFiles) -> hp_mdl::Model {
    hp_mdl::load(&f.mdl, &f.vvd, &f.vtx, &hp_mdl::LoadOptions::default()).expect("hp-mdl loads the model")
}

#[test]
fn a_static_prop_reads_back() {
    let names = vec!["top".to_owned(), "legs".to_owned()];
    let f = compile(&table(), &names, &options(true, Collision::Pieces)).expect("compile");
    let m = load(&f);
    assert!(m.is_static_prop);
    assert_eq!(m.name, "hammerpro/table.mdl");
    assert_eq!(m.materials, names);
    assert_eq!(m.material_dirs, [r"models\hammerpro\table\"]);
    assert_eq!(m.surface_prop, "wood");
    assert!((m.mass - 25.0).abs() < 1e-4);
    assert_eq!(m.meshes.len(), 2, "one mesh per material");
    assert_eq!(m.triangle_count(), 60);
    assert_eq!(m.vertex_count(), 5 * 24);
    let (lo, hi) = m.bounds;
    assert_eq!(lo, [-24.0, -12.0, 0.0]);
    assert_eq!(hi, [24.0, 12.0, 30.0]);
    for mesh in &m.meshes {
        for t in mesh.indices.chunks_exact(3) {
            let p = |i: u32| DVec3::from_array(mesh.vertices[i as usize].pos.map(f64::from));
            let n = DVec3::from_array(mesh.vertices[t[0] as usize].normal.map(f64::from));
            let face = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
            assert!(face.dot(n) > 0.0, "winding");
        }
    }
    assert_eq!(m.sequences.len(), 1);
    assert_eq!(m.bone_names, ["static_prop"]);
    let checksum = m.checksum;
    assert_eq!(i32::from_le_bytes(f.vvd[8..12].try_into().unwrap()), checksum);
    assert_eq!(i32::from_le_bytes(f.vtx[16..20].try_into().unwrap()), checksum);
    let phy = f.phy.as_ref().expect("collision");
    assert_eq!(i32::from_le_bytes(phy[12..16].try_into().unwrap()), checksum);
    assert_eq!(f.mdl.len(), i32::from_le_bytes(f.mdl[76..80].try_into().unwrap()) as usize);
}

#[test]
fn collision_pieces_box_and_none() {
    let names = vec!["top".to_owned(), "legs".to_owned()];
    let f = compile(&table(), &names, &options(false, Collision::Pieces)).expect("compile");
    assert_eq!(f.convex_pieces, 5, "the top and four legs");
    let phy = f.phy.as_ref().expect("collision");
    let c = hp_mdl::parse_phy(phy).expect("hp-mdl reads the collision");
    assert_eq!(c.solids.len(), 1);
    let s = &c.solids[0];
    for v in &s.vertices {
        let p = DVec3::from_array(v.map(f64::from));
        assert!(
            p.cmpge(DVec3::new(-24.01, -12.01, -0.01)).all() && p.cmple(DVec3::new(24.01, 12.01, 30.01)).all(),
            "{p}"
        );
    }
    let text = String::from_utf8_lossy(phy);
    assert!(text.contains("\"mass\" \"25.000000\"") && text.contains("\"surfaceprop\" \"wood\""), "{text}");
    assert!(text.contains("\"concave\" \"1\""));
    assert!(phy.ends_with(b"}\n\0"));
    let m = load(&f);
    assert!(!m.is_static_prop);
    assert_eq!(m.bone_names, ["root"]);
    let boxed = compile(&table(), &names, &options(true, Collision::Box)).expect("compile");
    assert_eq!(boxed.convex_pieces, 1);
    let c = hp_mdl::parse_phy(boxed.phy.as_ref().unwrap()).unwrap();
    assert_eq!(c.solids[0].triangles.len(), 12);
    let xs: Vec<f32> = c.solids[0].vertices.iter().map(|v| v[0]).collect();
    let (xmin, xmax) = xs.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
    assert!((xmin + 23.75).abs() < 0.01 && (xmax - 23.75).abs() < 0.01, "{xmin} {xmax}");
    let m = load(&boxed);
    assert!((m.hull_min[2] + 0.0).abs() < 0.01 && (m.hull_max[2] - 30.0).abs() < 0.01, "{:?}", m.hull_max);
    let none = compile(&table(), &names, &options(true, Collision::None)).expect("compile");
    assert!(none.phy.is_none());
}

#[test]
fn flat_pieces_get_a_collision() {
    let mut t = cuboid(DVec3::new(0.0, 0.0, 0.0), DVec3::new(32.0, 32.0, 1.0), 0);
    t.retain(|t| t.normal[0] == DVec3::Z);
    for t in &mut t {
        for p in &mut t.pos {
            p.z = 0.0;
        }
    }
    let m = Model {
        triangles: t,
        materials: vec![Material { name: "sign".to_owned(), ..Material::default() }],
        up: UpAxis::Z,
        meters_per_unit: 1.0,
        objects: Vec::new(),
    };
    let f = compile(&m, &["sign".to_owned()], &options(true, Collision::Pieces)).expect("compile");
    assert_eq!(f.convex_pieces, 1);
    let c = hp_mdl::parse_phy(f.phy.as_ref().unwrap()).unwrap();
    let zs: Vec<f32> = c.solids[0].vertices.iter().map(|v| v[2]).collect();
    let (zmin, zmax) = zs.iter().fold((f32::MAX, f32::MIN), |(a, b), &z| (a.min(z), b.max(z)));
    assert!((zmax - zmin - 1.0).abs() < 0.01, "{zmin} {zmax}");
}

#[test]
fn many_pieces_are_grouped() {
    let mut triangles = Vec::new();
    for i in 0..200 {
        let x = f64::from(i) * 4.0;
        triangles.extend(cuboid(DVec3::new(x, 0.0, 0.0), DVec3::new(x + 2.0, 2.0, 2.0), 0));
    }
    let m = Model {
        triangles,
        materials: vec![Material { name: "a".to_owned(), ..Material::default() }],
        up: UpAxis::Z,
        meters_per_unit: 1.0,
        objects: Vec::new(),
    };
    let f = compile(&m, &["a".to_owned()], &options(true, Collision::Pieces)).expect("compile");
    assert!(f.convex_pieces <= 40 && f.convex_pieces >= 20, "{}", f.convex_pieces);
    hp_mdl::parse_phy(f.phy.as_ref().unwrap()).unwrap();
}

#[test]
fn large_models_split_into_body_parts() {
    let n = 200;
    let mut triangles = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let p = |a: usize, b: usize| DVec3::new(a as f64, b as f64, ((a * 7 + b * 3) % 5) as f64 * 0.1);
            let uv = |a: usize, b: usize| DVec2::new(a as f64 * 0.37, b as f64 * 0.21);
            let q = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            for k in [[0, 1, 2], [0, 2, 3]] {
                let o = (j * n + i) as f64 * 1e-3;
                triangles.push(Triangle {
                    pos: k.map(|x| p(q[x].0, q[x].1)),
                    normal: [DVec3::Z; 3],
                    uv: k.map(|x| uv(q[x].0, q[x].1) + DVec2::splat(o + x as f64)),
                    material: 0,
                });
            }
        }
    }
    let m = Model {
        triangles,
        materials: vec![Material { name: "a".to_owned(), ..Material::default() }],
        up: UpAxis::Z,
        meters_per_unit: 1.0,
        objects: Vec::new(),
    };
    let f = compile(&m, &["a".to_owned()], &options(true, Collision::Box)).expect("compile");
    assert_eq!(f.body_parts, 3, "65536 vertices at most per body part");
    let loaded = load(&f);
    assert_eq!(loaded.triangle_count(), 80_000);
    assert_eq!(loaded.body_parts.len(), f.body_parts);

    let i32_at = |o: usize| i32::from_le_bytes(f.mdl[o..o + 4].try_into().unwrap());
    let (num_parts, parts_at) = (i32_at(232) as usize, i32_at(236) as usize);
    let mut ids = Vec::new();
    for p in 0..num_parts {
        let bp = parts_at + p * 16;
        let model = bp + i32_at(bp + 12) as usize;
        let (num_meshes, meshes_at) = (i32_at(model + 72) as usize, model + i32_at(model + 76) as usize);
        for k in 0..num_meshes {
            ids.push(i32_at(meshes_at + k * 116 + 32));
        }
    }
    assert_eq!(ids, (0..ids.len() as i32).collect::<Vec<_>>());
}

#[test]
fn empty_and_too_many_materials() {
    let m = Model::default();
    assert_eq!(compile(&m, &[], &options(true, Collision::Box)).unwrap_err(), StudioError::Empty);
    let mut m = table();
    m.materials = (0..40).map(|i| Material { name: format!("m{i}"), ..Material::default() }).collect();
    for (i, t) in m.triangles.iter_mut().enumerate() {
        t.material = i % 40;
    }
    let names: Vec<String> = (0..40).map(|i| format!("m{i}")).collect();
    assert_eq!(compile(&m, &names, &options(true, Collision::Box)).unwrap_err(), StudioError::TooManyMaterials(40));
}

fn sphere(subdivisions: usize) -> Model {
    let (x, y, z) = (DVec3::X, DVec3::Y, DVec3::Z);
    let mut tris: Vec<[DVec3; 3]> = Vec::new();
    for (a, b) in [(x, y), (y, -x), (-x, -y), (-y, x)] {
        tris.push([a, b, z]);
        tris.push([b, a, -z]);
    }
    for _ in 0..subdivisions {
        let mut next = Vec::new();
        for [a, b, c] in tris {
            let (ab, bc, ca) = ((a + b).normalize(), (b + c).normalize(), (c + a).normalize());
            next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        tris = next;
    }
    let triangles = tris
        .into_iter()
        .map(|t| Triangle {
            pos: t.map(|p| p * 48.0),
            normal: t,
            uv: t.map(|p| DVec2::new(p.x * 0.5 + 0.5, p.y * 0.5 + 0.5)),
            material: 0,
        })
        .collect();
    Model {
        triangles,
        materials: vec![Material { name: "ball".to_owned(), ..Material::default() }],
        up: UpAxis::Z,
        meters_per_unit: 1.0 / crate::UNITS_PER_METER,
        objects: Vec::new(),
    }
}

#[test]
fn levels_of_detail_read_back() {
    let names = vec!["ball".to_owned()];
    let mut opts = options(true, Collision::Pieces);
    opts.model = "hammerpro/ball".to_owned();
    let f = compile(&sphere(4), &names, &opts).expect("compile");
    assert_eq!(f.lods, 5, "the full model and 4 levels");
    assert_eq!(f.lod_triangles.len(), 5);
    assert_eq!(f.lod_triangles[0], 2048);
    assert!(f.lod_triangles.windows(2).all(|w| w[1] < w[0]), "{:?}", f.lod_triangles);
    assert!(f.lod_triangles[1] <= 1100 && f.lod_triangles[4] <= 200, "{:?}", f.lod_triangles);
    let full = load(&f);
    assert_eq!(full.num_lods, 5);
    assert_eq!(full.lod_switch_points.len(), 5);
    assert_eq!(full.lod_switch_points[0], 0.0);
    assert!(full.lod_switch_points.windows(2).all(|w| w[1] > w[0]), "{:?}", full.lod_switch_points);
    let mut vertices = None;
    for (lod, &tris) in f.lod_triangles.iter().enumerate() {
        let lo = hp_mdl::LoadOptions { lod: lod as u32, ..hp_mdl::LoadOptions::default() };
        let m = hp_mdl::load(&f.mdl, &f.vvd, &f.vtx, &lo).expect("loads");
        assert_eq!(m.lod, lod as u32);
        assert_eq!(m.triangle_count(), tris, "LOD {lod}");
        let r = m.meshes.iter().flat_map(|x| x.vertices.iter()).map(|v| DVec3::from(v.pos.map(f64::from)).length());
        assert!(r.into_iter().all(|r| (r - 48.0).abs() < 0.01));
        vertices.get_or_insert(m.meshes[0].vertices.len());
    }
    assert!(!f.mdl.is_empty() && i32::from_le_bytes(f.mdl[152..156].try_into().unwrap()) & 0x40 == 0);
    let mut off = opts.clone();
    off.lod.enabled = false;
    let f = compile(&sphere(2), &names, &off).expect("compile");
    assert_eq!((f.lods, load(&f).num_lods), (1, 1));
}

#[test]
fn physics_props_get_a_shadow_lod() {
    let names = vec!["ball".to_owned()];
    let f = compile(&sphere(3), &names, &options(false, Collision::Pieces)).expect("compile");
    let flags = i32::from_le_bytes(f.mdl[152..156].try_into().unwrap());
    assert_eq!(flags & 0x40, 0x40, "STUDIOHDR_FLAGS_HASSHADOWLOD");
    let m = load(&f);
    assert_eq!(m.num_lods, f.lods as u32, "the shadow LOD is not drawn");
    let vtx_lods = i32::from_le_bytes(f.vtx[20..24].try_into().unwrap());
    assert_eq!(vtx_lods as usize, f.lods + 1);
    let vvd_lods = i32::from_le_bytes(f.vvd[12..16].try_into().unwrap());
    assert_eq!(vvd_lods as usize, f.lods + 1);
}
