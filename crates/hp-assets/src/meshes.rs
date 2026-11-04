use glam::{DVec3, Vec3};
use hp_editor::SolidNode;
use hp_render::{BlendVertex, MarkTri, MaterialId, MeshBatch, ObjectMesh, Vertex, WalkClass};

fn f3(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

fn batch_index(batches: &mut Vec<MeshBatch>, material: MaterialId) -> usize {
    if let Some(i) = batches.iter().position(|b| b.material == material) {
        return i;
    }
    batches.push(MeshBatch {
        material,
        vertices: Vec::new(),
        indices: Vec::new(),
        blend: Vec::new(),
        cubemaps: Vec::new(),
    });
    batches.len() - 1
}

#[cfg(test)]
pub(crate) fn solid_mesh(
    node: &SolidNode,
    material_of: &dyn Fn(&str) -> MaterialId,
    wire: [u8; 3],
    walk: bool,
) -> ObjectMesh {
    solid_mesh_by_side(node, &|side: &hp_vmf::Side| material_of(&side.material), wire, walk)
}

pub(crate) type FaceCubemaps<'a> = &'a dyn Fn(&hp_vmf::Side, &hp_geom::FaceGeom) -> u16;

fn walk_class(w: hp_geom::DispWalk) -> WalkClass {
    match w {
        hp_geom::DispWalk::Buildable => WalkClass::Buildable,
        hp_geom::DispWalk::Walkable => WalkClass::Walkable,
        hp_geom::DispWalk::Steep => WalkClass::Steep,
    }
}

fn walk_classes(info: &hp_vmf::DispInfo, d: &hp_geom::DispMesh) -> Vec<WalkClass> {
    use hp_app::settings::DispWalkMode;
    let display = hp_app::runtime::render::render_display();
    let limit = f64::from(display.disp_walk_limit) / 100.0;
    match display.disp_walk_mode {
        DispWalkMode::Source => hp_geom::disp_walk(info, d).into_iter().map(walk_class).collect(),
        DispWalkMode::Limit => hp_geom::triangle_up_angles(&d.positions, &d.indices)
            .into_iter()
            .map(|a| if a <= limit { WalkClass::Buildable } else { WalkClass::Steep })
            .collect(),
        DispWalkMode::Gradient => hp_geom::triangle_up_angles(&d.positions, &d.indices)
            .into_iter()
            .map(|a| WalkClass::GRADIENT[hp_geom::slope_band(a, limit).min(WalkClass::GRADIENT.len() - 1)])
            .collect(),
    }
}

fn disp_grid_lines(positions: &[DVec3], n: usize, outline: &mut Vec<[f32; 3]>, detail: &mut Vec<[f32; 3]>) {
    let last = n.saturating_sub(1);
    for r in 0..n {
        let out = if r == 0 || r == last { &mut *outline } else { &mut *detail };
        for c in 0..last {
            for (a, b) in [(r * n + c, r * n + c + 1), (c * n + r, (c + 1) * n + r)] {
                if let (Some(pa), Some(pb)) = (positions.get(a), positions.get(b)) {
                    out.push(f3(*pa));
                    out.push(f3(*pb));
                }
            }
        }
    }
}

const SEAM_QUANT: f64 = 8.0;

fn seam_key(p: DVec3) -> [i64; 3] {
    let q = (p * SEAM_QUANT).round();
    [q.x as i64, q.y as i64, q.z as i64]
}

fn grid_border(n: usize) -> impl Iterator<Item = usize> {
    let last = n.saturating_sub(1);
    (0..n * n).filter(move |k| {
        let (i, j) = (k / n.max(1), k % n.max(1));
        i == 0 || j == 0 || i == last || j == last
    })
}

#[derive(Default)]
pub(crate) struct SeamNormals {
    sums: ahash::AHashMap<[i64; 3], DVec3>,
}

impl SeamNormals {
    pub(crate) fn add(&mut self, mesh: &hp_geom::DispMesh) {
        for k in grid_border(mesh.row_len) {
            let (Some(p), Some(n)) = (mesh.positions.get(k), mesh.normals.get(k)) else { continue };
            if p.is_finite() && n.is_finite() {
                *self.sums.entry(seam_key(*p)).or_insert(DVec3::ZERO) += *n;
            }
        }
    }

    fn normal(&self, p: DVec3, own: DVec3) -> DVec3 {
        self.sums.get(&seam_key(p)).and_then(|s| s.try_normalize()).unwrap_or(own)
    }
}

#[cfg(test)]
pub(crate) fn solid_mesh_by_side(
    node: &SolidNode,
    material_of: &dyn Fn(&hp_vmf::Side) -> MaterialId,
    wire: [u8; 3],
    walk: bool,
) -> ObjectMesh {
    solid_mesh_seams(node, material_of, wire, walk, None, None)
}

pub(crate) fn solid_mesh_seams(
    node: &SolidNode,
    material_of: &dyn Fn(&hp_vmf::Side) -> MaterialId,
    wire: [u8; 3],
    walk: bool,
    seams: Option<&SeamNormals>,
    cubes: Option<FaceCubemaps<'_>>,
) -> ObjectMesh {
    let solid = &node.solid;
    let has_disp = node.disp.iter().any(Option::is_some);
    let mut batches: Vec<MeshBatch> = Vec::with_capacity(2);
    let mut edges: Vec<[f32; 3]> = Vec::new();
    let mut detail_edges: Vec<[f32; 3]> = Vec::new();
    let mut source_edges: Vec<[f32; 3]> = Vec::new();
    let mut marks: Vec<MarkTri> = Vec::new();
    for face in &node.geom.faces {
        let Some(side) = solid.sides.get(face.side_index) else { continue };
        let cube = cubes.map(|c| c(side, face));
        if has_disp {
            let Some(d) = node.disp_mesh(face.side_index) else { continue };
            let bi = batch_index(&mut batches, material_of(side));
            let Some(b) = batches.get_mut(bi) else { continue };
            let base = u32::try_from(b.vertices.len()).unwrap_or(u32::MAX);
            if let Some(mb) = side.disp.as_deref().and_then(hp_vmf::multiblend::MultiBlend::of) {
                b.blend.resize(b.vertices.len(), BlendVertex::default());
                b.blend.extend((0..d.positions.len()).map(|i| multiblend_vertex(&mb, i)));
            }
            let row = d.row_len.max(1);
            let last = row - 1;
            for (i, p) in d.positions.iter().enumerate() {
                let flat = d.base_positions.get(i).copied().unwrap_or(*p);
                let uv = hp_geom::texel_uv(side, flat);
                let mut n = d.normals.get(i).copied().unwrap_or(d.face_normal);
                let (r, c) = (i / row, i % row);
                if let Some(s) = seams
                    && (r == 0 || c == 0 || r == last || c == last)
                {
                    n = s.normal(*p, n);
                }
                let alpha = d.alphas.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
                b.vertices.push(Vertex { pos: f3(*p), normal: f3(n), uv: [uv.x as f32, uv.y as f32], alpha });
            }
            if let Some(cube) = cube {
                fill_cubemaps(b, base as usize, cube);
            }
            b.indices.extend(d.indices.iter().map(|i| base.saturating_add(*i)));
            disp_grid_lines(&d.positions, d.row_len, &mut edges, &mut detail_edges);
            if walk && let Some(info) = side.disp.as_deref() {
                let classes = walk_classes(info, d);
                marks.reserve(classes.len());
                for (t, w) in d.indices.chunks_exact(3).zip(classes) {
                    let p = |k: u32| d.positions.get(k as usize).map_or([0.0; 3], |p| f3(*p));
                    marks.push(MarkTri { p: [p(t[0]), p(t[1]), p(t[2])], class: w });
                }
            }
            continue;
        }
        if face.vertices.len() < 3 {
            continue;
        }
        let bi = batch_index(&mut batches, material_of(side));
        let Some(b) = batches.get_mut(bi) else { continue };
        let base = u32::try_from(b.vertices.len()).unwrap_or(u32::MAX);
        let normal = f3(face.normal());
        for &p in &face.vertices {
            let uv = hp_geom::texel_uv(side, p);
            b.vertices.push(Vertex { pos: f3(p), normal, uv: [uv.x as f32, uv.y as f32], alpha: 0.0 });
        }
        if let Some(cube) = cube {
            fill_cubemaps(b, base as usize, cube);
        }
        hp_geom::triangulate_fan_into(base, face.vertices.len(), &mut b.indices);
    }
    let e = node.geom.edges();
    let out = if has_disp { &mut source_edges } else { &mut edges };
    out.reserve(e.len() * 2);
    for [a, b] in e {
        out.push(f3(a));
        out.push(f3(b));
    }
    let (aabb_min, aabb_max) =
        if node.bounds.is_empty() { ([0.0; 3], [0.0; 3]) } else { (f3(node.bounds.min), f3(node.bounds.max)) };
    ObjectMesh { batches, edges, detail_edges, source_edges, marks, aabb_min, aabb_max, wire_color: wire }
}

fn fill_cubemaps(b: &mut MeshBatch, first: usize, cube: u16) {
    b.cubemaps.resize(first, hp_render::NO_CUBEMAP);
    b.cubemaps.resize(b.vertices.len(), cube);
}

pub(crate) fn assign_placed_faces(batches: &mut [MeshBatch], assign: &crate::cubemaps::CubeAssign) {
    for b in batches {
        if b.cubemaps.len() != b.vertices.len() {
            b.cubemaps.clear();
            continue;
        }
        let mut start = 0;
        while start < b.vertices.len() {
            let number = b.cubemaps[start];
            let mut end = start + 1;
            while end < b.vertices.len() && b.cubemaps[end] == number {
                end += 1;
            }
            let run = &b.vertices[start..end];
            let n = run.len() as f64;
            let center = run.iter().map(|v| DVec3::from(v.pos.map(f64::from))).sum::<DVec3>() / n;
            let normal = run.iter().map(|v| DVec3::from(v.normal.map(f64::from))).sum::<DVec3>().normalize_or_zero();
            let cube = assign.face(u32::MAX, center, normal);
            b.cubemaps[start..end].fill(cube);
            start = end;
        }
    }
}

pub(crate) fn face_numbers() -> impl Fn(&hp_vmf::Side, &hp_geom::FaceGeom) -> u16 {
    let next = std::cell::Cell::new(0u16);
    move |_, _| {
        let n = next.get();
        next.set(if n >= hp_render::NO_CUBEMAP - 1 { 0 } else { n + 1 });
        n
    }
}

pub(crate) fn box_mesh(min: DVec3, max: DVec3, material: MaterialId, faces: bool, wire: [u8; 3]) -> ObjectMesh {
    let (lo, hi) = (min.as_vec3(), max.as_vec3());
    let corner = |i: usize| {
        Vec3::new(
            if i & 1 != 0 { hi.x } else { lo.x },
            if i & 2 != 0 { hi.y } else { lo.y },
            if i & 4 != 0 { hi.z } else { lo.z },
        )
        .to_array()
    };
    let mut batches = Vec::new();
    if faces {
        const FACES: [([usize; 4], [f32; 3]); 6] = [
            ([0, 2, 6, 4], [-1.0, 0.0, 0.0]),
            ([1, 5, 7, 3], [1.0, 0.0, 0.0]),
            ([0, 4, 5, 1], [0.0, -1.0, 0.0]),
            ([2, 3, 7, 6], [0.0, 1.0, 0.0]),
            ([0, 1, 3, 2], [0.0, 0.0, -1.0]),
            ([4, 6, 7, 5], [0.0, 0.0, 1.0]),
        ];
        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);
        for (quad, normal) in FACES {
            let base = vertices.len() as u32;
            for (k, c) in quad.iter().enumerate() {
                let uv = [if k == 1 || k == 2 { 1.0 } else { 0.0 }, if k >= 2 { 1.0 } else { 0.0 }];
                vertices.push(Vertex { pos: corner(*c), normal, uv, alpha: 0.0 });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        batches.push(MeshBatch { material, vertices, indices, blend: Vec::new(), cubemaps: Vec::new() });
    }
    const EDGES: [(usize, usize); 12] =
        [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];
    let mut edges = Vec::with_capacity(24);
    for (a, b) in EDGES {
        edges.push(corner(a));
        edges.push(corner(b));
    }
    ObjectMesh {
        batches,
        edges,
        aabb_min: lo.min(hi).to_array(),
        aabb_max: lo.max(hi).to_array(),
        wire_color: wire,
        ..Default::default()
    }
}

fn multiblend_vertex(mb: &hp_vmf::multiblend::MultiBlend, i: usize) -> BlendVertex {
    let Some(w) = mb.weights.get(i) else { return BlendVertex::default() };
    BlendVertex::painted(w.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) as f32 } else { 0.0 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> hp_editor::Document {
        let map = hp_vmf::parse(hp_app::testing::TEST_VMF).unwrap_or_else(|_| hp_vmf::Map::new_empty());
        hp_editor::Document::from_map(map)
    }

    #[test]
    fn brush_mesh() {
        let d = doc();
        let node = d.scene().solid(2).unwrap();
        let mesh = solid_mesh(
            node,
            &|m| if m.eq_ignore_ascii_case("DEV/DEV_MEASUREGENERIC01B") { 7 } else { 1 },
            [1, 2, 3],
            false,
        );
        assert_eq!(mesh.batches.len(), 1);
        let b = &mesh.batches[0];
        assert_eq!(b.material, 7);
        assert_eq!(b.vertices.len(), 24);
        assert_eq!(b.indices.len(), 36);
        assert_eq!(mesh.edges.len(), 24, "12 unique edges");
        assert_eq!(mesh.aabb_min, [-256.0, -256.0, -16.0]);
        assert_eq!(mesh.aabb_max, [256.0, 256.0, 0.0]);
        assert_eq!(mesh.wire_color, [1, 2, 3]);
        let top = b.vertices.iter().find(|v| v.normal == [0.0, 0.0, 1.0] && v.pos[0] == 256.0).unwrap();
        assert_eq!(top.uv[0], 1024.0);
    }

    #[test]
    fn displacement_source_brush_and_seam_normals() {
        let mut d = doc();
        let mut ids = Vec::new();
        for (lo, hi) in [(-256.0, 0.0), (0.0, 256.0)] {
            let params = hp_geom::PrimitiveParams {
                bounds: hp_geom::Aabb::new(DVec3::new(lo, -128.0, 300.0), DVec3::new(hi, 128.0, 316.0)),
                ..Default::default()
            };
            ids.push(d.create_primitive(&params, hp_editor::Owner::World).expect("block")[0]);
        }
        let top = |d: &hp_editor::Document, id| {
            let n = d.scene().solid(id).unwrap();
            let f = n.geom.faces.iter().find(|f| f.normal().z > 0.9).unwrap();
            hp_editor::FaceRef::new(id, f.side_index)
        };
        let faces: Vec<_> = ids.iter().map(|id| top(&d, *id)).collect();
        assert_eq!(d.create_displacements(&faces, 2).ok(), Some(2));
        for (i, f) in faces.iter().enumerate() {
            let base = d.scene().solid(f.solid).unwrap().disp_mesh(f.side).unwrap().base_positions.clone();
            d.edit_solid(f.solid, |s| {
                let disp = s.sides[f.side].disp.as_mut().unwrap();
                for (k, b) in base.iter().enumerate() {
                    disp.normals[k] = DVec3::Z;
                    disp.distances[k] = if i == 0 { (b.x + 256.0) / 8.0 } else { 32.0 };
                }
            })
            .unwrap();
        }
        let nodes: Vec<&SolidNode> = ids.iter().map(|id| d.scene().solid(*id).unwrap()).collect();
        let plain = solid_mesh(nodes[0], &|_| 3, [0, 0, 0], false);
        assert_eq!(plain.source_edges.len(), 24, "the 12 edges of the source block");
        assert_eq!(plain.edges.len(), 16 * 2, "the outline of the displacement");
        let mut seams = SeamNormals::default();
        for n in &nodes {
            for m in n.disp.iter().flatten() {
                seams.add(m);
            }
        }
        let seam_normal = |mesh: &ObjectMesh| -> Vec<[f32; 3]> {
            let mut v: Vec<_> = mesh.batches[0].vertices.iter().filter(|v| v.pos[0] == 0.0).map(|v| v.normal).collect();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v
        };
        let mats = |_: &hp_vmf::Side| 3;
        let left = solid_mesh_seams(nodes[0], &mats, [0; 3], false, Some(&seams), None);
        let right = solid_mesh_seams(nodes[1], &mats, [0; 3], false, Some(&seams), None);
        assert_eq!(seam_normal(&left).len(), 5);
        assert_eq!(seam_normal(&left), seam_normal(&right), "one normal per seam vertex");
        let right_alone = solid_mesh(nodes[1], &|_| 3, [0, 0, 0], false);
        assert_ne!(seam_normal(&left), seam_normal(&right_alone), "without the neighbours they differ");
        let inner = |m: &ObjectMesh| {
            m.batches[0].vertices.iter().find(|v| v.pos[0] == 128.0 && v.pos[1] == 0.0).map(|v| v.normal)
        };
        assert_eq!(inner(&right), inner(&right_alone));
    }

    #[test]
    fn displacement_mesh() {
        let mut d = doc();
        let face = hp_editor::FaceRef::new(2, 0);
        assert_eq!(d.create_displacements(&[face], 2).ok(), Some(1));
        let node = d.scene().solid(2).unwrap();
        let mesh = solid_mesh(node, &|_| 3, [0, 0, 0], false);
        assert_eq!(mesh.batches.len(), 1);
        assert_eq!(mesh.batches[0].vertices.len(), 25, "only the displacement surface is drawn");
        assert_eq!(mesh.batches[0].indices.len(), 32 * 3);
        assert_eq!(mesh.edges.len(), 16 * 2, "outline");
        assert_eq!(mesh.detail_edges.len(), 24 * 2, "inner grid lines");
        assert!(mesh.marks.is_empty(), "no walkability marks unless asked");
        assert!(mesh.batches[0].blend.is_empty(), "no 4-way blend data");
        let (lo, hi) = (mesh.aabb_min, mesh.aabb_max);
        let on_border = |p: &[f32; 3]| p[0] == lo[0] || p[0] == hi[0] || p[1] == lo[1] || p[1] == hi[1];
        assert!(mesh.edges.iter().all(on_border));
        for pair in mesh.detail_edges.chunks_exact(2) {
            assert!(!(on_border(&pair[0]) && on_border(&pair[1])), "inner line {pair:?}");
        }
        let walk = solid_mesh_by_side(node, &|_| 3, [0, 0, 0], true);
        assert_eq!(walk.marks.len(), 32);
        assert!(walk.marks.iter().all(|m| m.class == WalkClass::Buildable));
        assert_eq!(walk.edges, mesh.edges);

        let mut node = node.clone();
        let disp = node.solid.sides[0].disp.as_deref_mut().expect("displacement");
        let mut mb = hp_vmf::multiblend::MultiBlend::new(25);
        mb.weights[3] = [0.0, 2.0, 0.0, 2.0];
        mb.colors[1][3] = [1.0, 0.0, 0.0];
        mb.weights[4] = [f64::NAN, -1.0, 0.0, 0.0];
        assert!(mb.write_to(disp));
        let mesh = solid_mesh(&node, &|_| 3, [0, 0, 0], false);
        let blend = &mesh.batches[0].blend;
        assert_eq!(blend.len(), 25);
        assert_eq!(blend[0], BlendVertex::painted([1.0, 0.0, 0.0, 0.0]));
        assert_eq!(blend[3], BlendVertex::painted([0.0, 1.0, 0.0, 1.0]), "raw amounts, like the engine reads them");
        assert_eq!(blend[4], BlendVertex::painted([0.0, 0.0, 0.0, 0.0]), "hostile weights show texture 1");
    }

    #[test]
    fn faces_carry_their_cubemaps() {
        let d = doc();
        let node = d.scene().solid(2).unwrap();
        let plain = solid_mesh(node, &|_| 3, [0, 0, 0], false);
        assert!(plain.batches[0].cubemaps.is_empty(), "no cubemap asked: the default one");
        let cubes = |_: &hp_vmf::Side, f: &hp_geom::FaceGeom| if f.normal().z > 0.5 { 1 } else { 2 };
        let mesh = solid_mesh_seams(node, &|_| 3, [0; 3], false, None, Some(&cubes));
        let b = &mesh.batches[0];
        assert_eq!(b.cubemaps.len(), b.vertices.len());
        for (v, c) in b.vertices.iter().zip(&b.cubemaps) {
            assert_eq!(*c, if v.normal[2] > 0.5 { 1 } else { 2 });
        }
        let numbers = face_numbers();
        let mut placed = solid_mesh_seams(node, &|_| 3, [0; 3], false, None, Some(&numbers));
        let runs = placed.batches[0].cubemaps.windows(2).filter(|w| w[0] != w[1]).count() + 1;
        assert_eq!(runs, 6, "one number per face");
        let assign = crate::cubemaps::CubeAssign::for_tests(&[([0.0, 0.0, 500.0], 4), ([0.0, 0.0, -500.0], 5)]);
        assign_placed_faces(&mut placed.batches, &assign);
        let b = &placed.batches[0];
        for (v, c) in b.vertices.iter().zip(&b.cubemaps) {
            if v.normal[2] > 0.5 {
                assert_eq!(*c, 4, "the top faces the probe above");
            } else if v.normal[2] < -0.5 {
                assert_eq!(*c, 5, "the bottom faces the probe below");
            }
        }
    }

    #[test]
    fn boxes() {
        let m = box_mesh(DVec3::splat(-8.0), DVec3::splat(8.0), 5, true, [9, 9, 9]);
        assert_eq!(m.batches[0].vertices.len(), 24);
        assert_eq!(m.batches[0].indices.len(), 36);
        assert_eq!(m.edges.len(), 24);
        for v in &m.batches[0].vertices {
            let n = Vec3::from(v.normal);
            assert!(Vec3::from(v.pos).dot(n) > 0.0, "normals point out");
        }
        let e = box_mesh(DVec3::ZERO, DVec3::ONE, 5, false, [0; 3]);
        assert!(e.batches.is_empty());
        assert_eq!(e.aabb_max, [1.0; 3]);
    }
}
