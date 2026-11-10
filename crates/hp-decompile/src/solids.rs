use glam::DVec3;
use hp_geom::Plane as GeomPlane;
use hp_vmf::{Id, Side, Solid, TexAxis};

use crate::lumps::{Lumps, surf};
use crate::textures::original_material;

const MAX_TREE_DEPTH: usize = 4096;

pub(crate) fn model_brushes(l: &Lumps, model: usize) -> Vec<usize> {
    let Some(m) = l.models.get(model) else { return Vec::new() };
    let mut out = Vec::new();
    let mut seen_brush = vec![false; l.brushes.len()];
    let mut seen_node = vec![false; l.nodes.len()];
    let mut stack: Vec<(i32, usize)> = vec![(m.headnode, 0)];
    while let Some((child, depth)) = stack.pop() {
        if depth > MAX_TREE_DEPTH {
            continue;
        }
        if child >= 0 {
            let i = child as usize;
            let Some(node) = l.nodes.get(i) else { continue };
            if std::mem::replace(&mut seen_node[i], true) {
                continue;
            }
            stack.push((node.children[1], depth + 1));
            stack.push((node.children[0], depth + 1));
        } else {
            let leaf = (-1i64 - i64::from(child)) as usize;
            let Some(leaf) = l.leafs.get(leaf) else { continue };
            let first = usize::from(leaf.first_brush);
            for k in first..first + usize::from(leaf.num_brushes) {
                let Some(&b) = l.leafbrushes.get(k) else { break };
                let b = usize::from(b);
                if b < seen_brush.len() && !std::mem::replace(&mut seen_brush[b], true) {
                    out.push(b);
                }
            }
        }
    }
    out
}

pub(crate) fn orphan_class(l: &Lumps, brushes: &[usize]) -> &'static str {
    let mut trigger = false;
    let mut all_tool = true;
    let mut any = false;
    for &b in brushes {
        let Some(brush) = l.brushes.get(b) else { continue };
        let first = usize::try_from(brush.firstside).unwrap_or(0);
        for side in l.brushsides.iter().skip(first).take(usize::try_from(brush.numsides).unwrap_or(0)) {
            let name = usize::try_from(side.texinfo)
                .ok()
                .and_then(|t| l.texinfo.get(t))
                .and_then(|t| usize::try_from(t.texdata).ok())
                .and_then(|d| l.texdata_names.get(d))
                .map(|n| n.to_ascii_lowercase())
                .unwrap_or_default();
            any = true;
            if name == "tools/toolstrigger" {
                trigger = true;
            }
            if !name.starts_with("tools/") {
                all_tool = false;
            }
        }
    }
    if any && trigger && all_tool { "trigger_multiple" } else { "func_brush" }
}

pub(crate) fn orphan_models(
    l: &Lumps,
    claimed: &std::collections::HashMap<usize, DVec3>,
) -> Vec<(usize, hp_bsp::Entity, DVec3)> {
    let mut out = Vec::new();
    for m in 1..l.models.len() {
        if claimed.contains_key(&m) {
            continue;
        }
        let brushes = model_brushes(l, m);
        if brushes.is_empty() {
            continue;
        }
        let origin = l.models[m].origin;
        let mut e = hp_bsp::Entity::default();
        e.pairs.push(("classname".to_owned(), orphan_class(l, &brushes).to_owned()));
        e.pairs.push(("model".to_owned(), format!("*{m}")));
        if origin != DVec3::ZERO {
            e.pairs.push(("origin".to_owned(), format!("{} {} {}", origin.x, origin.y, origin.z)));
        }
        out.push((m, e, origin));
    }
    out
}

pub(crate) struct IdGen {
    pub next_object: Id,
    pub next_side: Id,
}

impl IdGen {
    pub(crate) fn object(&mut self) -> Id {
        let id = self.next_object;
        self.next_object = self.next_object.saturating_add(1);
        id
    }

    pub(crate) fn side(&mut self) -> Id {
        let id = self.next_side;
        self.next_side = self.next_side.saturating_add(1);
        id
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SideRecord {
    pub id: Id,
    pub normal: DVec3,
    pub dist: f64,
    pub polygon: Vec<DVec3>,
}

pub(crate) fn snap(x: f64) -> f64 {
    let r = x.round();
    let v = if (x - r).abs() < 0.01 { r } else { (x * 1000.0).round() / 1000.0 };
    if v == 0.0 { 0.0 } else { v }
}

pub(crate) fn snap_vec(v: DVec3) -> DVec3 {
    DVec3::new(snap(v.x), snap(v.y), snap(v.z))
}

pub(crate) fn plane_points(polygon: &[DVec3], normal: DVec3) -> Option<[DVec3; 3]> {
    let n = polygon.len();
    if n < 3 {
        return None;
    }
    let p0 = polygon[0];
    let mut best = (0.0, 1, 2);
    for i in 1..n {
        for j in i + 1..n {
            let area = (polygon[i] - p0).cross(polygon[j] - p0).length();
            if area > best.0 {
                best = (area, i, j);
            }
        }
    }
    if best.0 <= 1e-9 {
        return None;
    }
    let (a, b, c) = (polygon[best.2], polygon[best.1], p0);
    let got = (a - b).cross(c - b);
    if got.dot(normal) >= 0.0 { Some([a, b, c]) } else { Some([c, b, a]) }
}

pub(crate) fn tex_axes(vecs: &[[f64; 4]; 2], offset: DVec3) -> (TexAxis, TexAxis) {
    let axis = |v: &[f64; 4], fallback: DVec3| {
        let dir = DVec3::new(v[0], v[1], v[2]);
        let len = dir.length();
        if !len.is_finite() || len < 1e-9 {
            return TexAxis { axis: fallback, shift: 0.0, scale: 0.25 };
        }
        let shift = v[3] - dir.dot(offset);
        TexAxis { axis: snap_axis(dir / len), shift: clean(shift), scale: clean(1.0 / len) }
    };
    (axis(&vecs[0], DVec3::X), axis(&vecs[1], DVec3::NEG_Y))
}

fn snap_axis(v: DVec3) -> DVec3 {
    let f = |x: f64| {
        let r = x.round();
        if (x - r).abs() < 1e-5 { r } else { (x * 1e6).round() / 1e6 }
    };
    DVec3::new(f(v.x), f(v.y), f(v.z))
}

fn clean(x: f64) -> f64 {
    let r = (x * 1e4).round() / 1e4;
    if (x - r).abs() < 1e-4 { if r == 0.0 { 0.0 } else { r } } else { x }
}

pub(crate) struct BuiltSolid {
    pub solid: Solid,
    pub sides: Vec<SideRecord>,
    pub renamed_materials: usize,
}

impl BuiltSolid {
    pub(crate) fn assign_ids(&mut self, ids: &mut IdGen) {
        self.solid.id = ids.object();
        for (side, rec) in self.solid.sides.iter_mut().zip(self.sides.iter_mut()) {
            side.id = ids.side();
            rec.id = side.id;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SideTex {
    Info(i32),
    Named(&'static str),
}

pub(crate) fn side_material(l: &Lumps, texinfo: i32, map_name: &str) -> (String, bool) {
    let name = l.texinfo_name(texinfo);
    if name.is_empty() {
        let flags = usize::try_from(texinfo).ok().and_then(|i| l.texinfo.get(i)).map_or(0, |t| t.flags);
        let tool = if flags & surf::SKY != 0 {
            "TOOLS/TOOLSSKYBOX"
        } else if flags & surf::HINT != 0 {
            "TOOLS/TOOLSHINT"
        } else if flags & surf::SKIP != 0 {
            "TOOLS/TOOLSSKIP"
        } else {
            "TOOLS/TOOLSNODRAW"
        };
        return (tool.to_owned(), false);
    }
    original_material(name, map_name)
}

pub(crate) fn world_axes(normal: DVec3) -> (TexAxis, TexAxis) {
    let a = normal.abs();
    let (u, v) = if a.z >= a.x && a.z >= a.y {
        (DVec3::X, DVec3::NEG_Y)
    } else if a.x >= a.y {
        (DVec3::Y, DVec3::NEG_Z)
    } else {
        (DVec3::X, DVec3::NEG_Z)
    };
    (TexAxis { axis: u, shift: 0.0, scale: 0.25 }, TexAxis { axis: v, shift: 0.0, scale: 0.25 })
}

pub(crate) fn make_side(
    l: &Lumps,
    points: [DVec3; 3],
    normal: DVec3,
    tex: SideTex,
    offset: DVec3,
    map_name: &str,
) -> (Side, bool) {
    let (material, renamed, axes, lightmap_scale) = match tex {
        SideTex::Info(texinfo) => {
            let (material, renamed) = side_material(l, texinfo, map_name);
            let info = usize::try_from(texinfo).ok().and_then(|i| l.texinfo.get(i));
            let axes = info.map_or_else(|| world_axes(normal), |t| tex_axes(&t.vecs, offset));
            let scale =
                info.map(|t| t.lightmap_scale).filter(|s| (1.0..=1024.0).contains(s)).map_or(16, |s| s.round() as i32);
            (material, renamed, axes, scale)
        }
        SideTex::Named(name) => (name.to_owned(), false, world_axes(normal), 16),
    };
    let side = Side {
        id: 0,
        plane: points,
        material,
        uaxis: axes.0,
        vaxis: axes.1,
        rotation: 0.0,
        lightmap_scale,
        smoothing_groups: 0,
        ..Side::default()
    };
    (side, renamed)
}

pub(crate) fn build_brush(l: &Lumps, index: usize, offset: DVec3, map_name: &str) -> Option<BuiltSolid> {
    let brush = l.brushes.get(index)?;
    let first = usize::try_from(brush.firstside).ok()?;
    let count = usize::try_from(brush.numsides).ok()?.min(256);
    let mut planes = Vec::with_capacity(count);
    let mut texs = Vec::with_capacity(count);
    for k in first..first.checked_add(count)? {
        let side = l.brushsides.get(k)?;
        if side.bevel {
            continue;
        }
        let p = l.plane(i64::from(side.planenum))?;
        let plane = GeomPlane::new(p.normal, p.dist + p.normal.dot(offset))?;
        planes.push(plane);
        texs.push(SideTex::Info(i32::from(side.texinfo)));
    }
    build_from_planes(l, &planes, &texs, offset, map_name)
}

pub(crate) fn build_from_planes(
    l: &Lumps,
    planes: &[GeomPlane],
    texs: &[SideTex],
    offset: DVec3,
    map_name: &str,
) -> Option<BuiltSolid> {
    if planes.len() < 4 {
        return None;
    }
    let geom = hp_geom::build_solid_from_planes(planes);
    if !geom.valid {
        return None;
    }
    for snapped in [true, false] {
        let mut sides = Vec::with_capacity(geom.faces.len());
        let mut records = Vec::with_capacity(geom.faces.len());
        let mut renamed_materials = 0;
        for face in &geom.faces {
            let polygon: Vec<DVec3> =
                if snapped { face.vertices.iter().map(|&v| snap_vec(v)).collect() } else { face.vertices.clone() };
            let Some(mut points) = plane_points(&polygon, face.plane.normal) else { continue };
            let n = face.plane.normal;
            if n.abs().max_element() < 1.0 - 1e-9 {
                points = points.map(|p| p - n * (n.dot(p) - face.plane.dist));
            }
            let tex = texs.get(face.side_index).copied().unwrap_or(SideTex::Named("TOOLS/TOOLSNODRAW"));
            let (side, renamed) = make_side(l, points, face.plane.normal, tex, offset, map_name);
            renamed_materials += usize::from(renamed);
            records.push(SideRecord { id: 0, normal: face.plane.normal, dist: face.plane.dist, polygon });
            sides.push(side);
        }
        if sides.len() >= 4 && hp_geom::validate_solid(&sides).is_ok() {
            let solid = Solid { id: 0, sides, ..Solid::default() };
            return Some(BuiltSolid { solid, sides: records, renamed_materials });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_points_follow_the_vmf_convention() {
        let poly = [
            DVec3::new(0.0, 0.0, 64.0),
            DVec3::new(64.0, 0.0, 64.0),
            DVec3::new(64.0, 64.0, 64.0),
            DVec3::new(0.0, 64.0, 64.0),
        ];
        let p = plane_points(&poly, DVec3::Z).expect("points");
        let side = Side { plane: p, ..Side::default() };
        assert!((side.normal() - DVec3::Z).length() < 1e-9);
        assert!((side.plane_distance() - 64.0).abs() < 1e-9);
        assert!(plane_points(&poly[..2], DVec3::Z).is_none());
    }

    #[test]
    fn snapping_removes_float_noise_only() {
        assert_eq!(snap(127.999_98), 128.0);
        assert_eq!(snap(-0.000_01), 0.0);
        assert_eq!(snap(12.345_678), 12.346);
        assert_eq!(snap(0.5), 0.5);
    }

    #[test]
    fn texture_axes_from_texel_vectors() {
        let vecs = [[4.0, 0.0, 0.0, 16.0], [0.0, -4.0, 0.0, 0.0]];
        let (u, v) = tex_axes(&vecs, DVec3::new(32.0, 0.0, 0.0));
        assert_eq!(u.axis, DVec3::X);
        assert_eq!(u.scale, 0.25);
        assert_eq!(u.shift, 16.0 - 128.0);
        assert_eq!(v.axis, DVec3::NEG_Y);
        let (u, _) = tex_axes(&[[0.0; 4], [0.0; 4]], DVec3::ZERO);
        assert_eq!(u.axis, DVec3::X);
    }
}
