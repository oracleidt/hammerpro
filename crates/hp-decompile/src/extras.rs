use std::collections::HashMap;

use glam::DVec3;
use hp_geom::Plane as GeomPlane;
use hp_vmf::{DispInfo, Entity, Id, fmt_vec3};

use crate::lumps::Lumps;
use crate::solids::{BuiltSolid, SideRecord, SideTex, build_from_planes, snap, snap_vec};

const DISP_BRUSH_DEPTH: f64 = 8.0;

const MAX_DISP_POWER: i32 = 4;

pub(crate) struct DispSolid {
    pub built: BuiltSolid,
    pub side: usize,
    pub face: usize,
}

pub(crate) fn face_plane(l: &Lumps, face: &crate::lumps::Face) -> Option<GeomPlane> {
    let p = l.plane(i64::from(face.planenum))?;
    let (n, d) = if face.side != 0 { (-p.normal, -p.dist) } else { (p.normal, p.dist) };
    GeomPlane::new(n, d)
}

pub(crate) fn displacement(l: &Lumps, index: usize, map_name: &str) -> Option<DispSolid> {
    let d = l.dispinfo.get(index)?;
    if !(2..=MAX_DISP_POWER).contains(&d.power) {
        return None;
    }
    let face_index = usize::from(d.map_face);
    let face = l.faces.get(face_index)?;
    let polygon: Vec<DVec3> = l.face_vertices(face).into_iter().map(snap_vec).collect();
    if polygon.len() != 4 {
        return None;
    }
    let top = face_plane(l, face)?;
    let n = top.normal;
    let center = polygon.iter().copied().sum::<DVec3>() / 4.0;
    let mut planes = vec![top, GeomPlane::new(-n, -(top.dist - DISP_BRUSH_DEPTH))?];
    for i in 0..4 {
        let a = polygon[i];
        let b = polygon[(i + 1) % 4];
        let mut wall = GeomPlane::from_normal_point((b - a).cross(n), a)?;
        if wall.distance(center) > 0.0 {
            wall = wall.flip();
        }
        planes.push(wall);
    }
    let mut texs = vec![SideTex::Info(i32::from(face.texinfo))];
    texs.extend([SideTex::Named("TOOLS/TOOLSNODRAW"); 5]);
    let mut built = build_from_planes(l, &planes, &texs, DVec3::ZERO, map_name)?;
    let side = built.sides.iter().position(|r| r.normal.dot(n) > 0.999)?;
    let row = (1usize << d.power) + 1;
    let count = row * row;
    let tris = 2 * (row - 1) * (row - 1);
    let start = usize::try_from(d.vert_start).ok()?;
    let verts = l.dispverts.get(start..start.checked_add(count)?)?;
    let mut info = DispInfo::new_flat(d.power as u32, snap_vec(d.start), n);
    for (k, v) in verts.iter().enumerate() {
        let len = v.vector.length();
        if len.is_finite() && len > 1e-6 && v.dist.is_finite() {
            info.normals[k] = v.vector / len;
            info.distances[k] = v.dist;
        } else {
            info.normals[k] = n;
            info.distances[k] = 0.0;
        }
        info.alphas[k] = if v.alpha.is_finite() { v.alpha.clamp(0.0, 255.0) } else { 0.0 };
    }
    if let Some(tags) = usize::try_from(d.tri_start).ok().and_then(|s| l.disptris.get(s..s.checked_add(tris)?)) {
        info.triangle_tags = vmf_triangle_tags(tags, &polygon, &info);
    }
    info.allowed_verts = d.allowed_verts.iter().map(|&v| v as i32).collect();
    if d.min_tess < 0 {
        info.flags = d.min_tess & 0x0E;
    }
    if let Some(blend) = d.multiblend_start.and_then(|s| l.dispmultiblend.get(s..s.checked_add(count)?)) {
        let mut mb = hp_vmf::multiblend::MultiBlend::new(count);
        let finite = |v: f32| if v.is_finite() { f64::from(v) } else { 0.0 };
        for (k, e) in blend.iter().enumerate() {
            mb.weights[k] = std::array::from_fn(|c| finite(e[c]));
            mb.alphas[k] = std::array::from_fn(|c| finite(e[4 + c]));
            for (ch, colors) in mb.colors.iter_mut().enumerate() {
                colors[k] = std::array::from_fn(|c| finite(e[8 + ch * 3 + c]));
            }
        }
        mb.write_to(&mut info);
    }
    built.solid.sides[side].disp = Some(Box::new(info));
    Some(DispSolid { built, side, face: face_index })
}

fn vmf_triangle_tags(compiled: &[u16], polygon: &[DVec3], info: &DispInfo) -> Vec<i32> {
    let ccw: Vec<DVec3> = polygon.iter().rev().copied().collect();
    match hp_geom::build_disp(&ccw, info) {
        Some(mesh) => hp_geom::vmf_tags_from_compiled(compiled, &mesh),
        None => Vec::new(),
    }
}

pub(crate) fn static_props(bsp: &hp_bsp::Bsp, warnings: &mut Vec<String>) -> Vec<Entity> {
    let lumps = match bsp.game_lumps() {
        Ok(g) => g,
        Err(e) => {
            warnings.push(format!("the game lumps could not be read: {e}"));
            return Vec::new();
        }
    };
    let Some(lump) = lumps.get(hp_bsp::gamelump::id::STATIC_PROPS) else { return Vec::new() };
    let data = match lump.data() {
        Ok(d) => d,
        Err(e) => {
            warnings.push(format!("the static props could not be read: {e}"));
            return Vec::new();
        }
    };
    match parse_static_props(&data, lump.version) {
        Some(props) => props,
        None => {
            warnings.push(format!("the static prop lump (version {}) could not be read", lump.version));
            Vec::new()
        }
    }
}

fn rd_i32(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn num(v: f64) -> String {
    let v = snap(v);
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") }
}

pub(crate) fn parse_static_props(data: &[u8], version: u16) -> Option<Vec<Entity>> {
    let mut at = 0usize;
    let names = usize::try_from(rd_i32(data, at)?).ok()?;
    at += 4;
    if names.checked_mul(128)? > data.len() {
        return None;
    }
    let mut models = Vec::with_capacity(names);
    for _ in 0..names {
        let raw = data.get(at..at + 128)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(128);
        models.push(String::from_utf8_lossy(&raw[..end]).replace('\\', "/"));
        at += 128;
    }
    let leaves = usize::try_from(rd_i32(data, at)?).ok()?;
    at = at.checked_add(4)?.checked_add(leaves.checked_mul(2)?)?;
    let count = usize::try_from(rd_i32(data, at)?).ok()?;
    at += 4;
    if count == 0 {
        return Some(Vec::new());
    }
    let size = data.len().checked_sub(at)? / count;
    if size < 56 {
        return None;
    }
    use hp_bsp::staticprops::{StaticPropFormat as F, StaticPropRecord};
    let format = F::detect_in(version, size, None, data.get(at..at + size));
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let b = data.get(at + i * size..at + (i + 1) * size)?;
        let r = StaticPropRecord::read(format.unwrap_or(F::V4), b)?;
        let v3 = |a: [f32; 3]| DVec3::new(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]));
        let flags = r.flags;
        let mut e = Entity { classname: "prop_static".to_owned(), ..Entity::default() };
        let model = models.get(usize::from(r.prop_type)).cloned().unwrap_or_default();
        e.set("angles", fmt_vec3(snap_vec(v3(r.angles))));
        e.set("disableshadows", if flags & 0x10 != 0 { "1" } else { "0" });
        e.set("fademaxdist", num(f64::from(r.fade_max)));
        e.set("fademindist", num(f64::from(r.fade_min)));
        if format.is_some_and(|f| f != F::V4) {
            e.set("fadescale", num(f64::from(r.forced_fade_scale)));
        }
        if flags & 0x08 != 0 {
            e.set("ignorenormals", "1");
        }
        if flags & 0x20 != 0 {
            e.set("screenspacefade", "1");
        }
        if flags & 0x40 != 0 {
            e.set("disablevertexlighting", "1");
        }
        if flags & 0x80 != 0 {
            e.set("disableselfshadowing", "1");
        }
        e.set("model", model);
        let has_color =
            matches!(format, Some(F::V7L4d | F::V8 | F::V9 | F::V10Csgo | F::V11Lite | F::V11BlackMesa | F::V11Csgo));
        if has_color {
            let c = r.diffuse_modulation;
            e.set("rendercolor", format!("{} {} {}", c[0], c[1], c[2]));
            e.set("renderamt", c[3].to_string());
        }
        if !r.targetname.is_empty() {
            e.set("targetname", r.targetname.clone());
        }
        if format == Some(F::V11Csgo) && r.uniform_scale != 1.0 {
            e.set("uniformscale", num(f64::from(r.uniform_scale)));
        }
        e.set("skin", r.skin.to_string());
        e.set("solid", r.solid.to_string());
        e.set("origin", fmt_vec3(snap_vec(v3(r.origin))));
        out.push(e);
    }
    Some(out)
}

pub(crate) fn cubemaps(bsp: &hp_bsp::Bsp) -> Vec<Entity> {
    bsp.cubemaps()
        .unwrap_or_default()
        .into_iter()
        .map(|(p, size)| {
            let mut e = Entity { classname: "env_cubemap".to_owned(), ..Entity::default() };
            e.set("cubemapsize", size.clamp(0, 16).to_string());
            e.set("origin", format!("{} {} {}", p[0], p[1], p[2]));
            e
        })
        .collect()
}

pub(crate) struct SideIndex {
    by_plane: HashMap<(i64, i64, i64, i64), Vec<SideRecord>>,
    by_face: HashMap<usize, Id>,
}

fn plane_key(n: DVec3, d: f64) -> (i64, i64, i64, i64) {
    ((n.x * 100.0).round() as i64, (n.y * 100.0).round() as i64, (n.z * 100.0).round() as i64, (d / 4.0).round() as i64)
}

fn inside(polygon: &[DVec3], normal: DVec3, p: DVec3) -> bool {
    let n = polygon.len();
    n >= 3
        && (0..n).all(|i| {
            let a = polygon[i];
            let b = polygon[(i + 1) % n];
            (b - a).cross(p - a).dot(normal) >= -0.1
        })
}

impl SideIndex {
    pub(crate) fn new() -> Self {
        Self { by_plane: HashMap::new(), by_face: HashMap::new() }
    }

    pub(crate) fn add(&mut self, rec: &SideRecord) {
        self.by_plane.entry(plane_key(rec.normal, rec.dist)).or_default().push(rec.clone());
    }

    pub(crate) fn add_face(&mut self, face: usize, side: Id) {
        self.by_face.insert(face, side);
    }

    pub(crate) fn side_of_face(&self, l: &Lumps, index: usize, offset: DVec3) -> Option<Id> {
        if let Some(&id) = self.by_face.get(&index) {
            return Some(id);
        }
        let face = l.faces.get(index)?;
        let plane = face_plane(l, face)?;
        let dist = plane.dist + plane.normal.dot(offset);
        let poly: Vec<DVec3> = l.face_vertices(face).into_iter().map(|v| v + offset).collect();
        if poly.is_empty() {
            return None;
        }
        let center = poly.iter().copied().sum::<DVec3>() / poly.len() as f64;
        let key = plane_key(plane.normal, dist);
        let mut best: Option<(f64, Id)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for dd in -1..=1 {
                        let k = (key.0 + dx, key.1 + dy, key.2 + dz, key.3 + dd);
                        for rec in self.by_plane.get(&k).into_iter().flatten() {
                            if rec.normal.dot(plane.normal) < 0.999 || (rec.dist - dist).abs() > 0.5 {
                                continue;
                            }
                            if inside(&rec.polygon, rec.normal, center) {
                                return Some(rec.id);
                            }
                            let c = rec.polygon.iter().copied().sum::<DVec3>() / rec.polygon.len().max(1) as f64;
                            let d2 = c.distance_squared(center);
                            if best.is_none_or(|(b, _)| d2 < b) {
                                best = Some((d2, rec.id));
                            }
                        }
                    }
                }
            }
        }
        best.map(|(_, id)| id)
    }
}

pub(crate) fn face_models(l: &Lumps) -> Vec<Option<usize>> {
    let mut out = vec![None; l.faces.len()];
    for (m, model) in l.models.iter().enumerate() {
        let (Ok(first), Ok(n)) = (usize::try_from(model.firstface), usize::try_from(model.numfaces)) else { continue };
        for f in first..first.saturating_add(n).min(out.len()) {
            out[f].get_or_insert(m);
        }
    }
    out
}

pub(crate) fn overlays(
    l: &Lumps,
    sides: &SideIndex,
    model_offsets: &HashMap<usize, DVec3>,
    map_name: &str,
) -> (Vec<Entity>, usize) {
    let models = face_models(l);
    let mut lost = 0;
    let mut out = Vec::with_capacity(l.overlays.len());
    for o in &l.overlays {
        let mut ids: Vec<Id> = Vec::new();
        for &f in &o.faces {
            let Ok(f) = usize::try_from(f) else { continue };
            let offset =
                models.get(f).copied().flatten().and_then(|m| model_offsets.get(&m)).copied().unwrap_or_default();
            if let Some(id) = sides.side_of_face(l, f, offset)
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            lost += 1;
        }
        let (material, _) = crate::solids::side_material(l, i32::from(o.texinfo), map_name);
        let basis_u = DVec3::new(o.uv_points[0].z, o.uv_points[1].z, o.uv_points[2].z);
        let normal = o.normal;
        let basis_v = normal.cross(basis_u);
        let mut e = Entity { classname: "info_overlay".to_owned(), ..Entity::default() };
        e.set("angles", "0 0 0");
        e.set("BasisNormal", fmt_vec3(round_dir(normal)));
        e.set("BasisOrigin", fmt_vec3(snap_vec(o.origin)));
        e.set("BasisU", fmt_vec3(round_dir(basis_u)));
        e.set("BasisV", fmt_vec3(round_dir(basis_v)));
        e.set("EndU", num(o.u[1]));
        e.set("EndV", num(o.v[1]));
        e.set("material", material);
        e.set("RenderOrder", o.render_order.to_string());
        e.set("sides", ids.iter().map(Id::to_string).collect::<Vec<_>>().join(" "));
        e.set("StartU", num(o.u[0]));
        e.set("StartV", num(o.v[0]));
        for (k, p) in o.uv_points.iter().enumerate() {
            e.set(&format!("uv{k}"), fmt_vec3(DVec3::new(snap(p.x), snap(p.y), 0.0)));
        }
        e.set("origin", fmt_vec3(snap_vec(o.origin)));
        out.push(e);
    }
    (out, lost)
}

fn round_dir(v: DVec3) -> DVec3 {
    let f = |x: f64| {
        let r = x.round();
        if (x - r).abs() < 1e-5 { r + 0.0 } else { (x * 1e6).round() / 1e6 }
    };
    DVec3::new(f(v.x), f(v.y), f(v.z))
}

pub(crate) fn occluder(l: &Lumps, index: usize, map_name: &str) -> Option<BuiltSolid> {
    let o = l.occluders.get(index)?;
    let mut planes = Vec::new();
    let mut texs = Vec::new();
    for (planenum, _) in &o.polys {
        let p = l.plane(i64::from(*planenum))?;
        planes.push(GeomPlane::new(p.normal, p.dist)?);
        texs.push(SideTex::Named("TOOLS/TOOLSOCCLUDER"));
    }
    let (lo, hi) = (o.mins, o.maxs);
    for (n, d) in [
        (DVec3::X, hi.x),
        (DVec3::NEG_X, -lo.x),
        (DVec3::Y, hi.y),
        (DVec3::NEG_Y, -lo.y),
        (DVec3::Z, hi.z),
        (DVec3::NEG_Z, -lo.z),
    ] {
        if planes.iter().any(|p: &GeomPlane| p.normal.dot(n) > 0.999 && (p.dist - d).abs() < 0.5) {
            continue;
        }
        planes.push(GeomPlane::new(n, d)?);
        texs.push(SideTex::Named("TOOLS/TOOLSNODRAW"));
    }
    build_from_planes(l, &planes, &texs, DVec3::ZERO, map_name)
}

pub(crate) fn portal_center(l: &Lumps, key: u16) -> Option<DVec3> {
    let p = l.areaportals.iter().find(|p| p.key == key && p.num_verts > 0)?;
    let first = usize::from(p.first_vert);
    let verts = l.clipportalverts.get(first..first + usize::from(p.num_verts))?;
    Some(verts.iter().copied().sum::<DVec3>() / verts.len() as f64)
}

pub(crate) fn touching_groups(bounds: &[(DVec3, DVec3)]) -> Vec<Vec<usize>> {
    const CELL: f64 = 256.0;
    const MAX_CELLS: i64 = 512;
    let n = bounds.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let touch = |a: &(DVec3, DVec3), b: &(DVec3, DVec3)| {
        let e = 0.5;
        a.0.x <= b.1.x + e
            && b.0.x <= a.1.x + e
            && a.0.y <= b.1.y + e
            && b.0.y <= a.1.y + e
            && a.0.z <= b.1.z + e
            && b.0.z <= a.1.z + e
    };
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    let mut large = Vec::new();
    for (i, (lo, hi)) in bounds.iter().enumerate() {
        let c0 = (*lo / CELL).floor();
        let c1 = (*hi / CELL).floor();
        let span = (c1 - c0).max(DVec3::ZERO) + DVec3::ONE;
        if !span.is_finite() || span.x * span.y * span.z > MAX_CELLS as f64 {
            large.push(i);
            continue;
        }
        for x in c0.x as i64..=c1.x as i64 {
            for y in c0.y as i64..=c1.y as i64 {
                for z in c0.z as i64..=c1.z as i64 {
                    grid.entry((x, y, z)).or_default().push(i);
                }
            }
        }
    }
    for cell in grid.values() {
        for (k, &i) in cell.iter().enumerate() {
            for &j in &cell[k + 1..] {
                if touch(&bounds[i], &bounds[j]) {
                    let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                    if a != b {
                        parent[a.max(b)] = a.min(b);
                    }
                }
            }
        }
    }
    for &i in &large {
        for j in 0..n {
            if i != j && touch(&bounds[i], &bounds[j]) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut out: Vec<Vec<usize>> = groups.into_values().collect();
    out.sort_by_key(|g| g[0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_triangle_tags_become_hammer_tags() {
        let polygon = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 128.0, 0.0),
            DVec3::new(128.0, 128.0, 0.0),
            DVec3::new(128.0, 0.0, 0.0),
        ];
        let info = DispInfo::new_flat(2, polygon[0], DVec3::Z);
        assert_eq!(vmf_triangle_tags(&[6; 32], &polygon, &info), vec![9; 32]);
        let off = vmf_triangle_tags(&[0; 32], &polygon, &info);
        assert_eq!(off, vec![9 | 2 | 16; 32]);
        assert_eq!(vmf_triangle_tags(&[2; 32], &polygon, &info), vec![9 | 16; 32]);
        let ccw: Vec<DVec3> = polygon.iter().rev().copied().collect();
        let mesh = hp_geom::build_disp(&ccw, &info).expect("mesh");
        for compiled in [6u16, 0, 2, 4] {
            let mut back = info.clone();
            back.triangle_tags = vmf_triangle_tags(&[compiled; 32], &polygon, &info);
            assert_eq!(hp_geom::compiled_disp_tags(&back, &mesh), vec![compiled; 32], "{compiled}");
        }
    }

    #[test]
    fn touching_brushes_share_a_group() {
        let b = |x: f64| (DVec3::new(x, 0.0, 0.0), DVec3::new(x + 64.0, 64.0, 64.0));
        let huge = (DVec3::splat(-100_000.0), DVec3::splat(-99_000.0));
        let groups = touching_groups(&[b(0.0), b(64.0), b(1000.0), b(128.0), huge]);
        assert_eq!(groups, vec![vec![0, 1, 3], vec![2], vec![4]]);
    }

    #[test]
    fn static_prop_lump_versions() {
        let mut data = Vec::new();
        data.extend_from_slice(&1i32.to_le_bytes());
        let mut name = [0u8; 128];
        name[..20].copy_from_slice(b"models/props/a01.mdl");
        data.extend_from_slice(&name);
        data.extend_from_slice(&1i32.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&2i32.to_le_bytes());
        for i in 0..2 {
            let mut p = vec![0u8; 76];
            p[0..4].copy_from_slice(&(64.0f32 * i as f32).to_le_bytes());
            p[16..20].copy_from_slice(&90.0f32.to_le_bytes());
            p[30] = 6;
            p[31] = 0x10;
            p[32..36].copy_from_slice(&2i32.to_le_bytes());
            p[56..60].copy_from_slice(&1.0f32.to_le_bytes());
            p[64..68].copy_from_slice(&[255, 128, 0, 200]);
            data.extend_from_slice(&p);
        }
        let props = parse_static_props(&data, 10).expect("parses");
        assert_eq!(props.len(), 2);
        assert_eq!(props[1].get("origin"), Some("64 0 0"));
        assert_eq!(props[1].get("angles"), Some("0 90 0"));
        assert_eq!(props[0].get("model"), Some("models/props/a01.mdl"));
        assert_eq!(props[0].get("skin"), Some("2"));
        assert_eq!(props[0].get("solid"), Some("6"));
        assert_eq!(props[0].get("disableshadows"), Some("1"));
        assert_eq!(props[0].get("rendercolor"), Some("255 128 0"));
        assert_eq!(props[0].get("renderamt"), Some("200"));
        for n in 0..data.len() {
            let _ = parse_static_props(&data[..n], 10);
        }
        let mut bad = data.clone();
        bad[0..4].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(parse_static_props(&bad, 10).is_none());
    }

    #[test]
    fn static_props_of_every_branch() {
        use hp_bsp::staticprops::{StaticPropFormat, StaticPropLump, StaticPropRecord};
        let record = StaticPropRecord {
            origin: [8.0, 0.0, 0.0],
            flags: 0x10 | 0x100,
            diffuse_modulation: [1, 2, 3, 4],
            targetname: "boat".to_owned(),
            uniform_scale: 2.0,
            ..StaticPropRecord::default()
        };
        let lump = StaticPropLump { names: vec!["models/a.mdl".to_owned()], leaves: vec![0], props: vec![record] };
        for f in StaticPropFormat::ALL {
            let props = parse_static_props(&lump.to_bytes(f), f.version()).expect("parses");
            assert_eq!(props[0].get("origin"), Some("8 0 0"), "{f:?}");
            assert_eq!(props[0].get("disableshadows"), Some("1"), "{f:?}");
            let color = matches!(
                f,
                StaticPropFormat::V7L4d
                    | StaticPropFormat::V8
                    | StaticPropFormat::V9
                    | StaticPropFormat::V10Csgo
                    | StaticPropFormat::V11Lite
                    | StaticPropFormat::V11BlackMesa
                    | StaticPropFormat::V11Csgo
            );
            assert_eq!(props[0].get("rendercolor"), color.then_some("1 2 3"), "{f:?}");
            let named = matches!(f, StaticPropFormat::V5Ship | StaticPropFormat::V6Bgt);
            assert_eq!(props[0].get("targetname"), named.then_some("boat"), "{f:?}");
            assert_eq!(props[0].get("uniformscale"), (f == StaticPropFormat::V11Csgo).then_some("2"), "{f:?}");
        }
    }
}
