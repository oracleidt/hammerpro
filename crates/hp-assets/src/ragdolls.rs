use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, mpsc};

use hp_mdl::SkinMatrix;
use hp_render::{ModelId, ModelPart, Renderer};

use crate::AssetManager;
use crate::material::MatKind;
use crate::models::entity_matrix;
use crate::pick::TriBvh;

type Key = (String, u32);
type Loaded = (Key, Option<Arc<hp_ragdoll::Rig>>);

struct Posed {
    stamp: u64,
    model: ModelId,
    bounds: hp_geom::Aabb,
    pick: Option<Arc<TriBvh>>,
    pick_pending: bool,
    pick_parts: Option<Vec<ModelPart>>,
}

pub(crate) struct Ragdolls {
    generation: u64,
    profiles: HashMap<Key, Option<Arc<hp_ragdoll::Rig>>>,
    pending: HashSet<Key>,
    send: mpsc::Sender<(u64, Loaded)>,
    recv: mpsc::Receiver<(u64, Loaded)>,
    posed: HashMap<u32, Posed>,
    pick_send: mpsc::Sender<(u64, u32, u64, Arc<TriBvh>)>,
    pick_recv: mpsc::Receiver<(u64, u32, u64, Arc<TriBvh>)>,
    pub bones: HashMap<u32, Arc<Vec<SkinMatrix>>>,
}

impl Default for Ragdolls {
    fn default() -> Self {
        let (send, recv) = mpsc::channel();
        let (pick_send, pick_recv) = mpsc::channel();
        Self {
            generation: 0,
            profiles: HashMap::new(),
            pending: HashSet::new(),
            send,
            recv,
            posed: HashMap::new(),
            pick_send,
            pick_recv,
            bones: HashMap::new(),
        }
    }
}

impl Ragdolls {
    pub(crate) fn live_models(&self) -> impl Iterator<Item = ModelId> + '_ {
        self.posed.values().map(|p| p.model)
    }

    pub(crate) fn loading(&self) -> bool {
        !self.pending.is_empty() || self.posed.values().any(|pose| pose.pick_pending)
    }
}

impl AssetManager {
    pub(crate) fn set_physics_bones_impl(&mut self, values: &[(u32, Arc<Vec<SkinMatrix>>)]) {
        let next: HashMap<_, _> = values.iter().cloned().collect();
        if self.ragdolls.bones != next {
            self.ragdolls.bones = next;
            self.scene.instances_dirty = true;
        }
    }

    pub(crate) fn poll_ragdolls(&mut self, renderer: &mut Renderer) {
        if self.ragdolls.generation != self.generation {
            for posed in self.ragdolls.posed.drain().map(|(_, p)| p) {
                renderer.destroy_model(posed.model);
            }
            self.ragdolls.profiles.clear();
            self.ragdolls.pending.clear();
            self.ragdolls.generation = self.generation;
        }
        while let Ok((generation, (key, rig))) = self.ragdolls.recv.try_recv() {
            if generation == self.generation {
                self.ragdolls.pending.remove(&key);
                self.ragdolls.profiles.insert(key, rig);
                self.scene.instances_dirty = true;
            }
        }
        while let Ok((generation, id, stamp, bvh)) = self.ragdolls.pick_recv.try_recv() {
            if generation == self.generation
                && let Some(posed) = self.ragdolls.posed.get_mut(&id).filter(|pose| pose.stamp == stamp)
            {
                posed.pick = Some(bvh);
                posed.pick_pending = false;
            }
        }
    }

    pub(crate) fn ragdoll_models(
        &mut self,
        doc: &hp_editor::Document,
        renderer: &mut Renderer,
    ) -> HashMap<u32, ModelId> {
        let mut result = HashMap::new();
        let mut keep = HashSet::new();
        let requests: Vec<_> = self
            .scene
            .ents
            .iter()
            .filter_map(|(id, state)| {
                let entity = &doc.scene().entity(*id)?.entity;
                if !matches!(entity.classname.to_ascii_lowercase().as_str(), "prop_ragdoll" | "physics_prop_ragdoll") {
                    return None;
                }
                let (path, skin) = state.desc.model.clone()?;
                Some((
                    *id,
                    (path, state.desc.body),
                    skin,
                    state.desc.origin,
                    state.desc.angles,
                    state.desc.scale,
                    entity.get("angleOverride").unwrap_or("").to_owned(),
                ))
            })
            .collect();
        for (id, key, skin, origin, angles, scale, overrides) in requests {
            keep.insert(id);
            if !self.ragdolls.profiles.contains_key(&key)
                && !self.ragdolls.pending.contains(&key)
                && let Some(fs) = self.fs.clone()
            {
                self.ragdolls.pending.insert(key.clone());
                let send = self.ragdolls.send.clone();
                let key = key.clone();
                let generation = self.generation;
                rayon::spawn(move || {
                    let rig = hp_ragdoll::Rig::load(&key.0, |path| fs.read(path), key.1).ok().map(Arc::new);
                    let _ = send.send((generation, (key, rig)));
                });
            }
            let Some(Some(rig)) = self.ragdolls.profiles.get(&key).cloned() else { continue };
            let entity = entity_matrix(origin, angles, scale);
            let bones = self
                .ragdolls
                .bones
                .get(&id)
                .cloned()
                .unwrap_or_else(|| Arc::new(rig.skin(entity, &rig.body_poses(entity, &overrides))));
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            skin.hash(&mut hasher);
            for value in bones.iter().flatten().flatten() {
                value.to_bits().hash(&mut hasher);
            }
            let stamp = hasher.finish();
            if self.ragdolls.posed.get(&id).is_none_or(|posed| posed.stamp != stamp) {
                let mut parts = Vec::new();
                let mut bounds = hp_geom::Aabb::EMPTY;
                for mesh in &rig.model.meshes {
                    let candidates = rig.model.material_candidates(mesh.skin_ref, skin as usize);
                    let name = self
                        .fs
                        .as_ref()
                        .and_then(|fs| candidates.iter().find(|name| fs.exists(&format!("materials/{name}.vmt"))))
                        .or_else(|| candidates.first())
                        .cloned()
                        .unwrap_or_default();
                    let material = self.asset_material(&name, MatKind::Model);
                    let vertices = crate::preview::skin_part(&bones, &mesh.vertices, &mesh.bone_weights);
                    for vertex in &vertices {
                        bounds.extend(glam::DVec3::from(vertex.pos.map(f64::from)));
                    }
                    parts.push(ModelPart { material, vertices, indices: mesh.indices.clone(), tangents: Vec::new() });
                }
                let model = renderer.create_model(&parts);
                if let Some(meta) = crate::residency::model_meta(&parts) {
                    self.residency.models.insert(model, meta);
                }
                let posed = Posed { stamp, model, bounds, pick: None, pick_pending: false, pick_parts: Some(parts) };
                if let Some(old) = self.ragdolls.posed.insert(id, posed) {
                    renderer.destroy_model(old.model);
                }
            }
            if let Some(posed) = self.ragdolls.posed.get_mut(&id) {
                if !self.ragdolls.bones.contains_key(&id)
                    && let Some(parts) = posed.pick_parts.take()
                {
                    posed.pick_pending = true;
                    let send = self.ragdolls.pick_send.clone();
                    let generation = self.generation;
                    rayon::spawn(move || {
                        let bvh = Arc::new(TriBvh::build(crate::pick::parts_triangles(
                            parts.iter().map(|part| (part.vertices.as_slice(), part.indices.as_slice())),
                        )));
                        let _ = send.send((generation, id, stamp, bvh));
                    });
                }
                result.insert(id, posed.model);
            }
        }
        let removed: Vec<_> = self.ragdolls.posed.keys().filter(|id| !keep.contains(id)).copied().collect();
        for id in removed {
            if let Some(posed) = self.ragdolls.posed.remove(&id) {
                renderer.destroy_model(posed.model);
            }
        }
        result
    }

    pub(crate) fn ragdoll_bounds(&self, id: u32) -> Option<hp_geom::Aabb> {
        self.ragdolls.posed.get(&id).map(|pose| pose.bounds)
    }

    pub(crate) fn ragdoll_ray(&self, id: u32, ray: &hp_geom::Ray) -> Option<hp_editor::EntityRay> {
        let posed = self.ragdolls.posed.get(&id)?;
        let Some(bvh) = &posed.pick else { return Some(hp_editor::EntityRay::Bounds) };
        Some(match bvh.ray(ray.origin, ray.dir) {
            Some((t, normal)) => hp_editor::EntityRay::Hit { t, normal },
            None => hp_editor::EntityRay::Miss,
        })
    }
}
