use std::collections::{HashMap, HashSet};

use glam::{DVec3, IVec3};
use hp_editor::SolidNode;
use hp_editor::luxels::{LuxelAxes, LuxelGrid, MAX_BRUSH_LIGHTMAP_DIM, lightmap_scale};
use hp_render::lightmap::{LightmapBatch, LightmapFace, LightmapVertex, face_flags};
use hp_render::{MaterialId, Renderer};

const CELL: f64 = 1024.0;
const FIRST_BLOCK: u32 = 0x8000_0000;

pub(crate) struct GridFace {
    pub material: MaterialId,
    pub scale: f32,
    pub flags: u32,
    pub vertices: Vec<LightmapVertex>,
    pub indices: Vec<u32>,
}

struct SolidGrid {
    cell: IVec3,
    faces: Vec<u32>,
    batches: Vec<(MaterialId, Vec<LightmapVertex>, Vec<u32>)>,
}

#[derive(Default)]
pub(crate) struct EditorGrid {
    built: bool,
    solids: HashMap<u32, SolidGrid>,
    cells: HashMap<IVec3, (u32, HashSet<u32>)>,
    next_block: u32,
    next_face: u32,
    free_faces: Vec<u32>,
    pending: HashSet<u32>,
    content: u64,
}

fn orient(indices: &mut [u32], flat: &[DVec3], normal: DVec3) {
    let mut sum = 0.0;
    for t in indices.chunks_exact(3) {
        let (Some(a), Some(b), Some(c)) = (flat.get(t[0] as usize), flat.get(t[1] as usize), flat.get(t[2] as usize))
        else {
            continue;
        };
        sum += (*b - *a).cross(*c - *a).dot(normal);
    }
    if sum < 0.0 {
        for t in indices.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
    }
}

fn vertex(pos: DVec3, uv: glam::DVec2, lm: glam::DVec2, alpha: f64) -> LightmapVertex {
    LightmapVertex {
        pos: pos.as_vec3().to_array(),
        uv: uv.as_vec2().to_array(),
        lm: lm.as_vec2().to_array(),
        alpha: alpha as f32,
        face: 0,
        blend: 0,
    }
}

pub(crate) fn solid_faces(node: &SolidNode, lit: &mut dyn FnMut(&str) -> Option<MaterialId>) -> Vec<GridFace> {
    let has_disp = node.disp.iter().any(Option::is_some);
    let mut out = Vec::new();
    for face in &node.geom.faces {
        if face.vertices.len() < 3 {
            continue;
        }
        let disp = node.disp_mesh(face.side_index);
        if has_disp && disp.is_none() {
            continue;
        }
        let Some(side) = node.solid.sides.get(face.side_index) else { continue };
        let Some(material) = lit(&side.material) else { continue };
        let Some(axes) = LuxelAxes::for_side(side) else { continue };
        let Some(grid) = LuxelGrid::from_points(&axes, &face.vertices) else { continue };
        let normal = face.normal();
        let lm = |p: DVec3| grid.local(axes.coords(p));
        let (vertices, mut indices, flat, flags) = match disp {
            Some(d) => {
                if d.positions.len() != d.base_positions.len() || d.row_len < 2 {
                    continue;
                }
                let vertices: Vec<LightmapVertex> = d
                    .positions
                    .iter()
                    .zip(&d.base_positions)
                    .enumerate()
                    .map(|(i, (p, b))| {
                        let p = if p.is_finite() { *p } else { *b };
                        let alpha = d.alphas.get(i).copied().unwrap_or(0.0);
                        vertex(p, hp_geom::texel_uv(side, *b), lm(*b), alpha)
                    })
                    .collect();
                let power = (d.row_len - 1).trailing_zeros();
                let flags = face_flags::DISP | ((power << face_flags::POWER_SHIFT) & face_flags::POWER_MASK);
                (vertices, d.indices.clone(), d.base_positions.clone(), flags)
            }
            None => {
                let vertices: Vec<LightmapVertex> =
                    face.vertices.iter().map(|p| vertex(*p, hp_geom::texel_uv(side, *p), lm(*p), 0.0)).collect();
                let n = face.vertices.len() as u32;
                let indices: Vec<u32> = (1..n - 1).flat_map(|i| [0, i, i + 1]).collect();
                let wide = grid.size[0].max(grid.size[1]) > MAX_BRUSH_LIGHTMAP_DIM;
                (vertices, indices, face.vertices.clone(), if wide { face_flags::WIDE } else { 0 })
            }
        };
        let count = vertices.len() as u32;
        if indices.iter().any(|i| *i >= count) {
            continue;
        }
        orient(&mut indices, &flat, normal);
        out.push(GridFace { material, scale: lightmap_scale(side) as f32, flags, vertices, indices });
    }
    out
}

fn cell_of(node: &SolidNode) -> IVec3 {
    let c = (node.bounds.min + node.bounds.max) * 0.5;
    if c.is_finite() { (c / CELL).floor().as_ivec3() } else { IVec3::ZERO }
}

impl EditorGrid {
    #[cfg(test)]
    pub(crate) fn is_built(&self) -> bool {
        self.built
    }

    pub(crate) fn content_changed(&mut self, r: &mut Renderer, content: u64) {
        if self.built && self.content != content {
            self.clear(r);
        }
        self.content = content;
    }

    pub(crate) fn note_changes(&mut self, changed: Vec<u32>) {
        if self.built {
            self.pending.extend(changed);
        }
    }

    pub(crate) fn forget(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn clear(&mut self, r: &mut Renderer) {
        if !self.built {
            return;
        }
        let faces: Vec<u32> = self.solids.values().flat_map(|s| s.faces.iter().copied()).collect();
        r.remove_lightmap_faces(&faces);
        for (block, _) in self.cells.values() {
            r.remove_lightmap_block(*block);
        }
        let content = self.content;
        self.forget();
        self.content = content;
    }

    fn alloc_face(&mut self) -> u32 {
        if let Some(i) = self.free_faces.pop() {
            return i;
        }
        let i = self.next_face;
        self.next_face += 1;
        i
    }

    fn drop_solid(&mut self, r: &mut Renderer, id: u32, touched: &mut HashSet<IVec3>) {
        let Some(old) = self.solids.remove(&id) else { return };
        r.remove_lightmap_faces(&old.faces);
        self.free_faces.extend(old.faces);
        if let Some((_, members)) = self.cells.get_mut(&old.cell) {
            members.remove(&id);
        }
        touched.insert(old.cell);
    }

    fn put_solid(
        &mut self,
        r: &mut Renderer,
        id: u32,
        node: &SolidNode,
        faces: Vec<GridFace>,
        touched: &mut HashSet<IVec3>,
    ) {
        if faces.is_empty() {
            return;
        }
        let generation = r.lightmap_generation(id);
        let mut list: Vec<(u32, LightmapFace)> = Vec::with_capacity(faces.len());
        let mut by_material: HashMap<MaterialId, (Vec<LightmapVertex>, Vec<u32>)> = HashMap::new();
        for f in faces {
            let index = self.alloc_face();
            list.push((index, LightmapFace { object: id, generation, scale: f.scale, flags: f.flags }));
            let (verts, idx) = by_material.entry(f.material).or_default();
            let base = verts.len() as u32;
            verts.extend(f.vertices.into_iter().map(|v| LightmapVertex { face: index, ..v }));
            idx.extend(f.indices.into_iter().map(|i| base + i));
        }
        r.add_lightmap_faces(&list);
        let cell = cell_of(node);
        let next = &mut self.next_block;
        let entry = self.cells.entry(cell).or_insert_with(|| {
            let block = FIRST_BLOCK + *next;
            *next += 1;
            (block, HashSet::new())
        });
        entry.1.insert(id);
        touched.insert(cell);
        let mut batches: Vec<(MaterialId, Vec<LightmapVertex>, Vec<u32>)> =
            by_material.into_iter().map(|(m, (v, i))| (m, v, i)).collect();
        batches.sort_by_key(|b| b.0);
        self.solids.insert(id, SolidGrid { cell, faces: list.into_iter().map(|(i, _)| i).collect(), batches });
    }

    fn upload_cells(&mut self, r: &mut Renderer, touched: HashSet<IVec3>) {
        for cell in touched {
            let Some((block, members)) = self.cells.get(&cell) else { continue };
            let block = *block;
            let mut ids: Vec<u32> = members.iter().copied().collect();
            ids.sort_unstable();
            let mut merged: HashMap<MaterialId, (Vec<LightmapVertex>, Vec<u32>)> = HashMap::new();
            for id in ids {
                let Some(s) = self.solids.get(&id) else { continue };
                for (m, v, i) in &s.batches {
                    let (verts, idx) = merged.entry(*m).or_default();
                    let base = verts.len() as u32;
                    verts.extend_from_slice(v);
                    idx.extend(i.iter().map(|k| base + k));
                }
            }
            if merged.is_empty() {
                r.remove_lightmap_block(block);
                self.cells.remove(&cell);
                continue;
            }
            let mut keys: Vec<MaterialId> = merged.keys().copied().collect();
            keys.sort_unstable();
            let batches: Vec<LightmapBatch<'_>> = keys
                .iter()
                .filter_map(|m| {
                    let (v, i) = merged.get(m)?;
                    Some(LightmapBatch { material: *m, vertices: v, indices: i })
                })
                .collect();
            r.add_lightmap_block(block, &batches);
        }
    }

    pub(crate) fn sync(
        &mut self,
        r: &mut Renderer,
        solids: &hp_editor::IdMap<SolidNode>,
        lit: &mut dyn FnMut(&str) -> Option<MaterialId>,
    ) {
        let ids: Vec<u32> = if self.built {
            let mut v: Vec<u32> = self.pending.drain().collect();
            v.sort_unstable();
            v
        } else {
            self.built = true;
            self.pending.clear();
            let mut v: Vec<u32> = solids.keys().copied().collect();
            v.sort_unstable();
            v
        };
        if ids.is_empty() {
            return;
        }
        let mut touched = HashSet::new();
        for id in ids {
            self.drop_solid(r, id, &mut touched);
            let Some(node) = solids.get(&id) else { continue };
            if r.object_flags(id).is_none() {
                continue;
            }
            let faces = solid_faces(node, lit);
            self.put_solid(r, id, node, faces, &mut touched);
        }
        self.upload_cells(r, touched);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(scale: i32, material: &str, disp: bool) -> hp_editor::Document {
        let (lo, hi) = (-64.0, 64.0);
        let planes = [
            format!("({lo} {hi} {hi}) ({hi} {hi} {hi}) ({hi} {lo} {hi})"),
            format!("({lo} {lo} {lo}) ({hi} {lo} {lo}) ({hi} {hi} {lo})"),
            format!("({lo} {hi} {hi}) ({lo} {lo} {hi}) ({lo} {lo} {lo})"),
            format!("({hi} {hi} {lo}) ({hi} {lo} {lo}) ({hi} {lo} {hi})"),
            format!("({hi} {hi} {hi}) ({lo} {hi} {hi}) ({lo} {hi} {lo})"),
            format!("({hi} {lo} {lo}) ({lo} {lo} {lo}) ({lo} {lo} {hi})"),
        ];
        let axes = [
            ("[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
            ("[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
            ("[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
            ("[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
            ("[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
            ("[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
        ];
        let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\" solid { \"id\" \"2\" ");
        for (k, (plane, (u, v))) in planes.iter().zip(axes).enumerate() {
            let info = if disp && k == 0 {
                format!("dispinfo {{ \"power\" \"2\" \"startposition\" \"[{lo} {lo} {hi}]\" }}")
            } else {
                String::new()
            };
            text += &format!(
                "side {{ \"id\" \"{}\" \"plane\" \"{plane}\" \"material\" \"{material}\" \"uaxis\" \"{u}\" \"vaxis\" \"{v}\" \"lightmapscale\" \"{scale}\" {info} }} ",
                10 + k
            );
        }
        text += "} }";
        hp_editor::Document::from_map(hp_vmf::parse(&text).expect("parse"))
    }

    fn grid_faces(scale: i32, material: &str, disp: bool, lit: bool) -> (hp_editor::Document, Vec<GridFace>) {
        let d = doc(scale, material, disp);
        let faces = solid_faces(d.scene().solid(2).expect("solid"), &mut |_| lit.then_some(0));
        (d, faces)
    }

    #[test]
    fn cells_are_one_luxel_wide() {
        let (_, faces) = grid_faces(16, "dev/dev_measuregeneric01b", false, true);
        assert_eq!(faces.len(), 6);
        for f in &faces {
            assert_eq!(f.scale, 16.0);
            let lo = f.vertices.iter().fold([f32::MAX; 2], |a, v| [a[0].min(v.lm[0]), a[1].min(v.lm[1])]);
            let hi = f.vertices.iter().fold([f32::MIN; 2], |a, v| [a[0].max(v.lm[0]), a[1].max(v.lm[1])]);
            assert!(lo[0] >= -1e-3 && lo[1] >= -1e-3, "{lo:?}");
            assert!((hi[0] - lo[0] - 8.0).abs() < 1e-3 && (hi[1] - lo[1] - 8.0).abs() < 1e-3, "{lo:?} {hi:?}");
        }
        let (_, fine) = grid_faces(2, "dev/dev_measuregeneric01b", false, true);
        assert!(fine.iter().all(|f| f.scale == 2.0 && f.flags & face_flags::WIDE != 0));
    }

    #[test]
    fn unlit_faces_are_skipped() {
        assert!(grid_faces(16, "tools/toolsnodraw", false, false).1.is_empty());
    }

    #[test]
    fn triangles_face_outward() {
        let (d, list) = grid_faces(16, "dev/dev_measuregeneric01b", false, true);
        let node = d.scene().solid(2).expect("solid");
        for (f, g) in list.iter().zip(&node.geom.faces) {
            for t in f.indices.chunks_exact(3) {
                let p = |i: u32| DVec3::from(f.vertices[i as usize].pos.map(f64::from));
                let n = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
                assert!(n.dot(g.normal()) > 0.0);
            }
        }
    }

    #[test]
    fn displacements_keep_the_luxels_of_their_base_face() {
        let (_, list) = grid_faces(16, "dev/dev_measuregeneric01b", true, true);
        assert_eq!(list.len(), 1);
        let f = &list[0];
        assert_eq!(f.vertices.len(), 25);
        assert_eq!(f.flags & face_flags::DISP, face_flags::DISP);
        assert_eq!((f.flags & face_flags::POWER_MASK) >> face_flags::POWER_SHIFT, 2);
        let span = f.vertices.iter().map(|v| v.lm[0]).fold(f32::MIN, f32::max)
            - f.vertices.iter().map(|v| v.lm[0]).fold(f32::MAX, f32::min);
        assert!((span - 8.0).abs() < 1e-3, "{span}");
    }

    fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .ok()?;
        if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
            return None;
        }
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hammerpro lightmap grid test"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    }

    fn top_mesh() -> hp_render::ObjectMesh {
        let corners = [[-64.0, -64.0, 64.0], [64.0, -64.0, 64.0], [64.0, 64.0, 64.0], [-64.0, 64.0, 64.0]];
        let vertices = corners
            .iter()
            .map(|p| hp_render::Vertex { pos: *p, normal: [0.0, 0.0, 1.0], uv: [p[0], p[1]], alpha: 0.0 })
            .collect();
        hp_render::ObjectMesh {
            batches: vec![hp_render::MeshBatch {
                material: 0,
                vertices,
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: Vec::new(),
            }],
            aabb_min: [-64.0, -64.0, 64.0],
            aabb_max: [64.0, 64.0, 64.0],
            wire_color: [200, 200, 200],
            ..Default::default()
        }
    }

    fn row(r: &mut Renderer) -> Vec<[f32; 3]> {
        let params = hp_render::ViewParams {
            size: [128, 128],
            kind: hp_render::ViewKind::Perspective {
                eye: [0.0, 0.0, 150.0],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
                fov_y_deg: 80.0,
                near: 4.0,
                far: 10000.0,
            },
            mode: hp_render::RenderMode::Flat,
            grid: None,
            background: [0.0, 0.0, 0.0],
            selection_color: [1.0, 0.1, 0.1, 0.45],
            show_sprites: false,
            show_models: true,
            msaa: false,
        };
        r.render_view(0, &params, &hp_render::Overlay::default());
        let (w, _, px) = r.read_view_rgba8(0).expect("readback");
        (16..112u32)
            .map(|x| {
                let o = ((61 * w + x) * 4) as usize;
                [0, 1, 2].map(|k| f32::from(px[o + k]))
            })
            .collect()
    }

    fn lines(row: &[[f32; 3]]) -> usize {
        let lum: Vec<f32> = row.iter().map(|c| c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11).collect();
        let mut count = 0;
        let mut inside = false;
        for (i, l) in lum.iter().enumerate() {
            let around = lum[i.saturating_sub(4)..(i + 5).min(lum.len())].iter().copied().fold(0.0, f32::max);
            let dark = *l < around * 0.85;
            if dark && !inside {
                count += 1;
            }
            inside = dark;
        }
        count
    }

    #[test]
    fn the_grid_is_drawn_without_a_bake() {
        let Some((device, queue)) = gpu() else {
            eprintln!("no GPU adapter: test skipped");
            return;
        };
        let mut r = Renderer::new(&device, &queue);
        r.set_material(0, &hp_render::MaterialDesc { texture: None, tint: [1.0; 4], ..Default::default() });
        r.set_lightmap_display(hp_render::lightmap::LightmapDisplay::Grid, 1.0);
        r.set_lightmap_tracking(true);
        r.upsert_object(2, top_mesh());
        let plain = row(&mut r);
        assert_eq!(lines(&plain), 0, "no grid before the editor grid is built");

        let mut grid = EditorGrid::default();
        let coarse = doc(16, "dev/dev_measuregeneric01b", false);
        grid.sync(&mut r, coarse.scene().solids(), &mut |_| Some(0));
        assert!(grid.is_built());
        let s = r.lightmap_stats();
        assert_eq!((s.faces, s.valid_faces, s.baked_faces, s.blocks), (6, 6, 0, 1));
        let at16 = row(&mut r);
        let n16 = lines(&at16);
        assert!((6..=9).contains(&n16), "a 128 unit face at scale 16 shows 8 luxels: {n16} lines");

        r.upsert_object(2, top_mesh());
        grid.note_changes(r.take_lightmap_changes());
        let fine = doc(8, "dev/dev_measuregeneric01b", false);
        grid.sync(&mut r, fine.scene().solids(), &mut |_| Some(0));
        assert_eq!(r.lightmap_stats().valid_faces, 6);
        let at8 = row(&mut r);
        let n8 = lines(&at8);
        assert!(n8 >= n16 * 2 - 2, "scale 8 doubles the lines: {n8} against {n16}");
        let redness = |row: &[[f32; 3]]| row.iter().map(|c| c[0] - c[1]).sum::<f32>() / row.len() as f32;
        assert!(redness(&at8) > redness(&at16) + 10.0, "finer scales are tinted red");

        grid.clear(&mut r);
        assert_eq!(r.lightmap_stats().faces, 0);
        assert!(!grid.is_built());
        assert_eq!(lines(&row(&mut r)), 0);
    }
}
