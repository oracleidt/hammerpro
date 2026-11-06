#![forbid(unsafe_code)]

use glam::{DQuat, DVec3};
use rapier::prelude::*;
use std::collections::HashMap;

pub type BodyId = u64;
const UNIT: f64 = 0.0254;
pub const STEP: f64 = 1.0 / 120.0;

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
}
struct Drag {
    anchor: RigidBodyHandle,
    joint: ImpulseJointHandle,
    target: RigidBodyHandle,
}

pub struct Simulation {
    world: PhysicsWorld,
    bindings: HashMap<BodyId, Binding>,
    reverse: HashMap<RigidBodyHandle, BodyId>,
    drag: Option<Drag>,
    accumulator: f64,
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
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for part in &body.parts {
            let transform = pose(part.transform).ok_or(Error::Properties)?;
            if part.points.len() < 4 || part.points.iter().any(|p| !p.is_finite()) {
                return Err(Error::Geometry);
            }
            let points: Vec<_> = part.points.iter().map(|p| vector(*p * UNIT)).collect();
            let shape = SharedShape::convex_hull(&points).ok_or(Error::Geometry)?;
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
        let area = (size.x * size.y).max(size.x * size.z).max(size.y * size.z);
        let collider = ColliderBuilder::compound(pieces)
            .mass(body.mass)
            .friction(body.friction.max(0.0))
            .restitution(body.restitution.clamp(0.0, 1.0));
        let handle =
            self.world.insert_body(RigidBodyBuilder::dynamic().pose(transform).ccd_enabled(true).angular_damping(0.05));
        self.world.insert_collider(collider, Some(handle));
        self.bindings.insert(body.id, Binding { handle, area, mass: body.mass });
        self.reverse.insert(handle, body.id);
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
        self.world.detect_collisions(&(), &());
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
            let velocity = body.linvel();
            let speed = velocity.length();
            if density > 0.0 && speed > 1e-8 {
                let magnitude = (0.5 * density * binding.area * speed * speed).min(binding.mass * speed / STEP);
                body.add_force(-velocity * (magnitude / speed), false);
            }
        }
        self.world.step();
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
