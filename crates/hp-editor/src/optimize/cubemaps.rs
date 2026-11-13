use std::collections::HashMap;

use glam::DVec3;
use hp_geom::Aabb;

use super::{Finding, Impact, SolidIndex, Topic, fmt_point};
use crate::limits::portals::BspVis;
use crate::selection::ObjRef;
use crate::{Document, EditError, EditResult, Id};

pub const EYE_HEIGHT: f64 = 64.0;
pub const FLOOR_LOW: f64 = 32.0;
pub const TOO_CLOSE: f64 = 128.0;
pub const FAR: f64 = 1024.0;
pub const ROOM_REACH: f64 = 1024.0;
pub const LARGE_ROOM: f64 = 1024.0;
pub const MAX_SIDES: usize = 128;
pub const MANY: usize = 128;
pub const MAX_PROBES: usize = 1023;
pub const DEFAULT_PIXELS: u32 = 128;
pub const LARGE_SIZE: i32 = 9;
pub const SMALL_SIZE: i32 = 5;
const FRONT: f64 = 4.0;
const LIFT: f64 = 16.0;
const MAX_ANCHORS: usize = 16;
const BIG_FACE: f64 = 65_536.0;
pub const MIN_SUGGEST_SURFACE: f64 = 128.0 * 128.0;
pub const MIN_STRENGTH: f64 = 0.05;
const MERGE_PAIRS: usize = 6;
const FAR_WEIGHT: f64 = 4.0;
const ANCHOR_FACE: f64 = 4096.0;
const FACADE_RADIUS: f64 = 1024.0;
const FACADE_PASSES: usize = 3;
const MAX_FACADES: usize = 48;
const FACADE_DISTANCES: [f64; 8] = [384.0, 256.0, 512.0, 640.0, 192.0, 768.0, 128.0, 96.0];
const ANCHOR_GAP: f64 = 256.0;
const CLEARANCE: f64 = 8.0;
const MAX_SPLIT: usize = 4;
const SAMPLE_FACES: usize = 24;
const MAX_ROOM_FACES: usize = 40_000;
const OPENING_WIDTH: f64 = 160.0;
const SMALL_FACE: f64 = 1024.0;
const ROOM_RAISE: f64 = 24.0;
const NARROWER: f64 = 0.6;
const PRESSED: f64 = 8.0;
const SPOTS: usize = 8;
const OPENING_HEIGHT: f64 = 100.0;
const OPENING_PROBE: f64 = 512.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialClass {
    Water,
    Glass,
    Metal,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Surface {
    Water,
    Glass,
    Metal,
    Floor,
    Wall,
    Ceiling,
}

impl Surface {
    pub const ALL: [Surface; 6] =
        [Surface::Water, Surface::Glass, Surface::Metal, Surface::Floor, Surface::Wall, Surface::Ceiling];

    pub fn id(self) -> &'static str {
        match self {
            Surface::Water => "water",
            Surface::Glass => "glass",
            Surface::Metal => "metal",
            Surface::Floor => "floor",
            Surface::Wall => "wall",
            Surface::Ceiling => "ceiling",
        }
    }

    fn of(class: Option<MaterialClass>, normal: DVec3) -> Surface {
        match class {
            Some(MaterialClass::Water) => Surface::Water,
            Some(MaterialClass::Glass) => Surface::Glass,
            Some(MaterialClass::Metal) => Surface::Metal,
            _ if normal.z > 0.7 => Surface::Floor,
            _ if normal.z < -0.7 => Surface::Ceiling,
            _ => Surface::Wall,
        }
    }
}

pub fn class_by_name(material: &str) -> Option<MaterialClass> {
    let m = crate::glob::normalize_material(material).to_ascii_lowercase();
    if m.starts_with("tools/") || m.is_empty() {
        return None;
    }
    if m.starts_with("liquids/") || m.contains("water") {
        return Some(MaterialClass::Water);
    }
    if m.contains("glass") || m.contains("window") {
        return Some(MaterialClass::Glass);
    }
    if ["metal", "chrome", "steel", "mirror"].iter().any(|k| m.contains(k)) {
        return Some(MaterialClass::Metal);
    }
    if ["marble", "polished", "shiny", "tile"].iter().any(|k| m.contains(k)) {
        return Some(MaterialClass::Other);
    }
    None
}

pub fn class_from_material(name: &str, shader_water: bool, translucent: bool) -> MaterialClass {
    if shader_water {
        return MaterialClass::Water;
    }
    match class_by_name(name) {
        Some(c) => c,
        None if translucent => MaterialClass::Glass,
        None => MaterialClass::Other,
    }
}

pub struct CubemapKnowledge<'a> {
    pub material: &'a dyn Fn(&str) -> Option<MaterialClass>,
    pub strength: &'a dyn Fn(&str) -> f64,
}

pub fn full_strength(_material: &str) -> f64 {
    1.0
}

pub fn parse_tint(value: &str) -> Option<f64> {
    let t = value.trim();
    let bytes = t.starts_with('{');
    let parts: Vec<f64> = t
        .trim_matches(|c| matches!(c, '[' | ']' | '{' | '}'))
        .split_whitespace()
        .filter_map(|x| x.parse::<f64>().ok())
        .collect();
    let scale = if bytes { 1.0 / 255.0 } else { 1.0 };
    match parts.as_slice() {
        [v] => Some(v * scale),
        [r, g, b, ..] => Some((0.2126 * r + 0.7152 * g + 0.0722 * b) * scale),
        _ => None,
    }
    .filter(|v| v.is_finite())
    .map(|v| v.max(0.0))
}

pub fn size_pixels(size: i32) -> u32 {
    if size <= 0 { DEFAULT_PIXELS } else { 1u32 << (size.min(12) - 1) }
}

pub fn size_bytes(size: i32) -> u64 {
    let s = u64::from(size_pixels(size));
    6 * s * s * 4 * 4 / 3
}

#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub id: Id,
    pub origin: DVec3,
    pub size: i32,
    pub sides: Vec<u32>,
    pub room: Option<usize>,
    pub faces: usize,
    pub floor_gap: Option<f64>,
    pub inside: bool,
    pub outside: bool,
    pub area: Option<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceUse {
    pub solid: Id,
    pub face: usize,
    pub side: u32,
    pub center: DVec3,
    pub normal: DVec3,
    pub area: f64,
    pub surface: Surface,
    pub reflective: bool,
    pub strength: f64,
    pub probe: Option<usize>,
    pub manual: bool,
    pub distance: f64,
    pub blocked: bool,
    pub foreign: bool,
    pub room: Option<usize>,
    pub spot: DVec3,
    pub spots: Vec<DVec3>,
}

impl FaceUse {
    pub fn wrong(&self) -> bool {
        self.reflective && (self.blocked || self.foreign)
    }

    pub fn front(&self) -> DVec3 {
        self.spot + self.normal * FRONT
    }

    pub fn sees(&self, idx: &SolidIndex, p: DVec3) -> bool {
        self.spots.iter().any(|s| idx.clear_line(*s + self.normal * FRONT, p))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub faces: Vec<usize>,
    pub center: DVec3,
    pub bounds: Aabb,
    pub surface: f64,
    pub probes: Vec<usize>,
    pub area: Option<u16>,
    pub kinds: Vec<(Surface, usize)>,
    pub wrong: usize,
    anchors: Vec<DVec3>,
}

impl Room {
    pub fn kind_text(&self) -> String {
        self.kinds.iter().map(|(k, n)| format!("{} {n}", k.id())).collect::<Vec<_>>().join(", ")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub room: Option<usize>,
    pub origin: DVec3,
    pub size: i32,
    pub sides: Vec<u32>,
    pub faces: usize,
    pub probe: Option<Id>,
    pub reason: &'static str,
}

impl Suggestion {
    pub fn is_new(&self) -> bool {
        self.probe.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CubemapReport {
    pub probes: Vec<Probe>,
    pub faces: Vec<FaceUse>,
    pub rooms: Vec<Room>,
    pub suggestions: Vec<Suggestion>,
    pub findings: Vec<Finding>,
    pub memory_bytes: u64,
    pub reflective: usize,
    pub blocked: usize,
    pub wrong: usize,
    pub unfixable: usize,
    pub far: usize,
    pub compiler_areas: bool,
    pub revision: u64,
}

impl CubemapReport {
    pub fn probe_index(&self, id: Id) -> Option<usize> {
        self.probes.iter().position(|p| p.id == id)
    }
}

fn atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (neg, digits) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut v: i64 = 0;
    for c in digits.chars() {
        let Some(d) = c.to_digit(10) else { break };
        v = (v * 10 + i64::from(d)).min(1 << 30);
    }
    (if neg { -v } else { v }) as i32
}

pub fn parse_sides(s: &str) -> Vec<u32> {
    s.split_whitespace().filter_map(|t| t.parse::<i64>().ok()).filter(|v| *v > 0).map(|v| v as u32).collect()
}

pub fn nearest_in_front(origins: &[DVec3], center: DVec3, normal: DVec3) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None;
    for (i, o) in origins.iter().enumerate() {
        let d = *o - center;
        let dist = d.length();
        let dir = if dist > 1e-10 { d / dist } else { DVec3::ZERO };
        if dir.dot(normal) >= 0.0 && best.is_none_or(|b| dist < b.0) {
            best = Some((dist, i));
        }
    }
    if best.is_some() {
        return best.map(|b| b.1);
    }
    for (i, o) in origins.iter().enumerate() {
        let dist = o.distance(center);
        if best.is_none_or(|b| dist < b.0) {
            best = Some((dist, i));
        }
    }
    best.map(|b| b.1)
}

fn skipped(material: &str) -> bool {
    let m = crate::glob::normalize_material(material).to_ascii_lowercase();
    m.starts_with("tools/") || m.is_empty()
}

fn probes_of(doc: &Document) -> Vec<Probe> {
    let mut list: Vec<&crate::scene::EntityNode> =
        doc.scene.entities.values().filter(|n| n.entity.classname.trim().eq_ignore_ascii_case("env_cubemap")).collect();
    list.sort_by_key(|n| n.order());
    list.into_iter()
        .filter_map(|n| {
            let origin = n.entity.origin()?;
            origin.is_finite().then(|| Probe {
                id: n.entity.id,
                origin,
                size: atoi(n.entity.get("cubemapsize").unwrap_or("0")),
                sides: parse_sides(n.entity.get("sides").unwrap_or("")),
                room: None,
                faces: 0,
                floor_gap: None,
                inside: false,
                outside: false,
                area: None,
            })
        })
        .take(MAX_PROBES)
        .collect()
}

fn visible_spots(idx: &SolidIndex, f: &hp_geom::FaceGeom) -> Vec<DVec3> {
    let center = f.center();
    let normal = f.normal();
    let step = f.vertices.len().div_ceil(SPOTS).max(1);
    std::iter::once(center)
        .chain(f.vertices.iter().step_by(step).map(|v| center + (*v - center) * 0.85))
        .filter(|p| idx.contains(*p + normal).is_none() && idx.ray_hit(*p + normal * 0.5, normal, PRESSED).is_none())
        .collect()
}

fn faces_of(doc: &Document, k: &CubemapKnowledge<'_>, idx: &SolidIndex) -> Vec<FaceUse> {
    let mut memo: HashMap<String, (Option<MaterialClass>, f64)> = HashMap::new();
    let mut out = Vec::new();
    for n in doc.scene.solids.values() {
        if !n.geom.valid || !doc.is_visible(ObjRef::Solid(n.solid.id)) {
            continue;
        }
        for (fi, f) in n.geom.faces.iter().enumerate() {
            let Some(side) = n.solid.sides.get(f.side_index) else { continue };
            if f.vertices.len() < 3 || skipped(&side.material) {
                continue;
            }
            let spots = visible_spots(idx, f);
            let Some(&spot) = spots.first() else { continue };
            let (class, strength) = *memo.entry(side.material.to_ascii_lowercase()).or_insert_with(|| {
                let class = (k.material)(&side.material);
                (class, if class.is_some() { (k.strength)(&side.material) } else { 0.0 })
            });
            let class = class.filter(|_| strength >= MIN_STRENGTH);
            let normal = f.normal();
            out.push(FaceUse {
                solid: n.solid.id,
                face: fi,
                side: side.id,
                center: f.center(),
                normal,
                area: f.area(),
                surface: Surface::of(class, normal),
                reflective: class.is_some(),
                strength,
                probe: None,
                manual: false,
                distance: 0.0,
                blocked: false,
                foreign: false,
                room: None,
                spot,
                spots,
            });
        }
    }
    out
}

fn facing(f: &FaceUse, origin: DVec3) -> bool {
    (origin - f.center).dot(f.normal) >= 0.0
}

fn floor_below(idx: &SolidIndex, p: DVec3) -> Option<f64> {
    idx.ray_hit(p, DVec3::NEG_Z, 4096.0)
}

fn through_opening(idx: &SolidIndex, a: DVec3, b: DVec3) -> bool {
    let d = b - a;
    let len = d.length();
    if len < OPENING_WIDTH {
        return false;
    }
    let flat = DVec3::new(d.x, d.y, 0.0);
    let side = if flat.length() > 1e-6 { flat.cross(DVec3::Z).normalize() } else { DVec3::ZERO };
    let reach = |p: DVec3, dir: DVec3| idx.ray_hit(p, dir, OPENING_PROBE).unwrap_or(OPENING_PROBE);
    let width = |p: DVec3| if side == DVec3::ZERO { f64::INFINITY } else { reach(p, side) + reach(p, -side) };
    let height = |p: DVec3| reach(p, DVec3::Z) + reach(p, DVec3::NEG_Z);
    let u = d / len;
    let near_a = [0.0f64, 24.0, 48.0].map(|t| a + u * t.min(len * 0.5));
    let near_b = [0.0f64, 24.0, 48.0].map(|t| b - u * t.min(len * 0.5));
    let widest = |ps: &[DVec3; 3], f: &dyn Fn(DVec3) -> f64| ps.iter().map(|p| f(*p)).fold(0.0, f64::max);
    let end_width = widest(&near_a, &width).min(widest(&near_b, &width));
    let end_height = widest(&near_a, &height).min(widest(&near_b, &height));
    let steps = ((len / 24.0).ceil() as usize).clamp(4, 48);
    (1..steps).any(|i| {
        let t = len * i as f64 / steps as f64;
        if t < 24.0 || t > len - 24.0 {
            return false;
        }
        let p = a + u * t;
        let w = width(p);
        let h = height(p);
        (w < OPENING_WIDTH && w < end_width * NARROWER) || (h < OPENING_HEIGHT && h < end_height * NARROWER)
    })
}

fn room_point(idx: &SolidIndex, f: &FaceUse) -> DVec3 {
    point_above(idx, f.spot, f.normal)
}

fn point_above(idx: &SolidIndex, spot: DVec3, normal: DVec3) -> DVec3 {
    let base = spot + normal * LIFT;
    if idx.contains(base).is_some() || !idx.clear_line(spot + normal, base) {
        return spot + normal;
    }
    let up = base + DVec3::Z * ROOM_RAISE;
    if idx.clear_line(base, up) && idx.contains(up).is_none() { up } else { base }
}

fn sees(idx: &SolidIndex, a: DVec3, b: DVec3) -> bool {
    idx.clear_line(a, b) && !through_opening(idx, a, b)
}

fn group_rooms(faces: &mut [FaceUse], idx: &SolidIndex, vis: Option<&BspVis>) -> Vec<Room> {
    let mut order: Vec<usize> = (0..faces.len()).filter(|&i| faces[i].reflective).collect();
    order.sort_by(|a, b| faces[*b].area.total_cmp(&faces[*a].area).then(a.cmp(b)));
    order.truncate(MAX_ROOM_FACES);
    let mut rooms: Vec<Room> = Vec::new();
    for i in order {
        let fp = room_point(idx, &faces[i]);
        let area = vis.and_then(|v| v.area_at(fp));
        let mut found = rooms.iter().position(|r| {
            if let (Some(a), Some(b)) = (area, r.area)
                && a != b
            {
                return false;
            }
            let mut near: Vec<&DVec3> = r.anchors.iter().filter(|p| p.distance(fp) <= ROOM_REACH).collect();
            near.sort_by(|x, y| x.distance(fp).total_cmp(&y.distance(fp)));
            near.into_iter().take(4).any(|p| sees(idx, fp, *p))
        });
        if found.is_none() && faces[i].area < SMALL_FACE {
            found = rooms
                .iter()
                .enumerate()
                .filter(|(_, r)| area.is_none() || r.area.is_none() || area == r.area)
                .map(|(ri, r)| (ri, r.anchors.iter().map(|a| a.distance(fp)).fold(f64::INFINITY, f64::min)))
                .filter(|(_, d)| *d <= ROOM_REACH * 0.5)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(ri, _)| ri);
        }
        let r = match found {
            Some(r) => r,
            None => {
                rooms.push(Room {
                    faces: Vec::new(),
                    center: DVec3::ZERO,
                    bounds: Aabb::EMPTY,
                    surface: 0.0,
                    probes: Vec::new(),
                    area,
                    kinds: Vec::new(),
                    wrong: 0,
                    anchors: vec![fp],
                });
                rooms.len() - 1
            }
        };
        let room = &mut rooms[r];
        room.faces.push(i);
        if room.area.is_none() {
            room.area = area;
        }
        let big = faces[i].area > BIG_FACE;
        let extra = faces[i].spots.iter().skip(1).filter(|_| big).map(|s| point_above(idx, *s, faces[i].normal));
        let anchor = faces[i].area >= ANCHOR_FACE || room.anchors.is_empty();
        for p in std::iter::once(fp).filter(|_| anchor).chain(extra) {
            if room.anchors.len() < MAX_ANCHORS && room.anchors.iter().all(|a| a.distance(p) > ANCHOR_GAP) {
                room.anchors.push(p);
            }
        }
        faces[i].room = Some(r);
    }
    let mut rooms = merge_rooms(rooms, faces, idx);
    for room in &mut rooms {
        let mut w = 0.0;
        let mut c = DVec3::ZERO;
        let mut kinds: HashMap<Surface, usize> = HashMap::new();
        for &i in &room.faces {
            let f = &faces[i];
            let a = f.area.max(1.0);
            w += a;
            c += f.center * a;
            let half = f.area.max(0.0).sqrt() * 0.5;
            let n = f.normal.abs();
            let spread = if n.z >= n.x && n.z >= n.y {
                DVec3::new(half, half, 0.0)
            } else if n.x >= n.y {
                DVec3::new(0.0, half, half)
            } else {
                DVec3::new(half, 0.0, half)
            };
            room.bounds.extend(f.center - spread);
            room.bounds.extend(f.center + spread);
            *kinds.entry(f.surface).or_default() += 1;
        }
        room.center = if w > 0.0 { c / w } else { DVec3::ZERO };
        room.surface = room.faces.iter().map(|&i| faces[i].area * faces[i].strength.min(1.0)).sum();
        let mut k: Vec<(Surface, usize)> = kinds.into_iter().collect();
        k.sort();
        room.kinds = k;
    }
    rooms
}

fn rooms_meet(a: &Room, b: &Room, small: bool, idx: &SolidIndex) -> bool {
    if let (Some(x), Some(y)) = (a.area, b.area)
        && x != y
    {
        return false;
    }
    let reach = if small { ROOM_REACH * 0.5 } else { ROOM_REACH };
    let mut pairs: Vec<(f64, DVec3, DVec3)> = a
        .anchors
        .iter()
        .flat_map(|p| b.anchors.iter().map(move |q| (p.distance(*q), *p, *q)))
        .filter(|(d, _, _)| *d <= reach)
        .collect();
    pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
    pairs.into_iter().take(MERGE_PAIRS).any(|(_, p, q)| if small { idx.clear_line(p, q) } else { sees(idx, p, q) })
}

fn merge_rooms(rooms: Vec<Room>, faces: &mut [FaceUse], idx: &SolidIndex) -> Vec<Room> {
    let n = rooms.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while parent[r] != r {
            r = parent[r];
        }
        let mut c = i;
        while parent[c] != r {
            let next = parent[c];
            parent[c] = r;
            c = next;
        }
        r
    }
    let raw: Vec<f64> =
        rooms.iter().map(|r| r.faces.iter().map(|&i| faces[i].area * faces[i].strength.min(1.0)).sum()).collect();
    let small: Vec<bool> = raw.iter().map(|w| *w < MIN_SUGGEST_SURFACE).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if small[i] || small[j] || find(&mut parent, i) == find(&mut parent, j) {
                continue;
            }
            if rooms_meet(&rooms[i], &rooms[j], false, idx) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                parent[b.max(a)] = a.min(b);
            }
        }
    }
    for i in (0..n).filter(|&i| small[i]) {
        let gap = |j: usize| {
            rooms[i]
                .anchors
                .iter()
                .flat_map(|p| rooms[j].anchors.iter().map(move |q| p.distance(*q)))
                .fold(f64::INFINITY, f64::min)
        };
        let mut near: Vec<(f64, usize)> = (0..n).filter(|&j| j != i && !small[j]).map(|j| (gap(j), j)).collect();
        near.sort_by(|x, y| x.0.total_cmp(&y.0));
        if let Some(&(_, j)) = near.iter().take(3).find(|(_, j)| rooms_meet(&rooms[i], &rooms[*j], true, idx)) {
            let root = find(&mut parent, j);
            parent[i] = root;
        }
    }
    let mut out: Vec<Room> = Vec::new();
    let mut slot: Vec<Option<usize>> = vec![None; n];
    for (i, room) in rooms.into_iter().enumerate() {
        let root = find(&mut parent, i);
        match slot[root] {
            Some(k) => {
                let target = &mut out[k];
                target.faces.extend(room.faces);
                if target.area.is_none() {
                    target.area = room.area;
                }
                for p in room.anchors {
                    if target.anchors.len() < MAX_ANCHORS * 2
                        && target.anchors.iter().all(|a| a.distance(p) > ANCHOR_GAP)
                    {
                        target.anchors.push(p);
                    }
                }
            }
            None => {
                slot[root] = Some(out.len());
                out.push(room);
            }
        }
    }
    for (k, room) in out.iter().enumerate() {
        for &f in &room.faces {
            faces[f].room = Some(k);
        }
    }
    out
}

fn serves(room: &Room, p: &Probe, idx: &SolidIndex) -> Option<f64> {
    if let (Some(a), Some(b)) = (room.area, p.area)
        && a != b
    {
        return None;
    }
    room.anchors
        .iter()
        .filter(|a| a.distance(p.origin) <= ROOM_REACH * 1.5)
        .filter(|a| sees(idx, p.origin, **a))
        .map(|a| a.distance(p.origin))
        .min_by(f64::total_cmp)
}

fn free(idx: &SolidIndex, p: DVec3) -> bool {
    idx.contains(p).is_none()
        && [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z]
            .iter()
            .all(|d| idx.contains(p + *d * CLEARANCE).is_none())
}

fn eye_height(idx: &SolidIndex, faces: &[&FaceUse], q: DVec3) -> f64 {
    let top_floor = faces
        .iter()
        .filter(|f| f.normal.z > 0.7 && f.center.z <= q.z + 1.0)
        .map(|f| f.center.z)
        .fold(f64::NEG_INFINITY, f64::max);
    let ray_floor = floor_below(idx, q).map(|t| q.z - t);
    let floor = match ray_floor {
        Some(r) if top_floor.is_finite() => Some(r.max(top_floor)),
        Some(r) => Some(r),
        None => top_floor.is_finite().then_some(top_floor),
    };
    let ceil = idx.ray_hit(q, DVec3::Z, 4096.0).map(|t| q.z + t);
    match (floor, ceil) {
        (Some(f), Some(c)) if c - f < EYE_HEIGHT + 24.0 => (f + c) * 0.5,
        (Some(f), Some(c)) => (f + EYE_HEIGHT).min(c - 16.0),
        (Some(f), None) => f + EYE_HEIGHT,
        (None, Some(c)) => q.z.min(c - 16.0),
        (None, None) => q.z,
    }
}

fn eye_point(idx: &SolidIndex, faces: &[&FaceUse]) -> Option<DVec3> {
    let mut w = 0.0;
    let mut q = DVec3::ZERO;
    for f in faces {
        let a = f.area.max(1.0);
        w += a;
        q += (f.center + f.normal * LIFT) * a;
    }
    if w <= 0.0 {
        return None;
    }
    let q = q / w;
    let sample: Vec<DVec3> = {
        let step = (faces.len() / SAMPLE_FACES).max(1);
        faces.iter().step_by(step).take(SAMPLE_FACES).map(|f| f.front()).collect()
    };
    let seen = |p: DVec3| sample.iter().filter(|s| idx.clear_line(p, **s)).count();
    let need = sample.len().div_ceil(2).max(1);
    let (low, high) = faces.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), f| {
        let z = f.center.z;
        (a.min(z), b.max(z))
    });
    let at = |c: DVec3| -> Option<DVec3> {
        if idx.contains(c).is_some() {
            return None;
        }
        let p = DVec3::new(c.x, c.y, eye_height(idx, faces, c)).round();
        let inside = p.z >= low - EYE_HEIGHT && p.z <= high + EYE_HEIGHT * 2.0;
        (inside && free(idx, p)).then_some(p)
    };
    let mut best: Option<(usize, f64, DVec3)> = None;
    if let Some(p) = at(q).filter(|p| seen(*p) > 0) {
        let n = seen(p);
        if n >= need {
            return Some(p);
        }
        best = Some((n, 0.0, p));
    }
    for ring in 1..=16 {
        let r = f64::from(ring) * 16.0;
        for k in 0..8 {
            let a = std::f64::consts::FRAC_PI_4 * f64::from(k);
            let Some(p) = at(q + DVec3::new(a.cos() * r, a.sin() * r, 0.0)) else { continue };
            let n = seen(p);
            if n == 0 {
                continue;
            }
            if best.is_none_or(|(bn, bd, _)| n > bn || (n == bn && r < bd)) {
                best = Some((n, r, p));
            }
        }
        if best.is_some_and(|(n, _, _)| n >= need) {
            break;
        }
    }
    best.map(|(_, _, p)| p)
}

fn split(faces: &[&FaceUse], k: usize) -> Vec<Vec<usize>> {
    if k <= 1 || faces.len() < k * 2 {
        return vec![(0..faces.len()).collect()];
    }
    let pts: Vec<DVec3> = faces.iter().map(|f| f.center).collect();
    let mut centers = vec![pts[0]];
    while centers.len() < k {
        let far = (0..pts.len())
            .max_by(|a, b| {
                let da = centers.iter().map(|c| c.distance_squared(pts[*a])).fold(f64::INFINITY, f64::min);
                let db = centers.iter().map(|c| c.distance_squared(pts[*b])).fold(f64::INFINITY, f64::min);
                da.total_cmp(&db).then(b.cmp(a))
            })
            .unwrap_or(0);
        centers.push(pts[far]);
    }
    let mut assign = vec![0usize; pts.len()];
    for _ in 0..10 {
        for (i, p) in pts.iter().enumerate() {
            assign[i] = (0..k)
                .min_by(|a, b| centers[*a].distance_squared(*p).total_cmp(&centers[*b].distance_squared(*p)))
                .unwrap_or(0);
        }
        for (c, center) in centers.iter_mut().enumerate() {
            let (mut s, mut w) = (DVec3::ZERO, 0.0);
            for (i, f) in faces.iter().enumerate() {
                if assign[i] == c {
                    let a = f.area.max(1.0);
                    s += f.center * a;
                    w += a;
                }
            }
            if w > 0.0 {
                *center = s / w;
            }
        }
    }
    (0..k).map(|c| (0..pts.len()).filter(|i| assign[*i] == c).collect::<Vec<_>>()).filter(|g| !g.is_empty()).collect()
}

pub fn usual_size(probes: &[Probe]) -> i32 {
    let mut sizes: Vec<i32> = probes.iter().map(|p| p.size.clamp(0, 12)).collect();
    sizes.sort_unstable();
    sizes.get(sizes.len() / 2).copied().unwrap_or(0)
}

fn suggest_new(report: &CubemapReport, idx: &SolidIndex) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let size = usual_size(&report.probes);
    for (ri, room) in report.rooms.iter().enumerate() {
        if !room.probes.is_empty() {
            continue;
        }
        if room.surface < MIN_SUGGEST_SURFACE {
            continue;
        }
        let faces: Vec<&FaceUse> = room.faces.iter().map(|&i| &report.faces[i]).collect();
        let span = room.bounds.size();
        let extent = span.x.max(span.y);
        let k = ((extent / LARGE_ROOM).ceil() as usize).clamp(1, MAX_SPLIT);
        let groups = split(&faces, k);
        let many = groups.len() > 1;
        for g in groups {
            let sub: Vec<&FaceUse> = g.iter().map(|&i| faces[i]).collect();
            let Some(origin) = eye_point(idx, &sub) else { continue };
            if out.iter().any(|s: &Suggestion| s.origin.distance(origin) < TOO_CLOSE) {
                continue;
            }
            out.push(Suggestion {
                room: Some(ri),
                origin,
                size,
                sides: Vec::new(),
                faces: sub.len(),
                probe: None,
                reason: if many { "large_room" } else { "room_without_cubemap" },
            });
        }
    }
    for (ri, room) in report.rooms.iter().enumerate().filter(|(_, r)| !r.probes.is_empty()) {
        let far: Vec<&FaceUse> = room
            .faces
            .iter()
            .map(|&i| &report.faces[i])
            .filter(|f| !f.manual && f.probe.is_some() && f.distance > FAR)
            .collect();
        let weight: f64 = far.iter().map(|f| f.area * f.strength.min(1.0)).sum();
        if weight < MIN_SUGGEST_SURFACE * FAR_WEIGHT {
            continue;
        }
        let bounds = far.iter().fold(Aabb::EMPTY, |mut b, f| {
            b.extend(f.center);
            b
        });
        let extent = bounds.size().x.max(bounds.size().y);
        let k = ((extent / (FAR * 2.0)).ceil() as usize).clamp(1, MAX_SPLIT);
        for g in split(&far, k) {
            let sub: Vec<&FaceUse> = g.iter().map(|&i| far[i]).collect();
            let sub_weight: f64 = sub.iter().map(|f| f.area * f.strength.min(1.0)).sum();
            if sub_weight < MIN_SUGGEST_SURFACE {
                continue;
            }
            let Some(origin) = eye_point(idx, &sub) else { continue };
            let covered = report.probes.iter().any(|p| p.origin.distance(origin) < FAR * 0.5)
                || out.iter().any(|s: &Suggestion| s.origin.distance(origin) < FAR * 0.5);
            if covered {
                continue;
            }
            out.push(Suggestion {
                room: Some(ri),
                origin,
                size,
                sides: Vec::new(),
                faces: sub.len(),
                probe: None,
                reason: "far_from_cubemap",
            });
        }
    }
    out
}

pub fn uncovered(f: &FaceUse, origins: &[DVec3], idx: &SolidIndex) -> bool {
    if !f.reflective || f.manual {
        return false;
    }
    let Some(pick) = nearest_in_front(origins, f.center, f.normal) else { return true };
    if !facing(f, origins[pick]) || f.sees(idx, origins[pick]) {
        return false;
    }
    let fp = f.front();
    !origins.iter().any(|o| o.distance(fp) <= FAR * 2.0 && facing(f, *o) && f.sees(idx, *o))
}

fn front_point(idx: &SolidIndex, faces: &[&FaceUse]) -> Option<DVec3> {
    let (mut c, mut n, mut w) = (DVec3::ZERO, DVec3::ZERO, 0.0);
    for f in faces {
        let a = (f.area * f.strength.min(1.0)).max(1.0);
        c += f.spot * a;
        n += f.normal * a;
        w += a;
    }
    if w <= 0.0 {
        return None;
    }
    let c = c / w;
    let flat = DVec3::new(n.x, n.y, 0.0);
    let dir = if flat.length() > 1e-6 { flat.normalize() } else { return eye_point(idx, faces) };
    let sample: Vec<&FaceUse> = {
        let step = (faces.len() / SAMPLE_FACES).max(1);
        faces.iter().step_by(step).take(SAMPLE_FACES).copied().collect()
    };
    let seen = |p: DVec3| sample.iter().filter(|f| facing(f, p) && f.sees(idx, p)).count();
    let need = sample.len().div_ceil(2).max(1);
    let mut best: Option<(usize, DVec3)> = None;
    for d in FACADE_DISTANCES {
        let mut p = c + dir * d;
        if let Some(t) = floor_below(idx, p) {
            p.z = p.z.max(p.z - t + EYE_HEIGHT);
        }
        let p = p.round();
        if !free(idx, p) {
            continue;
        }
        let n = seen(p);
        if n >= need {
            return Some(p);
        }
        if n > 0 && best.is_none_or(|(bn, _)| n > bn) {
            best = Some((n, p));
        }
    }
    best.map(|(_, p)| p)
}

fn suggest_facades(report: &CubemapReport, news: &[Suggestion], idx: &SolidIndex) -> Vec<Suggestion> {
    let mut origins: Vec<DVec3> = report.probes.iter().map(|p| p.origin).collect();
    origins.extend(news.iter().map(|s| s.origin));
    let size = usual_size(&report.probes);
    let mut out: Vec<Suggestion> = Vec::new();
    for _ in 0..FACADE_PASSES {
        let mut all = origins.clone();
        all.extend(out.iter().map(|s| s.origin));
        let mut left: Vec<&FaceUse> = report.faces.iter().filter(|f| uncovered(f, &all, idx)).collect();
        left.sort_by(|a, b| (b.area * b.strength).total_cmp(&(a.area * a.strength)));
        let mut used = vec![false; left.len()];
        let mut added = 0;
        for i in 0..left.len() {
            if used[i] || out.len() >= MAX_FACADES {
                continue;
            }
            let seed = left[i];
            let group: Vec<usize> = (i..left.len())
                .filter(|&j| {
                    !used[j]
                        && left[j].spot.distance(seed.spot) <= FACADE_RADIUS
                        && left[j].normal.dot(seed.normal) > 0.5
                })
                .collect();
            for &j in &group {
                used[j] = true;
            }
            let faces: Vec<&FaceUse> = group.iter().map(|&j| left[j]).collect();
            let weight: f64 = faces.iter().map(|f| f.area * f.strength.min(1.0)).sum();
            if weight < MIN_SUGGEST_SURFACE {
                continue;
            }
            let Some(origin) = front_point(idx, &faces) else { continue };
            let crowded = all.iter().chain(out.iter().map(|s| &s.origin)).any(|o| o.distance(origin) < TOO_CLOSE);
            if crowded {
                continue;
            }
            out.push(Suggestion {
                room: seed.room,
                origin,
                size,
                sides: Vec::new(),
                faces: faces.len(),
                probe: None,
                reason: "facade",
            });
            added += 1;
        }
        if added == 0 {
            break;
        }
    }
    out
}

fn suggest_sides(report: &CubemapReport, news: &mut [Suggestion], idx: &SolidIndex) -> Vec<Suggestion> {
    struct Spot {
        origin: DVec3,
        room: Option<usize>,
        probe: Option<usize>,
        new: Option<usize>,
    }
    let mut spots: Vec<Spot> = report
        .probes
        .iter()
        .enumerate()
        .map(|(i, p)| Spot { origin: p.origin, room: p.room, probe: Some(i), new: None })
        .collect();
    for (i, s) in news.iter().enumerate() {
        spots.push(Spot { origin: s.origin, room: s.room, probe: None, new: Some(i) });
    }
    if spots.is_empty() {
        return Vec::new();
    }
    let origins: Vec<DVec3> = spots.iter().map(|s| s.origin).collect();
    let mut extra: HashMap<usize, Vec<(f64, u32)>> = HashMap::new();
    let belongs =
        |s: &Spot, room: usize| s.room == Some(room) || s.probe.is_some_and(|p| report.rooms[room].probes.contains(&p));
    for f in report.faces.iter().filter(|f| f.reflective && !f.manual) {
        let Some(room) = f.room else { continue };
        let Some(pick) = nearest_in_front(&origins, f.center, f.normal) else { continue };
        let fp = f.front();
        let fine = !facing(f, origins[pick]) || f.sees(idx, origins[pick]);
        if belongs(&spots[pick], room) && fine {
            continue;
        }
        if !belongs(&spots[pick], room) && !facing(f, origins[pick]) {
            continue;
        }
        let own = spots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.origin.distance(fp) <= FAR * 2.0)
            .map(|(i, s)| {
                let visible = facing(f, s.origin) && f.sees(idx, s.origin);
                (i, !visible, !belongs(s, room), s.origin.distance(fp))
            })
            .filter(|(_, hidden, other, _)| !(*hidden && *other))
            .min_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)).then(a.3.total_cmp(&b.3)))
            .map(|(i, _, _, _)| i);
        if let Some(best) = own.filter(|b| *b != pick) {
            extra.entry(best).or_default().push((f.area, f.side));
        }
    }
    let mut amend = Vec::new();
    let mut keys: Vec<usize> = extra.keys().copied().collect();
    keys.sort_unstable();
    for key in keys {
        let mut list = extra.remove(&key).unwrap_or_default();
        list.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut ids: Vec<u32> = list.into_iter().map(|(_, s)| s).collect();
        ids.dedup();
        let spot = &spots[key];
        if let Some(n) = spot.new {
            let s = &mut news[n];
            ids.truncate(MAX_SIDES);
            s.sides = ids;
            s.sides.sort_unstable();
        } else if let Some(p) = spot.probe {
            let probe = &report.probes[p];
            let mut all = probe.sides.clone();
            for id in ids {
                if all.len() >= MAX_SIDES.max(probe.sides.len()) {
                    break;
                }
                if !all.contains(&id) {
                    all.push(id);
                }
            }
            let added = all.len() - probe.sides.len();
            if added == 0 {
                continue;
            }
            amend.push(Suggestion {
                room: probe.room,
                origin: probe.origin,
                size: probe.size,
                sides: all,
                faces: added,
                probe: Some(probe.id),
                reason: "sides",
            });
        }
    }
    amend
}

fn existing_sides(doc: &Document) -> std::collections::HashSet<u32> {
    doc.scene.solids.values().flat_map(|n| n.solid.sides.iter().map(|s| s.id)).collect()
}

pub fn analyze(doc: &Document, k: &CubemapKnowledge<'_>, vis: Option<&BspVis>) -> CubemapReport {
    let idx = SolidIndex::new(doc, false);
    let vis = vis.filter(|v| v.leaf_count() > 0);
    let mut probes = probes_of(doc);
    let mut faces = faces_of(doc, k, &idx);
    let mut manual: HashMap<u32, usize> = HashMap::new();
    for (i, p) in probes.iter().enumerate() {
        for s in &p.sides {
            manual.entry(*s).or_insert(i);
        }
    }
    for p in &mut probes {
        p.inside = idx.contains(p.origin).is_some();
        p.floor_gap = if p.inside { None } else { floor_below(&idx, p.origin) };
        p.area = vis.and_then(|v| v.area_at(p.origin));
        p.outside = !p.inside && vis.is_some_and(|v| v.cluster_at(p.origin).is_none());
    }
    let origins: Vec<DVec3> = probes.iter().map(|p| p.origin).collect();
    for f in &mut faces {
        let (pick, man) = match manual.get(&f.side) {
            Some(&p) => (Some(p), true),
            None => (nearest_in_front(&origins, f.center, f.normal), false),
        };
        f.probe = pick;
        f.manual = man;
        if let Some(p) = pick {
            f.distance = origins[p].distance(f.center);
        }
    }
    for f in faces.iter_mut().filter(|f| f.reflective) {
        if let Some(p) = f.probe {
            f.blocked = facing(f, origins[p]) && !f.sees(&idx, origins[p]);
            probes[p].faces += 1;
        }
    }
    let mut rooms = group_rooms(&mut faces, &idx, vis);
    for (pi, p) in probes.iter_mut().enumerate() {
        if p.inside {
            continue;
        }
        let mut best: Option<(f64, usize)> = None;
        for (ri, room) in rooms.iter_mut().enumerate() {
            if let Some(d) = serves(room, p, &idx) {
                room.probes.push(pi);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, ri));
                }
            }
        }
        p.room = best.map(|b| b.1);
    }
    for f in faces.iter_mut().filter(|f| f.reflective) {
        if let (Some(r), Some(p)) = (f.room, f.probe) {
            let elsewhere = probes[p].room.is_none_or(|pr| pr != r);
            let better = || {
                rooms[r].probes.iter().any(|&q| {
                    let o = origins[q];
                    o.distance(f.center) <= FAR * 2.0 && facing(f, o) && f.sees(&idx, o)
                })
            };
            f.foreign = facing(f, origins[p]) && !rooms[r].probes.contains(&p) && elsewhere && better();
        }
    }
    for room in &mut rooms {
        room.wrong = if room.probes.is_empty() { 0 } else { room.faces.iter().filter(|&&i| faces[i].wrong()).count() };
    }
    let reflective = faces.iter().filter(|f| f.reflective).count();
    let blocked = faces.iter().filter(|f| f.reflective && f.blocked).count();
    let wrong = rooms.iter().map(|r| r.wrong).sum();
    let far = faces.iter().filter(|f| f.reflective && f.probe.is_some() && f.distance > FAR).count();
    let memory_bytes = probes.iter().map(|p| size_bytes(p.size)).sum();
    let mut report = CubemapReport {
        probes,
        faces,
        rooms,
        suggestions: Vec::new(),
        findings: Vec::new(),
        memory_bytes,
        reflective,
        blocked,
        wrong,
        unfixable: 0,
        far,
        compiler_areas: vis.is_some(),
        revision: doc.scene_revision(),
    };
    let mut news = suggest_new(&report, &idx);
    let facades = suggest_facades(&report, &news, &idx);
    news.extend(facades);
    let amend = suggest_sides(&report, &mut news, &idx);
    let mut all: Vec<DVec3> = report.probes.iter().map(|p| p.origin).collect();
    all.extend(news.iter().map(|s| s.origin));
    let fixed: std::collections::HashSet<u32> =
        amend.iter().chain(news.iter()).flat_map(|s| s.sides.iter().copied()).collect();
    report.unfixable = report.faces.iter().filter(|f| !fixed.contains(&f.side) && uncovered(f, &all, &idx)).count();
    news.extend(amend);
    report.suggestions = news;
    report.findings = findings(doc, &report);
    report
}

fn mb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
}

fn findings(doc: &Document, r: &CubemapReport) -> Vec<Finding> {
    let mut out = Vec::new();
    let t = Topic::Cubemaps;
    if r.probes.is_empty() && r.reflective > 0 {
        out.push(
            Finding::new(
                t,
                "cubemap_none",
                Impact::High,
                format!(
                    "The map has no env_cubemap but {} faces use reflective env_cubemap materials in {} rooms: they all \
                     reflect the default sky cubemap and shine in the dark. Place cubemaps with the Cubemaps tool \
                     (Tools > Placement and props > Cubemaps).",
                    r.reflective,
                    r.rooms.len()
                ),
            )
            .score(5000.0)
            .points(r.rooms.iter().take(16).map(|x| x.center).collect())
            .args(vec![r.reflective.to_string(), r.rooms.len().to_string()]),
        );
    }
    if !r.probes.is_empty() {
        for room in r.rooms.iter().filter(|x| x.probes.is_empty()) {
            let impact = if room.surface > 512.0 * 512.0 {
                Impact::High
            } else if room.surface >= MIN_SUGGEST_SURFACE {
                Impact::Medium
            } else {
                Impact::Low
            };
            out.push(
                Finding::new(
                    t,
                    "cubemap_room_missing",
                    impact,
                    format!(
                        "The room around {} has {} reflective faces ({}) and no env_cubemap that can see it: they \
                         reflect a cubemap from another room. Place one at eye height in this room (Cubemaps tool).",
                        fmt_point(room.center),
                        room.faces.len(),
                        room.kind_text()
                    ),
                )
                .score(room.surface / 1000.0)
                .points(vec![room.center])
                .boxes(vec![room.bounds.expand(16.0)])
                .args(vec![fmt_point(room.center), room.faces.len().to_string(), room.kind_text()]),
            );
        }
    }
    for room in r.rooms.iter().filter(|x| !x.probes.is_empty() && x.wrong > 0) {
        let mut ids: Vec<Id> = room
            .faces
            .iter()
            .filter(|&&i| r.faces[i].wrong())
            .filter_map(|&i| r.faces[i].probe)
            .map(|p| r.probes[p].id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let list = ids.iter().map(Id::to_string).collect::<Vec<_>>().join(", ");
        out.push(
            Finding::new(
                t,
                "cubemap_through_wall",
                Impact::Medium,
                format!(
                    "{} reflective faces of the room around {} take the cubemap of another room or one behind a \
                     wall (env_cubemap {}): add those faces to the sides of a cubemap of the room (the Cubemaps \
                     tool suggests them) or move the cubemap.",
                    room.wrong,
                    fmt_point(room.center),
                    list
                ),
            )
            .score(room.wrong as f64 * 10.0)
            .objects(ids.iter().map(|i| ObjRef::Entity(*i)).collect())
            .points(vec![room.center])
            .boxes(vec![room.bounds.expand(16.0)])
            .args(vec![room.wrong.to_string(), fmt_point(room.center), list]),
        );
    }
    if r.far > 0 {
        let pts: Vec<DVec3> = r
            .faces
            .iter()
            .filter(|f| f.reflective && f.probe.is_some() && f.distance > FAR)
            .take(16)
            .map(|f| f.center)
            .collect();
        out.push(
            Finding::new(
                t,
                "cubemap_far",
                Impact::Low,
                format!(
                    "{} reflective faces are more than {FAR:.0} units from their cubemap: their reflections do not \
                     match what is around them. Add a cubemap closer to them.",
                    r.far
                ),
            )
            .score(r.far as f64)
            .points(pts)
            .args(vec![r.far.to_string(), format!("{FAR:.0}")]),
        );
    }
    for p in &r.probes {
        let obj = vec![ObjRef::Entity(p.id)];
        let at = fmt_point(p.origin);
        if p.inside {
            out.push(
                Finding::new(
                    t,
                    "cubemap_inside",
                    Impact::High,
                    format!(
                        "env_cubemap {} at {at} is inside a brush: buildcubemaps renders it black and every face \
                         using it reflects black. Move it out into the room.",
                        p.id
                    ),
                )
                .score(800.0)
                .objects(obj.clone())
                .points(vec![p.origin])
                .args(vec![p.id.to_string(), at.clone()]),
            );
            continue;
        }
        if p.outside {
            out.push(
                Finding::new(
                    t,
                    "cubemap_outside",
                    Impact::High,
                    format!(
                        "env_cubemap {} at {at} is outside the playable space (in the void): it captures nothing \
                         useful. Move it into a room.",
                        p.id
                    ),
                )
                .score(700.0)
                .objects(obj.clone())
                .points(vec![p.origin])
                .args(vec![p.id.to_string(), at.clone()]),
            );
            continue;
        }
        if let Some(gap) = p.floor_gap.filter(|g| *g < FLOOR_LOW) {
            out.push(
                Finding::new(
                    t,
                    "cubemap_floor",
                    Impact::Low,
                    format!(
                        "env_cubemap {} at {at} is only {gap:.0} units above the floor: reflections look as seen \
                         from the ground. Raise it to eye height ({EYE_HEIGHT:.0} to 72 units above the floor).",
                        p.id
                    ),
                )
                .score(20.0)
                .objects(obj.clone())
                .points(vec![p.origin])
                .args(vec![p.id.to_string(), at.clone(), format!("{gap:.0}")]),
            );
        }
        if p.size >= LARGE_SIZE {
            out.push(
                Finding::new(
                    t,
                    "cubemap_large_size",
                    Impact::Low,
                    format!(
                        "env_cubemap {} at {at} is {px}x{px} ({kb} KB of memory): keep that size for mirrors and big \
                         shiny floors, 0 (default, {DEFAULT_PIXELS}x{DEFAULT_PIXELS}) is enough elsewhere.",
                        p.id,
                        px = size_pixels(p.size),
                        kb = size_bytes(p.size) / 1024
                    ),
                )
                .score(5.0)
                .objects(obj.clone())
                .points(vec![p.origin])
                .args(vec![
                    p.id.to_string(),
                    at.clone(),
                    size_pixels(p.size).to_string(),
                    (size_bytes(p.size) / 1024).to_string(),
                ]),
            );
        }
        let room_surface = p.room.map_or(0.0, |ri| r.rooms[ri].surface);
        if p.size > 0 && p.size <= SMALL_SIZE && room_surface > 128.0 * 128.0 {
            out.push(
                Finding::new(
                    t,
                    "cubemap_small_size",
                    Impact::Low,
                    format!(
                        "env_cubemap {} at {at} is only {px}x{px} but lights large reflective surfaces: reflections \
                         look blurry. Use 0 (default, {DEFAULT_PIXELS}x{DEFAULT_PIXELS}).",
                        p.id,
                        px = size_pixels(p.size)
                    ),
                )
                .score(4.0)
                .objects(obj.clone())
                .points(vec![p.origin])
                .args(vec![p.id.to_string(), at.clone(), size_pixels(p.size).to_string()]),
            );
        }
    }
    let sides = existing_sides(doc);
    for p in &r.probes {
        let missing: Vec<String> = p.sides.iter().filter(|s| !sides.contains(s)).map(u32::to_string).collect();
        if missing.is_empty() {
            continue;
        }
        out.push(
            Finding::new(
                t,
                "cubemap_deleted_side",
                Impact::Medium,
                format!(
                    "env_cubemap {} lists faces that no longer exist in its sides ({}): VBSP warns about it. Remove \
                     them from the sides keyvalue.",
                    p.id,
                    missing.join(" ")
                ),
            )
            .score(30.0)
            .objects(vec![ObjRef::Entity(p.id)])
            .points(vec![p.origin])
            .args(vec![p.id.to_string(), missing.join(" ")]),
        );
    }
    for (i, a) in r.probes.iter().enumerate() {
        if a.inside {
            continue;
        }
        for b in r.probes.iter().skip(i + 1).filter(|b| !b.inside) {
            let d = a.origin.distance(b.origin);
            if d >= TOO_CLOSE || a.room != b.room {
                continue;
            }
            out.push(
                Finding::new(
                    t,
                    "cubemap_close",
                    Impact::Low,
                    format!(
                        "env_cubemap {} and {} are only {d:.0} units apart in the same room: they capture nearly the \
                         same picture and cost memory twice. Delete one or move it to another room.",
                        a.id, b.id
                    ),
                )
                .score(3.0)
                .objects(vec![ObjRef::Entity(a.id), ObjRef::Entity(b.id)])
                .points(vec![a.origin, b.origin])
                .args(vec![a.id.to_string(), b.id.to_string(), format!("{d:.0}")]),
            );
        }
    }
    let busy = r.probes.len() > MANY || r.memory_bytes > 64 * 1024 * 1024;
    if busy {
        let impact = if r.probes.len() > MANY * 3 { Impact::Medium } else { Impact::Low };
        out.push(
            Finding::new(
                t,
                "cubemap_many",
                impact,
                format!(
                    "The map has {} cubemaps (about {} MB of textures, {} max): more cubemaps load slower and bloat \
                     the BSP. One per room with reflective surfaces is usually enough.",
                    r.probes.len(),
                    mb(r.memory_bytes),
                    MAX_PROBES
                ),
            )
            .score(r.probes.len() as f64)
            .args(vec![r.probes.len().to_string(), mb(r.memory_bytes), MAX_PROBES.to_string()]),
        );
    }
    super::sort_findings(&mut out);
    out
}

impl Document {
    pub fn place_cubemaps(&mut self, list: &[Suggestion]) -> EditResult<Vec<Id>> {
        if list.is_empty() {
            return Err(EditError::Nothing("no cubemap suggestion to apply"));
        }
        if list.iter().any(|s| !s.origin.is_finite()) {
            return Err(EditError::Invalid("cubemap position is not finite".to_owned()));
        }
        self.tx("Place cubemaps", |doc| {
            let mut ids = Vec::new();
            for s in list {
                let sides = s.sides.iter().map(u32::to_string).collect::<Vec<_>>().join(" ");
                match s.probe {
                    Some(id) => {
                        if doc.scene.entity(id).is_none() {
                            continue;
                        }
                        if doc.update_entity(id, |e| e.set("sides", sides)) {
                            ids.push(id);
                        }
                    }
                    None => {
                        let mut kv = vec![("cubemapsize".to_owned(), s.size.clamp(0, 12).to_string())];
                        if !sides.is_empty() {
                            kv.push(("sides".to_owned(), sides));
                        }
                        ids.push(doc.create_point_entity("env_cubemap", s.origin.round(), &kv)?);
                    }
                }
            }
            Ok(ids)
        })
    }
}

#[cfg(test)]
#[path = "cubemap_tests.rs"]
mod tests;
