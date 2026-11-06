use std::cell::RefCell;
use std::collections::HashMap;

use crate::environment::ModelHitbox;

use super::*;

type Key = (u32, String);
pub(super) type Cache = HashMap<Key, Option<Arc<[ModelHitbox]>>>;

pub(super) struct Frame<'a> {
    world: &'a dyn Environment,
    previous: &'a Cache,
    current: RefCell<Cache>,
}

impl<'a> Frame<'a> {
    pub(super) fn new(world: &'a dyn Environment, previous: &'a Cache) -> Self {
        Self { world, previous, current: RefCell::new(HashMap::new()) }
    }

    pub(super) fn finish(self) -> Cache {
        self.current.into_inner()
    }
}

impl Environment for Frame<'_> {
    fn model_hitboxes(&self, object: u32, set: &str) -> Option<Arc<[ModelHitbox]>> {
        let key = (object, set.to_owned());
        if let Some(value) = self.current.borrow().get(&key) {
            return value.clone();
        }
        let value = self.world.model_hitboxes(object, set);
        self.current.borrow_mut().insert(key, value.clone());
        value
    }

    fn previous_model_hitboxes(&self, object: u32, set: &str) -> Option<Arc<[ModelHitbox]>> {
        self.previous.get(&(object, set.to_owned())).cloned().flatten()
    }

    fn inside_model(&self, object: u32, position: V3) -> bool {
        self.world.inside_model(object, position)
    }
    fn trace_environment(
        &self,
        environment: crate::environment::RayEnvironment,
        start: V3,
        direction: V3,
        maximum: f32,
    ) -> Option<crate::environment::RayHit> {
        self.world.trace_environment(environment, start, direction, maximum)
    }
    fn trace(&self, start: V3, end: V3, radius: f32, brush_only: bool) -> Option<crate::environment::Trace> {
        self.world.trace(start, end, radius, brush_only)
    }
    fn wind(&self, position: V3) -> V3 {
        self.world.wind(position)
    }
    fn lighting(&self, position: V3) -> V3 {
        self.world.lighting(position)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Position {
    cp: i32,
    scale: f32,
    inside: i32,
    bias: V3,
    desired: i32,
    set: String,
}

impl Position {
    pub(super) fn control_point(&self) -> i32 {
        self.cp
    }
    pub(super) fn needs_collision(&self) -> bool {
        self.inside > 0
    }
}

pub(super) fn compile_position(op: &Operator) -> Option<Position> {
    op.function.eq_ignore_ascii_case("Position on Model Random").then(|| Position {
        cp: i(op, "control_point_number", 0).clamp(0, 63),
        scale: f(op, "model hitbox scale", 1.0),
        inside: i(op, "force to be inside model", 0).clamp(0, 128),
        bias: v(op, "direction bias", [0.0; 3]),
        desired: i(op, "desired hitbox", -1),
        set: op.params.str("hitbox set").unwrap_or("effects").to_owned(),
    })
}

fn object(points: &Points, cp: i32) -> Option<u32> {
    points.objects.get(cp as usize).copied().flatten()
}

fn point(hitbox: &ModelHitbox, relative: V3) -> V3 {
    let local: V3 = std::array::from_fn(|axis| lerp(hitbox.minimum[axis], hitbox.maximum[axis], relative[axis]));
    std::array::from_fn(|axis| {
        hitbox.transform[axis][0] * local[0]
            + hitbox.transform[axis][1] * local[1]
            + hitbox.transform[axis][2] * local[2]
            + hitbox.transform[axis][3]
    })
}

pub(super) fn initialize(
    op: &Position,
    particle: &mut Particle,
    points: &Points,
    world: &dyn Environment,
    rng: &mut Rng,
) {
    let boxes = object(points, op.cp).and_then(|object| world.model_hitboxes(object, &op.set));
    let Some(boxes) = boxes.filter(|boxes| !boxes.is_empty()) else {
        particle.pos = points.get(op.cp);
        particle.hitbox = 0;
        particle.hitbox_relative = [0.0; 3];
        return;
    };
    let object = object(points, op.cp).unwrap();
    let maximum = boxes.len() - 1;
    let (low, high) = if op.desired >= 0 {
        let index = (op.desired as usize).min(maximum);
        (index, index)
    } else {
        (0, maximum)
    };
    let attempts = if op.bias.iter().any(|value| value.abs() > 0.0001) { op.inside.max(5) } else { op.inside };
    let mut best = -1.0e20;
    for _ in 0..=attempts {
        let index = low + (rng.next() * (high - low + 1) as f32) as usize;
        let relative = rng.range3([1.0 - op.scale; 3], [op.scale; 3]);
        let position = point(&boxes[index], relative);
        let mut score = rng.next() * 72.0 + dot(sub(position, points.get(op.cp)), op.bias);
        if op.inside > 0 && world.inside_model(object, position) {
            score += 1000.0;
        }
        if score > best {
            best = score;
            particle.pos = position;
            particle.hitbox = index as i32;
            particle.hitbox_relative = relative;
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Lock {
    cp: i32,
    start: f32,
    end: f32,
    set: String,
}

impl Lock {
    pub(super) fn control_point(&self) -> i32 {
        self.cp
    }
}

pub(super) fn compile_lock(op: &Operator) -> Option<Lock> {
    op.function.eq_ignore_ascii_case("Movement Lock to Bone").then(|| Lock {
        cp: i(op, "control_point_number", 0).clamp(0, 63),
        start: f(op, "lifetime fade start", 0.0),
        end: f(op, "lifetime fade end", 0.0),
        set: op.params.str("hitbox set").unwrap_or("effects").to_owned(),
    })
}

pub(super) fn apply_lock(op: &Lock, particle: &mut Particle, points: &Points, dt: f32, world: &dyn Environment) {
    if dt <= 0.0 {
        return;
    }
    let age = particle.age;
    if op.end > 0.0 && age >= op.end {
        return;
    }
    let Some(object) = object(points, op.cp) else {
        return;
    };
    let Some(current) = world.model_hitboxes(object, &op.set) else {
        return;
    };
    let previous = world.previous_model_hitboxes(object, &op.set).unwrap_or_else(|| Arc::clone(&current));
    if current.len() != previous.len() {
        return;
    }
    let Some(index) = usize::try_from(particle.hitbox).ok().filter(|index| *index < current.len()) else {
        return;
    };
    let mut strength = age.min(dt) / dt;
    if age > op.start && op.end > op.start {
        strength *= 1.0 - (age - op.start) / (op.end - op.start);
    }
    let delta = scale(
        sub(point(&current[index], particle.hitbox_relative), point(&previous[index], particle.hitbox_relative)),
        strength,
    );
    particle.pos = add(particle.pos, delta);
    particle.previous_pos = add(particle.previous_pos, delta);
}
