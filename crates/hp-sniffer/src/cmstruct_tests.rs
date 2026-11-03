use super::*;
use crate::procmem::VecMem;

fn put(raw: &mut [u8], at: usize, ptr: usize, v: u64) {
    if ptr == 8 {
        raw[at..at + 8].copy_from_slice(&v.to_le_bytes());
    } else {
        raw[at..at + 4].copy_from_slice(&(v as u32).to_le_bytes());
    }
}

struct Built {
    mem: VecMem,
    entities: Vec<(u64, usize)>,
    planes_at: u64,
}

fn build(ptr: usize) -> Built {
    let stride = pair_stride(ptr);
    let to_ptr = align(4, ptr);
    let planes_at = 0x20_0000u64;
    let nodes_at = 0x21_0000u64;
    let leafs_at = 0x22_0000u64;
    let lb_at = 0x23_0000u64;
    let models_at = 0x24_0000u64;
    let brushes_at = 0x25_0000u64;
    let sides_at = 0x26_0000u64;
    let boxes_at = 0x27_0000u64;
    let ents_at = 0x28_0000u64;
    let vis_at = 0x29_0000u64;
    let struct_at = 0x30_0000u64;
    let mut mem = VecMem::default();

    let mut planes = Vec::new();
    for i in 0..8u32 {
        let (n, kind): ([f32; 3], u8) = match i % 6 {
            0 => ([1.0, 0.0, 0.0], 0),
            1 => ([-1.0, 0.0, 0.0], 0),
            2 => ([0.0, 1.0, 0.0], 1),
            3 => ([0.0, -1.0, 0.0], 1),
            4 => ([0.0, 0.0, 1.0], 2),
            _ => ([0.0, 0.0, -1.0], 2),
        };
        for v in n {
            planes.extend_from_slice(&v.to_le_bytes());
        }
        planes.extend_from_slice(&(8.0 * (i + 1) as f32).to_le_bytes());
        planes.extend_from_slice(&[kind, 0, 0, 0]);
    }
    mem.add(planes_at, planes);

    let nrec = node_record(ptr);
    let mut nodes = vec![0u8; nrec * 2];
    put(&mut nodes, 0, ptr, planes_at);
    nodes[ptr..ptr + 4].copy_from_slice(&1i32.to_le_bytes());
    nodes[ptr + 4..ptr + 8].copy_from_slice(&(-1i32).to_le_bytes());
    put(&mut nodes, nrec, ptr, planes_at + 20);
    nodes[nrec + ptr..nrec + ptr + 4].copy_from_slice(&(-2i32).to_le_bytes());
    nodes[nrec + ptr + 4..nrec + ptr + 8].copy_from_slice(&(-3i32).to_le_bytes());
    mem.add(nodes_at, nodes);

    let mut leafs = Vec::new();
    for (contents, cluster, first, num) in [(1i32, -1i16, 0u16, 0u16), (0, 0, 0, 2), (0, 1, 2, 1), (0, 0, 3, 0)] {
        leafs.extend_from_slice(&contents.to_le_bytes());
        leafs.extend_from_slice(&cluster.to_le_bytes());
        leafs.extend_from_slice(&0u16.to_le_bytes());
        leafs.extend_from_slice(&first.to_le_bytes());
        leafs.extend_from_slice(&num.to_le_bytes());
        leafs.extend_from_slice(&0u32.to_le_bytes());
    }
    mem.add(leafs_at, leafs);
    mem.add(lb_at, [0u16, 1, 2].iter().flat_map(|v| v.to_le_bytes()).collect());

    let mrec = if ptr == 8 { 72 } else { 56 };
    let mut models = vec![0u8; mrec];
    for (k, v) in [-64.0f32, -64.0, -64.0, 64.0, 64.0, 64.0].iter().enumerate() {
        models[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    models[36..40].copy_from_slice(&5i32.to_le_bytes());
    mem.add(models_at, models);

    let mut brushes = Vec::new();
    for (contents, numsides, index) in [(1i32, -1i32, 0i32), (1, 4, 0)] {
        brushes.extend_from_slice(&contents.to_le_bytes());
        brushes.extend_from_slice(&numsides.to_le_bytes());
        brushes.extend_from_slice(&index.to_le_bytes());
    }
    mem.add(brushes_at, brushes);

    let srec = side_record(ptr);
    let mut sides = vec![0u8; srec * 4];
    for k in 0..4usize {
        put(&mut sides, k * srec, ptr, planes_at + 20 * k as u64);
        sides[k * srec + ptr..k * srec + ptr + 2].copy_from_slice(&3u16.to_le_bytes());
    }
    mem.add(sides_at, sides);
    let mut boxes = vec![0u8; 48];
    boxes[0..4].copy_from_slice(&0.0f32.to_le_bytes());
    boxes[16..20].copy_from_slice(&32.0f32.to_le_bytes());
    boxes[20..24].copy_from_slice(&32.0f32.to_le_bytes());
    boxes[24..28].copy_from_slice(&32.0f32.to_le_bytes());
    mem.add(boxes_at, boxes);
    let text = b"{\n\"classname\" \"worldspawn\"\n}\n\0".to_vec();
    let text_len = text.len();
    mem.add(ents_at, text);
    mem.add(vis_at, vec![7u8; 32]);

    let mut s = vec![0u8; 20 * stride + 64];
    let mut off = 0usize;
    let pair = |s: &mut Vec<u8>, off: &mut usize, count: i32, addr: u64| {
        s[*off..*off + 4].copy_from_slice(&count.to_le_bytes());
        put(s, *off + to_ptr, ptr, addr);
        *off += stride;
    };
    pair(&mut s, &mut off, 4, sides_at);
    pair(&mut s, &mut off, 1, boxes_at);
    pair(&mut s, &mut off, 8, planes_at);
    pair(&mut s, &mut off, 2, nodes_at);
    pair(&mut s, &mut off, 4, leafs_at);
    s[off..off + 4].copy_from_slice(&3i32.to_le_bytes());
    off += 8;
    pair(&mut s, &mut off, 3, lb_at);
    pair(&mut s, &mut off, 1, models_at);
    pair(&mut s, &mut off, 2, brushes_at);
    pair(&mut s, &mut off, 0, 0x2a_0000);
    pair(&mut s, &mut off, 32, vis_at);
    s[off..off + 4].copy_from_slice(&(text_len as i32).to_le_bytes());
    off += 4;
    s[off..off + 4].copy_from_slice(&(text_len as i32).to_le_bytes());
    off += 4;
    put(&mut s, align(off, ptr), ptr, ents_at);
    mem.add(struct_at, s);
    Built { mem, entities: vec![(ents_at, text_len - 1)], planes_at }
}

#[test]
fn the_collision_structure_is_found_from_the_entity_text_and_from_the_planes_in_both_layouts() {
    for ptr in [8usize, 4] {
        let b = build(ptr);
        let by_text = find_by_entities(&b.mem, &b.entities, &[ptr]).expect("by entities");
        let by_planes = find_by_planes(&b.mem, b.planes_at, 8, &[ptr]).expect("by planes");
        assert_eq!(by_text, by_planes, "{ptr}");
        assert_eq!((by_text.planes.count, by_text.nodes.count, by_text.leafs.count), (8, 2, 4));
        assert_eq!((by_text.empty_leaf, by_text.cmodels.count, by_text.brushes.count, by_text.vis.count), (3, 1, 2, 32));
        assert!(find_by_entities(&b.mem, &b.entities, &[if ptr == 8 { 4 } else { 8 }]).is_none(), "the other width does not fit");
        assert!(find_by_planes(&b.mem, b.planes_at, 9, &[ptr]).is_none(), "the plane count has to agree");
        assert!(find_by_entities(&b.mem, &[(0x28_0000, 5000)], &[ptr]).is_none(), "the text length has to agree");
    }
}

#[test]
fn the_collision_world_is_read_from_the_structure_in_both_layouts() {
    for ptr in [8usize, 4] {
        let b = build(ptr);
        let cs = find_by_planes(&b.mem, b.planes_at, 8, &[ptr]).expect("structure");
        let stride = if ptr == 8 { 72 } else { 56 };
        let mc = read_memory_cm(&b.mem, &cs, stride).expect("collision world");
        assert_eq!(mc.cm.brushes.len(), 2);
        assert_eq!(mc.cm.sides.len(), 4);
        assert_eq!(mc.cm.sides[2].plane, 2);
        assert_eq!(mc.cm.sides[2].surface, 3);
        assert_eq!(mc.cm.boxes.len(), 1);
        assert_eq!(mc.cm.boxes[0].maxs, [32.0; 3]);
        assert_eq!(mc.leafs.len(), 3, "the extra empty leaf of the engine is not a leaf of the map");
        assert_eq!(mc.leafs[1].num_brushes, 2);
        assert_eq!(mc.leafbrushes, vec![0, 1, 2]);
        assert_eq!(mc.nodes.len(), 2);
        assert_eq!(mc.nodes[0].children, [1, -1]);
        assert_eq!(mc.nodes[1].plane, 1);
        assert_eq!(mc.models.len(), 1);
        assert_eq!(mc.models[0].headnode, 5);
        assert_eq!(mc.models[0].maxs, [64.0; 3]);
        assert_eq!(mc.plane_bytes.len(), 8 * 20);
        let mut broken = cs.clone();
        broken.cmodels.count = 0;
        assert!(read_memory_cm(&b.mem, &broken, stride).is_none());
    }
}

#[test]
fn record_sizes_follow_the_pointer_width() {
    assert_eq!((side_record(8), side_record(4)), (16, 8));
    assert_eq!((node_record(8), node_record(4)), (16, 12));
    assert_eq!((pair_stride(8), pair_stride(4)), (16, 8));
}
