use glam::{DVec3, Vec3};

use crate::materials::contents;
use crate::planes::PlaneSet;
use crate::winding::{ON_EPSILON, Winding};

#[derive(Clone, Debug)]
pub struct BrushSide {
    pub plane: usize,
    pub texinfo: i32,
    pub surf: u32,
    pub winding: Option<Winding>,
    pub bevel: bool,
    pub visible: bool,
    pub no_split: bool,
    pub side_id: u32,
    pub disp: Option<usize>,
    pub contents: u32,
    pub fwinding: Option<Vec<Vec3>>,
    pub smoothing: u32,
    pub overlays: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct MapBrush {
    pub id: u32,
    pub entity: usize,
    pub contents: u32,
    pub sides: Vec<BrushSide>,
    pub mins: DVec3,
    pub maxs: DVec3,
    pub surfaceprop: String,
}

pub type Notes = Vec<String>;

impl MapBrush {
    pub fn make_windings(&mut self, planes: &PlaneSet) -> bool {
        self.mins = DVec3::splat(f64::MAX);
        self.maxs = DVec3::splat(f64::MIN);
        let plane_list: Vec<usize> = self.sides.iter().filter(|s| !s.bevel).map(|s| s.plane).collect();
        for i in 0..self.sides.len() {
            if self.sides[i].bevel {
                self.sides[i].winding = None;
                continue;
            }
            let p = *planes.get(self.sides[i].plane);
            let mut w = Some(Winding::base_for_plane(p.normal, p.dist));
            for &other in &plane_list {
                let Some(cur) = w.take() else { break };
                if other == self.sides[i].plane {
                    w = Some(cur);
                    continue;
                }
                if other == (self.sides[i].plane ^ 1) {
                    break;
                }
                w = cur.clip_back(planes.get(other), 0.0, false);
            }
            if let Some(w) = &w {
                for p in &w.points {
                    self.mins = self.mins.min(*p);
                    self.maxs = self.maxs.max(*p);
                }
            }
            self.sides[i].winding = w;
        }

        self.sides.iter().filter(|s| s.winding.is_some()).count() >= 4
            && self.mins.cmple(self.maxs).all()
            && self.mins.abs().max_element() < crate::winding::MAX_WORLD
            && self.maxs.abs().max_element() < crate::winding::MAX_WORLD
    }

    fn float_windings(&self, planes: &PlaneSet, with_bevels: bool) -> Vec<Option<Vec<Vec3>>> {
        let normalize = |v: Vec3| v * (1.0 / (v.length() + 1e-10));
        let plane = |i: usize| {
            let p = planes.get(i);
            (p.normal.as_vec3(), p.dist as f32)
        };
        let base = |n: Vec3, d: f32| {
            let mut x = 0;
            let mut max = -1.0f32;
            for i in 0..3 {
                if n[i].abs() > max {
                    max = n[i].abs();
                    x = i;
                }
            }
            let mut up = if x == 2 { Vec3::X } else { Vec3::Z };
            let v = up.dot(n);
            up = normalize(up + n * -v);
            let org = n * d;
            let right = up.cross(n) * 65536.0;
            let up = up * 65536.0;
            vec![org - right + up, org + right + up, org + right - up, org - right - up]
        };
        let chop = |w: Vec<Vec3>, n: Vec3, d: f32, eps: f32| -> Option<Vec<Vec3>> {
            let dists: Vec<f32> = w.iter().map(|p| p.dot(n) - d).collect();
            let side = |x: f32| {
                if x > eps {
                    0
                } else if x < -eps {
                    1
                } else {
                    2
                }
            };
            let sides: Vec<u8> = dists.iter().map(|&x| side(x)).collect();
            if !sides.contains(&0) {
                return None;
            }
            if !sides.contains(&1) {
                return Some(w);
            }
            let m = w.len();
            let mut out = Vec::with_capacity(m + 4);
            for i in 0..m {
                let p1 = w[i];
                if sides[i] == 2 {
                    out.push(p1);
                    continue;
                }
                if sides[i] == 0 {
                    out.push(p1);
                }
                let next = sides[(i + 1) % m];
                if next == 2 || next == sides[i] {
                    continue;
                }
                let p2 = w[(i + 1) % m];
                let dot = dists[i] / (dists[i] - dists[(i + 1) % m]);
                let mut mid = Vec3::ZERO;
                for j in 0..3 {
                    mid[j] = if n[j] == 1.0 {
                        d
                    } else if n[j] == -1.0 {
                        -d
                    } else {
                        p1[j] + dot * (p2[j] - p1[j])
                    };
                }
                out.push(mid);
            }
            Some(out)
        };
        let real: Vec<usize> = (0..self.sides.len()).filter(|&j| !self.sides[j].bevel).collect();
        (0..self.sides.len())
            .map(|i| {
                if self.sides[i].bevel && !with_bevels {
                    return None;
                }
                let (n, d) = plane(self.sides[i].plane);
                let mut w = Some(base(n, d));
                for &j in &real {
                    if j == i {
                        continue;
                    }
                    let (n, d) = plane(self.sides[j].plane ^ 1);
                    w = chop(w?, n, d, 0.01);
                }
                w
            })
            .collect()
    }

    pub fn apply_origin(&mut self, planes: &mut PlaneSet, origin: DVec3) {
        let o = origin.as_vec3();
        for s in &mut self.sides {
            let (n, d) = planes.getf(s.plane);
            s.plane = planes.find_f32(n, d - n.dot(o));
        }
        self.make_windings(planes);
        let fw = self.float_windings(planes, true);
        for (s, w) in self.sides.iter_mut().zip(fw) {
            s.fwinding = w;
        }
    }

    pub fn add_bevels(&mut self, planes: &mut PlaneSet) {
        let template = |plane: usize, sides: &[BrushSide]| BrushSide {
            plane,
            texinfo: sides.first().map_or(-1, |s| s.texinfo),
            surf: sides.first().map_or(0, |s| s.surf),
            winding: None,
            bevel: true,
            visible: false,
            no_split: true,
            side_id: 0,
            disp: None,
            contents: sides.first().map_or(0, |s| s.contents),
            fwinding: None,
            smoothing: sides.first().map_or(0, |s| s.smoothing),
            overlays: Vec::new(),
        };
        let mut fw = self.float_windings(planes, false);
        let (fmins, fmaxs) = float_bounds(&fw);
        let mut order = 0;
        for axis in 0..3 {
            for dir in [-1.0, 1.0] {
                let found = self.sides.iter().position(|s| planes.get(s.plane).normal[axis] == dir);
                let at = match found {
                    Some(i) => i,
                    None => {
                        let mut n = DVec3::ZERO;
                        n[axis] = dir;
                        let dist = if dir > 0.0 { fmaxs[axis] } else { -fmins[axis] };
                        let p = planes.find_f32(n.as_vec3(), dist);
                        let s = template(p, &self.sides);
                        self.sides.push(s);
                        fw.push(None);
                        self.sides.len() - 1
                    }
                };
                if at != order && order < self.sides.len() {
                    self.sides.swap(at, order);
                    fw.swap(at, order);
                }
                order += 1;
            }
        }
        if self.sides.len() == 6 {
            for (s, w) in self.sides.iter_mut().zip(fw) {
                s.fwinding = w;
            }
            return;
        }
        let mut i = 6;
        while i < self.sides.len() {
            let Some(w) = fw.get(i).cloned().flatten() else {
                i += 1;
                continue;
            };
            let normalize = |v: Vec3| {
                let r = v.length();
                (v * (1.0 / (r + 1e-10)), r)
            };
            for j in 0..w.len() {
                let k = (j + 1) % w.len();
                let pj = w[j];
                let (vec, len) = normalize(pj - w[k]);
                if len < 0.5 {
                    continue;
                }
                let vec = snap_vector(vec);
                if vec.to_array().iter().any(|c| *c == 1.0 || *c == -1.0) {
                    continue;
                }
                for axis in 0..3 {
                    for dir in [-1.0f32, 1.0] {
                        let mut v2 = Vec3::ZERO;
                        v2[axis] = dir;
                        let (normal, nlen) = normalize(vec.cross(v2));
                        if nlen < 0.5 {
                            continue;
                        }
                        let dist = pj.dot(normal);
                        let mut proper = true;
                        for (s, w2) in self.sides.iter().zip(&fw) {
                            let p = planes.get(s.plane);
                            let pn = p.normal.as_vec3();
                            if (pn - normal).abs().max_element() < 0.01 && (p.dist as f32 - dist).abs() < 0.01 {
                                proper = false;
                                break;
                            }
                            if let Some(w2) = w2
                                && w2.iter().any(|q| q.dot(normal) - dist > 0.1)
                            {
                                proper = false;
                                break;
                            }
                        }
                        if !proper {
                            continue;
                        }
                        let p = planes.find_f32(normal, dist);
                        if planes.get(p).kind <= crate::planes::PLANE_Z {
                            continue;
                        }
                        let s = template(p, &self.sides);
                        self.sides.push(s);
                        fw.push(None);
                    }
                }
            }
            i += 1;
        }
        for (s, w) in self.sides.iter_mut().zip(fw) {
            s.fwinding = w;
        }
    }

    pub fn contents_from_sides(side_contents: &[u32]) -> (u32, bool) {
        let Some(&first) = side_contents.first() else { return (contents::EMPTY, false) };
        let mixed = side_contents.iter().any(|c| *c != first);
        let mut c = first;
        let union = side_contents.iter().fold(0, |a, s| a | s);
        let see_through = union & (contents::WINDOW | contents::GRATE | contents::WATER | contents::SLIME);
        if see_through != 0 {
            c |= see_through | contents::TRANSLUCENT;
            c &= !contents::SOLID;
        }
        c |= union & contents::LADDER;
        (c, mixed)
    }

    pub fn volume(&self) -> f64 {
        let Some(corner) = self.sides.iter().find_map(|s| s.winding.as_ref()).and_then(|w| w.points.first().copied())
        else {
            return 0.0;
        };
        let mut v = 0.0;
        for s in &self.sides {
            let Some(w) = &s.winding else { continue };
            let n = w.normal();
            let d = -(corner - w.points[0]).dot(n);
            v += d * w.area();
        }
        v / 3.0
    }
}

pub fn float_bounds(ws: &[Option<Vec<Vec3>>]) -> (Vec3, Vec3) {
    let mut lo = Vec3::splat(99999.0);
    let mut hi = Vec3::splat(-99999.0);
    for p in ws.iter().flatten().flatten() {
        add_point_to_bounds(*p, &mut lo, &mut hi);
    }
    (lo, hi)
}

pub fn add_point_to_bounds(p: Vec3, lo: &mut Vec3, hi: &mut Vec3) {
    for i in 0..3 {
        if p[i] < lo[i] {
            lo[i] = p[i];
        }
        if p[i] > hi[i] {
            hi[i] = p[i];
        }
    }
}

pub fn tiny(b: &MapBrush) -> bool {
    b.volume() < 1.0 || b.sides.iter().filter(|s| s.winding.as_ref().is_some_and(|w| !w.is_tiny())).count() < 3
}

pub const BRUSH_EPSILON: f64 = ON_EPSILON;

fn snap_vector(v: Vec3) -> Vec3 {
    for i in 0..3 {
        for one in [1.0f32, -1.0] {
            if (v[i] - one).abs() < 0.00001 {
                let mut out = Vec3::ZERO;
                out[i] = one;
                return out;
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn cube(planes: &mut PlaneSet, lo: DVec3, hi: DVec3) -> MapBrush {
        let mut sides = Vec::new();
        for axis in 0..3 {
            for (dir, d) in [(1.0, hi[axis]), (-1.0, -lo[axis])] {
                let mut n = DVec3::ZERO;
                n[axis] = dir;
                let p = planes.find(n, d).unwrap();
                sides.push(BrushSide {
                    plane: p,
                    texinfo: 0,
                    surf: 0,
                    winding: None,
                    bevel: false,
                    visible: false,
                    no_split: false,
                    side_id: 0,
                    disp: None,
                    contents: contents::SOLID,
                    fwinding: None,
                    smoothing: 0,
                    overlays: Vec::new(),
                });
            }
        }
        let mut b = MapBrush {
            id: 1,
            entity: 0,
            contents: contents::SOLID,
            sides,
            mins: lo,
            maxs: hi,
            surfaceprop: String::new(),
        };
        assert!(b.make_windings(planes));
        b
    }

    #[test]
    fn box_windings_bounds_and_bevels() {
        let mut planes = PlaneSet::new();
        let mut b = cube(&mut planes, DVec3::ZERO, DVec3::new(64.0, 32.0, 16.0));
        assert_eq!(b.mins, DVec3::ZERO);
        assert_eq!(b.maxs, DVec3::new(64.0, 32.0, 16.0));
        assert!((b.volume() - 64.0 * 32.0 * 16.0).abs() < 1e-6);
        b.add_bevels(&mut planes);
        assert_eq!(b.sides.len(), 6, "a box needs no bevel");
        assert_eq!(planes.get(b.sides[0].plane).normal, -DVec3::X);
        assert_eq!(planes.get(b.sides[5].plane).normal, DVec3::Z);
        let mut planes = PlaneSet::new();
        let mut w = cube(&mut planes, DVec3::ZERO, DVec3::splat(64.0));
        let slope = planes.find(DVec3::new(1.0, 0.0, 1.0), 64.0 / 2f64.sqrt() * 1.0).unwrap();
        w.sides.retain(|s| planes.get(s.plane).normal != DVec3::Z);
        w.sides.push(BrushSide { plane: slope, ..w.sides[0].clone() });
        assert!(w.make_windings(&planes));
        let before = w.sides.len();
        w.add_bevels(&mut planes);
        assert!(w.sides.len() > before, "the top of the wedge is a bevel");
        assert!(w.sides.iter().skip(6).any(|s| !s.bevel));
    }

    #[test]
    fn shallow_sloped_thin_brush_has_no_snapped_axial_edge_bevels() {
        let mut planes = PlaneSet::new();
        let mut brush = cube(&mut planes, DVec3::ZERO, DVec3::splat(64.0));
        let source = [
            [0.30073991417884827, -0.9537061452865601, 0.0, -7652.36328125],
            [-0.30073991417884827, 0.9537061452865601, 0.0, 7658.3642578125],
            [0.007729232311248779, 0.0024272450245916843, -0.9999672174453735, 74.98281860351562],
            [-0.007729227188974619, -0.0024272301234304905, 0.9999672174453735, -66.98250579833984],
            [-0.956061840057373, -0.2931649684906006, 0.0, -16283.2099609375],
            [0.952019214630127, 0.3060379922389984, 0.0, 16448.54296875],
        ];
        for (side, plane) in brush.sides.iter_mut().zip(source) {
            side.plane = planes.find_f32(Vec3::new(plane[0] as f32, plane[1] as f32, plane[2] as f32), plane[3] as f32);
        }
        assert!(brush.make_windings(&planes));
        brush.add_bevels(&mut planes);
        assert!(brush.sides.iter().skip(6).all(|side| planes.get(side.plane).kind > crate::planes::PLANE_Z));
    }
}
