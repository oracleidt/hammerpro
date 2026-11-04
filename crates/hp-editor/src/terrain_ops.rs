use glam::{DVec2, DVec3};
use hp_geom::{Aabb, Primitive, PrimitiveParams, build_disp, make_primitive, update_disp_tags};
use hp_vmf::multiblend::MultiBlend;
use hp_vmf::{DispInfo, Side, Solid};

use crate::document::Document;
use crate::scene::Owner;
use crate::selection::FaceRef;
use crate::{EditError, EditResult, Id};

pub const MAX_TERRAIN_TILES: usize = 4096;
const NODRAW: &str = "TOOLS/TOOLSNODRAW";

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainLayout {
    pub min: DVec2,
    pub size: DVec2,
    pub tiles: (u32, u32),
    pub power: u32,
    pub plane_z: f64,
    pub thickness: f64,
    pub material: String,
    pub lightmap_scale: i32,
}

pub struct TerrainSurface<'a> {
    pub height: &'a dyn Fn(f64, f64) -> f64,
    pub weights: Option<&'a dyn Fn(f64, f64) -> [f64; 4]>,
}

fn edges(min: f64, size: f64, count: u32) -> Vec<f64> {
    (0..=count)
        .map(|i| match i {
            0 => min,
            i if i == count => min + size,
            i => (min + size * f64::from(i) / f64::from(count)).round_ties_even(),
        })
        .collect()
}

impl Document {
    pub fn create_terrain(
        &mut self,
        layout: &TerrainLayout,
        surface: &TerrainSurface<'_>,
        label: &str,
    ) -> EditResult<Vec<Id>> {
        let (tx, ty) = layout.tiles;
        let count = (tx as usize).saturating_mul(ty as usize);
        if tx == 0 || ty == 0 || count > MAX_TERRAIN_TILES {
            return Err(EditError::Invalid(format!("the terrain needs 1 to {MAX_TERRAIN_TILES} tiles")));
        }
        if !(2..=4).contains(&layout.power) {
            return Err(hp_geom::GeomError::BadPower(layout.power).into());
        }
        let finite = layout.min.is_finite() && layout.size.is_finite() && layout.plane_z.is_finite();
        if !finite || layout.size.min_element() <= 0.0 || !(layout.thickness.is_finite() && layout.thickness > 0.0) {
            return Err(EditError::Invalid("terrain size and thickness must be positive numbers".to_owned()));
        }
        if layout.min.abs().max_element() > 16384.0 || layout.size.max_element() > 32768.0 {
            return Err(EditError::Invalid("the terrain is outside the map limits".to_owned()));
        }
        let xs = edges(layout.min.x, layout.size.x, tx);
        let ys = edges(layout.min.y, layout.size.y, ty);
        let mut next = self.next_side_id;
        let mut pieces: Vec<Vec<Side>> = Vec::with_capacity(count);
        let mut tops: Vec<usize> = Vec::with_capacity(count);
        for yw in ys.windows(2) {
            for xw in xs.windows(2) {
                let bounds = Aabb::new(
                    DVec3::new(xw[0], yw[0], layout.plane_z - layout.thickness),
                    DVec3::new(xw[1], yw[1], layout.plane_z),
                );
                let params = PrimitiveParams {
                    kind: Primitive::Block,
                    bounds,
                    material: layout.material.clone(),
                    lightmap_scale: layout.lightmap_scale,
                    ..PrimitiveParams::default()
                };
                let mut made = make_primitive(&params, &mut || {
                    let id = next;
                    next = next.saturating_add(1);
                    id
                })?;
                let mut sides = made.pop().ok_or(EditError::Nothing("no solid created"))?;
                let top = sides
                    .iter()
                    .position(|s| s.normal().z > 0.99)
                    .ok_or_else(|| EditError::Invalid("the tile has no top face".to_owned()))?;
                for (i, s) in sides.iter_mut().enumerate() {
                    if i != top {
                        s.material = NODRAW.to_owned();
                    }
                }
                tops.push(top);
                pieces.push(sides);
            }
        }
        self.tx(label, |doc| {
            let ids = doc.create_solids(pieces, Owner::World)?;
            let faces: Vec<FaceRef> = ids.iter().zip(&tops).map(|(id, top)| FaceRef::new(*id, *top)).collect();
            doc.create_displacements(&faces, layout.power)?;
            doc.set_terrain_surface(&faces, surface, label)?;
            Ok(ids)
        })
    }

    pub fn set_terrain_surface(
        &mut self,
        faces: &[FaceRef],
        surface: &TerrainSurface<'_>,
        label: &str,
    ) -> EditResult<usize> {
        let mut updates: Vec<(Id, Solid)> = Vec::new();
        let mut count = 0;
        let mut seen = std::collections::HashSet::new();
        for f in faces {
            if !seen.insert(*f) {
                continue;
            }
            let Some(n) = self.scene.solids.get(&f.solid) else { continue };
            let Some(poly) = n.geom.face_for_side(f.side) else { continue };
            let Some(old) = n.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()) else { continue };
            let mut disp = old.clone();
            if !apply_surface(&mut disp, &poly.vertices, surface) {
                continue;
            }
            let idx = match updates.iter().position(|(id, _)| *id == f.solid) {
                Some(i) => i,
                None => {
                    updates.push((f.solid, n.solid.clone()));
                    updates.len() - 1
                }
            };
            if let Some(s) = updates.get_mut(idx).and_then(|(_, s)| s.sides.get_mut(f.side)) {
                s.disp = Some(Box::new(disp));
                count += 1;
            }
        }
        if count == 0 {
            return Ok(0);
        }
        self.tx(label, |doc| {
            doc.update_solids(updates);
            Ok(count)
        })
    }
}

fn apply_surface(disp: &mut DispInfo, poly: &[DVec3], surface: &TerrainSurface<'_>) -> bool {
    let Some(mesh) = build_disp(poly, disp) else { return false };
    let n = disp.row_len();
    let count = n * n;
    if mesh.base_positions.len() != count
        || disp.normals.len() != count
        || disp.distances.len() != count
        || disp.offsets.len() != count
    {
        return false;
    }
    let normal = mesh.face_normal;
    let mut blend = surface.weights.map(|_| MultiBlend::new(count));
    for (k, base) in mesh.base_positions.iter().enumerate() {
        let h = (surface.height)(base.x, base.y);
        if let (Some(nrm), Some(dist), Some(off)) =
            (disp.normals.get_mut(k), disp.distances.get_mut(k), disp.offsets.get_mut(k))
        {
            *nrm = normal;
            *dist = if h.is_finite() { h } else { 0.0 };
            *off = DVec3::ZERO;
        }
        if let (Some(w), Some(b)) = (surface.weights, blend.as_mut()) {
            let weights = w(base.x, base.y);
            if let Some(slot) = b.weights.get_mut(k) {
                *slot = weights;
            }
            if let Some(a) = disp.alphas.get_mut(k) {
                *a = (255.0 * (1.0 - weights[0])).clamp(0.0, 255.0);
            }
        }
    }
    if let Some(mut b) = blend {
        b.normalize();
        b.write_to(disp);
    }
    update_disp_tags(disp, poly);
    true
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;

    fn layout(tiles: (u32, u32), power: u32) -> TerrainLayout {
        TerrainLayout {
            min: DVec2::new(-512.0, -256.0),
            size: DVec2::new(1024.0, 512.0),
            tiles,
            power,
            plane_z: 0.0,
            thickness: 16.0,
            material: "DEV/DEV_BLENDMEASURE".to_owned(),
            lightmap_scale: 16,
        }
    }

    fn ramp(x: f64, _y: f64) -> f64 {
        (x + 512.0) / 4.0
    }

    fn positions(doc: &Document, ids: &[Id]) -> Vec<DVec3> {
        let mut all = Vec::new();
        for id in ids {
            let node = doc.scene().solid(*id).unwrap();
            for (i, _) in node.solid.sides.iter().enumerate() {
                if let Some(m) = node.disp_mesh(i) {
                    all.extend(m.positions.iter().copied());
                }
            }
        }
        all
    }

    #[test]
    fn terrain_tiles_follow_the_height_function_and_share_seams() {
        let mut doc = Document::new_empty();
        let surface = TerrainSurface { height: &ramp, weights: None };
        let ids = doc.create_terrain(&layout((4, 2), 3), &surface, "Generate terrain").unwrap();
        assert_eq!(ids.len(), 8);
        assert_eq!(doc.disp_faces().len(), 8);
        let all = positions(&doc, &ids);
        assert_eq!(all.len(), 8 * 81);
        for p in &all {
            assert!((p.z - ramp(p.x, p.y)).abs() < 1e-6, "{p:?}");
        }
        let mut by_xy: std::collections::HashMap<(i64, i64), f64> = std::collections::HashMap::new();
        for p in &all {
            let key = ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64);
            let z = *by_xy.entry(key).or_insert(p.z);
            assert!((z - p.z).abs() < 1e-6, "seam mismatch at {key:?}");
        }
        assert_eq!(by_xy.len(), (4 * 8 + 1) * (2 * 8 + 1));
        let xs: Vec<f64> = all.iter().map(|p| p.x).collect();
        assert_eq!(xs.iter().copied().fold(f64::MAX, f64::min), -512.0);
        assert_eq!(xs.iter().copied().fold(f64::MIN, f64::max), 512.0);
    }

    #[test]
    fn terrain_is_one_undo_step_and_selects_the_tiles() {
        let mut doc = Document::new_empty();
        let surface = TerrainSurface { height: &ramp, weights: None };
        let ids = doc.create_terrain(&layout((2, 2), 2), &surface, "Generate terrain").unwrap();
        assert_eq!(doc.selected_solid_ids().len(), ids.len());
        assert!(doc.undo());
        assert!(doc.scene().solids.is_empty());
        assert!(doc.redo());
        assert_eq!(doc.disp_faces().len(), 4);
    }

    #[test]
    fn weights_write_four_way_blend_rows_and_alpha() {
        let mut doc = Document::new_empty();
        let weights = |x: f64, _y: f64| if x < 0.0 { [1.0, 0.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0, 0.0] };
        let surface = TerrainSurface { height: &ramp, weights: Some(&weights) };
        let ids = doc.create_terrain(&layout((2, 1), 2), &surface, "Import heightmap").unwrap();
        let mut saw_rock = false;
        for id in &ids {
            let node = doc.scene().solid(*id).unwrap();
            for side in &node.solid.sides {
                let Some(d) = side.disp.as_deref() else { continue };
                let mb = MultiBlend::of(d).unwrap();
                saw_rock |= mb.weights.iter().any(|w| w[1] > 0.99);
                assert!(d.alphas.iter().any(|a| *a == 0.0 || *a == 255.0));
            }
        }
        assert!(saw_rock);
    }

    #[test]
    fn replacing_heights_keeps_the_solids_and_rejects_bad_layouts() {
        let mut doc = Document::new_empty();
        let flat = |_x: f64, _y: f64| 0.0;
        let surface = TerrainSurface { height: &flat, weights: None };
        let ids = doc.create_terrain(&layout((2, 1), 2), &surface, "Generate terrain").unwrap();
        let faces = doc.disp_faces();
        let up = |_x: f64, _y: f64| 40.0;
        let n = doc
            .set_terrain_surface(&faces, &TerrainSurface { height: &up, weights: None }, "Generate terrain")
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(doc.scene().solids.len(), ids.len());
        assert!(positions(&doc, &ids).iter().all(|p| (p.z - 40.0).abs() < 1e-6));
        assert!(doc.create_terrain(&layout((0, 1), 2), &surface, "x").is_err());
        assert!(doc.create_terrain(&layout((1, 1), 5), &surface, "x").is_err());
        assert!(doc.create_terrain(&layout((100, 100), 2), &surface, "x").is_err());
    }
}
