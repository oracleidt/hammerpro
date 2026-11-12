use glam::DVec3;
use hp_geom::Aabb;

pub const MAX_PRT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PORTALS: usize = 1 << 20;
const MAX_POINTS: usize = 256;
const MAX_CLUSTERS: u32 = 1 << 20;
const MAX_COORD: f64 = 1.0e6;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortalError {
    #[error("not a portal file (PRT1 expected)")]
    NotPrt,
    #[error("line {line}: {message}")]
    Bad { line: usize, message: String },
    #[error("the file is too large")]
    TooLarge,
    #[error("{0}")]
    Lump(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Portal {
    pub clusters: [u32; 2],
    pub points: Vec<DVec3>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PortalFile {
    pub clusters: u32,
    pub portals: Vec<Portal>,
    pub bounds: Aabb,
    pub declared_portals: usize,
}

impl PortalFile {
    pub fn portals_of(&self, cluster: u32) -> impl Iterator<Item = &Portal> {
        self.portals.iter().filter(move |p| p.clusters.contains(&cluster))
    }
}

pub fn parse_prt(text: &str) -> Result<PortalFile, PortalError> {
    if text.len() as u64 > MAX_PRT_BYTES {
        return Err(PortalError::TooLarge);
    }
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l.trim())).filter(|(_, l)| !l.is_empty());
    let Some((_, magic)) = lines.next() else { return Err(PortalError::NotPrt) };
    if !magic.eq_ignore_ascii_case("PRT1") {
        return Err(PortalError::NotPrt);
    }
    let mut number = |what: &str| -> Result<u64, PortalError> {
        let (n, l) = lines.next().ok_or(PortalError::Bad { line: 0, message: format!("missing {what}") })?;
        l.split_ascii_whitespace()
            .next()
            .and_then(|t| t.parse::<u64>().ok())
            .ok_or(PortalError::Bad { line: n, message: format!("invalid {what}: {l:?}") })
    };
    let clusters = number("cluster count")?;
    let declared = number("portal count")?;
    if clusters > u64::from(MAX_CLUSTERS) {
        return Err(PortalError::Bad { line: 2, message: format!("{clusters} clusters") });
    }
    let clusters = clusters as u32;
    let want = (declared as usize).min(MAX_PORTALS);
    let mut portals = Vec::with_capacity(want.min(text.len() / 32 + 1));
    let mut bounds = Aabb::EMPTY;
    for (n, line) in lines {
        if portals.len() >= want {
            break;
        }
        let bad = |message: String| PortalError::Bad { line: n, message };
        let (head, rest) = line.split_once('(').map_or((line, ""), |(h, r)| (h, r));
        let nums: Vec<&str> = head.split_ascii_whitespace().collect();
        if nums.len() < 3 {
            break;
        }
        let count: usize = nums[0].parse().map_err(|_| bad(format!("invalid point count {:?}", nums[0])))?;
        let a: u32 = nums[1].parse().map_err(|_| bad(format!("invalid cluster {:?}", nums[1])))?;
        let b: u32 = nums[2].parse().map_err(|_| bad(format!("invalid cluster {:?}", nums[2])))?;
        if !(3..=MAX_POINTS).contains(&count) {
            return Err(bad(format!("{count} points")));
        }
        if a >= clusters || b >= clusters {
            return Err(bad(format!("cluster {} of {clusters}", a.max(b))));
        }
        let mut points = Vec::with_capacity(count);
        for group in rest.split('(') {
            let group = group.trim().trim_end_matches(')').trim();
            if group.is_empty() {
                continue;
            }
            let mut v = [0.0f64; 3];
            let mut it = group.split_ascii_whitespace();
            for slot in &mut v {
                let t = it.next().ok_or_else(|| bad(format!("incomplete point {group:?}")))?;
                let x: f64 = t.parse().map_err(|_| bad(format!("invalid number {t:?}")))?;
                if !x.is_finite() || x.abs() > MAX_COORD {
                    return Err(bad(format!("invalid number {t:?}")));
                }
                *slot = x;
            }
            if points.len() < count {
                points.push(DVec3::from_array(v));
            }
        }
        if points.len() != count {
            return Err(bad(format!("{} points, {count} announced", points.len())));
        }
        for p in &points {
            bounds.extend(*p);
        }
        portals.push(Portal { clusters: [a, b], points });
    }
    Ok(PortalFile { clusters, portals, bounds, declared_portals: declared as usize })
}

pub mod lump {
    pub const PLANES: usize = 1;
    pub const VISIBILITY: usize = 4;
    pub const NODES: usize = 5;
    pub const LEAFS: usize = 10;
    pub const MODELS: usize = 14;
    pub const TEXINFO: usize = 6;
    pub const FACES: usize = 7;
    pub const LEAF_FACES: usize = 16;
    pub const DISPINFO: usize = 26;
    pub const OVERLAYS: usize = 45;
}

fn i32_at(d: &[u8], o: usize) -> Option<i32> {
    d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    d.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))
}

fn f32_at(d: &[u8], o: usize) -> Option<f32> {
    d.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Leaf {
    cluster: i16,
    area: u16,
    min: [i16; 3],
    max: [i16; 3],
}

#[derive(Clone, Debug, Default)]
pub struct BspVis {
    planes: Vec<(DVec3, f64)>,
    nodes: Vec<(u32, [i32; 2])>,
    leaves: Vec<Leaf>,
    head: i32,
    clusters: u32,
    vis: Vec<u8>,
    cluster_bounds: Vec<Aabb>,
    area_portals: Vec<Vec<(u16, u16)>>,
    portal_geometry: Vec<Vec<AreaPortal>>,
    portal_entities: Vec<PortalEntity>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PortalEntity {
    pub number: u16,
    pub door: String,
    pub name: String,
    pub start_open: bool,
    pub window: bool,
    pub fade_dist: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct AreaPortal {
    key: u16,
    other: u16,
    normal: DVec3,
    dist: f64,
    verts: Vec<DVec3>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewCone {
    pub eye: DVec3,
    pub forward: DVec3,
    pub right: DVec3,
    pub up: DVec3,
    pub tan_x: f64,
    pub tan_y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRect {
    pub left: f64,
    pub right: f64,
    pub bottom: f64,
    pub top: f64,
}

impl ScreenRect {
    pub const FULL: ScreenRect = ScreenRect { left: -1.0, right: 1.0, bottom: -1.0, top: 1.0 };

    fn intersect(&self, o: &ScreenRect) -> Option<ScreenRect> {
        let r = ScreenRect {
            left: self.left.max(o.left),
            right: self.right.min(o.right),
            bottom: self.bottom.max(o.bottom),
            top: self.top.min(o.top),
        };
        (r.left < r.right && r.bottom < r.top).then_some(r)
    }

    fn union(&self, o: &ScreenRect) -> ScreenRect {
        ScreenRect {
            left: self.left.min(o.left),
            right: self.right.max(o.right),
            bottom: self.bottom.min(o.bottom),
            top: self.top.max(o.top),
        }
    }
}

impl ViewCone {
    pub fn planes(&self, rect: &ScreenRect) -> [(DVec3, f64); 4] {
        let (f, r, u) = (self.forward, self.right, self.up);
        let n = [
            f * (rect.right * self.tan_x) - r,
            r - f * (rect.left * self.tan_x),
            f * (rect.top * self.tan_y) - u,
            u - f * (rect.bottom * self.tan_y),
        ];
        n.map(|n| {
            let n = n.normalize_or_zero();
            (n, n.dot(self.eye))
        })
    }

    fn screen_extents(&self, poly: &[DVec3]) -> Option<ScreenRect> {
        let mut pts: Vec<DVec3> = poly.to_vec();
        for (n, d) in self.planes(&ScreenRect::FULL) {
            if pts.is_empty() {
                return None;
            }
            let mut out = Vec::with_capacity(pts.len() + 2);
            let mut prev = pts[pts.len() - 1];
            let mut prev_dot = n.dot(prev) - d;
            for &cur in &pts {
                let dot = n.dot(cur) - d;
                if (dot > 0.0) != (prev_dot > 0.0) {
                    let t = prev_dot / (prev_dot - dot);
                    out.push(prev + (cur - prev) * t);
                }
                if dot > 0.0 {
                    out.push(cur);
                }
                prev = cur;
                prev_dot = dot;
            }
            pts = out;
        }
        let mut rect: Option<ScreenRect> = None;
        for p in pts {
            let v = p - self.eye;
            let z = v.dot(self.forward).max(1e-6);
            let x = (v.dot(self.right) / z / self.tan_x).clamp(-1.0, 1.0);
            let y = (v.dot(self.up) / z / self.tan_y).clamp(-1.0, 1.0);
            let pr = ScreenRect { left: x, right: x, bottom: y, top: y };
            rect = Some(rect.map_or(pr, |r| r.union(&pr)));
        }
        rect
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AreaFlow {
    pub own_area: Option<u16>,
    pub visible: Vec<bool>,
    pub rects: Vec<Option<ScreenRect>>,
    pub cone: Option<ViewCone>,
    pub portals: usize,
    pub portals_open: usize,
    pub portals_seen: usize,
}

impl AreaFlow {
    pub fn visible_areas(&self) -> usize {
        self.visible.iter().filter(|v| **v).count()
    }

    pub fn box_visible(&self, area: u16, min: DVec3, max: DVec3) -> bool {
        if self.own_area.is_none() || self.own_area == Some(area) {
            return true;
        }
        if !self.visible.get(usize::from(area)).copied().unwrap_or(true) {
            return false;
        }
        let (Some(cone), Some(Some(rect))) = (self.cone, self.rects.get(usize::from(area))) else { return true };
        let (center, half) = ((min + max) * 0.5, (max - min) * 0.5);
        cone.planes(rect).iter().all(|(n, d)| n.dot(center) - d + n.abs().dot(half) >= 0.0)
    }
}

struct FlowWalk<'a, F> {
    cone: &'a ViewCone,
    open: &'a F,
    flow: &'a mut AreaFlow,
    on_stack: &'a mut [bool],
    seen: &'a mut Vec<u16>,
}

const AREA_SIZE: usize = 8;
const AREA_PORTAL_SIZE: usize = 12;
const MAX_AREAS: usize = 1024;
const MAX_AREA_PORTALS: usize = 8192;

impl BspVis {
    pub fn from_lumps(
        planes: &[u8],
        nodes: &[u8],
        leafs: &[u8],
        leaf_version: u32,
        models: &[u8],
        vis: &[u8],
    ) -> Result<BspVis, PortalError> {
        let plane_list: Vec<(DVec3, f64)> = planes
            .chunks_exact(20)
            .map(|c| {
                let f = |o| f64::from(f32_at(c, o).unwrap_or(0.0));
                (DVec3::new(f(0), f(4), f(8)), f(12))
            })
            .collect();
        let node_list: Vec<(u32, [i32; 2])> = nodes
            .chunks_exact(32)
            .map(|c| {
                let p = i32_at(c, 0).unwrap_or(-1);
                (u32::try_from(p).unwrap_or(u32::MAX), [i32_at(c, 4).unwrap_or(-1), i32_at(c, 8).unwrap_or(-1)])
            })
            .collect();
        let leaf_size = if leaf_version == 0 { 56 } else { 32 };
        let leaf_list: Vec<Leaf> = leafs
            .chunks_exact(leaf_size)
            .map(|c| {
                let s = |o| i16_at(c, o).unwrap_or(0);
                Leaf {
                    cluster: s(4),
                    area: (s(6) as u16) & 0x1ff,
                    min: [s(8), s(10), s(12)],
                    max: [s(14), s(16), s(18)],
                }
            })
            .collect();
        if node_list.is_empty() || leaf_list.is_empty() {
            return Err(PortalError::Lump("the map has no BSP tree".to_owned()));
        }
        let head = i32_at(models, 36).ok_or_else(|| PortalError::Lump("the map has no world model".to_owned()))?;
        let mut clusters = 0u32;
        if vis.len() >= 4 {
            let n = i32_at(vis, 0).unwrap_or(0);
            let n = u32::try_from(n).unwrap_or(0);
            if n > 0 && n <= MAX_CLUSTERS && (n as usize) * 8 + 4 <= vis.len() {
                clusters = n;
            }
        }
        let max_cluster = leaf_list.iter().map(|l| l.cluster).max().unwrap_or(-1);
        let bound_count = usize::try_from(i32::from(max_cluster) + 1).unwrap_or(0).max(clusters as usize);
        let mut cluster_bounds = vec![Aabb::EMPTY; bound_count];
        for l in &leaf_list {
            if let Ok(c) = usize::try_from(l.cluster)
                && let Some(b) = cluster_bounds.get_mut(c)
            {
                let min = DVec3::new(f64::from(l.min[0]), f64::from(l.min[1]), f64::from(l.min[2]));
                let max = DVec3::new(f64::from(l.max[0]), f64::from(l.max[1]), f64::from(l.max[2]));
                b.extend(min);
                b.extend(max);
            }
        }
        Ok(BspVis {
            planes: plane_list,
            nodes: node_list,
            leaves: leaf_list,
            head,
            clusters,
            vis: vis.to_vec(),
            cluster_bounds,
            area_portals: Vec::new(),
            portal_geometry: Vec::new(),
            portal_entities: Vec::new(),
        })
    }

    pub fn set_area_portals(&mut self, areas: &[u8], portals: &[u8], clip_verts: &[u8], entities: Vec<PortalEntity>) {
        let portal_count = (portals.len() / AREA_PORTAL_SIZE).min(MAX_AREA_PORTALS);
        let portal = |i: usize| -> Option<(u16, u16)> {
            if i >= portal_count {
                return None;
            }
            let o = i * AREA_PORTAL_SIZE;
            Some((u16_at(portals, o)?, u16_at(portals, o + 2)?))
        };
        self.area_portals = areas
            .chunks_exact(AREA_SIZE)
            .take(MAX_AREAS)
            .map(|a| {
                let n = usize::try_from(i32_at(a, 0).unwrap_or(0)).unwrap_or(0).min(MAX_AREA_PORTALS);
                let first = usize::try_from(i32_at(a, 4).unwrap_or(0)).unwrap_or(0);
                (first..first.saturating_add(n)).filter_map(portal).collect()
            })
            .collect();
        let vert = |i: usize| -> Option<DVec3> {
            let o = i.checked_mul(12)?;
            let v = DVec3::new(
                f64::from(f32_at(clip_verts, o)?),
                f64::from(f32_at(clip_verts, o + 4)?),
                f64::from(f32_at(clip_verts, o + 8)?),
            );
            (v.is_finite() && v.abs().max_element() <= MAX_COORD).then_some(v)
        };
        let planes = &self.planes;
        let geometry = |i: usize| -> Option<AreaPortal> {
            if i >= portal_count {
                return None;
            }
            let o = i * AREA_PORTAL_SIZE;
            let first = usize::from(u16_at(portals, o + 4)?);
            let count = usize::from(u16_at(portals, o + 6)?).min(MAX_POINTS);
            let plane = usize::try_from(i32_at(portals, o + 8)?).ok()?;
            let (normal, dist) = *planes.get(plane)?;
            Some(AreaPortal {
                key: u16_at(portals, o)?,
                other: u16_at(portals, o + 2)?,
                normal,
                dist,
                verts: (first..first + count).map_while(vert).collect(),
            })
        };
        self.portal_geometry = areas
            .chunks_exact(AREA_SIZE)
            .take(MAX_AREAS)
            .map(|a| {
                let n = usize::try_from(i32_at(a, 0).unwrap_or(0)).unwrap_or(0).min(MAX_AREA_PORTALS);
                let first = usize::try_from(i32_at(a, 4).unwrap_or(0)).unwrap_or(0);
                (first..first.saturating_add(n)).filter_map(geometry).collect()
            })
            .collect();
        self.portal_entities = entities;
    }

    pub fn portal_distance(&self, key: u16, p: DVec3) -> Option<f64> {
        let portal = self.portal_geometry.iter().flatten().find(|g| g.key == key && !g.verts.is_empty())?;
        let mut b = Aabb::EMPTY;
        for v in &portal.verts {
            b.extend(*v);
        }
        Some((p.clamp(b.min, b.max) - p).length())
    }

    pub fn flow_areas(&self, cone: &ViewCone, open: impl Fn(u16) -> bool) -> AreaFlow {
        let count = self.area_portals.len().max(self.area_count());
        let mut keys: Vec<u16> = self.portal_geometry.iter().flatten().map(|p| p.key).collect();
        keys.sort_unstable();
        keys.dedup();
        let portals_open = keys.iter().filter(|k| open(**k)).count();
        let mut flow = AreaFlow {
            own_area: None,
            visible: vec![true; count],
            rects: vec![None; count],
            cone: Some(*cone),
            portals: keys.len(),
            portals_open,
            portals_seen: 0,
        };
        let own = self.area_at(cone.eye);
        let Some(own) = own.filter(|a| usize::from(*a) < count && !keys.is_empty()) else { return flow };
        flow.own_area = Some(own);
        flow.visible = vec![false; count];
        let mut on_stack = vec![false; count];
        let mut seen_keys: Vec<u16> = Vec::new();
        let mut walk = FlowWalk { cone, open: &open, flow: &mut flow, on_stack: &mut on_stack, seen: &mut seen_keys };
        self.flow_through(own, &ScreenRect::FULL, &mut walk, 0);
        seen_keys.sort_unstable();
        seen_keys.dedup();
        flow.portals_seen = seen_keys.len();
        flow
    }

    fn flow_through(&self, area: u16, clip: &ScreenRect, w: &mut FlowWalk<'_, impl Fn(u16) -> bool>, depth: usize) {
        let a = usize::from(area);
        if a >= w.flow.visible.len() || depth > MAX_AREAS {
            return;
        }
        w.flow.visible[a] = true;
        w.flow.rects[a] = Some(w.flow.rects[a].map_or(*clip, |r| r.union(clip)));
        w.on_stack[a] = true;
        for p in self.portal_geometry.get(a).map(Vec::as_slice).unwrap_or(&[]) {
            if w.on_stack.get(usize::from(p.other)).copied().unwrap_or(true) || !(w.open)(p.key) {
                continue;
            }
            let dist = p.normal.dot(w.cone.eye) - p.dist;
            if dist < -0.1 {
                continue;
            }
            let window = if dist > 0.1 { w.cone.screen_extents(&p.verts) } else { Some(ScreenRect::FULL) };
            let Some(window) = window.and_then(|r| r.intersect(clip)) else { continue };
            w.seen.push(p.key);
            self.flow_through(p.other, &window, w, depth + 1);
        }
        w.on_stack[a] = false;
    }

    pub fn portal_entities(&self) -> &[PortalEntity] {
        &self.portal_entities
    }

    pub fn area_at(&self, p: DVec3) -> Option<u16> {
        let mut node = self.head;
        for _ in 0..=self.nodes.len() {
            if node < 0 {
                let leaf = self.leaves.get(usize::try_from(-1 - node).ok()?)?;
                return (leaf.cluster >= 0).then_some(leaf.area);
            }
            let (plane, children) = *self.nodes.get(usize::try_from(node).ok()?)?;
            let (n, d) = *self.planes.get(plane as usize)?;
            node = if n.dot(p) - d >= 0.0 { children[0] } else { children[1] };
        }
        None
    }

    pub fn reachable_areas(&self, from: u16, open: impl Fn(u16) -> bool) -> Vec<bool> {
        let count = self.area_portals.len().max(self.area_count());
        if self.area_portals.iter().all(Vec::is_empty) {
            return vec![true; count];
        }
        let mut seen = vec![false; count];
        let mut stack = vec![usize::from(from)];
        while let Some(a) = stack.pop() {
            if a >= seen.len() || seen[a] {
                continue;
            }
            seen[a] = true;
            for &(key, other) in self.area_portals.get(a).map(Vec::as_slice).unwrap_or(&[]) {
                if open(key) && !seen.get(usize::from(other)).copied().unwrap_or(true) {
                    stack.push(usize::from(other));
                }
            }
        }
        seen
    }

    #[cfg(test)]
    pub(crate) fn for_tests(clusters: u32, vis: Vec<u8>) -> BspVis {
        BspVis { clusters, vis, ..BspVis::default() }
    }

    pub fn area_portal_pairs(&self) -> Vec<(u16, u16, u16)> {
        let mut out: Vec<(u16, u16, u16)> = Vec::new();
        for (area, list) in self.area_portals.iter().enumerate() {
            let Ok(area) = u16::try_from(area) else { break };
            for &(key, other) in list {
                let pair = (key, area.min(other), area.max(other));
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
        out.sort_unstable();
        out
    }

    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    pub fn leaves(&self) -> impl Iterator<Item = (i16, u16, Aabb)> + '_ {
        self.leaves.iter().map(|l| {
            let min = DVec3::new(f64::from(l.min[0]), f64::from(l.min[1]), f64::from(l.min[2]));
            let max = DVec3::new(f64::from(l.max[0]), f64::from(l.max[1]), f64::from(l.max[2]));
            (l.cluster, l.area, Aabb::new(min, max))
        })
    }

    pub fn cluster_count(&self) -> u32 {
        self.clusters
    }

    pub fn has_visibility(&self) -> bool {
        self.clusters > 0
    }

    pub fn area_count(&self) -> usize {
        self.leaves.iter().map(|l| usize::from(l.area)).max().map_or(0, |a| a + 1)
    }

    pub fn cluster_bounds(&self) -> &[Aabb] {
        &self.cluster_bounds
    }

    pub fn cluster_areas(&self) -> Vec<u16> {
        let mut out = vec![0u16; self.cluster_bounds.len()];
        for l in &self.leaves {
            if let Ok(c) = usize::try_from(l.cluster)
                && let Some(a) = out.get_mut(c)
                && *a == 0
            {
                *a = l.area;
            }
        }
        out
    }

    pub fn cluster_at(&self, p: DVec3) -> Option<u32> {
        let mut node = self.head;
        for _ in 0..=self.nodes.len() {
            if node < 0 {
                let leaf = self.leaves.get(usize::try_from(-1 - node).ok()?)?;
                return u32::try_from(leaf.cluster).ok();
            }
            let (plane, children) = *self.nodes.get(usize::try_from(node).ok()?)?;
            let (n, d) = *self.planes.get(plane as usize)?;
            node = if n.dot(p) - d >= 0.0 { children[0] } else { children[1] };
        }
        None
    }

    pub fn clusters_in_box(&self, min: DVec3, max: DVec3) -> Vec<u32> {
        let center = (min + max) * 0.5;
        let half = (max - min) * 0.5;
        let mut out = Vec::new();
        let mut stack = vec![self.head];
        let mut steps = 0usize;
        while let Some(node) = stack.pop() {
            steps += 1;
            if steps > self.nodes.len() + self.leaves.len() + 1 {
                break;
            }
            if node < 0 {
                if let Some(leaf) = usize::try_from(-1 - node).ok().and_then(|i| self.leaves.get(i))
                    && let Ok(c) = u32::try_from(leaf.cluster)
                {
                    out.push(c);
                }
                continue;
            }
            let Some(&(plane, children)) = usize::try_from(node).ok().and_then(|i| self.nodes.get(i)) else {
                continue;
            };
            let Some(&(n, d)) = self.planes.get(plane as usize) else { continue };
            let dist = n.dot(center) - d;
            let reach = n.abs().dot(half);
            if dist >= -reach {
                stack.push(children[0]);
            }
            if dist <= reach {
                stack.push(children[1]);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn visible_from(&self, cluster: u32) -> Option<Vec<bool>> {
        if cluster >= self.clusters {
            return None;
        }
        let ofs = i32_at(&self.vis, 4 + cluster as usize * 8)?;
        let mut pos = usize::try_from(ofs).ok()?;
        let n = self.clusters as usize;
        let row = n.div_ceil(8);
        let mut bits: Vec<u8> = Vec::with_capacity(row);
        while bits.len() < row {
            let b = *self.vis.get(pos)?;
            pos += 1;
            if b != 0 {
                bits.push(b);
            } else {
                let run = usize::from(*self.vis.get(pos)?);
                pos += 1;
                if run == 0 {
                    return None;
                }
                let run = run.min(row - bits.len());
                bits.extend(std::iter::repeat_n(0u8, run));
            }
        }
        let mut out = vec![false; n];
        for (i, v) in out.iter_mut().enumerate() {
            *v = bits[i / 8] & (1 << (i % 8)) != 0;
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRT: &str =
        "PRT1\n3\n2\n4 0 1 (0 0 0 ) (0 64 0 ) (0 64 64 ) (0 0 64 ) \n3 1 2 (64 0 0) (64 64 0) (64 0 64)\n";

    #[test]
    fn parses_portal_files() {
        let p = parse_prt(PRT).expect("prt");
        assert_eq!(p.clusters, 3);
        assert_eq!(p.portals.len(), 2);
        assert_eq!(p.portals[0].clusters, [0, 1]);
        assert_eq!(p.portals[0].points[2], DVec3::new(0.0, 64.0, 64.0));
        assert_eq!(p.portals[1].points.len(), 3);
        assert_eq!(p.bounds.max, DVec3::new(64.0, 64.0, 64.0));
        assert_eq!(p.portals_of(1).count(), 2);
        let crlf = PRT.replace('\n', "\r\n") + "0 1 -1\r\n";
        assert_eq!(parse_prt(&crlf).expect("crlf").portals.len(), 2);
    }

    #[test]
    fn rejects_bad_portal_files() {
        assert_eq!(parse_prt(""), Err(PortalError::NotPrt));
        assert_eq!(parse_prt("PRT2\n1\n0\n"), Err(PortalError::NotPrt));
        assert!(parse_prt("PRT1\nx\n").is_err());
        assert!(parse_prt("PRT1\n2\n1\n3 0 5 (0 0 0) (1 0 0) (0 1 0)\n").is_err(), "cluster out of range");
        assert!(parse_prt("PRT1\n2\n1\n4 0 1 (0 0 0) (1 0 0) (0 1 0)\n").is_err(), "missing point");
        assert!(parse_prt("PRT1\n2\n1\n3 0 1 (0 0 nan) (1 0 0) (0 1 0)\n").is_err());
        assert!(parse_prt("PRT1\n2\n1\n99999 0 1 (0 0 0)\n").is_err());
        let p = parse_prt("PRT1\n2\n4000000000\n3 0 1 (0 0 0) (1 0 0) (0 1 0)\n").expect("prt");
        assert_eq!(p.portals.len(), 1);
    }

    fn tiny_bsp() -> BspVis {
        let mut planes = Vec::new();
        for f in [1.0f32, 0.0, 0.0, 0.0] {
            planes.extend_from_slice(&f.to_le_bytes());
        }
        planes.extend_from_slice(&0i32.to_le_bytes());
        let mut nodes = Vec::new();
        for v in [0i32, -1, -2] {
            nodes.extend_from_slice(&v.to_le_bytes());
        }
        nodes.extend_from_slice(&[0u8; 20]);
        let mut leafs = Vec::new();
        for (cluster, area, minx, maxx) in [(0i16, 1i16, 0i16, 64i16), (1, 2, -64, 0)] {
            leafs.extend_from_slice(&0i32.to_le_bytes());
            leafs.extend_from_slice(&cluster.to_le_bytes());
            leafs.extend_from_slice(&area.to_le_bytes());
            for v in [minx, -64, -64, maxx, 64, 64] {
                leafs.extend_from_slice(&v.to_le_bytes());
            }
            leafs.extend_from_slice(&[0u8; 12]);
        }
        let mut models = vec![0u8; 36];
        models.extend_from_slice(&0i32.to_le_bytes());
        models.extend_from_slice(&[0u8; 8]);
        let mut vis = Vec::new();
        vis.extend_from_slice(&3i32.to_le_bytes());
        for (pvs, pas) in [(28i32, 28i32), (29, 29), (30, 30)] {
            vis.extend_from_slice(&pvs.to_le_bytes());
            vis.extend_from_slice(&pas.to_le_bytes());
        }
        vis.extend_from_slice(&[0b011, 0b011, 0b100]);
        BspVis::from_lumps(&planes, &nodes, &leafs, 1, &models, &vis).expect("vis")
    }

    #[test]
    fn decodes_visibility() {
        let v = tiny_bsp();
        assert!(v.has_visibility());
        assert_eq!(v.cluster_count(), 3);
        assert_eq!(v.cluster_at(DVec3::new(10.0, 0.0, 0.0)), Some(0));
        assert_eq!(v.cluster_at(DVec3::new(-10.0, 0.0, 0.0)), Some(1));
        assert_eq!(v.visible_from(0), Some(vec![true, true, false]));
        assert_eq!(v.visible_from(2), Some(vec![false, false, true]));
        assert_eq!(v.visible_from(3), None);
        assert_eq!(v.area_count(), 3);
        assert_eq!(v.cluster_areas(), vec![1, 2, 0]);
        assert_eq!(v.cluster_bounds()[1].min.x, -64.0);
    }

    fn portal_bsp() -> BspVis {
        let mut v = tiny_bsp();
        v.planes.push((DVec3::NEG_X, 0.0));
        let mut areas = Vec::new();
        for (n, first) in [(0i32, 0i32), (1, 0), (1, 1)] {
            areas.extend_from_slice(&n.to_le_bytes());
            areas.extend_from_slice(&first.to_le_bytes());
        }
        let mut portals = Vec::new();
        for (other, plane) in [(2u16, 0i32), (1, 1)] {
            for h in [7u16, other, 0, 4] {
                portals.extend_from_slice(&h.to_le_bytes());
            }
            portals.extend_from_slice(&plane.to_le_bytes());
        }
        let mut verts = Vec::new();
        for (y, z) in [(-32.0f32, -32.0f32), (32.0, -32.0), (32.0, 32.0), (-32.0, 32.0)] {
            for c in [0.0f32, y, z] {
                verts.extend_from_slice(&c.to_le_bytes());
            }
        }
        v.set_area_portals(&areas, &portals, &verts, Vec::new());
        v
    }

    fn cone(eye: DVec3, forward: DVec3) -> ViewCone {
        let right = forward.cross(DVec3::Z).normalize();
        ViewCone { eye, forward, right, up: right.cross(forward), tan_x: 1.0, tan_y: 1.0 }
    }

    #[test]
    fn area_flood_goes_through_open_portals_in_view() {
        let v = portal_bsp();
        let eye = DVec3::new(48.0, 0.0, 0.0);
        let f = v.flow_areas(&cone(eye, DVec3::NEG_X), |_| true);
        assert_eq!(f.own_area, Some(1));
        assert_eq!((f.portals, f.portals_open, f.portals_seen), (1, 1, 1));
        assert!(f.visible[1] && f.visible[2]);
        assert_eq!(f.visible_areas(), 2);
        let r = f.rects[2].expect("window");
        assert!(r.left > -1.0 && r.right < 1.0 && r.bottom > -1.0 && r.top < 1.0, "{r:?}");
        let small = DVec3::splat(4.0);
        let behind = DVec3::new(-40.0, 0.0, 0.0);
        assert!(f.box_visible(2, behind - small, behind + small));
        let aside = DVec3::new(-40.0, 300.0, 0.0);
        assert!(!f.box_visible(2, aside - small, aside + small));
        assert!(f.box_visible(1, aside - small, aside + small));
        let f = v.flow_areas(&cone(eye, DVec3::X), |_| true);
        assert!(f.visible[1] && !f.visible[2]);
        assert_eq!(f.portals_seen, 0);
        let f = v.flow_areas(&cone(eye, DVec3::NEG_X), |_| false);
        assert!(!f.visible[2]);
        assert_eq!(f.portals_open, 0);
        let f = tiny_bsp().flow_areas(&cone(eye, DVec3::X), |_| false);
        assert_eq!((f.own_area, f.portals), (None, 0));
        assert!(f.box_visible(2, aside - small, aside + small));
        assert_eq!(v.portal_distance(7, DVec3::new(10.0, 0.0, 0.0)), Some(10.0));
        assert_eq!(v.portal_distance(8, DVec3::ZERO), None);
    }

    #[test]
    fn hostile_lumps_never_panic() {
        assert!(BspVis::from_lumps(&[], &[], &[], 1, &[], &[]).is_err());
        let v = tiny_bsp();
        let mut cyc = v.clone();
        cyc.nodes = vec![(0, [0, 0])];
        assert_eq!(cyc.cluster_at(DVec3::ONE), None);
        let mut far = v.clone();
        far.vis[4..8].copy_from_slice(&1_000_000i32.to_le_bytes());
        assert_eq!(far.visible_from(0), None);
        let mut zero = v;
        zero.vis[28] = 0;
        zero.vis[29] = 0;
        assert_eq!(zero.visible_from(0), None);
    }
}
