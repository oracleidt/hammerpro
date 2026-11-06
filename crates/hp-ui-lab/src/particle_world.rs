use std::collections::HashMap;
use std::sync::Arc;

use glam::{DMat4, DVec3};
use hp_app::AppState;
use hp_app::state::CollisionTriangles;
use hp_editor::Owner;
use hp_geom::{Aabb, SweepHit};
use hp_pcf::environment::{Environment, Trace, Vector};

use crate::particles::ParticleHandle;

mod tree;
use tree::SurfaceTree;

#[derive(Clone, Debug)]
struct Body {
    path: String,
    transform: DMat4,
    collision: Option<SurfaceTree>,
}

#[derive(Clone, Debug)]
struct Surface {
    bounds: Aabb,
    points: [DVec3; 3],
}

#[derive(Clone, Debug, Default)]
pub struct Physics {
    key: Option<(u64, u64, u64, usize)>,
    models: HashMap<String, Option<CollisionTriangles>>,
    bodies: Vec<Body>,
    compiled: Option<SurfaceTree>,
}

fn vec(p: Vector) -> DVec3 {
    DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
}

fn non_solid_class(class: &str) -> bool {
    class.starts_with("trigger_")
        || class.starts_with("func_areaportal")
        || matches!(
            class,
            "func_illusionary"
                | "func_occluder"
                | "func_dustmotes"
                | "func_dustcloud"
                | "func_precipitation"
                | "func_smokevolume"
                | "func_viscluster"
                | "func_ladder"
                | "func_buyzone"
                | "func_bomb_target"
                | "env_bubbles"
                | "env_embers"
        )
}

fn non_solid_material(material: &str) -> bool {
    matches!(
        material.to_ascii_lowercase().as_str(),
        "tools/toolstrigger"
            | "tools/toolsskip"
            | "tools/toolshint"
            | "tools/toolsfog"
            | "tools/toolsareaportal"
            | "tools/toolsoccluder"
            | "tools/toolsinvisibleladder"
            | "tools/toolsorigin"
            | "tools/toolsplayerclip"
            | "tools/toolsnpcclip"
            | "tools/toolsclip"
    )
}

pub fn prepare(state: &mut AppState, handle: &ParticleHandle) {
    let Ok(mut shared) = handle.lock() else { return };
    let physics = Arc::make_mut(&mut shared.physics);
    let compiled = (state.runtime.lab.player.active && !state.runtime.lab.player.editor_world)
        .then(|| state.runtime.health.world.as_ref())
        .flatten();
    let compiled_id = compiled.map_or(0, |world| Arc::as_ptr(world) as usize);
    let key = (state.sidecar.serial(), state.doc.revision(), state.assets.generation(), compiled_id);
    if physics.key != Some(key) {
        if physics.key.is_none_or(|old| old.2 != key.2) {
            physics.models.clear();
        }
        physics.bodies.clear();
        physics.compiled = compiled.map(|world| {
            let mut surfaces = Vec::new();
            for face in world.faces.iter().chain(&world.entity_faces) {
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
            SurfaceTree::new(surfaces)
        });
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
            physics.bodies.push(Body { path, transform, collision: None });
        }
        physics.key = Some(key);
    }
    for body in &mut physics.bodies {
        if !physics.models.contains_key(&body.path)
            && let Some(model) = state.assets.collision_triangles(&body.path)
        {
            physics.models.insert(body.path.clone(), model);
        }
        if body.collision.is_none()
            && let Some(Some(model)) = physics.models.get(&body.path)
        {
            body.collision = Some(SurfaceTree::new(
                model
                    .iter()
                    .map(|triangle| {
                        let points = triangle.map(|point| body.transform.transform_point3(vec(point)));
                        Surface { bounds: Aabb::from_points(points), points }
                    })
                    .collect(),
            ));
        }
    }
}

pub struct ParticleEnvironment<'a> {
    pub state: &'a AppState,
    pub physics: &'a Physics,
}

impl Environment for ParticleEnvironment<'_> {
    fn trace(&self, start: Vector, end: Vector, radius: f32, brush_only: bool) -> Option<Trace> {
        if !radius.is_finite() || radius < 0.0 {
            return None;
        }
        let (start, end) = (vec(start), vec(end));
        let half = DVec3::splat(f64::from(radius));
        let bounds = Aabb::new(start, end).expand(f64::from(radius));
        let mut best = if let Some(surfaces) = &self.physics.compiled {
            surfaces.trace(start, end, half, &bounds)
        } else {
            let filter = |node: &hp_editor::SolidNode| {
                let owner = match node.owner {
                    Owner::World => true,
                    Owner::Entity(id) => self.state.doc.scene().entity(id).is_some_and(|node| {
                        !non_solid_class(&node.entity.classname.to_ascii_lowercase())
                            && node.entity.get("Solidity").is_none_or(|s| s.trim() != "1")
                    }),
                };
                owner && node.solid.sides.iter().any(|side| !non_solid_material(&side.material))
            };
            self.state.doc.trace_hull(start, end, half, Some(&filter)).map(|(_, hit)| hit)
        };
        if !brush_only {
            for body in &self.physics.bodies {
                if let Some(tree) = &body.collision
                    && let Some(hit) = tree.trace(start, end, half, &bounds)
                    && best.is_none_or(|old: SweepHit| hit.fraction < old.fraction)
                {
                    best = Some(hit);
                }
            }
        }
        best.map(|hit| {
            let approach = -hit.normal.dot(end - start);
            let fraction = if !hit.start_solid && approach > 1e-12 {
                (hit.fraction - 0.03125 / approach).max(0.0)
            } else {
                hit.fraction
            };
            Trace { fraction: fraction as f32, normal: hit.normal.as_vec3().to_array(), start_solid: hit.start_solid }
        })
    }

    fn wind(&self, position: Vector) -> Vector {
        hp_app::runtime::wind::velocity_at(vec(position)).unwrap_or(DVec3::ZERO).as_vec3().to_array()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_hull_traces_match_captured_garrys_mod_results() {
        let mut state = hp_app::testing::test_state();
        state.doc =
            hp_editor::Document::from_map(hp_vmf::parse(include_str!("../tests/data/particle_trace.vmf")).unwrap());
        let handle = crate::particles::new_handle();
        prepare(&mut state, &handle);
        let shared = handle.lock().unwrap();
        let world = ParticleEnvironment { state: &state, physics: &shared.physics };
        let captures: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/data/particle_trace_native.json")).unwrap();
        let vector = |value: &serde_json::Value| {
            let v = value.as_array().unwrap();
            [v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32, v[2].as_f64().unwrap() as f32]
        };
        for capture in captures {
            let start = vector(&capture["start"]);
            let end = vector(&capture["finish"]);
            let radius = capture["radius"].as_f64().unwrap() as f32;
            let hit = world.trace(start, end, radius, true);
            if !capture["hit"].as_bool().unwrap() {
                assert!(hit.is_none());
                continue;
            }
            let hit = hit.unwrap();
            let expected = capture["fraction"].as_f64().unwrap() as f32;
            assert!(
                (hit.fraction - expected).abs() < 1e-6,
                "radius {radius}, {start:?} -> {end:?}: {} versus native {expected}",
                hit.fraction
            );
            assert_eq!(hit.normal, vector(&capture["normal"]));
            let point = vec(start).lerp(vec(end), f64::from(hit.fraction));
            assert!(point.distance(vec(vector(&capture["hitpos"]))) < 0.0001);
            assert_eq!(hit.start_solid, capture["startsolid"].as_bool().unwrap());
        }
    }
}
