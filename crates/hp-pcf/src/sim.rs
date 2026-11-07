use std::sync::Arc;

use crate::environment::{EmptyEnvironment, Environment};
use crate::model::{Operator, SystemDef};

mod alpha;
mod batch;
mod color;
mod common;
mod control;
mod cull;
mod emission;
mod forces;
mod model;
mod noise;
mod oscillation;
mod path;
mod radius;
mod ramp;
mod random;
mod rays;
mod remap;
mod scalar;
mod simd_random;
mod spatial;

pub const MAX_PREVIEW_PARTICLES: usize = 5000;
pub const MAX_CONTROL_POINTS: usize = 64;
const LOOP_PAUSE: f32 = 0.6;
const INITIAL_PREVIOUS_DT: f32 = 0.05;
const MAX_CHILD_DEPTH: usize = 4;
const MAX_SYSTEMS: usize = 16;
pub const MAX_EFFECT_PARTICLES: usize = MAX_PREVIEW_PARTICLES * MAX_SYSTEMS;

type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn mul(a: V3, b: V3) -> V3 {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: V3) -> f32 {
    dot(a, a).sqrt()
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: V3, b: V3, t: f32) -> V3 {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Debug)]
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        let value = random::sample(self.0);
        self.0 = self.0.wrapping_add(1);
        value
    }

    fn integer(&mut self, minimum: u32, maximum: u32) -> u32 {
        minimum + (self.next() * (maximum.saturating_sub(minimum) as f32 + 1.0)) as u32
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        lerp(a, b, self.next())
    }

    fn range_exp(&mut self, a: f32, b: f32, exp: f32) -> f32 {
        let r = self.next();
        let r = if exp >= 0.0 && (exp - 1.0).abs() > 1e-4 { r.powf(exp) } else { r };
        lerp(a, b, r)
    }

    fn range3(&mut self, a: V3, b: V3) -> V3 {
        [self.range(a[0], b[0]), self.range(a[1], b[1]), self.range(a[2], b[2])]
    }

    fn in_sphere(&mut self) -> (V3, f32) {
        for _ in 0..8 {
            let v = [self.range(-1.0, 1.0), self.range(-1.0, 1.0), self.range(-1.0, 1.0)];
            let l2 = dot(v, v);
            if l2 > 1e-4 && l2 <= 1.0 {
                return (v, l2.sqrt());
            }
        }
        ([0.0, 0.0, 1.0], 1.0)
    }

    fn dir(&mut self) -> V3 {
        let (v, l) = self.in_sphere();
        scale(v, 1.0 / l)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Points {
    pub pos: Vec<V3>,
    pub axes: [V3; 3],
    pub orientations: Vec<[V3; 3]>,
    pub parents: Vec<i32>,
    pub objects: Vec<Option<u32>>,
}

impl Points {
    /// Native frame of a secondary control point whose orientation has never been set.
    pub const UNSET_AXES: [V3; 3] = [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];

    pub fn at(origin: V3) -> Points {
        Points {
            pos: vec![origin],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            orientations: Vec::new(),
            parents: Vec::new(),
            objects: Vec::new(),
        }
    }

    pub fn with_angles(origin: V3, angles: V3) -> Points {
        let [p, y, r] = angles.map(|a| if a.is_finite() { a.to_radians() } else { 0.0 });
        let (sp, cp) = p.sin_cos();
        let (sy, cy) = y.sin_cos();
        let (sr, cr) = r.sin_cos();
        let forward = [cp * cy, cp * sy, -sp];
        let right = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
        let up = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
        Points {
            pos: vec![origin],
            axes: [forward, scale(right, -1.0), up],
            orientations: Vec::new(),
            parents: Vec::new(),
            objects: Vec::new(),
        }
    }

    /// Unattached effects created by the game's convenience API initialize CP 0 and CP 1.
    pub fn unparented(origin: V3) -> Points {
        Points { pos: vec![origin, origin], ..Self::at(origin) }
    }

    pub fn get(&self, i: i32) -> V3 {
        self.pos.get(i.clamp(0, MAX_CONTROL_POINTS as i32 - 1) as usize).copied().unwrap_or([0.0; 3])
    }

    fn local(&self, v: V3) -> V3 {
        add(add(scale(self.axes[0], v[0]), scale(self.axes[1], v[1])), scale(self.axes[2], v[2]))
    }

    pub fn orientation(&self, cp: i32) -> [V3; 3] {
        if cp == 0 {
            self.axes
        } else {
            usize::try_from(cp).ok().and_then(|i| self.orientations.get(i)).copied().unwrap_or(Self::UNSET_AXES)
        }
    }

    fn highest(&self) -> usize {
        self.pos.len().saturating_sub(1)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Emit {
    Continuous { rate: f32, start: f32, duration: f32, used: f32, cp: i32, axis: usize, parent: bool },
    Instant { min: u32, max: u32, start: f32 },
    Noise { noise: noise::Scalar, used: f32, start: f32, duration: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    PositionX,
    Lifetime,
    Radius,
    Rotation,
    RotationSpeed,
    Alpha,
    Alpha2,
    CreationTime,
    Sequence,
    Sequence2,
    TrailLength,
    Yaw,
    Other,
}

impl Field {
    fn from_index(i: i32) -> Field {
        match i {
            0 => Field::PositionX,
            1 => Field::Lifetime,
            3 => Field::Radius,
            4 => Field::Rotation,
            5 => Field::RotationSpeed,
            7 => Field::Alpha,
            16 => Field::Alpha2,
            8 => Field::CreationTime,
            9 => Field::Sequence,
            13 => Field::Sequence2,
            10 => Field::TrailLength,
            12 => Field::Yaw,
            _ => Field::Other,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Init {
    RayEnvironment(rays::Cull),
    Impact(spatial::Impact),
    Scalar(scalar::Remap),
    InheritVelocity {
        cp: i32,
        scale: f32,
    },
    Warp(control::Warp),
    Sphere {
        cp: i32,
        dmin: f32,
        dmax: f32,
        bias: V3,
        abs: [bool; 3],
        smin: f32,
        smax: f32,
        sexp: f32,
        lmin: V3,
        lmax: V3,
        spread: bool,
    },
    Box {
        cp: i32,
        min: V3,
        max: V3,
    },
    Offset {
        min: V3,
        max: V3,
        local: bool,
        radius: bool,
    },
    Ring {
        cp: i32,
        radius: f32,
        thickness: f32,
        smin: f32,
        smax: f32,
        even: bool,
    },
    Path {
        start: i32,
        end: i32,
        max_dist: f32,
        bulge: f32,
        bulge_control: i32,
        mid: f32,
        steps: Option<f32>,
        looping: bool,
    },
    Hierarchy(path::Hierarchy),
    Model(model::Position),
    FromParent {
        inherit: f32,
    },
    Lifetime {
        min: f32,
        max: f32,
        exp: f32,
    },
    LifeFromSequence {
        fps: f32,
    },
    Radius {
        min: f32,
        max: f32,
        exp: f32,
    },
    Color(color::Color),
    Alpha {
        min: f32,
        max: f32,
        exp: f32,
    },
    Rotation {
        initial: f32,
        min: f32,
        max: f32,
        exp: f32,
        flip: bool,
    },
    RotationSpeed {
        constant: f32,
        min: f32,
        max: f32,
        exp: f32,
        flip: bool,
    },
    Yaw {
        initial: f32,
        min: f32,
        max: f32,
        exp: f32,
    },
    YawFlip {
        chance: f32,
    },
    Sequence {
        second: bool,
        min: i32,
        max: i32,
    },
    TrailLength {
        min: f32,
        max: f32,
    },
    Velocity {
        cp: i32,
        smin: f32,
        smax: f32,
        lmin: V3,
        lmax: V3,
    },
    Noise(noise::Velocity),
    NoiseScalar {
        field: Field,
        noise: noise::Scalar,
    },
    PreAge(noise::Scalar),
    Remap {
        input: Field,
        imin: f32,
        imax: f32,
        output: Field,
        omin: f32,
        omax: f32,
        multiply: bool,
        within: bool,
    },
}

impl Init {
    fn modifier(&self) -> bool {
        match self {
            Self::Sphere { .. }
            | Self::Box { .. }
            | Self::Ring { .. }
            | Self::Path { .. }
            | Self::Model(_)
            | Self::Hierarchy(_)
            | Self::FromParent { .. }
            | Self::Lifetime { .. }
            | Self::Radius { .. }
            | Self::Color(_)
            | Self::Alpha { .. }
            | Self::Rotation { .. }
            | Self::RotationSpeed { .. }
            | Self::Yaw { .. }
            | Self::Sequence { .. }
            | Self::TrailLength { .. }
            | Self::NoiseScalar { .. } => false,
            Self::RayEnvironment(_)
            | Self::Impact(_)
            | Self::Scalar(_)
            | Self::InheritVelocity { .. }
            | Self::Warp(_)
            | Self::Offset { .. }
            | Self::LifeFromSequence { .. }
            | Self::YawFlip { .. }
            | Self::Velocity { .. }
            | Self::Noise(_)
            | Self::PreAge(_)
            | Self::Remap { .. } => true,
        }
    }

    fn written_attributes(&self) -> u32 {
        match self {
            Self::Model(_) => 0xc105,
            Self::Path { steps: Some(_), .. } => 0x8005,
            Self::FromParent { .. } => 7,
            Self::Sphere { .. } | Self::Box { .. } | Self::Ring { .. } | Self::Path { .. } | Self::Hierarchy(_) => 5,
            Self::Lifetime { .. } => 1 << 1,
            Self::Radius { .. } => 1 << 3,
            Self::Rotation { .. } => 1 << 4,
            Self::RotationSpeed { .. } => 1 << 5,
            Self::Color(_) => 1 << 6,
            Self::Alpha { .. } => 1 << 7,
            Self::Sequence { second, .. } => 1 << if *second { 13 } else { 9 },
            Self::TrailLength { .. } => 1 << 10,
            Self::Yaw { .. } => 1 << 12,
            Self::NoiseScalar { field, .. } => match field {
                Field::Lifetime => 1 << 1,
                Field::Radius => 1 << 3,
                Field::PositionX => 1 << 0,
                Field::Rotation => 1 << 4,
                Field::RotationSpeed => 1 << 5,
                Field::Alpha => 1 << 7,
                Field::Alpha2 => 1 << 16,
                Field::CreationTime => 1 << 8,
                Field::Sequence => 1 << 9,
                Field::TrailLength => 1 << 10,
                Field::Yaw => 1 << 12,
                Field::Sequence2 => 1 << 13,
                Field::Other => 0,
            },
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Op {
    Decay,
    FieldRemap(remap::Scalar),
    AlphaSimple(alpha::Simple),
    BoneLock(model::Lock),
    Scalar(scalar::Remap),
    Lock(control::Lock),
    Force(forces::Force),
    Spatial(spatial::SpatialOp),
    CullRandom { start: f32, end: f32, exponent: f32, percentage: f32 },
    Movement { gravity: V3, drag: f32 },
    AlphaRandom(alpha::Random),
    FadeAndDecay(alpha::FadeDecay),
    FadeTracer(alpha::FadeDecay),
    RadiusScale(radius::Scale),
    ColorFade { color: [f32; 3], t0: f32, t1: f32, ease: bool },
    RotationBasic,
    Spin { rate: f32, stop: f32, min: f32, yaw: bool },
    OscScalar { field: Field, wave: oscillation::Scalar },
    OscVector { field: i32, wave: oscillation::Vector },
    NoiseScalar { field: Field, min: f32, max: f32, speed: f32, additive: bool },
    NoiseVector { field: i32, min: V3, max: V3, speed: f32, additive: bool },
    Ramp { field: Field, ramp: ramp::Ramp },
    MaxVelocity { max: f32 },
    Constrain { cp: i32, min: f32, max: f32, offset: V3 },
    RotateAround { cp: i32, axis: V3, rate: f32 },
    Dampen { cp: i32, range: f32, factor: f32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderKind {
    #[default]
    None,
    Sprites,
    Trail,
    Rope,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Render {
    pub kind: RenderKind,
    pub orient: u8,
    pub rate: f32,
    pub second_rate: f32,
    pub fit: bool,
    pub fps: bool,
    pub min_len: f32,
    pub max_len: f32,
    pub len_fade_in: f32,
    pub constrain_radius: bool,
    pub ignore_dt: bool,
    /// View-dependent rotation modifier, applied before drawing any renderer.
    pub screen_velocity_angle: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub group_id: i32,
    pub name: String,
    pub material: String,
    emitters: Vec<Emit>,
    inits: Vec<Init>,
    ops: Vec<Op>,
    operator_schedule: Vec<common::Entry>,
    op_schedule: Vec<usize>,
    trace_schedule: Vec<usize>,
    child_schedule: Vec<usize>,
    requires_order_invariance: bool,
    child_points: Vec<control::ChildPoints>,
    point_traces: Vec<control::PointTrace>,
    max: usize,
    initial: usize,
    radius: f32,
    color: [f32; 3],
    alpha: f32,
    rotation: f32,
    rotation_speed: f32,
    sequence: u32,
    pub draws: bool,
    pub render: Render,
    pub children: Vec<(String, f32)>,
    pub unsupported: Vec<String>,
}

fn f(op: &Operator, name: &str, default: f32) -> f32 {
    op.float(name).unwrap_or(default)
}

fn v(op: &Operator, name: &str, default: V3) -> V3 {
    op.vec3(name).unwrap_or(default)
}

fn b(op: &Operator, name: &str, default: bool) -> bool {
    op.bool(name).unwrap_or(default)
}

fn i(op: &Operator, name: &str, default: i32) -> i32 {
    op.int(name).unwrap_or(default)
}

fn rgb(c: [u8; 4]) -> [f32; 3] {
    [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0]
}

fn vec_or(op: &Operator, name: &str, default: V3) -> V3 {
    op.vec3(name).or_else(|| op.float(name).map(|x| [x; 3])).unwrap_or(default)
}

fn bools3(op: &Operator, name: &str) -> [bool; 3] {
    op.vec3(name).map_or([false; 3], |x| x.map(|c| c != 0.0))
}

impl Plan {
    pub fn model_control_points(&self) -> Vec<i32> {
        let mut points: Vec<_> = self
            .inits
            .iter()
            .filter_map(|init| match init {
                Init::Model(op) => Some(op.control_point()),
                _ => None,
            })
            .chain(self.ops.iter().filter_map(|op| match op {
                Op::BoneLock(op) => Some(op.control_point()),
                _ => None,
            }))
            .collect();
        points.sort_unstable();
        points.dedup();
        points
    }

    pub fn uses_ray_environments(&self) -> bool {
        self.inits.iter().any(|init| matches!(init, Init::RayEnvironment(_)))
    }

    pub fn model_collision_control_points(&self) -> Vec<i32> {
        self.inits
            .iter()
            .filter_map(|init| match init {
                Init::Model(op) if op.needs_collision() => Some(op.control_point()),
                _ => None,
            })
            .collect()
    }

    pub fn trace_requirement(&self) -> Option<bool> {
        if !self.point_traces.is_empty() || self.inits.iter().any(|op| matches!(op, Init::Impact(_))) {
            return Some(true);
        }
        self.ops
            .iter()
            .filter_map(|op| match op {
                Op::Spatial(spatial::SpatialOp::Collision { brush_only, .. }) => Some(!brush_only),
                _ => None,
            })
            .max()
    }

    pub fn new(def: &SystemDef) -> Plan {
        let mut p = Plan {
            group_id: def.group_id,
            name: def.name.clone(),
            material: def.material.clone(),
            emitters: Vec::new(),
            inits: Vec::new(),
            ops: Vec::new(),
            operator_schedule: Vec::new(),
            op_schedule: Vec::new(),
            trace_schedule: Vec::new(),
            child_schedule: Vec::new(),
            requires_order_invariance: def.renderers.iter().any(|op| op.function.eq_ignore_ascii_case("render_rope")),
            child_points: Vec::new(),
            point_traces: Vec::new(),
            max: (def.max_particles as usize).min(MAX_PREVIEW_PARTICLES),
            initial: (def.initial_particles as usize).min(MAX_PREVIEW_PARTICLES),
            radius: def.radius,
            color: rgb(def.color),
            alpha: f32::from(def.color[3]) / 255.0,
            rotation: 0.0,
            rotation_speed: 0.0,
            sequence: 0,
            draws: false,
            render: Render::default(),
            children: def.children.iter().map(|c| (c.system.clone(), c.delay)).collect(),
            unsupported: Vec::new(),
        };
        p.compile_renderers(def);
        for op in &def.emitters {
            p.compile_emitter(op);
        }
        for op in &def.initializers {
            p.compile_init(op);
        }
        let mut modifiers = Vec::new();
        // Source initializes each base attribute once, then runs modifiers in their authored order.
        // Emitters have already assigned creation time and the particle identifier.
        let mut initialized = (1u32 << 8) | (1 << 11);
        p.inits.retain(|init| {
            if init.modifier() {
                modifiers.push(init.clone());
                return false;
            }
            let written = init.written_attributes();
            if written != 0 && written & !initialized == 0 {
                return false;
            }
            initialized |= written;
            true
        });
        p.inits.extend(modifiers);
        for (category, operators) in [&def.forces, &def.operators, &def.constraints].into_iter().enumerate() {
            for op in operators {
                let schedule = p.operator_schedule.len();
                p.operator_schedule.push(common::Entry::new(op, category, oscillation::before_emitters(&op.function)));
                let first = p.ops.len();
                let traces = p.point_traces.len();
                let children = p.child_points.len();
                p.compile_op(op);
                p.op_schedule.extend(std::iter::repeat_n(schedule, p.ops.len() - first));
                p.trace_schedule.extend(std::iter::repeat_n(schedule, p.point_traces.len() - traces));
                p.child_schedule.extend(std::iter::repeat_n(schedule, p.child_points.len() - children));
            }
        }
        p
    }

    fn compile_renderers(&mut self, def: &SystemDef) {
        for op in &def.renderers {
            let name = op.function.to_ascii_lowercase();
            if name == "render_screen_velocity_rotate" {
                self.render.screen_velocity_angle =
                    Some(crate::screen_velocity::forward_radians(f(op, "forward_angle", -90.0)));
                continue;
            }
            let kind = match name.as_str() {
                "render_animated_sprites" => RenderKind::Sprites,
                "render_sprite_trail" => RenderKind::Trail,
                "render_rope" => RenderKind::Rope,
                _ if name.contains("sprite") => RenderKind::Sprites,
                _ => {
                    self.unsupported.push(op.function.clone());
                    continue;
                }
            };
            if self.render.kind != RenderKind::None {
                continue;
            }
            self.render = Render {
                kind,
                orient: i(op, "orientation_type", 0).clamp(0, 3) as u8,
                rate: f(op, "animation rate", 0.1).clamp(0.0, 1000.0),
                second_rate: f(op, "second sequence animation rate", 0.0).clamp(0.0, 1000.0),
                fit: b(op, "animation_fit_lifetime", false),
                fps: b(op, "use animation rate as FPS", false),
                min_len: f(op, "min length", 0.0),
                max_len: f(op, "max length", 2000.0),
                len_fade_in: f(op, "length fade in time", 0.0),
                constrain_radius: b(op, "constrain radius to length", true),
                ignore_dt: b(op, "ignore delta time", false),
                screen_velocity_angle: self.render.screen_velocity_angle,
            };
        }
        self.draws = self.render.kind != RenderKind::None;
    }

    fn compile_emitter(&mut self, op: &Operator) {
        let start = f(op, "emission_start_time", 0.0);
        let duration = f(op, "emission_duration", 0.0).max(0.0);
        match op.function.to_ascii_lowercase().as_str() {
            "emit_continuously" => self.emitters.push(Emit::Continuous {
                rate: f(op, "emission_rate", 100.0).max(0.0),
                start,
                duration,
                used: f(op, "scale emission to used control points", 0.0),
                cp: i(op, "emission count scale control point", -1).clamp(-1, 63),
                axis: i(op, "emission count scale control point field", 0).clamp(0, 2) as usize,
                parent: b(op, "use parent particles for emission scaling", false),
            }),
            "emit_instantaneously" => {
                let max = i(op, "num_to_emit", 100).clamp(0, 100_000) as u32;
                let min = op.int("num_to_emit_minimum").map_or(max, |m| (m.clamp(0, 100_000) as u32).min(max));
                self.emitters.push(Emit::Instant { min, max, start });
            }
            "emit noise" => self.emitters.push(Emit::Noise {
                noise: noise::Scalar::emitter(op),
                used: f(op, "scale emission to used control points", 0.0),
                start,
                duration,
            }),
            _ => self.unsupported.push(op.function.clone()),
        }
    }

    fn compile_init(&mut self, op: &Operator) {
        if let Some(cull) = rays::compile(op) {
            self.inits.push(Init::RayEnvironment(cull));
            return;
        }
        if let Some(color) = color::compile(op) {
            self.inits.push(Init::Color(color));
            return;
        }
        let init = match op.function.to_ascii_lowercase().as_str() {
            "position within sphere random" => Init::Sphere {
                cp: i(op, "control_point_number", 0),
                dmin: f(op, "distance_min", 0.0),
                dmax: f(op, "distance_max", 0.0),
                bias: v(op, "distance_bias", [1.0; 3]),
                abs: bools3(op, "distance_bias_absolute_value"),
                smin: f(op, "speed_min", 0.0),
                smax: f(op, "speed_max", 0.0),
                sexp: f(op, "speed_random_exponent", 1.0),
                lmin: v(op, "speed_in_local_coordinate_system_min", [0.0; 3]),
                lmax: v(op, "speed_in_local_coordinate_system_max", [0.0; 3]),
                spread: b(op, "randomly distribute to highest supplied Control Point", false),
            },
            "position within box random" => Init::Box {
                cp: i(op, "control point number", i(op, "control_point_number", 0)),
                min: v(op, "min", [0.0; 3]),
                max: v(op, "max", [0.0; 3]),
            },
            "position modify offset random" => Init::Offset {
                min: v(op, "offset min", [0.0; 3]),
                max: v(op, "offset max", [0.0; 3]),
                local: b(op, "offset in local space 0/1", false),
                radius: b(op, "offset proportional to radius 0/1", false),
            },
            "position along ring" => Init::Ring {
                cp: i(op, "control point number", 0),
                radius: f(op, "initial radius", 0.0),
                thickness: f(op, "thickness", 0.0),
                smin: f(op, "min initial speed", 0.0),
                smax: f(op, "max initial speed", 0.0),
                even: b(op, "even distribution", false),
            },
            "position along path random" | "position along path sequential" => Init::Path {
                start: i(op, "start control point number", 0),
                end: i(op, "end control point number", 0),
                max_dist: f(op, "maximum distance", 0.0),
                bulge: f(op, "bulge", 0.0),
                bulge_control: i(op, "bulge control 0=random 1=orientation of start pnt 2=orientation of end point", 0),
                mid: f(op, "mid point position", 0.5),
                steps: if op.function.eq_ignore_ascii_case("position along path sequential") {
                    Some(f(op, "particles to map from start to end", 100.0))
                } else {
                    None
                },
                looping: b(op, "restart behavior (0 = bounce, 1 = loop )", true),
            },
            "position on model random" => Init::Model(model::compile_position(op).unwrap()),
            "position in cp hierarchy" => Init::Hierarchy(path::Hierarchy::compile(op)),
            "position from parent particles" | "position from parent cache" => {
                Init::FromParent { inherit: f(op, "Inherited Velocity Scale", 0.0) }
            }
            "lifetime random" => Init::Lifetime {
                min: f(op, "lifetime_min", 0.0),
                max: f(op, "lifetime_max", 0.0),
                exp: f(op, "lifetime_random_exponent", 1.0),
            },
            "lifetime from sequence" => {
                Init::LifeFromSequence { fps: f(op, "Frames Per Second", 30.0).clamp(0.01, 1000.0) }
            }
            "radius random" => Init::Radius {
                min: f(op, "radius_min", 1.0).clamp(0.0, 100_000.0),
                max: f(op, "radius_max", 1.0).clamp(0.0, 100_000.0),
                exp: f(op, "radius_random_exponent", 1.0),
            },

            "alpha random" => Init::Alpha {
                min: f(op, "alpha_min", 255.0) * (1.0 / 255.0),
                max: f(op, "alpha_max", 255.0) * (1.0 / 255.0),
                exp: f(op, "alpha_random_exponent", 1.0),
            },
            "rotation random" => Init::Rotation {
                initial: f(op, "rotation_initial", 0.0).to_radians(),
                min: f(op, "rotation_offset_min", 0.0).to_radians(),
                max: f(op, "rotation_offset_max", 360.0).to_radians(),
                exp: f(op, "rotation_random_exponent", 1.0),
                flip: b(op, "randomly_flip_direction", false),
            },
            "rotation speed random" => Init::RotationSpeed {
                constant: f(op, "rotation_speed_constant", 0.0).to_radians(),
                min: f(op, "rotation_speed_random_min", 0.0).to_radians(),
                max: f(op, "rotation_speed_random_max", 360.0).to_radians(),
                exp: f(op, "rotation_speed_random_exponent", 1.0),
                flip: b(op, "randomly_flip_direction", false),
            },
            "rotation yaw random" => Init::Yaw {
                initial: f(op, "yaw_initial", 0.0).to_radians(),
                min: f(op, "yaw_offset_min", 0.0).to_radians(),
                max: f(op, "yaw_offset_max", 360.0).to_radians(),
                exp: f(op, "yaw_random_exponent", 1.0),
            },
            "rotation yaw flip random" => Init::YawFlip { chance: f(op, "Flip Percentage", 0.5).clamp(0.0, 1.0) },
            "sequence random" | "sequence two random" => Init::Sequence {
                second: op.function.eq_ignore_ascii_case("Sequence Two Random"),
                min: i(op, "sequence_min", 0).max(0),
                max: i(op, "sequence_max", 0).max(0),
            },
            "trail length random" => Init::TrailLength {
                min: f(op, "length_min", 0.1).clamp(0.0, 100.0),
                max: f(op, "length_max", 0.1).clamp(0.0, 100.0),
            },
            "velocity random" => Init::Velocity {
                cp: i(op, "control_point_number", 0).clamp(0, 63),
                smin: f(op, "random_speed_min", f(op, "speed_min", 0.0)),
                smax: f(op, "random_speed_max", f(op, "speed_max", 0.0)),
                lmin: v(op, "speed_in_local_coordinate_system_min", [0.0; 3]),
                lmax: v(op, "speed_in_local_coordinate_system_max", [0.0; 3]),
            },
            "velocity inherit from control point" => {
                Init::InheritVelocity { cp: i(op, "control point number", 0), scale: f(op, "velocity scale", 1.0) }
            }
            "velocity noise" | "initial velocity noise" => Init::Noise(noise::Velocity::compile(op)),
            "remap noise to scalar" => {
                let field = Field::from_index(i(op, "output field", 3));
                Init::NoiseScalar { field, noise: noise::Scalar::compile(op, field, false) }
            }
            "lifetime pre-age noise" => Init::PreAge(noise::Scalar::compile(op, Field::Other, true)),
            "remap initial scalar" => Init::Remap {
                input: Field::from_index(i(op, "input field", 8)),
                imin: f(op, "input minimum", 0.0),
                imax: f(op, "input maximum", 1.0),
                output: Field::from_index(i(op, "output field", 3)),
                omin: f(op, "output minimum", 0.0),
                omax: f(op, "output maximum", 1.0),
                multiply: b(op, "output is scalar of initial random range", false),
                within: b(op, "only active within specified input range", false),
            },
            _ => {
                if let Some(impact) = spatial::compile_impact(op) {
                    self.inits.push(Init::Impact(impact));
                    return;
                }
                if let Some(remap) = scalar::compile(op) {
                    self.inits.push(Init::Scalar(remap));
                    return;
                }
                if let Some(warp) = control::compile_warp(op) {
                    self.inits.push(Init::Warp(warp));
                    return;
                }
                self.unsupported.push(op.function.clone());
                return;
            }
        };
        self.inits.push(init);
    }

    fn compile_op(&mut self, op: &Operator) {
        let name = op.function.to_ascii_lowercase();
        let compiled = match name.as_str() {
            "remap scalar" => {
                let Some(remap) = remap::Scalar::compile(op) else {
                    self.unsupported.push(op.function.clone());
                    return;
                };
                Op::FieldRemap(remap)
            }
            "cull random" => Op::CullRandom {
                start: f(op, "Cull Start Time", 0.0),
                end: f(op, "Cull End Time", 1.0),
                exponent: f(op, "Cull Time Exponent", 1.0),
                percentage: f(op, "Cull Percentage", 0.5),
            },
            "movement basic" => {
                Op::Movement { gravity: v(op, "gravity", [0.0; 3]), drag: f(op, "drag", 0.0).clamp(0.0, 1.0) }
            }
            "lifespan decay" => Op::Decay,
            "alpha fade in random" => Op::AlphaRandom(alpha::Random::compile(op, false)),
            "alpha fade out random" => Op::AlphaRandom(alpha::Random::compile(op, true)),
            "alpha fade in simple" => {
                Op::AlphaSimple(alpha::Simple { out: false, span: f(op, "proportional fade in time", 0.25) })
            }
            "alpha fade out simple" => {
                Op::AlphaSimple(alpha::Simple { out: true, span: f(op, "proportional fade out time", 0.25) })
            }
            "alpha fade and decay" => Op::FadeAndDecay(alpha::FadeDecay::compile(op)),
            "alpha fade and decay for tracers" => Op::FadeTracer(alpha::FadeDecay::compile(op)),
            "radius scale" => Op::RadiusScale(radius::Scale::compile(op)),
            "color fade" => Op::ColorFade {
                color: rgb(op.color("color_fade").unwrap_or([255; 4])),
                t0: f(op, "fade_start_time", 0.0),
                t1: f(op, "fade_end_time", 1.0),
                ease: b(op, "ease_in_and_out", true),
            },
            "rotation basic" => Op::RotationBasic,
            "rotation spin roll" | "rotation spin yaw" => {
                let yaw = name == "rotation spin yaw";
                let keys = if yaw {
                    ["yaw_rate_degrees", "yaw_stop_time", "yaw_rate_min"]
                } else {
                    ["spin_rate_degrees", "spin_stop_time", "spin_rate_min"]
                };
                Op::Spin {
                    rate: f(op, keys[0], 0.0).to_radians(),
                    stop: f(op, keys[1], 0.0).max(0.0),
                    min: f(op, keys[2], 0.0).to_radians(),
                    yaw,
                }
            }
            "oscillate scalar" => Op::OscScalar {
                field: Field::from_index(i(op, "oscillation field", 7)),
                wave: oscillation::Scalar::compile(op),
            },
            "oscillate vector" => {
                Op::OscVector { field: i(op, "oscillation field", 0), wave: oscillation::Vector::compile(op) }
            }
            "noise scalar" => {
                let field = Field::from_index(i(op, "output field", 3));
                let angle = |value: f32| {
                    if matches!(field, Field::Rotation | Field::Yaw) {
                        (value as f64 * (std::f64::consts::PI / 180.0)) as f32
                    } else {
                        value
                    }
                };
                Op::NoiseScalar {
                    field,
                    min: angle(f(op, "output minimum", 0.0)),
                    max: angle(f(op, "output maximum", 1.0)),
                    speed: f(op, "noise coordinate scale", 0.1),
                    additive: b(op, "additive", false),
                }
            }
            "noise vector" => Op::NoiseVector {
                field: i(op, "output field", 6),
                min: v(op, "output minimum", [0.0; 3]),
                max: v(op, "output maximum", [1.0; 3]),
                speed: f(op, "noise coordinate scale", 0.1),
                additive: b(op, "additive", false),
            },
            "ramp scalar linear random"
            | "ramp scalar spline random"
            | "ramp scalar linear simple"
            | "ramp scalar spline simple" => Op::Ramp {
                field: Field::from_index(i(op, "ramp field", 3).clamp(0, 21)),
                ramp: ramp::Ramp::compile(op),
            },
            "movement max velocity" => Op::MaxVelocity { max: f(op, "maximum velocity", 0.0).max(0.0) },
            "constrain distance to control point" => Op::Constrain {
                cp: i(op, "control point number", 0),
                min: f(op, "minimum distance", 0.0).max(0.0),
                max: f(op, "maximum distance", 100.0).max(0.0),
                offset: v(op, "offset of center", [0.0; 3]),
            },
            "movement rotate particle around axis" => Op::RotateAround {
                cp: i(op, "Control Point", 0),
                axis: v(op, "Rotation Axis", [0.0, 0.0, 1.0]),
                rate: f(op, "Rotation Rate", 180.0).to_radians(),
            },
            "movement dampen relative to control point" => Op::Dampen {
                cp: i(op, "control_point_number", 0),
                range: f(op, "falloff range", 100.0).max(0.0),
                factor: f(op, "dampen scale", 1.0).clamp(0.0, 1.0),
            },
            _ => {
                if let Some(trace) = control::compile_point_trace(op) {
                    self.point_traces.push(trace);
                    return;
                }
                if let Some(child) = control::compile_child_points(op) {
                    self.child_points.push(child);
                    return;
                }
                if let Some(remap) = scalar::compile(op) {
                    self.ops.push(Op::Scalar(remap));
                    return;
                }
                if let Some(lock) = model::compile_lock(op) {
                    self.ops.push(Op::BoneLock(lock));
                    return;
                }
                if let Some(lock) = control::compile_lock(op) {
                    self.ops.push(Op::Lock(lock));
                    return;
                }
                if let Some(compiled) = forces::compile(op) {
                    self.ops.push(Op::Force(compiled));
                    return;
                }
                if let Some(compiled) = spatial::compile(op) {
                    self.ops.push(Op::Spatial(compiled));
                    return;
                }
                self.unsupported.push(op.function.clone());
                return;
            }
        };
        self.ops.push(compiled);
    }

    fn finite(&self) -> bool {
        self.emitters.iter().all(|e| match e {
            Emit::Continuous { duration, .. } | Emit::Noise { duration, .. } => *duration > 0.0,
            Emit::Instant { .. } => true,
        })
    }

    fn emission_end(&self) -> Option<f32> {
        let mut end = 0.0f32;
        for e in &self.emitters {
            match e {
                Emit::Continuous { start, duration, .. } | Emit::Noise { start, duration, .. } if *duration > 0.0 => {
                    end = end.max(start + duration);
                }
                Emit::Continuous { .. } | Emit::Noise { .. } => return None,
                Emit::Instant { start, .. } => end = end.max(*start),
            }
        }
        Some(end)
    }

    pub fn continuous(&self) -> bool {
        !self.emitters.is_empty() && self.emission_end().is_none()
    }

    pub fn cycle(&self, p: &Particle, total: f32) -> f32 {
        self.sequence_cycle(p, total, false)
    }

    pub fn sequence_cycle(&self, p: &Particle, total: f32, second: bool) -> f32 {
        let r = &self.render;
        if r.fit && r.kind != RenderKind::Sprites {
            let life = if p.life > 0.0 { p.life } else { 1.0 };
            return p.age / life;
        }
        let rate = if second { r.second_rate } else { r.rate };
        if r.fps {
            return if total > 0.0 { p.age * rate / total } else { 0.0 };
        }
        p.age * rate
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    dead: bool,
    previous_pos: V3,
    previous_dt: f32,
    velocity_snapshot: V3,
    initial_values: [f32; Field::Other as usize + 1],
    hitbox: i32,
    hitbox_relative: V3,
    pub pos: V3,
    pub vel: V3,
    pub age: f32,
    pub life: f32,
    pub radius: f32,
    pub color: [f32; 3],
    pub alpha: f32,
    pub alpha2: f32,
    pub rotation: f32,
    pub sequence: u32,
    pub sequence2: u32,
    sequence_values: [f32; 2],
    pub trail: f32,
    pub flip: bool,
    pub seed: u32,
    base_color: [f32; 3],
    base_alpha: f32,
    rotation_speed: f32,
    pub yaw: f32,
    alpha_add: f32,
    born: f32,
}

impl Particle {
    /// The actual previous-position attribute, including control-point and
    /// collision changes. Preserve compatibility with callers editing velocity.
    pub fn previous_position(&self) -> V3 {
        if self.vel != self.velocity_snapshot {
            sub(self.pos, scale(self.vel, self.previous_dt))
        } else {
            self.previous_pos
        }
    }

    fn sequence_value(&self, second: bool) -> f32 {
        let value = self.sequence_values[usize::from(second)];
        let public = if second { self.sequence2 } else { self.sequence };
        // Preserve the float attribute unless the public integer selection was edited.
        if value as u32 == public { value } else { public as f32 }
    }

    fn set_sequence_value(&mut self, second: bool, value: f32) {
        self.sequence_values[usize::from(second)] = value;
        if second {
            self.sequence2 = value as u32;
        } else {
            self.sequence = value as u32;
        }
    }

    /// Source combines both alpha attributes in the byte-sized particle render list.
    pub fn opacity(&self) -> f32 {
        ((self.alpha * self.alpha2).clamp(0.0, 1.0) * 255.0).round_ties_even() / 255.0
    }
}

#[derive(Clone, Debug)]
pub struct Sim {
    plan: Arc<Plan>,
    pub particles: Vec<Particle>,
    time: f32,
    previous_dt: f32,
    current_dt: f32,
    emission: Vec<emission::State>,
    parented: bool,
    fired: Vec<bool>,
    rng: Rng,
    seed: u32,
    born: u32,
    frames: Vec<u32>,
    previous_points: Option<Points>,
    point_traces: Vec<control::PointTraceState>,
    colors: Vec<Option<color::Cached>>,
    path_steps: Vec<(f32, bool)>,
    path_bulge: V3,
    previous_models: model::Cache,
    operator_states: Vec<common::State>,
    particle_op_states: Vec<ParticleOpState>,
    lock_states: Vec<control::LockState>,
    simd_random: simd_random::Stream,
}

impl Sim {
    pub fn new(plan: Arc<Plan>, seed: u32) -> Sim {
        let n = plan.emitters.len();
        let emission = plan.emitters.iter().map(emission::State::new).collect();
        let colors = vec![None; plan.inits.len()];
        let path_steps = vec![(0.0, false); plan.inits.len()];
        let point_traces = vec![control::PointTraceState::default(); plan.point_traces.len()];
        let operator_states = vec![common::State::default(); plan.operator_schedule.len()];
        let lock_states = vec![control::LockState::default(); plan.ops.len()];
        Sim {
            plan,
            particles: Vec::new(),
            time: 0.0,
            previous_dt: INITIAL_PREVIOUS_DT,
            current_dt: 0.0,
            emission,
            parented: false,
            fired: vec![false; n],
            rng: Rng(seed),
            seed,
            born: 0,
            frames: Vec::new(),
            previous_points: None,
            point_traces,
            colors,
            path_steps,
            path_bulge: Rng(seed).range3([-1.0; 3], [1.0; 3]),
            previous_models: Default::default(),
            operator_states,
            particle_op_states: Vec::new(),
            lock_states,
            simd_random: simd_random::Stream::new(seed),
        }
    }

    pub fn plan(&self) -> &Arc<Plan> {
        &self.plan
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn set_sequence_frames(&mut self, frames: Vec<u32>) {
        self.frames = frames;
    }

    pub fn invalidate_lighting(&mut self) {
        self.colors.fill(None);
    }

    pub fn restart(&mut self) {
        self.particles.clear();
        self.time = 0.0;
        self.previous_dt = INITIAL_PREVIOUS_DT;
        self.current_dt = 0.0;
        self.born = 0;
        self.previous_points = None;
        self.point_traces.fill(control::PointTraceState::default());
        self.colors.fill(None);
        self.path_steps.fill((0.0, false));
        self.previous_models.clear();
        self.operator_states.fill(common::State::default());
        self.particle_op_states.clear();
        self.lock_states.fill(control::LockState::default());
        for (state, emitter) in self.emission.iter_mut().zip(&self.plan.emitters) {
            *state = emission::State::new(emitter);
        }
        self.fired.iter_mut().for_each(|f| *f = false);
    }

    pub fn finished(&self) -> bool {
        self.particles.is_empty() && self.plan.emission_end().is_some_and(|end| self.time > end)
    }

    fn sphere(rng: &mut Rng, part: &mut Particle, points: &Points, positions: &control::Positions<'_>, init: &Init) {
        let Init::Sphere { cp, dmin, dmax, bias, abs, smin, smax, sexp, lmin, lmax, spread } = init else {
            return;
        };
        let center = if *spread && points.highest() > 0 {
            positions.at((rng.integer(0, points.highest() as u32) as usize) as i32, part.born)
        } else {
            positions.at(*cp, part.born)
        };
        let (mut d, l) = rng.in_sphere();
        for k in 0..3 {
            if abs[k] {
                d[k] = d[k].abs();
            }
        }
        let d = mul(d, *bias);
        let n = length(d);
        let dir = if n > 1e-6 { scale(d, 1.0 / n) } else { [0.0, 0.0, 1.0] };
        part.pos = add(center, scale(dir, lerp(*dmin, *dmax, l)));
        let speed = rng.range_exp(*smin, *smax, *sexp);
        part.vel = add(scale(dir, speed), points.local(rng.range3(*lmin, *lmax)));
    }

    fn spawn_batch(
        &mut self,
        points: &Points,
        parents: &[(V3, V3)],
        dt: f32,
        world: &dyn Environment,
        births: emission::Batch,
    ) {
        let plan = self.plan.clone();
        let base_born = self.born;
        let first = self.particles.len();
        let origin = [0.0; 3];
        for born in births {
            let seed = self.seed.wrapping_add(base_born.wrapping_add((self.particles.len() - first) as u32));
            let part = Particle {
                dead: false,
                previous_pos: origin,
                previous_dt: self.previous_dt,
                velocity_snapshot: [0.0; 3],
                initial_values: [0.0; Field::Other as usize + 1],
                hitbox: 0,
                hitbox_relative: [0.0; 3],
                pos: origin,
                vel: [0.0; 3],
                age: 0.0,
                life: 1.0,
                radius: plan.radius,
                color: plan.color,
                alpha: plan.alpha,
                alpha2: 1.0,
                rotation: plan.rotation,
                sequence: plan.sequence,
                sequence2: 0,
                sequence_values: [plan.sequence as f32, 0.0],
                trail: 0.1,
                flip: false,
                seed,
                base_color: plan.color,
                base_alpha: plan.alpha,
                rotation_speed: plan.rotation_speed,
                yaw: 0.0,
                alpha_add: 0.0,
                born,
            };
            self.particles.push(part);
        }
        let batch = &mut self.particles[first..];
        let count = batch.len();
        if count == 0 {
            return;
        }
        // Initialize the whole emitter batch before advancing to the next initializer.
        let positions = control::Positions {
            current: points,
            previous: self.previous_points.as_ref().unwrap_or(points),
            time: self.time + dt,
            dt,
        };
        for (index, init) in plan.inits.iter().enumerate() {
            if let Init::NoiseScalar { field, noise } = init {
                noise.initialize_batch(*field, batch, first, world);
                continue;
            }
            for (part_index, part) in batch.iter_mut().enumerate() {
                match init {
                    Init::RayEnvironment(cull) => rays::initialize(cull, part, dt, world),
                    Init::Impact(impact) => part.life = spatial::impact_lifetime(impact, part, world),
                    Init::Scalar(remap) => {
                        if let Some(value) = scalar::evaluate(remap, part, points, world, self.time, false) {
                            set_field(part, remap.field, value, false);
                        }
                    }
                    Init::InheritVelocity { cp, scale: amount } => {
                        if dt > 0.0 {
                            let previous = self.previous_points.as_ref().unwrap_or(points);
                            let velocity = scale(sub(points.get(*cp), previous.get(*cp)), amount / dt);
                            part.vel = add(part.vel, velocity);
                            part.previous_pos = sub(part.previous_pos, scale(velocity, part.previous_dt));
                        }
                    }
                    Init::Warp(warp) => control::apply_warp(warp, part, points, self.time, &mut self.rng),
                    Init::Sphere { .. } => Self::sphere(&mut self.rng, part, points, &positions, init),
                    Init::Box { cp, min, max } => {
                        part.pos = add(positions.at(*cp, part.born), self.rng.range3(*min, *max));
                    }
                    Init::Offset { min, max, local, radius } => {
                        let mut o = self.rng.range3(*min, *max);
                        if *radius {
                            o = scale(o, part.radius);
                        }
                        if *local {
                            o = points.local(o);
                        }
                        part.pos = add(part.pos, o);
                        part.previous_pos = add(part.previous_pos, o);
                    }
                    Init::Ring { cp, radius, thickness, smin, smax, even } => {
                        let t = if *even {
                            (base_born.wrapping_add(part_index as u32) % 32) as f32 / 32.0
                        } else {
                            self.rng.next()
                        };
                        let a = t * std::f32::consts::TAU;
                        let r = radius + self.rng.range(-0.5, 0.5) * thickness;
                        let out = points.local([a.cos(), a.sin(), 0.0]);
                        part.pos = add(positions.at(*cp, part.born), scale(out, r));
                        part.vel = add(part.vel, scale(out, self.rng.range(*smin, *smax)));
                    }
                    Init::Path { start, end, max_dist, bulge, bulge_control, mid, steps, looping } => {
                        let a = positions.at(*start, part.born);
                        let z = positions.at(*end, part.born);
                        let t = if let Some(count) = *steps {
                            let amount = if count > 1.0 { 1.0 / (count - 1.0) } else { 0.0 };
                            let (position, reverse) = &mut self.path_steps[index];
                            let mut step = if *reverse { -amount } else { amount };
                            if *position > 1.0 || *position < 0.0 {
                                if *looping {
                                    *position += if *reverse { 1.0 } else { -1.0 };
                                } else {
                                    *reverse = !*reverse;
                                    step = -step;
                                    *position += step * 2.0;
                                }
                            }
                            let value = *position;
                            *position += step;
                            value
                        } else {
                            self.rng.next()
                        };
                        let middle = path::middle(
                            points,
                            [(*start, a), (*end, z)],
                            *mid,
                            *bulge,
                            *bulge_control,
                            self.path_bulge,
                        );
                        let p = lerp3(lerp3(a, middle, t), lerp3(middle, z, t), t);
                        part.pos = add(p, self.rng.range3([-*max_dist; 3], [*max_dist; 3]));
                    }
                    Init::Model(op) => model::initialize(op, part, points, world, &mut self.rng),
                    Init::Hierarchy(op) => {
                        part.pos = path::hierarchy(op, points, self.time, part.born, self.path_bulge, &mut self.rng);
                    }
                    Init::FromParent { inherit } => {
                        if !parents.is_empty() {
                            let (pos, vel) = parents[self.rng.integer(0, parents.len() as u32 - 1) as usize];
                            part.pos = pos;
                            part.vel = add(part.vel, scale(vel, *inherit));
                        }
                    }
                    Init::Lifetime { min, max, exp } => part.life = self.rng.range_exp(*min, *max, *exp),
                    Init::LifeFromSequence { fps } => {
                        let frames = self.frames.get(part.sequence as usize).copied().unwrap_or(0);
                        part.life = if frames > 0 { frames as f32 / fps } else { 1.0 };
                    }
                    Init::Radius { min, max, exp } => part.radius = self.rng.range_exp(*min, *max, *exp),
                    Init::Color(color) => {
                        color::initialize(color, part, points, world, &mut self.rng, &mut self.colors[index]);
                    }
                    Init::Alpha { min, max, exp } => part.base_alpha = self.rng.range_exp(*min, *max, *exp),
                    Init::Rotation { initial, min, max, exp, flip } => {
                        let mut r = self.rng.range_exp(*min, *max, *exp);
                        if *flip && self.rng.next() < 0.5 {
                            r = -r;
                        }
                        part.rotation = if *flip { r } else { initial + r };
                    }
                    Init::RotationSpeed { constant, min, max, exp, flip } => {
                        let mut s = self.rng.range_exp(*min, *max, *exp);
                        if *flip && self.rng.next() < 0.5 {
                            s = -s;
                        }
                        part.rotation_speed = if *flip { s } else { constant + s };
                    }
                    Init::Yaw { initial, min, max, exp } => {
                        part.yaw = initial + self.rng.range_exp(*min, *max, *exp);
                    }
                    Init::YawFlip { chance } => {
                        if self.rng.next() < *chance {
                            part.yaw += std::f32::consts::PI;
                        }
                    }
                    Init::Sequence { min, max, second } => {
                        let (lo, hi) = (*min.min(max), *min.max(max));
                        let value = self.rng.integer(lo as u32, hi as u32);
                        part.set_sequence_value(*second, value as f32);
                    }
                    Init::TrailLength { min, max } => part.trail = self.rng.range(*min, *max),
                    Init::Velocity { cp, smin, smax, lmin, lmax } => {
                        let d = self.rng.dir();
                        let speed = self.rng.range(*smin, *smax);
                        let local = self.rng.range3(*lmin, *lmax);
                        let axes = points.orientation(*cp);
                        let local =
                            add(add(scale(axes[0], local[0]), scale(axes[1], local[1])), scale(axes[2], local[2]));
                        let velocity = add(scale(d, speed), local);
                        part.vel = add(part.vel, velocity);
                        part.previous_pos = sub(part.previous_pos, scale(velocity, part.previous_dt));
                    }
                    Init::Noise(op) => {
                        let velocity = op.evaluate(part.pos, part.born, points);
                        part.vel = add(part.vel, velocity);
                        part.previous_pos = sub(part.previous_pos, scale(velocity, part.previous_dt));
                    }
                    Init::NoiseScalar { .. } => {} // Handled once for the complete batch above.
                    Init::Remap { input, imin, imax, output, omin, omax, multiply, within } => {
                        let x = get_field(part, *input);
                        if *within && (x < imin.min(*imax) || x > imin.max(*imax)) {
                            continue;
                        }
                        let k =
                            if (imax - imin).abs() > 1e-6 { ((x - imin) / (imax - imin)).clamp(0.0, 1.0) } else { 0.0 };
                        set_field(part, *output, lerp(*omin, *omax, k), *multiply);
                    }
                    Init::PreAge(noise) => {
                        part.born -= noise.evaluate(part.pos, part.born, world).clamp(0.0, 1.0) * part.life;
                    }
                }
                if init.written_attributes() & (1 << 2) != 0 {
                    part.previous_pos = sub(part.pos, scale(part.vel, part.previous_dt));
                }
            }
        }
        for part in batch.iter_mut() {
            if !part.life.is_finite() || part.life < 0.0 {
                part.life = 1.0;
            }
            part.age = self.time - part.born;
            part.color = part.base_color.map(|value| value.clamp(0.0, 1.0));
            part.alpha = part.base_alpha;
            part.velocity_snapshot = part.vel;
            for field in [
                Field::PositionX,
                Field::Lifetime,
                Field::Radius,
                Field::Rotation,
                Field::RotationSpeed,
                Field::Alpha,
                Field::Alpha2,
                Field::CreationTime,
                Field::Sequence,
                Field::Sequence2,
                Field::TrailLength,
                Field::Yaw,
            ] {
                part.initial_values[field as usize] = get_field(part, field);
            }
        }
        self.born = self.born.wrapping_add(count as u32);
    }

    pub fn step(&mut self, dt: f32, origin: V3, budget: usize) {
        self.step_at(dt, &Points::unparented(origin), &[], budget);
    }

    pub fn step_at(&mut self, dt: f32, points: &Points, parents: &[(V3, V3)], budget: usize) {
        self.step_in(dt, points, parents, budget, &EmptyEnvironment);
    }

    pub fn step_in(&mut self, dt: f32, points: &Points, parents: &[(V3, V3)], budget: usize, world: &dyn Environment) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.current_dt = dt;
        let cap = self.plan.max.min(budget);
        let t0 = self.time;
        let t1 = t0 + dt;
        self.parented |= !parents.is_empty();
        let previous_models = std::mem::take(&mut self.previous_models);
        let frame = model::Frame::new(world, &previous_models);
        let world = &frame;
        let plan = self.plan.clone();
        common::prepare(&plan.operator_schedule, &mut self.operator_states, t1, self.seed, false);
        let mut generated = std::borrow::Cow::Borrowed(points);
        for (index, (op, state)) in plan.point_traces.iter().zip(&mut self.point_traces).enumerate() {
            if !self.operator_states[plan.trace_schedule[index]].active() {
                continue;
            }
            control::update_point_trace(op, state, generated.to_mut(), world, t1);
        }
        let points = &*generated;
        if t0 == 0.0 && self.fired.iter().all(|f| !f) && self.born == 0 {
            let count = plan.initial.min(cap.saturating_sub(self.particles.len()));
            self.spawn_batch(points, parents, dt, world, emission::Batch::instant(t0, count));
        }
        for (i, e) in plan.emitters.iter().enumerate() {
            let room = cap.saturating_sub(self.particles.len());
            let births = match e {
                Emit::Instant { min, max, start } => {
                    let mut count = 0;
                    if !self.fired[i] && t1 >= *start {
                        self.fired[i] = true;
                        count = if max > min { self.rng.integer(*min, *max) as usize } else { *max as usize };
                    }
                    emission::Batch::instant(*start, count.min(room))
                }
                _ => self.emission[i].births(e, (t1, dt), points, self.parented.then_some(parents.len()), room, world),
            };
            self.spawn_batch(points, parents, dt, world, births);
        }
        for part in &mut self.particles {
            // Public velocity edits must update the native previous-position representation.
            if part.vel != part.velocity_snapshot {
                part.previous_pos = sub(part.pos, scale(part.vel, part.previous_dt));
            }
            part.age += dt;
        }
        operate(
            &plan,
            &self.operator_states,
            &mut self.particles,
            points,
            dt,
            world,
            &mut self.rng,
            t1,
            &mut self.particle_op_states,
            &mut self.lock_states,
            &mut self.simd_random,
        );
        for part in &mut self.particles {
            part.velocity_snapshot = part.vel;
        }

        self.particles.retain(|p| !p.dead && p.pos.iter().all(|c| c.is_finite() && c.abs() < 1.0e6));
        self.time = t1;
        if dt > 0.0 {
            self.previous_dt = dt;
        }
        self.previous_points = Some(points.clone());
        self.previous_models = frame.finish();
    }
}

fn get_field(p: &Particle, field: Field) -> f32 {
    match field {
        Field::PositionX => p.pos[0],
        Field::Lifetime => p.life,
        Field::Radius => p.radius,
        Field::Rotation => p.rotation,
        Field::RotationSpeed => p.rotation_speed,
        Field::Alpha => p.base_alpha,
        Field::Alpha2 => p.alpha2,
        Field::CreationTime => p.born,
        Field::Sequence => p.sequence_value(false),
        Field::Sequence2 => p.sequence_value(true),
        Field::TrailLength => p.trail,
        Field::Yaw => p.yaw,
        Field::Other => 0.0,
    }
}

fn set_field(p: &mut Particle, field: Field, x: f32, multiply: bool) {
    if !x.is_finite() {
        return;
    }
    let slot = match field {
        Field::PositionX => &mut p.pos[0],
        Field::Lifetime => &mut p.life,
        Field::Radius => &mut p.radius,
        Field::Rotation => &mut p.rotation,
        Field::RotationSpeed => &mut p.rotation_speed,
        Field::Alpha => &mut p.base_alpha,
        Field::Alpha2 => &mut p.alpha2,
        Field::TrailLength => &mut p.trail,
        Field::Yaw => &mut p.yaw,
        Field::CreationTime => &mut p.born,
        Field::Sequence | Field::Sequence2 => {
            let second = field == Field::Sequence2;
            let value = if multiply { p.sequence_value(second) * x } else { x };
            p.set_sequence_value(second, value);
            return;
        }
        Field::Other => return,
    };
    let value = if multiply { *slot * x } else { x };
    *slot = if matches!(field, Field::Alpha | Field::Alpha2) { value.clamp(0.0, 1.0) } else { value };
}

#[derive(Clone, Copy, Debug, Default)]
struct ParticleOpState {
    alpha: f32,
    alpha_written: bool,
    color: V3,
    color_written: bool,
    old: V3,
    acceleration: V3,
}

#[allow(clippy::too_many_arguments)]
fn operate(
    plan: &Plan,
    operator_states: &[common::State],
    particles: &mut Vec<Particle>,
    points: &Points,
    dt: f32,
    world: &dyn Environment,
    rng: &mut Rng,
    time: f32,
    particle_states: &mut Vec<ParticleOpState>,
    lock_states: &mut [control::LockState],
    simd_random: &mut simd_random::Stream,
) {
    particle_states.resize(particles.len(), ParticleOpState::default());
    for (part, context) in particles.iter().zip(particle_states.iter_mut()) {
        *context =
            ParticleOpState { alpha: part.base_alpha, color: part.base_color, old: part.pos, ..Default::default() };
    }
    if particles.iter().any(|particle| particle.dead) {
        cull::compact(particles, particle_states, plan.requires_order_invariance, |particle| particle.dead);
    }
    for (k, op) in plan.ops.iter().enumerate() {
        if particles.is_empty() {
            break;
        }
        let state = operator_states[plan.op_schedule[k]];
        if !state.active() {
            continue;
        }
        if let Op::Lock(lock) = op {
            control::apply_lock(lock, &mut lock_states[k], particles, points, time, dt, state.strength, simd_random);
            continue;
        }
        let mut has_dead = false;
        for (part, context) in particles.iter_mut().zip(particle_states.iter_mut()) {
            let life = if part.life > 0.0 { part.life } else { 1.0 };
            let t = (part.age / life).clamp(0.0, 1.0);
            let mut alpha = context.alpha;
            let mut alpha_written = context.alpha_written;
            let mut color = context.color;
            let mut color_written = context.color_written;
            let old = context.old;
            let mut acceleration = context.acceleration;
            let before_pos = part.pos;
            let before_vel = part.vel;
            match op {
                Op::Decay => part.dead |= cull::lifespan_expired((time, part.born, part.life)),
                Op::FieldRemap(remap) => {
                    let current = |field| {
                        if field == Field::Alpha {
                            if alpha_written { alpha + part.alpha_add } else { part.alpha }
                        } else {
                            get_field(part, field)
                        }
                    };
                    let value = remap.value(current(remap.input), current(remap.output), state.strength);
                    match remap.output {
                        Field::Alpha => {
                            // Native strength blending can overshoot the alpha endpoints.
                            part.alpha = value;
                            alpha = value;
                            alpha_written = false;
                            part.alpha_add = 0.0;
                        }
                        Field::Alpha2 => part.alpha2 = value,
                        Field::CreationTime => {
                            set_field(part, Field::CreationTime, value, false);
                            part.age = time - part.born;
                        }
                        field => set_field(part, field, value, false),
                    }
                }
                Op::BoneLock(lock) => model::apply_lock(lock, part, points, dt, world),
                Op::Scalar(remap) => {
                    if alpha_written {
                        part.alpha = alpha + part.alpha_add;
                    }
                    if let Some(value) = scalar::evaluate(remap, part, points, world, part.born + part.age, true) {
                        match remap.field {
                            Field::Alpha => {
                                alpha = value;
                                part.alpha_add = 0.0;
                            }
                            Field::Radius => {
                                part.radius = value;
                            }
                            _ => set_field(part, remap.field, value, false),
                        }
                    }
                }
                Op::Lock(_) => {}
                Op::Force(force) => acceleration = add(acceleration, forces::acceleration(force, part, points, rng)),
                Op::Spatial(op) => spatial::apply(op, part, old, points, dt, world),
                Op::CullRandom { start, end, exponent, percentage } => {
                    let chance = rng.next();
                    let threshold = lerp(*start, *end, rng.next().powf(*exponent));
                    if chance <= *percentage && t >= *start && t <= *end && t >= threshold {
                        part.dead = true;
                    }
                }
                Op::Movement { gravity, drag } => {
                    if dt > 0.0 {
                        let movement = if part.previous_dt > 0.0 {
                            scale(sub(part.pos, part.previous_pos), dt / part.previous_dt)
                        } else {
                            scale(part.vel, dt)
                        };
                        let movement = scale(movement, (1.0 - drag).powf(dt * 30.0));
                        part.previous_pos = part.pos;
                        part.previous_dt = dt;
                        part.pos = add(part.pos, add(movement, scale(add(*gravity, acceleration), dt * dt)));
                        part.vel = scale(sub(part.pos, part.previous_pos), 1.0 / dt);
                    }
                }
                Op::AlphaSimple(fade) => {
                    let current = if alpha_written { alpha + part.alpha_add } else { part.alpha };
                    if let Some(value) =
                        fade.value((time, part.born, part.life), part.initial_values[Field::Alpha as usize], current)
                    {
                        alpha = value;
                        part.alpha_add = 0.0;
                        alpha_written = true;
                    }
                }
                Op::AlphaRandom(fade) => {
                    if let Some(value) = fade.value(
                        (time, part.born, part.life),
                        part.seed,
                        state.random_offset,
                        part.initial_values[Field::Alpha as usize],
                    ) {
                        alpha = value;
                        part.alpha_add = 0.0;
                        alpha_written = true;
                    }
                }
                Op::FadeAndDecay(fade) | Op::FadeTracer(fade) => {
                    let (value, dead) =
                        fade.sample((time, part.born, part.life), part.initial_values[Field::Alpha as usize]);
                    if matches!(op, Op::FadeTracer(_)) {
                        let (position, trail, expired) = alpha::tracer_lifetime(
                            (time, part.born, part.life),
                            dt,
                            part.pos,
                            part.previous_pos,
                            part.trail,
                        );
                        part.pos = position;
                        part.trail = trail;
                        part.dead |= expired;
                    } else {
                        part.dead |= dead;
                    }
                    if let Some(value) = value {
                        part.alpha = value;
                        alpha = value;
                        alpha_written = false;
                        part.alpha_add = 0.0;
                    }
                }
                Op::RadiusScale(scale) => {
                    if let Some(value) =
                        scale.value((time, part.born, part.life), part.initial_values[Field::Radius as usize])
                    {
                        part.radius = value;
                    }
                }
                Op::ColorFade { color: target, t0, t1, ease: smooth } => {
                    if t >= *t0 {
                        let mut x = if t1 > t0 { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 1.0 };
                        if *smooth {
                            x = ease(x);
                        }
                        color = lerp3(color, *target, x);
                        color_written = true;
                    }
                }
                Op::RotationBasic => part.rotation += part.rotation_speed * dt,
                Op::Spin { rate, stop, min, yaw } => {
                    let strength = if *stop > 0.0 { (1.0 - part.age / stop).max(0.0) } else { 1.0 };
                    let r = (lerp(*min, *rate, strength) + part.rotation_speed) * dt;
                    if *yaw {
                        part.yaw += r;
                    } else {
                        part.rotation += r;
                    }
                }
                Op::OscScalar { field, wave } => {
                    let Some(delta) =
                        wave.delta((time, part.born, part.life), part.seed, state.random_offset, dt * state.strength)
                    else {
                        continue;
                    };
                    match field {
                        Field::PositionX => part.pos[0] += delta,
                        Field::Radius => part.radius += delta,
                        Field::Alpha => {
                            let current = if alpha_written { alpha + part.alpha_add } else { part.alpha };
                            alpha = (current + delta).clamp(0.0, 1.0);
                            part.alpha_add = 0.0;
                        }
                        Field::Alpha2 => part.alpha2 = (part.alpha2 + delta).clamp(0.0, 1.0),
                        Field::Rotation => part.rotation += delta,
                        Field::Yaw => part.yaw += delta,
                        Field::CreationTime => part.born += delta,
                        _ => {
                            let value = get_field(part, *field) + delta;
                            set_field(part, *field, value, false);
                        }
                    }
                }
                Op::OscVector { field, wave } => {
                    let Some(delta) =
                        wave.delta((time, part.born, part.life), part.seed, state.random_offset, dt * state.strength)
                    else {
                        continue;
                    };
                    match field {
                        0 => part.pos = add(part.pos, delta),
                        2 => part.previous_pos = add(part.previous_pos, delta),
                        6 => {
                            let current = if color_written { color } else { part.color };
                            color = add(current, delta).map(|value| value.clamp(0.0, 1.0));
                            color_written = true;
                        }
                        _ => {}
                    }
                }
                Op::NoiseScalar { field, min, max, speed, additive } => {
                    let x = noise::map(noise::scalar(scale(part.pos, *speed)), *min, *max, additive.then_some(dt));
                    match field {
                        Field::Radius => {
                            let current = part.radius;
                            part.radius = (x + if *additive { current } else { 0.0 }).max(0.0);
                        }
                        Field::Alpha => {
                            let current = if alpha_written { alpha } else { part.alpha };
                            alpha = x + if *additive { current } else { 0.0 };
                            part.alpha_add = 0.0;
                        }
                        _ => {
                            let value = x + if *additive { get_field(part, *field) } else { 0.0 };
                            set_field(part, *field, value, false);
                        }
                    }
                }
                Op::NoiseVector { field, min, max, speed, additive } => {
                    let n = noise::vector(scale(part.pos, *speed));
                    let x = std::array::from_fn(|axis| noise::map(n[axis], min[axis], max[axis], None));
                    if *field == 6 {
                        let current = if color_written { color } else { part.color };
                        color = if *additive { add(current, scale(x, dt)) } else { x };
                        color_written = true;
                    } else if *field == 0 {
                        part.pos = if *additive { add(part.pos, scale(x, dt)) } else { x };
                    } else if *field == 2 {
                        part.previous_pos = if *additive { add(part.previous_pos, scale(x, dt)) } else { x };
                    }
                }
                Op::Ramp { field, ramp } => {
                    let current = match field {
                        Field::Radius => part.radius,
                        Field::Alpha if alpha_written => alpha + part.alpha_add,
                        Field::Alpha => part.alpha,
                        _ => get_field(part, *field),
                    };
                    let Some(value) = ramp.apply(
                        (time, part.born, part.life),
                        part.seed,
                        state.random_offset,
                        dt * state.strength,
                        current,
                    ) else {
                        continue;
                    };
                    match field {
                        Field::Radius => {
                            part.radius = value;
                        }
                        Field::Alpha => {
                            alpha = value;
                            part.alpha_add = 0.0;
                        }
                        _ => set_field(part, *field, value, false),
                    }
                }
                Op::MaxVelocity { max } => {
                    let s = length(part.vel);
                    if *max > 0.0 && s > *max {
                        part.vel = scale(part.vel, max / s);
                    }
                }
                Op::Constrain { cp, min, max, offset } => {
                    let c = add(points.get(*cp), *offset);
                    let d = sub(part.pos, c);
                    let l = length(d);
                    if l > 1e-4 {
                        let clamped = l.clamp(*min, max.max(*min));
                        if (clamped - l).abs() > 1e-4 {
                            part.pos = add(c, scale(d, clamped / l));
                        }
                    }
                }
                Op::RotateAround { cp, axis, rate } => {
                    let n = length(*axis);
                    if n > 1e-6 {
                        let a = scale(*axis, 1.0 / n);
                        let c = points.get(*cp);
                        let d = sub(part.pos, c);
                        let (s, co) = (rate * dt).sin_cos();
                        let r = add(add(scale(d, co), scale(cross(a, d), s)), scale(a, dot(a, d) * (1.0 - co)));
                        part.pos = add(c, r);
                    }
                }
                Op::Dampen { cp, range, factor } => {
                    if *range > 0.0 {
                        let d = length(sub(part.pos, points.get(*cp)));
                        if d < *range {
                            let k = lerp(*factor, 1.0, d / range);
                            part.vel = scale(part.vel, k.powf(dt * 30.0));
                        }
                    }
                }
            }
            alpha_written |= match op {
                Op::Scalar(remap) => remap.field == Field::Alpha,
                Op::NoiseScalar { field, .. } | Op::OscScalar { field, .. } | Op::Ramp { field, .. } => {
                    *field == Field::Alpha
                }
                _ => false,
            };
            if !matches!(
                op,
                Op::Movement { .. }
                    | Op::FadeTracer(_)
                    | Op::FieldRemap(remap::Scalar { output: Field::PositionX, .. })
                    | Op::Spatial(spatial::SpatialOp::Collision { .. })
                    | Op::NoiseVector { .. }
                    | Op::OscVector { .. }
                    | Op::OscScalar { field: Field::PositionX, .. }
                    | Op::Ramp { field: Field::PositionX, .. }
            ) {
                if part.vel != before_vel {
                    part.previous_pos = sub(part.pos, scale(part.vel, part.previous_dt));
                } else if part.pos != before_pos {
                    part.previous_pos = add(part.previous_pos, sub(part.pos, before_pos));
                }
            }

            has_dead |= part.dead;
            *context = ParticleOpState { alpha, alpha_written, color, color_written, old, acceleration };
        }
        if has_dead {
            cull::compact(particles, particle_states, plan.requires_order_invariance, |particle| particle.dead);
        }
    }
    for (part, context) in particles.iter_mut().zip(particle_states.iter()) {
        if context.alpha_written {
            part.alpha = (context.alpha + part.alpha_add).clamp(0.0, 1.0);
        }
        if context.color_written {
            part.color = context.color.map(|c| c.clamp(0.0, 1.0));
        }
    }
}

#[derive(Clone, Debug)]
pub struct Effect {
    pub sims: Vec<(f32, Sim)>,
    parents: Vec<Option<usize>>,
    pub missing: Vec<String>,
    time: f32,
    idle: f32,
    scratch: Vec<(V3, V3)>,
}

impl Effect {
    /// Apply view-dependent particle attributes after simulation and before
    /// collecting draw instances. Source writes rotation back to the simulation;
    /// later operators must observe it, including while playback is paused.
    pub fn prepare_render(&mut self, view: [[f32; 4]; 2]) {
        for (delay, sim) in &mut self.sims {
            if self.time < *delay {
                continue;
            }
            let Some(forward) = sim.plan.render.screen_velocity_angle else { continue };
            for particle in &mut sim.particles {
                if let Some(rotation) =
                    crate::screen_velocity::rotation(particle.pos, particle.previous_position(), view, forward)
                {
                    particle.rotation = rotation;
                }
            }
        }
    }

    pub fn new(name: &str, lookup: &dyn Fn(&str) -> Option<Arc<Plan>>, seed: u32) -> Option<Effect> {
        let root = lookup(name)?;
        let mut sims: Vec<(f32, Sim)> = Vec::new();
        let mut parents = Vec::new();
        let mut missing = Vec::new();
        let mut queue: std::collections::VecDeque<(Arc<Plan>, f32, usize, Option<usize>)> =
            std::collections::VecDeque::from([(root, 0.0, 0, None)]);
        let mut n = 0u32;
        while let Some((plan, delay, depth, parent)) = queue.pop_front() {
            if sims.len() >= MAX_SYSTEMS {
                break;
            }
            let index = sims.len();
            if depth < MAX_CHILD_DEPTH {
                for (child, d) in &plan.children {
                    match lookup(child) {
                        Some(c) => queue.push_back((c, delay + d, depth + 1, Some(index))),
                        None => {
                            if !missing.iter().any(|m: &String| m.eq_ignore_ascii_case(child)) {
                                missing.push(child.clone());
                            }
                        }
                    }
                }
            }
            n = n.wrapping_add(1);
            let mut sim = Sim::new(plan, seed.wrapping_mul(2_654_435_761).wrapping_add(n));
            sim.parented = parent.is_some();
            sims.push((delay, sim));
            parents.push(parent);
        }
        Some(Effect { sims, parents, missing, time: 0.0, idle: 0.0, scratch: Vec::new() })
    }

    pub fn set_sequence_frames(&mut self, frames: &dyn Fn(&Plan) -> Option<Vec<u32>>) {
        for (_, sim) in &mut self.sims {
            if let Some(f) = frames(&sim.plan) {
                sim.set_sequence_frames(f);
            }
        }
    }

    pub fn invalidate_lighting(&mut self) {
        for (_, sim) in &mut self.sims {
            sim.invalidate_lighting();
        }
    }

    pub fn continuous(&self) -> bool {
        self.sims.iter().all(|(_, s)| s.plan.emitters.is_empty() || s.plan.continuous())
    }

    pub fn step(&mut self, dt: f32, origin: V3, budget: usize) {
        self.step_at(dt, &Points::unparented(origin), budget);
    }

    pub fn step_at(&mut self, dt: f32, points: &Points, budget: usize) {
        self.step_in(dt, points, budget, &EmptyEnvironment);
    }

    pub fn step_in(&mut self, dt: f32, points: &Points, budget: usize, world: &dyn Environment) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.time += dt;
        let mut left = budget;
        for i in 0..self.sims.len() {
            let delay = self.sims[i].0;
            if self.time < delay {
                continue;
            }
            self.scratch.clear();
            let mut child_points = std::borrow::Cow::Borrowed(points);
            if let Some(Some(p)) = self.parents.get(i)
                && let Some((_, parent)) = self.sims.get(*p)
            {
                if let Some(points) = &parent.previous_points {
                    child_points = std::borrow::Cow::Owned(points.clone());
                }
                self.scratch.extend(parent.particles.iter().map(|q| (q.pos, q.vel)));
                for (index, spec) in parent.plan.child_points.iter().enumerate() {
                    if !parent.operator_states[parent.plan.child_schedule[index]].active() {
                        continue;
                    }
                    control::copy_child_points(
                        spec,
                        &parent.particles,
                        child_points.to_mut(),
                        self.sims[i].1.plan.group_id,
                    );
                }
            }
            let sim = &mut self.sims[i].1;
            sim.step_in(dt, &child_points, &self.scratch, left.saturating_add(sim.particles.len()).min(budget), world);
            left = left.saturating_sub(sim.particles.len());
        }
        let done = self.sims.iter().all(|(d, s)| self.time >= *d && s.plan.finite() && s.finished());
        if done {
            self.idle += dt;
            if self.idle >= LOOP_PAUSE {
                self.idle = 0.0;
                self.time = 0.0;
                for (_, s) in &mut self.sims {
                    s.restart();
                }
            }
        }
    }

    pub fn particles(&self) -> impl Iterator<Item = (&Plan, &Particle)> + '_ {
        self.sims
            .iter()
            .filter(|(_, s)| s.plan.draws)
            .flat_map(|(_, s)| s.particles.iter().map(move |p| (&**s.plan(), p)))
    }

    pub fn systems(&self) -> impl Iterator<Item = (&Plan, &[Particle])> + '_ {
        self.sims.iter().filter(|(_, s)| s.plan.draws).map(|(_, s)| (&**s.plan(), &s.particles[..]))
    }

    /// Each collection renders with its own current timestep, including zero-DT frames.
    pub fn systems_with_dt(&self) -> impl Iterator<Item = (&Plan, &[Particle], f32)> + '_ {
        self.sims.iter().filter(|(_, s)| s.plan.draws).map(|(_, s)| (&**s.plan(), &s.particles[..], s.current_dt))
    }

    pub fn particle_count(&self) -> usize {
        self.sims.iter().map(|(_, s)| s.particles.len()).sum()
    }
}
