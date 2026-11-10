use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hp_geom::{DispMesh, SolidGeom};
use hp_kv::Block;
use hp_vmf::{Cameras, Cordons, DispInfo, EditorInfo, Entity, Side, Solid, VisGroup};

use crate::Id;
use crate::ids::{IdMap, IdSet, id_map};
use crate::scene::{EntityNode, GroupNode, SolidNode};
use crate::selection::{FaceRef, ObjRef, Selection};

mod patch;

pub(crate) use patch::Bases;

pub const SELECTION_STEP: &str = "Selection";
pub const FACE_SELECTION_STEP: &str = "Face selection";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UndoLimits {
    pub max_steps: usize,
    pub max_bytes: usize,
    pub merge_window: Option<Duration>,
}

impl Default for UndoLimits {
    fn default() -> Self {
        Self { max_steps: 256, max_bytes: 512 * 1024 * 1024, merge_window: Some(Duration::from_millis(1500)) }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Misc {
    pub cameras: Cameras,
    pub cordons: Cordons,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SelSnap {
    pub objects: Box<[ObjRef]>,
    pub faces: Box<[FaceRef]>,
}

impl SelSnap {
    pub fn of(sel: &Selection) -> Self {
        Self { objects: sel.objects.iter().copied().collect(), faces: sel.faces.iter().copied().collect() }
    }

    pub fn into_selection(self) -> Selection {
        Selection {
            objects: self.objects.into_vec().into_iter().collect(),
            faces: self.faces.into_vec().into_iter().collect(),
        }
    }

    pub fn same_objects(&self, sel: &Selection) -> bool {
        self.objects.len() == sel.objects.len() && self.objects.iter().all(|o| sel.objects.contains(o))
    }

    pub fn same_faces(&self, sel: &Selection) -> bool {
        self.faces.len() == sel.faces.len() && self.faces.iter().all(|f| sel.faces.contains(f))
    }

    pub fn same_as(&self, sel: &Selection) -> bool {
        self.same_objects(sel) && self.same_faces(sel)
    }

    pub fn bytes(&self) -> usize {
        alloc_bytes::<ObjRef>(self.objects.len()) + alloc_bytes::<FaceRef>(self.faces.len())
    }
}

#[derive(Debug, Default)]
pub(crate) struct Snapshot {
    pub solids: IdMap<Option<SolidNode>>,
    pub entities: IdMap<Option<EntityNode>>,
    pub groups: IdMap<Option<GroupNode>>,
    pub world: Option<Entity>,
    pub visgroups: Option<(Vec<VisGroup>, IdSet)>,
    pub selection: Option<SelSnap>,
    pub misc: Option<Misc>,
    pub extra_blocks: Option<Vec<Block>>,
    pub solid_patches: patch::SolidPatches,
    pub entity_patches: patch::EntityPatches,
}

impl Snapshot {
    pub fn new() -> Self {
        Self {
            solids: id_map(0),
            entities: id_map(0),
            groups: id_map(0),
            solid_patches: id_map(0),
            entity_patches: id_map(0),
            ..Self::default()
        }
    }

    pub fn has_no_document_change(&self) -> bool {
        self.solids.is_empty()
            && !self.has_patches()
            && self.entities.is_empty()
            && self.groups.is_empty()
            && self.world.is_none()
            && self.visgroups.is_none()
            && self.misc.is_none()
            && self.extra_blocks.is_none()
    }

    pub fn bytes(&self) -> usize {
        let mut b = std::mem::size_of::<Self>();
        b += map_bytes(&self.solids) + self.solids.values().flatten().map(solid_node_bytes).sum::<usize>();
        b += map_bytes(&self.entities)
            + self.entities.values().flatten().map(|n| entity_bytes(&n.entity)).sum::<usize>();
        b += map_bytes(&self.groups)
            + self.groups.values().flatten().map(|n| editor_bytes(&n.group.editor)).sum::<usize>();
        if let Some(w) = &self.world {
            b += std::mem::size_of::<Entity>() + entity_bytes(w);
        }
        if let Some((v, h)) = &self.visgroups {
            b += vec_bytes(v) + v.iter().map(visgroup_bytes).sum::<usize>() + set_bytes(h);
        }
        if let Some(s) = &self.selection {
            b += s.bytes();
        }
        if let Some(m) = &self.misc {
            b += vec_bytes(&m.cameras.cameras) + vec_bytes(&m.cordons.extra_boxes) + vec_bytes(&m.cordons.cordons);
            b += m.cordons.cordons.iter().map(|c| string_bytes(&c.name)).sum::<usize>();
        }
        if let Some(x) = &self.extra_blocks {
            b += blocks_bytes(x);
        }
        b + self.patch_bytes()
    }

    pub fn absorb(&mut self, newer: Snapshot) {
        for (id, st) in newer.solids {
            self.solids.entry(id).or_insert(st);
        }
        for (id, st) in newer.entities {
            self.entities.entry(id).or_insert(st);
        }
        for (id, st) in newer.groups {
            self.groups.entry(id).or_insert(st);
        }
        if self.world.is_none() {
            self.world = newer.world;
        }
        if self.visgroups.is_none() {
            self.visgroups = newer.visgroups;
        }
        if self.selection.is_none() {
            self.selection = newer.selection;
        }
        if self.misc.is_none() {
            self.misc = newer.misc;
        }
        if self.extra_blocks.is_none() {
            self.extra_blocks = newer.extra_blocks;
        }
    }
}

const BLOCK_OVERHEAD: usize = 16;

fn alloc_bytes<T>(cap: usize) -> usize {
    let size = cap.saturating_mul(std::mem::size_of::<T>());
    if size == 0 { 0 } else { size + BLOCK_OVERHEAD }
}

fn vec_bytes<T>(v: &Vec<T>) -> usize {
    alloc_bytes::<T>(v.capacity())
}

fn string_bytes(s: &String) -> usize {
    alloc_bytes::<u8>(s.capacity())
}

fn opt_string_bytes(s: &Option<String>) -> usize {
    s.as_ref().map_or(0, string_bytes)
}

fn map_bytes<V>(m: &IdMap<V>) -> usize {
    index_bytes::<(usize, Id, V)>(m.capacity())
}

fn set_bytes<T, S>(s: &indexmap::IndexSet<T, S>) -> usize {
    index_bytes::<(usize, T)>(s.capacity())
}

fn index_bytes<E>(cap: usize) -> usize {
    if cap == 0 {
        return 0;
    }
    let buckets = (cap.saturating_mul(8) / 7).max(4).next_power_of_two();
    alloc_bytes::<E>(cap) + buckets * (std::mem::size_of::<usize>() + 1) + 16 + BLOCK_OVERHEAD
}

fn kv_bytes(props: &Vec<(String, String)>) -> usize {
    vec_bytes(props) + props.iter().map(|(k, v)| string_bytes(k) + string_bytes(v)).sum::<usize>()
}

fn blocks_bytes(blocks: &Vec<Block>) -> usize {
    vec_bytes(blocks)
        + blocks.iter().map(|b| string_bytes(&b.name) + kv_bytes(&b.props) + blocks_bytes(&b.children)).sum::<usize>()
}

fn editor_bytes(e: &EditorInfo) -> usize {
    vec_bytes(&e.visgroup_ids)
        + opt_string_bytes(&e.logical_pos)
        + opt_string_bytes(&e.comments)
        + kv_bytes(&e.extra_props)
}

fn visgroup_bytes(v: &VisGroup) -> usize {
    string_bytes(&v.name)
        + kv_bytes(&v.extra_props)
        + vec_bytes(&v.children)
        + v.children.iter().map(visgroup_bytes).sum::<usize>()
}

fn disp_info_bytes(d: &DispInfo) -> usize {
    alloc_bytes::<DispInfo>(1)
        + vec_bytes(&d.normals)
        + vec_bytes(&d.distances)
        + vec_bytes(&d.offsets)
        + vec_bytes(&d.offset_normals)
        + vec_bytes(&d.alphas)
        + vec_bytes(&d.triangle_tags)
        + vec_bytes(&d.allowed_verts)
        + kv_bytes(&d.extra_props)
        + blocks_bytes(&d.extra_blocks)
}

fn side_bytes(s: &Side) -> usize {
    string_bytes(&s.material)
        + s.disp.as_deref().map_or(0, disp_info_bytes)
        + s.vertices_plus.as_ref().map_or(0, vec_bytes)
        + kv_bytes(&s.extra_props)
        + blocks_bytes(&s.extra_blocks)
}

fn solid_bytes(s: &Solid) -> usize {
    vec_bytes(&s.sides)
        + s.sides.iter().map(side_bytes).sum::<usize>()
        + editor_bytes(&s.editor)
        + kv_bytes(&s.extra_props)
        + blocks_bytes(&s.extra_blocks)
}

fn shared_bytes<T>(a: &Arc<T>, bytes: impl FnOnce(&T) -> usize) -> usize {
    (alloc_bytes::<T>(1) + 2 * std::mem::size_of::<usize>() + bytes(a)) / Arc::strong_count(a).max(1)
}

fn geom_bytes(g: &SolidGeom) -> usize {
    vec_bytes(&g.faces) + g.faces.iter().map(|f| vec_bytes(&f.vertices)).sum::<usize>()
}

fn disp_mesh_bytes(m: &DispMesh) -> usize {
    vec_bytes(&m.positions)
        + vec_bytes(&m.normals)
        + vec_bytes(&m.alphas)
        + vec_bytes(&m.base_positions)
        + vec_bytes(&m.indices)
}

fn solid_node_bytes(n: &SolidNode) -> usize {
    solid_bytes(&n.solid)
        + shared_bytes(&n.geom, geom_bytes)
        + vec_bytes(&n.disp)
        + n.disp.iter().flatten().map(|m| shared_bytes(m, disp_mesh_bytes)).sum::<usize>()
}

fn entity_bytes(e: &Entity) -> usize {
    string_bytes(&e.classname)
        + kv_bytes(&e.props)
        + vec_bytes(&e.connections)
        + e.connections
            .iter()
            .map(|c| {
                string_bytes(&c.output) + string_bytes(&c.target) + string_bytes(&c.input) + string_bytes(&c.param)
            })
            .sum::<usize>()
        + vec_bytes(&e.solids)
        + e.solids.iter().map(solid_bytes).sum::<usize>()
        + vec_bytes(&e.groups)
        + e.groups.iter().map(|g| editor_bytes(&g.editor)).sum::<usize>()
        + editor_bytes(&e.editor)
        + blocks_bytes(&e.extra_blocks)
}

#[derive(Debug)]
pub(crate) struct Tx {
    pub label: String,
    pub merge_tag: Option<String>,
    pub depth: u32,
    pub snap: Snapshot,
}

#[derive(Debug)]
pub(crate) struct UndoEntry {
    pub label: String,
    pub merge_tag: Option<String>,
    pub snap: Snapshot,
    pub bytes: usize,
    pub serial: u64,
    pub time: Instant,
    pub selection_only: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Burst {
    #[default]
    None,
    Selection,
    Edit,
}

#[derive(Debug)]
pub(crate) struct History {
    pub undo: VecDeque<UndoEntry>,
    pub redo: Vec<UndoEntry>,
    pub tx: Option<Tx>,
    pub limits: UndoLimits,
    pub bytes: usize,
    pub next_serial: u64,
    pub base_serial: u64,
    pub record_selection: bool,
    pub burst: Burst,
    pub selection_restored: bool,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            tx: None,
            limits: UndoLimits::default(),
            bytes: 0,
            next_serial: 1,
            base_serial: 0,
            record_selection: false,
            burst: Burst::None,
            selection_restored: false,
        }
    }
}

impl History {
    pub fn state_serial(&self) -> u64 {
        self.undo.back().map_or(self.base_serial, |e| e.serial)
    }

    pub fn alloc_serial(&mut self) -> u64 {
        let s = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1);
        s
    }

    pub fn enforce_limits(&mut self) {
        let max_steps = self.limits.max_steps.max(1);
        while self.undo.len() > max_steps {
            let older = self.undo.len() - 1;
            match self.undo.iter().take(older).position(|e| e.selection_only) {
                Some(i) if i > 0 => {
                    if let Some(e) = self.undo.remove(i) {
                        self.bytes = self.bytes.saturating_sub(e.bytes);
                        dispose(e);
                    }
                }
                _ => self.drop_oldest_undo(),
            }
        }
        while self.bytes > self.limits.max_bytes {
            if !self.redo.is_empty() {
                let e = self.redo.remove(0);
                self.bytes = self.bytes.saturating_sub(e.bytes);
                dispose(e);
            } else if self.undo.len() > 1 {
                self.drop_oldest_undo();
            } else {
                break;
            }
        }
    }

    fn drop_oldest_undo(&mut self) {
        if let Some(e) = self.undo.pop_front() {
            self.base_serial = e.serial;
            self.bytes = self.bytes.saturating_sub(e.bytes);
            dispose(e);
        }
    }

    pub fn clear_redo(&mut self) {
        for e in self.redo.drain(..) {
            self.bytes = self.bytes.saturating_sub(e.bytes);
            dispose(e);
        }
    }
}

fn dispose(e: UndoEntry) {
    if e.bytes > 256 * 1024 {
        rayon::spawn(move || drop(e));
    } else {
        drop(e);
    }
}

pub(crate) trait Ordered {
    fn order_key(&self) -> u64;
}

impl Ordered for SolidNode {
    fn order_key(&self) -> u64 {
        self.order
    }
}

impl Ordered for EntityNode {
    fn order_key(&self) -> u64 {
        self.order
    }
}

impl Ordered for GroupNode {
    fn order_key(&self) -> u64 {
        self.order
    }
}

const BULK_THRESHOLD: usize = 32;

pub(crate) fn apply_states<T: Ordered>(map: &mut IdMap<T>, states: Vec<(Id, Option<T>)>) -> Vec<(Id, Option<T>)> {
    let structural = states
        .iter()
        .filter(|(id, st)| match (map.get(id), st) {
            (Some(cur), Some(new)) => cur.order_key() != new.order_key(),
            (None, None) => false,
            _ => true,
        })
        .count();
    let mut out = Vec::with_capacity(states.len());
    if structural <= BULK_THRESHOLD {
        for (id, st) in states {
            let old = match st {
                Some(new) => match map.get_mut(&id) {
                    Some(cur) if cur.order_key() == new.order_key() => Some(std::mem::replace(cur, new)),
                    Some(_) => {
                        let old = map.shift_remove(&id);
                        insert_ordered(map, id, new);
                        old
                    }
                    None => {
                        insert_ordered(map, id, new);
                        None
                    }
                },
                None => map.shift_remove(&id),
            };
            out.push((id, old));
        }
        return out;
    }
    let mut updates: IdMap<Option<T>> = id_map(states.len());
    for (id, st) in states {
        updates.insert(id, st);
    }
    let mut entries: Vec<(Id, T)> = Vec::with_capacity(map.len() + updates.len());
    for (id, node) in map.drain(..) {
        match updates.swap_remove(&id) {
            Some(st) => {
                out.push((id, Some(node)));
                if let Some(new) = st {
                    entries.push((id, new));
                }
            }
            None => entries.push((id, node)),
        }
    }
    for (id, st) in updates {
        out.push((id, None));
        if let Some(new) = st {
            entries.push((id, new));
        }
    }
    entries.sort_by_key(|(_, n)| n.order_key());
    map.reserve(entries.len());
    map.extend(entries);
    out
}

fn insert_ordered<T: Ordered>(map: &mut IdMap<T>, id: Id, node: T) {
    let key = node.order_key();
    let at_end = map.last().is_none_or(|(_, last)| last.order_key() <= key);
    if at_end {
        map.insert(id, node);
    } else {
        let pos = map.partition_point(|_, v| v.order_key() < key);
        map.shift_insert(pos, id, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct N(u64);
    impl Ordered for N {
        fn order_key(&self) -> u64 {
            self.0
        }
    }

    fn keys(m: &IdMap<N>) -> Vec<Id> {
        m.keys().copied().collect()
    }

    #[test]
    fn small_and_bulk_paths_keep_order() {
        let mut m: IdMap<N> = id_map(0);
        for i in 0..100u32 {
            m.insert(i, N(u64::from(i)));
        }
        let old = apply_states(&mut m, vec![(3, None), (50, None)]);
        assert_eq!(m.len(), 98);
        let back: Vec<(Id, Option<N>)> = old;
        let again = apply_states(&mut m, back);
        assert!(again.iter().all(|(_, o)| o.is_none()));
        assert_eq!(keys(&m), (0..100).collect::<Vec<_>>());
        let states: Vec<(Id, Option<N>)> = (0..100).filter(|i| i % 2 == 0).map(|i| (i, None)).collect();
        let old = apply_states(&mut m, states);
        assert_eq!(m.len(), 50);
        apply_states(&mut m, old);
        assert_eq!(keys(&m), (0..100).collect::<Vec<_>>());
        apply_states(&mut m, vec![(10, Some(N(10)))]);
        assert_eq!(m.get_index_of(&10), Some(10));
        apply_states(&mut m, vec![(1000, Some(N(1000)))]);
        assert_eq!(m.last().map(|(k, _)| *k), Some(1000));
    }
}
