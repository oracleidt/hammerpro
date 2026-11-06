use std::collections::HashMap;
use std::sync::Arc;

use glam::{DMat4, DVec3};
use hp_app::AppState;
use hp_app::state::CollisionTriangles;
use hp_editor::Owner;
use hp_geom::{Aabb, SweepHit};
use hp_pcf::environment::{Environment, Trace, Vector};

use crate::particles::ParticleHandle;

mod build;
pub(crate) mod models;
pub(crate) mod rays;
mod tree;
pub(crate) use build::Preparation;
pub use build::prepare;
use tree::SurfaceTree;

#[derive(Clone, Debug)]
struct Body {
    id: u32,
    path: String,
    transform: DMat4,
    collision: Option<Arc<SurfaceTree>>,
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
    includes_props: bool,
    bodies: Vec<Body>,
    compiled: Option<Arc<SurfaceTree>>,
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

pub struct ParticleEnvironment<'a> {
    pub state: &'a AppState,
    pub physics: &'a Physics,
    pub lighting: Option<&'a hp_app::state::ParticleLighting>,
    pub(crate) rays: Option<&'a rays::RayWorld>,
    pub(crate) models: Option<&'a models::Models>,
}

impl Environment for ParticleEnvironment<'_> {
    fn model_hitboxes(&self, object: u32, set: &str) -> Option<Arc<[hp_pcf::environment::ModelHitbox]>> {
        self.models?.data.hitboxes(object, set)
    }

    fn inside_model(&self, object: u32, position: Vector) -> bool {
        self.physics.bodies.iter().filter(|body| body.id == object).any(|body| {
            body.collision.as_ref().is_some_and(|tree| {
                let direction = [0.524, 0.373, 0.765];
                tree.ray(position, direction, f32::MAX)
                    .is_some_and(|hit| hit.normal.iter().zip(direction).map(|(a, b)| a * b).sum::<f32>() > 0.0)
            })
        })
    }
    fn trace_environment(
        &self,
        environment: hp_pcf::environment::RayEnvironment,
        start: Vector,
        direction: Vector,
        maximum: f32,
    ) -> Option<hp_pcf::environment::RayHit> {
        self.rays?.trace(environment, start, direction, maximum)
    }

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
            let flags = if radius == 0.0 { hp_geom::DISP_FLAG_NO_RAY } else { hp_geom::DISP_FLAG_NO_HULL };
            self.state.doc.trace_hull_with_disp_flags(start, end, half, Some(&filter), flags).map(|(_, hit)| hit)
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

    fn lighting(&self, position: Vector) -> Vector {
        let styles = self.lighting.map_or(&[][..], |lighting| lighting.styles.as_slice());
        if self.state.runtime.lab.player.active
            && !self.state.runtime.lab.player.editor_world
            && let Some(world) = &self.state.runtime.health.world
        {
            return if world.light.mode == hp_editor::limits::world_light::LightMode::Fullbright {
                [1.0; 3]
            } else {
                world.light.props.point_color_with_styles(position, styles)
            };
        }
        if let Some(lighting) = self.lighting
            && let Some(baked) = &lighting.baked
            && lighting
                .region
                .is_none_or(|[lo, hi]| (0..3).all(|axis| (lo[axis]..=hi[axis]).contains(&f64::from(position[axis]))))
        {
            return baked.point_color_with_styles(position, styles);
        }
        [1.0; 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_lighting_uses_the_bake_region_and_the_active_compiled_world() {
        let mut state = hp_app::testing::test_state();
        let physics = Physics::default();
        let lighting = hp_app::state::ParticleLighting {
            baked: Some(Arc::new(hp_editor::limits::world_light::PropLighting::default())),
            region: Some([[-16.0; 3], [16.0; 3]]),
            ..Default::default()
        };
        let world = ParticleEnvironment {
            state: &state,
            physics: &physics,
            lighting: Some(&lighting),
            rays: None,
            models: None,
        };
        assert_eq!(world.lighting([0.0; 3]), [0.0; 3]);
        assert_eq!(world.lighting([100.0; 3]), [1.0; 3]);
        state.runtime.lab.player.active = true;
        state.runtime.lab.player.editor_world = false;
        state.runtime.health.world = Some(Arc::new(hp_editor::limits::world::BspWorld::default()));
        let world = ParticleEnvironment {
            state: &state,
            physics: &physics,
            lighting: Some(&lighting),
            rays: None,
            models: None,
        };
        assert_eq!(world.lighting([0.0; 3]), [1.0; 3]);
        let mut compiled = hp_editor::limits::world::BspWorld::default();
        compiled.light.mode = hp_editor::limits::world_light::LightMode::Hdr;
        state.runtime.health.world = Some(Arc::new(compiled));
        let world = ParticleEnvironment { state: &state, physics: &physics, lighting: None, rays: None, models: None };
        assert_eq!(world.lighting([100.0; 3]), [0.0; 3]);
    }

    #[test]
    fn displacement_ray_and_hull_flags_are_respected_independently() {
        let mut state = hp_app::testing::test_state();
        state.doc = hp_editor::Document::new_empty();
        let id = state
            .doc
            .create_primitive(
                &hp_geom::PrimitiveParams {
                    bounds: Aabb::new(DVec3::new(-64.0, -64.0, -16.0), DVec3::new(64.0, 64.0, 0.0)),
                    ..Default::default()
                },
                Owner::World,
            )
            .unwrap()[0];
        let side =
            state.doc.scene().solid(id).unwrap().geom.faces.iter().find(|f| f.plane.normal.z > 0.9).unwrap().side_index;
        let face = hp_editor::FaceRef { solid: id, side };
        state.doc.create_displacements(&[face], 2).unwrap();
        for (flag, ray, hull) in [
            (hp_geom::DISP_FLAG_NO_RAY, false, true),
            (hp_geom::DISP_FLAG_NO_HULL, true, false),
            (hp_geom::DISP_FLAG_NO_PHYSICS, true, true),
        ] {
            state
                .doc
                .set_disp_flags(
                    &[face],
                    hp_geom::DISP_FLAG_NO_RAY | hp_geom::DISP_FLAG_NO_HULL | hp_geom::DISP_FLAG_NO_PHYSICS,
                    false,
                )
                .unwrap();
            state.doc.set_disp_flags(&[face], flag, true).unwrap();
            let world = ParticleEnvironment {
                state: &state,
                physics: &Physics::default(),
                lighting: None,
                rays: None,
                models: None,
            };
            assert_eq!(world.trace([0.0, 0.0, 64.0], [0.0, 0.0, -64.0], 0.0, true).is_some(), ray);
            assert_eq!(world.trace([0.0, 0.0, 64.0], [0.0, 0.0, -64.0], 2.0, true).is_some(), hull);
        }
    }

    #[test]
    fn brush_hull_traces_match_captured_garrys_mod_results() {
        let mut state = hp_app::testing::test_state();
        state.doc =
            hp_editor::Document::from_map(hp_vmf::parse(include_str!("../tests/data/particle_trace.vmf")).unwrap());
        let handle = crate::particles::new_handle();
        prepare(&mut state, &handle, &egui::Context::default());
        let shared = handle.lock().unwrap();
        let world =
            ParticleEnvironment { state: &state, physics: &shared.physics, lighting: None, rays: None, models: None };
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
