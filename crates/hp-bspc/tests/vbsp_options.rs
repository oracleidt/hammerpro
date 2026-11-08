mod common;

use common::lumps::Lumps;
use common::{GameMats, Vmf, block, disp_block, room};
use glam::DVec3;
use hp_bspc::materials::MaterialSource;
use hp_bspc::{CompileError, CompileOptions, CompileOutput, VisLevel};

const WALL: &str = "dev/dev_measuregeneric01b";
const WATER: &str = "dev/dev_water2";

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn run_with(
    m: &Vmf,
    mats: &dyn MaterialSource,
    opts: &CompileOptions,
) -> (Result<CompileOutput, CompileError>, Vec<String>) {
    let map = hp_vmf::parse(&m.text()).expect("the VMF parses");
    let mut log = Vec::new();
    let out = hp_bspc::compile(&map, mats, &hp_bspc::props::NoModels, opts, &mut |l| log.push(l.to_owned()));
    (out, log)
}

fn run(m: &Vmf, opts: &CompileOptions) -> (Result<CompileOutput, CompileError>, Vec<String>) {
    run_with(m, &GameMats, opts)
}

fn compiled(m: &Vmf, opts: &CompileOptions) -> (Lumps, Vec<String>) {
    let (out, log) = run(m, opts);
    let out = out.unwrap_or_else(|e| panic!("compile failed: {e}\n{}", log.join("\n")));
    (Lumps::from_bytes(&out.bsp), log)
}

fn failure(m: &Vmf, opts: &CompileOptions) -> String {
    match run(m, opts).0 {
        Ok(_) => panic!("the compile should fail"),
        Err(e) => e.to_string(),
    }
}

fn room_map(lo: DVec3, hi: DVec3) -> Vmf {
    let mut m = Vmf::new();
    m.world = room(lo, hi, WALL);
    m.point("info_player_start", v(lo.x + 32.0, lo.y + 32.0, lo.z + 8.0), &[("angles", "0 0 0")]);
    m
}

fn small_room() -> Vmf {
    room_map(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 192.0))
}

fn rooms_in_a_row(count: usize) -> Vmf {
    let len = 512.0;
    let mut m = room_map(v(0.0, 0.0, 0.0), v(len * count as f64, 256.0, 128.0));
    for k in 1..count {
        let x = len * k as f64;
        m.world.push(block(v(x - 8.0, 0.0, 0.0), v(x + 8.0, 96.0, 128.0), WALL));
        m.world.push(block(v(x - 8.0, 160.0, 0.0), v(x + 8.0, 256.0, 128.0), WALL));
        m.world.push(block(v(x - 8.0, 96.0, 96.0), v(x + 8.0, 160.0, 128.0), WALL));
    }
    m
}

fn named_lights(m: &mut Vmf, count: usize) {
    for k in 0..count {
        let name = format!("sw{k}");
        m.point(
            "light",
            v(-200.0 + (k % 20) as f64 * 20.0, -200.0 + (k / 20) as f64 * 20.0, 100.0),
            &[("targetname", &name), ("_light", "255 255 255 200")],
        );
    }
}

fn styles(lumps: &Lumps) -> Vec<i32> {
    lumps
        .entities()
        .iter()
        .filter(|e| e.iter().any(|(k, val)| k == "classname" && val == "light"))
        .filter_map(|e| e.iter().find(|(k, _)| k == "style").and_then(|(_, s)| s.parse().ok()))
        .collect()
}

#[test]
fn ninety_six_switched_lights_get_styles_32_to_127() {
    let mut m = small_room();
    named_lights(&mut m, 96);
    let (lumps, _) = compiled(&m, &CompileOptions::default());
    let mut s = styles(&lumps);
    s.sort_unstable();
    assert_eq!(s, (32..128).collect::<Vec<i32>>());
}

#[test]
fn a_ninety_seventh_switched_light_is_fatal() {
    let mut m = small_room();
    named_lights(&mut m, 97);
    assert_eq!(failure(&m, &CompileOptions::default()), "Too many switched lights (error at light sw96), max = 96");
}

#[test]
fn a_displacement_on_a_brush_entity_is_fatal() {
    let mut m = small_room();
    let mut patch = block(v(-64.0, -64.0, 0.0), v(64.0, 64.0, 16.0), WALL);
    patch.sides[0].disp = Some(disp_block(2, v(-64.0, -64.0, 16.0), 1.0));
    m.brush_entity("func_brush", &[("solidity", "0")], vec![patch]);
    assert_eq!(
        failure(&m, &CompileOptions::default()),
        "Error: displacement found on a(n) func_brush entity - not supported (entity 2, brush 0)"
    );
}

fn detail_and_water() -> Vmf {
    let mut m = small_room();
    m.brush_entity("func_detail", &[], vec![block(v(-64.0, -64.0, 0.0), v(0.0, 0.0, 64.0), WALL)]);
    m.world.push(block(v(64.0, 64.0, 0.0), v(192.0, 192.0, 48.0), WATER));
    m
}

const DETAIL: i32 = 0x0800_0000;
const MASK_WATER: i32 = 0x30;

#[test]
fn inline_world_detail_contents_matches_func_detail_with_all_detail_switches() {
    let original = hp_vmf::parse(&detail_and_water().text()).unwrap();
    let mut inline = original.clone();
    let entity = inline.entities.iter().position(|e| e.classname == "func_detail").unwrap();
    let mut detail = inline.entities.remove(entity);
    for solid in &mut detail.solids {
        for side in &mut solid.sides {
            side.add_compile_contents(hp_vmf::CONTENTS_DETAIL);
        }
    }
    inline.world.solids.extend(detail.solids);
    for options in [
        CompileOptions::default(),
        CompileOptions { no_detail: true, ..Default::default() },
        CompileOptions { full_detail: true, ..Default::default() },
    ] {
        let compile = |map: &hp_vmf::Map| {
            let out = hp_bspc::compile(map, &GameMats, &hp_bspc::props::NoModels, &options, &mut |_| {}).unwrap();
            Lumps::from_bytes(&out.bsp)
        };
        let reference = compile(&original);
        let result = compile(&inline);
        assert_eq!(result.nodes().len(), reference.nodes().len());
        assert_eq!(result.faces().len(), reference.faces().len());
        let mut expected: Vec<_> = reference.brushes().iter().map(|b| b.contents).collect();
        let mut actual: Vec<_> = result.brushes().iter().map(|b| b.contents).collect();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected);
    }
}

#[test]
fn inline_nodraw_flags_hide_faces_without_removing_collision() {
    let mut m = small_room();
    m.world.push(block(v(-64.0, -64.0, 16.0), v(64.0, 64.0, 64.0), WALL));
    let visible = hp_vmf::parse(&m.text()).unwrap();
    let mut hidden = visible.clone();
    for side in &mut hidden.world.solids.last_mut().unwrap().sides {
        side.extra_props.push(("flags".into(), "128".into()));
    }
    let compile = |map: &hp_vmf::Map| {
        let out = hp_bspc::compile(map, &GameMats, &hp_bspc::props::NoModels, &CompileOptions::default(), &mut |_| {})
            .unwrap();
        Lumps::from_bytes(&out.bsp)
    };
    let before = compile(&visible);
    let after = compile(&hidden);
    assert_eq!(after.brushes(), before.brushes());
    assert!(after.faces().len() < before.faces().len());
}

#[test]
fn nodetail_nowater_and_fulldetail() {
    let m = detail_and_water();
    let (all, _) = compiled(&m, &CompileOptions::default());
    let count = |l: &Lumps, mask: i32| l.brushes().iter().filter(|b| b.contents & mask != 0).count();
    assert_eq!(count(&all, DETAIL), 1);
    assert_eq!(count(&all, MASK_WATER), 1);
    let (no_detail, _) = compiled(&m, &CompileOptions { no_detail: true, ..Default::default() });
    assert_eq!(count(&no_detail, DETAIL), 0);
    assert_eq!(no_detail.brushes().len(), all.brushes().len() - 1);
    let (no_water, _) = compiled(&m, &CompileOptions { no_water: true, ..Default::default() });
    assert_eq!(count(&no_water, MASK_WATER), 0);
    assert_eq!(no_water.brushes().len(), all.brushes().len() - 1);
    let (full, _) = compiled(&m, &CompileOptions { full_detail: true, ..Default::default() });
    assert_eq!(count(&full, DETAIL), 0);
    assert_eq!(full.brushes().len(), all.brushes().len());
}

#[test]
fn microbrushes_are_reported() {
    let mut m = small_room();
    m.world.push(block(v(0.0, 0.0, 0.0), v(0.5, 0.5, 0.5), WALL));
    let (_, log) = compiled(&m, &CompileOptions::default());
    let micro: Vec<&String> = log.iter().filter(|l| l.ends_with(": WARNING, microbrush")).collect();
    assert_eq!(micro.len(), 1, "{log:?}");
    assert!(micro[0].starts_with("Brush "));
    let (_, log) = compiled(&m, &CompileOptions { micro_volume: 0.1, ..Default::default() });
    assert!(!log.iter().any(|l| l.contains("microbrush")), "{log:?}");
}

#[test]
fn duplicate_planes_are_reported() {
    let mut m = small_room();
    let mut b = block(v(0.0, 0.0, 0.0), v(32.0, 32.0, 32.0), WALL);
    let first = b.sides[0].clone();
    b.sides.push(first);
    m.world.push(b);
    let (_, log) = compiled(&m, &CompileOptions::default());
    assert!(log.iter().any(|l| l.ends_with(", Side 6: duplicate plane")), "{log:?}");
}

fn luxel_density(lumps: &Lumps) -> f32 {
    let t = &lumps.texinfos()[0];
    let l = t.lightmap[0];
    (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt()
}

#[test]
fn luxel_options_scale_the_lightmaps() {
    let m = small_room();
    let (base, _) = compiled(&m, &CompileOptions::default());
    assert!((luxel_density(&base) - 1.0 / 16.0).abs() < 1e-6);
    let (scaled, _) = compiled(&m, &CompileOptions { luxel_scale: 2.0, ..Default::default() });
    assert!((luxel_density(&scaled) - 1.0 / 32.0).abs() < 1e-6);
    let (min, _) = compiled(&m, &CompileOptions { luxel_scale: 0.01, min_luxel_scale: 4.0, ..Default::default() });
    assert!((luxel_density(&min) - 1.0 / 4.0).abs() < 1e-6);
    let mut zero = small_room();
    for s in zero.world.iter_mut().flat_map(|b| b.sides.iter_mut()) {
        s.lightmap_scale = 0;
    }
    let (z, _) = compiled(&zero, &CompileOptions { default_luxel_size: 8.0, ..Default::default() });
    assert!((luxel_density(&z) - 1.0 / 8.0).abs() < 1e-6);
}

#[test]
fn face_options_change_the_faces() {
    let m = room_map(v(16.0, 16.0, 0.0), v(1008.0, 1008.0, 256.0));
    let faces = |o: CompileOptions| compiled(&m, &o).0.faces().len();
    let base = faces(CompileOptions::default());
    let wide = faces(CompileOptions { max_lightmap_dim: 125.0, ..Default::default() });
    assert!(wide < base, "{wide} {base}");
    let whole = faces(CompileOptions { max_lightmap_dim: 125.0, no_subdiv: true, ..Default::default() });
    assert!(whole <= wide, "{whole} {wide}");
    for o in [
        CompileOptions { no_merge: true, ..Default::default() },
        CompileOptions { no_tjunc: true, ..Default::default() },
        CompileOptions { no_prune: true, ..Default::default() },
    ] {
        assert!(faces(o) > 0);
    }
    let (out, _) = run(&m, &CompileOptions { no_subdiv: true, ..Default::default() });
    assert!(out.is_ok(), "{:?}", out.err().map(|e| e.to_string()));
    let (out, _) = run(&m, &CompileOptions { no_subdiv: true, luxel_scale: 0.25, ..Default::default() });
    let err = out.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        err.starts_with(
            "Bad surface extents - surface is too big to have a lightmap
	material dev/dev_measuregeneric01b"
        ),
        "{err}"
    );
}

#[test]
fn novirtualmesh_writes_polygon_soups() {
    let mut m = small_room();
    let mut patch = block(v(-192.0, -192.0, 0.0), v(64.0, 64.0, 16.0), WALL);
    patch.sides[0].disp = Some(disp_block(3, v(-192.0, -192.0, 16.0), 3.5));
    m.world.push(patch);
    let (virt, _) = compiled(&m, &CompileOptions::default());
    let (soup, _) = compiled(&m, &CompileOptions { no_virtual_mesh: true, ..Default::default() });
    assert!(!virt.raw(hp_bspc::emit::lump::PHYSDISP).is_empty());
    assert!(soup.raw(hp_bspc::emit::lump::PHYSDISP).is_empty());
    assert_ne!(virt.raw(hp_bspc::emit::lump::PHYSCOLLIDE), soup.raw(hp_bspc::emit::lump::PHYSCOLLIDE));
}

#[test]
fn too_many_overlays_is_fatal() {
    let mut m = small_room();
    for k in 0..8193 {
        m.point("info_overlay", v(0.0, 0.0, 1.0 + k as f64 * 0.01), &[("material", WALL), ("sides", "")]);
    }
    assert_eq!(failure(&m, &CompileOptions::default()), "Too Many Overlays!\nMAX_MAP_OVERLAYS = 8192");
}

#[test]
fn an_overlay_on_too_many_faces_is_fatal() {
    let mut m = Vmf::new();
    m.world.push(block(v(-2048.0, -2048.0, -16.0), v(2048.0, 2048.0, 0.0), WALL));
    m.world.extend(room(v(-2048.0, -2048.0, 0.0), v(2048.0, 2048.0, 256.0), WALL));
    m.point("info_player_start", v(0.0, 0.0, 8.0), &[]);
    m.point(
        "info_overlay",
        v(0.0, 0.0, 0.0),
        &[("material", WALL), ("sides", "3"), ("BasisNormal", "0 0 1"), ("BasisU", "1 0 0"), ("BasisV", "0 1 0")],
    );
    let err = failure(&m, &CompileOptions::default());
    assert!(err.starts_with("Overlay touching too many faces (touching "), "{err}");
    assert!(err.ends_with(", max 64)\nOverlay dev/dev_measuregeneric01b at 0.0 0.0 0.0"), "{err}");
}

#[test]
fn too_many_brush_models_is_fatal() {
    let mut m = room_map(v(-2048.0, -2048.0, 0.0), v(2048.0, 2048.0, 128.0));
    for k in 0..4096 {
        let (x, y) = ((k % 64) as f64 * 60.0 - 1960.0, (k / 64) as f64 * 60.0 - 1960.0);
        m.brush_entity("func_wall", &[], vec![block(v(x, y, 0.0), v(x + 16.0, y + 16.0, 16.0), WALL)]);
    }
    assert_eq!(failure(&m, &CompileOptions::default()), "Too many brush models in map, max = 4096");
}

fn decode_pvs(raw: &[u8]) -> Vec<Vec<u8>> {
    let i32_at = |o: usize| i32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
    let n = i32_at(0).max(0) as usize;
    let bytes = n.div_ceil(8);
    (0..n)
        .map(|c| {
            let mut at = i32_at(4 + c * 8) as usize;
            let mut row = Vec::with_capacity(bytes);
            while row.len() < bytes {
                if raw[at] == 0 {
                    row.extend(std::iter::repeat_n(0, usize::from(raw[at + 1])));
                    at += 2;
                } else {
                    row.push(raw[at]);
                    at += 1;
                }
            }
            row.truncate(bytes);
            row
        })
        .collect()
}

fn radial_leaves(lumps: &Lumps) -> usize {
    lumps
        .raw(hp_bspc::emit::lump::LEAFS)
        .chunks_exact(32)
        .filter(|l| (u16::from_le_bytes([l[6], l[7]]) >> 9) & hp_bspc::emit::LEAF_FLAGS_RADIAL != 0)
        .count()
}

#[test]
fn radius_override_culls_far_clusters_and_marks_the_leaves() {
    let m = rooms_in_a_row(6);
    let vis = CompileOptions { vis: VisLevel::Full, ..Default::default() };
    let (full, _) = compiled(&m, &vis);
    let (near, log) = compiled(&m, &CompileOptions { radius_override: Some(512.0), ..vis.clone() });
    assert!(log.iter().any(|l| l.starts_with("Vis Radius = 512.00")), "{log:?}");
    assert_eq!(radial_leaves(&full), 0);
    assert!(radial_leaves(&near) > 0);
    let seen = |l: &Lumps| -> usize {
        let pvs = decode_pvs(l.raw(hp_bspc::emit::lump::VISIBILITY));
        pvs.iter().map(|r| r.iter().map(|b| b.count_ones() as usize).sum::<usize>()).sum()
    };
    assert!(seen(&near) < seen(&full), "{} {}", seen(&near), seen(&full));
    let (unsorted, _) = compiled(&m, &CompileOptions { vis_sort: false, threads: Some(1), ..vis.clone() });
    assert_eq!(unsorted.raw(hp_bspc::emit::lump::VISIBILITY), full.raw(hp_bspc::emit::lump::VISIBILITY));
}

#[test]
fn trace_gives_vvis_line_file() {
    let m = rooms_in_a_row(3);
    let (out, log) = run(&m, &CompileOptions { vis: VisLevel::Full, vis_trace: Some((0, 1)), ..Default::default() });
    let out = out.expect("compiles");
    let trace = out.trace.unwrap_or_else(|| panic!("no trace: {log:?}"));
    assert!(trace.lines().count() >= 2);
    assert!(trace.ends_with("\r\n"));
}

#[test]
fn the_lighting_takes_styles_up_to_127() {
    let mut m = small_room();
    named_lights(&mut m, 96);
    let (out, log) = run(&m, &CompileOptions::default());
    let bsp = out.unwrap_or_else(|e| panic!("{e}: {log:?}")).bsp;
    let opts = hp_bspc::rad::RadOptions {
        ldr: true,
        hdr: false,
        bounces: 0,
        quality: hp_bspc::rad::RadQuality::Draft,
        ..Default::default()
    };
    let lit = hp_bspc::rad::light_bsp_with(&bsp, &opts, &hp_bspc::props::NoModels, &mut |_| {}).expect("lit");
    let lumps = Lumps::from_bytes(&lit);
    let mut lights: Vec<i32> =
        lumps.raw(15).chunks_exact(88).map(|r| i32::from_le_bytes([r[44], r[45], r[46], r[47]])).collect();
    lights.sort_unstable();
    assert_eq!(lights, (32..128).collect::<Vec<i32>>());
}

#[test]
fn hidden_objects_are_not_compiled() {
    let mut m = small_room();
    m.world.push(block(v(0.0, 0.0, 0.0), v(32.0, 32.0, 32.0), WALL));
    m.brush_entity("func_brush", &[], vec![block(v(64.0, 0.0, 0.0), v(96.0, 32.0, 32.0), WALL)]);
    let mut map = hp_vmf::parse(&m.text()).expect("parses");
    let brushes = |map: &hp_vmf::Map| {
        let out = hp_bspc::compile(map, &GameMats, &hp_bspc::props::NoModels, &CompileOptions::default(), &mut |_| {})
            .expect("compiles");
        let l = Lumps::from_bytes(&out.bsp);
        (l.brushes().len(), l.models().len())
    };
    let (all, models) = brushes(&map);
    assert_eq!(models, 2);
    map.world.solids.last_mut().expect("solid").hidden = true;
    let (fewer, _) = brushes(&map);
    assert_eq!(fewer, all - 1);
    map.entities.iter_mut().find(|e| e.classname == "func_brush").expect("entity").hidden = true;
    let (fewest, models) = brushes(&map);
    assert_eq!((fewest, models), (all - 2, 1));
}

fn face_area(l: &Lumps) -> f64 {
    l.faces()
        .iter()
        .map(|f| {
            let p: Vec<DVec3> =
                l.face_points(f).iter().map(|q| v(f64::from(q[0]), f64::from(q[1]), f64::from(q[2]))).collect();
            (2..p.len()).map(|i| (p[i - 1] - p[0]).cross(p[i] - p[0]).length() * 0.5).sum::<f64>()
        })
        .sum()
}

#[test]
fn blocksize_changes_the_cuts_not_the_surfaces() {
    let mut m = room_map(v(-1500.0, -700.0, 0.0), v(1300.0, 900.0, 256.0));
    for x in [-1024.0, 0.0, 1024.0] {
        m.world.push(block(v(x - 40.0, -40.0, 0.0), v(x + 40.0, 40.0, 96.0), WALL));
    }
    let (default, log) = compiled(&m, &CompileOptions::default());
    assert!(!log.iter().any(|l| l.contains("-blocksize")));
    let (whole, log) = compiled(&m, &CompileOptions { block_size: 0, ..Default::default() });
    assert!(log.iter().any(|l| l.contains("in one block (-blocksize 0)")), "{log:#?}");
    let (big, log) = compiled(&m, &CompileOptions { block_size: 4096, ..Default::default() });
    assert!(log.iter().any(|l| l.contains("blocks of 4096 units")), "{log:#?}");
    assert!(whole.leaves().len() < default.leaves().len(), "{} {}", whole.leaves().len(), default.leaves().len());
    assert!(whole.faces().len() < default.faces().len(), "{} {}", whole.faces().len(), default.faces().len());
    assert!(big.faces().len() < default.faces().len());
    let area = face_area(&default);
    for other in [&whole, &big] {
        let a = face_area(other);
        assert!((a - area).abs() < area * 1e-6, "{a} {area}");
    }
}

#[test]
fn blocksize_keeps_the_leaks() {
    for opts in [
        CompileOptions { block_size: 0, ..Default::default() },
        CompileOptions { block_size: 2048, ..Default::default() },
    ] {
        let (out, log) = run(&common::maps::generated_leaking(11), &opts);
        let out = out.unwrap_or_else(|e| panic!("compile failed: {e}\n{}", log.join("\n")));
        assert!(out.leak.is_some(), "block size {}", opts.block_size);
        let (out, _) = run(&common::maps::generated(7), &opts);
        assert!(out.expect("compiles").leak.is_none(), "block size {}", opts.block_size);
    }
}

#[test]
fn blocksize_values() {
    use hp_bspc::compile::block_size_of;
    assert_eq!(block_size_of(0.0), Some(0));
    assert_eq!(block_size_of(1024.0), Some(1024));
    assert_eq!(block_size_of(2048.7), Some(2048));
    assert_eq!(block_size_of(32768.0), Some(32768));
    for bad in [-1.0, 64.0, 127.0, 40000.0, f64::NAN, f64::INFINITY] {
        assert_eq!(block_size_of(bad), None, "{bad}");
    }
}

#[test]
fn propdebug_lists_the_props_with_problems() {
    let mut m = small_room();
    m.world.push(block(v(64.0, 64.0, 0.0), v(192.0, 192.0, 128.0), WALL));
    m.point("prop_static", v(0.0, 0.0, 16.0), &[("model", "models/props_c17/oildrum001.mdl")]);
    m.point("prop_static", v(128.0, 128.0, 64.0), &[("model", "models/props_c17/oildrum001.mdl")]);
    m.point("prop_dynamic", v(0.0, 0.0, 16.0), &[("model", "")]);
    let (_, quiet) = compiled(&m, &CompileOptions::default());
    assert!(!quiet.iter().any(|l| l.contains("Prop debug")));
    let (_, log) = compiled(&m, &CompileOptions { prop_debug: true, ..Default::default() });
    let at = log.iter().position(|l| l.contains("---- Prop debug (-propdebug) ----")).expect("the report");
    let report = &log[at..];
    assert!(report.iter().any(|l| l.contains("touches no open leaf")), "{report:#?}");
    assert!(report.iter().any(|l| l.contains("prop_static") && l.contains("at (128 128 64)")), "{report:#?}");
    assert!(report.iter().any(|l| l.contains("it has no model")), "{report:#?}");
    assert!(report.iter().any(|l| l.starts_with("2 of the 3 props have problems")), "{report:#?}");
}
