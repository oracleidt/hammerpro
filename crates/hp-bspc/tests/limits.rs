mod common;

use common::lumps::Lumps;
use common::{Vmf, block, room};
use glam::DVec3;

const WALL: &str = "dev/dev_measuregeneric01b";

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn stretched_by_a_tjunction(depth: f64) -> Vmf {
    let far = 0.95 + depth - 0.01;
    let mut m = Vmf::new();
    m.world = room(v(-64.0, -64.0, -64.0), v(256.0, 256.0, 128.0), WALL);
    let mut slab = block(v(0.0, 0.95, -16.0), v(16.0, far, 0.0), WALL);
    slab.sides[0].lightmap_scale = 1;
    m.world.push(slab);
    m.world.push(block(v(8.0, far + 0.2, -16.0), v(24.0, far + 16.0, 0.0), WALL));
    m.point("info_player_start", v(128.0, 128.0, 8.0), &[("angles", "0 0 0")]);
    m
}

fn lightmap_size(l: &Lumps, f: &common::lumps::Face, lightmap: &[[f32; 4]; 2]) -> [i32; 2] {
    let pts: Vec<glam::Vec3> = l.face_points(f).into_iter().map(glam::Vec3::from).collect();
    hp_bspc::faces::lightmap_extents(pts, lightmap).1
}

fn compile_sizes(name: &str, m: &Vmf, opts: &hp_bspc::CompileOptions) -> (Lumps, Vec<String>, Vec<[i32; 2]>) {
    let text = m.text();
    if let Ok(dir) = std::env::var("HP_LIMITS_DUMP") {
        let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.vmf")), &text);
    }
    let map = hp_vmf::parse(&text).expect("the VMF parses");
    let mut log = Vec::new();
    let out =
        hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, opts, &mut |l| log.push(l.to_owned()))
            .unwrap_or_else(|e| panic!("{name} compiles: {e}"));
    let l = Lumps::from_bytes(&out.bsp);
    let infos = l.texinfos();
    let mut sizes = Vec::new();
    for f in l.faces() {
        let info = &infos[f.texinfo as usize];
        if info.flags as u32 & (hp_bspc::materials::surf::NOLIGHT | hp_bspc::materials::surf::SKY) != 0 {
            continue;
        }
        let size = lightmap_size(&l, &f, &info.lightmap);
        assert_eq!(f.lightmap_size, size, "{name}: the face records its lightmap size");
        sizes.push(size);
    }
    (l, log, sizes)
}

#[test]
fn a_face_stretched_past_33_luxels_compiles_like_vbsp() {
    let (_, log, sizes) =
        compile_sizes("limits_tjunction_32", &stretched_by_a_tjunction(32.0), &hp_bspc::CompileOptions::default());
    assert!(sizes.contains(&[16, 34]), "VBSP's 16 x 34 face: {sizes:?}");
    assert!(sizes.iter().all(|s| s[0] <= 126 && s[1] <= 126));
    assert!(
        !log.iter().any(|l| l.contains("Bad surface extents") || l.contains("too big for their lightmap")),
        "{log:#?}"
    );
}

#[test]
fn a_face_stretched_past_126_luxels_is_cut_again() {
    let opts = hp_bspc::CompileOptions { max_lightmap_dim: 125.0, ..Default::default() };
    let (_, log, sizes) = compile_sizes("limits_tjunction_125", &stretched_by_a_tjunction(125.0), &opts);
    assert!(log.iter().any(|l| l.contains("too big for their lightmap")), "the slab was cut again: {log:#?}");
    assert!(sizes.iter().all(|s| s[0] <= 126 && s[1] <= 126), "{sizes:?}");
}

#[test]
fn golden_maps_use_no_more_vertices_than_vbsp() {
    for (name, map) in common::maps::golden() {
        let ours = Lumps::from_bytes(&common::compile(&map.text()).bsp);
        let valve = Lumps::open(&common::fixtures().join(format!("{name}.bsp")));
        let count = |l: &Lumps| (l.vertices().len(), l.edges().len(), l.faces().len());
        let (o, v) = (count(&ours), count(&valve));
        assert!(o.0 <= v.0, "{name}: {} vertices, VBSP {}", o.0, v.0);
        assert!(o.1 <= v.1, "{name}: {} edges, VBSP {}", o.1, v.1);
        assert!(o.2 <= v.2, "{name}: {} faces, VBSP {}", o.2, v.2);
    }
}

#[test]
fn relaxed_limits_only_warn() {
    let mut m = Vmf::new();
    m.world = room(v(16.0, 16.0, 0.0), v(1008.0, 1008.0, 256.0), WALL);
    m.point("info_player_start", v(128.0, 128.0, 8.0), &[("angles", "0 0 0")]);
    let map = hp_vmf::parse(&m.text()).expect("the VMF parses");
    let opts = hp_bspc::CompileOptions { no_subdiv: true, luxel_scale: 0.25, ..Default::default() };
    let strict = hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &opts, &mut |_| {});
    assert!(strict.err().is_some_and(|e| e.to_string().starts_with("Bad surface extents")));
    let mut log = Vec::new();
    let relaxed = hp_bspc::compile(
        &map,
        &common::GameMats,
        &hp_bspc::props::NoModels,
        &hp_bspc::CompileOptions { relaxed_limits: true, ..opts },
        &mut |l| log.push(l.to_owned()),
    );
    assert!(relaxed.is_ok(), "{:?}", relaxed.err().map(|e| e.to_string()));
    assert!(
        log.iter().any(|l| l.starts_with("Warning: Bad surface extents") && l.contains(hp_bspc::compile::RELAXED_NOTE)),
        "{log:#?}"
    );
}

#[test]
fn lean_planes_leave_out_the_planes_of_displacement_brushes() {
    let map = hp_vmf::parse(&common::maps::displacement().text()).expect("the VMF parses");
    let compile = |lean: bool| {
        let opts = hp_bspc::CompileOptions { lean_planes: lean, ..Default::default() };
        let out = hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &opts, &mut |_| {})
            .unwrap_or_else(|e| panic!("compiles: {e}"));
        Lumps::from_bytes(&out.bsp)
    };
    let (full, lean) = (compile(false), compile(true));
    assert!(lean.planes().len() < full.planes().len(), "{} vs {}", lean.planes().len(), full.planes().len());
    let disp_planes = |l: &Lumps| {
        let planes = l.planes();
        let mut v: Vec<String> = l
            .faces()
            .iter()
            .filter(|f| f.dispinfo != -1)
            .map(|f| {
                let p = &planes[usize::from(f.plane)];
                format!("{:?} {}", p.normal, p.dist)
            })
            .collect();
        v.sort();
        v
    };
    assert!(!disp_planes(&full).is_empty());
    assert_eq!(disp_planes(&full), disp_planes(&lean), "the displacements keep their planes");
    assert_eq!(full.faces().len(), lean.faces().len());
}
