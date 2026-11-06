#![forbid(unsafe_code)]

use glam::{DQuat, DVec3};
use rapier::prelude::*;
use std::collections::{HashMap, HashSet};

pub type BodyId = u64;
const UNIT: f64 = 0.0254;
pub const STEP: f64 = 1.0 / 120.0;

mod fluid;

pub struct WaterVolume {
    pub planes: Vec<(DVec3, f64)>,
    pub bounds: [DVec3; 2],
    pub density: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: DVec3,
    pub rotation: DQuat,
}
impl Default for Transform {
    fn default() -> Self {
        Self { position: DVec3::ZERO, rotation: DQuat::IDENTITY }
    }
}

#[derive(Clone, Debug)]
pub struct ConvexPart {
    pub points: Vec<DVec3>,
    pub transform: Transform,
}

#[derive(Clone, Debug)]
pub struct Body {
    pub id: BodyId,
    pub transform: Transform,
    pub parts: Vec<ConvexPart>,
    pub mass: f64,
    pub friction: f64,
    pub restitution: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub gravity: f64,
    pub air_density: f64,
    pub time_scale: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self { gravity: 600.0, air_density: 2.0, time_scale: 1.0 }
    }
}

impl Settings {
    pub fn clamped(self) -> Self {
        let defaults = Self::default();
        let finite = |value: f64, default: f64, min: f64, max: f64| {
            if value.is_finite() { value.clamp(min, max) } else { default }
        };
        Self {
            gravity: finite(self.gravity, defaults.gravity, -100000.0, 100000.0),
            air_density: finite(self.air_density, defaults.air_density, 0.0, 1000.0),
            time_scale: finite(self.time_scale, defaults.time_scale, 0.01, 4.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub body: Option<BodyId>,
    pub point: DVec3,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid collision geometry")]
    Geometry,
    #[error("Invalid body transform or physical properties")]
    Properties,
    #[error("Duplicate physics body {0}")]
    Duplicate(BodyId),
}

struct Binding {
    handle: RigidBodyHandle,
    area: f64,
    mass: f64,
    hulls: Vec<fluid::Polyhedron>,
    volume: f64,
}
struct Drag {
    anchor: RigidBodyHandle,
    joint: ImpulseJointHandle,
    target: RigidBodyHandle,
}

#[derive(Default)]
struct Contacts(HashSet<(u128, u128)>);
impl PhysicsHooks for Contacts {
    fn filter_contact_pair(&self, context: &PairFilterContext<'_>) -> Option<SolverFlags> {
        let a = context.colliders[context.collider1].user_data;
        let b = context.colliders[context.collider2].user_data;
        (!self.0.contains(&(a.min(b), a.max(b)))).then_some(SolverFlags::COMPUTE_RIGID_IMPULSES)
    }
}

pub struct Joint {
    pub parent: BodyId,
    pub child: BodyId,
    pub parent_frame: Transform,
    pub child_frame: Transform,
    pub limits: [[f64; 2]; 3],
    pub friction: [f64; 3],
}

pub struct Simulation {
    world: PhysicsWorld,
    bindings: HashMap<BodyId, Binding>,
    reverse: HashMap<RigidBodyHandle, BodyId>,
    drag: Option<Drag>,
    accumulator: f64,
    water: Vec<WaterVolume>,
    contacts: Contacts,
    pub paused: bool,
    pub settings: Settings,
}

fn vector(v: DVec3) -> Vector {
    Vector::new(v.x, v.y, v.z)
}
fn from_vector(v: Vector) -> DVec3 {
    DVec3::new(v.x, v.y, v.z)
}
fn pose(t: Transform) -> Option<Pose> {
    if !t.position.is_finite() || !t.rotation.is_finite() || t.rotation.length_squared() < 1e-12 {
        return None;
    }
    let q = t.rotation.normalize();
    Some(Pose::from_parts(vector(t.position * UNIT), Rotation::from_xyzw(q.x, q.y, q.z, q.w)))
}

impl Default for Simulation {
    fn default() -> Self {
        Self::new(Settings::default())
    }
}
impl Simulation {
    pub fn new(settings: Settings) -> Self {
        let mut world = PhysicsWorld::new();
        world.integration_parameters.dt = STEP;
        world.integration_parameters.max_ccd_substeps = 4;
        Self {
            world,
            bindings: HashMap::new(),
            reverse: HashMap::new(),
            drag: None,
            accumulator: 0.0,
            water: Vec::new(),
            contacts: Contacts::default(),
            paused: true,
            settings: settings.clamped(),
        }
    }
    pub fn add_body(&mut self, body: &Body) -> Result<(), Error> {
        if self.bindings.contains_key(&body.id) {
            return Err(Error::Duplicate(body.id));
        }
        let transform = pose(body.transform).ok_or(Error::Properties)?;
        if !body.mass.is_finite() || body.mass <= 0.0 || !body.friction.is_finite() || !body.restitution.is_finite() {
            return Err(Error::Properties);
        }
        let mut pieces = Vec::new();
        let mut hulls = Vec::new();
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for part in &body.parts {
            let transform = pose(part.transform).ok_or(Error::Properties)?;
            if part.points.len() < 4 || part.points.iter().any(|p| !p.is_finite()) {
                return Err(Error::Geometry);
            }
            let points: Vec<_> = part.points.iter().map(|p| vector(*p * UNIT)).collect();
            let shape = SharedShape::convex_hull(&points).ok_or(Error::Geometry)?;
            let hull = shape.as_convex_polyhedron().ok_or(Error::Geometry)?;
            let faces = hull
                .faces()
                .iter()
                .map(|face| {
                    let start = face.first_vertex_or_edge as usize;
                    hull.vertices_adj_to_face()[start..start + face.num_vertices_or_edges as usize]
                        .iter()
                        .map(|i| {
                            part.transform.rotation.normalize() * (from_vector(hull.points()[*i as usize]) / UNIT)
                                + part.transform.position
                        })
                        .collect()
                })
                .collect();
            hulls.push(fluid::Polyhedron { faces });
            for p in &part.points {
                let p = part.transform.rotation.normalize() * (*p) + part.transform.position;
                lo = lo.min(p);
                hi = hi.max(p);
            }
            pieces.push((transform, shape));
        }
        if pieces.is_empty() {
            return Err(Error::Geometry);
        }
        let size = (hi - lo) * UNIT;
        let volume = hulls.iter().filter_map(fluid::Polyhedron::volume_center).map(|(v, _)| v).sum();
        let area = (size.x * size.y).max(size.x * size.z).max(size.y * size.z);
        let collider = ColliderBuilder::compound(pieces)
            .user_data(u128::from(body.id) + 1)
            .active_hooks(ActiveHooks::FILTER_CONTACT_PAIRS)
            .mass(body.mass)
            .friction(body.friction.max(0.0))
            .restitution(body.restitution.clamp(0.0, 1.0));
        let handle =
            self.world.insert_body(RigidBodyBuilder::dynamic().pose(transform).ccd_enabled(true).angular_damping(0.05));
        self.world.insert_collider(collider, Some(handle));
        self.bindings.insert(body.id, Binding { handle, area, mass: body.mass, hulls, volume });
        self.reverse.insert(handle, body.id);
        Ok(())
    }
    pub fn add_water(&mut self, mut water: WaterVolume) -> Result<(), Error> {
        if !water.density.is_finite()
            || water.density <= 0.0
            || water.planes.len() < 4
            || water.bounds.iter().any(|p| !p.is_finite())
            || !water.bounds[0].cmple(water.bounds[1]).all()
        {
            return Err(Error::Properties);
        }
        for (normal, distance) in &mut water.planes {
            let length = normal.length();
            if !length.is_finite() || length < 1e-12 || !distance.is_finite() {
                return Err(Error::Geometry);
            }
            *normal /= length;
            *distance /= length;
        }
        self.water.push(water);
        Ok(())
    }
    pub fn add_static_mesh(&mut self, vertices: &[DVec3], triangles: &[[u32; 3]]) -> Result<(), Error> {
        if vertices.iter().any(|p| !p.is_finite()) || triangles.iter().flatten().any(|i| *i as usize >= vertices.len())
        {
            return Err(Error::Geometry);
        }
        let mesh = ColliderBuilder::trimesh(vertices.iter().map(|p| vector(*p * UNIT)).collect(), triangles.to_vec())
            .map_err(|_| Error::Geometry)?;
        self.world.insert_collider(mesh.friction(0.8), None);
        Ok(())
    }
    pub fn prepare(&mut self) {
        self.world.detect_collisions(&self.contacts, &());
    }
    pub fn body_count(&self) -> usize {
        self.bindings.len()
    }
    pub fn set_damping(&mut self, id: BodyId, linear: f64, angular: f64) {
        if linear.is_finite()
            && angular.is_finite()
            && let Some(binding) = self.bindings.get(&id)
        {
            let body = &mut self.world.bodies[binding.handle];
            body.set_linear_damping(linear.clamp(0.0, 1000.0));
            body.set_angular_damping(angular.clamp(0.0, 1000.0));
        }
    }
    pub fn disable_collision_pair(&mut self, a: BodyId, b: BodyId) {
        let (a, b) = (u128::from(a) + 1, u128::from(b) + 1);
        self.contacts.0.insert((a.min(b), a.max(b)));
    }
    pub fn add_joint(&mut self, joint: &Joint) -> Result<(), Error> {
        let parent = self.bindings.get(&joint.parent).ok_or(Error::Properties)?.handle;
        let child = self.bindings.get(&joint.child).ok_or(Error::Properties)?.handle;
        if parent == child {
            return Err(Error::Properties);
        }
        let mut builder = GenericJointBuilder::new(JointAxesMask::LOCKED_SPHERICAL_AXES)
            .local_frame1(pose(joint.parent_frame).ok_or(Error::Properties)?)
            .local_frame2(pose(joint.child_frame).ok_or(Error::Properties)?);
        for (index, axis) in [JointAxis::AngX, JointAxis::AngY, JointAxis::AngZ].into_iter().enumerate() {
            let [min, max] = joint.limits[index];
            let friction = joint.friction[index];
            if !min.is_finite() || !max.is_finite() || min > max || !friction.is_finite() || friction < 0.0 {
                return Err(Error::Properties);
            }
            builder = builder.limits(axis, [min, max]);
            if friction > 0.0 {
                builder = builder.motor_velocity(axis, 0.0, 1.0).motor_max_force(axis, friction);
            }
        }
        self.world.impulse_joints.insert(parent, child, builder, true);
        Ok(())
    }
    pub fn transform(&self, id: BodyId) -> Option<Transform> {
        let body = self.world.bodies.get(self.bindings.get(&id)?.handle)?;
        let q = body.rotation();
        Some(Transform {
            position: from_vector(body.translation()) / UNIT,
            rotation: DQuat::from_xyzw(q.x, q.y, q.z, q.w),
        })
    }
    pub fn transforms(&self) -> impl Iterator<Item = (BodyId, Transform)> + '_ {
        self.bindings.keys().filter_map(|id| self.transform(*id).map(|t| (*id, t)))
    }
    pub fn velocity(&self, id: BodyId) -> Option<DVec3> {
        self.world.bodies.get(self.bindings.get(&id)?.handle).map(|b| from_vector(b.linvel()) / UNIT)
    }
    pub fn set_velocity(&mut self, id: BodyId, velocity: DVec3) {
        if velocity.is_finite()
            && let Some(binding) = self.bindings.get(&id)
        {
            self.world.bodies[binding.handle].set_linvel(vector(velocity * UNIT), true);
        }
    }
    pub fn advance(&mut self, dt: f64) -> usize {
        if self.paused || !dt.is_finite() || dt <= 0.0 {
            return 0;
        }
        self.settings = self.settings.clamped();
        self.accumulator += (dt.min(0.25) * self.settings.time_scale).min(1.0);
        let mut count = 0;
        while self.accumulator + 1e-12 >= STEP {
            self.step();
            self.accumulator -= STEP;
            count += 1;
        }
        count
    }
    pub fn step(&mut self) {
        self.settings = self.settings.clamped();
        self.world.gravity = Vector::new(0.0, 0.0, -self.settings.gravity * UNIT);
        let density = self.settings.air_density;
        for binding in self.bindings.values() {
            let body = &mut self.world.bodies[binding.handle];
            body.reset_forces(false);
            body.reset_torques(false);
            let velocity = body.linvel();
            let speed = velocity.length();
            if density > 0.0 && speed > 1e-8 {
                let magnitude = (0.5 * density * binding.area * speed * speed).min(binding.mass * speed / STEP);
                body.add_force(-velocity * (magnitude / speed), false);
            }
            if !self.water.is_empty() {
                let q = body.rotation();
                let matrix = glam::DMat4::from_rotation_translation(
                    DQuat::from_xyzw(q.x, q.y, q.z, q.w),
                    from_vector(body.translation()) / UNIT,
                );
                let mut immersed = Vec::new();
                let mut total_volume = 0.0;
                for hull in &binding.hulls {
                    let hull = hull.transformed(matrix);
                    let mut lo = DVec3::INFINITY;
                    let mut hi = DVec3::NEG_INFINITY;
                    for point in hull.faces.iter().flatten() {
                        lo = lo.min(*point);
                        hi = hi.max(*point);
                    }
                    for water in &self.water {
                        if !lo.cmple(water.bounds[1]).all() || !hi.cmpge(water.bounds[0]).all() {
                            continue;
                        }
                        let mut wet = hull.clone();
                        for (normal, distance) in &water.planes {
                            wet = wet.clipped(*normal, *distance);
                        }
                        if let Some((volume, center)) = wet.volume_center() {
                            immersed.push((volume, center, water.density));
                            total_volume += volume;
                        }
                    }
                }
                let weight = if total_volume > binding.volume { binding.volume / total_volume } else { 1.0 };
                for (volume, center, density) in immersed {
                    let mass = volume * weight * UNIT.powi(3) * density;
                    let buoyancy = Vector::new(0.0, 0.0, mass * self.settings.gravity * UNIT);
                    let point = vector(center * UNIT);
                    body.add_force_at_point(buoyancy, point, true);
                    let velocity = body.velocity_at_point(point);
                    let speed = velocity.length();
                    if speed > 1e-8 {
                        let fraction = (volume * weight / binding.volume).clamp(0.0, 1.0);
                        let direction = velocity / speed;
                        let arm = (point - body.center_of_mass()).cross(direction);
                        let inverse_mass =
                            1.0 / binding.mass + arm.dot(body.mass_properties().effective_world_inv_inertia * arm);
                        let drag = (0.5 * density * binding.area * fraction * speed * speed
                            + 2.0 * binding.mass * fraction * speed)
                            .min(speed / (inverse_mass * STEP) * fraction);
                        body.add_force_at_point(-velocity * (drag / speed), point, false);
                    }
                }
                if total_volume > 0.0 && binding.volume > 0.0 {
                    let damping = (-6.0 * STEP * (total_volume / binding.volume).min(1.0)).exp();
                    body.set_angvel(body.angvel() * damping, false);
                }
            }
        }
        self.world.step_with_events(&self.contacts, &());
    }
    pub fn ray(&self, origin: DVec3, direction: DVec3, max_distance: f64) -> Option<(BodyId, DVec3)> {
        let hit = self.ray_hit(origin, direction, max_distance)?;
        Some((hit.body?, hit.point))
    }
    pub fn ray_hit(&self, origin: DVec3, direction: DVec3, max_distance: f64) -> Option<RayHit> {
        let direction = direction.try_normalize()?;
        if !origin.is_finite() || !max_distance.is_finite() || max_distance <= 0.0 {
            return None;
        }
        let ray = Ray::new(vector(origin * UNIT), vector(direction));
        let (collider, distance) = self.world.cast_ray(&ray, max_distance * UNIT, true, QueryFilter::default())?;
        let body = self.world.colliders[collider].parent().and_then(|parent| self.reverse.get(&parent).copied());
        Some(RayHit { body, point: origin + direction * (distance / UNIT) })
    }
    pub fn shoot(&mut self, origin: DVec3, direction: DVec3, strength: f64) -> Option<BodyId> {
        let (id, point) = self.ray(origin, direction, 65536.0)?;
        if !strength.is_finite() || strength <= 0.0 {
            return None;
        }
        let binding = &self.bindings[&id];
        let impulse = direction.normalize() * (strength * binding.mass * UNIT);
        self.world.bodies[binding.handle].apply_impulse_at_point(vector(impulse), vector(point * UNIT), true);
        Some(id)
    }
    pub fn explode(&mut self, origin: DVec3, radius: f64, strength: f64) {
        if !origin.is_finite() || !radius.is_finite() || radius <= 0.0 || !strength.is_finite() || strength <= 0.0 {
            return;
        }
        for binding in self.bindings.values() {
            let body = &mut self.world.bodies[binding.handle];
            let delta = from_vector(body.center_of_mass()) / UNIT - origin;
            let distance = delta.length();
            if distance >= radius {
                continue;
            }
            let direction = if distance > 1e-6 { delta / distance } else { DVec3::Z };
            body.apply_impulse(vector(direction * (strength * (1.0 - distance / radius) * binding.mass * UNIT)), true);
        }
    }
    pub fn begin_drag(&mut self, id: BodyId, point: DVec3) -> bool {
        self.end_drag();
        if !point.is_finite() {
            return false;
        }
        let Some(binding) = self.bindings.get(&id) else {
            return false;
        };
        let target = binding.handle;
        let local = self.world.bodies[target].position().inverse_transform_point(vector(point * UNIT));
        let anchor =
            self.world.insert_body(RigidBodyBuilder::kinematic_position_based().translation(vector(point * UNIT)));
        let joint = self.world.impulse_joints.insert(
            anchor,
            target,
            SphericalJointBuilder::new().local_anchor1(Vector::ZERO).local_anchor2(local),
            true,
        );
        self.drag = Some(Drag { anchor, joint, target });
        true
    }
    pub fn drag_to(&mut self, point: DVec3) {
        if point.is_finite()
            && let Some(drag) = &self.drag
        {
            self.world.bodies[drag.anchor].set_next_kinematic_translation(vector(point * UNIT));
            self.world.bodies[drag.target].wake_up(true);
        }
    }
    pub fn end_drag(&mut self) {
        if let Some(drag) = self.drag.take() {
            self.world.impulse_joints.remove(drag.joint, true);
            self.world.remove_body(drag.anchor);
            self.world.wake_up(drag.target, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ragdoll_joints_keep_anchors_together_and_respect_angular_limits() {
        let mut sim = Simulation::default();
        floor(&mut sim);
        sim.add_body(&cube(1, DVec3::Z * 80.0)).unwrap();
        sim.add_body(&cube(2, DVec3::Z * 120.0)).unwrap();
        sim.disable_collision_pair(1, 2);
        sim.add_joint(&Joint {
            parent: 1,
            child: 2,
            parent_frame: Transform { position: DVec3::Z * 20.0, ..Default::default() },
            child_frame: Transform { position: -DVec3::Z * 20.0, ..Default::default() },
            limits: [[-0.2, 0.2], [0.0, 0.0], [0.0, 0.0]],
            friction: [0.0; 3],
        })
        .unwrap();
        sim.set_velocity(2, DVec3::X * 300.0);
        sim.prepare();
        for _ in 0..1200 {
            sim.step();
        }
        let a = sim.transform(1).unwrap();
        let b = sim.transform(2).unwrap();
        let anchor_a = a.position + a.rotation * (DVec3::Z * 20.0);
        let anchor_b = b.position + b.rotation * (-DVec3::Z * 20.0);
        assert!(anchor_a.distance(anchor_b) < 0.5, "{anchor_a:?} {anchor_b:?}");
        let relative = a.rotation.inverse() * b.rotation;
        assert!(relative.to_scaled_axis().length() < 0.23, "{relative:?}");
    }
    #[test]
    fn per_body_collision_rules_disable_only_the_requested_pair() {
        let mut sim = Simulation::new(Settings { gravity: 0.0, air_density: 0.0, time_scale: 1.0 });
        sim.add_body(&cube(1, DVec3::ZERO)).unwrap();
        sim.add_body(&cube(2, DVec3::ZERO)).unwrap();
        sim.disable_collision_pair(1, 2);
        sim.prepare();
        for _ in 0..120 {
            sim.step();
        }
        assert!(sim.transform(1).unwrap().position.length() < 1e-8);
        assert!(sim.transform(2).unwrap().position.length() < 1e-8);
        sim.add_body(&cube(3, DVec3::new(-100.0, 0.0, 0.0))).unwrap();
        sim.set_velocity(3, DVec3::X * 100.0);
        for _ in 0..240 {
            sim.step();
        }
        assert!(sim.transform(1).unwrap().position.x > 1.0 || sim.transform(2).unwrap().position.x > 1.0);
    }
    fn water(sim: &mut Simulation, min: DVec3, max: DVec3) {
        sim.add_water(WaterVolume {
            planes: vec![
                (DVec3::X, max.x),
                (-DVec3::X, -min.x),
                (DVec3::Y, max.y),
                (-DVec3::Y, -min.y),
                (DVec3::Z, max.z),
                (-DVec3::Z, -min.z),
            ],
            bounds: [min, max],
            density: 1000.0,
        })
        .unwrap();
    }
    #[test]
    fn water_buoyancy_uses_submerged_hull_volume_without_solid_surface() {
        let mut sim = Simulation::new(Settings { air_density: 0.0, ..Default::default() });
        water(&mut sim, DVec3::splat(-512.0), DVec3::new(512.0, 512.0, 0.0));
        let mut body = cube(1, -DVec3::Z * 100.0);
        body.mass = (32.0 * UNIT).powi(3) * 500.0;
        sim.add_body(&body).unwrap();
        sim.prepare();
        assert!(sim.ray_hit(DVec3::new(100.0, 0.0, 100.0), -DVec3::Z, 200.0).is_none());
        sim.step();
        assert!((sim.velocity(1).unwrap().z - 5.0).abs() < 0.01, "{:?}", sim.velocity(1));
        for _ in 0..3600 {
            sim.step();
        }
        let position = sim.transform(1).unwrap().position;
        assert!(position.z.abs() < 2.0, "{position:?}");
        assert!(sim.velocity(1).unwrap().length() < 2.0, "{:?}", sim.velocity(1));
    }
    #[test]
    fn dense_objects_sink_and_remote_water_has_no_effect() {
        let mut sim = Simulation::new(Settings { air_density: 0.0, ..Default::default() });
        water(&mut sim, DVec3::splat(-512.0), DVec3::new(512.0, 512.0, 0.0));
        let mut body = cube(1, -DVec3::Z * 100.0);
        body.mass = (32.0 * UNIT).powi(3) * 2000.0;
        sim.add_body(&body).unwrap();
        sim.add_body(&cube(2, DVec3::new(2000.0, 0.0, 100.0))).unwrap();
        sim.step();
        assert!((sim.velocity(1).unwrap().z + 2.5).abs() < 0.01);
        assert!((sim.velocity(2).unwrap().z + 5.0).abs() < 0.01);
    }
    fn cube(id: BodyId, position: DVec3) -> Body {
        let points = (0..8)
            .map(|i| {
                DVec3::new(
                    if i & 1 == 0 { -16.0 } else { 16.0 },
                    if i & 2 == 0 { -16.0 } else { 16.0 },
                    if i & 4 == 0 { -16.0 } else { 16.0 },
                )
            })
            .collect();
        Body {
            id,
            transform: Transform { position, ..Default::default() },
            parts: vec![ConvexPart { points, transform: Transform::default() }],
            mass: 10.0,
            friction: 0.8,
            restitution: 0.0,
        }
    }
    fn floor(sim: &mut Simulation) {
        sim.add_static_mesh(
            &[
                DVec3::new(-1024.0, -1024.0, 0.0),
                DVec3::new(1024.0, -1024.0, 0.0),
                DVec3::new(1024.0, 1024.0, 0.0),
                DVec3::new(-1024.0, 1024.0, 0.0),
            ],
            &[[0, 1, 2], [0, 2, 3]],
        )
        .unwrap();
    }
    #[test]
    fn bodies_fall_rotate_and_settle_on_real_triangle_geometry() {
        let mut sim = Simulation::default();
        floor(&mut sim);
        let mut body = cube(1, DVec3::new(0.0, 0.0, 200.0));
        body.transform.rotation = DQuat::from_rotation_y(0.3);
        sim.add_body(&body).unwrap();
        sim.prepare();
        sim.paused = false;
        for _ in 0..600 {
            sim.advance(1.0 / 60.0);
        }
        let pose = sim.transform(1).unwrap();
        assert!(pose.position.z > 15.0 && pose.position.z < 17.0, "{pose:?}");
        assert!(pose.rotation.angle_between(body.transform.rotation) > 0.1);
        assert!(sim.velocity(1).unwrap().length() < 1.0);
    }
    #[test]
    fn pause_and_slow_motion_use_fixed_time_steps() {
        let mut sim = Simulation::new(Settings { gravity: 600.0, air_density: 0.0, time_scale: 0.5 });
        sim.add_body(&cube(1, DVec3::Z * 200.0)).unwrap();
        assert_eq!(sim.advance(1.0), 0);
        assert_eq!(sim.transform(1).unwrap().position.z, 200.0);
        sim.paused = false;
        for _ in 0..60 {
            sim.advance(1.0 / 60.0);
        }
        let z = sim.transform(1).unwrap().position.z;
        assert!((z - 125.0).abs() < 2.0, "{z}");
        sim.paused = true;
        let before = sim.transform(1).unwrap();
        sim.advance(0.2);
        assert_eq!(sim.transform(1).unwrap(), before);
        sim.step();
        assert!(sim.transform(1).unwrap().position.z < before.position.z);
    }
    #[test]
    fn bullets_and_explosions_apply_impulses_without_non_finite_states() {
        let mut sim = Simulation::new(Settings { gravity: 0.0, air_density: 0.0, time_scale: 1.0 });
        sim.add_body(&cube(1, DVec3::new(100.0, 0.0, 64.0))).unwrap();
        sim.prepare();
        assert_eq!(sim.shoot(DVec3::new(0.0, 0.0, 64.0), DVec3::X, 300.0), Some(1));
        sim.step();
        assert!(sim.velocity(1).unwrap().x > 290.0);
        let position = sim.transform(1).unwrap().position;
        sim.explode(position, 256.0, 100.0);
        sim.step();
        assert!(sim.velocity(1).unwrap().z > 90.0);
        assert!(sim.transform(1).unwrap().position.is_finite());
    }
    #[test]
    fn dragging_moves_the_picked_point_and_preserves_release_momentum() {
        let mut sim = Simulation::new(Settings { gravity: 0.0, air_density: 0.0, time_scale: 1.0 });
        sim.add_body(&cube(1, DVec3::Z * 64.0)).unwrap();
        sim.prepare();
        assert!(sim.begin_drag(1, DVec3::Z * 64.0));
        for frame in 1..=10 {
            sim.drag_to(DVec3::new(f64::from(frame) * 2.0, 0.0, 64.0));
            sim.step();
        }
        let before = sim.transform(1).unwrap();
        assert!(before.position.x > 15.0, "{before:?}");
        sim.end_drag();
        sim.step();
        assert!(sim.transform(1).unwrap().position.x > before.position.x);
        assert!(sim.velocity(1).unwrap().x > 100.0);
    }

    #[test]
    fn static_geometry_occludes_picking_and_shooting() {
        let mut sim = Simulation::default();
        sim.add_body(&cube(1, DVec3::new(100.0, 0.0, 64.0))).unwrap();
        sim.add_static_mesh(
            &[
                DVec3::new(50.0, -128.0, -128.0),
                DVec3::new(50.0, 128.0, -128.0),
                DVec3::new(50.0, 128.0, 128.0),
                DVec3::new(50.0, -128.0, 128.0),
            ],
            &[[0, 1, 2], [0, 2, 3]],
        )
        .unwrap();
        sim.prepare();
        let origin = DVec3::Z * 64.0;
        let hit = sim.ray_hit(origin, DVec3::X, 1024.0).unwrap();
        assert_eq!(hit.body, None);
        assert!((hit.point.x - 50.0).abs() < 1e-8);
        assert_eq!(sim.ray(origin, DVec3::X, 1024.0), None);
        assert_eq!(sim.shoot(origin, DVec3::X, 300.0), None);
        assert_eq!(sim.velocity(1).unwrap(), DVec3::ZERO);
    }

    #[test]
    fn continuous_collision_stops_fast_bodies_at_thin_surfaces() {
        let mut sim = Simulation::new(Settings { gravity: 0.0, air_density: 0.0, time_scale: 1.0 });
        floor(&mut sim);
        sim.add_body(&cube(1, DVec3::Z * 100.0)).unwrap();
        sim.set_velocity(1, -DVec3::Z * 20000.0);
        sim.prepare();
        for _ in 0..10 {
            sim.step();
            assert!(sim.transform(1).unwrap().position.z >= 15.0);
        }
    }

    #[test]
    fn invalid_live_settings_cannot_poison_body_poses() {
        let mut sim = Simulation::default();
        sim.add_body(&cube(1, DVec3::Z * 200.0)).unwrap();
        sim.paused = false;
        sim.settings = Settings { gravity: f64::NAN, air_density: f64::INFINITY, time_scale: f64::NAN };
        assert_eq!(sim.advance(1.0 / 60.0), 2);
        let transform = sim.transform(1).unwrap();
        assert!(transform.position.is_finite() && transform.rotation.is_finite());
        assert_eq!(sim.settings.gravity, Settings::default().gravity);
        assert_eq!(sim.settings.air_density, Settings::default().air_density);
    }

    #[test]
    fn compound_shapes_keep_concave_gaps_open_and_stack() {
        let mut sim = Simulation::default();
        floor(&mut sim);
        let mut arch = cube(1, DVec3::Z * 32.0);
        let mut left = arch.parts[0].clone();
        left.transform.position.x = -48.0;
        let mut right = left.clone();
        right.transform.position.x = 48.0;
        arch.parts = vec![left, right];
        sim.add_body(&arch).unwrap();
        sim.add_body(&cube(2, DVec3::new(48.0, 0.0, 120.0))).unwrap();
        sim.prepare();
        assert_eq!(sim.ray(DVec3::new(0.0, -128.0, 32.0), DVec3::Y, 256.0), None);
        for _ in 0..900 {
            sim.step();
        }
        let bottom = sim.transform(1).unwrap().position.z;
        let top = sim.transform(2).unwrap().position.z;
        assert!((bottom - 16.0).abs() < 1.0, "{bottom}");
        assert!((top - bottom - 32.0).abs() < 1.0, "{top} {bottom}");
    }
}
