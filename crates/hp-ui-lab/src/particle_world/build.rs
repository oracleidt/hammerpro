use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use super::*;

type Key = (u64, u64, u64, usize);

#[derive(Debug)]
struct Pending {
    key: Key,
    props: bool,
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Option<Physics>>,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug, Default)]
pub(crate) struct Preparation {
    pending: Option<Pending>,
}

fn compiled(state: &AppState) -> Option<&Arc<hp_editor::limits::world::BspWorld>> {
    (state.runtime.lab.player.active && !state.runtime.lab.player.editor_world)
        .then_some(state.runtime.health.world.as_ref())
        .flatten()
}

fn key(state: &AppState) -> Key {
    (
        state.sidecar.serial(),
        state.doc.revision(),
        state.assets.generation(),
        compiled(state).map_or(0, |world| Arc::as_ptr(world) as usize),
    )
}

impl Physics {
    pub(crate) fn ready(&self, state: &AppState, props: bool) -> bool {
        self.key == Some(key(state)) && (!props || self.includes_props)
    }
}

pub fn prepare(state: &mut AppState, handle: &ParticleHandle, ctx: &egui::Context) {
    super::models::prepare(state, handle, ctx);
    super::rays::prepare(state, handle, ctx);
    let Ok(mut shared) = handle.lock() else { return };
    let Some(props) = shared.world_requirement.filter(|_| state.settings.lab.show_particles) else {
        shared.world_preparation.pending = None;
        return;
    };
    let key = key(state);
    let finished = shared.world_preparation.pending.as_ref().and_then(|pending| match pending.result.try_recv() {
        Ok(result) => Some(result),
        Err(mpsc::TryRecvError::Disconnected) => Some(None),
        Err(mpsc::TryRecvError::Empty) => None,
    });
    if let Some(result) = finished {
        shared.world_preparation.pending = None;
        if let Some(result) = result.filter(|result| result.key == Some(key)) {
            shared.physics = Arc::new(result);
        }
    }
    if shared.physics.ready(state, props) {
        return;
    }
    if let Some(pending) = &shared.world_preparation.pending {
        if pending.key != key || (props && !pending.props) {
            pending.cancel.store(true, Ordering::Relaxed);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        return;
    }
    let mut bodies = Vec::new();
    if props {
        for node in state.doc.scene().entities().values() {
            let entity = &node.entity;
            if !entity.classname.to_ascii_lowercase().starts_with("prop_")
                || matches!(entity.classname.as_str(), "prop_detail" | "prop_ragdoll")
                || entity.get("solid").is_some_and(|value| value.trim() == "0")
            {
                continue;
            }
            let Some(path) =
                entity.get("model").map(|s| s.trim().replace('\\', "/").to_ascii_lowercase()).filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Some(origin) = entity.origin().filter(|p| p.is_finite()) else { continue };
            let angles = entity.angles().filter(|p| p.is_finite()).unwrap_or(DVec3::ZERO);
            let scale = entity
                .get("modelscale")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|s| s.is_finite() && *s > 0.0)
                .unwrap_or(1.0);
            let transform =
                DMat4::from_scale_rotation_translation(DVec3::splat(scale), hp_geom::angles_to_quat(angles), origin);
            bodies.push(Body { id: entity.id, path, transform, collision: None });
        }
    }
    let previous = Arc::clone(&shared.physics);
    let world = compiled(state).cloned();
    let fs = state.assets.fs().cloned();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let (send, result) = mpsc::sync_channel(1);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = build(key, props, bodies, world, fs, &previous, &worker_cancel);
        let _ = send.send(result);
        ctx.request_repaint();
    });
    shared.world_preparation.pending = Some(Pending { key, props, cancel, result });
}

fn model_triangles(fs: &hp_fs::GameFs, path: &str) -> Option<CollisionTriangles> {
    let phy = fs.read(&hp_mdl::phy_path(path))?;
    let model = match fs.read(path) {
        Some(mdl) => hp_mdl::parse_phy_for_model(&phy, &mdl, hp_mdl::Pose::FirstSequence),
        None => hp_mdl::parse_phy(&phy),
    }
    .ok()?;
    let triangles: Vec<_> = model
        .solids
        .iter()
        .flat_map(|solid| {
            solid.piece_triangles().filter_map(|triangle| {
                Some([
                    *solid.vertices.get(triangle[0] as usize)?,
                    *solid.vertices.get(triangle[1] as usize)?,
                    *solid.vertices.get(triangle[2] as usize)?,
                ])
            })
        })
        .collect();
    (!triangles.is_empty()).then(|| Arc::new(triangles))
}

fn build(
    key: Key,
    props: bool,
    mut bodies: Vec<Body>,
    world: Option<Arc<hp_editor::limits::world::BspWorld>>,
    fs: Option<hp_fs::GameFs>,
    previous: &Physics,
    cancel: &AtomicBool,
) -> Option<Physics> {
    let same_assets = previous.key.is_some_and(|old| old.2 == key.2);
    let mut models = if same_assets { previous.models.clone() } else { HashMap::new() };
    let compiled = if previous.key.is_some_and(|old| old.3 == key.3) {
        previous.compiled.clone()
    } else if let Some(world) = world {
        let mut surfaces = Vec::new();
        for face in world.faces.iter().chain(&world.entity_faces) {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            if world.materials.get(face.material as usize).is_some_and(|material| non_solid_material(material)) {
                continue;
            }
            for triangle in hp_editor::limits::world::BspWorld::face_triangles(face).chunks_exact(3) {
                let Some(points) = triangle
                    .iter()
                    .map(|index| world.vertices.get(face.first as usize + *index as usize).map(|v| vec(v.pos)))
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let points = [points[0], points[1], points[2]];
                surfaces.push(Surface { bounds: Aabb::from_points(points), points });
            }
        }
        Some(Arc::new(SurfaceTree::new(surfaces)))
    } else {
        None
    };
    let old_bodies: HashMap<_, _> = previous
        .bodies
        .iter()
        .filter(|_| same_assets)
        .map(|body| ((body.path.clone(), body.transform.to_cols_array().map(f64::to_bits)), body.collision.clone()))
        .collect();
    for body in &mut bodies {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let stamp = (body.path.clone(), body.transform.to_cols_array().map(f64::to_bits));
        if let Some(collision) = old_bodies.get(&stamp) {
            body.collision = collision.clone();
            continue;
        }
        let model = models
            .entry(body.path.clone())
            .or_insert_with(|| fs.as_ref().and_then(|fs| model_triangles(fs, &body.path)));
        if let Some(model) = model {
            body.collision = Some(Arc::new(SurfaceTree::new(
                model
                    .iter()
                    .map(|triangle| {
                        let points = triangle.map(|point| body.transform.transform_point3(vec(point)));
                        Surface { bounds: Aabb::from_points(points), points }
                    })
                    .collect(),
            )));
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    Some(Physics { key: Some(key), models, bodies, compiled, includes_props: props })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_worlds_stay_lazy_and_traced_worlds_publish_an_immutable_snapshot() {
        let mut state = hp_app::testing::test_state();
        state.settings.lab.show_particles = true;
        let handle = crate::particles::new_handle();
        let original = Arc::clone(&handle.lock().unwrap().physics);
        let ctx = egui::Context::default();
        prepare(&mut state, &handle, &ctx);
        assert!(handle.lock().unwrap().world_preparation.pending.is_none());
        assert!(original.key.is_none());
        handle.lock().unwrap().world_requirement = Some(false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            prepare(&mut state, &handle, &ctx);
            if handle.lock().unwrap().physics.ready(&state, false) {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let shared = handle.lock().unwrap();
        assert!(!shared.physics.ready(&state, true));
        assert!(shared.physics.bodies.is_empty());
        assert!(original.key.is_none());
    }

    #[test]
    fn cancelled_and_replaced_asset_worlds_do_not_reuse_old_model_data() {
        let mut previous = Physics { key: Some((1, 1, 1, 0)), ..Default::default() };
        previous.models.insert("models/missing.mdl".into(), None);
        let cancelled = AtomicBool::new(true);
        assert!(build((1, 2, 1, 0), true, vec![], None, None, &previous, &cancelled).is_none());
        let result = build((2, 1, 2, 0), false, vec![], None, None, &previous, &AtomicBool::new(false)).unwrap();
        assert!(result.models.is_empty());
        assert_eq!(previous.models.len(), 1);
    }
}
