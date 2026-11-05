mod common;

use common::lumps::{Face, Lumps, vertex_of};
use glam::DVec3;
use hp_bspc::materials::contents;

struct Compiled {
    name: String,
    out: hp_bspc::CompileOutput,
    lumps: Lumps,
}

#[test]
fn smoke_volumes_keep_inline_bounds_settings_and_do_not_fill_the_world() {
    for material in ["tools/toolsfog", "tools/toolstrigger", "tools/toolsnodraw"] {
        let mut map = common::Vmf::new();
        map.world =
            common::room(DVec3::new(-256.0, -256.0, 0.0), DVec3::new(256.0, 256.0, 256.0), "dev/dev_measuregeneric01b");
        map.point("info_player_start", DVec3::new(200.0, 200.0, 8.0), &[]);
        map.brush_entity(
            "func_smokevolume",
            &[
                ("origin", "64 32 0"),
                ("targetname", "smoke"),
                ("material", "particle/particle_smokegrenade"),
                ("ParticleSpacingDistance", "32"),
                ("ParticleDrawWidth", "64"),
                ("Density", "1"),
                ("spawnflags", "1"),
            ],
            vec![common::block(DVec3::new(-128.0, -128.0, 32.0), DVec3::new(128.0, 128.0, 160.0), material)],
        );
        let out = common::compile(&map.text());
        let lumps = Lumps::from_bytes(&out.bsp);
        let models = lumps.models();
        assert_eq!(models.len(), 2);
        assert_eq!(models[1].mins, [-192.0, -160.0, 32.0]);
        assert_eq!(models[1].maxs, [64.0, 96.0, 160.0]);
        let entities = lumps.entities();
        let smoke =
            entities.iter().find(|e| e.iter().any(|(k, v)| k == "classname" && v == "func_smokevolume")).unwrap();
        for (key, value) in [
            ("model", "*1"),
            ("material", "particle/particle_smokegrenade"),
            ("ParticleSpacingDistance", "32"),
            ("Density", "1"),
        ] {
            assert!(smoke.iter().any(|(k, v)| k.eq_ignore_ascii_case(key) && v == value), "{material}: missing {key}");
        }
        let leaf = leaf_at(&lumps, DVec3::new(0.0, 0.0, 96.0));
        assert_eq!(lumps.leaves()[leaf].contents as u32 & contents::SOLID, 0);
        assert!(model_leaves(&lumps, 1).iter().all(|i| lumps.leaves()[*i].cluster == -1));
    }
}

fn compiled(name: &str, text: &str, mats: &dyn hp_bspc::MaterialSource) -> Compiled {
    let out = common::compile_with(text, mats, &hp_bspc::CompileOptions::default());
    let lumps = Lumps::from_bytes(&out.bsp);
    Compiled { name: name.to_owned(), out, lumps }
}

fn all_maps() -> Vec<Compiled> {
    let mut v = Vec::new();
    for (name, map) in common::maps::golden() {
        let text = map.text();
        v.push(compiled(name, &text, &common::GameMats));
        v.push(compiled(&format!("{name} (materials by name)"), &text, &common::ByName));
    }
    v.push(compiled("generated", &common::maps::generated(7).text(), &common::GameMats));
    v.push(compiled("generated, leaking", &common::maps::generated_leaking(11).text(), &common::GameMats));
    v
}

fn maps() -> &'static [Compiled] {
    static MAPS: std::sync::OnceLock<Vec<Compiled>> = std::sync::OnceLock::new();
    MAPS.get_or_init(all_maps)
}

fn model_leaves(l: &Lumps, model: usize) -> Vec<usize> {
    let nodes = l.nodes();
    let mut out = Vec::new();
    let mut stack = vec![l.models()[model].head_node];
    while let Some(n) = stack.pop() {
        if n < 0 {
            out.push((-1 - n) as usize);
        } else {
            let node = &nodes[n as usize];
            stack.extend(node.children);
        }
    }
    out.sort_unstable();
    out
}

fn leaf_at(l: &Lumps, p: DVec3) -> usize {
    let (nodes, planes) = (l.nodes(), l.planes());
    let mut n = l.models()[0].head_node;
    while n >= 0 {
        let node = &nodes[n as usize];
        let pl = &planes[node.plane as usize];
        let d = p.dot(DVec3::new(pl.normal[0].into(), pl.normal[1].into(), pl.normal[2].into())) - f64::from(pl.dist);
        n = if d >= 0.0 { node.children[0] } else { node.children[1] };
    }
    (-1 - n) as usize
}

fn dv(p: [f32; 3]) -> DVec3 {
    DVec3::new(p[0].into(), p[1].into(), p[2].into())
}

#[test]
fn every_compile_validates() {
    for m in maps() {
        let problems = hp_bspc::validate::validate(&m.out.bsp);
        assert!(problems.is_empty(), "{}: {problems:?}", m.name);
    }
}

#[test]
fn the_leaking_maps_leak_and_the_others_do_not() {
    for m in maps() {
        assert_eq!(m.out.leak.is_some(), m.name.contains("leak"), "{}", m.name);
    }
    let map = hp_vmf::parse(&common::maps::leaking().text()).unwrap();
    let opts = hp_bspc::CompileOptions { leak_test: true, ..Default::default() };
    match hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &opts, &mut |_| {}) {
        Err(hp_bspc::CompileError::Leaked { path }) => assert!(path.len() >= 2, "a leak path"),
        other => panic!("the leak test lets a leaking map through: {:?}", other.map(|o| o.leak.is_some())),
    }
}

#[test]
fn every_face_hangs_from_the_tree() {
    for m in maps() {
        let l = &m.lumps;
        let (faces, nodes, models, leaf_faces) = (l.faces(), l.nodes(), l.models(), l.leaf_faces());
        let mut on_nodes = vec![0u32; faces.len()];
        for (mi, model) in models.iter().enumerate() {
            let range = model.first_face as usize..(model.first_face + model.num_faces) as usize;
            let mut stack = vec![model.head_node];
            while let Some(n) = stack.pop() {
                if n < 0 {
                    continue;
                }
                let node = &nodes[n as usize];
                let (a, b) = (node.first_face as usize, node.first_face as usize + node.num_faces as usize);
                assert!(
                    a >= range.start && b <= range.end,
                    "{}: node {n} lists faces {a}..{b}, outside model {mi} ({range:?})",
                    m.name
                );
                for c in &mut on_nodes[a..b] {
                    *c += 1;
                }
                stack.extend(node.children);
            }
        }
        let mut in_leaves = vec![false; faces.len()];
        for &f in &leaf_faces {
            in_leaves[f as usize] = true;
        }
        for (i, f) in faces.iter().enumerate() {
            if f.on_node != 0 {
                assert_eq!(on_nodes[i], 1, "{}: face {i} is on {} nodes", m.name, on_nodes[i]);
            } else {
                assert_eq!(on_nodes[i], 0, "{}: face {i} (not on a node) listed by a node", m.name);
                assert!(in_leaves[i] || f.dispinfo != -1, "{}: detail face {i} is in no leaf", m.name);
            }
        }
        let covered: usize = models.iter().map(|m| m.num_faces as usize).sum();
        assert_eq!(covered, faces.len(), "{}: the models cover every face", m.name);
    }
}

#[test]
fn leaf_and_node_indices_are_valid() {
    for m in maps() {
        let l = &m.lumps;
        let (leaves, nodes, faces, brushes) = (l.leaves(), l.nodes(), l.faces(), l.brushes());
        let (leaf_faces, leaf_brushes, planes) = (l.leaf_faces(), l.leaf_brushes(), l.planes());
        for (i, leaf) in leaves.iter().enumerate() {
            let fe = leaf.first_leaf_face as usize + leaf.num_leaf_faces as usize;
            assert!(fe <= leaf_faces.len(), "{}: leaf {i} faces end {fe} past {}", m.name, leaf_faces.len());
            let be = leaf.first_leaf_brush as usize + leaf.num_leaf_brushes as usize;
            assert!(be <= leaf_brushes.len(), "{}: leaf {i} brushes end {be} past {}", m.name, leaf_brushes.len());
            for &f in &leaf_faces[leaf.first_leaf_face as usize..fe] {
                assert!((f as usize) < faces.len(), "{}: leaf {i} lists face {f}", m.name);
            }
            for &b in &leaf_brushes[leaf.first_leaf_brush as usize..be] {
                assert!((b as usize) < brushes.len(), "{}: leaf {i} lists brush {b}", m.name);
            }
            let water = l.leaf_water().len();
            assert!(
                leaf.water_data == -1 || (leaf.water_data as usize) < water,
                "{}: leaf {i} water data {}",
                m.name,
                leaf.water_data
            );
        }
        for (i, node) in nodes.iter().enumerate() {
            assert!((node.plane as usize) < planes.len(), "{}: node {i} plane {}", m.name, node.plane);
            for c in node.children {
                if c >= 0 {
                    assert!((c as usize) < nodes.len() && c as usize > i, "{}: node {i} child {c}", m.name);
                } else {
                    assert!(((-1 - c) as usize) < leaves.len(), "{}: node {i} leaf child {c}", m.name);
                }
            }
        }
        let mut seen = vec![false; leaves.len()];
        for model in 0..l.models().len() {
            for leaf in model_leaves(l, model) {
                assert!(!seen[leaf], "{}: leaf {leaf} reached twice", m.name);
                seen[leaf] = true;
            }
        }
    }
}

fn t_junctions(polys: &[Vec<DVec3>]) -> Vec<String> {
    let mut all: Vec<DVec3> = polys.iter().flatten().copied().collect();
    all.sort_by(|a, b| a.to_array().partial_cmp(&b.to_array()).unwrap());
    all.dedup_by(|a, b| a.distance(*b) < 1e-3);
    let mut out = Vec::new();
    for (fi, poly) in polys.iter().enumerate() {
        for k in 0..poly.len() {
            let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
            let ab = b - a;
            let len2 = ab.length_squared();
            for &p in &all {
                if p.distance(a) < 0.01 || p.distance(b) < 0.01 {
                    continue;
                }
                let t = (p - a).dot(ab) / len2;
                if t > 0.0 && t < 1.0 && (a + ab * t).distance(p) < 0.01 {
                    out.push(format!("vertex {p} lies on edge {a} -> {b} of polygon {fi}"));
                }
            }
        }
    }
    out
}

fn polygons(l: &Lumps, faces: &[Face]) -> Vec<Vec<DVec3>> {
    let (edges, surf, verts) = (l.edges(), l.surfedges(), l.vertices());
    faces
        .iter()
        .map(|f| {
            (0..f.num_edges as i32)
                .map(|k| dv(verts[vertex_of(&edges, surf[(f.first_edge + k) as usize]) as usize]))
                .collect()
        })
        .collect()
}

#[test]
fn the_t_junction_check_finds_them() {
    let v = DVec3::new;
    let square = vec![v(0.0, 0.0, 0.0), v(2.0, 0.0, 0.0), v(2.0, 2.0, 0.0), v(0.0, 2.0, 0.0)];
    let small = vec![v(2.0, 0.0, 0.0), v(3.0, 0.0, 0.0), v(3.0, 1.0, 0.0), v(2.0, 1.0, 0.0)];
    assert_eq!(t_junctions(&[square.clone(), small.clone()]).len(), 1, "(2 1 0) lies inside the edge of the square");
    let fixed = vec![v(0.0, 0.0, 0.0), v(2.0, 0.0, 0.0), v(2.0, 1.0, 0.0), v(2.0, 2.0, 0.0), v(0.0, 2.0, 0.0)];
    assert!(t_junctions(&[fixed, small]).is_empty());
    let m = maps().iter().find(|m| m.name == "golden_tjunc").unwrap();
    let originals: Vec<Face> = m.lumps.original_faces().into_iter().filter(|f| f.dispinfo == -1).collect();
    assert!(!t_junctions(&polygons(&m.lumps, &originals)).is_empty(), "the T-junction map has T-junctions to fix");
}

#[test]
fn no_t_junction_is_left() {
    for m in maps() {
        let l = &m.lumps;
        let faces = l.faces();
        for (mi, model) in l.models().iter().enumerate() {
            let list: Vec<Face> = faces[model.first_face as usize..(model.first_face + model.num_faces) as usize]
                .iter()
                .filter(|f| f.dispinfo == -1)
                .copied()
                .collect();
            let found = t_junctions(&polygons(l, &list));
            assert!(found.is_empty(), "{}: T-junctions in model {mi}: {found:?}", m.name);
        }
    }
}

#[test]
fn every_open_world_leaf_has_a_cluster() {
    for m in maps() {
        let l = &m.lumps;
        let leaves = l.leaves();
        let clusters = m.out.clusters;
        let leaks = m.out.leak.is_some();
        let mut used = vec![false; clusters.max(0) as usize];
        for i in model_leaves(l, 0) {
            let leaf = &leaves[i];
            if leaks {
                assert_eq!(leaf.cluster, 0, "{}: leaf {i} of a leaking map has cluster {}", m.name, leaf.cluster);
                continue;
            }
            if leaf.contents & contents::SOLID as i32 != 0 {
                assert_eq!(leaf.cluster, -1, "{}: solid leaf {i} has cluster {}", m.name, leaf.cluster);
                continue;
            }
            assert!(leaf.cluster >= 0, "{}: open leaf {i} (contents {:#x}) has no cluster", m.name, leaf.contents);
            assert!((leaf.cluster as i32) < clusters, "{}: leaf {i} cluster {} of {clusters}", m.name, leaf.cluster);
            used[leaf.cluster as usize] = true;
        }
        if leaks {
            assert_eq!(
                (clusters, l.raw(hp_bspc::emit::lump::VISIBILITY).len()),
                (0, 0),
                "{}: a leaking map has no visibility",
                m.name
            );
        } else {
            assert!(clusters > 0, "{}: no cluster", m.name);
            assert!(used.iter().all(|u| *u), "{}: a cluster has no leaf", m.name);
            assert!(l.raw(hp_bspc::emit::lump::VISIBILITY).is_empty(), "{}: visibility written without VVIS", m.name);
        }
    }
}

#[test]
fn the_visibility_has_every_cluster() {
    let mut list: Vec<(String, String)> =
        common::maps::golden().into_iter().map(|(n, m)| (n.to_owned(), m.text())).collect();
    list.push(("generated".into(), common::maps::generated(7).text()));
    for (name, text) in list {
        let opts = hp_bspc::CompileOptions { vis: hp_bspc::VisLevel::Fast, ..Default::default() };
        let out = common::compile_with(&text, &common::GameMats, &opts);
        let problems = hp_bspc::validate::validate(&out.bsp);
        assert!(problems.is_empty(), "{name}: {problems:?}");
        let l = Lumps::from_bytes(&out.bsp);
        if out.leak.is_some() {
            assert!(l.raw(hp_bspc::emit::lump::VISIBILITY).is_empty(), "{name}: visibility of a leaking map");
        } else {
            assert_eq!(l.vis_clusters(), out.clusters, "{name}: the visibility lump has every cluster");
            let max = l.leaves().iter().map(|x| x.cluster).max().unwrap_or(-1);
            assert_eq!(i32::from(max) + 1, out.clusters, "{name}: the leaves use every cluster");
        }
    }
}

#[test]
fn portals_agree_with_the_clusters() {
    for m in maps() {
        let l = &m.lumps;
        if m.out.leak.is_some() {
            assert_eq!(m.out.prt, "PRT1\n0\n0\n", "{}", m.name);
            assert!(m.out.vis_portals.is_empty(), "{}", m.name);
            continue;
        }
        let leaves = l.leaves();
        let mut lines = m.out.prt.lines();
        assert_eq!(lines.next(), Some("PRT1"), "{}", m.name);
        let nc: i32 = lines.next().unwrap().trim().parse().unwrap();
        let np: usize = lines.next().unwrap().trim().parse().unwrap();
        assert_eq!(nc, m.out.clusters, "{}: clusters of the portal file", m.name);
        let body: Vec<&str> = lines.filter(|x| !x.trim().is_empty()).collect();
        assert_eq!(body.len(), np, "{}: portals of the portal file", m.name);
        assert_eq!(m.out.vis_portals.len(), np, "{}: portals of the output", m.name);
        for (line, portal) in body.iter().zip(&m.out.vis_portals) {
            let head: Vec<&str> = line.split('(').next().unwrap().split_whitespace().collect();
            let n: usize = head[0].parse().unwrap();
            let (a, b): (u32, u32) = (head[1].parse().unwrap(), head[2].parse().unwrap());
            assert_eq!(n, portal.points.len(), "{}: {line}", m.name);
            assert_eq!(line.matches('(').count(), n, "{}: {line}", m.name);
            assert_eq!([a, b], portal.clusters, "{}: {line}", m.name);
            assert!(a != b && (a as i32) < nc && (b as i32) < nc, "{}: {line}", m.name);
            let pts = &portal.points;
            let center = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
            let mut normal = DVec3::ZERO;
            for k in 0..pts.len() {
                normal += pts[k].cross(pts[(k + 1) % pts.len()]);
            }
            let normal = normal.normalize();
            let mut want = vec![a as i16, b as i16];
            want.sort_unstable();
            let found: Vec<Vec<i16>> = [0.02, 0.05, 0.1, 0.25, 0.5]
                .iter()
                .map(|d| {
                    let mut sides: Vec<i16> =
                        [-d, *d].iter().map(|d| leaves[leaf_at(l, center + normal * *d)].cluster).collect();
                    sides.sort_unstable();
                    sides
                })
                .collect();
            assert!(found.contains(&want), "{}: the leaves on both sides of portal {line}: clusters {found:?}", m.name);
        }
    }
}

#[test]
fn leaking_maps_compile_whole() {
    for m in maps().iter().filter(|m| m.name.contains("leak")) {
        let path = m.out.leak.as_ref().expect("a leak");
        assert!(path.len() >= 2, "{}: leak path {path:?}", m.name);
        assert!(m.out.faces > 0 && m.out.brushes > 0, "{}: faces {} brushes {}", m.name, m.out.faces, m.out.brushes);
        assert!(!m.lumps.leaves().is_empty(), "{}", m.name);
    }
}

#[test]
fn a_world_without_brushes_compiles() {
    let blocks = hp_bspc::CompileOptions::default().block_size;
    let cases = [
        (true, false, blocks),
        (true, false, 0),
        (false, false, blocks),
        (false, false, 0),
        (false, true, blocks),
        (false, true, 0),
    ];
    for (with_entity, with_disp, block_size) in cases {
        let mut m = common::Vmf::new();
        if with_disp {
            let mut patch =
                common::block(DVec3::new(-64.0, -64.0, 0.0), DVec3::new(64.0, 64.0, 16.0), "dev/dev_measuregeneric01b");
            patch.sides[0].disp = Some(common::disp_block(2, DVec3::new(-64.0, -64.0, 16.0), 1.5));
            m.world.push(patch);
        }
        if with_entity {
            m.brush_entity(
                "func_door",
                &[("targetname", "door")],
                vec![common::block(
                    DVec3::new(-32.0, -32.0, 0.0),
                    DVec3::new(32.0, 32.0, 64.0),
                    "dev/dev_measuregeneric01b",
                )],
            );
            m.point("info_player_start", DVec3::new(0.0, 200.0, 8.0), &[("angles", "0 0 0")]);
        }
        let opts = hp_bspc::CompileOptions { block_size, ..Default::default() };
        let map = hp_vmf::parse(&m.text()).expect("the VMF parses");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .stack_size(1 << 20)
            .spawn(move || {
                let r = hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &opts, &mut |_| {});
                let _ = tx.send(r.map(|_| ()).map_err(|e| format!("{e:?}")));
            })
            .expect("thread");
        let done = rx.recv_timeout(std::time::Duration::from_secs(60));
        assert!(
            done.is_ok(),
            "the compile ends (entity {with_entity}, displacement {with_disp}, block size {block_size}): {done:?}"
        );
    }
}

#[test]
fn trains_are_built_around_their_origin() {
    let out = common::compile(&common::maps::trains().text());
    let lumps = Lumps::from_bytes(&out.bsp);
    let ents = lumps.entities();
    let get = |e: &Vec<(String, String)>, k: &str| e.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    let of = |class: &str| ents.iter().find(|e| get(e, "classname").as_deref() == Some(class)).expect(class).clone();
    let models = lumps.models();
    for (class, origin, half) in
        [("func_tracktrain", "-256 -256 64", [64.0, 32.0, 32.0]), ("func_tanktrain", "256 256 64", [32.0, 16.0, 16.0])]
    {
        let e = of(class);
        assert_eq!(get(&e, "origin").as_deref(), Some(origin), "{class}: {e:?}");
        let model: usize = get(&e, "model").and_then(|m| m.trim_start_matches('*').parse().ok()).expect("model");
        assert!(model > 0);
        let m = &models[model];
        for (k, h) in half.iter().enumerate() {
            assert!((m.mins[k] + h).abs() < 1.5 && (m.maxs[k] - h).abs() < 1.5, "{class} {m:?}");
        }
        assert_eq!(get(&e, "target").as_deref(), Some(if class == "func_tanktrain" { "t3" } else { "t1" }));
    }
    let track = of("path_track");
    assert!(get(&track, "target").is_some() && get(&track, "origin").is_some());
    let map = hp_vmf::parse(&common::maps::trains().text()).expect("map");
    let again = hp_bspc::compile::entities_only(&map, &common::GameMats, &out.bsp).expect("entities only");
    let train = Lumps::from_bytes(&again)
        .entities()
        .into_iter()
        .find(|e| get(e, "classname").as_deref() == Some("func_tracktrain"));
    assert_eq!(
        train.and_then(|e| get(&e, "origin")).as_deref(),
        Some("-256 -256 64"),
        "the entity-only compile agrees"
    );
}
