use glam::{DMat3, DMat4, DVec3};
use hp_geom::Aabb;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriMesh {
    pub verts: Vec<DVec3>,
    pub tris: Vec<[u32; 3]>,
    pub normals: Vec<DVec3>,
    pub tags: Vec<u32>,
    pub closed: bool,
}

pub fn round_to(v: f64, step: f64) -> f64 {
    let r = (v / step).round() * step;
    if r == 0.0 { 0.0 } else { r }
}

impl TriMesh {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    pub fn tri(&self, i: usize) -> [DVec3; 3] {
        let t = self.tris[i];
        [self.verts[t[0] as usize], self.verts[t[1] as usize], self.verts[t[2] as usize]]
    }

    pub fn add_vertex(&mut self, p: DVec3) -> u32 {
        self.verts.push(p);
        (self.verts.len() - 1) as u32
    }

    pub fn add_tri(&mut self, t: [u32; 3], tag: u32) -> bool {
        let [a, b, c] = t.map(|i| self.verts[i as usize]);
        let Some(n) = (b - a).cross(c - a).try_normalize().filter(|n| n.is_finite()) else { return false };
        self.tris.push(t);
        self.normals.push(n);
        self.tags.push(tag);
        true
    }

    pub fn add_tri_facing(&mut self, t: [u32; 3], facing: DVec3, tag: u32) -> bool {
        let [a, b, c] = t.map(|i| self.verts[i as usize]);
        let n = (b - a).cross(c - a);
        if n.dot(facing) < 0.0 { self.add_tri([t[0], t[2], t[1]], tag) } else { self.add_tri(t, tag) }
    }

    pub fn add_polygon(&mut self, points: &[DVec3], tag: u32) {
        if points.len() < 3 {
            return;
        }
        let base = self.verts.len() as u32;
        self.verts.extend_from_slice(points);
        for i in 1..points.len() as u32 - 1 {
            self.add_tri([base, base + i, base + i + 1], tag);
        }
    }

    pub fn add_polygon_facing(&mut self, points: &[DVec3], facing: DVec3, tag: u32) {
        if points.len() < 3 {
            return;
        }
        let base = self.verts.len() as u32;
        self.verts.extend_from_slice(points);
        for i in 1..points.len() as u32 - 1 {
            self.add_tri_facing([base, base + i, base + i + 1], facing, tag);
        }
    }

    pub fn append(&mut self, other: &TriMesh) {
        let base = self.verts.len() as u32;
        self.verts.extend_from_slice(&other.verts);
        self.tris.extend(other.tris.iter().map(|t| t.map(|i| i + base)));
        self.normals.extend_from_slice(&other.normals);
        self.tags.extend_from_slice(&other.tags);
        self.closed = false;
    }

    pub fn transformed(&self, m: &DMat4) -> TriMesh {
        let lin = DMat3::from_mat4(*m);
        let flip = lin.determinant() < 0.0;
        let nm = lin.inverse().transpose();
        let verts: Vec<DVec3> = self.verts.iter().map(|p| m.transform_point3(*p)).collect();
        let tris: Vec<[u32; 3]> =
            if flip { self.tris.iter().map(|t| [t[0], t[2], t[1]]).collect() } else { self.tris.clone() };
        let normals = self.normals.iter().map(|n| (nm * *n).try_normalize().unwrap_or(*n)).collect();
        TriMesh { verts, tris, normals, tags: self.tags.clone(), closed: self.closed }
    }

    pub fn translated(&self, d: DVec3) -> TriMesh {
        let mut out = self.clone();
        for v in &mut out.verts {
            *v += d;
        }
        out
    }

    pub fn bounds(&self) -> Aabb {
        let mut b = Aabb::EMPTY;
        for t in &self.tris {
            for &i in t {
                b.extend(self.verts[i as usize]);
            }
        }
        b
    }

    pub fn used_vertices(&self) -> Vec<DVec3> {
        let mut used = vec![false; self.verts.len()];
        for t in &self.tris {
            for &i in t {
                used[i as usize] = true;
            }
        }
        self.verts.iter().zip(used).filter(|(_, u)| *u).map(|(v, _)| *v).collect()
    }

    pub fn edges(&self) -> Vec<[u32; 2]> {
        let mut out: Vec<[u32; 2]> = Vec::with_capacity(self.tris.len() * 3 / 2 + 3);
        for t in &self.tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                out.push(if a < b { [a, b] } else { [b, a] });
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn welded(&self, tolerance: f64) -> TriMesh {
        let q = |v: f64| (v / tolerance).round() as i64;
        let mut map: std::collections::HashMap<(i64, i64, i64), u32> = std::collections::HashMap::new();
        let mut verts = Vec::new();
        let remap: Vec<u32> = self
            .verts
            .iter()
            .map(|p| {
                *map.entry((q(p.x), q(p.y), q(p.z))).or_insert_with(|| {
                    verts.push(*p);
                    (verts.len() - 1) as u32
                })
            })
            .collect();
        let mut out = TriMesh { verts, closed: self.closed, ..TriMesh::default() };
        for (i, t) in self.tris.iter().enumerate() {
            let t = t.map(|k| remap[k as usize]);
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                continue;
            }
            out.tris.push(t);
            out.normals.push(self.normals[i]);
            out.tags.push(self.tags[i]);
        }
        out
    }

    pub fn area(&self) -> f64 {
        (0..self.tris.len())
            .map(|i| {
                let [a, b, c] = self.tri(i);
                (b - a).cross(c - a).length() * 0.5
            })
            .sum()
    }

    pub fn convex_box(min: DVec3, max: DVec3, tag: u32) -> TriMesh {
        let c = |i: usize| {
            DVec3::new(
                if i & 1 != 0 { max.x } else { min.x },
                if i & 2 != 0 { max.y } else { min.y },
                if i & 4 != 0 { max.z } else { min.z },
            )
        };
        let mut m = TriMesh::new();
        let faces: [([usize; 4], DVec3); 6] = [
            ([0, 2, 6, 4], DVec3::NEG_X),
            ([1, 5, 7, 3], DVec3::X),
            ([0, 4, 5, 1], DVec3::NEG_Y),
            ([2, 3, 7, 6], DVec3::Y),
            ([0, 1, 3, 2], DVec3::NEG_Z),
            ([4, 6, 7, 5], DVec3::Z),
        ];
        for (f, n) in faces {
            m.add_polygon_facing(&f.map(c), n, tag);
        }
        m.closed = true;
        m
    }
}
