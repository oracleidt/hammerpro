use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{LoadOptions, load, mdl, sibling_paths};

const GMOD_MODELS: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\models";
const WORKSHOP: &str = r"D:\SteamLibrary\steamapps\workshop\content\4000";

#[path = "../tests/common/mod.rs"]
mod common;

fn collect_mdls(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mdls(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mdl")) {
            out.push(path);
        }
    }
}

fn read_set(path: &Path) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let s = path.to_str()?;
    let (vvd, vtx) = sibling_paths(s);
    let mdl = std::fs::read(path).ok()?;
    let vvd = std::fs::read(vvd).ok()?;
    let vtx = vtx.iter().find_map(|p| std::fs::read(p).ok())?;
    Some((mdl, vvd, vtx))
}

#[test]
fn static_bones_and_winding() {
    let root = Path::new(GMOD_MODELS);
    if !root.is_dir() {
        eprintln!("skipping: {GMOD_MODELS} not found");
        return;
    }
    let mut files = Vec::new();
    collect_mdls(root, &mut files);
    let (mut agree, mut disagree) = (0u64, 0u64);
    let mut static_bones: BTreeMap<String, usize> = BTreeMap::new();
    for path in &files {
        let Some((mdl_bytes, vvd, vtx)) = read_set(path) else { continue };
        let Ok(studio) = mdl::parse(&mdl_bytes) else { continue };
        if studio.info.flags & mdl::FLAG_STATIC_PROP != 0 {
            let identity = studio.bones.len() == 1
                && studio.bones.first().is_some_and(|b| {
                    let m = b.pose_to_bone;
                    let id = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
                    m.iter().flatten().zip(id.iter().flatten()).all(|(x, y)| (x - y).abs() < 1e-4)
                });
            *static_bones.entry(format!("bones={} identity={identity}", studio.bones.len())).or_default() += 1;
        }
        let Ok(model) = load(&mdl_bytes, &vvd, &vtx, &LoadOptions::default()) else { continue };
        for mesh in &model.meshes {
            for tri in mesh.indices.chunks_exact(3) {
                let v = |i: u32| mesh.vertices[i as usize];
                let (a, b, c) = (v(tri[0]), v(tri[1]), v(tri[2]));
                let e1 = [b.pos[0] - a.pos[0], b.pos[1] - a.pos[1], b.pos[2] - a.pos[2]];
                let e2 = [c.pos[0] - a.pos[0], c.pos[1] - a.pos[1], c.pos[2] - a.pos[2]];
                let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let vn: Vec<f32> = (0..3).map(|k| a.normal[k] + b.normal[k] + c.normal[k]).collect();
                let d = n[0] * vn[0] + n[1] * vn[1] + n[2] * vn[2];
                if d > 0.0 {
                    agree += 1;
                } else if d < 0.0 {
                    disagree += 1;
                }
            }
        }
    }
    eprintln!("static prop bones: {static_bones:?}");
    eprintln!("winding: {agree} triangles agree with normals, {disagree} disagree");
    assert!(agree > disagree * 20, "winding should agree with vertex normals");
}

fn layout_stats(
    name: &str,
    mdl_bytes: &[u8],
    vvd: &[u8],
    vtx: &[u8],
    stats: &mut BTreeMap<String, usize>,
    ambiguous: &mut Vec<String>,
) {
    use crate::model::load_with_layout;
    use crate::vtx::{Layout, STRIP_IS_TRILIST, STRIP_IS_TRISTRIP, Vtx};
    use crate::vvd::{VertexMap, Vvd};

    let version = mdl::parse_header(mdl_bytes).map_or(0, |h| h.version);
    let mut bump = |key: &str| *stats.entry(format!("v{version} {key}")).or_default() += 1;
    let opts = LoadOptions { body_groups: crate::BodyGroupSelect::All, ..LoadOptions::default() };
    let std = load_with_layout(mdl_bytes, vvd, vtx, &opts, Some(Layout::STANDARD));
    let ext = load_with_layout(mdl_bytes, vvd, vtx, &opts, Some(Layout::EXTENDED));
    let (ps, pe) = Vtx::parse(vtx)
        .map(|v| (v.strips_partition(Layout::STANDARD), v.strips_partition(Layout::EXTENDED)))
        .unwrap_or((false, false));
    match load(mdl_bytes, vvd, vtx, &opts) {
        Ok(chosen) => {
            let expected = match (ps, pe) {
                (true, false) => std.as_ref().ok(),
                (false, true) => ext.as_ref().ok(),
                _ => None,
            };
            if expected.is_some_and(|e| e.meshes != chosen.meshes) {
                ambiguous.push(format!("{name}: load picked the inconsistent layout"));
            }
        }
        Err(e) => ambiguous.push(format!("{name}: load failed: {e}")),
    }
    let layout = match (&std, &ext) {
        (Ok(a), Ok(b)) => {
            if a.meshes == b.meshes {
                bump("layout: both build, identical");
            } else if ps && pe {
                ambiguous.push(format!("{name}: both layouts plausible, different geometry"));
            } else {
                bump("layout: both build, partition picks one");
            }
            if pe && !ps { Layout::EXTENDED } else { Layout::STANDARD }
        }
        (Ok(_), Err(_)) => {
            bump("layout: standard only");
            Layout::STANDARD
        }
        (Err(_), Ok(_)) => {
            bump("layout: extended only");
            Layout::EXTENDED
        }
        (Err(_), Err(_)) => {
            bump("layout: none");
            return;
        }
    };
    if Vvd::parse(vvd).is_ok_and(|v| v.vertex_map(0).is_ok_and(|m| matches!(m, VertexMap::Table(_)))) {
        bump("vvd with fixups");
    }
    let Ok(vtx) = Vtx::parse(vtx) else { return };
    if (0..vtx.num_lods).any(|l| vtx.material_replacements(l).is_ok_and(|r| !r.is_empty())) {
        bump("lod material replacements");
    }
    let (mut groups_gt1, mut strips_gt1, mut tristrips, mut trilists) = (false, false, false, false);
    for bp in 0..vtx.num_body_parts {
        let Ok((ms, mn)) = vtx.body_part(bp) else { continue };
        for m in 0..mn {
            let Ok((ls, ln)) = vtx.model_lods(Vtx::model_at(ms, m)) else { continue };
            for l in 0..ln {
                let Ok((es, en)) = vtx.lod_meshes(Vtx::lod_at(ls, l)) else { continue };
                for e in 0..en {
                    let Ok((gs, gn)) = vtx.mesh_strip_groups(Vtx::mesh_at(es, e), layout) else { continue };
                    groups_gt1 |= gn > 1;
                    for g in 0..gn {
                        let Ok(group) = vtx.strip_group(gs, g, layout) else { continue };
                        strips_gt1 |= group.num_strips > 1;
                        for s in 0..group.num_strips {
                            let Ok(strip) = vtx.strip(&group, s, layout) else { continue };
                            tristrips |= strip.flags & STRIP_IS_TRISTRIP != 0;
                            trilists |= strip.flags & STRIP_IS_TRILIST != 0;
                        }
                    }
                }
            }
        }
    }
    for (flag, key) in [
        (groups_gt1, "meshes with several strip groups"),
        (strips_gt1, "strip groups with several strips"),
        (tristrips, "strip flag 0x02 (tristrip or sub-d quads)"),
        (trilists, "triangle lists"),
    ] {
        if flag {
            bump(key);
        }
    }
}

#[test]
fn vtx_layout_detection_is_unambiguous() {
    let root = Path::new(GMOD_MODELS);
    if !root.is_dir() {
        eprintln!("skipping: {GMOD_MODELS} not found");
        return;
    }
    let mut files = Vec::new();
    collect_mdls(root, &mut files);
    let mut stats = BTreeMap::new();
    let mut ambiguous = Vec::new();
    for path in &files {
        let Some((mdl_bytes, vvd, vtx)) = read_set(path) else { continue };
        layout_stats(&path.display().to_string(), &mdl_bytes, &vvd, &vtx, &mut stats, &mut ambiguous);
    }
    eprintln!("{} models: {stats:#?}", files.len());
    assert!(ambiguous.is_empty(), "layouts disagree: {ambiguous:?}");
    assert!(!stats.keys().any(|k| k.ends_with("layout: none")));
}

#[test]
#[ignore]
fn vtx_layout_detection_workshop() {
    let Ok(items) = std::fs::read_dir(WORKSHOP) else {
        eprintln!("skipping: {WORKSHOP} not found");
        return;
    };
    let mut stats = BTreeMap::new();
    let mut ambiguous = Vec::new();
    for item in items.flatten() {
        let Ok(files) = std::fs::read_dir(item.path()) else { continue };
        for file in files.flatten() {
            let Some(archive) = common::open_gma(&file.path()) else { continue };
            for mdl in archive.files_with_ext(".mdl") {
                let Ok(f) = common::read_model(&mdl, |p| archive.read(p)) else { continue };
                layout_stats(&mdl, &f.mdl, &f.vvd, &f.vtx, &mut stats, &mut ambiguous);
            }
        }
    }
    eprintln!("{stats:#?}");
    assert!(ambiguous.is_empty(), "{ambiguous:#?}");
}

#[test]
fn corrupted_real_models_never_panic() {
    let mut sets = Vec::new();
    if let Some(set) = read_set(&Path::new(GMOD_MODELS).join("castle_painting.mdl")) {
        sets.push(set);
    }
    let vpk = Path::new(GMOD_MODELS).join(r"..\..\sourceengine\hl2_misc_dir.vpk");
    if let Some(archive) = common::open_vpk(&vpk) {
        for name in ["models/humans/group03m/male_05.mdl", "models/combine_camera/combine_camera.mdl"] {
            if let Ok(f) = common::read_model(name, |p| archive.read(p)) {
                sets.push((f.mdl, f.vvd, f.vtx));
            }
        }
    }
    if sets.is_empty() {
        eprintln!("skipping: no game models found");
        return;
    }
    for (i, (mdl_bytes, vvd, vtx)) in sets.iter().enumerate() {
        let (ok, err) = crate::synthetic_tests::torture([mdl_bytes, vvd, vtx], 0x5eed + i as u64, 4000);
        eprintln!("set {i}: {ok} loaded, {err} rejected");
    }
}

#[test]
#[ignore]
fn debug_layout() {
    let Ok(path) = std::env::var("HP_MDL_DEBUG") else { return };
    let Some((mdl_bytes, vvd_bytes, _vtx)) = read_set(Path::new(&path)) else { return };
    let studio = mdl::parse(&mdl_bytes).expect("mdl");
    eprintln!("version {} flags {:#x}", studio.info.version, studio.info.flags);
    for (b, bp) in studio.body_parts.iter().enumerate() {
        for (m, model) in bp.models.iter().enumerate() {
            eprintln!(
                "bp {b} '{}' model {m} '{}' vertex_index {} ({}) num_vertices {}",
                bp.name,
                model.name,
                model.vertex_index,
                model.vertex_index / 48,
                model.num_vertices
            );
            for (i, mesh) in model.meshes.iter().enumerate() {
                eprintln!("   mesh {i} offset {} count {}", mesh.vertex_offset, mesh.num_vertices);
            }
        }
    }
    let r = crate::reader::Reader::new(&vvd_bytes, crate::error::FileKind::Vvd);
    let lods: Vec<i32> = (0..8).map(|i| r.i32(16 + i * 4).unwrap_or(-1)).collect();
    eprintln!(
        "vvd num_lods {} lod_vertices {lods:?} fixups {} start {} vstart {} tstart {} len {}",
        r.i32(12).unwrap_or(-1),
        r.i32(48).unwrap_or(-1),
        r.i32(52).unwrap_or(-1),
        r.i32(56).unwrap_or(-1),
        r.i32(60).unwrap_or(-1),
        vvd_bytes.len()
    );
    let n = r.i32(48).unwrap_or(0).max(0) as usize;
    let start = r.i32(52).unwrap_or(0).max(0) as usize;
    for i in 0..n.min(40) {
        let f = start + i * 12;
        eprintln!(
            "   fixup lod {} src {} count {}",
            r.i32(f).unwrap_or(-1),
            r.i32(f + 4).unwrap_or(-1),
            r.i32(f + 8).unwrap_or(-1)
        );
    }
}

#[test]
#[ignore]
fn profile_phases() {
    use crate::vtx::{Layout, Vtx};
    use crate::vvd::Vvd;
    use std::time::Instant;
    let Ok(path) = std::env::var("HP_MDL_DEBUG") else { return };
    let Some((m, v, x)) = read_set(Path::new(&path)) else { return };
    let n = 100;
    let time = |name: &str, f: &dyn Fn()| {
        let t = Instant::now();
        for _ in 0..n {
            f();
        }
        eprintln!("{name}: {:.1} us", t.elapsed().as_secs_f64() * 1e6 / n as f64);
    };
    time("mdl::parse", &|| {
        std::hint::black_box(mdl::parse(&m).ok());
    });
    time("vvd parse+map", &|| {
        std::hint::black_box(Vvd::parse(&v).ok().map(|v| v.vertex_map(0).ok().map(|m| m.len())));
    });
    time("vtx partition x2", &|| {
        let vtx = Vtx::parse(&x).ok();
        std::hint::black_box(vtx.map(|t| (t.strips_partition(Layout::STANDARD), t.strips_partition(Layout::EXTENDED))));
    });
    time("posing", &|| {
        let s = mdl::parse(&m).ok();
        std::hint::black_box(s.map(|s| crate::pose::posing(&m, &s, crate::Pose::FirstSequence).sequence));
    });
    time("decode all vvd vertices", &|| {
        let vvd = Vvd::parse(&v).expect("vvd");
        let mut acc = 0.0f32;
        for i in 0..vvd.total_vertices() {
            if let Ok((p, _, _)) = vvd.vertex(i) {
                acc += p[0];
            }
        }
        std::hint::black_box(acc);
    });
    time("load bind pose", &|| {
        std::hint::black_box(load(&m, &v, &x, &LoadOptions { pose: crate::Pose::Bind, ..LoadOptions::default() }).ok());
    });
    for layout in [Layout::STANDARD, Layout::EXTENDED] {
        time(&format!("forced {layout:?}"), &|| {
            std::hint::black_box(
                crate::model::load_with_layout(
                    &m,
                    &v,
                    &x,
                    &LoadOptions { pose: crate::Pose::Bind, ..LoadOptions::default() },
                    Some(layout),
                )
                .is_ok(),
            );
        });
    }
    eprintln!("version {:?}", mdl::parse_header(&m).map(|h| h.version).ok());
    time("full load", &|| {
        std::hint::black_box(load(&m, &v, &x, &LoadOptions::default()).ok());
    });
    let model = load(&m, &v, &x, &LoadOptions::default()).expect("load");
    eprintln!("{} meshes, {} vertices, {} tris", model.meshes.len(), model.vertex_count(), model.triangle_count());
}

#[test]
fn layered_poses_agree_with_plain_poses_and_animate() {
    let root = Path::new(GMOD_MODELS);
    if !root.is_dir() {
        eprintln!("skipping: {GMOD_MODELS} not found");
        return;
    }
    let mut files = Vec::new();
    collect_mdls(root, &mut files);
    let read = |p: &str| std::fs::read(root.parent()?.join(p)).ok();
    let (mut compared, mut agree, mut moving) = (0usize, 0usize, 0usize);
    for path in &files {
        if compared >= 300 {
            break;
        }
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Some(set) = crate::AnimSet::load(&bytes, read) else { continue };
        for index in 0..set.sequence_count().min(8) {
            let Some(anim) = set.sequence_at(index, &[]) else { continue };
            if anim.clip.frames < 3 {
                continue;
            }
            let (Some(a), Some(b)) =
                (set.skin_layered(&anim, 0, 0.0, 0.0), set.skin_layered(&anim, anim.clip.frames / 2, 0.5, 0.0))
            else {
                continue;
            };
            assert!(a.iter().chain(&b).flatten().flatten().all(|v| v.is_finite()), "{}", path.display());
            if a != b {
                moving += 1;
            }
            if let Some(plain) = set.skin(&anim, 0, 0.0) {
                compared += 1;
                let close = plain
                    .iter()
                    .flatten()
                    .flatten()
                    .zip(a.iter().flatten().flatten())
                    .all(|(x, y)| (x - y).abs() < 0.05);
                agree += usize::from(close);
            }
        }
    }
    eprintln!("layered poses: {compared} compared, {agree} agree with the plain pose, {moving} moving");
    assert!(compared < 20 || agree * 10 >= compared * 8, "{agree} of {compared}");
}
