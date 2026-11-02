use super::*;
use crate::types::MeshBatch;

fn vert(p: [f32; 3], n: [f32; 3]) -> Vertex {
    Vertex { pos: p, normal: n, uv: [0.0; 2], alpha: 1.0 }
}

fn cube(min: [f32; 3], size: f32, materials: [MaterialId; 2]) -> ObjectMesh {
    let [x, y, z] = min;
    let s = size;
    let quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], n: [f32; 3]| {
        (vec![vert(a, n), vert(b, n), vert(c, n), vert(d, n)], vec![0u32, 1, 2, 0, 2, 3])
    };
    let faces = [
        quad([x, y, z + s], [x + s, y, z + s], [x + s, y + s, z + s], [x, y + s, z + s], [0.0, 0.0, 1.0]),
        quad([x, y, z], [x + s, y, z], [x + s, y + s, z], [x, y + s, z], [0.0, 0.0, -1.0]),
    ];
    let batches = faces
        .into_iter()
        .zip(materials)
        .map(|((vertices, indices), material)| MeshBatch {
            material,
            vertices,
            indices,
            blend: Vec::new(),
            cubemaps: Vec::new(),
        })
        .collect();
    let mut edges = Vec::new();
    for i in 0..4 {
        edges.push([x + i as f32, y, z]);
        edges.push([x + i as f32, y + s, z]);
    }
    ObjectMesh {
        batches,
        edges,
        aabb_min: min,
        aabb_max: [x + s, y + s, z + s],
        wire_color: [255, 0, 0],
        ..Default::default()
    }
}

fn scene() -> Scene {
    Scene::new([1 << 24, 1 << 24, 1 << 24])
}

fn bytes(out: &FlushOutput, kind: ArenaKind) -> usize {
    out.writes.iter().filter(|w| w.arena == kind).map(|w| w.data.len()).sum()
}

#[test]
fn insert_creates_chunks_and_writes() {
    let mut s = scene();
    s.upsert(1, cube([0.0; 3], 64.0, [10, 11]));
    s.upsert(2, cube([100.0, 0.0, 0.0], 64.0, [10, 10]));
    let out = s.flush(true);
    assert_eq!(s.stored_objects, 2);
    assert_eq!(s.live_chunks, 3);
    assert_eq!(s.materials.len(), 2);
    assert_eq!(bytes(&out, ArenaKind::MeshVerts), 16 * size_of::<GpuVertex>());
    assert_eq!(bytes(&out, ArenaKind::Indices), 24 * 4);
    assert_eq!(bytes(&out, ArenaKind::EdgeVerts), 16 * size_of::<EdgeVertex>());
    assert_eq!(out.state_ranges, vec![(0, 4)]);
    assert_eq!(s.states[2].color, pack_rgb8([255, 0, 0]));
    let out = s.flush(true);
    assert!(out.writes.is_empty() && out.state_ranges.is_empty());
}

fn disp_cube(min: [f32; 3], with_marks: bool) -> ObjectMesh {
    let mut m = cube(min, 64.0, [1, 1]);
    let [x, y, z] = min;
    m.detail_edges = vec![[x, y + 32.0, z], [x + 64.0, y + 32.0, z], [x + 32.0, y, z], [x + 32.0, y + 64.0, z]];
    if with_marks {
        let t = |class| crate::types::MarkTri { p: [[x, y, z], [x + 64.0, y, z], [x, y + 64.0, z]], class };
        m.marks = vec![t(crate::types::WalkClass::Walkable), t(crate::types::WalkClass::Steep)];
    }
    m
}

fn edge_verts(out: &FlushOutput) -> Vec<EdgeVertex> {
    out.writes
        .iter()
        .filter(|w| w.arena == ArenaKind::EdgeVerts)
        .flat_map(|w| bytemuck::cast_slice::<u8, EdgeVertex>(&w.data).to_vec())
        .collect()
}

#[test]
fn source_edges_carry_their_bit() {
    let mut s = scene();
    let mut m = disp_cube([0.0; 3], false);
    m.source_edges = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 64.0], [64.0, 0.0, 0.0], [64.0, 0.0, 64.0]];
    s.upsert(1, m);
    let out = s.flush(true);
    let verts = edge_verts(&out);
    let slot = s.objects[&1].slot;
    assert_eq!(verts.iter().filter(|v| v.slot == slot | SOURCE_BIT).count(), 4);
    assert_eq!(verts.iter().filter(|v| v.slot == slot).count(), 8, "the outline is apart");
    assert_eq!(verts.iter().filter(|v| v.slot == slot | DETAIL_BIT).count(), 4);
}

#[test]
fn detail_edges_and_marks_carry_their_bits() {
    let mut s = scene();
    s.upsert(1, disp_cube([0.0; 3], true));
    let out = s.flush(true);
    assert_eq!(s.live_chunks, 3);
    assert_eq!(s.mark_chunks.len(), 1);
    assert_eq!(s.edge_chunks.len(), 1);
    let verts = edge_verts(&out);
    let slot = s.objects[&1].slot;
    let outline = verts.iter().filter(|v| v.slot == slot).count();
    let detail = verts.iter().filter(|v| v.slot == slot | DETAIL_BIT).count();
    assert_eq!((outline, detail), (8, 4), "4 outline lines, 2 detail lines");
    let classes: Vec<u32> =
        verts.iter().filter(|v| v.slot & !DETAIL_BIT > SLOT_MASK).map(|v| v.slot >> CLASS_SHIFT).collect();
    assert_eq!(classes, vec![1, 1, 1, 2, 2, 2], "three vertices per mark, the class in the high bits");
    assert!(verts.iter().all(|v| v.slot & SLOT_MASK == slot));

    s.upsert(1, disp_cube([0.0, 8.0, 0.0], true));
    let out = s.flush(true);
    assert_eq!(bytes(&out, ArenaKind::EdgeVerts), (12 + 6) * size_of::<EdgeVertex>());
    assert_eq!(s.live_chunks, 3);
    s.upsert(1, disp_cube([0.0, 8.0, 0.0], false));
    s.flush(true);
    assert!(s.mark_chunks.is_empty());
    assert_eq!(s.live_chunks, 2);
    s.upsert(1, disp_cube([0.0; 3], true));
    s.flush(true);
    s.set_flags(1, ObjectFlags::HIDDEN);
    s.flush(true);
    let marks = s.mark_chunks[0];
    assert!(s.chunks[marks as usize].as_ref().is_some_and(|c| c.all_hidden && c.is_mark() && c.is_edge()));
    s.remove(1);
    s.flush(true);
    assert_eq!(s.live_chunks, 0);
    assert!(s.mark_chunks.is_empty() && s.edge_chunks.is_empty());
}

#[test]
fn indices_are_absolute_and_oriented() {
    let mut s = scene();
    s.upsert(1, cube([0.0; 3], 64.0, [1, 1]));
    s.upsert(2, cube([10.0, 0.0, 0.0], 64.0, [1, 1]));
    let out = s.flush(true);
    let idx: Vec<u32> = out
        .writes
        .iter()
        .filter(|w| w.arena == ArenaKind::Indices)
        .flat_map(|w| bytemuck::cast_slice::<u8, u32>(&w.data).to_vec())
        .collect();
    assert_eq!(idx.len(), 24);
    let verts: Vec<GpuVertex> = out
        .writes
        .iter()
        .filter(|w| w.arena == ArenaKind::MeshVerts)
        .flat_map(|w| bytemuck::cast_slice::<u8, GpuVertex>(&w.data).to_vec())
        .collect();
    let chunk = s.chunks.iter().flatten().find(|c| !c.is_edge()).expect("mesh chunk");
    for tri in idx.chunks(3) {
        let v = |i: u32| verts[(i - chunk.vtx.offset) as usize];
        let p = |i: u32| Vec3::from(v(i).pos);
        let n = (p(tri[1]) - p(tri[0])).cross(p(tri[2]) - p(tri[0]));
        let nz = ((v(tri[0]).nrm_alpha >> 16) & 0xff) as u8 as i8;
        assert!(n.z * f32::from(nz) > 0.0, "triangle must face its normal");
    }
}

#[test]
fn same_topology_update_is_in_place() {
    let mut s = scene();
    for i in 0..50u32 {
        s.upsert(i, cube([i as f32 * 70.0, 0.0, 0.0], 64.0, [1, 2]));
    }
    s.flush(true);
    let chunks_before = s.live_chunks;
    s.upsert(7, cube([7.0 * 70.0, 5.0, 0.0], 64.0, [1, 2]));
    let out = s.flush(true);
    assert_eq!(s.live_chunks, chunks_before);
    assert_eq!(bytes(&out, ArenaKind::MeshVerts), 8 * size_of::<GpuVertex>());
    assert_eq!(bytes(&out, ArenaKind::Indices), 12 * 4);
    assert_eq!(bytes(&out, ArenaKind::EdgeVerts), 8 * size_of::<EdgeVertex>());
}

#[test]
fn append_and_remove_use_slack_and_holes() {
    let mut s = scene();
    for i in 0..40u32 {
        s.upsert(i, cube([i as f32 * 70.0, 0.0, 0.0], 64.0, [1, 1]));
    }
    s.flush(true);
    s.upsert(100, cube([0.0, 300.0, 0.0], 64.0, [1, 1]));
    let out = s.flush(true);
    assert_eq!(bytes(&out, ArenaKind::MeshVerts), 8 * size_of::<GpuVertex>());
    s.remove(3);
    let out = s.flush(true);
    assert_eq!(bytes(&out, ArenaKind::MeshVerts), 0);
    assert_eq!(bytes(&out, ArenaKind::Indices), 12 * 4);
    let zeroed = out.writes.iter().filter(|w| w.arena == ArenaKind::Indices).all(|w| w.data.iter().all(|&b| b == 0));
    assert!(zeroed);
    assert_eq!(bytes(&out, ArenaKind::EdgeVerts), 8 * size_of::<EdgeVertex>());
    assert_eq!(s.stored_objects, 40);
    assert!(!s.objects.contains_key(&3));
}

#[test]
fn overflow_rebuilds_chunk() {
    let mut s = scene();
    s.upsert(1, cube([0.0; 3], 64.0, [1, 1]));
    s.flush(true);
    for i in 2..200u32 {
        s.upsert(i, cube([i as f32 * 70.0, 0.0, 0.0], 64.0, [1, 1]));
    }
    let out = s.flush(true);
    assert_eq!(bytes(&out, ArenaKind::MeshVerts), 199 * 8 * size_of::<GpuVertex>());
    let chunk = s.chunks.iter().flatten().find(|c| !c.is_edge()).expect("mesh chunk");
    assert_eq!(chunk.used_v, 199 * 8);
    assert_eq!(chunk.members.len(), 199);
    assert!(chunk.vtx.len >= chunk.used_v);
}

#[test]
fn many_removals_compact_chunk() {
    let mut s = scene();
    for i in 0..100u32 {
        s.upsert(i, cube([i as f32 * 70.0, 0.0, 0.0], 64.0, [1, 1]));
    }
    s.flush(true);
    for i in 0..60u32 {
        s.remove(i);
    }
    s.flush(true);
    let chunk = s.chunks.iter().flatten().find(|c| !c.is_edge()).expect("mesh chunk");
    assert_eq!(chunk.dead_v, 0, "compacted");
    assert_eq!(chunk.used_v, 40 * 8);
}

#[test]
fn removing_everything_frees_arenas() {
    let mut s = scene();
    for i in 0..10u32 {
        s.upsert(i, cube([i as f32 * 3000.0, 0.0, 0.0], 64.0, [1, 2]));
    }
    s.flush(true);
    for i in 0..10u32 {
        s.remove(i);
    }
    s.flush(true);
    assert_eq!(s.live_chunks, 0);
    assert_eq!(s.stored_objects, 0);
    assert!(s.arenas.iter().all(|a| a.end() == 0));
    assert!(s.objects.is_empty());
}

#[test]
fn material_level_adapts_to_density() {
    let mut s = scene();
    for i in 0..3000u32 {
        let x = (i % 60) as f32 * 600.0 - 18000.0;
        let y = (i / 60) as f32 * 600.0 - 15000.0;
        s.upsert(i, cube([x, y, 0.0], 64.0, [1, 1]));
    }
    s.flush(true);
    assert_eq!(s.materials[0].tris, 12_000);
    assert_eq!(s.materials[0].level, COARSEST_LEVEL);
    let coarse = s.materials[0].chunks.len();
    for i in 3000..9000u32 {
        let x = (i % 60) as f32 * 600.0 - 18000.0;
        let y = (i / 60) as f32 * 300.0 - 15000.0;
        s.upsert(i, cube([x, y, 100.0], 64.0, [1, 1]));
    }
    s.flush(true);
    assert_eq!(s.materials[0].level, 1);
    assert!(s.materials[0].chunks.len() > coarse);
    let total: usize =
        s.materials[0].chunks.iter().filter_map(|&c| s.chunks[c as usize].as_ref()).map(|c| c.members.len()).sum();
    assert_eq!(total, 9000);
}

#[test]
fn material_hints_set_the_final_level_at_once() {
    let mut s = scene();
    s.hint_material_triangles(1, 200_000);
    assert_eq!(s.materials[0].level, 0);
    for batch in 0..3u32 {
        for i in batch * 100..(batch + 1) * 100 {
            let x = (i % 50) as f32 * 600.0;
            s.upsert(i, cube([x, (i / 50) as f32 * 600.0, 0.0], 64.0, [1, 1]));
        }
        s.flush(true);
        assert_eq!(s.materials[0].level, 0, "batch {batch}");
    }
    s.clear();
    assert_eq!(s.materials[0].hint, 0);
    assert_eq!(s.materials[0].level, COARSEST_LEVEL);
}

#[test]
fn flags_and_refs() {
    let mut s = scene();
    s.set_flags(5, ObjectFlags::SELECTED);
    let slot = s.acquire_ref(5);
    assert_eq!(s.states[slot as usize].flags, ObjectFlags::SELECTED.0);
    s.release_ref(5);
    assert!(s.objects.contains_key(&5));
    s.remove(5);
    assert!(!s.objects.contains_key(&5));
    let a = s.acquire_ref(9);
    let b = s.acquire_ref(9);
    assert_eq!(a, b);
    s.release_ref(9);
    assert!(s.objects.contains_key(&9));
    s.release_ref(9);
    assert!(!s.objects.contains_key(&9));
    let _ = s.acquire_ref(11);
    s.set_flags(11, ObjectFlags::SELECTED);
    s.release_ref(11);
    assert!(!s.objects.contains_key(&11));
}

#[test]
fn malformed_meshes_are_sanitized() {
    let mut s = scene();
    let mesh = ObjectMesh {
        batches: vec![MeshBatch {
            material: 1,
            vertices: vec![vert([0.0; 3], [0.0, 0.0, 1.0]), vert([f32::NAN, 0.0, 0.0], [0.0; 3])],
            indices: vec![0, 1, 2, 0, 0, 0, 7],
            blend: Vec::new(),
            cubemaps: Vec::new(),
        }],
        edges: vec![[0.0; 3], [1.0, 0.0, 0.0], [f32::INFINITY; 3]],
        aabb_min: [f32::NAN; 3],
        aabb_max: [1.0; 3],
        wire_color: [0; 3],
        ..Default::default()
    };
    s.upsert(1, mesh);
    let out = s.flush(true);
    assert_eq!(bytes(&out, ArenaKind::Indices), 12);
    assert_eq!(bytes(&out, ArenaKind::EdgeVerts), 2 * size_of::<EdgeVertex>());
    let e = s.objects.get(&1).and_then(|e| e.stored.as_ref()).expect("stored");
    assert!(e.aabb_min.is_finite());
}

#[test]
fn clear_resets_everything() {
    let mut s = scene();
    for i in 0..20u32 {
        s.upsert(i, cube([i as f32 * 70.0, 0.0, 0.0], 64.0, [1, 2]));
    }
    s.flush(true);
    let _ = s.acquire_ref(3);
    s.clear();
    s.flush(true);
    assert_eq!(s.live_chunks, 0);
    assert_eq!(s.stored_objects, 0);
    assert_eq!(s.objects.len(), 1, "the referenced object keeps its slot");
    s.upsert(1, cube([0.0; 3], 64.0, [1, 2]));
    s.flush(true);
    assert_eq!(s.stored_objects, 1);
}

#[test]
fn upsert_then_remove_in_same_frame() {
    let mut s = scene();
    s.upsert(1, cube([0.0; 3], 64.0, [1, 2]));
    s.remove(1);
    let out = s.flush(true);
    assert!(out.writes.is_empty());
    assert!(s.objects.is_empty());
    s.upsert(1, cube([0.0; 3], 64.0, [1, 2]));
    s.flush(true);
    s.remove(1);
    s.upsert(1, cube([0.0; 3], 32.0, [1, 2]));
    s.flush(true);
    assert_eq!(s.stored_objects, 1);
}

#[test]
fn random_edit_stress_matches_model() {
    let mut s = scene();
    let mut seed = 0x1234_5678u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut alive = std::collections::BTreeSet::new();
    for round in 0..40 {
        for _ in 0..50 {
            let id = rnd() % 300;
            match rnd() % 4 {
                0 => {
                    s.remove(id);
                    alive.remove(&id);
                }
                _ => {
                    let x = (rnd() % 20000) as f32 - 10000.0;
                    let mats = [rnd() % 5, rnd() % 5];
                    s.upsert(id, cube([x, (rnd() % 5000) as f32, 0.0], 32.0 + (rnd() % 3) as f32, mats));
                    alive.insert(id);
                }
            }
        }
        s.flush(true);
        assert_eq!(s.stored_objects as usize, alive.len(), "round {round}");
        for (&id, e) in &s.objects {
            let Some(st) = &e.stored else { continue };
            for b in &st.batches {
                let c = b.chunk.expect("batch has a chunk");
                let ch = s.chunks[c as usize].as_ref().expect("chunk alive");
                let m = ch.members.get(&id).expect("member");
                assert_eq!(m.v_cnt as usize, b.verts.len());
                assert!(m.v_off + m.v_cnt <= ch.used_v && ch.used_v <= ch.vtx.len);
                assert!(m.i_off + m.i_cnt <= ch.used_i && ch.used_i <= ch.idx.len);
            }
        }
        let members: usize = s.chunks.iter().flatten().filter(|c| !c.is_edge()).map(|c| c.members.len()).sum();
        let batches: usize = s.objects.values().filter_map(|e| e.stored.as_ref()).map(|st| st.batches.len()).sum();
        assert_eq!(members, batches);
        for kind in [ArenaKind::MeshVerts, ArenaKind::Indices, ArenaKind::EdgeVerts] {
            let mut ranges: Vec<(u32, u32)> = s
                .chunks
                .iter()
                .flatten()
                .filter_map(|c| match kind {
                    ArenaKind::MeshVerts if !c.is_edge() => Some((c.vtx.offset, c.vtx.len)),
                    ArenaKind::EdgeVerts if c.is_edge() => Some((c.vtx.offset, c.vtx.len)),
                    ArenaKind::Indices if !c.is_edge() => Some((c.idx.offset, c.idx.len)),
                    _ => None,
                })
                .filter(|r| r.1 > 0)
                .collect();
            ranges.sort_unstable();
            for w in ranges.windows(2) {
                assert!(w[0].0 + w[0].1 <= w[1].0, "{kind:?} ranges overlap: {:?}", w);
            }
        }
    }
}

#[test]
fn classic_colors_live_in_the_object_state() {
    let mut s = scene();
    s.upsert(1, cube([0.0; 3], 64.0, [10, 11]));
    s.set_classic_color(1, Some([255, 255, 255]));
    let slot = s.objects[&1].slot as usize;
    assert_eq!(s.states[slot].color, pack_rgb8([255, 0, 0]));
    assert_eq!(s.states[slot].classic, pack_rgb8([255, 255, 255]));
    s.set_flags(1, ObjectFlags::SELECTED);
    s.set_wire_color(1, [0, 0, 255]);
    s.set_tint(1, 0x80ff_ffff);
    assert_eq!(s.states[slot].classic, pack_rgb8([255, 255, 255]), "other writes keep the classic color");
    assert_eq!(s.wire_color(1), Some(pack_rgb8([0, 0, 255])));
    assert_eq!(s.classic_color(1), Some(pack_rgb8([255, 255, 255])));
    s.set_classic_color(1, None);
    assert_eq!(s.states[slot].classic, 0);
    s.set_classic_color(1, Some([1, 2, 3]));
    s.flush(true);
    s.remove(1);
    s.flush(true);
    assert!(!s.objects.contains_key(&1));
    assert_eq!(s.states[slot].classic, 0, "a freed slot forgets it");
}
