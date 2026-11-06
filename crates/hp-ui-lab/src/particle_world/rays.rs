use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use hp_editor::limits::world::{BspWorld, ParticleVolumes, particle_precipitation};
use hp_pcf::environment::{RayEnvironment, RayHit};

use super::*;

type Key = (u64, u64, usize);

#[derive(Clone, Debug, Default)]
pub(crate) struct RayWorld {
    key: Option<Key>,
    precipitation: SurfaceTree,
    blockers: SurfaceTree,
}

impl RayWorld {
    pub(crate) fn ready(&self, state: &AppState) -> bool {
        self.key == Some(key(state))
    }

    pub(crate) fn trace(
        &self,
        environment: RayEnvironment,
        start: Vector,
        direction: Vector,
        maximum: f32,
    ) -> Option<RayHit> {
        match environment {
            RayEnvironment::Precipitation => &self.precipitation,
            RayEnvironment::PrecipitationBlocker => &self.blockers,
        }
        .ray(start, direction, maximum)
    }

    fn new(key: Key, volumes: ParticleVolumes) -> Self {
        let tree = |triangles: Vec<[[f32; 3]; 3]>| {
            SurfaceTree::new(
                triangles
                    .into_iter()
                    .filter_map(|triangle| {
                        let points = triangle.map(vec);
                        (points.iter().all(|point| point.is_finite())
                            && (points[1] - points[0]).cross(points[2] - points[0]).length_squared() > 1e-18)
                            .then(|| Surface { bounds: Aabb::from_points(points), points })
                    })
                    .collect(),
            )
        };
        Self { key: Some(key), precipitation: tree(volumes.precipitation), blockers: tree(volumes.blockers) }
    }
}

#[derive(Debug)]
struct Pending {
    key: Key,
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Option<RayWorld>>,
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

fn compiled(state: &AppState) -> Option<&Arc<BspWorld>> {
    (state.runtime.lab.player.active && !state.runtime.lab.player.editor_world)
        .then_some(state.runtime.health.world.as_ref())
        .flatten()
}

fn key(state: &AppState) -> Key {
    (
        state.sidecar.serial(),
        if compiled(state).is_some() { 0 } else { state.doc.revision() },
        compiled(state).map_or(0, |world| Arc::as_ptr(world) as usize),
    )
}

pub(super) fn prepare(state: &AppState, handle: &ParticleHandle, ctx: &egui::Context) {
    let Ok(mut shared) = handle.lock() else { return };
    if !shared.ray_requirement || !state.settings.lab.show_particles {
        shared.ray_preparation.pending = None;
        return;
    }
    let key = key(state);
    let finished = shared.ray_preparation.pending.as_ref().and_then(|pending| match pending.result.try_recv() {
        Ok(result) => Some(result),
        Err(mpsc::TryRecvError::Disconnected) => Some(None),
        Err(mpsc::TryRecvError::Empty) => None,
    });
    if let Some(result) = finished {
        shared.ray_preparation.pending = None;
        if let Some(result) = result.filter(|result| result.key == Some(key)) {
            shared.rays = Arc::new(result);
        }
    }
    if shared.rays.ready(state) {
        return;
    }
    if let Some(pending) = &shared.ray_preparation.pending {
        if pending.key != key {
            pending.cancel.store(true, Ordering::Relaxed);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        return;
    }
    let world = compiled(state).cloned();
    let mut entities = Vec::new();
    if world.is_none() && state.doc.scene().entities().values().any(|node| particle_precipitation(&node.entity)) {
        for (id, node) in state.doc.scene().entities() {
            let blocker = node.entity.classname.eq_ignore_ascii_case("func_precipitation_blocker");
            if !blocker && !particle_precipitation(&node.entity) {
                continue;
            }
            let solids = state
                .doc
                .scene()
                .solids()
                .values()
                .filter(|solid| solid.owner == Owner::Entity(*id) && solid.geom.valid)
                .map(|solid| Arc::clone(&solid.geom))
                .collect();
            entities.push((blocker, solids));
        }
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let (send, result) = mpsc::sync_channel(1);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let volumes = match world {
            Some(world) => Some(world.particle_volumes.clone()),
            None => cook(entities, &worker_cancel),
        };
        let result =
            volumes.filter(|_| !worker_cancel.load(Ordering::Relaxed)).map(|volumes| RayWorld::new(key, volumes));
        let _ = send.send(result);
        ctx.request_repaint();
    });
    shared.ray_preparation.pending = Some(Pending { key, cancel, result });
}

fn cook(entities: Vec<(bool, Vec<Arc<hp_geom::SolidGeom>>)>, cancel: &AtomicBool) -> Option<ParticleVolumes> {
    use hp_bspc::phys::{BrushSide, ConvexSolid, PhysModel};
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    let mut volumes = ParticleVolumes::default();
    for (blocker, geometry) in entities {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let bounds = geometry.iter().fold(Aabb::empty(), |bounds, solid| bounds.union(&solid.aabb));
        if geometry.is_empty() {
            continue;
        }
        let model = PhysModel {
            model_index: 1,
            solids: geometry
                .iter()
                .enumerate()
                .map(|(index, solid)| ConvexSolid {
                    brush: index as i32,
                    sides: solid
                        .faces
                        .iter()
                        .map(|face| {
                            let n = face.plane.normal.as_vec3().to_array();
                            BrushSide {
                                plane: [n[0], n[1], n[2], face.plane.dist as f32],
                                visible: true,
                                ..Default::default()
                            }
                        })
                        .collect(),
                })
                .collect(),
            size: (bounds.max - bounds.min).as_vec3().to_array(),
            ..Default::default()
        };
        let lump = hp_bspc::phys::physcollide_lump(&[model]);
        let Some(phy) = hp_bsp::physics::model_as_phy(&lump, 1) else { continue };
        let Ok(collision) = hp_mdl::parse_phy(&phy) else { continue };
        let target = if blocker { &mut volumes.blockers } else { &mut volumes.precipitation };
        for solid in collision.solids {
            for triangle in solid.piece_triangles() {
                let points = triangle.map(|index| solid.vertices[index as usize]);
                target.push([points[0], points[2], points[1]]);
            }
        }
    }
    Some(volumes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_pcf::environment::Environment;

    fn box_geometry(minimum: [f64; 3], maximum: [f64; 3]) -> Arc<hp_geom::SolidGeom> {
        let sides = hp_geom::make_primitive(
            &hp_geom::PrimitiveParams {
                bounds: Aabb::new(DVec3::from(minimum), DVec3::from(maximum)),
                ..Default::default()
            },
            &mut || 1,
        )
        .unwrap();
        Arc::new(hp_geom::build_solid(&sides[0]))
    }

    fn cooked(blocker: bool) -> ParticleVolumes {
        let mut entities = vec![(false, vec![box_geometry([-128.0, -128.0, 16.0], [128.0, 128.0, 256.0])])];
        if blocker {
            entities.push((true, vec![box_geometry([-64.0, -64.0, 128.0], [64.0, 64.0, 144.0])]));
        }
        cook(entities, &AtomicBool::new(false)).unwrap()
    }

    fn native(blocker: bool) -> ParticleVolumes {
        let data = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../hp-pcf/tests/data/precip_physics.lump"));
        let triangles = |model| {
            let phy = hp_bsp::physics::model_as_phy(data, model).unwrap();
            hp_mdl::parse_phy(&phy)
                .unwrap()
                .solids
                .iter()
                .flat_map(|solid| {
                    solid.piece_triangles().map(|triangle| {
                        let points = triangle.map(|index| solid.vertices[index as usize]);
                        [points[0], points[2], points[1]]
                    })
                })
                .collect()
        };
        ParticleVolumes { precipitation: triangles(1), blockers: if blocker { triangles(2) } else { vec![] } }
    }

    #[test]
    fn cooked_and_native_volume_rays_keep_inward_normals_and_parametric_distance() {
        for volumes in [cooked(true), native(true)] {
            let world = RayWorld::new((0, 0, 0), volumes);
            for (environment, height) in
                [(RayEnvironment::Precipitation, 255.5), (RayEnvironment::PrecipitationBlocker, 128.5)]
            {
                let origin = [1.0, 2.0, 64.0];
                let hit = world.trace(environment, origin, [0.0, 0.0, 2.0], f32::MAX).unwrap();
                assert!((hit.distance - (height - 64.0) / 2.0).abs() < 0.0001);
                assert_eq!(
                    hit.normal,
                    if environment == RayEnvironment::Precipitation { [0.0, 0.0, -1.0] } else { [0.0, 0.0, 1.0] }
                );
                assert!(world.trace(environment, origin, [0.0, 0.0, 2.0], hit.distance - 0.001).is_none());
            }
            let outside =
                world.trace(RayEnvironment::Precipitation, [1.0, 2.0, 300.0], [0.0, 0.0, -1.0], f32::MAX).unwrap();
            assert_eq!(outside.normal, [0.0, 0.0, -1.0]);
            assert!(
                world.trace(RayEnvironment::Precipitation, [300.0, 2.0, 64.0], [0.0, 0.0, 1.0], f32::MAX).is_none()
            );
            assert!(world.trace(RayEnvironment::Precipitation, [0.0; 3], [0.0; 3], f32::MAX).is_none());
        }
    }

    #[test]
    fn editor_and_compiled_particle_rays_match_all_native_culling_captures() {
        let mut definitions = hp_pcf::parse_file(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../hp-pcf/tests/data/precip_cases.pcf"
        )))
        .unwrap();
        for definition in &mut definitions {
            definition.initializers.retain(|op| !op.function.eq_ignore_ascii_case("Remap Scalar to Vector"));
        }
        let contexts: Vec<serde_json::Value> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../hp-pcf/tests/data/precip_native_samples.json"
        )))
        .unwrap();
        let state = hp_app::testing::test_state();
        let physics = Physics::default();
        for context in contexts {
            let blocker = !context["blocker"].is_null();
            for volumes in [cooked(blocker), native(blocker)] {
                let rays = RayWorld::new(key(&state), volumes);
                let world = ParticleEnvironment {
                    state: &state,
                    physics: &physics,
                    lighting: None,
                    rays: Some(&rays),
                    models: None,
                };
                for case in context["cases"].as_array().unwrap() {
                    let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
                    let plan = Arc::new(hp_pcf::Plan::new(
                        definitions.iter().find(|definition| definition.name == name).unwrap(),
                    ));
                    assert!(plan.uses_ray_environments());
                    assert_eq!(plan.trace_requirement(), None);
                    let mut sim = hp_pcf::Sim::new(plan, 17);
                    sim.step_in(0.025, &hp_pcf::Points::at([0.0, 0.0, 64.0]), &[], 100, &world);
                    assert_eq!(
                        !sim.particles.is_empty(),
                        case["alive"].as_bool().unwrap(),
                        "{} {name}",
                        context["name"]
                    );
                    if let Some(particle) = sim.particles.first() {
                        let expected = case["life"].as_f64().unwrap() as f32;
                        assert!(
                            (particle.life - expected).abs() < 0.00001,
                            "{} {name}: {} vs {expected}",
                            context["name"],
                            particle.life
                        );
                    }
                }
                assert_eq!(
                    world
                        .trace_environment(RayEnvironment::Precipitation, [1.0, 2.0, 64.0], [0.0, 0.0, 1.0], f32::MAX)
                        .unwrap()
                        .normal,
                    [0.0, 0.0, -1.0]
                );
            }
        }
    }

    #[test]
    fn unused_named_worlds_stay_lazy_and_compiled_replacements_invalidate_snapshots() {
        let mut state = hp_app::testing::test_state();
        state.settings.lab.show_particles = true;
        let handle = crate::particles::new_handle();
        let ctx = egui::Context::default();
        prepare(&state, &handle, &ctx);
        assert!(handle.lock().unwrap().ray_preparation.pending.is_none());
        state.runtime.lab.player.active = true;
        state.runtime.lab.player.editor_world = false;
        state.runtime.health.world = Some(Arc::new(BspWorld { particle_volumes: native(true), ..Default::default() }));
        handle.lock().unwrap().ray_requirement = true;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            prepare(&state, &handle, &ctx);
            if handle.lock().unwrap().rays.ready(&state) {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let old = Arc::clone(&handle.lock().unwrap().rays);
        assert!(old.trace(RayEnvironment::Precipitation, [1.0, 2.0, 64.0], [0.0, 0.0, 1.0], f32::MAX).is_some());
        assert!(handle.lock().unwrap().physics.key.is_none());
        state.runtime.health.world = Some(Arc::new(BspWorld::default()));
        assert!(!old.ready(&state));
        assert!(cook(vec![], &AtomicBool::new(true)).is_none());
        assert!(cook(vec![(false, vec![])], &AtomicBool::new(true)).is_none());
    }
}
