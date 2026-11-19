use glam::{DVec2, DVec3};
use hp_geom::{Justify, Plane, TextureAlign, WELD_EPS};
use hp_vmf::{Side, Solid};

use crate::document::Document;
use crate::glob::{glob_replace, normalize_material, same_material};
use crate::ids::{IdMap, id_map};
use crate::selection::{FaceRef, ObjRef};
use crate::{EditError, EditResult, Id};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceEdit {
    pub material: Option<String>,
    pub align: Option<TextureAlign>,
    pub u_axis: Option<DVec3>,
    pub v_axis: Option<DVec3>,
    pub rotation: Option<f64>,
    pub rotate_by: Option<f64>,
    pub rotation_value: Option<f64>,
    pub u_scale: Option<f64>,
    pub v_scale: Option<f64>,
    pub scale_by: Option<DVec2>,
    pub u_shift: Option<f64>,
    pub v_shift: Option<f64>,
    pub shift_by: Option<DVec2>,
    pub lightmap_scale: Option<i32>,
    pub smoothing_groups: Option<u32>,
}

impl FaceEdit {
    fn validate(&self) -> EditResult<()> {
        if let Some(m) = &self.material
            && m.trim().is_empty()
        {
            return Err(EditError::Invalid("empty material name".to_owned()));
        }
        if let Some(l) = self.lightmap_scale
            && l < 1
        {
            return Err(EditError::Invalid(format!("invalid lightmap scale {l}")));
        }
        for a in [self.u_axis, self.v_axis].into_iter().flatten() {
            if a.try_normalize().is_none() {
                return Err(EditError::Invalid("texture axis is zero or not finite".to_owned()));
            }
        }
        let finite = [
            self.rotation,
            self.rotate_by,
            self.rotation_value,
            self.u_scale,
            self.v_scale,
            self.u_shift,
            self.v_shift,
        ]
        .into_iter()
        .flatten()
        .all(f64::is_finite)
            && self.scale_by.is_none_or(|s| s.is_finite())
            && self.shift_by.is_none_or(|s| s.is_finite());
        if !finite {
            return Err(EditError::Invalid("face values must be finite".to_owned()));
        }
        Ok(())
    }

    fn apply(&self, side: &mut Side, normal: DVec3) {
        if let Some(m) = &self.material {
            m.trim().clone_into(&mut side.material);
        }
        if let Some(align) = self.align {
            hp_geom::reset_axes(side, normal, align);
        }
        if let Some(u) = self.u_axis.and_then(DVec3::try_normalize) {
            side.uaxis.axis = u;
        }
        if let Some(v) = self.v_axis.and_then(DVec3::try_normalize) {
            side.vaxis.axis = v;
        }
        if let Some(r) = self.rotation {
            hp_geom::set_rotation(side, normal, r);
        }
        if let Some(r) = self.rotate_by {
            hp_geom::rotate_texture(side, normal, r);
        }
        if let Some(r) = self.rotation_value {
            side.rotation = r;
        }
        if self.u_scale.is_some() || self.v_scale.is_some() {
            let su = self.u_scale.unwrap_or(side.uaxis.scale);
            let sv = self.v_scale.unwrap_or(side.vaxis.scale);
            hp_geom::set_texture_scale(side, su, sv);
        }
        if let Some(s) = self.scale_by {
            hp_geom::scale_texture(side, s.x, s.y);
        }
        if let Some(u) = self.u_shift {
            side.uaxis.shift = u;
        }
        if let Some(v) = self.v_shift {
            side.vaxis.shift = v;
        }
        if let Some(s) = self.shift_by {
            hp_geom::shift_texture(side, s.x, s.y);
        }
        if let Some(l) = self.lightmap_scale {
            side.lightmap_scale = l;
        }
        if let Some(g) = self.smoothing_groups {
            side.smoothing_groups = g;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ReplaceScope {
    #[default]
    All,
    Selection,
    Objects(Vec<ObjRef>),
}

fn side_normal(side: &Side) -> DVec3 {
    Plane::from_side(side).map_or(DVec3::Z, |p| p.normal)
}

impl Document {
    pub fn face_side(&self, face: FaceRef) -> Option<&Side> {
        self.scene.solids.get(&face.solid)?.solid.sides.get(face.side)
    }

    pub fn face_polygon(&self, face: FaceRef) -> Option<&[DVec3]> {
        let n = self.scene.solids.get(&face.solid)?;
        n.geom.face_for_side(face.side).map(|f| f.vertices.as_slice())
    }

    fn faces_by_solid(&self, faces: &[FaceRef]) -> EditResult<IdMap<Vec<usize>>> {
        let mut out: IdMap<Vec<usize>> = id_map(faces.len());
        for f in faces {
            let ok = self.scene.solids.get(&f.solid).is_some_and(|n| f.side < n.solid.sides.len());
            if !ok {
                return Err(EditError::FaceNotFound(f.solid, f.side));
            }
            let list = out.entry(f.solid).or_default();
            if !list.contains(&f.side) {
                list.push(f.side);
            }
        }
        Ok(out)
    }

    fn edit_sides(
        &mut self,
        label: &str,
        faces: &[FaceRef],
        f: impl Fn(&mut Side, DVec3, &[DVec3]),
    ) -> EditResult<usize> {
        let grouped = self.faces_by_solid(faces)?;
        let mut updates: Vec<(Id, Solid)> = Vec::with_capacity(grouped.len());
        let mut count = 0;
        for (sid, sides) in &grouped {
            let Some(n) = self.scene.solids.get(sid) else { continue };
            let mut solid = n.solid.clone();
            let mut changed = false;
            for &i in sides {
                let poly: &[DVec3] = n.geom.face_for_side(i).map_or(&[], |fg| fg.vertices.as_slice());
                let Some(side) = solid.sides.get_mut(i) else { continue };
                let before = side.clone();
                f(side, side_normal(&before), poly);
                if *side != before {
                    changed = true;
                    count += 1;
                }
            }
            if changed {
                updates.push((*sid, solid));
            }
        }
        if updates.is_empty() {
            return Ok(0);
        }
        self.tx(label, |doc| {
            doc.update_solids(updates);
            Ok(count)
        })
    }

    pub fn apply_material(&mut self, faces: &[FaceRef], material: &str) -> EditResult<usize> {
        let material = material.trim();
        if material.is_empty() {
            return Err(EditError::Invalid("empty material name".to_owned()));
        }
        self.edit_sides("Apply material", faces, |side, _, _| {
            if side.material != material {
                material.clone_into(&mut side.material);
            }
        })
    }

    pub fn apply_material_to_objects(&mut self, objs: &[ObjRef], material: &str) -> EditResult<usize> {
        let faces = self.all_faces_of(objs);
        self.apply_material(&faces, material)
    }

    pub fn all_faces_of(&self, objs: &[ObjRef]) -> Vec<FaceRef> {
        let mut faces = Vec::new();
        for sid in self.scene.expand_solids(objs) {
            if let Some(n) = self.scene.solids.get(&sid) {
                faces.extend((0..n.solid.sides.len()).map(|i| FaceRef::new(sid, i)));
            }
        }
        faces
    }

    pub fn edit_faces(&mut self, faces: &[FaceRef], edit: &FaceEdit) -> EditResult<usize> {
        edit.validate()?;
        self.edit_sides("Edit faces", faces, |side, normal, _| edit.apply(side, normal))
    }

    pub fn justify_faces(
        &mut self,
        faces: &[FaceRef],
        mode: Justify,
        treat_as_one: bool,
        tex_size: &dyn Fn(&str) -> Option<(u32, u32)>,
    ) -> EditResult<usize> {
        let fallback = self.options.fallback_texture_size;
        let all = if treat_as_one { self.points_of_faces(faces) } else { Vec::new() };
        self.edit_sides("Justify texture", faces, |side, _, poly| {
            let (w, h) = tex_size(&side.material).unwrap_or(fallback);
            let pts = if treat_as_one { all.as_slice() } else { poly };
            hp_geom::justify(side, pts, w, h, mode);
        })
    }

    pub fn fit_faces(
        &mut self,
        faces: &[FaceRef],
        repeat_u: f64,
        repeat_v: f64,
        treat_as_one: bool,
        tex_size: &dyn Fn(&str) -> Option<(u32, u32)>,
    ) -> EditResult<usize> {
        let fallback = self.options.fallback_texture_size;
        let all = if treat_as_one { self.points_of_faces(faces) } else { Vec::new() };
        self.edit_sides("Fit texture", faces, |side, _, poly| {
            let (w, h) = tex_size(&side.material).unwrap_or(fallback);
            let pts = if treat_as_one { all.as_slice() } else { poly };
            hp_geom::fit_texture(side, pts, w, h, repeat_u, repeat_v);
        })
    }

    pub fn align_faces_to_view(&mut self, faces: &[FaceRef], right: DVec3, up: DVec3) -> EditResult<usize> {
        self.edit_sides("Align texture", faces, |side, _, _| hp_geom::align_to_view(side, right, up))
    }

    fn points_of_faces(&self, faces: &[FaceRef]) -> Vec<DVec3> {
        let mut pts = Vec::new();
        for f in faces {
            if let Some(p) = self.face_polygon(*f) {
                pts.extend_from_slice(p);
            }
        }
        pts
    }

    pub fn wrap_texture(&mut self, src: FaceRef, dst: &[FaceRef]) -> EditResult<usize> {
        self.apply_texture_and_alignment(src, dst, None, true)
    }

    pub fn apply_texture_and_alignment(
        &mut self,
        src: FaceRef,
        dst: &[FaceRef],
        material: Option<&str>,
        wrap: bool,
    ) -> EditResult<usize> {
        let mut src_side = self.face_side(src).ok_or(EditError::FaceNotFound(src.solid, src.side))?.clone();
        if let Some(m) = material.map(str::trim) {
            if m.is_empty() {
                return Err(EditError::Invalid("empty material name".to_owned()));
            }
            m.clone_into(&mut src_side.material);
        }
        if !wrap {
            return self.edit_sides("Apply texture and alignment", dst, |side, normal, _| {
                side.material.clone_from(&src_side.material);
                side.uaxis = src_side.uaxis;
                side.vaxis = src_side.vaxis;
                side.rotation = src_side.rotation;
                side.lightmap_scale = src_side.lightmap_scale;
                hp_geom::ensure_valid_axes(side, normal);
            });
        }
        let src_normal = side_normal(&src_side);
        let src_poly: Vec<DVec3> = self.face_polygon(src).map(<[DVec3]>::to_vec).unwrap_or_default();
        let src_center = hp_geom::polygon_center(&src_poly);
        let src_plane = Plane::from_side(&src_side);
        self.edit_sides("Wrap texture", dst, |side, normal, poly| {
            let edge_point = shared_point(&src_poly, poly)
                .or_else(|| src_plane.and_then(|sp| plane_line_point(&sp, side, src_center)))
                .unwrap_or(src_center);
            hp_geom::wrap_texture(&src_side, src_normal, side, normal, edge_point);
        })
    }

    pub fn replace_materials(&mut self, find: &str, replace: &str, scope: &ReplaceScope) -> EditResult<usize> {
        let find = normalize_material(find).into_owned();
        let replace = replace.trim();
        if find.is_empty() || replace.is_empty() {
            return Err(EditError::Invalid("empty material name".to_owned()));
        }
        let faces: Vec<FaceRef> = match scope {
            ReplaceScope::All => {
                let mut v = Vec::new();
                for n in self.scene.solids.values() {
                    v.extend((0..n.solid.sides.len()).map(|i| FaceRef::new(n.solid.id, i)));
                }
                v
            }
            ReplaceScope::Selection => {
                let objs = self.selected_objects();
                let mut v = self.all_faces_of(&objs);
                v.extend(self.selection.faces.iter().copied());
                v
            }
            ReplaceScope::Objects(objs) => self.all_faces_of(objs),
        };
        self.edit_sides("Replace materials", &faces, |side, _, _| {
            let current = normalize_material(&side.material).to_lowercase();
            if let Some(new) = glob_replace(&find, replace, &current)
                && !same_material(&new, &side.material)
            {
                side.material = new;
            }
        })
    }

    pub fn used_materials(&self) -> Vec<(String, usize)> {
        let mut map: std::collections::HashMap<String, (String, usize)> = std::collections::HashMap::new();
        for n in self.scene.solids.values() {
            for s in &n.solid.sides {
                let key = normalize_material(&s.material).to_ascii_lowercase();
                map.entry(key).or_insert_with(|| (s.material.clone(), 0)).1 += 1;
            }
        }
        let mut v: Vec<(String, (String, usize))> = map.into_iter().collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v.into_iter().map(|(_, x)| x).collect()
    }
}

fn shared_point(a: &[DVec3], b: &[DVec3]) -> Option<DVec3> {
    a.iter().copied().find(|p| b.iter().any(|q| p.distance_squared(*q) < (WELD_EPS * 10.0).powi(2)))
}

fn plane_line_point(src: &Plane, dst_side: &Side, near: DVec3) -> Option<DVec3> {
    let dst = Plane::from_side(dst_side)?;
    let (n1, n2) = (src.normal, dst.normal);
    let dir = n1.cross(n2);
    let (a, b, c) = (n1.dot(n1), n1.dot(n2), n2.dot(n2));
    let det = a * c - b * b;
    if dir.length_squared() < 1e-12 || det.abs() < 1e-12 {
        return None;
    }
    let p0 = (n1 * (src.dist * c - dst.dist * b) + n2 * (dst.dist * a - src.dist * b)) / det;
    let d = dir.normalize();
    let p = p0 + d * (near - p0).dot(d);
    p.is_finite().then_some(p)
}
