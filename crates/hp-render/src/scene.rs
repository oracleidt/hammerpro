use ahash::{HashMap, HashMapExt};
use glam::Vec3;
use rayon::prelude::*;

use crate::arena::{Range32, RangeAlloc};
use crate::gpu::{
    CLASS_SHIFT, DETAIL_BIT, EdgeVertex, GpuVertex, ObjState, SLOT_MASK, SOURCE_BIT, pack_blend_vertex,
    pack_normal_alpha, pack_rgb8,
};
use crate::types::{BlendVertex, MaterialDesc, MaterialId, ObjectFlags, ObjectId, ObjectMesh, Vertex};

pub(crate) const NULL_SLOT: u32 = 0;
const SKY_SKIPPED: ObjectFlags =
    ObjectFlags(ObjectFlags::HIDDEN.0 | ObjectFlags::NO_3D.0 | ObjectFlags::SKY_3D.0 | ObjectFlags::BRUSH_ENTITY.0);
pub(crate) const DEAD_SLOT: u32 = 1;
const FIRST_SLOT: u32 = 2;

pub(crate) const LEVEL_CELL: [f32; 3] = [2048.0, 8192.0, 65536.0];
const COARSEST_LEVEL: u8 = 2;
const LEVEL_THRESHOLD: [u64; 2] = [131_072, 16_384];
pub(crate) const EDGE_CELL: f32 = 8192.0;
const EDGE_MATERIAL: u32 = u32::MAX;
const MARK_MATERIAL: u32 = u32::MAX - 1;

const WHITE: u32 = 0xffff_ffff;
const STATE_MERGE_GAP: u32 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArenaKind {
    MeshVerts = 0,
    Indices = 1,
    EdgeVerts = 2,
}

pub(crate) const ARENA_ELEM: [u64; 3] =
    [size_of::<GpuVertex>() as u64, size_of::<u32>() as u64, size_of::<EdgeVertex>() as u64];

#[derive(Debug)]
pub(crate) struct Write {
    pub arena: ArenaKind,
    pub offset: u64,
    pub data: Vec<u8>,
}

pub(crate) const ASSEMBLE_BATCH: usize = 32;

#[derive(Debug, Default)]
pub(crate) struct FlushOutput {
    pub writes: Vec<Write>,
    pub rebuilt: Vec<u32>,
    pub state_ranges: Vec<(u32, u32)>,
    pub states_len: u32,
    pub shadow_states: bool,
    pub casters: bool,
}

pub(crate) const NO_CASTER: u32 =
    ObjectFlags::HIDDEN.0 | ObjectFlags::NO_3D.0 | ObjectFlags::NO_SHADOW.0 | ObjectFlags::ROPE.0;

pub(crate) const SHADOW_NEUTRAL: u32 = ObjectFlags::SELECTED.0
    | ObjectFlags::HOVERED.0
    | ObjectFlags::DIMMED.0
    | ObjectFlags::INVALID.0
    | ObjectFlags::ALWAYS_EDGES.0
    | ObjectFlags::NO_2D.0
    | ObjectFlags::NO_GAME_WIRE.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ChunkKey {
    pub material: u32,
    pub level: u8,
    pub cell: [i32; 3],
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Member {
    pub v_off: u32,
    pub v_cnt: u32,
    pub i_off: u32,
    pub i_cnt: u32,
}

pub(crate) struct Chunk {
    pub key: ChunkKey,
    pub generation: u64,
    pub members: HashMap<ObjectId, Member>,
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub vtx: Range32,
    pub idx: Range32,
    pub used_v: u32,
    pub used_i: u32,
    pub dead_v: u32,
    pub dead_i: u32,
    pub dirty: bool,
    pub rebuilt_epoch: u64,
    pub list_pos: u32,
    pub all_hidden: bool,
    pub all_compiled: bool,
}

impl Chunk {
    pub fn is_edge(&self) -> bool {
        self.key.material == EDGE_MATERIAL || self.key.material == MARK_MATERIAL
    }

    pub fn is_mark(&self) -> bool {
        self.key.material == MARK_MATERIAL
    }
}

pub(crate) struct StoredBatch {
    pub material: u32,
    pub verts: Vec<GpuVertex>,
    pub indices: Vec<u32>,
    pub chunk: Option<u32>,
}

pub(crate) struct StoredMesh {
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub batches: Vec<StoredBatch>,
    pub edges: Vec<EdgeVertex>,
    pub edge_chunk: Option<u32>,
    pub marks: Vec<EdgeVertex>,
    pub mark_chunk: Option<u32>,
}

impl StoredMesh {
    fn center(&self) -> Vec3 {
        (self.aabb_min + self.aabb_max) * 0.5
    }

    fn batch(&self, material: u32) -> Option<&StoredBatch> {
        self.batches.iter().find(|b| b.material == material)
    }

    fn batch_mut(&mut self, material: u32) -> Option<&mut StoredBatch> {
        self.batches.iter_mut().find(|b| b.material == material)
    }
}

enum Pending {
    None,
    Upsert(ObjectMesh),
    Remove,
}

pub(crate) struct ObjEntry {
    pub slot: u32,
    pub flags: u32,
    pub color: u32,
    pub tint: u32,
    ext_refs: u32,
    keep: bool,
    in_pending: bool,
    pending: Pending,
    pub stored: Option<StoredMesh>,
}

pub(crate) struct MaterialInfo {
    pub desc: Option<MaterialDesc>,
    pub tris: u64,
    pub hint: u64,
    pub level: u8,
    pub chunks: Vec<u32>,
}

struct StagedWrite {
    chunk: u32,
    generation: u64,
    write: Write,
}

struct Packed {
    aabb_min: Vec3,
    aabb_max: Vec3,
    batches: Vec<(MaterialId, Vec<GpuVertex>, Vec<u32>)>,
    edges: Vec<EdgeVertex>,
    marks: Vec<EdgeVertex>,
}

#[derive(Clone, Copy)]
enum Insert {
    Mesh(u32),
    Edges,
    Marks,
}

pub(crate) struct Scene {
    pub objects: HashMap<ObjectId, ObjEntry>,
    free_slots: Vec<u32>,
    next_slot: u32,
    pub states: Vec<ObjState>,
    state_dirty: Vec<u32>,
    state_dirty_all: bool,
    pending: Vec<ObjectId>,
    pub materials: Vec<MaterialInfo>,
    material_index: HashMap<MaterialId, u32>,
    pub chunks: Vec<Option<Chunk>>,
    free_chunks: Vec<u32>,
    chunk_map: HashMap<ChunkKey, u32>,
    pub edge_chunks: Vec<u32>,
    pub mark_chunks: Vec<u32>,
    pub arenas: [RangeAlloc; 3],
    epoch: u64,
    generation: u64,
    staged: Vec<StagedWrite>,
    dirty_chunks: Vec<u32>,
    pub stored_objects: u32,
    pub live_chunks: u32,
    alloc_failed_logged: bool,
    pub translucent: ahash::HashSet<ObjectId>,
    pub translucent_gen: u64,
    hidden_dirty: Vec<u32>,
    pub always_edges: u32,
    pub invalid: u32,
    pub sky_3d: u32,
    pub brush_entities: u32,
    pub selected: ahash::HashSet<ObjectId>,
    pub selected_gen: u64,
    pub state_gen: u64,
    shadow_states: bool,
    pub membership_gen: u64,
    pub bulk: bool,
}

fn cell_of(center: Vec3, size: f32) -> [i32; 3] {
    let c = |v: f32| {
        let v = if v.is_finite() { v } else { 0.0 };
        (v / size).floor().clamp(-1.0e6, 1.0e6) as i32
    };
    [c(center.x), c(center.y), c(center.z)]
}

fn desired_level(tris: u64, current: u8) -> u8 {
    let fine = if tris > LEVEL_THRESHOLD[0] {
        0
    } else if tris > LEVEL_THRESHOLD[1] {
        1
    } else {
        2
    };
    if fine < current {
        return fine;
    }
    match current {
        0 if tris < LEVEL_THRESHOLD[0] / 4 => fine.max(1),
        1 if tris < LEVEL_THRESHOLD[1] / 4 => 2,
        _ => current,
    }
}

fn slack(n: u32) -> u32 {
    n.saturating_add(n / 4).saturating_add(48)
}

fn room(n: u32, bulk: bool) -> u32 {
    if bulk { n.saturating_mul(2).saturating_add(48) } else { slack(n) }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn pack_triangles(
    vertices: &[Vertex],
    blend: &[BlendVertex],
    cubemaps: &[u16],
    indices: &[u32],
    slot: u32,
    out_v: &mut Vec<GpuVertex>,
    out_i: &mut Vec<u32>,
    bounds: &mut [Vec3; 2],
) {
    pack_triangles_with_winding(vertices, blend, cubemaps, indices, slot, out_v, out_i, bounds, true);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn pack_triangles_with_winding(
    vertices: &[Vertex],
    blend: &[BlendVertex],
    cubemaps: &[u16],
    indices: &[u32],
    slot: u32,
    out_v: &mut Vec<GpuVertex>,
    out_i: &mut Vec<u32>,
    bounds: &mut [Vec3; 2],
    fix_winding: bool,
) {
    let [lo, hi] = bounds;
    let limit = (u32::MAX / 2) as usize;
    if vertices.len() > limit || out_v.len() + vertices.len() > limit {
        return;
    }
    let base = out_v.len() as u32;
    out_v.extend(vertices.iter().enumerate().map(|(i, v)| {
        let p = Vec3::from(v.pos);
        if p.is_finite() {
            *lo = lo.min(p);
            *hi = hi.max(p);
        }
        GpuVertex {
            pos: if p.is_finite() { v.pos } else { [0.0; 3] },
            uv: if v.uv[0].is_finite() && v.uv[1].is_finite() { v.uv } else { [0.0; 2] },
            nrm_alpha: pack_normal_alpha(v.normal, v.alpha),
            slot,
            blend: blend.get(i).map_or(0, pack_blend_vertex),
            cube: u32::from(cubemaps.get(i).copied().unwrap_or(crate::NO_CUBEMAP)),
        }
    }));
    out_i.reserve(indices.len());
    for tri in indices.chunks_exact(3) {
        let (Some(va), Some(vb), Some(vc)) =
            (vertices.get(tri[0] as usize), vertices.get(tri[1] as usize), vertices.get(tri[2] as usize))
        else {
            continue;
        };
        let (pa, pb, pc) = (Vec3::from(va.pos), Vec3::from(vb.pos), Vec3::from(vc.pos));
        if !(pa.is_finite() && pb.is_finite() && pc.is_finite()) {
            continue;
        }
        let geo = (pb - pa).cross(pc - pa);
        let avg = Vec3::from(va.normal) + Vec3::from(vb.normal) + Vec3::from(vc.normal);
        if fix_winding && geo.dot(avg) < 0.0 {
            out_i.extend_from_slice(&[base + tri[0], base + tri[2], base + tri[1]]);
        } else {
            out_i.extend_from_slice(&[base + tri[0], base + tri[1], base + tri[2]]);
        }
    }
}

fn pack_mesh(mesh: ObjectMesh, slot: u32) -> Packed {
    let ObjectMesh { mut batches, edges, detail_edges, source_edges, marks, aabb_min, aabb_max, wire_color: _ } = mesh;
    let slot = slot & SLOT_MASK;
    batches.sort_by_key(|b| b.material);
    let mut out: Vec<(MaterialId, Vec<GpuVertex>, Vec<u32>)> = Vec::with_capacity(batches.len());
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for b in &batches {
        if !matches!(out.last(), Some((m, _, _)) if *m == b.material) {
            out.push((b.material, Vec::with_capacity(b.vertices.len()), Vec::with_capacity(b.indices.len())));
        }
        if let Some((_, verts, indices)) = out.last_mut() {
            let mut bounds = [lo, hi];
            pack_triangles(&b.vertices, &b.blend, &b.cubemaps, &b.indices, slot, verts, indices, &mut bounds);
            [lo, hi] = bounds;
        }
    }
    out.retain(|(_, v, i)| !v.is_empty() && !i.is_empty());
    let mut edge_out = Vec::with_capacity(edges.len() + detail_edges.len() + source_edges.len());
    for (list, bits) in [(&edges, slot), (&detail_edges, slot | DETAIL_BIT), (&source_edges, slot | SOURCE_BIT)] {
        for pair in list.chunks_exact(2) {
            let (a, b) = (Vec3::from(pair[0]), Vec3::from(pair[1]));
            if a.is_finite() && b.is_finite() {
                lo = lo.min(a).min(b);
                hi = hi.max(a).max(b);
                edge_out.push(EdgeVertex { pos: pair[0], slot: bits });
                edge_out.push(EdgeVertex { pos: pair[1], slot: bits });
            }
        }
    }
    let mut mark_out = Vec::with_capacity(marks.len() * 3);
    for m in &marks {
        if m.p.iter().all(|p| Vec3::from(*p).is_finite()) {
            let bits = slot | ((m.class as u32) << CLASS_SHIFT);
            mark_out.extend(m.p.iter().map(|&pos| EdgeVertex { pos, slot: bits }));
        }
    }
    let (amin, amax) = (Vec3::from(aabb_min), Vec3::from(aabb_max));
    let (aabb_min, aabb_max) = if amin.is_finite() && amax.is_finite() && amin.cmple(amax).all() {
        (amin, amax)
    } else if lo.cmple(hi).all() {
        (lo, hi)
    } else {
        (Vec3::ZERO, Vec3::ZERO)
    };
    Packed { aabb_min, aabb_max, batches: out, edges: edge_out, marks: mark_out }
}

impl Scene {
    pub fn new(arena_limits: [u32; 3]) -> Self {
        let mut states = vec![ObjState::default(); FIRST_SLOT as usize];
        states[NULL_SLOT as usize] = ObjState { flags: 0, color: WHITE, tint: WHITE, classic: 0 };
        states[DEAD_SLOT as usize] = ObjState { flags: ObjectFlags::HIDDEN.0, color: WHITE, tint: WHITE, classic: 0 };
        Self {
            objects: HashMap::new(),
            free_slots: Vec::new(),
            next_slot: FIRST_SLOT,
            states,
            state_dirty: Vec::new(),
            state_dirty_all: true,
            pending: Vec::new(),
            materials: Vec::new(),
            material_index: HashMap::new(),
            chunks: Vec::new(),
            free_chunks: Vec::new(),
            chunk_map: HashMap::new(),
            edge_chunks: Vec::new(),
            mark_chunks: Vec::new(),
            arenas: arena_limits.map(RangeAlloc::new),
            epoch: 0,
            generation: 0,
            staged: Vec::new(),
            dirty_chunks: Vec::new(),
            stored_objects: 0,
            live_chunks: 0,
            alloc_failed_logged: false,
            translucent: ahash::HashSet::default(),
            translucent_gen: 1,
            hidden_dirty: Vec::new(),
            always_edges: 0,
            invalid: 0,
            sky_3d: 0,
            brush_entities: 0,
            selected: ahash::HashSet::default(),
            selected_gen: 1,
            state_gen: 1,
            shadow_states: false,
            membership_gen: 1,
            bulk: false,
        }
    }

    fn alloc_slot(&mut self) -> u32 {
        if let Some(s) = self.free_slots.pop() {
            return s;
        }
        let s = self.next_slot;
        self.next_slot += 1;
        self.states.push(ObjState::default());
        s
    }

    fn mark_state(&mut self, slot: u32) {
        if self.state_dirty.len() > 8192 {
            self.state_dirty_all = true;
            self.state_dirty.clear();
        }
        if !self.state_dirty_all {
            self.state_dirty.push(slot);
        }
    }

    fn write_state(&mut self, slot: u32, flags: u32, color: u32, tint: u32) {
        self.state_gen += 1;
        if let Some(s) = self.states.get_mut(slot as usize) {
            *s = ObjState { flags, color, tint, classic: s.classic };
        }
        self.mark_state(slot);
    }

    fn clear_state(&mut self, slot: u32) {
        self.state_gen += 1;
        if let Some(s) = self.states.get_mut(slot as usize) {
            *s = ObjState { flags: 0, color: WHITE, tint: WHITE, classic: 0 };
        }
        self.mark_state(slot);
    }

    fn entry_mut(&mut self, id: ObjectId) -> &mut ObjEntry {
        let new_slot = if self.objects.contains_key(&id) { None } else { Some(self.alloc_slot()) };
        if let Some(slot) = new_slot {
            self.clear_state(slot);
        }
        self.objects.entry(id).or_insert_with(|| ObjEntry {
            slot: new_slot.unwrap_or(NULL_SLOT),
            flags: 0,
            color: WHITE,
            tint: WHITE,
            ext_refs: 0,
            keep: false,
            in_pending: false,
            pending: Pending::None,
            stored: None,
        })
    }

    fn maybe_free(&mut self, id: ObjectId) {
        let Some(e) = self.objects.get(&id) else { return };
        if e.keep || e.ext_refs > 0 || e.stored.is_some() || !matches!(e.pending, Pending::None) {
            return;
        }
        let (slot, translucent, flags) = (e.slot, e.tint >> 24 != 0xff, e.flags);
        self.shadow_states |= flags & NO_CASTER == 0;
        self.objects.remove(&id);
        self.count_flags(id, flags, 0);
        if translucent {
            self.translucent.remove(&id);
            self.translucent_gen += 1;
        }
        self.clear_state(slot);
        self.free_slots.push(slot);
    }

    pub fn upsert(&mut self, id: ObjectId, mesh: ObjectMesh) {
        let color = pack_rgb8(mesh.wire_color);
        let e = self.entry_mut(id);
        e.keep = true;
        e.pending = Pending::Upsert(mesh);
        let push = !e.in_pending;
        e.in_pending = true;
        let (slot, flags, tint, changed) = (e.slot, e.flags, e.tint, e.color != color);
        e.color = color;
        if push {
            self.pending.push(id);
        }
        if changed {
            self.write_state(slot, flags, color, tint);
        }
    }

    pub fn remove(&mut self, id: ObjectId) {
        let Some(e) = self.objects.get_mut(&id) else { return };
        e.keep = false;
        if e.stored.is_some() {
            e.pending = Pending::Remove;
            if !e.in_pending {
                e.in_pending = true;
                self.pending.push(id);
            }
        } else {
            e.pending = Pending::None;
            self.maybe_free(id);
        }
    }

    pub fn clear(&mut self) {
        let ids: Vec<ObjectId> = self.objects.keys().copied().collect();
        for id in ids {
            if let Some(e) = self.objects.get_mut(&id) {
                e.keep = false;
                e.stored = None;
                e.pending = Pending::None;
                e.in_pending = false;
                let old = e.flags;
                e.flags = 0;
                e.color = WHITE;
                e.tint = WHITE;
                self.count_flags(id, old, 0);
                let Some(e) = self.objects.get(&id) else { continue };
                let slot = e.slot;
                self.clear_state(slot);
            }
            self.maybe_free(id);
        }
        self.pending.clear();
        self.translucent.clear();
        self.translucent_gen += 1;
        self.shadow_states = true;
        self.hidden_dirty.clear();
        self.chunks.clear();
        self.free_chunks.clear();
        self.chunk_map.clear();
        self.edge_chunks.clear();
        self.mark_chunks.clear();
        self.staged.clear();
        self.dirty_chunks.clear();
        for m in &mut self.materials {
            m.chunks.clear();
            m.tris = 0;
            m.hint = 0;
            m.level = COARSEST_LEVEL;
        }
        let limits = [0, 1, 2].map(|i| self.arenas[i].limit());
        self.arenas = limits.map(RangeAlloc::new);
        self.stored_objects = 0;
        self.live_chunks = 0;
    }

    pub fn set_flags(&mut self, id: ObjectId, flags: ObjectFlags) {
        let e = self.entry_mut(id);
        e.keep |= e.stored.is_some() || e.in_pending || e.ext_refs == 0;
        if e.flags == flags.0 {
            return;
        }
        let old = e.flags;
        let drawn = e.stored.is_some() || e.ext_refs > 0;
        let hidden_changed = (old ^ flags.0) & (ObjectFlags::HIDDEN.0 | ObjectFlags::COMPILED_LIGHT.0) != 0;
        self.shadow_states |= drawn && (old ^ flags.0) & !SHADOW_NEUTRAL != 0;
        let Some(e) = self.objects.get_mut(&id) else { return };
        e.flags = flags.0;
        self.count_flags(id, old, flags.0);
        let Some(e) = self.objects.get(&id) else { return };
        let (slot, color, tint) = (e.slot, e.color, e.tint);
        self.write_state(slot, flags.0, color, tint);
        if hidden_changed && let Some(stored) = self.objects.get(&id).and_then(|e| e.stored.as_ref()) {
            let chunks =
                stored.batches.iter().filter_map(|b| b.chunk).chain(stored.edge_chunk).chain(stored.mark_chunk);
            self.hidden_dirty.extend(chunks);
        }
    }

    pub fn set_tint(&mut self, id: ObjectId, tint: u32) {
        let e = self.entry_mut(id);
        e.keep |= e.stored.is_some() || e.in_pending || e.ext_refs == 0;
        if e.tint == tint {
            return;
        }
        let was = e.tint >> 24 != 0xff;
        let is = tint >> 24 != 0xff;
        let alpha_changed = (e.stored.is_some() || e.ext_refs > 0) && (e.tint ^ tint) >> 24 != 0;
        e.tint = tint;
        let (slot, flags, color) = (e.slot, e.flags, e.color);
        self.shadow_states |= alpha_changed;
        self.write_state(slot, flags, color, tint);
        if was != is {
            if is {
                self.translucent.insert(id);
            } else {
                self.translucent.remove(&id);
            }
            self.translucent_gen += 1;
        }
    }

    pub fn tint(&self, id: ObjectId) -> Option<u32> {
        self.objects.get(&id).map(|e| e.tint)
    }

    fn count_flags(&mut self, id: ObjectId, old: u32, new: u32) {
        let changed = old ^ new;
        if changed == 0 {
            return;
        }
        let count = |n: &mut u32, flag: ObjectFlags| {
            if changed & flag.0 != 0 {
                if new & flag.0 != 0 {
                    *n += 1;
                } else {
                    *n = n.saturating_sub(1);
                }
            }
        };
        count(&mut self.always_edges, ObjectFlags::ALWAYS_EDGES);
        count(&mut self.invalid, ObjectFlags::INVALID);
        count(&mut self.sky_3d, ObjectFlags::SKY_3D);
        count(&mut self.brush_entities, ObjectFlags::BRUSH_ENTITY);
        if changed & ObjectFlags::SELECTED.0 != 0 {
            if new & ObjectFlags::SELECTED.0 != 0 {
                self.selected.insert(id);
            } else {
                self.selected.remove(&id);
            }
            self.selected_gen += 1;
        }
    }

    pub fn set_wire_color(&mut self, id: ObjectId, rgb: [u8; 3]) {
        let color = pack_rgb8(rgb);
        let e = self.entry_mut(id);
        e.keep |= e.stored.is_some() || e.in_pending || e.ext_refs == 0;
        if e.color == color {
            return;
        }
        e.color = color;
        let (slot, flags, tint) = (e.slot, e.flags, e.tint);
        self.write_state(slot, flags, color, tint);
    }

    pub fn set_classic_color(&mut self, id: ObjectId, rgb: Option<[u8; 3]>) {
        let classic = rgb.map_or(0, pack_rgb8);
        let e = self.entry_mut(id);
        e.keep |= e.stored.is_some() || e.in_pending || e.ext_refs == 0;
        let slot = e.slot;
        match self.states.get_mut(slot as usize) {
            Some(s) if s.classic != classic => s.classic = classic,
            _ => return,
        }
        self.mark_state(slot);
    }

    pub fn wire_color(&self, id: ObjectId) -> Option<u32> {
        self.objects.get(&id).map(|e| e.color)
    }

    pub fn classic_color(&self, id: ObjectId) -> Option<u32> {
        let e = self.objects.get(&id)?;
        self.states.get(e.slot as usize).map(|s| s.classic)
    }

    pub fn flags(&self, id: ObjectId) -> Option<ObjectFlags> {
        self.objects.get(&id).map(|e| ObjectFlags(e.flags))
    }

    pub(crate) fn sky_signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = ahash::AHasher::default();
        for m in &self.materials {
            if !m.desc.as_ref().is_some_and(|d| d.flags.contains(crate::types::MaterialFlags::SKY)) {
                continue;
            }
            for &c in &m.chunks {
                let Some(Some(ch)) = self.chunks.get(c as usize) else { continue };
                (c, ch.used_i).hash(&mut h);
                for id in ch.members.keys() {
                    let Some(e) = self.objects.get(id) else { continue };
                    (id, e.flags & SKY_SKIPPED.0).hash(&mut h);
                    if let Some(s) = e.stored.as_ref() {
                        for v in s.aabb_min.to_array().iter().chain(&s.aabb_max.to_array()) {
                            v.to_bits().hash(&mut h);
                        }
                        if let Some(b) = s.batch(ch.key.material) {
                            (b.verts.len(), b.indices.len()).hash(&mut h);
                            for v in &b.verts {
                                (v.pos.map(f32::to_bits), v.nrm_alpha).hash(&mut h);
                            }
                        }
                    }
                }
            }
        }
        h.finish()
    }

    pub(crate) fn sky_triangles(&self, travel: Vec3, out: &mut Vec<[Vec3; 3]>) {
        let skipped = SKY_SKIPPED;
        for m in &self.materials {
            if !m.desc.as_ref().is_some_and(|d| d.flags.contains(crate::types::MaterialFlags::SKY)) {
                continue;
            }
            for &c in &m.chunks {
                let Some(Some(ch)) = self.chunks.get(c as usize) else { continue };
                if ch.used_i == 0 {
                    continue;
                }
                for id in ch.members.keys() {
                    let Some(e) = self.objects.get(id) else { continue };
                    if ObjectFlags(e.flags).intersects(skipped) {
                        continue;
                    }
                    let Some(b) = e.stored.as_ref().and_then(|s| s.batch(ch.key.material)) else { continue };
                    for t in b.indices.chunks_exact(3) {
                        let v = |i: u32| b.verts.get(i as usize);
                        let (Some(a), Some(bv), Some(c)) = (v(t[0]), v(t[1]), v(t[2])) else { continue };
                        let n = [a, bv, c].iter().fold(Vec3::ZERO, |s, x| {
                            let p = x.nrm_alpha;
                            let f = |shift: u32| f32::from(((p >> shift) & 255) as u8 as i8) / 127.0;
                            s + Vec3::new(f(0), f(8), f(16))
                        });
                        if crate::sky_layers::facing(n, travel) {
                            out.push([Vec3::from(a.pos), Vec3::from(bv.pos), Vec3::from(c.pos)]);
                        }
                    }
                }
            }
        }
    }

    pub fn acquire_ref(&mut self, id: ObjectId) -> u32 {
        let e = self.entry_mut(id);
        e.ext_refs += 1;
        e.slot
    }

    pub fn release_ref(&mut self, id: ObjectId) {
        if let Some(e) = self.objects.get_mut(&id) {
            e.ext_refs = e.ext_refs.saturating_sub(1);
            self.maybe_free(id);
        }
    }

    pub fn material_slot(&mut self, id: MaterialId) -> u32 {
        if let Some(&i) = self.material_index.get(&id) {
            return i;
        }
        let i = self.materials.len() as u32;
        self.materials.push(MaterialInfo { desc: None, tris: 0, hint: 0, level: COARSEST_LEVEL, chunks: Vec::new() });
        self.material_index.insert(id, i);
        i
    }

    pub fn hint_material_triangles(&mut self, id: MaterialId, tris: u64) {
        let i = self.material_slot(id) as usize;
        if let Some(m) = self.materials.get_mut(i) {
            m.hint = tris;
            if m.chunks.is_empty() {
                m.level = desired_level(tris.max(m.tris), COARSEST_LEVEL);
            }
        }
    }

    pub fn find_material(&self, id: MaterialId) -> Option<u32> {
        self.material_index.get(&id).copied()
    }

    pub fn fresh(&self) -> Scene {
        let mut s = Scene::new(self.arenas.each_ref().map(RangeAlloc::limit));
        s.materials = self
            .materials
            .iter()
            .map(|m| MaterialInfo { desc: m.desc, tris: 0, hint: 0, level: COARSEST_LEVEL, chunks: Vec::new() })
            .collect();
        s.material_index = self.material_index.clone();
        s
    }

    pub fn adopt_materials(&mut self, live: &Scene) {
        debug_assert!(self.materials.len() <= live.materials.len());
        for (i, m) in live.materials.iter().enumerate() {
            match self.materials.get_mut(i) {
                Some(own) => own.desc = m.desc,
                None => self.materials.push(MaterialInfo {
                    desc: m.desc,
                    tris: 0,
                    hint: 0,
                    level: COARSEST_LEVEL,
                    chunks: Vec::new(),
                }),
            }
        }
        for (id, &i) in &live.material_index {
            self.material_index.insert(*id, i);
        }
        self.shadow_states = true;
        self.state_dirty_all = true;
        self.state_dirty.clear();
    }

    fn chunk(&self, c: u32) -> Option<&Chunk> {
        self.chunks.get(c as usize).and_then(Option::as_ref)
    }

    fn chunk_mut(&mut self, c: u32) -> Option<&mut Chunk> {
        self.chunks.get_mut(c as usize).and_then(Option::as_mut)
    }

    fn get_or_create_chunk(&mut self, key: ChunkKey) -> u32 {
        if let Some(&c) = self.chunk_map.get(&key) {
            return c;
        }
        self.generation += 1;
        let list = match key.material {
            EDGE_MATERIAL => &mut self.edge_chunks,
            MARK_MATERIAL => &mut self.mark_chunks,
            _ => match self.materials.get_mut(key.material as usize) {
                Some(m) => &mut m.chunks,
                None => &mut self.edge_chunks,
            },
        };
        let idx = self.free_chunks.pop().unwrap_or(self.chunks.len() as u32);
        let list_pos = list.len() as u32;
        list.push(idx);
        let chunk = Chunk {
            key,
            generation: self.generation,
            members: HashMap::new(),
            aabb_min: Vec3::splat(f32::INFINITY),
            aabb_max: Vec3::splat(f32::NEG_INFINITY),
            vtx: Range32 { offset: 0, len: 0 },
            idx: Range32 { offset: 0, len: 0 },
            used_v: 0,
            used_i: 0,
            dead_v: 0,
            dead_i: 0,
            dirty: false,
            rebuilt_epoch: 0,
            list_pos,
            all_hidden: false,
            all_compiled: false,
        };
        if (idx as usize) < self.chunks.len() {
            self.chunks[idx as usize] = Some(chunk);
        } else {
            self.chunks.push(Some(chunk));
        }
        self.chunk_map.insert(key, idx);
        self.live_chunks += 1;
        idx
    }

    fn destroy_chunk(&mut self, c: u32) {
        let Some(chunk) = self.chunks.get_mut(c as usize).and_then(Option::take) else { return };
        self.membership_gen += 1;
        let vk = if chunk.is_edge() { ArenaKind::EdgeVerts } else { ArenaKind::MeshVerts };
        self.arenas[vk as usize].free(chunk.vtx);
        self.arenas[ArenaKind::Indices as usize].free(chunk.idx);
        self.chunk_map.remove(&chunk.key);
        let list = match chunk.key.material {
            EDGE_MATERIAL => &mut self.edge_chunks,
            MARK_MATERIAL => &mut self.mark_chunks,
            _ => match self.materials.get_mut(chunk.key.material as usize) {
                Some(m) => &mut m.chunks,
                None => return,
            },
        };
        let pos = chunk.list_pos as usize;
        if pos < list.len() {
            list.swap_remove(pos);
            if let Some(&moved) = list.get(pos)
                && let Some(Some(mc)) = self.chunks.get_mut(moved as usize)
            {
                mc.list_pos = pos as u32;
            }
        }
        self.free_chunks.push(c);
        self.live_chunks = self.live_chunks.saturating_sub(1);
    }

    fn mark_dirty(&mut self, c: u32) {
        if let Some(chunk) = self.chunk_mut(c)
            && !chunk.dirty
        {
            chunk.dirty = true;
            self.dirty_chunks.push(c);
        }
    }

    fn stage(&mut self, c: u32, write: Write) {
        if let Some(chunk) = self.chunk(c) {
            let generation = chunk.generation;
            self.staged.push(StagedWrite { chunk: c, generation, write });
        }
    }

    fn remove_member(&mut self, c: u32, id: ObjectId) {
        self.hidden_dirty.push(c);
        self.membership_gen += 1;
        let Some(chunk) = self.chunk_mut(c) else { return };
        let Some(m) = chunk.members.remove(&id) else { return };
        if chunk.members.is_empty() {
            self.destroy_chunk(c);
            return;
        }
        if chunk.dirty {
            return;
        }
        chunk.dead_v += m.v_cnt;
        chunk.dead_i += m.i_cnt;
        let too_dead = chunk.dead_v.saturating_mul(2) > chunk.used_v;
        let is_edge = chunk.is_edge();
        let (vtx_off, idx_off) = (chunk.vtx.offset, chunk.idx.offset);
        if too_dead {
            self.mark_dirty(c);
            return;
        }
        if is_edge {
            let dead = vec![EdgeVertex { pos: [0.0; 3], slot: DEAD_SLOT }; m.v_cnt as usize];
            let offset = u64::from(vtx_off + m.v_off) * ARENA_ELEM[ArenaKind::EdgeVerts as usize];
            self.stage(c, Write { arena: ArenaKind::EdgeVerts, offset, data: bytemuck::cast_slice(&dead).to_vec() });
        } else if m.i_cnt > 0 {
            let offset = u64::from(idx_off + m.i_off) * ARENA_ELEM[ArenaKind::Indices as usize];
            self.stage(c, Write { arena: ArenaKind::Indices, offset, data: vec![0u8; m.i_cnt as usize * 4] });
        }
    }

    fn stage_member_data(&mut self, c: u32, id: ObjectId, kind: Insert) {
        let Some(chunk) = self.chunk(c) else { return };
        let Some(m) = chunk.members.get(&id).copied() else { return };
        let (vtx_off, idx_off) = (chunk.vtx.offset, chunk.idx.offset);
        let Some(stored) = self.objects.get(&id).and_then(|e| e.stored.as_ref()) else { return };
        match kind {
            Insert::Edges | Insert::Marks => {
                let offset = u64::from(vtx_off + m.v_off) * ARENA_ELEM[ArenaKind::EdgeVerts as usize];
                let verts = if matches!(kind, Insert::Marks) { &stored.marks } else { &stored.edges };
                let data = bytemuck::cast_slice(verts).to_vec();
                self.stage(c, Write { arena: ArenaKind::EdgeVerts, offset, data });
            }
            Insert::Mesh(mat) => {
                let Some(b) = stored.batch(mat) else { return };
                let base = vtx_off + m.v_off;
                let idx: Vec<u32> = b.indices.iter().map(|i| i + base).collect();
                let v = Write {
                    arena: ArenaKind::MeshVerts,
                    offset: u64::from(base) * ARENA_ELEM[ArenaKind::MeshVerts as usize],
                    data: bytemuck::cast_slice(&b.verts).to_vec(),
                };
                let i = Write {
                    arena: ArenaKind::Indices,
                    offset: u64::from(idx_off + m.i_off) * ARENA_ELEM[ArenaKind::Indices as usize],
                    data: bytemuck::cast_slice(&idx).to_vec(),
                };
                self.stage(c, v);
                self.stage(c, i);
            }
        }
    }

    fn insert_member(&mut self, id: ObjectId, kind: Insert) {
        let Some(stored) = self.objects.get(&id).and_then(|e| e.stored.as_ref()) else { return };
        let (center, amin, amax) = (stored.center(), stored.aabb_min, stored.aabb_max);
        let (key, v_cnt, i_cnt) = match kind {
            Insert::Edges => {
                if stored.edges.is_empty() {
                    return;
                }
                let key = ChunkKey { material: EDGE_MATERIAL, level: 0, cell: cell_of(center, EDGE_CELL) };
                (key, stored.edges.len() as u32, 0)
            }
            Insert::Marks => {
                if stored.marks.is_empty() {
                    return;
                }
                let key = ChunkKey { material: MARK_MATERIAL, level: 0, cell: cell_of(center, EDGE_CELL) };
                (key, stored.marks.len() as u32, 0)
            }
            Insert::Mesh(mat) => {
                let Some(b) = stored.batch(mat) else { return };
                let level = self.materials.get(mat as usize).map_or(COARSEST_LEVEL, |m| m.level);
                let cell = cell_of(center, LEVEL_CELL[level as usize]);
                (ChunkKey { material: mat, level, cell }, b.verts.len() as u32, b.indices.len() as u32)
            }
        };
        let c = self.get_or_create_chunk(key);
        self.hidden_dirty.push(c);
        self.membership_gen += 1;
        if let Some(e) = self.objects.get_mut(&id)
            && let Some(s) = e.stored.as_mut()
        {
            match kind {
                Insert::Edges => s.edge_chunk = Some(c),
                Insert::Marks => s.mark_chunk = Some(c),
                Insert::Mesh(mat) => {
                    if let Some(b) = s.batch_mut(mat) {
                        b.chunk = Some(c);
                    }
                }
            }
        }
        let Some(chunk) = self.chunk_mut(c) else { return };
        chunk.aabb_min = chunk.aabb_min.min(amin);
        chunk.aabb_max = chunk.aabb_max.max(amax);
        let fits = !chunk.dirty
            && chunk.vtx.len > 0
            && chunk.used_v.checked_add(v_cnt).is_some_and(|n| n <= chunk.vtx.len)
            && chunk.used_i.checked_add(i_cnt).is_some_and(|n| n <= chunk.idx.len);
        if fits {
            let m = Member { v_off: chunk.used_v, v_cnt, i_off: chunk.used_i, i_cnt };
            chunk.used_v += v_cnt;
            chunk.used_i += i_cnt;
            chunk.members.insert(id, m);
            self.stage_member_data(c, id, kind);
        } else {
            chunk.members.insert(id, Member { v_off: 0, v_cnt, i_off: 0, i_cnt });
            self.mark_dirty(c);
        }
    }

    fn apply_object(&mut self, id: ObjectId, new: Option<StoredMesh>, inserts: &mut Vec<(ObjectId, Insert)>) {
        let Some(entry) = self.objects.get_mut(&id) else { return };
        let old = entry.stored.take();
        let had = old.is_some();
        let has = new.is_some();
        if let Some(o) = &old {
            for b in &o.batches {
                if let Some(m) = self.materials.get_mut(b.material as usize) {
                    m.tris = m.tris.saturating_sub((b.indices.len() / 3) as u64);
                }
            }
        }
        if let Some(n) = &new {
            for b in &n.batches {
                if let Some(m) = self.materials.get_mut(b.material as usize) {
                    m.tris += (b.indices.len() / 3) as u64;
                }
            }
        }
        let mut new = new;
        let mut in_place: Vec<(u32, Insert)> = Vec::new();
        if let Some(o) = &old {
            for ob in &o.batches {
                let Some(c) = ob.chunk else { continue };
                let reuse = new.as_mut().and_then(|n| {
                    let same_cell = self.chunk(c).is_some_and(|ch| {
                        !ch.dirty
                            && ch.key.cell == cell_of(n.center(), LEVEL_CELL[ch.key.level as usize])
                            && ch.members.get(&id).is_some_and(|m| {
                                n.batch(ob.material).is_some_and(|nb| {
                                    nb.verts.len() as u32 == m.v_cnt && nb.indices.len() as u32 == m.i_cnt
                                })
                            })
                    });
                    if same_cell { n.batch_mut(ob.material) } else { None }
                });
                match reuse {
                    Some(nb) => {
                        nb.chunk = Some(c);
                        in_place.push((c, Insert::Mesh(ob.material)));
                    }
                    None => self.remove_member(c, id),
                }
            }
            if let Some(c) = o.edge_chunk {
                let reuse = new.as_mut().is_some_and(|n| {
                    self.chunk(c).is_some_and(|ch| {
                        !ch.dirty
                            && ch.key.cell == cell_of(n.center(), EDGE_CELL)
                            && ch.members.get(&id).is_some_and(|m| m.v_cnt == n.edges.len() as u32)
                    })
                });
                if reuse {
                    if let Some(n) = new.as_mut() {
                        n.edge_chunk = Some(c);
                    }
                    in_place.push((c, Insert::Edges));
                } else {
                    self.remove_member(c, id);
                }
            }
            if let Some(c) = o.mark_chunk {
                let reuse = new.as_mut().is_some_and(|n| {
                    self.chunk(c).is_some_and(|ch| {
                        !ch.dirty
                            && ch.key.cell == cell_of(n.center(), EDGE_CELL)
                            && ch.members.get(&id).is_some_and(|m| m.v_cnt == n.marks.len() as u32)
                    })
                });
                if reuse {
                    if let Some(n) = new.as_mut() {
                        n.mark_chunk = Some(c);
                    }
                    in_place.push((c, Insert::Marks));
                } else {
                    self.remove_member(c, id);
                }
            }
        }
        if let Some(n) = &new {
            for b in &n.batches {
                if b.chunk.is_none() {
                    inserts.push((id, Insert::Mesh(b.material)));
                }
            }
            if n.edge_chunk.is_none() && !n.edges.is_empty() {
                inserts.push((id, Insert::Edges));
            }
            if n.mark_chunk.is_none() && !n.marks.is_empty() {
                inserts.push((id, Insert::Marks));
            }
        }
        if let Some(entry) = self.objects.get_mut(&id) {
            entry.stored = new;
        }
        for (c, kind) in in_place {
            if let Some((amin, amax)) =
                self.objects.get(&id).and_then(|e| e.stored.as_ref()).map(|s| (s.aabb_min, s.aabb_max))
                && let Some(ch) = self.chunk_mut(c)
            {
                ch.aabb_min = ch.aabb_min.min(amin);
                ch.aabb_max = ch.aabb_max.max(amax);
            }
            self.stage_member_data(c, id, kind);
        }
        match (had, has) {
            (false, true) => self.stored_objects += 1,
            (true, false) => self.stored_objects = self.stored_objects.saturating_sub(1),
            _ => {}
        }
    }

    fn update_levels(&mut self, touched: &[u32], inserts: &mut Vec<(ObjectId, Insert)>) {
        for &mat in touched {
            let Some(m) = self.materials.get(mat as usize) else { continue };
            let level = desired_level(m.tris.max(m.hint), m.level);
            if level == m.level {
                continue;
            }
            let chunks = m.chunks.clone();
            if let Some(m) = self.materials.get_mut(mat as usize) {
                m.level = level;
            }
            for c in chunks {
                let ids: Vec<ObjectId> =
                    self.chunk(c).map(|ch| ch.members.keys().copied().collect()).unwrap_or_default();
                for id in ids {
                    if let Some(b) =
                        self.objects.get_mut(&id).and_then(|e| e.stored.as_mut()).and_then(|s| s.batch_mut(mat))
                    {
                        b.chunk = None;
                    }
                    inserts.push((id, Insert::Mesh(mat)));
                }
                self.destroy_chunk(c);
            }
        }
    }

    #[cfg(test)]
    pub fn flush(&mut self, parallel: bool) -> FlushOutput {
        let mut out = self.flush_deferred(parallel);
        let rebuilt = std::mem::take(&mut out.rebuilt);
        let mut writes = self.assemble_chunks(&rebuilt, parallel);
        writes.append(&mut out.writes);
        out.writes = writes;
        out
    }

    pub fn assemble_chunks(&self, ids: &[u32], parallel: bool) -> Vec<Write> {
        let jobs: Vec<&Chunk> = ids.iter().filter_map(|&c| self.chunk(c)).collect();
        let objects = &self.objects;
        if parallel && jobs.len() > 4 {
            jobs.into_par_iter().flat_map_iter(|ch| assemble_chunk(ch, objects)).collect()
        } else {
            jobs.into_iter().flat_map(|ch| assemble_chunk(ch, objects)).collect()
        }
    }

    pub fn flush_deferred(&mut self, parallel: bool) -> FlushOutput {
        self.epoch += 1;
        let mut out = FlushOutput::default();

        let pending = std::mem::take(&mut self.pending);
        let mut jobs: Vec<(ObjectId, u32, Option<ObjectMesh>)> = Vec::with_capacity(pending.len());
        for id in pending {
            let Some(e) = self.objects.get_mut(&id) else { continue };
            if !e.in_pending {
                continue;
            }
            e.in_pending = false;
            let caster = e.flags & NO_CASTER == 0;
            match std::mem::replace(&mut e.pending, Pending::None) {
                Pending::Upsert(mesh) => jobs.push((id, e.slot, Some(mesh))),
                Pending::Remove => jobs.push((id, e.slot, None)),
                Pending::None => continue,
            }
            out.casters |= caster;
        }

        let packed: Vec<(ObjectId, Option<Packed>)> = if parallel && jobs.len() > 64 {
            jobs.into_par_iter().map(|(id, slot, mesh)| (id, mesh.map(|m| pack_mesh(m, slot)))).collect()
        } else {
            jobs.into_iter().map(|(id, slot, mesh)| (id, mesh.map(|m| pack_mesh(m, slot)))).collect()
        };

        let mut inserts: Vec<(ObjectId, Insert)> = Vec::new();
        let mut touched: Vec<u32> = Vec::new();
        for (id, p) in packed {
            if let Some(o) = self.objects.get(&id).and_then(|e| e.stored.as_ref()) {
                touched.extend(o.batches.iter().map(|b| b.material));
            }
            let new = p.map(|p| {
                let mut batches: Vec<StoredBatch> = p
                    .batches
                    .into_iter()
                    .map(|(mid, verts, indices)| StoredBatch {
                        material: self.material_slot(mid),
                        verts,
                        indices,
                        chunk: None,
                    })
                    .collect();
                batches.sort_by_key(|b| b.material);
                touched.extend(batches.iter().map(|b| b.material));
                StoredMesh {
                    aabb_min: p.aabb_min,
                    aabb_max: p.aabb_max,
                    batches,
                    edges: p.edges,
                    edge_chunk: None,
                    marks: p.marks,
                    mark_chunk: None,
                }
            });
            self.apply_object(id, new, &mut inserts);
            self.maybe_free(id);
        }
        touched.sort_unstable();
        touched.dedup();
        self.update_levels(&touched, &mut inserts);

        for (id, kind) in inserts {
            self.insert_member(id, kind);
        }

        let dirty = std::mem::take(&mut self.dirty_chunks);
        let mut rebuilt: Vec<u32> = Vec::with_capacity(dirty.len());
        for c in dirty {
            if self.layout_chunk(c) {
                rebuilt.push(c);
            }
        }
        out.rebuilt = rebuilt;

        let epoch = self.epoch;
        for s in std::mem::take(&mut self.staged) {
            let valid =
                self.chunk(s.chunk).is_some_and(|ch| ch.generation == s.generation && ch.rebuilt_epoch != epoch);
            if valid {
                out.writes.push(s.write);
            }
        }

        out.states_len = self.states.len() as u32;
        if self.state_dirty_all {
            out.state_ranges.push((0, self.states.len() as u32));
        } else if !self.state_dirty.is_empty() {
            let mut d = std::mem::take(&mut self.state_dirty);
            d.sort_unstable();
            d.dedup();
            let mut start = d[0];
            let mut end = start + 1;
            for &s in &d[1..] {
                if s <= end + STATE_MERGE_GAP {
                    end = s + 1;
                } else {
                    out.state_ranges.push((start, end));
                    start = s;
                    end = s + 1;
                }
            }
            out.state_ranges.push((start, end));
        }
        self.state_dirty_all = false;
        self.state_dirty.clear();
        out.shadow_states = std::mem::take(&mut self.shadow_states);

        self.refresh_hidden_chunks();
        out
    }

    fn refresh_hidden_chunks(&mut self) {
        if self.hidden_dirty.is_empty() {
            return;
        }
        let mut dirty = std::mem::take(&mut self.hidden_dirty);
        dirty.sort_unstable();
        dirty.dedup();
        let all_have = |chunk: &Chunk, flag: u32| {
            !chunk.members.is_empty()
                && chunk.members.keys().all(|id| self.objects.get(id).is_some_and(|e| e.flags & flag != 0))
        };
        for c in dirty {
            let Some(Some(chunk)) = self.chunks.get(c as usize) else { continue };
            let hidden = all_have(chunk, ObjectFlags::HIDDEN.0);
            let compiled = all_have(chunk, ObjectFlags::COMPILED_LIGHT.0);
            if let Some(Some(chunk)) = self.chunks.get_mut(c as usize) {
                chunk.all_hidden = hidden;
                chunk.all_compiled = compiled;
            }
        }
    }

    fn layout_chunk(&mut self, c: u32) -> bool {
        let epoch = self.epoch;
        let empty = match self.chunk(c) {
            Some(ch) if ch.dirty => ch.members.is_empty(),
            _ => return false,
        };
        if empty {
            self.destroy_chunk(c);
            return false;
        }
        let Some(chunk) = self.chunks.get_mut(c as usize).and_then(Option::as_mut) else { return false };
        chunk.dirty = false;
        let objects = &self.objects;
        let (mut v, mut i) = (0u32, 0u32);
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        let is_edge = chunk.is_edge();
        let is_mark = chunk.is_mark();
        let mat = chunk.key.material;
        for (id, m) in chunk.members.iter_mut() {
            let Some(s) = objects.get(id).and_then(|e| e.stored.as_ref()) else {
                m.v_cnt = 0;
                m.i_cnt = 0;
                continue;
            };
            let (vc, ic) = if is_mark {
                (s.marks.len() as u32, 0)
            } else if is_edge {
                (s.edges.len() as u32, 0)
            } else {
                s.batch(mat).map_or((0, 0), |b| (b.verts.len() as u32, b.indices.len() as u32))
            };
            *m = Member { v_off: v, v_cnt: vc, i_off: i, i_cnt: ic };
            v = v.saturating_add(vc);
            i = i.saturating_add(ic);
            lo = lo.min(s.aabb_min);
            hi = hi.max(s.aabb_max);
        }
        chunk.aabb_min = lo;
        chunk.aabb_max = hi;
        chunk.used_v = v;
        chunk.used_i = i;
        chunk.dead_v = 0;
        chunk.dead_i = 0;
        chunk.rebuilt_epoch = epoch;
        let vk = if is_edge { ArenaKind::EdgeVerts } else { ArenaKind::MeshVerts } as usize;
        let ik = ArenaKind::Indices as usize;
        let mut ok = true;
        let bulk = self.bulk;
        if chunk.vtx.len < v || chunk.vtx.len > slack(v).saturating_mul(3) {
            self.arenas[vk].free(chunk.vtx);
            chunk.vtx = Range32 { offset: 0, len: 0 };
            match self.arenas[vk].alloc(room(v, bulk)) {
                Some(r) => chunk.vtx = r,
                None => ok = false,
            }
        }
        if !is_edge && (chunk.idx.len < i || chunk.idx.len > slack(i).saturating_mul(3)) {
            self.arenas[ik].free(chunk.idx);
            chunk.idx = Range32 { offset: 0, len: 0 };
            match self.arenas[ik].alloc(room(i, bulk)) {
                Some(r) => chunk.idx = r,
                None => ok = false,
            }
        }
        if !ok {
            chunk.used_v = 0;
            chunk.used_i = 0;
            if !self.alloc_failed_logged {
                log::error!("hp-render: GPU geometry arena is full, some geometry will not be drawn");
                self.alloc_failed_logged = true;
            }
            return false;
        }
        true
    }

    pub fn chunks_of<'a>(&self, ids: impl IntoIterator<Item = &'a ObjectId>) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for id in ids {
            let Some(s) = self.objects.get(id).and_then(|e| e.stored.as_ref()) else { continue };
            out.extend(s.batches.iter().filter_map(|b| b.chunk).chain(s.edge_chunk).chain(s.mark_chunk));
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn state_range(&self, start: u32, end: u32) -> &[ObjState] {
        let end = (end as usize).min(self.states.len());
        let start = (start as usize).min(end);
        &self.states[start..end]
    }
}

fn assemble_chunk(ch: &Chunk, objects: &HashMap<ObjectId, ObjEntry>) -> Vec<Write> {
    if ch.is_edge() {
        let mut verts: Vec<EdgeVertex> = vec![EdgeVertex::default(); ch.used_v as usize];
        for (id, m) in &ch.members {
            let Some(s) = objects.get(id).and_then(|e| e.stored.as_ref()) else { continue };
            let (o, n) = (m.v_off as usize, m.v_cnt as usize);
            let list = if ch.is_mark() { &s.marks } else { &s.edges };
            if let (Some(dst), Some(src)) = (verts.get_mut(o..o + n), list.get(..n)) {
                dst.copy_from_slice(src);
            }
        }
        return vec![Write {
            arena: ArenaKind::EdgeVerts,
            offset: u64::from(ch.vtx.offset) * ARENA_ELEM[ArenaKind::EdgeVerts as usize],
            data: bytemuck::cast_slice(&verts).to_vec(),
        }];
    }
    let mut verts: Vec<GpuVertex> = vec![GpuVertex::default(); ch.used_v as usize];
    let mut indices: Vec<u32> = vec![0; ch.used_i as usize];
    for (id, m) in &ch.members {
        let Some(b) = objects.get(id).and_then(|e| e.stored.as_ref()).and_then(|s| s.batch(ch.key.material)) else {
            continue;
        };
        let (vo, vn) = (m.v_off as usize, m.v_cnt as usize);
        if let (Some(dst), Some(src)) = (verts.get_mut(vo..vo + vn), b.verts.get(..vn)) {
            dst.copy_from_slice(src);
        }
        let base = ch.vtx.offset + m.v_off;
        let (io, ino) = (m.i_off as usize, m.i_cnt as usize);
        if let (Some(dst), Some(src)) = (indices.get_mut(io..io + ino), b.indices.get(..ino)) {
            for (d, s) in dst.iter_mut().zip(src) {
                *d = s + base;
            }
        }
    }
    vec![
        Write {
            arena: ArenaKind::MeshVerts,
            offset: u64::from(ch.vtx.offset) * ARENA_ELEM[ArenaKind::MeshVerts as usize],
            data: bytemuck::cast_slice(&verts).to_vec(),
        },
        Write {
            arena: ArenaKind::Indices,
            offset: u64::from(ch.idx.offset) * ARENA_ELEM[ArenaKind::Indices as usize],
            data: bytemuck::cast_slice(&indices).to_vec(),
        },
    ]
}

#[cfg(test)]
#[path = "scene_tests.rs"]
mod tests;
