use glam::DVec3;

use crate::emit::lump;

pub(crate) const MASK_OPAQUE: i32 = 0x1 | 0x80 | 0x4000;
pub(crate) const CONTENTS_SOLID: i32 = 0x1;

pub(crate) fn f32_at(d: &[u8], o: usize) -> f32 {
    d.get(o..o + 4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub(crate) fn i32_at(d: &[u8], o: usize) -> i32 {
    d.get(o..o + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub(crate) fn u16_at(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

pub(crate) fn i16_at(d: &[u8], o: usize) -> i16 {
    d.get(o..o + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]))
}

fn vec_at(d: &[u8], o: usize) -> DVec3 {
    let v = DVec3::new(f64::from(f32_at(d, o)), f64::from(f32_at(d, o + 4)), f64::from(f32_at(d, o + 8)));
    if v.is_finite() { v } else { DVec3::ZERO }
}

fn fin(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Plane {
    pub normal: DVec3,
    pub dist: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Face {
    pub plane: usize,
    pub side: u8,
    pub first_edge: i32,
    pub num_edges: i32,
    pub texinfo: i32,
    pub disp: i32,
    pub mins: [i32; 2],
    pub size: [i32; 2],
    pub smoothing: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TexInfo {
    pub tex: [[f32; 4]; 2],
    pub lm: [[f32; 4]; 2],
    pub flags: u32,
    pub texdata: i32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Leaf {
    pub contents: i32,
    pub cluster: i32,
    pub area: usize,
    pub mins: [f64; 3],
    pub maxs: [f64; 3],
    pub first_face: usize,
    pub num_faces: usize,
    pub first_brush: usize,
    pub num_brushes: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub plane: usize,
    pub children: [i32; 2],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Model {
    pub head: i32,
    pub first_face: usize,
    pub num_faces: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Brush {
    pub first_side: usize,
    pub num_sides: usize,
    pub contents: i32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BrushSide {
    pub plane: usize,
    pub texinfo: i32,
    pub bevel: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Disp {
    pub start: DVec3,
    pub vert_start: usize,
    pub power: u32,
    pub no_shadow: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Vis {
    pub clusters: usize,
    rows: Vec<Vec<u8>>,
}

impl Vis {
    #[cfg(test)]
    pub fn sees(&self, from: i32, to: i32) -> bool {
        if from < 0 || to < 0 {
            return true;
        }
        let (Some(row), to) = (self.rows.get(from as usize), to as usize) else { return true };
        row.get(to >> 3).is_none_or(|b| b & (1 << (to & 7)) != 0)
    }

    pub fn row(&self, cluster: i32) -> Option<&[u8]> {
        if cluster < 0 {
            return None;
        }
        self.rows.get(cluster as usize).map(Vec::as_slice)
    }
}

#[derive(Debug, Default)]
pub(crate) struct MapData {
    pub planes: Vec<Plane>,
    pub verts: Vec<DVec3>,
    pub edges: Vec<[u16; 2]>,
    pub surfedges: Vec<i32>,
    pub faces: Vec<Face>,
    pub faces_raw: Vec<u8>,
    pub faces_version: u32,
    pub texinfo: Vec<TexInfo>,
    pub reflectivity: Vec<[f32; 3]>,
    pub leafs: Vec<Leaf>,
    pub nodes: Vec<Node>,
    pub models: Vec<Model>,
    pub leaffaces: Vec<u16>,
    pub leafbrushes: Vec<u16>,
    pub brushes: Vec<Brush>,
    pub sides: Vec<BrushSide>,
    pub disps: Vec<Disp>,
    pub disp_verts: Vec<(DVec3, f64)>,
    pub vis: Vis,
    pub entities: Vec<hp_bsp::Entity>,
}

fn decompress_row(data: &[u8], mut at: usize, bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes);
    while out.len() < bytes {
        let Some(&b) = data.get(at) else { break };
        at += 1;
        if b != 0 {
            out.push(b);
            continue;
        }
        let Some(&run) = data.get(at) else { break };
        at += 1;
        let run = (run as usize).min(bytes - out.len());
        out.extend(std::iter::repeat_n(0u8, run));
        if run == 0 {
            break;
        }
    }
    out.resize(bytes, 0);
    out
}

fn parse_vis(d: &[u8]) -> Vis {
    let n = i32_at(d, 0);
    if n <= 0 || (n as usize).saturating_mul(8) + 4 > d.len() || n > 65536 {
        return Vis::default();
    }
    let n = n as usize;
    let bytes = n.div_ceil(8);
    let rows = (0..n)
        .map(|c| {
            let ofs = i32_at(d, 4 + c * 8);
            if ofs <= 0 || ofs as usize >= d.len() { vec![0xff; bytes] } else { decompress_row(d, ofs as usize, bytes) }
        })
        .collect();
    Vis { clusters: n, rows }
}

impl MapData {
    pub fn read(bsp: &hp_bsp::Bsp) -> Result<MapData, String> {
        let get = |i: usize| -> Result<Vec<u8>, String> {
            bsp.lump(i).map(|c| c.into_owned()).map_err(|e| format!("lump {i} cannot be read: {e}"))
        };
        let version = |i: usize| bsp.lump_info(i).map_or(0, |l| l.version);
        let mut m = MapData::default();
        let d = get(lump::PLANES)?;
        m.planes = (0..d.len() / 20)
            .map(|i| {
                let n = vec_at(&d, i * 20);
                let n = n.try_normalize().unwrap_or(DVec3::Z);
                Plane { normal: n, dist: f64::from(fin(f32_at(&d, i * 20 + 12))) }
            })
            .collect();
        let d = get(lump::VERTEXES)?;
        m.verts = (0..d.len() / 12).map(|i| vec_at(&d, i * 12)).collect();
        let d = get(lump::EDGES)?;
        m.edges = (0..d.len() / 4).map(|i| [u16_at(&d, i * 4), u16_at(&d, i * 4 + 2)]).collect();
        let d = get(lump::SURFEDGES)?;
        m.surfedges = (0..d.len() / 4).map(|i| i32_at(&d, i * 4)).collect();
        let d = get(lump::TEXINFO)?;
        m.texinfo = (0..d.len() / 72)
            .map(|i| {
                let o = i * 72;
                let v = |k: usize| -> [f32; 4] { std::array::from_fn(|c| fin(f32_at(&d, o + k * 16 + c * 4))) };
                TexInfo {
                    tex: [v(0), v(1)],
                    lm: [v(2), v(3)],
                    flags: i32_at(&d, o + 64) as u32,
                    texdata: i32_at(&d, o + 68),
                }
            })
            .collect();
        let d = get(lump::TEXDATA)?;
        m.reflectivity = (0..d.len() / 32)
            .map(|i| std::array::from_fn(|c| fin(f32_at(&d, i * 32 + c * 4)).clamp(0.0, 1.0)))
            .collect();
        let d = get(lump::FACES)?;
        m.faces_version = version(lump::FACES);
        m.faces = (0..d.len() / 56)
            .map(|i| {
                let o = i * 56;
                Face {
                    plane: usize::from(u16_at(&d, o)),
                    side: d.get(o + 2).copied().unwrap_or(0),
                    first_edge: i32_at(&d, o + 4),
                    num_edges: i32::from(i16_at(&d, o + 8)),
                    texinfo: i32::from(i16_at(&d, o + 10)),
                    disp: i32::from(i16_at(&d, o + 12)),
                    mins: [i32_at(&d, o + 28), i32_at(&d, o + 32)],
                    size: [i32_at(&d, o + 36), i32_at(&d, o + 40)],
                    smoothing: i32_at(&d, o + 52) as u32,
                }
            })
            .collect();
        m.faces_raw = d;
        m.faces_raw.truncate(m.faces.len() * 56);
        let d = get(lump::LEAFS)?;
        let leaf_size = if version(lump::LEAFS) == 0 { 56 } else { 32 };
        m.leafs = (0..d.len() / leaf_size)
            .map(|i| {
                let o = i * leaf_size;
                let b = |k: usize| f64::from(i16_at(&d, o + 8 + k * 2));
                Leaf {
                    contents: i32_at(&d, o),
                    cluster: i32::from(i16_at(&d, o + 4)),
                    area: usize::from(u16_at(&d, o + 6) & 0x1ff),
                    mins: [b(0), b(1), b(2)],
                    maxs: [b(3), b(4), b(5)],
                    first_face: usize::from(u16_at(&d, o + 20)),
                    num_faces: usize::from(u16_at(&d, o + 22)),
                    first_brush: usize::from(u16_at(&d, o + 24)),
                    num_brushes: usize::from(u16_at(&d, o + 26)),
                }
            })
            .collect();
        let d = get(lump::NODES)?;
        m.nodes = (0..d.len() / 32)
            .map(|i| {
                let o = i * 32;
                Node { plane: i32_at(&d, o).max(0) as usize, children: [i32_at(&d, o + 4), i32_at(&d, o + 8)] }
            })
            .collect();
        let d = get(lump::MODELS)?;
        m.models = (0..d.len() / 48)
            .map(|i| {
                let o = i * 48;
                Model {
                    head: i32_at(&d, o + 36),
                    first_face: i32_at(&d, o + 40).max(0) as usize,
                    num_faces: i32_at(&d, o + 44).max(0) as usize,
                }
            })
            .collect();
        let d = get(lump::LEAFFACES)?;
        m.leaffaces = (0..d.len() / 2).map(|i| u16_at(&d, i * 2)).collect();
        let d = get(lump::LEAFBRUSHES)?;
        m.leafbrushes = (0..d.len() / 2).map(|i| u16_at(&d, i * 2)).collect();
        let d = get(lump::BRUSHES)?;
        m.brushes = (0..d.len() / 12)
            .map(|i| Brush {
                first_side: i32_at(&d, i * 12).max(0) as usize,
                num_sides: i32_at(&d, i * 12 + 4).clamp(0, 128) as usize,
                contents: i32_at(&d, i * 12 + 8),
            })
            .collect();
        let d = get(lump::BRUSHSIDES)?;
        m.sides = (0..d.len() / 8)
            .map(|i| BrushSide {
                plane: usize::from(u16_at(&d, i * 8)),
                texinfo: i32::from(i16_at(&d, i * 8 + 2)),
                bevel: d.get(i * 8 + 6).is_some_and(|b| *b != 0),
            })
            .collect();
        let d = get(lump::DISPINFO)?;
        m.disps = (0..d.len() / 176)
            .map(|i| {
                let o = i * 176;
                let min_tess = i32_at(&d, o + 24);
                Disp {
                    start: vec_at(&d, o),
                    vert_start: i32_at(&d, o + 12).max(0) as usize,
                    power: i32_at(&d, o + 20).clamp(0, 31) as u32,
                    no_shadow: min_tess < 0 && min_tess & crate::disp::NO_SHADOW_BIT != 0,
                }
            })
            .collect();
        let d = get(lump::DISP_VERTS)?;
        m.disp_verts = (0..d.len() / 20)
            .map(|i| {
                let dist = f64::from(fin(f32_at(&d, i * 20 + 12)));
                (vec_at(&d, i * 20), dist)
            })
            .collect();
        m.vis = parse_vis(&get(lump::VISIBILITY)?);
        m.entities = bsp.entities().unwrap_or_default();
        Ok(m)
    }

    pub fn face_vertices(&self, f: &Face) -> Vec<usize> {
        let mut out = Vec::new();
        if f.first_edge < 0 || !(3..=64).contains(&f.num_edges) {
            return out;
        }
        for k in 0..f.num_edges as usize {
            let Some(&e) = self.surfedges.get(f.first_edge as usize + k) else { return Vec::new() };
            let Some(edge) = self.edges.get(e.unsigned_abs() as usize) else { return Vec::new() };
            let v = usize::from(if e >= 0 { edge[0] } else { edge[1] });
            if v >= self.verts.len() {
                return Vec::new();
            }
            out.push(v);
        }
        out
    }

    pub fn face_points(&self, f: &Face) -> Vec<DVec3> {
        let mut pts = Vec::new();
        if f.first_edge < 0 || !(3..=64).contains(&f.num_edges) {
            return pts;
        }
        for k in 0..f.num_edges as usize {
            let Some(&e) = self.surfedges.get(f.first_edge as usize + k) else { return Vec::new() };
            let Some(edge) = self.edges.get(e.unsigned_abs() as usize) else { return Vec::new() };
            let v = if e >= 0 { edge[0] } else { edge[1] };
            let Some(p) = self.verts.get(usize::from(v)) else { return Vec::new() };
            pts.push(*p);
        }
        pts
    }

    pub fn face_plane(&self, f: &Face) -> Option<Plane> {
        let p = *self.planes.get(f.plane)?;
        Some(if f.side != 0 && f.plane.is_multiple_of(2) { Plane { normal: -p.normal, dist: -p.dist } } else { p })
    }

    pub fn face_flags(&self, f: &Face) -> u32 {
        usize::try_from(f.texinfo).ok().and_then(|t| self.texinfo.get(t)).map_or(0, |t| t.flags)
    }

    pub fn point_leaf(&self, head: i32, p: DVec3) -> Option<usize> {
        let mut n = head;
        for _ in 0..=self.nodes.len() {
            if n < 0 {
                let leaf = (-1 - n) as usize;
                return (leaf < self.leafs.len()).then_some(leaf);
            }
            let node = self.nodes.get(n as usize)?;
            let plane = self.planes.get(node.plane)?;
            n = node.children[usize::from(p.dot(plane.normal) - plane.dist < 0.0)];
        }
        None
    }

    pub fn cluster_at(&self, p: DVec3) -> i32 {
        let head = self.models.first().map_or(-1, |m| m.head);
        if head < 0 && self.nodes.is_empty() {
            return -1;
        }
        self.point_leaf(head, p).and_then(|l| self.leafs.get(l)).map_or(-1, |l| l.cluster)
    }

    pub fn vis_cluster_at(&self, p: DVec3) -> i32 {
        if self.vis.clusters == 0 { -1 } else { self.cluster_at(p) }
    }

    pub fn area_at(&self, p: DVec3) -> Option<usize> {
        let head = self.models.first()?.head;
        self.point_leaf(head, p).and_then(|l| self.leafs.get(l)).map(|l| l.area)
    }

    pub fn face_disp(&self, f: &Face) -> Option<&Disp> {
        usize::try_from(f.disp).ok().and_then(|d| self.disps.get(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vis_rows() {
        let mut d = Vec::new();
        d.extend_from_slice(&2i32.to_le_bytes());
        d.extend_from_slice(&20i32.to_le_bytes());
        d.extend_from_slice(&20i32.to_le_bytes());
        d.extend_from_slice(&21i32.to_le_bytes());
        d.extend_from_slice(&21i32.to_le_bytes());
        d.push(0b11);
        d.push(0b10);
        let v = parse_vis(&d);
        assert_eq!(v.clusters, 2);
        assert!(v.sees(0, 1));
        assert!(v.sees(1, 1));
        assert!(!v.sees(1, 0));
        assert!(v.sees(-1, 0));
        let v = parse_vis(&[255, 255, 255, 127, 1, 2]);
        assert_eq!(v.clusters, 0);
        assert_eq!(decompress_row(&[0, 0, 5], 0, 4), vec![0, 0, 0, 0]);
        assert_eq!(decompress_row(&[0, 200], 0, 4), vec![0, 0, 0, 0]);
    }
}
