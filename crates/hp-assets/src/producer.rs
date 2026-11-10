use ahash::AHashMap;
use hp_app::state::SceneOptions;
use hp_editor::{Changes, Document, Id};
use hp_render::{ModelInstance, ObjectFlags, ObjectMesh, Renderer, SpriteInstance};

use crate::lights::SceneLight;

use crate::AssetManager;

pub(crate) const DERIVED_ID_BASE: u32 = 0x7000_0000;
const DERIVED_ID_END: u32 = 0x8000_0000;

pub(crate) struct ProducerChanges<'a> {
    pub doc: &'a Changes,
    pub full: bool,
    pub entities: bool,
}

pub(crate) struct ProducerCx<'a> {
    pub doc: &'a Document,
    pub assets: &'a mut AssetManager,
    #[allow(dead_code)]
    pub renderer: &'a mut Renderer,
    #[allow(dead_code)]
    pub opts: &'a SceneOptions,
}

pub(crate) struct DerivedObject {
    pub key: u64,
    pub mesh: ObjectMesh,
    pub owner: Option<Id>,
    pub flags: ObjectFlags,
}

#[derive(Default)]
pub(crate) struct DerivedBuild {
    pub clear: bool,
    pub upsert: Vec<DerivedObject>,
    pub remove: Vec<u64>,
    pub sprites: Option<Vec<SpriteInstance>>,
    pub models: Option<Vec<ModelInstance>>,
    pub lights: Option<Vec<SceneLight>>,
}

pub(crate) trait DerivedProducer {
    fn preparing(&self) -> bool {
        false
    }
    fn name(&self) -> &'static str;

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool;

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild;

    fn object_id(&self, _key: u64) -> Option<u32> {
        None
    }
}

pub(crate) struct ObjectSink<'a> {
    pub selected: &'a mut ahash::AHashSet<u32>,
    pub placements: &'a mut crate::residency::ScenePlacements,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObjInfo {
    owner: Option<Id>,
    flags: ObjectFlags,
    allocated: bool,
}

struct Slot {
    producer: Box<dyn DerivedProducer>,
    objects: AHashMap<u64, u32>,
    sprites: Vec<SpriteInstance>,
    models: Vec<ModelInstance>,
    lights: Vec<SceneLight>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ListsDirty {
    pub sprites: bool,
    pub models: bool,
    pub lights: bool,
}

#[derive(Default)]
pub(crate) struct Derived {
    slots: Vec<Slot>,
    info: AHashMap<u32, ObjInfo>,
    by_owner: AHashMap<Id, Vec<u32>>,
    next_id: u32,
    free_ids: Vec<u32>,
    ready: bool,
}

fn object_flags(doc: &Document, info: &ObjInfo) -> Option<ObjectFlags> {
    match info.owner {
        Some(e) => doc.scene().entity(e).is_some().then(|| crate::scene::entity_flags(doc, e) | info.flags),
        None => Some(info.flags),
    }
}

impl Derived {
    pub(crate) fn preparing(&self) -> bool {
        self.slots.iter().any(|slot| slot.producer.preparing())
    }
    fn ensure_producers(&mut self) {
        if self.ready {
            return;
        }
        self.ready = true;
        self.next_id = DERIVED_ID_BASE;
        self.slots = crate::scene::producers()
            .into_iter()
            .map(|producer| Slot {
                producer,
                objects: AHashMap::new(),
                sprites: Vec::new(),
                models: Vec::new(),
                lights: Vec::new(),
            })
            .collect();
        debug_assert!(
            {
                let mut names: Vec<&str> = self.slots.iter().map(|s| s.producer.name()).collect();
                names.sort_unstable();
                names.windows(2).all(|w| w[0] != w[1])
            },
            "producer names must be unique"
        );
    }

    pub(crate) fn clear(&mut self) {
        for slot in &mut self.slots {
            slot.objects.clear();
            slot.sprites.clear();
            slot.models.clear();
            slot.lights.clear();
        }
        self.info.clear();
        self.by_owner.clear();
        self.free_ids.clear();
        self.next_id = DERIVED_ID_BASE;
    }

    pub(crate) fn flags_of(&self, doc: &Document, rid: u32) -> Option<Option<ObjectFlags>> {
        self.info.get(&rid).map(|info| object_flags(doc, info))
    }

    pub(crate) fn owner_of(&self, rid: u32) -> Option<Id> {
        self.info.get(&rid).and_then(|i| i.owner)
    }

    pub(crate) fn owned(&self, owner: Id) -> &[u32] {
        self.by_owner.get(&owner).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn refresh_all_flags(&self, doc: &Document, renderer: &mut Renderer) {
        for (rid, info) in &self.info {
            if let Some(f) = object_flags(doc, info) {
                renderer.set_object_flags(*rid, f);
            }
        }
    }

    pub(crate) fn refresh_owner_flags(&self, doc: &Document, renderer: &mut Renderer, owners: &[Id]) {
        for owner in owners {
            for rid in self.owned(*owner) {
                if let Some(f) = self.info.get(rid).and_then(|info| object_flags(doc, info)) {
                    renderer.set_object_flags(*rid, f);
                }
            }
        }
    }

    pub(crate) fn sprites(&self) -> impl Iterator<Item = &SpriteInstance> {
        self.slots.iter().flat_map(|s| &s.sprites)
    }

    pub(crate) fn models(&self) -> impl Iterator<Item = &ModelInstance> {
        self.slots.iter().flat_map(|s| &s.models)
    }

    pub(crate) fn lights(&self) -> impl Iterator<Item = &SceneLight> {
        self.slots.iter().flat_map(|s| &s.lights)
    }

    fn alloc_id(&mut self) -> Option<u32> {
        if let Some(id) = self.free_ids.pop() {
            return Some(id);
        }
        self.next_id = self.next_id.max(DERIVED_ID_BASE);
        (self.next_id < DERIVED_ID_END).then(|| {
            self.next_id += 1;
            self.next_id - 1
        })
    }

    fn forget(&mut self, rid: u32) {
        let Some(info) = self.info.remove(&rid) else { return };
        if let Some(owner) = info.owner
            && let Some(list) = self.by_owner.get_mut(&owner)
        {
            list.retain(|r| *r != rid);
            if list.is_empty() {
                self.by_owner.remove(&owner);
            }
        }
        if info.allocated {
            self.free_ids.push(rid);
        }
    }

    fn remove_object(&mut self, renderer: &mut Renderer, sink: &mut ObjectSink<'_>, rid: u32) {
        renderer.remove_object(rid);
        sink.selected.remove(&rid);
        sink.placements.forget(rid);
        self.forget(rid);
    }

    fn apply(
        &mut self,
        index: usize,
        build: DerivedBuild,
        doc: &Document,
        renderer: &mut Renderer,
        sink: &mut ObjectSink<'_>,
    ) -> ListsDirty {
        let DerivedBuild { clear, upsert, remove, sprites, models, lights } = build;
        if clear {
            let rids: Vec<u32> = self.slots[index].objects.drain().map(|(_, rid)| rid).collect();
            for rid in rids {
                self.remove_object(renderer, sink, rid);
            }
        }
        for key in remove {
            if let Some(rid) = self.slots[index].objects.remove(&key) {
                self.remove_object(renderer, sink, rid);
            }
        }
        for obj in upsert {
            let rid = match self.slots[index].objects.get(&obj.key) {
                Some(rid) => *rid,
                None => {
                    let chosen = self.slots[index].producer.object_id(obj.key);
                    let Some(rid) = chosen.or_else(|| self.alloc_id()) else {
                        log::warn!("{}: no render object id left", self.slots[index].producer.name());
                        continue;
                    };
                    self.slots[index].objects.insert(obj.key, rid);
                    self.info
                        .insert(rid, ObjInfo { owner: None, flags: ObjectFlags::empty(), allocated: chosen.is_none() });
                    rid
                }
            };
            let Some(old) = self.info.get(&rid).copied() else { continue };
            if old.owner != obj.owner {
                if let Some(prev) = old.owner
                    && let Some(list) = self.by_owner.get_mut(&prev)
                {
                    list.retain(|r| *r != rid);
                    if list.is_empty() {
                        self.by_owner.remove(&prev);
                    }
                }
                if let Some(owner) = obj.owner {
                    self.by_owner.entry(owner).or_default().push(rid);
                }
            }
            let info = ObjInfo { owner: obj.owner, flags: obj.flags, allocated: old.allocated };
            self.info.insert(rid, info);
            sink.placements.set_object(
                rid,
                crate::residency::Placement {
                    min: obj.mesh.aabb_min,
                    max: obj.mesh.aabb_max,
                    mats: obj.mesh.batches.iter().map(|b| (b.material, crate::residency::Density::Unknown)).collect(),
                },
            );
            renderer.upsert_object(rid, obj.mesh);
            let flags = object_flags(doc, &info).unwrap_or(info.flags);
            renderer.set_object_flags(rid, flags);
            if flags.contains(ObjectFlags::SELECTED) {
                sink.selected.insert(rid);
            } else {
                sink.selected.remove(&rid);
            }
        }
        let slot = &mut self.slots[index];
        let mut dirty = ListsDirty::default();
        if let Some(list) = sprites {
            dirty.sprites = !(list.is_empty() && slot.sprites.is_empty());
            slot.sprites = list;
        }
        if let Some(list) = models {
            dirty.models = !(list.is_empty() && slot.models.is_empty());
            slot.models = list;
        }
        if let Some(list) = lights {
            dirty.lights = !(list.is_empty() && slot.lights.is_empty());
            slot.lights = list;
        }
        dirty
    }
}

impl AssetManager {
    pub(crate) fn run_producers(
        &mut self,
        doc: &Document,
        renderer: &mut Renderer,
        opts: &SceneOptions,
        changes: &ProducerChanges<'_>,
    ) -> ListsDirty {
        let mut derived = std::mem::take(&mut self.scene.derived);
        derived.ensure_producers();
        let mut dirty = ListsDirty::default();
        for index in 0..derived.slots.len() {
            let build = {
                let mut cx = ProducerCx { doc, assets: &mut *self, renderer: &mut *renderer, opts };
                let producer = &mut derived.slots[index].producer;
                if !producer.changed(changes, &cx) {
                    continue;
                }
                producer.build(&mut cx)
            };
            let mut sink = ObjectSink { selected: &mut self.scene.selected, placements: &mut self.scene.placements };
            let d = derived.apply(index, build, doc, renderer, &mut sink);
            dirty.sprites |= d.sprites;
            dirty.models |= d.models;
            dirty.lights |= d.lights;
        }
        self.scene.derived = derived;
        dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_allocated_reused_and_bounded() {
        let mut d = Derived::default();
        let a = d.alloc_id();
        let b = d.alloc_id();
        assert_eq!((a, b), (Some(DERIVED_ID_BASE), Some(DERIVED_ID_BASE + 1)));
        d.info.insert(DERIVED_ID_BASE, ObjInfo { owner: Some(7), flags: ObjectFlags::empty(), allocated: true });
        d.by_owner.insert(7, vec![DERIVED_ID_BASE]);
        d.forget(DERIVED_ID_BASE);
        assert!(d.owned(7).is_empty());
        assert_eq!(d.alloc_id(), Some(DERIVED_ID_BASE), "freed ids are reused");
        d.next_id = DERIVED_ID_END;
        assert_eq!(d.alloc_id(), None);
        const { assert!(DERIVED_ID_END <= crate::entities::ENTITY_BIT) };
    }

    #[test]
    fn every_producer_has_a_unique_name() {
        let mut d = Derived::default();
        d.ensure_producers();
        assert!(d.slots.iter().any(|s| s.producer.name() == "decals"), "decals are the first producer");
    }
}
