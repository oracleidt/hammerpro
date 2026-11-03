use super::*;
use crate::hub::{Array, Field, Hub};
use crate::procmem::VecMem;

fn put_f32s(out: &mut Vec<u8>, v: &[f32]) {
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
}

fn hub_with(arrays: &[(Field, i64, u64)]) -> Hub {
    let mut h = Hub { ptr: 8, ..Hub::default() };
    for (f, c, a) in arrays {
        h.arrays.insert(*f, Array { count: *c, addr: *a });
    }
    h
}

#[test]
fn a_node_record_becomes_a_bsp_node() {
    let p = X64;
    let planes_addr = 0x1000u64;
    let nodes_addr = 0x2000u64;
    let leafs_addr = 0x3000u64;
    let mut nodes = vec![0u8; p.node_stride * 2];
    let specs = [
        (3u64, nodes_addr + p.node_stride as u64, leafs_addr),
        (5, leafs_addr + p.leaf_stride as u64, leafs_addr),
    ];
    for (i, (plane, c0, c1)) in specs.into_iter().enumerate() {
        let r = &mut nodes[i * p.node_stride..(i + 1) * p.node_stride];
        r[p.node_plane..p.node_plane + 8].copy_from_slice(&(planes_addr + plane * 20).to_le_bytes());
        r[p.node_child..p.node_child + 8].copy_from_slice(&c0.to_le_bytes());
        r[p.node_child + 8..p.node_child + 16].copy_from_slice(&c1.to_le_bytes());
        r[p.node_center..p.node_center + 4].copy_from_slice(&100.0f32.to_le_bytes());
        r[p.node_half..p.node_half + 4].copy_from_slice(&28.0f32.to_le_bytes());
        r[p.node_first_surface..p.node_first_surface + 2].copy_from_slice(&7u16.to_le_bytes());
        r[p.node_first_surface + 2..p.node_first_surface + 4].copy_from_slice(&2u16.to_le_bytes());
        r[p.node_area..p.node_area + 2].copy_from_slice(&1i16.to_le_bytes());
    }
    let mut mem = VecMem::default();
    mem.add(nodes_addr, nodes);
    let hub = hub_with(&[(Field::Planes, 10, planes_addr), (Field::Nodes, 2, nodes_addr), (Field::Leafs, 4, leafs_addr)]);
    let lump = node_lump(&mem, &hub, &p).expect("nodes");
    assert_eq!(lump.len(), 64);
    assert_eq!(i32_at(&lump, 0), 3);
    assert_eq!(i32_at(&lump, 4), 1, "a child inside the node array is a node index");
    assert_eq!(i32_at(&lump, 8), -1, "the first leaf is -1");
    assert_eq!(i32_at(&lump, 32 + 4), -2, "the second leaf is -2");
    assert_eq!(i16_at(&lump, 12), 72);
    assert_eq!(i16_at(&lump, 18), 128);
    assert_eq!(u16_at(&lump, 24), 7);
    assert_eq!(u16_at(&lump, 26), 2);
    assert_eq!(i16_at(&lump, 28), 1);
}

#[test]
fn a_leaf_keeps_its_bounds_flags_and_the_solid_leaf_rule() {
    let plain = |contents, first_mark, num_mark| MemLeaf {
        contents,
        area: 1,
        flags: 0,
        center: [0.0; 3],
        half: [0.0; 3],
        cluster: -1,
        water: -1,
        first_mark,
        num_mark,
    };
    let leafs = vec![
        plain(1, 55, 0),
        MemLeaf {
            contents: 0,
            area: 3,
            flags: 2,
            center: [-100.0, 50.0, 8.0],
            half: [10.0, 20.0, 30.0],
            cluster: 4,
            water: -1,
            first_mark: 9,
            num_mark: 2,
        },
        plain(0, 77, 0),
    ];
    let lump = leaf_lump(&leafs, &[(0, 0), (5, 3), (8, 0)]);
    assert_eq!(lump.len(), 96);
    assert_eq!(u16_at(&lump, 20), 0, "a solid leaf without faces starts at zero");
    let l = &lump[32..64];
    assert_eq!(i32_at(l, 0), 0);
    assert_eq!(i16_at(l, 4), 4);
    assert_eq!(u16_at(l, 6), 3 | (2 << 9));
    assert_eq!((i16_at(l, 8), i16_at(l, 10), i16_at(l, 12)), (-110, 30, -22));
    assert_eq!((i16_at(l, 14), i16_at(l, 16), i16_at(l, 18)), (-90, 70, 38));
    assert_eq!((u16_at(l, 20), u16_at(l, 22)), (9, 2));
    assert_eq!((u16_at(l, 24), u16_at(l, 26)), (5, 3));
    assert_eq!(u16_at(&lump, 64 + 20), 77, "an empty but not solid leaf keeps the value of the engine");
}

fn face_record(p: &Profile, plane_ptr: u64, flags: u16, count: u8, first: i32, texinfo: u16, disp: u64) -> Vec<u8> {
    let mut r = vec![0u8; p.s2_stride];
    r[p.s2_flags..p.s2_flags + 2].copy_from_slice(&flags.to_le_bytes());
    r[p.s2_count] = count;
    r[p.s2_plane..p.s2_plane + 8].copy_from_slice(&plane_ptr.to_le_bytes());
    r[p.s2_first..p.s2_first + 4].copy_from_slice(&first.to_le_bytes());
    r[p.s2_texinfo..p.s2_texinfo + 2].copy_from_slice(&((texinfo << 1) | 1).to_le_bytes());
    r[p.s2_disp..p.s2_disp + 8].copy_from_slice(&disp.to_le_bytes());
    r
}

#[test]
fn faces_are_decoded_from_the_four_surface_arrays() {
    let p = X64;
    let (planes, s1, s2, sl, light) = (0x1000u64, 0x2000u64, 0x3000u64, 0x4000u64, 0x10_0000u64);
    let mut a1 = vec![0u8; 32];
    a1[p.s1_prims..p.s1_prims + 2].copy_from_slice(&1u16.to_le_bytes());
    a1[p.s1_prims + 2..p.s1_prims + 4].copy_from_slice(&9u16.to_le_bytes());
    let mut a2 = face_record(&p, planes + 20 * 2, 0x40 | 0x2 | 0x8, 4, 12, 33, 0);
    a2.extend(face_record(&p, planes + 20, 0, 3, 16, 5, 0xdead));
    let mut al = vec![0u8; 80];
    al[0..2].copy_from_slice(&(-8i16).to_le_bytes());
    al[2..4].copy_from_slice(&(-16i16).to_le_bytes());
    al[4..6].copy_from_slice(&3i16.to_le_bytes());
    al[6..8].copy_from_slice(&1i16.to_le_bytes());
    al[p.sl_styles..p.sl_styles + 4].copy_from_slice(&[0, 33, 255, 255]);
    al[p.sl_samples..p.sl_samples + 8].copy_from_slice(&(light + 8).to_le_bytes());
    al[p.sl_stride + p.sl_styles..p.sl_stride + p.sl_styles + 4].copy_from_slice(&[255, 255, 255, 255]);
    let mut mem = VecMem::default();
    mem.add(s1, a1);
    mem.add(s2, a2);
    mem.add(sl, al);
    let mut hub = hub_with(&[(Field::Planes, 4, planes), (Field::Surfaces, 2, s1)]);
    hub.surfaces1 = s1;
    hub.surfaces2 = s2;
    hub.surfacelighting = sl;
    hub.lightdata = light;
    let faces = read_faces(&mem, &hub, &p).expect("faces");
    assert_eq!(faces.len(), 2);
    let f = &faces[0];
    assert_eq!((f.plane, f.back, f.on_node, f.first, f.count, f.texinfo), (2, true, true, 12, 4, 33));
    assert_eq!((f.mins, f.extents, f.prims, f.first_prim), ([-8, -16], [3, 1], 1, 9));
    assert_eq!(faces[1].disp_ptr, 0xdead);
    assert_eq!(light_bytes(f), 4 * 2 * 2 * 4 * 4, "bump mapped faces keep four sets per style");
    assert_eq!(lighting_len(&faces, light), 8 + light_bytes(f));
    let vi: Vec<u16> = (0..20).collect();
    let vx: Vec<[f32; 3]> = (0..20).map(|i| [i as f32 * 8.0, if i % 4 > 1 { 8.0 } else { 0.0 }, 0.0]).collect();
    let lump = face_lump(&faces, light, &vi, &vx, &HashMap::new());
    assert_eq!(lump.len(), 112);
    assert_eq!(u16_at(&lump, 0), 2);
    assert_eq!(lump[2], 1);
    assert_eq!(lump[3], 1);
    assert_eq!(i32_at(&lump, 4), 12);
    assert_eq!(i16_at(&lump, 8), 4);
    assert_eq!(i16_at(&lump, 10), 33);
    assert_eq!(i16_at(&lump, 12), -1);
    assert_eq!(&lump[16..20], &[0, 33, 255, 255]);
    assert_eq!(i32_at(&lump, 20), 8);
    assert_eq!(i32_at(&lump, 28), -8);
    assert_eq!(i32_at(&lump, 36), 3);
    assert_eq!(i32_at(&lump, 56 + 20), -1, "a face without samples has no light offset");
    assert_eq!(f32::from_le_bytes([lump[56 + 24], lump[56 + 25], lump[56 + 26], lump[56 + 27]]), 1_048_576.0);
}

#[test]
fn leaf_faces_come_from_surface_pointers() {
    let p = X64;
    let (s2, marks) = (0x3000u64, 0x5000u64);
    let mut raw = Vec::new();
    for i in [2u64, 0, 1] {
        raw.extend_from_slice(&(s2 + i * p.s2_stride as u64).to_le_bytes());
    }
    let mut mem = VecMem::default();
    mem.add(marks, raw);
    let mut hub = hub_with(&[(Field::MarkSurfaces, 3, marks), (Field::Surfaces, 3, 0)]);
    hub.surfaces2 = s2;
    let lump = leaf_faces_lump(&mem, &hub, &p).expect("leaf faces");
    assert_eq!(lump, vec![2, 0, 0, 0, 1, 0]);
    hub.arrays.insert(Field::Surfaces, Array { count: 2, addr: 0 });
    assert!(leaf_faces_lump(&mem, &hub, &p).is_none(), "a pointer outside the surface array is refused");
}

#[test]
fn small_tables_are_converted() {
    let p = X64;
    let mut mem = VecMem::default();
    let mut water = Vec::new();
    put_f32s(&mut water, &[-100.0, -200.0]);
    water.extend_from_slice(&[0x10, 0x02, 0x81, 0x02]);
    mem.add(0x100, water);
    let hub = hub_with(&[(Field::LeafWater, 1, 0x100)]);
    let lump = leafwater_lump(&mem, &hub).expect("water");
    assert_eq!(&lump[8..12], &[0x10, 0x02, 0, 0], "the garbage in the padding is cleared");
    let mut prim = vec![0u8; 12];
    prim[0] = 1;
    prim[4..6].copy_from_slice(&7u16.to_le_bytes());
    prim[6..8].copy_from_slice(&18u16.to_le_bytes());
    prim[8..10].copy_from_slice(&3u16.to_le_bytes());
    prim[10..12].copy_from_slice(&4u16.to_le_bytes());
    mem.add(0x200, prim);
    let hub = hub_with(&[(Field::Primitives, 1, 0x200)]);
    let lump = primitive_lump(&mem, &hub, &p).expect("prims");
    assert_eq!(lump, vec![1, 0, 7, 0, 18, 0, 3, 0, 4, 0]);
    let mut amb = Vec::new();
    for (c, f) in [(0u32, 99u32), (16, 4)] {
        amb.extend_from_slice(&c.to_le_bytes());
        amb.extend_from_slice(&f.to_le_bytes());
    }
    mem.add(0x300, amb);
    let hub = hub_with(&[(Field::Leafs, 2, 0), (Field::AmbientIndex, 0, 0x300)]);
    let index = ambient_index_lump(&mem, &hub, &p).expect("ambient");
    assert_eq!(index, vec![0, 0, 99, 0, 16, 0, 4, 0], "the index keeps the engine values");
    assert_eq!(ambient_samples(&index), 16);
}

fn light(intensity: [f32; 3], attn: [f32; 3], radius: f32) -> Vec<u8> {
    let mut r = vec![0u8; 88];
    r[12..16].copy_from_slice(&intensity[0].to_le_bytes());
    r[16..20].copy_from_slice(&intensity[1].to_le_bytes());
    r[20..24].copy_from_slice(&intensity[2].to_le_bytes());
    r[RADIUS_AT..RADIUS_AT + 4].copy_from_slice(&radius.to_le_bytes());
    for (k, a) in attn.iter().enumerate() {
        r[ATTN_AT + k * 4..ATTN_AT + k * 4 + 4].copy_from_slice(&a.to_le_bytes());
    }
    r
}

#[test]
fn the_radius_the_engine_computes_is_cleared_but_a_real_one_stays() {
    let i = [5839.4526, 7053.997, 7843.1377];
    let computed = computed_radius(&light(i, [0.0, 0.0, 1.0], 0.0), true).expect("quadratic");
    assert!((computed - 896.5515).abs() < 0.01, "{computed}");
    let linear = computed_radius(&light([3.3141835, 0.0, 0.0], [0.0, 1.0, 0.0], 0.0), true).expect("linear");
    assert!((linear - 220.9456).abs() < 0.01, "{linear}");
    let both = computed_radius(&light([7358.187, 0.0, 0.0], [0.0, 1.0, 2.0], 0.0), true).expect("both terms");
    assert!((both - 495.0004).abs() < 0.01, "the linear term counts too: {both}");
    let ldr = computed_radius(&light(i, [0.0, 0.0, 1.0], 0.0), false).expect("ldr");
    assert!((ldr - computed / 2.0f32.sqrt()).abs() < 0.01, "the low dynamic range threshold is twice as large: {ldr}");
    let mut data = light(i, [0.0, 0.0, 1.0], computed);
    data.extend(light(i, [0.0, 0.0, 1.0], 512.0));
    data.extend(light([0.3, 0.4, 0.5], [0.0, 0.0, 0.0], DEFAULT_RADIUS));
    data.extend(light(i, [1.0, 0.0, 0.0], DEFAULT_RADIUS));
    let mut mem = VecMem::default();
    mem.add(0x1000, data);
    let hub = hub_with(&[(Field::WorldLights, 4, 0x1000)]);
    let lump = worldlight_lump(&mem, &hub, true).expect("lights");
    let radius = |k: usize| f32_at(&lump, k * 88 + RADIUS_AT);
    assert_eq!(radius(0), 0.0);
    assert_eq!(radius(1), 512.0);
    assert_eq!(radius(2), 0.0);
    assert_eq!(radius(3), 0.0, "a constant only light gets the default radius too");
}

#[test]
fn texture_names_become_the_string_lumps() {
    let p = X64;
    let mut strings = Vec::new();
    let mut recs = Vec::new();
    let base = 0x9000u64;
    for name in ["BRICK/A", "TOOLS/TOOLSNODRAW", "X"] {
        let at = base + strings.len() as u64;
        let mut r = vec![0u8; p.texdata_stride];
        r[0..8].copy_from_slice(&at.to_le_bytes());
        recs.extend(r);
        strings.extend_from_slice(name.as_bytes());
        strings.push(0);
    }
    let mut mem = VecMem::default();
    mem.add(base, strings.clone());
    mem.add(0x7000, recs);
    let hub = hub_with(&[(Field::TexData, 3, 0x7000)]);
    let names = texdata_names(&mem, &hub, &p).expect("names");
    assert_eq!(names.iter().map(|n| n.1.as_str()).collect::<Vec<_>>(), vec!["BRICK/A", "TOOLS/TOOLSNODRAW", "X"]);
    let (data, table) = string_lumps(&mem, &names).expect("lumps");
    assert_eq!(data, strings);
    assert_eq!(table, vec![0, 0, 0, 0, 8, 0, 0, 0, 26, 0, 0, 0]);
}

#[test]
fn only_the_runtime_marks_of_the_root_are_cleared_and_the_marks_in_the_tree_are_kept() {
    let mut body = vec![0u8; 48 + 28 * 3];
    body[32..36].copy_from_slice(&48u32.to_le_bytes());
    body[36..40].copy_from_slice(&9u32.to_le_bytes());
    body[48..52].copy_from_slice(&56i32.to_le_bytes());
    body[48 + 27] = 5;
    body[76 + 27] = 6;
    body[104 + 27] = 7;
    clear_runtime_marks(&mut body);
    assert_eq!(&body[36..40], &[0, 0, 0, 0]);
    assert_eq!((body[48 + 27], body[76 + 27], body[104 + 27]), (0, 6, 7));
    let mut small = vec![7u8; 10];
    clear_runtime_marks(&mut small);
    assert_eq!(small, vec![7u8; 10], "too small to be a collision solid");
}

fn packed_solid() -> Vec<u8> {
    let hex = "00010000060001000c0048000100000000000000000000000000000000000000040004000000000005000000000000000d0008000200000000000000000064656661756c74002c000000000000504347de3ca84a";
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hex")).collect()
}

#[test]
fn the_packed_physics_keys_of_a_brush_model_become_their_text() {
    let text = decode_packed_keys(&packed_solid()).expect("keys");
    assert_eq!(
        text,
        "solid {\n\"index\" \"0\"\n\"mass\" \"50000.000000\"\n\"surfaceprop\" \"default\"\n\"volume\" \"5512815.000000\"\n}\n"
    );
    let mut bad = packed_solid();
    bad[0] = 9;
    assert!(decode_packed_keys(&bad).is_none());
    let mut unknown = packed_solid();
    unknown[12 + 8 * 4 + 4] = 99;
    assert!(decode_packed_keys(&unknown).is_none(), "an unknown key is not guessed");
    let model = PhysModel { index: 3, solids: vec![vec![0; 64]], keys: packed_solid() };
    let k = key_text(&model);
    assert_eq!(k.last(), Some(&0u8));
    assert!(String::from_utf8_lossy(&k).contains("\"mass\" \"50000.000000\""));
    let lost = PhysModel { index: 0, solids: vec![vec![0; 64]; 2], keys: Vec::new() };
    let text = String::from_utf8(key_text(&lost)).expect("text");
    assert!(text.starts_with("staticsolid {\n\"index\" \"0\"\n}\nstaticsolid {\n\"index\" \"1\"\n}\n"));
}

#[test]
fn collision_solids_get_their_header_back() {
    let p = X64;
    let mut body = vec![0u8; 64];
    body[28..32].copy_from_slice(&((64u32 << 8) | 0xfa).to_le_bytes());
    body[0] = 0xaa;
    let mut object = vec![0u8; 40];
    object[8..16].copy_from_slice(&0x6000u64.to_le_bytes());
    object[16..20].copy_from_slice(&1.0f32.to_le_bytes());
    object[20..24].copy_from_slice(&0.5f32.to_le_bytes());
    object[24..28].copy_from_slice(&1.0f32.to_le_bytes());
    let mut mem = VecMem::default();
    mem.add(0x5000, object);
    mem.add(0x6000, body);
    let solid = collide_solid(&mem, 0x5000, &p).expect("solid");
    assert_eq!(&solid[0..4], b"VPHY");
    assert_eq!(u16_at(&solid, 4), 0x100);
    assert_eq!(i32_at(&solid, 8), 64);
    assert_eq!(f32_at(&solid, 16), 0.5);
    assert_eq!(solid.len(), 28 + 64);
    assert_eq!(solid[28], 0xaa);
    let one = PhysModel { index: 0, solids: vec![solid.clone()], keys: packed_solid() };
    let lump = physcollide_lump(&[(one, 84)]);
    assert_eq!(i32_at(&lump, 0), 0);
    assert_eq!(i32_at(&lump, 4) as usize, 4 + solid.len());
    assert_eq!(i32_at(&lump, 12), 1);
    assert_eq!(&lump[lump.len() - 16..], &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn displacement_records_take_the_engine_fields() {
    let mut lump = vec![0u8; 176 * 2];
    lump[176 + 36..176 + 38].copy_from_slice(&9u16.to_le_bytes());
    let mut extras = HashMap::new();
    extras.insert(9usize, DispExtra { contents: 1, sample_start: 440, neighbors: vec![7; 88], allowed: vec![0xff; 40] });
    assert_eq!(patch_dispinfo(&mut lump, &extras), 1);
    assert_eq!(i32_at(&lump, 176 + 32), 1);
    assert_eq!(i32_at(&lump, 176 + 44), 440);
    assert_eq!(lump[176 + 48], 7);
    assert_eq!(lump[176 + 175], 0xff);
    assert_eq!(lump[0..176], [0u8; 176], "a record without an object is untouched");
}

#[test]
fn the_sample_position_vector_is_found_by_its_size() {
    let mut mem = VecMem::default();
    let mut head = Vec::new();
    head.extend_from_slice(&0x100000u64.to_le_bytes());
    head.extend_from_slice(&900u64.to_le_bytes());
    head.extend_from_slice(&900u64.to_le_bytes());
    mem.add(0x4000, head);
    mem.add(0x100000, vec![1u8; 1000]);
    let mut decoy = Vec::new();
    decoy.extend_from_slice(&0x100000u64.to_le_bytes());
    decoy.extend_from_slice(&5_000_000u64.to_le_bytes());
    decoy.extend_from_slice(&5_000_000u64.to_le_bytes());
    mem.add(0x3000, decoy);
    assert_eq!(find_sample_positions(&mem, 800, 20, 8), Some((0x100000, 900)));
    assert_eq!(find_sample_positions(&mem, 100_000, 20, 8), None);
}

#[test]
fn the_cubemap_table_keeps_origins_and_sizes() {
    let p = X64;
    let mut raw = vec![0u8; 64];
    raw[0..4].copy_from_slice(&1280.0f32.to_le_bytes());
    raw[4..8].copy_from_slice(&(-2432.0f32).to_le_bytes());
    raw[8..12].copy_from_slice(&32.0f32.to_le_bytes());
    raw[p.cubemap_size_at..p.cubemap_size_at + 4].copy_from_slice(&6i32.to_le_bytes());
    let mut mem = VecMem::default();
    mem.add(0x100, raw);
    let hub = hub_with(&[(Field::Cubemaps, 2, 0x100)]);
    let lump = cubemap_lump(&mem, &hub, &p).expect("cubemaps");
    assert_eq!(lump.len(), 32);
    assert_eq!((i32_at(&lump, 0), i32_at(&lump, 4), i32_at(&lump, 8), i32_at(&lump, 12)), (1280, -2432, 32, 6));
}

#[test]
fn bounds_survive_the_float_round_trip() {
    let (mn, mx) = bounds_shorts([-3712.0 + 4.0, 100.5, 0.0], [4.0, 0.5, 8.0]);
    assert_eq!((mn, mx), ([-3712, 100, -8], [-3704, 101, 8]));
    assert_eq!(polygon_area(&[[0.0, 0.0, 0.0], [64.0, 0.0, 0.0], [64.0, 32.0, 0.0], [0.0, 32.0, 0.0]]), 2048.0);
    assert_eq!(polygon_area(&[[0.0; 3], [1.0, 0.0, 0.0]]), 0.0);
}

fn pack(items: &[(u16, u16, u16, u16)], strings: &[&str], values: &[u8]) -> Vec<u8> {
    let table = 12 + items.len() * 8;
    let text_at = table + strings.len() * 2;
    let mut text = Vec::new();
    let mut offsets = Vec::new();
    for s in strings {
        offsets.push(text.len() as u16);
        text.extend_from_slice(s.as_bytes());
        text.push(0);
    }
    let values_at = text_at + text.len();
    let mut d = vec![0u8; 12];
    d[0..2].copy_from_slice(&0x100u16.to_le_bytes());
    d[4..6].copy_from_slice(&(items.len() as u16).to_le_bytes());
    d[6..8].copy_from_slice(&(strings.len() as u16).to_le_bytes());
    d[10..12].copy_from_slice(&(values_at as u16).to_le_bytes());
    for (k, a, b, c) in items {
        for v in [k, a, b, c] {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    for o in offsets {
        d.extend_from_slice(&o.to_le_bytes());
    }
    d.extend_from_slice(&text);
    d.extend_from_slice(values);
    d
}

#[test]
fn a_float_is_written_with_six_decimals_rounding_exact_ties_away_from_zero() {
    assert_eq!(fixed6(5328.3515625), "5328.351563");
    assert_eq!(fixed6(0.5), "0.500000");
    assert_eq!(fixed6(-1.25), "-1.250000");
    assert_eq!(fixed6(0.0), "0.000000");
    assert_eq!(fixed6(0.9999995), "1.000000");
    assert_eq!(fixed6(f64::from(27002.146f32)), "27002.146484");
}

#[test]
fn a_custom_key_inside_a_known_block_keeps_its_text_and_a_break_block_stays_on_one_line() {
    let mut values = Vec::new();
    values.extend_from_slice(&0i32.to_le_bytes());
    let d = pack(
        &[(1, 0, 0, 0), (0, 0, 0, 0), (0, 0xffff, 0, 1), (2, 0, 0, 0), (1, 0xffff, 2, 0), (0, 0xffff, 3, 4), (0, 0xffff, 5, 6), (2, 0xffff, 0, 0)],
        &["massbias", "2.000000", "break", "model", "gibs/x", "health", "0"],
        &values,
    );
    assert_eq!(
        decode_packed_keys(&d).expect("keys"),
        "solid {\n\"index\" \"0\"\n\"massbias\" \"2.000000\"\n}\nbreak {\"model\" \"gibs/x\" \"health\" \"0\"  }\n"
    );
}

#[test]
fn the_key_tables_of_every_block_name_the_keys_of_the_physics_library() {
    assert_eq!(key_info(0, 11).map(|k| k.0), Some("inertia"));
    assert_eq!(key_info(0, 13).map(|k| k.0), Some("volume"));
    assert_eq!(key_info(5, 6).map(|k| k.0), Some("massCenterOverride"));
    assert_eq!(key_info(1, 3).map(|k| k.0), Some("surfaceplane"));
    assert_eq!(key_info(2, 10).map(|k| k.0), Some("zfriction"));
    assert_eq!(key_info(3, 4).map(|k| k.0), Some("animfrictiontimehold"));
    assert_eq!(key_info(4, 1).map(|k| k.0), Some("collisionpair"));
    assert!(key_info(0, 14).is_none());
    assert!(key_info(9, 0).is_none());
    assert_eq!(block_name(2), Some("ragdollconstraint"));
    assert_eq!(block_name(4), Some("collisionrules"));
}

#[test]
fn a_ragdoll_constraint_block_is_written_with_its_float_and_int_keys() {
    let mut values = Vec::new();
    values.extend_from_slice(&1i32.to_le_bytes());
    values.extend_from_slice(&(-40.0f32).to_le_bytes());
    let d = pack(&[(1, 2, 0, 0), (0, 2, 0, 0), (0, 2, 8, 4), (2, 2, 0, 0)], &[], &values);
    assert_eq!(decode_packed_keys(&d).expect("keys"), "ragdollconstraint {\n\"parent\" \"1\"\n\"zmin\" \"-40.000000\"\n}\n");
}

#[test]
fn displacement_contents_come_from_the_array_of_records_that_holds_the_start_positions() {
    let starts = [[100.0f32, 200.0, 300.0], [-5.0, 6.0, 7.0], [1.5, 2.5, 3.5]];
    let contents = [0x10000002i32, 1, 0x10000020];
    let stride = 352usize;
    let mut raw = vec![0u8; stride * 3 + 64];
    for k in 0..3 {
        raw[k * stride..k * stride + 4].copy_from_slice(&contents[k].to_le_bytes());
        for (j, v) in starts[k].iter().enumerate() {
            raw[k * stride + 20 + j * 4..k * stride + 24 + j * 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    let mut mem = VecMem::default();
    mem.add(0x40_0000, raw);
    assert_eq!(alt_contents(&mem, &starts), Some(contents.to_vec()));
    let mut wrong = starts;
    wrong[1][0] = 99.0;
    assert_eq!(alt_contents(&mem, &wrong), None, "the second start has to follow at one stride");
    assert_eq!(alt_contents(&mem, &starts[..1]), None, "a single displacement has no stride");
}
