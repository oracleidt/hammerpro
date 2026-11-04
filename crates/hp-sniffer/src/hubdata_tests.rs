use super::*;
use crate::hub::{self, Slot2, layout};
use crate::hubdec::{X64, X86};
use crate::memmap::{CLeaf, MemoryCm};
use crate::procmem::VecMem;

fn offset_of(ptr: usize, which: Slot2) -> usize {
    layout(ptr).0.iter().find(|(s, _)| *s == which).map(|(_, o)| *o).expect("slot")
}

#[test]
fn the_world_structure_layout_follows_the_natural_alignment_of_the_engine_source() {
    let o = |s| offset_of(8, s);
    assert_eq!(o(Slot2::Count(Field::Submodels)), 0);
    assert_eq!(o(Slot2::Count(Field::Planes)), 4);
    assert_eq!(o(Slot2::Ptr(Field::Planes)), 0x08);
    assert_eq!(o(Slot2::Ptr(Field::Leafs)), 0x18);
    assert_eq!(o(Slot2::Ptr(Field::Vertexes)), 0x38);
    assert_eq!(o(Slot2::Count(Field::VertNormalIndices)), 0x70);
    assert_eq!(o(Slot2::Ptr(Field::Nodes)), 0x98);
    assert_eq!(o(Slot2::Ptr(Field::LeafMinDist)), 0xa0);
    assert_eq!(o(Slot2::Ptr(Field::TexInfo)), 0xb0);
    assert_eq!(o(Slot2::Ptr(Field::TexData)), 0xc0);
    assert_eq!(o(Slot2::Ptr(Field::DispInfos)), 0xd0);
    assert_eq!(o(Slot2::Count(Field::VertIndices)), 0x104);
    assert_eq!(o(Slot2::Ptr(Field::VertIndices)), 0x108);
    assert_eq!(o(Slot2::Ptr(Field::MarkSurfaces)), 0x118);
    assert_eq!(o(Slot2::Ptr(Field::WorldLights)), 0x130);
    assert_eq!(o(Slot2::Ptr(Field::Primitives)), 0x148);
    assert_eq!(o(Slot2::Ptr(Field::Areas)), 0x178);
    assert_eq!(o(Slot2::Ptr(Field::Cubemaps)), 0x1a0);
    assert_eq!(o(Slot2::Count(Field::Cubemaps)), 0x1a8);
    assert_eq!(o(Slot2::Ptr(Field::AmbientIndex)), 0x1b8);
    let p = |s| offset_of(4, s);
    assert_eq!(p(Slot2::Ptr(Field::Planes)), 0x08);
    assert_eq!(p(Slot2::Ptr(Field::Leafs)), 0x10);
    assert_eq!(p(Slot2::Ptr(Field::Vertexes)), 0x20);
}

struct Mini {
    mem: VecMem,
    cm: MemoryCm,
    hub_at: u64,
}

fn put_ptr(raw: &mut [u8], at: usize, ptr: usize, v: u64) {
    if ptr == 8 {
        raw[at..at + 8].copy_from_slice(&v.to_le_bytes());
    } else {
        raw[at..at + 4].copy_from_slice(&(v as u32).to_le_bytes());
    }
}

fn mini_world(ptr: usize) -> Mini {
    let p = if ptr == 8 { X64 } else { X86 };
    let (planes_at, leafs_at, nodes_at, s1_at, s2_at, sl_at, vi_at, vx_at, ti_at, td_at, str_at, hub_at, light_at) = (
        0x10_0000u64,
        0x11_0000u64,
        0x12_0000u64,
        0x13_0000u64,
        0x14_0000u64,
        0x15_0000u64,
        0x16_0000u64,
        0x17_0000u64,
        0x18_0000u64,
        0x19_0000u64,
        0x1a_0000u64,
        0x1b_0000u64,
        0x1c_0000u64,
    );
    let mut mem = VecMem::default();
    let mut planes = Vec::new();
    for i in 0..4u32 {
        for v in [0.0f32, 0.0, 1.0, 16.0 * (i + 1) as f32] {
            planes.extend_from_slice(&v.to_le_bytes());
        }
        planes.extend_from_slice(&[2, 0, 0, 0]);
    }
    mem.add(planes_at, planes);
    let mut leafs = vec![0u8; p.leaf_stride * 3];
    for (i, (contents, cluster)) in [(1i32, -1i16), (0, 0), (0, 1)].into_iter().enumerate() {
        let r = &mut leafs[i * p.leaf_stride..(i + 1) * p.leaf_stride];
        r[0..4].copy_from_slice(&contents.to_le_bytes());
        r[p.leaf_cluster..p.leaf_cluster + 2].copy_from_slice(&cluster.to_le_bytes());
        r[p.leaf_water..p.leaf_water + 2].copy_from_slice(&(-1i16).to_le_bytes());
    }
    mem.add(leafs_at, leafs);
    let mut nodes = vec![0u8; p.node_stride * 2];
    for i in 0..2usize {
        let r = &mut nodes[i * p.node_stride..(i + 1) * p.node_stride];
        put_ptr(r, p.node_plane, ptr, planes_at + 20 * i as u64);
        put_ptr(r, p.node_child, ptr, leafs_at + (1 + i) as u64 * p.leaf_stride as u64);
        put_ptr(r, p.node_child + ptr, ptr, leafs_at);
    }
    mem.add(nodes_at, nodes);
    let mut a1 = vec![0u8; p.s1_stride * 2];
    a1[p.s1_prims..p.s1_prims + 2].copy_from_slice(&0u16.to_le_bytes());
    mem.add(s1_at, a1);
    let mut a2 = vec![0u8; p.s2_stride * 2];
    for i in 0..2usize {
        let r = &mut a2[i * p.s2_stride..(i + 1) * p.s2_stride];
        r[p.s2_flags..p.s2_flags + 2].copy_from_slice(&0x2u16.to_le_bytes());
        r[p.s2_count] = 4;
        put_ptr(r, p.s2_plane, ptr, planes_at + 20 * i as u64);
        r[p.s2_first..p.s2_first + 4].copy_from_slice(&((i * 4) as i32).to_le_bytes());
        r[p.s2_texinfo..p.s2_texinfo + 2].copy_from_slice(&(((i as u16) << 1) | 1).to_le_bytes());
    }
    mem.add(s2_at, a2);
    let mut al = vec![0u8; p.sl_stride * 2];
    for i in 0..2usize {
        let r = &mut al[i * p.sl_stride..(i + 1) * p.sl_stride];
        r[4..6].copy_from_slice(&1i16.to_le_bytes());
        r[6..8].copy_from_slice(&1i16.to_le_bytes());
        r[p.sl_styles..p.sl_styles + 4].copy_from_slice(&[0, 255, 255, 255]);
        put_ptr(r, p.sl_samples, ptr, light_at + 16 * i as u64);
    }
    mem.add(sl_at, al);
    mem.add(light_at, (0..64u8).collect());
    let mut vi = Vec::new();
    for i in 0..8u16 {
        vi.extend_from_slice(&i.to_le_bytes());
    }
    mem.add(vi_at, vi);
    let mut vx = Vec::new();
    for (x, y) in [(0.0f32, 0.0f32), (16.0, 0.0), (16.0, 16.0), (0.0, 16.0)].into_iter().cycle().take(8) {
        for v in [x, y, 16.0] {
            vx.extend_from_slice(&v.to_le_bytes());
        }
    }
    mem.add(vx_at, vx);
    let mut ti = vec![0u8; p.texinfo_stride * 2];
    for i in 0..2usize {
        let r = &mut ti[i * p.texinfo_stride..(i + 1) * p.texinfo_stride];
        r[0..4].copy_from_slice(&1.0f32.to_le_bytes());
        r[p.texinfo_flags..p.texinfo_flags + 4].copy_from_slice(&0x1_0080i32.to_le_bytes());
        put_ptr(r, p.texinfo_material, ptr, 0x20_0000 + 0x100 * i as u64);
    }
    mem.add(ti_at, ti);
    let mut td = vec![0u8; p.texdata_stride * 2];
    put_ptr(&mut td, 0, ptr, str_at);
    put_ptr(&mut td, p.texdata_stride, ptr, str_at + 6);
    mem.add(td_at, td);
    mem.add(str_at, b"WALL\0AB\0".to_vec());

    let (offsets, total) = layout(ptr);
    let mut hub = vec![0u8; total];
    for (slot, off) in &offsets {
        match slot {
            Slot2::Count(f) => {
                let v: i32 = match f {
                    Field::Submodels => 1,
                    Field::Planes => 4,
                    Field::Leafs => 3,
                    Field::Nodes => 2,
                    Field::Surfaces => 2,
                    Field::TexInfo => 2,
                    Field::TexData => 2,
                    Field::VertIndices => 8,
                    Field::Vertexes => 8,
                    _ => 0,
                };
                hub[*off..*off + 4].copy_from_slice(&v.to_le_bytes());
            }
            Slot2::Ptr(f) => {
                let v = match f {
                    Field::Planes => planes_at,
                    Field::Leafs => leafs_at,
                    Field::Nodes => nodes_at,
                    Field::Surfaces => s1_at,
                    Field::TexInfo => ti_at,
                    Field::TexData => td_at,
                    Field::VertIndices => vi_at,
                    Field::Vertexes => vx_at,
                    _ => 0,
                };
                put_ptr(&mut hub, *off, ptr, v);
            }
            Slot2::Named(n) => {
                let v = match *n {
                    hub::SURFACES1 => s1_at,
                    hub::SURFACES2 => s2_at,
                    hub::SURFACE_LIGHTING => sl_at,
                    hub::LIGHTDATA => light_at,
                    _ => 0,
                };
                put_ptr(&mut hub, *off, ptr, v);
            }
        }
    }
    mem.add(hub_at, hub);
    let cm = MemoryCm {
        cm: crate::cm::CmFindings { planes: 4, planes_addr: planes_at, ..crate::cm::CmFindings::default() },
        leafs: vec![
            CLeaf { contents: 7, cluster: 9, area_flags: 0, first_brush: 0, num_brushes: 0 },
            CLeaf { contents: 1, cluster: -1, area_flags: 0, first_brush: 0, num_brushes: 0 },
            CLeaf { contents: 0, cluster: 0, area_flags: 0, first_brush: 0, num_brushes: 2 },
            CLeaf { contents: 0, cluster: 1, area_flags: 0, first_brush: 2, num_brushes: 1 },
            CLeaf { contents: 0, cluster: 5, area_flags: 0, first_brush: 3, num_brushes: 0 },
        ],
        ..MemoryCm::default()
    };
    Mini { mem, cm, hub_at }
}

#[test]
fn the_world_structure_is_found_from_the_plane_array_in_both_layouts() {
    for ptr in [8usize, 4] {
        let w = mini_world(ptr);
        let (hub, data) = capture(&w.mem, &w.cm, &[ptr], true).expect("hub");
        assert_eq!(hub.base, w.hub_at, "{ptr}");
        assert_eq!(hub.count(Field::Leafs), 3);
        assert_eq!(hub.count(Field::Surfaces), 2);
        assert_eq!(data.profile, if ptr == 8 { "x86-64" } else { "x86" });
        assert_eq!(data.cm_leaf_drop, 1, "a false leaf in front of the collision leaves is dropped");
        assert_eq!(data.faces.len(), 2);
        assert_eq!(data.vertindices.len(), 8);
        assert_eq!(data.piece(5).map(|p| p.bytes.len()), Some(64));
        assert_eq!(data.piece(10).map(|p| p.bytes.len()), Some(96));
        assert_eq!(data.piece(3).map(|p| p.bytes.len()), Some(96));
        assert_eq!(data.piece(58).map(|p| p.bytes.len()), Some(112));
        assert_eq!(data.piece(47).map(|p| p.bytes.clone()), Some(vec![0xff; 4]));
        assert_eq!(data.piece(47).map(|p| p.fidelity), Some(Fidelity::Equivalent));
        let light = data.piece(53).expect("lighting");
        assert_eq!(light.bytes.len(), 16 + 16, "the length follows from the last face with samples");
        assert_eq!(light.bytes[16], 16);
        assert_eq!(light.fidelity, Fidelity::Exact);
        let ldr = capture(&w.mem, &w.cm, &[ptr], false).expect("ldr").1;
        assert!(ldr.piece(8).is_some() && ldr.piece(7).is_some() && ldr.piece(53).is_none());
    }
}

#[test]
fn another_pointer_width_is_not_mistaken_for_the_structure() {
    let w = mini_world(8);
    assert!(capture(&w.mem, &w.cm, &[4], true).is_none());
    let mut other = mini_world(8);
    other.cm.cm.planes = 5;
    assert!(capture(&other.mem, &other.cm, &[8], true).is_none(), "the plane count has to agree");
}

#[test]
fn texture_infos_keep_the_engine_flags_without_the_extra_half() {
    let w = mini_world(8);
    let (hub, _) = capture(&w.mem, &w.cm, &[8], true).expect("hub");
    let t = read_texinfo(&w.mem, &hub, &X64).expect("texinfo");
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].flags, 0x80);
    assert_eq!(t[1].material, 0x20_0100);
    assert_eq!(t[0].vecs[0], 1.0);
}

#[test]
fn leaf_lists_of_the_collision_world_are_aligned_on_the_render_leaves() {
    let cm = |c: &[(i32, i16)]| -> Vec<CLeaf> {
        c.iter()
            .map(|&(contents, cluster)| CLeaf { contents, cluster, area_flags: 0x401, first_brush: 0, num_brushes: 0 })
            .collect()
    };
    let leaf = |contents, cluster| MemLeaf {
        contents,
        area: 1,
        flags: 2,
        center: [0.0; 3],
        half: [0.0; 3],
        cluster,
        water: -1,
        first_mark: 0,
        num_mark: 0,
    };
    let leafs = vec![leaf(1, -1), leaf(0, 0), leaf(0, 3)];
    assert_eq!(
        align_cm_leafs(&cm(&[(1, -1), (0, 0), (0, 3), (0, -1)]), &leafs),
        Some(0),
        "an extra leaf at the end is not an offset"
    );
    assert_eq!(align_cm_leafs(&cm(&[(9, 9), (1, -1), (0, 0), (0, 3)]), &leafs), Some(1));
    assert_eq!(align_cm_leafs(&cm(&[(1, -1), (0, 5), (0, 3)]), &leafs), None);
    assert_eq!(align_cm_leafs(&[], &leafs), None);
}

#[test]
fn edges_are_shared_between_faces_and_unused_entries_stay_valid() {
    let face = |first: i32| Face { first, count: 3, ..Face::default() };
    let vi: Vec<u16> = vec![0, 1, 2, 2, 1, 3, 7, 8];
    let (edges, surf) = edge_lumps(&vi, &[face(0), face(3)]);
    let surf: Vec<i32> = surf.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    assert_eq!(surf.len(), 8);
    assert_eq!(surf[..3], [1, 2, 3]);
    assert_eq!(surf[3], -2, "the second face walks the shared edge the other way round");
    assert!(surf[6] > 0 && surf[7] > 0, "entries no face uses still get an edge");
    let e: Vec<(u16, u16)> =
        edges.chunks_exact(4).map(|c| (u16::from_le_bytes([c[0], c[1]]), u16::from_le_bytes([c[2], c[3]]))).collect();
    assert_eq!(e[0], (0, 0));
    assert_eq!(e[surf[0] as usize], (0, 1));
    for (k, s) in surf.iter().take(6).enumerate() {
        let (a, b) = e[s.unsigned_abs() as usize];
        let (from, to) = if *s < 0 { (b, a) } else { (a, b) };
        let base = if k < 3 { 0 } else { 3 };
        assert_eq!(from, vi[k]);
        assert_eq!(to, vi[base + (k - base + 1) % 3]);
    }
}

#[test]
fn a_texture_data_lump_carries_the_material_values() {
    let infos = vec![Some(MaterialInfo { reflectivity: [0.25, 0.5, 0.75], width: 512, height: 256 }), None];
    let lump = texdata_lump(2, &infos);
    assert_eq!(lump.len(), 64);
    assert_eq!(f32::from_le_bytes([lump[4], lump[5], lump[6], lump[7]]), 0.5);
    assert_eq!(i32::from_le_bytes([lump[12], lump[13], lump[14], lump[15]]), 0);
    assert_eq!(i32::from_le_bytes([lump[16], lump[17], lump[18], lump[19]]), 512);
    assert_eq!(i32::from_le_bytes([lump[20], lump[21], lump[22], lump[23]]), 256);
    assert_eq!(i32::from_le_bytes([lump[32 + 12], lump[32 + 13], lump[32 + 14], lump[32 + 15]]), 1);
    assert_eq!(&lump[32..44], &[0u8; 12], "a material nobody references has no values");
}

#[test]
fn the_material_of_a_texture_info_gives_its_reflectivity_and_size() {
    let p = X64;
    let ptr = 0x30_0000u64;
    let tex = 0x40_0000u64;
    let mut mem = VecMem::default();
    let mut mat = vec![0u8; 0x100];
    mat[0x80 - 0x18..0x80 - 0x18 + 12]
        .copy_from_slice(&[0.1f32, 0.2, 0.3].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>());
    mat[0x80 - 32..0x80 - 24].copy_from_slice(&tex.to_le_bytes());
    mem.add(ptr - 0x80, mat);
    let mut t = vec![0u8; 0x80];
    t[40..42].copy_from_slice(&512u16.to_le_bytes());
    t[42..44].copy_from_slice(&256u16.to_le_bytes());
    mem.add(tex, t);
    let m = read_material_info(&mem, ptr, &p).expect("material");
    assert_eq!(m.reflectivity, [0.1, 0.2, 0.3]);
    assert_eq!((m.width, m.height), (512, 256));
    let mut odd = VecMem { blocks: mem.blocks.clone(), ..Default::default() };
    odd.blocks[1].1[40..42].copy_from_slice(&500u16.to_le_bytes());
    let m = read_material_info(&odd, ptr, &p).expect("material");
    assert_eq!((m.width, m.height), (0, 0), "a size that is not a power of two is not trusted");
    let unknown = Profile { material_reflectivity_at: None, ..X86 };
    assert!(read_material_info(&mem, ptr, &unknown).is_none(), "an unknown layout gives nothing");
}

#[test]
fn the_report_lists_every_lump_with_its_fidelity() {
    let w = mini_world(8);
    let (_, data) = capture(&w.mem, &w.cm, &[8], true).expect("hub");
    let r = data.report();
    assert!(r.found && r.hdr);
    assert!(r.lumps.windows(2).all(|p| p[0].lump <= p[1].lump));
    let text = r.render().join("\n");
    assert!(text.contains("engine world structure recognized (x86-64 layout, HDR lighting)"), "{text}");
    assert!(text.contains("nodes"));
    assert!(HubReport::default().render()[0].contains("not recognized"));
}

#[test]
fn the_lumps_are_grouped_into_source_rows() {
    let w = mini_world(8);
    let (_, data) = capture(&w.mem, &w.cm, &[8], true).expect("hub");
    let rows = data.report().source_rows();
    let find = |c: &str| rows.iter().find(|r| r.category == c).cloned();
    let lighting = find("lighting, lightmaps, ambient light and world lights").expect("lighting");
    assert_eq!(lighting.source, "memory");
    assert!(lighting.detail.contains("exact"), "{}", lighting.detail);
    let tools = find("lumps only the compilers and tools read").expect("not kept");
    assert_eq!(tools.source, "absent");
    assert!(tools.detail.contains("originalfaces"), "{}", tools.detail);
    assert!(HubReport::default().source_rows().is_empty());
}
