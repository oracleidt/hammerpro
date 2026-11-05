use glam::{DMat3, DVec3};

use crate::emit::Buf;
use crate::phys::surface::TraceSurface;
use crate::planes::PlaneSet;
use crate::tree::Tree;

pub trait ModelSource: Sync {
    fn bounds(&self, path: &str) -> Option<(DVec3, DVec3)>;

    fn loads(&self, _path: &str) -> Option<bool> {
        None
    }

    fn is_static_prop(&self, _path: &str) -> Option<bool> {
        None
    }

    fn static_prop_error(&self, _path: &str) -> Option<String> {
        None
    }

    fn hull_center(&self, _path: &str) -> Option<DVec3> {
        None
    }

    fn mesh_points(&self, _path: &str) -> Option<Vec<Vec<DVec3>>> {
        None
    }

    fn read_file(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }

    fn model_check(&self, _path: &str) -> Option<ModelCheck> {
        None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelCheck {
    pub exists: bool,
    pub loads: bool,
    pub has_vvd: bool,
    pub vvd_matches: bool,
    pub has_vtx: bool,
    pub vtx_matches: bool,
    pub has_phy: bool,
    pub phy_matches: bool,
    pub phy_solids: u32,
    pub skins: u32,
    pub bodies: u32,
    pub vertices: u32,
}

pub fn static_prop_error(
    path: &str,
    entity: &str,
    is_static: bool,
    keyvalues: &str,
    allow_dynamic: bool,
) -> Option<String> {
    if allow_dynamic {
        return None;
    }
    if !is_static {
        return Some(format!(
            "Error! To use model \"{path}\"
      with {entity}, it must be compiled with $staticprop!"
        ));
    }
    if keyvalues.trim().is_empty() {
        return None;
    }
    let kv = hp_kv::parse(keyvalues).ok()?;
    let prop_data = kv.child("prop_data").or_else(|| kv.children.iter().find_map(|c| c.child("prop_data")))?;
    let allow = prop_data.get("allowstatic").is_some_and(|v| crate::cubemap::atoi(v) != 0);
    (!allow).then(|| format!("Error! {entity} using model \"{path}\", which must be used on a dynamic entity (i.e. prop_physics). Deleted."))
}

pub struct NoModels;

impl ModelSource for NoModels {
    fn bounds(&self, _path: &str) -> Option<(DVec3, DVec3)> {
        None
    }
}

#[derive(Clone, Debug)]
pub struct StaticProp {
    pub model: String,
    pub origin: DVec3,
    pub angles: DVec3,
    pub solid: u8,
    pub skin: i32,
    pub fade_min: f32,
    pub fade_max: f32,
    pub fade_scale: f32,
    pub flags: u32,
    pub lighting_origin: Option<DVec3>,
    pub extra: PropExtra,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropExtra {
    pub dx_levels: [u16; 2],
    pub system_levels: [u8; 4],
    pub color: [u8; 4],
    pub disable_x360: bool,
    pub flags_ex: u32,
    pub uniform_scale: f32,
    pub targetname: String,
}

impl Default for PropExtra {
    fn default() -> Self {
        Self {
            dx_levels: [0; 2],
            system_levels: [0; 4],
            color: [255; 4],
            disable_x360: false,
            flags_ex: 0,
            uniform_scale: 1.0,
            targetname: String::new(),
        }
    }
}

impl PropExtra {
    pub fn from_entity(e: &hp_vmf::Entity) -> PropExtra {
        let int = |k: &str, max: f64| num(e, k, 0.0).clamp(0.0, max) as u32;
        let mut color = [255u8; 4];
        if let Some(c) = e.get("rendercolor") {
            for (slot, v) in color.iter_mut().zip(c.split_whitespace().take(3)) {
                *slot = v.parse::<f64>().ok().filter(|x| x.is_finite()).map_or(255, |x| x.clamp(0.0, 255.0) as u8);
            }
        }
        color[3] = num(e, "renderamt", 255.0).clamp(0.0, 255.0) as u8;
        let on = |k: &str| e.get(k).is_some_and(|v| crate::cubemap::atoi(v) != 0);
        let mut flags_ex = 0;
        if on("disableshadowdepth") {
            flags_ex |= 0x1;
        }
        if on("enablelightbounce") {
            flags_ex |= 0x4;
        }
        let scale = e
            .get("uniformscale")
            .or_else(|| e.get("modelscale"))
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v > 0.0);
        PropExtra {
            dx_levels: [int("mindxlevel", 65535.0) as u16, int("maxdxlevel", 65535.0) as u16],
            system_levels: [
                int("mincpulevel", 255.0) as u8,
                int("maxcpulevel", 255.0) as u8,
                int("mingpulevel", 255.0) as u8,
                int("maxgpulevel", 255.0) as u8,
            ],
            color,
            disable_x360: on("disableX360"),
            flags_ex,
            uniform_scale: scale.unwrap_or(1.0),
            targetname: e.get("targetname").unwrap_or("").trim().to_owned(),
        }
    }
}

pub mod flag {
    pub const FADES: u32 = 0x1;
    pub const USE_LIGHTING_ORIGIN: u32 = 0x2;
    pub const IGNORE_NORMALS: u32 = 0x8;
    pub const NO_SHADOW: u32 = 0x10;
    pub const SCREEN_SPACE_FADE: u32 = 0x20;
    pub const NO_PER_VERTEX_LIGHTING: u32 = 0x40;
    pub const NO_SELF_SHADOWING: u32 = 0x80;
    pub const NO_PER_TEXEL_LIGHTING: u32 = 0x100;
}

fn num(e: &hp_vmf::Entity, k: &str, default: f64) -> f64 {
    e.get(k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
}

fn vec(s: Option<&str>) -> Option<DVec3> {
    let v: Vec<f64> = s?.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.len() == 3 && v.iter().all(|x| x.is_finite())).then(|| DVec3::new(v[0], v[1], v[2]))
}

impl StaticProp {
    pub fn from_entity(e: &hp_vmf::Entity, lighting: &dyn Fn(&str) -> Option<DVec3>) -> Option<StaticProp> {
        let model = e.get("model")?.trim().replace('\\', "/");
        if model.is_empty() {
            return None;
        }
        let origin = vec(e.get("origin")).unwrap_or(DVec3::ZERO);
        let angles = vec(e.get("angles")).unwrap_or(DVec3::ZERO);
        let on = |k: &str| e.get(k).is_some_and(|v| v.trim() == "1");
        let fade_max = num(e, "fademaxdist", 0.0).max(0.0) as f32;
        let fade_min = match num(e, "fademindist", -1.0) as f32 {
            m if m < 0.0 => fade_max,
            m => m,
        };
        let lighting_origin = e.get("lightingorigin").map(str::trim).filter(|s| !s.is_empty()).and_then(lighting);
        let mut flags = 0;
        if fade_max > 0.0 {
            flags |= flag::FADES;
        }
        if lighting_origin.is_some() {
            flags |= flag::USE_LIGHTING_ORIGIN;
        }
        if on("ignorenormals") {
            flags |= flag::IGNORE_NORMALS;
        }
        if on("disableshadows") {
            flags |= flag::NO_SHADOW;
        }
        if on("screenspacefade") {
            flags |= flag::SCREEN_SPACE_FADE;
        }
        if on("disablevertexlighting") {
            flags |= flag::NO_PER_VERTEX_LIGHTING;
        }
        if on("disableselfshadowing") {
            flags |= flag::NO_SELF_SHADOWING;
        }
        Some(StaticProp {
            model,
            origin,
            angles,
            solid: num(e, "solid", 6.0) as u8,
            skin: num(e, "skin", 0.0) as i32,
            fade_min,
            fade_max,
            fade_scale: num(e, "fadescale", 1.0) as f32,
            flags,
            lighting_origin,
            extra: PropExtra::from_entity(e),
        })
    }

    fn rotation(&self) -> DMat3 {
        let a = self.angles;
        DMat3::from_rotation_z(a.y.to_radians())
            * DMat3::from_rotation_y(a.x.to_radians())
            * DMat3::from_rotation_x(a.z.to_radians())
    }

    pub fn world_hulls(&self, meshes: &[Vec<DVec3>]) -> Vec<Vec<DVec3>> {
        let m = self.rotation();
        meshes.iter().map(|mesh| mesh.iter().map(|p| self.origin + m * *p).collect()).collect()
    }

    pub fn world_bounds(&self, models: &dyn ModelSource) -> (DVec3, DVec3) {
        let (lo, hi) = models.bounds(&self.model).unwrap_or((DVec3::splat(-8.0), DVec3::splat(8.0)));
        let m = self.rotation();
        let mut wlo = DVec3::splat(f64::MAX);
        let mut whi = DVec3::splat(f64::MIN);
        for i in 0..8 {
            let c = DVec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            let p = self.origin + m * c;
            wlo = wlo.min(p);
            whi = whi.max(p);
        }
        (wlo, whi)
    }
}

pub fn box_leaves(tree: &Tree, planes: &PlaneSet, lo: DVec3, hi: DVec3) -> Vec<usize> {
    let center = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    let mut out = Vec::new();
    let mut stack = vec![tree.head];
    while let Some(n) = stack.pop() {
        let node = &tree.nodes[n];
        let Some(pl) = node.plane else {
            if node.contents & crate::materials::contents::SOLID == 0 {
                out.push(n);
            }
            continue;
        };
        let p = planes.get(pl);
        let d = p.distance(center);
        let r = p.normal.abs().dot(half);
        if d >= -r {
            stack.push(node.children[0]);
        }
        if d <= r {
            stack.push(node.children[1]);
        }
    }
    out
}

pub enum TreeStep {
    Node { normal: DVec3, dist: f64, children: [usize; 2] },
    Leaf { open: bool },
}

pub fn hull_leaves(head: usize, step: &dyn Fn(usize) -> TreeStep, hulls: &[Vec<DVec3>]) -> Vec<usize> {
    let mut lo = DVec3::splat(f64::MAX);
    let mut hi = DVec3::splat(f64::MIN);
    for p in hulls.iter().flatten() {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let mut out = Vec::new();
    if lo.cmpgt(hi).any() {
        return out;
    }
    let mut path: Vec<(DVec3, f64)> = Vec::new();
    hull_leaves_r(step, head, &mut path, (lo, hi), hulls, &mut out);
    out
}

fn hull_leaves_r(
    step: &dyn Fn(usize) -> TreeStep,
    mut node: usize,
    path: &mut Vec<(DVec3, f64)>,
    (lo, hi): (DVec3, DVec3),
    hulls: &[Vec<DVec3>],
    out: &mut Vec<usize>,
) {
    let depth = path.len();
    loop {
        let (normal, dist, [front, back]) = match step(node) {
            TreeStep::Node { normal, dist, children } => (normal, dist, children),
            TreeStep::Leaf { open } => {
                if open {
                    let cell = convex_points(path);
                    if !cell.is_empty() && hulls.iter().any(|h| !h.is_empty() && convex_overlap(h, &cell)) {
                        out.push(node);
                    }
                }
                path.truncate(depth);
                return;
            }
        };
        let mut cmin = DVec3::ZERO;
        let mut cmax = DVec3::ZERO;
        for i in 0..3 {
            if normal[i] >= 0.0 {
                cmin[i] = lo[i];
                cmax[i] = hi[i];
            } else {
                cmin[i] = hi[i];
                cmax[i] = lo[i];
            }
        }
        if normal.dot(cmax) <= dist {
            path.push((normal, dist));
            node = back;
        } else if normal.dot(cmin) >= dist {
            path.push((-normal, -dist));
            node = front;
        } else {
            path.push((normal, dist));
            hull_leaves_r(step, back, path, (lo, hi), hulls, out);
            let last = path.len() - 1;
            path[last] = (-normal, -dist);
            hull_leaves_r(step, front, path, (lo, hi), hulls, out);
            path.truncate(depth);
            return;
        }
    }
}

fn convex_points(planes: &[(DVec3, f64)]) -> Vec<DVec3> {
    let mut out = Vec::new();
    for (i, &(n, d)) in planes.iter().enumerate() {
        let mut w = Some(crate::winding::Winding::base_for_plane(n, d));
        for (j, &(m, e)) in planes.iter().enumerate() {
            if i == j {
                continue;
            }
            let Some(cur) = w.take() else { break };
            let p = crate::planes::MapPlane { normal: m, dist: e, kind: crate::planes::plane_type(m) };
            w = cur.clip_back(&p, 0.0, true);
        }
        if let Some(w) = w {
            out.extend(w.points);
        }
    }
    out
}

fn convex_overlap(p: &[DVec3], q: &[DVec3]) -> bool {
    const EPS: f64 = 1e-6;
    let support = |d: DVec3| {
        let far = |set: &[DVec3], d: DVec3| {
            set.iter().copied().fold((f64::MIN, DVec3::ZERO), |m, x| if x.dot(d) > m.0 { (x.dot(d), x) } else { m }).1
        };
        far(p, d) - far(q, -d)
    };
    let past = |x: DVec3, d: DVec3| x.dot(d) > EPS * d.length();
    let mut d = p[0] - q[0];
    if d.length_squared() < EPS * EPS {
        d = DVec3::X;
    }
    let mut c = support(d);
    d = -c;
    if d.length_squared() < EPS * EPS {
        return true;
    }
    let mut b = support(d);
    if !past(b, d) {
        return false;
    }
    d = (c - b).cross(-b).cross(c - b);
    if d.length_squared() < EPS * EPS {
        d = (c - b).cross(DVec3::X);
        if d.length_squared() < EPS * EPS {
            d = (c - b).cross(DVec3::NEG_Z);
        }
    }
    let mut a;
    let mut dd = DVec3::ZERO;
    let mut dim = 2;
    for _ in 0..64 {
        a = support(d);
        if !past(a, d) {
            return false;
        }
        dim += 1;
        if dim == 3 {
            let n = (b - a).cross(c - a);
            let ao = -a;
            dim = 2;
            if (b - a).cross(n).dot(ao) > 0.0 {
                c = a;
                d = (b - a).cross(ao).cross(b - a);
            } else if n.cross(c - a).dot(ao) > 0.0 {
                b = a;
                d = (c - a).cross(ao).cross(c - a);
            } else {
                dim = 3;
                if n.dot(ao) > 0.0 {
                    dd = c;
                    c = b;
                    b = a;
                    d = n;
                } else {
                    dd = b;
                    b = a;
                    d = -n;
                }
            }
        } else {
            let abc = (b - a).cross(c - a);
            let acd = (c - a).cross(dd - a);
            let adb = (dd - a).cross(b - a);
            let ao = -a;
            dim = 3;
            if abc.dot(ao) > 0.0 {
                dd = c;
                c = b;
                b = a;
                d = abc;
            } else if acd.dot(ao) > 0.0 {
                b = a;
                d = acd;
            } else if adb.dot(ao) > 0.0 {
                c = dd;
                dd = b;
                b = a;
                d = adb;
            } else {
                return true;
            }
        }
        if d.length_squared() < EPS * EPS {
            return true;
        }
    }
    false
}

pub type LeafCollideFn<'a> = dyn Fn(usize, &[[f32; 4]]) -> Option<std::sync::Arc<TraceSurface>> + 'a;

type LeafTestFn<'a> = dyn FnMut(usize, &[(usize, bool)]) + 'a;

pub type PropLeavesFn<'a> = dyn Fn(&StaticProp, Option<&[Vec<DVec3>]>) -> Vec<u16> + Sync + 'a;

pub fn prop_leaves(
    head: usize,
    step: &dyn Fn(usize) -> TreeStep,
    prop: &TraceSurface,
    origin: [f32; 3],
    angles: [f32; 3],
    leaf_collide: &LeafCollideFn<'_>,
) -> Vec<usize> {
    let (mins, maxs) = crate::phys::surface::placed_aabb(prop, origin, angles);
    let mut out = Vec::new();
    let mut path: Vec<(usize, bool)> = Vec::new();
    let mut test = |leaf: usize, path: &[(usize, bool)]| {
        let planes: Vec<[f32; 4]> = path
            .iter()
            .rev()
            .map(|&(n, front)| {
                let TreeStep::Node { normal, dist, .. } = step(n) else { return [0.0; 4] };
                let s = if front { -1.0f32 } else { 1.0 };
                [s * normal.x as f32, s * normal.y as f32, s * normal.z as f32, s * dist as f32]
            })
            .collect();
        if let Some(c) = leaf_collide(leaf, &planes)
            && crate::phys::surface::trace_collide_start_solid(&c, prop, origin, angles)
        {
            out.push(leaf);
        }
    };
    prop_leaves_r(step, head, &mut path, (mins, maxs), &mut test);
    out
}

fn prop_leaves_r(
    step: &dyn Fn(usize) -> TreeStep,
    mut node: usize,
    path: &mut Vec<(usize, bool)>,
    (mins, maxs): ([f32; 3], [f32; 3]),
    test: &mut LeafTestFn<'_>,
) {
    let depth = path.len();
    loop {
        let (n, d, [front, back]) = match step(node) {
            TreeStep::Node { normal, dist, children } => {
                ([normal.x as f32, normal.y as f32, normal.z as f32], dist as f32, children)
            }
            TreeStep::Leaf { open } => {
                if open {
                    test(node, path);
                }
                path.truncate(depth);
                return;
            }
        };
        let mut far = [0f32; 3];
        let mut near = [0f32; 3];
        for i in 0..3 {
            if n[i] < 0.0 {
                far[i] = mins[i];
                near[i] = maxs[i];
            } else {
                far[i] = maxs[i];
                near[i] = mins[i];
            }
        }
        if (far[0] * n[0] + far[1] * n[1]) + far[2] * n[2] > d {
            if (near[0] * n[0] + near[1] * n[1]) + near[2] * n[2] < d {
                path.push((node, false));
                prop_leaves_r(step, back, path, (mins, maxs), test);
                let last = path.len() - 1;
                path[last] = (node, true);
                prop_leaves_r(step, front, path, (mins, maxs), test);
                path.truncate(depth);
                return;
            }
            path.push((node, true));
            node = front;
        } else {
            path.push((node, false));
            node = back;
        }
    }
}

fn box_leaves_step(head: usize, step: &dyn Fn(usize) -> TreeStep, lo: DVec3, hi: DVec3) -> Vec<usize> {
    let center = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    let mut out = Vec::new();
    let mut stack = vec![head];
    while let Some(n) = stack.pop() {
        match step(n) {
            TreeStep::Leaf { open } => {
                if open {
                    out.push(n);
                }
            }
            TreeStep::Node { normal, dist, children } => {
                let d = normal.dot(center) - dist;
                let r = normal.abs().dot(half);
                if d >= -r {
                    stack.push(children[0]);
                }
                if d <= r {
                    stack.push(children[1]);
                }
            }
        }
    }
    out
}

pub fn sprp_lump(
    props: &[StaticProp],
    models: &dyn ModelSource,
    tree: &Tree,
    planes: &PlaneSet,
    format: hp_bsp::BspFormat,
    leaf_number: &(dyn Fn(usize) -> Option<u16> + Sync),
    warn: &mut dyn FnMut(String),
) -> (Vec<u8>, Vec<usize>) {
    let step = |n: usize| match tree.nodes[n].plane {
        Some(pl) => {
            let (normal, dist) = planes.getf(pl);
            TreeStep::Node { normal: normal.as_dvec3(), dist: f64::from(dist), children: tree.nodes[n].children }
        }
        None => TreeStep::Leaf { open: tree.nodes[n].contents & crate::materials::contents::SOLID == 0 },
    };
    sprp_lump_tree(props, models, tree.head, &step, format, leaf_number, warn)
}

pub fn sprp_lump_tree(
    props: &[StaticProp],
    models: &dyn ModelSource,
    head: usize,
    step: &(dyn Fn(usize) -> TreeStep + Sync),
    format: hp_bsp::BspFormat,
    leaf_number: &(dyn Fn(usize) -> Option<u16> + Sync),
    warn: &mut dyn FnMut(String),
) -> (Vec<u8>, Vec<usize>) {
    use std::sync::{Arc, Mutex, OnceLock};
    type Cell = Arc<OnceLock<Option<Arc<TraceSurface>>>>;
    let cells: Mutex<std::collections::HashMap<usize, Cell>> = Mutex::new(std::collections::HashMap::new());
    let leaf_collide = |leaf: usize, planes: &[[f32; 4]]| -> Option<Arc<TraceSurface>> {
        let cell = cells.lock().map(|mut m| m.entry(leaf).or_default().clone()).ok()?;
        cell.get_or_init(|| {
            let convex = crate::phys::surface::convex_from_planes_full(planes, 0.0)?;
            let collide = crate::phys::surface::convex_to_collide_plain(&[convex])?;
            Some(Arc::new(TraceSurface::new(&collide)))
        })
        .clone()
    };
    let leaves = |p: &StaticProp, model: &ModelData| -> Vec<u16> {
        let found = match model.collide() {
            Some(c) => {
                let v = |x: DVec3| [x.x as f32, x.y as f32, x.z as f32];
                prop_leaves(head, step, c, v(p.origin), v(p.angles), &leaf_collide)
            }
            None => {
                let (lo, hi) = p.world_bounds(models);
                box_leaves_step(head, step, lo, hi)
            }
        };
        found.into_iter().filter_map(leaf_number).collect()
    };
    sprp_build(props, models, true, format, &leaves, warn)
}

pub fn sprp_lump_with(
    props: &[StaticProp],
    models: &dyn ModelSource,
    leaves_of: &PropLeavesFn<'_>,
    warn: &mut dyn FnMut(String),
) -> Vec<u8> {
    let leaves = |p: &StaticProp, model: &ModelData| {
        let hulls = model.meshes.as_deref().map(|m| p.world_hulls(m));
        leaves_of(p, hulls.as_deref())
    };
    sprp_build(props, models, false, hp_bsp::BspFormat::default(), &leaves, warn).0
}

struct ModelData {
    ok: bool,
    refused: Option<String>,
    meshes: Option<Vec<Vec<DVec3>>>,
    collide: std::sync::OnceLock<Option<TraceSurface>>,
}

impl ModelData {
    fn collide(&self) -> Option<&TraceSurface> {
        self.collide
            .get_or_init(|| {
                use rayon::prelude::*;
                let meshes = self.meshes.as_ref()?;
                let convexes: Vec<Vec<u8>> = meshes
                    .par_iter()
                    .filter_map(|m| {
                        let pts: Vec<[f32; 3]> = m.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
                        crate::phys::surface::convex_from_verts_full(&pts)
                    })
                    .collect();
                if convexes.is_empty() {
                    return None;
                }
                let collide = crate::phys::surface::convex_to_collide_plain(&convexes)?;
                Some(TraceSurface::new(&collide))
            })
            .as_ref()
    }
}

fn sprp_build(
    props: &[StaticProp],
    models: &dyn ModelSource,
    collides: bool,
    format: hp_bsp::BspFormat,
    leaves_of: &(dyn Fn(&StaticProp, &ModelData) -> Vec<u16> + Sync),
    warn: &mut dyn FnMut(String),
) -> (Vec<u8>, Vec<usize>) {
    use rayon::prelude::*;
    let mut names: Vec<String> = Vec::new();
    let mut leaves: Vec<u16> = Vec::new();
    let mut records: Vec<u8> = Vec::new();
    let mut count_written = 0usize;
    let mut keys: Vec<String> = Vec::new();
    for p in props {
        let key = p.model.to_ascii_lowercase();
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    let model_of: std::collections::HashMap<&str, usize> =
        keys.iter().enumerate().map(|(i, k)| (k.as_str(), i)).collect();
    let first_name: Vec<&str> = keys
        .iter()
        .map(|k| props.iter().find(|p| p.model.eq_ignore_ascii_case(k)).map_or("", |p| p.model.as_str()))
        .collect();
    let model_data: Vec<ModelData> = first_name
        .par_iter()
        .map(|name| {
            let missing = models.loads(name) == Some(false);
            let refused = if missing { None } else { models.static_prop_error(name) };
            let (ok, meshes) =
                if missing || refused.is_some() { (false, None) } else { (true, models.mesh_points(name)) };
            let data = ModelData { ok, refused, meshes, collide: std::sync::OnceLock::new() };
            if collides && ok {
                data.collide();
            }
            data
        })
        .collect();
    let found_of: Vec<Option<Vec<u16>>> = props
        .par_iter()
        .map(|p| {
            let model = &model_data[model_of[p.model.to_ascii_lowercase().as_str()]];
            model.ok.then(|| leaves_of(p, model))
        })
        .collect();
    let mut reported = vec![false; keys.len()];
    let mut outside = Vec::new();
    for (k, (p, found)) in props.iter().zip(found_of).enumerate() {
        let Some(found) = found else {
            let m = model_of[p.model.to_ascii_lowercase().as_str()];
            if !reported[m] {
                reported[m] = true;
                match &model_data[m].refused {
                    Some(e) => warn(e.clone()),
                    None => warn(format!(
                        "Warning: prop_static model {} cannot be loaded: its props are left out (VBSP)",
                        p.model
                    )),
                }
            }
            continue;
        };
        if found.is_empty() {
            outside.push(k);
            warn(format!(
                "Warning: prop_static {} at ({:.0} {:.0} {:.0}) touches no open leaf (inside solid or outside the map): removed",
                p.model, p.origin.x, p.origin.y, p.origin.z
            ));
            continue;
        }
        let ty = match names.iter().position(|n| n.eq_ignore_ascii_case(&p.model)) {
            Some(i) => i,
            None => {
                names.push(p.model.clone());
                names.len() - 1
            }
        };
        let first = leaves.len();
        leaves.extend(found);
        let count = leaves.len() - first;
        count_written += 1;
        let f3 = |v: DVec3| [v.x as f32, v.y as f32, v.z as f32];
        let x = &p.extra;
        let dx_levels = if format.branch == hp_bsp::Branch::GarrysMod { [0, 0] } else { x.dx_levels };
        let record = hp_bsp::staticprops::StaticPropRecord {
            origin: f3(p.origin),
            angles: f3(p.angles),
            prop_type: ty as u16,
            first_leaf: first as u16,
            leaf_count: count as u16,
            solid: p.solid,
            flags: if format.static_props.has_wide_flags() {
                p.flags | flag::NO_PER_TEXEL_LIGHTING
            } else {
                p.flags & 0xff
            },
            skin: p.skin,
            fade_min: p.fade_min,
            fade_max: p.fade_max,
            lighting_origin: f3(p.lighting_origin.unwrap_or(DVec3::ZERO)),
            forced_fade_scale: p.fade_scale,
            min_dx_level: dx_levels[0],
            max_dx_level: dx_levels[1],
            min_cpu_level: x.system_levels[0],
            max_cpu_level: x.system_levels[1],
            min_gpu_level: x.system_levels[2],
            max_gpu_level: x.system_levels[3],
            diffuse_modulation: x.color,
            disable_x360: x.disable_x360,
            flags_ex: x.flags_ex,
            lightmap_resolution: [0, 0],
            uniform_scale: x.uniform_scale,
            targetname: x.targetname.clone(),
        };
        record.write(format.static_props, &mut records);
    }
    let mut out = Buf::default();
    out.i32(names.len() as i32);
    for n in &names {
        let mut raw = [0u8; 128];
        let bytes = n.as_bytes();
        let len = bytes.len().min(127);
        raw[..len].copy_from_slice(&bytes[..len]);
        out.0.extend_from_slice(&raw);
    }
    out.i32(leaves.len() as i32);
    for l in &leaves {
        out.u16(*l);
    }
    out.i32(count_written as i32);
    out.0.extend_from_slice(&records);
    (out.0, outside)
}

pub fn game_lump(lumps: &[(u32, u16, Vec<u8>)], lump_offset: usize) -> Vec<u8> {
    let mut b = Buf::default();
    b.i32(lumps.len() as i32);
    let mut offset = lump_offset + 4 + lumps.len() * 16;
    for (id, version, data) in lumps {
        b.u32(*id);
        b.u16(0);
        b.u16(*version);
        b.i32(offset as i32);
        b.i32(data.len() as i32);
        offset += data.len();
    }
    for (_, _, data) in lumps {
        b.0.extend_from_slice(data);
    }
    b.0
}

#[cfg(test)]
mod static_check_tests {
    use super::*;

    #[test]
    fn vbsp_can_allow_dynamic_and_non_static_models() {
        let kv_dynamic = "mdlkeyvalue\n{\nprop_data\n{\n\"base\" \"Metal.Small\"\n}\n}\n";
        let kv_allowed = "mdlkeyvalue\n{\nprop_data\n{\n\"base\" \"Metal.Small\"\n\"allowstatic\" \"1\"\n}\n}\n";
        let m = "models/props_c17/oildrum001.mdl";
        assert_eq!(
            static_prop_error(m, "prop_static", true, kv_dynamic, false).as_deref(),
            Some(
                "Error! prop_static using model \"models/props_c17/oildrum001.mdl\", which must be used on a dynamic entity (i.e. prop_physics). Deleted."
            )
        );
        assert_eq!(static_prop_error(m, "prop_static", true, kv_dynamic, true), None, "-AllowDynamicPropsAsStatic");
        assert_eq!(static_prop_error(m, "prop_static", true, kv_allowed, false), None);
        assert_eq!(static_prop_error(m, "prop_static", true, "", false), None);
        assert!(
            static_prop_error(m, "prop_static", false, "", false)
                .is_some_and(|e| e.ends_with("it must be compiled with $staticprop!"))
        );
        assert_eq!(static_prop_error(m, "prop_static", false, kv_dynamic, true), None);
    }

    struct Refusing;

    impl ModelSource for Refusing {
        fn bounds(&self, _path: &str) -> Option<(DVec3, DVec3)> {
            Some((DVec3::splat(-8.0), DVec3::splat(8.0)))
        }

        fn static_prop_error(&self, path: &str) -> Option<String> {
            path.contains("drum").then(|| format!("Error! prop_static using model \"{path}\", which must be used on a dynamic entity (i.e. prop_physics). Deleted."))
        }
    }

    #[test]
    fn refused_models_are_deleted_with_vbsp_message() {
        let prop = |model: &str| {
            let mut e = hp_vmf::Entity { classname: "prop_static".into(), ..Default::default() };
            e.set("model", model);
            e.set("origin", "0 0 0");
            StaticProp::from_entity(&e, &|_| None).expect("a prop")
        };
        let props = vec![prop("models/drum.mdl"), prop("models/crate.mdl"), prop("models/drum.mdl")];
        let step = |_n: usize| TreeStep::Leaf { open: true };
        let mut warnings = Vec::new();
        let (lump, _) =
            sprp_lump_tree(&props, &Refusing, 0, &step, hp_bsp::BspFormat::default(), &|_| Some(0), &mut |w| {
                warnings.push(w)
            });
        assert_eq!(i32::from_le_bytes([lump[0], lump[1], lump[2], lump[3]]), 1);
        assert_eq!(
            warnings.iter().filter(|w| w.contains("which must be used on a dynamic entity")).count(),
            1,
            "{warnings:?}"
        );
    }
}
