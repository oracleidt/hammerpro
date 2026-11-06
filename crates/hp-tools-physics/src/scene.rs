use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use glam::{DMat4, DVec3};
use hp_app::AppState;
use hp_editor::{ObjRef, Owner};
use hp_geom::{DispMesh, SolidGeom};
use hp_physics::{Body, ConvexPart, Simulation, Transform};

use crate::{Binding, Ragdoll, Session, engine_settings};

struct Brush {
    id: u32,
    owner: Owner,
    geom: Arc<SolidGeom>,
    disps: Vec<Option<Arc<DispMesh>>>,
    no_physics: Vec<bool>,
    materials: Vec<String>,
    water_contents: bool,
}

pub struct Snapshot {
    brushes: Vec<Brush>,
    entities: Vec<hp_vmf::Entity>,
    selected: HashSet<ObjRef>,
    fs: Option<hp_fs::GameFs>,
    settings: hp_app::settings::PhysicsSettings,
}

impl Snapshot {
    pub fn capture(state: &AppState) -> Self {
        let scene = state.doc.scene();
        let selected = state
            .doc
            .selected_objects()
            .into_iter()
            .map(|obj| match obj {
                ObjRef::Solid(id) => match scene.solid(id).map(|n| n.owner) {
                    Some(Owner::Entity(owner)) => ObjRef::Entity(owner),
                    _ => obj,
                },
                _ => obj,
            })
            .collect();
        Self {
            brushes: scene
                .solids()
                .values()
                .map(|node| Brush {
                    id: node.solid.id,
                    owner: node.owner,
                    geom: Arc::clone(&node.geom),
                    disps: node.disp.clone(),
                    no_physics: node
                        .solid
                        .sides
                        .iter()
                        .map(|side| {
                            side.disp.as_ref().is_some_and(|disp| disp.flags & hp_geom::DISP_FLAG_NO_PHYSICS != 0)
                        })
                        .collect(),
                    materials: node.solid.sides.iter().map(|side| side.material.clone()).collect(),
                    water_contents: node.solid.sides.iter().any(|side| {
                        side.extra_props.iter().any(|(key, value)| {
                            key.eq_ignore_ascii_case("contents")
                                && value.parse::<u32>().is_ok_and(|mask| mask & 32 != 0)
                        })
                    }),
                })
                .collect(),
            entities: scene.entities().values().map(|n| n.entity.clone()).collect(),
            selected,
            fs: state.assets.fs().cloned(),
            settings: state.settings.physics,
        }
    }

    pub fn build(self, cancel: &AtomicBool) -> Result<Session, String> {
        let mut simulation = Simulation::new(engine_settings(self.settings));
        let mut bindings = Vec::new();
        let mut ragdolls = Vec::new();
        let mut skipped = Vec::new();
        let entities: HashMap<_, _> = self.entities.iter().map(|e| (e.id, e)).collect();
        let mut dynamic: HashMap<ObjRef, Vec<&Brush>> = HashMap::new();
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for brush in &self.brushes {
            if cancel.load(Ordering::Relaxed) {
                return Err(String::new());
            }
            if let Owner::Entity(id) = brush.owner
                && entities.get(&id).is_none_or(|entity| !solid_entity(entity))
            {
                continue;
            }
            let obj = match brush.owner {
                Owner::World => ObjRef::Solid(brush.id),
                Owner::Entity(id) => ObjRef::Entity(id),
            };
            let water = brush.water_contents
                || brush
                    .materials
                    .iter()
                    .any(|name| self.fs.as_ref().and_then(|fs| fs.material(name)).is_some_and(|mat| mat.is_water()));
            if water {
                simulation
                    .add_water(hp_physics::WaterVolume {
                        planes: brush.geom.faces.iter().map(|face| (face.plane.normal, face.plane.dist)).collect(),
                        bounds: [brush.geom.aabb.min, brush.geom.aabb.max],
                        density: 1000.0,
                    })
                    .map_err(|error| error.to_string())?;
                continue;
            }
            if brush.materials.iter().all(|name| non_solid_material(name)) {
                continue;
            }
            if self.selected.contains(&obj) && brush.disps.iter().all(Option::is_none) {
                dynamic.entry(obj).or_default().push(brush);
            } else {
                for face in &brush.geom.faces {
                    if let Some(Some(disp)) = brush.disps.get(face.side_index) {
                        if brush.no_physics.get(face.side_index).copied().unwrap_or(false) {
                            continue;
                        }
                        append_mesh(
                            &mut vertices,
                            &mut triangles,
                            &disp.positions,
                            disp.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]),
                        );
                    } else if brush.disps.iter().all(Option::is_none) {
                        append_mesh(
                            &mut vertices,
                            &mut triangles,
                            &face.vertices,
                            face.triangles().map(|t| t.map(|v| v as u32)),
                        );
                    }
                }
            }
        }
        let mut dynamic: Vec<_> = dynamic.into_iter().collect();
        dynamic.sort_by_key(|(obj, _)| *obj);
        for (obj, brushes) in dynamic {
            let center = hp_geom::Aabb::from_points(
                brushes.iter().flat_map(|b| b.geom.faces.iter().flat_map(|face| face.vertices.iter().copied())),
            )
            .center();
            let parts = brushes
                .iter()
                .map(|brush| ConvexPart {
                    points: brush
                        .geom
                        .faces
                        .iter()
                        .flat_map(|face| face.vertices.iter().map(|p| *p - center))
                        .collect(),
                    transform: Transform::default(),
                })
                .collect();
            let transform = Transform { position: center, ..Default::default() };
            let id = simulation.body_count() as u64;
            simulation
                .add_body(&Body { id, transform, parts, mass: 10.0, friction: 0.8, restitution: 0.0 })
                .map_err(|error| error.to_string())?;
            bindings.push(Binding { id, obj, initial: DMat4::from_translation(center) });
        }
        let mut models = HashMap::new();
        let mut rigs = HashMap::new();
        for entity in &self.entities {
            if cancel.load(Ordering::Relaxed) {
                return Err(String::new());
            }
            if !entity.classname.to_ascii_lowercase().starts_with("prop_") && entity.classname != "physics_prop_ragdoll"
            {
                continue;
            }
            let Some(path) = entity.get("model").filter(|path| path.to_ascii_lowercase().ends_with(".mdl")) else {
                continue;
            };
            let obj = ObjRef::Entity(entity.id);
            let selected = self.selected.contains(&obj);
            if !selected && entity.get("solid").is_some_and(|s| s.trim() == "0") {
                continue;
            }
            if matches!(entity.classname.to_ascii_lowercase().as_str(), "prop_ragdoll" | "physics_prop_ragdoll") {
                let body = entity.get("body").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                let rig = rigs.entry((path.to_ascii_lowercase(), body)).or_insert_with(|| {
                    let fs = self.fs.as_ref()?;
                    hp_ragdoll::Rig::load(path, |path| fs.read(path), body).ok().map(Arc::new)
                });
                if let Some(rig) = rig {
                    if let Some(ragdoll) =
                        add_ragdoll(entity, Arc::clone(rig), selected, &mut simulation, &mut vertices, &mut triangles)?
                    {
                        ragdolls.push(ragdoll);
                    }
                } else if selected {
                    skipped.push(path.to_owned());
                }
                continue;
            }
            let data = models.entry(path.to_ascii_lowercase()).or_insert_with(|| {
                let fs = self.fs.as_ref()?;
                let phy = fs.read(&hp_mdl::phy_path(path))?;
                let collision = match fs.read(path) {
                    Some(mdl) => hp_mdl::parse_phy_for_model(&phy, &mdl, hp_mdl::Pose::FirstSequence),
                    None => hp_mdl::parse_phy(&phy),
                }
                .ok()?;
                Some((collision, physics_mass(&phy)))
            });
            let Some((collision, mass)) = data else {
                if selected {
                    skipped.push(path.to_owned());
                }
                continue;
            };
            let scale = model_scale(entity);
            let transform = Transform {
                position: entity.origin().filter(|p| p.is_finite()).unwrap_or(DVec3::ZERO),
                rotation: hp_geom::angles_to_quat(entity.angles().filter(|p| p.is_finite()).unwrap_or(DVec3::ZERO)),
            };
            let matrix = DMat4::from_rotation_translation(transform.rotation, transform.position);
            if selected {
                let mut parts = Vec::new();
                for solid in &collision.solids {
                    let all = solid.ledges.is_empty().then_some(&solid.triangles[..]);
                    let pieces = solid.ledges.iter().filter_map(|range| solid.triangles.get(range.clone()));
                    for triangles in all.into_iter().chain(pieces) {
                        let points = triangles
                            .iter()
                            .flat_map(|t| t.iter())
                            .filter_map(|i| solid.vertices.get(*i as usize))
                            .map(|p| DVec3::from(p.map(f64::from)) * scale)
                            .collect();
                        parts.push(ConvexPart { points, transform: Transform::default() });
                    }
                }
                let id = simulation.body_count() as u64;
                let override_mass = entity
                    .get("massScale")
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .unwrap_or(1.0);
                simulation
                    .add_body(&Body {
                        id,
                        transform,
                        parts,
                        mass: *mass * override_mass,
                        friction: 0.8,
                        restitution: 0.0,
                    })
                    .map_err(|error| error.to_string())?;
                bindings.push(Binding { id, obj, initial: matrix });
            } else {
                for solid in &collision.solids {
                    let points: Vec<_> = solid
                        .vertices
                        .iter()
                        .map(|p| matrix.transform_point3(DVec3::from(p.map(f64::from)) * scale))
                        .collect();
                    append_mesh(&mut vertices, &mut triangles, &points, solid.piece_triangles().copied());
                }
            }
        }
        if !triangles.is_empty() {
            simulation.add_static_mesh(&vertices, &triangles).map_err(|error| error.to_string())?;
        }
        simulation.prepare();
        Ok(Session { simulation, bindings, ragdolls, skipped })
    }
}

fn model_scale(entity: &hp_vmf::Entity) -> f64 {
    entity
        .get("modelscale")
        .or_else(|| entity.get("uniformscale"))
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(1.0)
        .min(10000.0)
}

fn transform(matrix: DMat4) -> Transform {
    let (_, rotation, position) = matrix.to_scale_rotation_translation();
    Transform { position, rotation }
}

pub(super) fn add_ragdoll(
    entity: &hp_vmf::Entity,
    rig: Arc<hp_ragdoll::Rig>,
    selected: bool,
    simulation: &mut Simulation,
    vertices: &mut Vec<DVec3>,
    triangles: &mut Vec<[u32; 3]>,
) -> Result<Option<Ragdoll>, String> {
    let scale = model_scale(entity);
    let initial = DMat4::from_scale_rotation_translation(
        DVec3::splat(scale),
        hp_geom::angles_to_quat(entity.angles().filter(|p| p.is_finite()).unwrap_or(DVec3::ZERO)),
        entity.origin().filter(|p| p.is_finite()).unwrap_or(DVec3::ZERO),
    );
    let poses = rig.body_poses(initial, entity.get("angleOverride").unwrap_or(""));
    let mass_scale = entity
        .get("massScale")
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n > 0.0)
        .unwrap_or(1.0);
    let mut bodies = Vec::new();
    for (body, pose) in rig.bodies.iter().zip(poses) {
        if selected {
            let id = simulation.body_count() as u64;
            let parts = body
                .parts
                .iter()
                .map(|part| ConvexPart {
                    points: part.iter().map(|p| *p * scale).collect(),
                    transform: Transform::default(),
                })
                .collect();
            simulation
                .add_body(&Body {
                    id,
                    transform: transform(pose),
                    parts,
                    mass: body.mass * mass_scale,
                    friction: 0.8,
                    restitution: 0.0,
                })
                .map_err(|e| e.to_string())?;
            simulation.set_damping(id, body.damping, body.angular_damping);
            bodies.push(id);
        } else {
            for part in &body.parts {
                let points: Vec<_> = part.iter().map(|point| pose.transform_point3(*point)).collect();
                append_mesh(
                    vertices,
                    triangles,
                    &points,
                    (0..points.len() as u32 / 3).map(|i| [i * 3, i * 3 + 1, i * 3 + 2]),
                );
            }
        }
    }
    if !selected {
        return Ok(None);
    }
    for joint in &rig.joints {
        let (parent_frame, child_frame) = rig.joint_frames(joint, scale);
        simulation
            .add_joint(&hp_physics::Joint {
                parent: bodies[joint.parent],
                child: bodies[joint.child],
                parent_frame: transform(parent_frame),
                child_frame: transform(child_frame),
                limits: joint.limits,
                friction: joint.friction,
            })
            .map_err(|e| e.to_string())?;
    }
    for &(a, b) in &rig.disabled_collisions {
        simulation.disable_collision_pair(bodies[a], bodies[b]);
    }
    Ok(Some(Ragdoll { entity: entity.id, rig, bodies, scale, initial }))
}

fn append_mesh(
    vertices: &mut Vec<DVec3>,
    triangles: &mut Vec<[u32; 3]>,
    points: &[DVec3],
    faces: impl Iterator<Item = [u32; 3]>,
) {
    let first = vertices.len() as u32;
    vertices.extend_from_slice(points);
    triangles.extend(faces.map(|t| t.map(|i| first + i)));
}

fn physics_mass(bytes: &[u8]) -> f64 {
    let Some(root) = hp_ragdoll::metadata(bytes) else { return 10.0 };
    let mass: f64 = root
        .children
        .iter()
        .filter(|b| b.name.eq_ignore_ascii_case("solid"))
        .filter_map(|b| b.get_parsed::<f64>("mass"))
        .filter(|v| v.is_finite() && *v > 0.0)
        .sum();
    if mass > 0.0 { mass } else { 10.0 }
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
    )
}

fn solid_entity(entity: &hp_vmf::Entity) -> bool {
    let class = entity.classname.to_ascii_lowercase();
    !(class.starts_with("trigger_")
        || class.starts_with("env_")
        || class.starts_with("func_areaportal")
        || matches!(
            class.as_str(),
            "func_illusionary"
                | "func_occluder"
                | "func_dustmotes"
                | "func_dustcloud"
                | "func_precipitation"
                | "func_smokevolume"
                | "func_viscluster"
                | "func_ladder"
        ))
        && !(class == "func_brush"
            && (entity.get("Solidity") == Some("1")
                || (entity.get("Solidity").is_none_or(|v| v == "0") && entity.get("StartDisabled") == Some("1"))))
}
