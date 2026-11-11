use glam::DVec3;
use hp_geom::{GeomError, PaintBrush, VertexEdit, VertexEditOptions, create_disp_with_axes, disp_paint};
use hp_vmf::{DispInfo, Solid};

use crate::document::Document;
use crate::selection::FaceRef;
use crate::{EditError, EditResult, Id};

impl Document {
    pub fn disp_faces(&self) -> Vec<FaceRef> {
        let mut out = Vec::new();
        for n in self.scene.solids.values() {
            for (i, s) in n.solid.sides.iter().enumerate() {
                if s.disp.is_some() {
                    out.push(FaceRef::new(n.solid.id, i));
                }
            }
        }
        out
    }

    pub fn create_displacements(&mut self, faces: &[FaceRef], power: u32) -> EditResult<usize> {
        if !(2..=4).contains(&power) {
            return Err(hp_geom::GeomError::BadPower(power).into());
        }
        let mut updates: Vec<(Id, Solid)> = Vec::new();
        let mut count = 0;
        let mut seen = std::collections::HashSet::new();
        for f in faces {
            if !seen.insert(*f) {
                continue;
            }
            let n = self.scene.solids.get(&f.solid).ok_or(EditError::FaceNotFound(f.solid, f.side))?;
            let side = n.solid.sides.get(f.side).ok_or(EditError::FaceNotFound(f.solid, f.side))?;
            let poly = n.geom.face_for_side(f.side).map(|fg| fg.vertices.as_slice()).unwrap_or(&[]);
            let disp = create_disp_with_axes(poly, power, side.uaxis.axis, side.vaxis.axis)?;
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
        self.tx("Create displacement", |doc| {
            doc.update_solids(updates);
            Ok(count)
        })
    }

    pub fn destroy_displacements(&mut self, faces: &[FaceRef]) -> EditResult<usize> {
        let mut updates: Vec<(Id, Solid)> = Vec::new();
        let mut count = 0;
        for f in faces {
            let Some(n) = self.scene.solids.get(&f.solid) else { continue };
            if n.solid.sides.get(f.side).is_none_or(|s| s.disp.is_none() && !unreadable_disp(s)) {
                continue;
            }
            let idx = match updates.iter().position(|(id, _)| *id == f.solid) {
                Some(i) => i,
                None => {
                    updates.push((f.solid, n.solid.clone()));
                    updates.len() - 1
                }
            };
            if let Some(s) = updates.get_mut(idx).and_then(|(_, s)| s.sides.get_mut(f.side))
                && (s.disp.take().is_some() | remove_unreadable_disp(s))
            {
                count += 1;
            }
        }
        if count == 0 {
            return Ok(0);
        }
        self.tx("Destroy displacement", |doc| {
            doc.update_solids(updates);
            Ok(count)
        })
    }

    pub fn disp_paint(&mut self, faces: &[FaceRef], brush: &PaintBrush) -> EditResult<usize> {
        if !brush.center.is_finite() || !brush.radius.is_finite() || brush.radius <= 0.0 || !brush.amount.is_finite() {
            return Err(EditError::Invalid("invalid paint brush".to_owned()));
        }
        let candidates: Vec<FaceRef> = if faces.is_empty() {
            self.disp_faces().into_iter().filter(|f| self.is_solid_visible(f.solid)).collect()
        } else {
            faces.to_vec()
        };
        let reach = hp_geom::Aabb::new(brush.center, brush.center).expand(brush.radius);
        let mut updates: Vec<(Id, Solid)> = Vec::new();
        let mut total = 0;
        for f in candidates {
            let Some(n) = self.scene.solids.get(&f.solid) else { continue };
            let Some(mesh) = n.disp_mesh(f.side) else { continue };
            if !brush.spatial {
                let flat = hp_geom::Aabb::from_points(mesh.base_positions.iter().copied());
                let n_axis = mesh.face_normal.abs();
                let mut grown = reach;
                for k in 0..3 {
                    if n_axis[k] > 1e-6 {
                        grown.min[k] = f64::NEG_INFINITY;
                        grown.max[k] = f64::INFINITY;
                    }
                }
                if !flat.intersects(&grown) {
                    continue;
                }
            } else if !mesh.aabb.intersects(&reach) {
                continue;
            }
            let Some(face) = n.geom.face_for_side(f.side) else { continue };
            let idx = match updates.iter().position(|(id, _)| *id == f.solid) {
                Some(i) => i,
                None => {
                    updates.push((f.solid, n.solid.clone()));
                    updates.len() - 1
                }
            };
            let Some(side) = updates.get_mut(idx).and_then(|(_, s)| s.sides.get_mut(f.side)) else { continue };
            let Some(disp) = side.disp.as_mut() else { continue };
            total += disp_paint(disp, &face.vertices, brush)?;
        }
        if total == 0 {
            return Ok(0);
        }
        self.tx("Paint displacement", |doc| {
            doc.update_solids(updates);
            Ok(total)
        })
    }
    fn map_disps(
        &mut self,
        faces: &[FaceRef],
        label: &str,
        mut f: impl FnMut(&mut DispInfo, &[DVec3], FaceRef) -> Result<usize, GeomError>,
    ) -> EditResult<usize> {
        let mut updates: Vec<(Id, Solid)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut total = 0;
        for face in faces {
            if !seen.insert(*face) {
                continue;
            }
            let Some(n) = self.scene.solids.get(&face.solid) else { continue };
            if n.solid.sides.get(face.side).is_none_or(|s| s.disp.is_none()) {
                continue;
            }
            let Some(poly) = n.geom.face_for_side(face.side) else { continue };
            let idx = match updates.iter().position(|(id, _)| *id == face.solid) {
                Some(i) => i,
                None => {
                    updates.push((face.solid, n.solid.clone()));
                    updates.len() - 1
                }
            };
            let disp = updates.get_mut(idx).and_then(|(_, s)| s.sides.get_mut(face.side)).and_then(|s| s.disp.as_mut());
            let Some(disp) = disp else { continue };
            total += f(disp, &poly.vertices, *face)?;
        }
        if total == 0 {
            return Ok(0);
        }
        self.tx(label, |doc| {
            doc.update_solids(updates);
            Ok(total)
        })
    }

    pub fn invert_disp_alpha(&mut self, faces: &[FaceRef]) -> EditResult<usize> {
        self.map_disps(faces, "Invert displacement alpha", |d, _, _| Ok(hp_geom::invert_alpha(d)))
    }

    pub fn disp_noise(&mut self, faces: &[FaceRef], min: f64, max: f64, seed: u64) -> EditResult<usize> {
        if !min.is_finite() || !max.is_finite() || min.abs() > 16384.0 || max.abs() > 16384.0 {
            return Err(EditError::Invalid("noise range".to_owned()));
        }
        self.map_disps(faces, "Displacement noise", |d, poly, _| hp_geom::add_noise(d, poly, min, max, seed))
    }

    pub fn disp_fractal_noise(
        &mut self,
        faces: &[FaceRef],
        min: f64,
        max: f64,
        params: &hp_geom::NoiseParams,
    ) -> EditResult<usize> {
        if !min.is_finite() || !max.is_finite() || min.abs() > 16384.0 || max.abs() > 16384.0 {
            return Err(EditError::Invalid("noise range".to_owned()));
        }
        if !params.scale.is_finite() || params.scale <= 0.0 || !params.persistence.is_finite() {
            return Err(EditError::Invalid("noise parameters".to_owned()));
        }
        let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
        self.map_disps(faces, "Fractal noise", |d, poly, _| {
            hp_geom::add_height_fn(d, poly, |base| lo + (hi - lo) * (hp_geom::fractal_noise(base, params) * 0.5 + 0.5))
        })
    }

    pub fn scale_disps(&mut self, faces: &[FaceRef], factor: f64) -> EditResult<usize> {
        if !factor.is_finite() || factor.abs() > 1000.0 {
            return Err(EditError::Invalid("displacement scale".to_owned()));
        }
        self.map_disps(faces, "Scale displacement", |d, poly, _| hp_geom::scale_disp(d, poly, factor))
    }

    pub fn set_disp_flags(&mut self, faces: &[FaceRef], mask: i32, on: bool) -> EditResult<usize> {
        self.map_disps(faces, "Displacement collision", |d, _, _| {
            let flags = if on { d.flags | mask } else { d.flags & !mask };
            let changed = usize::from(flags != d.flags);
            d.flags = flags;
            Ok(changed)
        })
    }

    pub fn set_disp_data(&mut self, label: &str, updates: Vec<(FaceRef, DispInfo)>) -> EditResult<usize> {
        let mut solids: Vec<(Id, Solid)> = Vec::new();
        let mut count = 0;
        for (face, data) in updates {
            let Some(n) = self.scene.solids.get(&face.solid) else { continue };
            let Some(old) = n.solid.sides.get(face.side).and_then(|s| s.disp.as_ref()) else { continue };
            if old.power != data.power {
                return Err(EditError::Invalid("displacement power changed".to_owned()));
            }
            if **old == data {
                continue;
            }
            let idx = match solids.iter().position(|(id, _)| *id == face.solid) {
                Some(i) => i,
                None => {
                    solids.push((face.solid, n.solid.clone()));
                    solids.len() - 1
                }
            };
            if let Some(side) = solids.get_mut(idx).and_then(|(_, s)| s.sides.get_mut(face.side)) {
                side.disp = Some(Box::new(data));
                count += 1;
            }
        }
        if count == 0 {
            return Ok(0);
        }
        self.tx(label, |doc| {
            doc.update_solids(solids);
            Ok(count)
        })
    }

    pub fn edit_solid_vertices(&mut self, id: Id, edit: &VertexEdit<'_>, opts: VertexEditOptions) -> EditResult<()> {
        let n = self.scene.solids.get(&id).ok_or(EditError::NotFound(crate::ObjRef::Solid(id)))?;
        let template = n.solid.sides.clone();
        self.edit_solid_vertices_from(id, edit, &template, opts)
    }

    pub fn edit_solid_vertices_from(
        &mut self,
        id: Id,
        edit: &VertexEdit<'_>,
        template: &[hp_vmf::Side],
        opts: VertexEditOptions,
    ) -> EditResult<()> {
        let n = self.scene.solids.get(&id).ok_or(EditError::NotFound(crate::ObjRef::Solid(id)))?;
        if edit.points.iter().any(|p| !p.is_finite()) {
            return Err(EditError::Invalid("vertex is not finite".to_owned()));
        }
        let mut next = self.next_side_id;
        let sides = hp_geom::rebuild_solid(edit, template, opts, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        })?;
        hp_geom::validate_solid(&sides)?;
        let mut solid = n.solid.clone();
        solid.sides = sides;
        self.next_side_id = next;
        self.tx("Edit vertices", |doc| {
            doc.update_solid(id, solid);
            doc.deselect_missing();
            Ok(())
        })
    }
}

pub(crate) fn unreadable_disp(side: &hp_vmf::Side) -> bool {
    side.disp.is_none() && side.extra_blocks.iter().any(|b| b.name.eq_ignore_ascii_case("dispinfo"))
}

fn remove_unreadable_disp(side: &mut hp_vmf::Side) -> bool {
    let before = side.extra_blocks.len();
    side.extra_blocks.retain(|b| !b.name.eq_ignore_ascii_case("dispinfo"));
    side.extra_blocks.len() != before
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use glam::DVec3;
    use hp_geom::{DISP_FLAG_NO_HULL, DISP_FLAG_NO_RAY, FaceTopology, VertexEdit, VertexEditOptions};

    use crate::selection::FaceRef;
    use crate::tests::{check_undo_redo, doc_with_cube};

    fn disp_doc() -> (crate::Document, FaceRef) {
        let (mut doc, id) = doc_with_cube();
        let top = FaceRef::new(id, 0);
        assert_eq!(doc.create_displacements(&[top], 2).expect("create"), 1);
        doc.clear_history();
        (doc, top)
    }

    fn disp(doc: &crate::Document, f: FaceRef) -> hp_vmf::DispInfo {
        (**doc.face_side(f).and_then(|s| s.disp.as_ref()).expect("disp")).clone()
    }

    #[test]
    fn invert_noise_scale_and_flags_are_undoable() {
        let (mut doc, top) = disp_doc();
        let n = check_undo_redo(&mut doc, |d| d.invert_disp_alpha(&[top, top]).expect("invert"));
        assert_eq!(n, 25);
        assert!(disp(&doc, top).alphas.iter().all(|a| *a == 255.0));
        let moved = check_undo_redo(&mut doc, |d| d.disp_noise(&[top], 2.0, 6.0, 42).expect("noise"));
        assert_eq!(moved, 25);
        let before: Vec<f64> = disp(&doc, top).distances;
        assert!(before.iter().all(|h| (2.0..=6.0).contains(h)));
        check_undo_redo(&mut doc, |d| d.scale_disps(&[top], 2.0).expect("scale"));
        let after = disp(&doc, top).distances;
        for (a, b) in before.iter().zip(&after) {
            assert!((a * 2.0 - b).abs() < 1e-9);
        }
        check_undo_redo(&mut doc, |d| {
            d.set_disp_flags(&[top], DISP_FLAG_NO_HULL | DISP_FLAG_NO_RAY, true).expect("flags")
        });
        assert_eq!(disp(&doc, top).flags, 12);
        assert_eq!(doc.set_disp_flags(&[top], DISP_FLAG_NO_RAY, true).expect("flags"), 0, "already set");
        doc.set_disp_flags(&[top], DISP_FLAG_NO_HULL, false).expect("flags");
        assert_eq!(disp(&doc, top).flags, DISP_FLAG_NO_RAY);
        assert!(doc.to_vmf_string().contains("\"flags\" \"8\""));
        let wall = FaceRef::new(top.solid, 2);
        assert_eq!(doc.invert_disp_alpha(&[wall]).expect("nothing"), 0);
        assert!(doc.disp_noise(&[top], f64::NAN, 1.0, 1).is_err());
        let fractal = hp_geom::NoiseParams { scale: 32.0, octaves: 3, persistence: 0.5, seed: 5 };
        let n = check_undo_redo(&mut doc, |d| d.disp_fractal_noise(&[top], -8.0, 8.0, &fractal).expect("fractal"));
        assert!(n > 0);
        assert!(doc.disp_fractal_noise(&[top], f64::NAN, 1.0, &fractal).is_err());
        assert!(doc.disp_fractal_noise(&[top], 0.0, 1.0, &hp_geom::NoiseParams { scale: 0.0, ..fractal }).is_err());
        assert!(doc.scale_disps(&[top], f64::INFINITY).is_err());
    }

    #[test]
    fn set_disp_data_replaces_whole_displacements() {
        let (mut doc, top) = disp_doc();
        let mut d = disp(&doc, top);
        d.distances[3] = 9.0;
        let n = check_undo_redo(&mut doc, |doc| {
            doc.set_disp_data("Sew displacements", vec![(top, d.clone())]).expect("set")
        });
        assert_eq!(n, 1);
        assert_eq!(disp(&doc, top).distances[3], 9.0);
        assert_eq!(doc.set_disp_data("Sew displacements", vec![(top, d.clone())]).expect("same"), 0);
        let mut bad = d;
        bad.power = 3;
        assert!(doc.set_disp_data("Sew displacements", vec![(top, bad)]).is_err());
    }

    #[test]
    fn edit_solid_vertices_with_topology_and_uv_lock() {
        let (mut doc, id) = doc_with_cube();
        let (base, faces) = doc.solid_vertices(id).expect("vertices");
        let templates: Vec<usize> =
            doc.scene().solid(id).expect("cube").geom.faces.iter().map(|f| f.side_index).collect();
        let pts: Vec<DVec3> = base.iter().map(|p| if p.z > 32.0 { *p + DVec3::Z * 16.0 } else { *p }).collect();
        let topo = FaceTopology { faces: &faces, templates: &templates };
        let edit = VertexEdit { points: &pts, base: Some(&base), topology: Some(topo) };
        check_undo_redo(&mut doc, |d| {
            d.edit_solid_vertices(id, &edit, VertexEditOptions { uv_lock: true }).expect("edit")
        });
        let b = doc.scene().solid(id).expect("cube").bounds;
        assert_eq!(b.max.z, 80.0);
        let wall = doc.scene().solid(id).expect("cube").solid.sides[2].clone();
        assert!((wall.vaxis.scale.abs() - 0.3125).abs() < 1e-9, "{}", wall.vaxis.scale);
        let before = doc.to_vmf_string();
        let flat: Vec<DVec3> = pts.iter().map(|p| DVec3::new(p.x, p.y, 0.0)).collect();
        let edit = VertexEdit { points: &flat, base: None, topology: None };
        assert!(doc.edit_solid_vertices(id, &edit, VertexEditOptions::default()).is_err());
        let nan = vec![DVec3::NAN; pts.len()];
        let edit = VertexEdit { points: &nan, base: None, topology: None };
        assert!(doc.edit_solid_vertices(id, &edit, VertexEditOptions::default()).is_err());
        assert_eq!(doc.to_vmf_string(), before);
    }
}
