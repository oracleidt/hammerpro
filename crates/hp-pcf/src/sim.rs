use std::sync::Arc;

use crate::environment::{EmptyEnvironment, Environment};
use crate::model::{Operator, SystemDef};

mod color;
mod control;
mod forces;
mod model;
mod noise;
mod path;
mod random;
mod rays;
mod scalar;
mod spatial;

pub const MAX_PREVIEW_PARTICLES: usize = 5000;
pub const MAX_CONTROL_POINTS: usize = 64;
const LOOP_PAUSE: f32 = 0.6;
const MAX_CHILD_DEPTH: usize = 4;
const MAX_SYSTEMS: usize = 16;
pub const MAX_EFFECT_PARTICLES: usize = MAX_PREVIEW_PARTICLES * MAX_SYSTEMS;
const MAX_SEQUENCES: u32 = 64;

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

fn bias(x: f32, amount: f32) -> f32 {
    if !(amount > 0.0 && amount < 1.0) || (amount - 0.5).abs() < 1e-4 {
        return x;
    }
    x.max(0.0).powf(amount.ln() / 0.5f32.ln())
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn hash01(seed: u32, key: u32) -> f32 {
    let mut x = seed ^ key.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
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
            usize::try_from(cp).ok().and_then(|i| self.orientations.get(i)).copied().unwrap_or([
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ])
        }
    }

    fn highest(&self) -> usize {
        self.pos.len().saturating_sub(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Emit {
    Continuous { rate: f32, start: f32, duration: f32 },
    Instant { min: u32, max: u32, start: f32 },
    Noise { min: f32, max: f32, speed: f32, start: f32, duration: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Lifetime,
    Radius,
    Rotation,
    RotationSpeed,
    Alpha,
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
            1 => Field::Lifetime,
            3 => Field::Radius,
            4 => Field::Rotation,
            5 => Field::RotationSpeed,
            7 | 16 => Field::Alpha,
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
        smin: f32,
        smax: f32,
        lmin: V3,
        lmax: V3,
    },
    Noise {
        min: V3,
        max: V3,
        local: bool,
    },
    NoiseScalar {
        field: Field,
        min: f32,
        max: f32,
    },
    PreAge {
        min: f32,
        max: f32,
    },
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

#[derive(Clone, Debug, PartialEq)]
enum Op {
    BoneLock(model::Lock),
    Scalar(scalar::Remap),
    Lock(control::Lock),
    Force(forces::Force),
    Spatial(spatial::SpatialOp),
    CullRandom {
        start: f32,
        end: f32,
        exponent: f32,
        percentage: f32,
    },
    Movement {
        gravity: V3,
        drag: f32,
    },
    FadeIn {
        min: f32,
        max: f32,
        proportional: bool,
        ease: bool,
    },
    FadeOut {
        min: f32,
        max: f32,
        proportional: bool,
        ease: bool,
    },
    FadeAndDecay {
        start: f32,
        end: f32,
        in0: f32,
        in1: f32,
        out0: f32,
        out1: f32,
    },
    RadiusScale {
        t0: f32,
        t1: f32,
        s0: f32,
        s1: f32,
        bias: f32,
        ease: bool,
    },
    ColorFade {
        color: [f32; 3],
        t0: f32,
        t1: f32,
        ease: bool,
    },
    RotationBasic,
    Spin {
        rate: f32,
        stop: f32,
        min: f32,
        yaw: bool,
    },
    OscScalar {
        field: Field,
        rate: [f32; 2],
        freq: [f32; 2],
        proportional: bool,
        t0: f32,
        t1: f32,
        se_prop: bool,
        mult: f32,
        phase: f32,
    },
    OscVector {
        color: bool,
        rate: [V3; 2],
        freq: [V3; 2],
        proportional: bool,
        t0: f32,
        t1: f32,
        se_prop: bool,
        mult: f32,
        phase: f32,
    },
    NoiseScalar {
        field: Field,
        min: f32,
        max: f32,
        speed: f32,
        additive: bool,
    },
    NoiseVector {
        field: i32,
        min: V3,
        max: V3,
        speed: f32,
        additive: bool,
    },
    Ramp {
        field: Field,
        rate: [f32; 2],
        t0: f32,
        t1: f32,
    },
    MaxVelocity {
        max: f32,
    },
    Constrain {
        cp: i32,
        min: f32,
        max: f32,
        offset: V3,
    },
    RotateAround {
        cp: i32,
        axis: V3,
        rate: f32,
    },
    Dampen {
        cp: i32,
        range: f32,
        factor: f32,
    },
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub group_id: i32,
    pub name: String,
    pub material: String,
    emitters: Vec<Emit>,
    inits: Vec<Init>,
    ops: Vec<Op>,
    child_points: Vec<control::ChildPoints>,
    point_traces: Vec<control::PointTrace>,
    decays: bool,
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
            child_points: Vec::new(),
            point_traces: Vec::new(),
            decays: false,
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
        for op in def.forces.iter().chain(&def.operators).chain(&def.constraints) {
            p.compile_op(op);
        }
        p
    }

    fn compile_renderers(&mut self, def: &SystemDef) {
        for op in &def.renderers {
            let name = op.function.to_ascii_lowercase();
            let kind = match name.as_str() {
                "render_animated_sprites" | "render_screen_velocity_rotate" => RenderKind::Sprites,
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
                min_len: f(op, "min length", 0.0).clamp(0.0, 100_000.0),
                max_len: f(op, "max length", 2000.0).clamp(0.0, 100_000.0),
                len_fade_in: f(op, "length fade in time", 0.0).clamp(0.0, 600.0),
            };
        }
        self.draws = self.render.kind != RenderKind::None;
    }

    fn compile_emitter(&mut self, op: &Operator) {
        let start = f(op, "emission_start_time", 0.0).clamp(0.0, 600.0);
        let duration = f(op, "emission_duration", 0.0).clamp(0.0, 600.0);
        match op.function.to_ascii_lowercase().as_str() {
            "emit_continuously" => self.emitters.push(Emit::Continuous {
                rate: f(op, "emission_rate", 100.0).clamp(0.0, 10_000.0),
                start,
                duration,
            }),
            "emit_instantaneously" => {
                let max = i(op, "num_to_emit", 100).clamp(0, 100_000) as u32;
                let min = op.int("num_to_emit_minimum").map_or(max, |m| (m.clamp(0, 100_000) as u32).min(max));
                self.emitters.push(Emit::Instant { min, max, start });
            }
            "emit noise" => self.emitters.push(Emit::Noise {
                min: f(op, "emission minimum", 0.0).clamp(0.0, 10_000.0),
                max: f(op, "emission maximum", 100.0).clamp(0.0, 10_000.0),
                speed: f(op, "time noise coordinate scale", 0.1).clamp(0.0, 100.0),
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
                min: f(op, "lifetime_min", 0.0).clamp(0.0, 600.0),
                max: f(op, "lifetime_max", 0.0).clamp(0.0, 600.0),
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
                min: (f(op, "alpha_min", 255.0) / 255.0).clamp(0.0, 1.0),
                max: (f(op, "alpha_max", 255.0) / 255.0).clamp(0.0, 1.0),
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
                min: i(op, "sequence_min", 0).clamp(0, MAX_SEQUENCES as i32 - 1),
                max: i(op, "sequence_max", 0).clamp(0, MAX_SEQUENCES as i32 - 1),
            },
            "trail length random" => Init::TrailLength {
                min: f(op, "length_min", 0.1).clamp(0.0, 100.0),
                max: f(op, "length_max", 0.1).clamp(0.0, 100.0),
            },
            "velocity random" => Init::Velocity {
                smin: f(op, "random_speed_min", f(op, "speed_min", 0.0)),
                smax: f(op, "random_speed_max", f(op, "speed_max", 0.0)),
                lmin: v(op, "speed_in_local_coordinate_system_min", [0.0; 3]),
                lmax: v(op, "speed_in_local_coordinate_system_max", [0.0; 3]),
            },
            "velocity inherit from control point" => {
                Init::InheritVelocity { cp: i(op, "control point number", 0), scale: f(op, "velocity scale", 1.0) }
            }
            "velocity noise" | "initial velocity noise" => Init::Noise {
                min: v(op, "output minimum", [0.0; 3]),
                max: v(op, "output maximum", [0.0; 3]),
                local: b(op, "Apply Velocity in Local Space (0/1)", false),
            },
            "remap noise to scalar" => Init::NoiseScalar {
                field: Field::from_index(i(op, "output field", 3)),
                min: f(op, "output minimum", 0.0),
                max: f(op, "output maximum", 1.0),
            },
            "lifetime pre-age noise" => Init::PreAge {
                min: f(op, "start age minimum", 0.0).clamp(0.0, 1.0),
                max: f(op, "start age maximum", 0.0).clamp(0.0, 1.0),
            },
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
            "cull random" => Op::CullRandom {
                start: f(op, "Cull Start Time", 0.0),
                end: f(op, "Cull End Time", 1.0),
                exponent: f(op, "Cull Time Exponent", 1.0),
                percentage: f(op, "Cull Percentage", 0.5),
            },
            "movement basic" => {
                Op::Movement { gravity: v(op, "gravity", [0.0; 3]), drag: f(op, "drag", 0.0).clamp(0.0, 1.0) }
            }
            "lifespan decay" => {
                self.decays = true;
                return;
            }
            "alpha fade in random" => Op::FadeIn {
                min: f(op, "fade in time min", 0.25).max(0.0),
                max: f(op, "fade in time max", 0.25).max(0.0),
                proportional: b(op, "proportional 0/1", true),
                ease: b(op, "ease in and out", true),
            },
            "alpha fade out random" => Op::FadeOut {
                min: f(op, "fade out time min", 0.25).max(0.0),
                max: f(op, "fade out time max", 0.25).max(0.0),
                proportional: b(op, "proportional 0/1", true),
                ease: b(op, "ease in and out", true),
            },
            "alpha fade in simple" => {
                let t = f(op, "proportional fade in time", 0.25).max(0.0);
                Op::FadeIn { min: t, max: t, proportional: true, ease: false }
            }
            "alpha fade out simple" => {
                let t = f(op, "proportional fade out time", 0.25).max(0.0);
                Op::FadeOut { min: t, max: t, proportional: true, ease: false }
            }
            "alpha fade and decay" | "alpha fade and decay for tracers" => {
                self.decays = true;
                Op::FadeAndDecay {
                    start: f(op, "start_alpha", 1.0),
                    end: f(op, "end_alpha", 0.0),
                    in0: f(op, "start_fade_in_time", 0.0),
                    in1: f(op, "end_fade_in_time", 0.5),
                    out0: f(op, "start_fade_out_time", 0.5),
                    out1: f(op, "end_fade_out_time", 1.0),
                }
            }
            "radius scale" => Op::RadiusScale {
                t0: f(op, "start_time", 0.0),
                t1: f(op, "end_time", 1.0),
                s0: f(op, "radius_start_scale", 1.0),
                s1: f(op, "radius_end_scale", 1.0),
                bias: f(op, "scale_bias", 0.5),
                ease: b(op, "ease_in_and_out", false),
            },
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
                rate: [f(op, "oscillation rate min", 0.0), f(op, "oscillation rate max", 0.0)],
                freq: [f(op, "oscillation frequency min", 1.0), f(op, "oscillation frequency max", 1.0)],
                proportional: b(op, "proportional 0/1", true),
                t0: (f(op, "start time min", 0.0) + f(op, "start time max", 0.0)) * 0.5,
                t1: (f(op, "end time min", 1.0) + f(op, "end time max", 1.0)) * 0.5,
                se_prop: b(op, "start/end proportional", true),
                mult: f(op, "oscillation multiplier", 2.0),
                phase: f(op, "oscillation start phase", 0.5),
            },
            "oscillate vector" => Op::OscVector {
                color: i(op, "oscillation field", 0) == 6,
                rate: [vec_or(op, "oscillation rate min", [0.0; 3]), vec_or(op, "oscillation rate max", [0.0; 3])],
                freq: [
                    vec_or(op, "oscillation frequency min", [1.0; 3]),
                    vec_or(op, "oscillation frequency max", [1.0; 3]),
                ],
                proportional: b(op, "proportional 0/1", true),
                t0: (f(op, "start time min", 0.0) + f(op, "start time max", 0.0)) * 0.5,
                t1: (f(op, "end time min", 1.0) + f(op, "end time max", 1.0)) * 0.5,
                se_prop: b(op, "start/end proportional", true),
                mult: f(op, "oscillation multiplier", 2.0),
                phase: f(op, "oscillation start phase", 0.5),
            },
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
            "ramp scalar linear random" | "ramp scalar spline random" => Op::Ramp {
                field: Field::from_index(i(op, "ramp field", 3)),
                rate: [f(op, "ramp rate min", 0.0), f(op, "ramp rate max", 0.0)],
                t0: (f(op, "start time min", 0.0) + f(op, "start time max", 0.0)) * 0.5,
                t1: (f(op, "end time min", 1.0) + f(op, "end time max", 1.0)) * 0.5,
            },
            "ramp scalar linear simple" | "ramp scalar spline simple" => {
                let rate = f(op, "ramp rate", 0.0);
                Op::Ramp {
                    field: Field::from_index(i(op, "ramp field", 3)),
                    rate: [rate, rate],
                    t0: f(op, "start time", 0.0),
                    t1: f(op, "end time", 1.0),
                }
            }
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
    pub rotation: f32,
    pub sequence: u32,
    pub sequence2: u32,
    pub trail: f32,
    pub flip: bool,
    pub seed: u32,
    base_radius: f32,
    base_color: [f32; 3],
    base_alpha: f32,
    fade_in: f32,
    fade_out: f32,
    rotation_speed: f32,
    pub yaw: f32,
    radius_scale: f32,
    radius_add: f32,
    alpha_add: f32,
    color_add: [f32; 3],
    born: f32,
}

#[derive(Clone, Debug)]
pub struct Sim {
    plan: Arc<Plan>,
    pub particles: Vec<Particle>,
    time: f32,
    acc: Vec<f32>,
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
}

impl Sim {
    pub fn new(plan: Arc<Plan>, seed: u32) -> Sim {
        let n = plan.emitters.len();
        let colors = vec![None; plan.inits.len()];
        let path_steps = vec![(0.0, false); plan.inits.len()];
        let point_traces = vec![control::PointTraceState::default(); plan.point_traces.len()];
        Sim {
            plan,
            particles: Vec::new(),
            time: 0.0,
            acc: vec![0.0; n],
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
        self.born = 0;
        self.previous_points = None;
        self.point_traces.fill(control::PointTraceState::default());
        self.colors.fill(None);
        self.path_steps.fill((0.0, false));
        self.previous_models.clear();
        self.acc.iter_mut().for_each(|a| *a = 0.0);
        self.fired.iter_mut().for_each(|f| *f = false);
    }

    pub fn finished(&self) -> bool {
        self.particles.is_empty() && self.plan.emission_end().is_some_and(|end| self.time > end)
    }

    fn sphere(&mut self, part: &mut Particle, points: &Points, init: &Init) {
        let Init::Sphere { cp, dmin, dmax, bias, abs, smin, smax, sexp, lmin, lmax, spread } = init else {
            return;
        };
        let center = if *spread && points.highest() > 0 {
            points.get((self.rng.integer(0, points.highest() as u32) as usize) as i32)
        } else {
            points.get(*cp)
        };
        let (mut d, l) = self.rng.in_sphere();
        for k in 0..3 {
            if abs[k] {
                d[k] = d[k].abs();
            }
        }
        let d = mul(d, *bias);
        let n = length(d);
        let dir = if n > 1e-6 { scale(d, 1.0 / n) } else { [0.0, 0.0, 1.0] };
        part.pos = add(center, scale(dir, lerp(*dmin, *dmax, l)));
        let speed = self.rng.range_exp(*smin, *smax, *sexp);
        part.vel = add(scale(dir, speed), points.local(self.rng.range3(*lmin, *lmax)));
    }

    fn spawn(&mut self, points: &Points, parents: &[(V3, V3)], dt: f32, world: &dyn Environment) {
        let plan = self.plan.clone();
        let origin = [0.0; 3];
        let seed = self.seed.wrapping_add(self.born);
        let mut part = Particle {
            dead: false,
            previous_pos: origin,
            previous_dt: 0.0,
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
            rotation: plan.rotation,
            sequence: plan.sequence,
            sequence2: 0,
            trail: 0.1,
            flip: false,
            seed,
            base_radius: plan.radius,
            base_color: plan.color,
            base_alpha: plan.alpha,
            fade_in: 0.0,
            fade_out: 0.0,
            rotation_speed: plan.rotation_speed,
            yaw: 0.0,
            radius_scale: 1.0,
            radius_add: 0.0,
            alpha_add: 0.0,
            color_add: [0.0; 3],
            born: self.time,
        };
        let mut pre_age = 0.0f32;
        for (index, init) in plan.inits.iter().enumerate() {
            match init {
                Init::RayEnvironment(cull) => rays::initialize(cull, &mut part, dt, world),
                Init::Impact(impact) => part.life = spatial::impact_lifetime(impact, &part, world),
                Init::Scalar(remap) => {
                    if let Some(value) = scalar::evaluate(remap, &part, points, world, self.time, false) {
                        set_field(&mut part, remap.field, value, false);
                    }
                }
                Init::InheritVelocity { cp, scale: amount } => {
                    if dt > 0.0 {
                        let previous = self.previous_points.as_ref().unwrap_or(points);
                        part.vel = add(part.vel, scale(sub(points.get(*cp), previous.get(*cp)), amount / dt));
                    }
                }
                Init::Warp(warp) => control::apply_warp(warp, &mut part, points, self.time, &mut self.rng),
                Init::Sphere { .. } => self.sphere(&mut part, points, init),
                Init::Box { cp, min, max } => part.pos = add(points.get(*cp), self.rng.range3(*min, *max)),
                Init::Offset { min, max, local, radius } => {
                    let mut o = self.rng.range3(*min, *max);
                    if *radius {
                        o = scale(o, part.base_radius);
                    }
                    if *local {
                        o = points.local(o);
                    }
                    part.pos = add(part.pos, o);
                }
                Init::Ring { cp, radius, thickness, smin, smax, even } => {
                    let t = if *even { (self.born % 32) as f32 / 32.0 } else { self.rng.next() };
                    let a = t * std::f32::consts::TAU;
                    let r = radius + self.rng.range(-0.5, 0.5) * thickness;
                    let out = points.local([a.cos(), a.sin(), 0.0]);
                    part.pos = add(points.get(*cp), scale(out, r));
                    part.vel = add(part.vel, scale(out, self.rng.range(*smin, *smax)));
                }
                Init::Path { start, end, max_dist, bulge, bulge_control, mid, steps, looping } => {
                    let a = points.get(*start);
                    let z = points.get(*end);
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
                    let middle = path::middle(points, *start, *end, *mid, *bulge, *bulge_control, self.path_bulge);
                    let p = lerp3(lerp3(a, middle, t), lerp3(middle, z, t), t);
                    part.pos = add(p, self.rng.range3([-*max_dist; 3], [*max_dist; 3]));
                }
                Init::Model(op) => model::initialize(op, &mut part, points, world, &mut self.rng),
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
                Init::Radius { min, max, exp } => part.base_radius = self.rng.range_exp(*min, *max, *exp),
                Init::Color(color) => {
                    color::initialize(color, &mut part, points, world, &mut self.rng, &mut self.colors[index]);
                }
                Init::Alpha { min, max } => part.base_alpha = self.rng.range(*min, *max),
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
                    if *second {
                        part.sequence2 = value;
                    } else {
                        part.sequence = value;
                    }
                }
                Init::TrailLength { min, max } => part.trail = self.rng.range(*min, *max),
                Init::Velocity { smin, smax, lmin, lmax } => {
                    let d = self.rng.dir();
                    part.vel = add(
                        part.vel,
                        add(scale(d, self.rng.range(*smin, *smax)), points.local(self.rng.range3(*lmin, *lmax))),
                    );
                }
                Init::Noise { min, max, local } => {
                    let n = self.rng.range3(*min, *max);
                    part.vel = add(part.vel, if *local { points.local(n) } else { n });
                }
                Init::NoiseScalar { field, min, max } => {
                    let x = self.rng.range(*min, *max);
                    set_field(&mut part, *field, x, false);
                }
                Init::Remap { input, imin, imax, output, omin, omax, multiply, within } => {
                    let x = get_field(&part, *input);
                    if *within && (x < imin.min(*imax) || x > imin.max(*imax)) {
                        continue;
                    }
                    let k = if (imax - imin).abs() > 1e-6 { ((x - imin) / (imax - imin)).clamp(0.0, 1.0) } else { 0.0 };
                    set_field(&mut part, *output, lerp(*omin, *omax, k), *multiply);
                }
                Init::PreAge { min, max } => pre_age = self.rng.range(*min, *max),
            }
        }
        for op in &plan.ops {
            match op {
                Op::FadeIn { min, max, .. } => part.fade_in = self.rng.range(*min, *max),
                Op::FadeOut { min, max, .. } => part.fade_out = self.rng.range(*min, *max),
                _ => {}
            }
        }
        if !part.life.is_finite() || part.life < 0.0 {
            part.life = 1.0;
        }
        if pre_age > 0.0 && part.life > 0.0 {
            part.age = pre_age.clamp(0.0, 0.95) * part.life;
        }
        part.radius = part.base_radius;
        part.color = part.base_color;
        part.alpha = part.base_alpha;
        for field in [
            Field::Lifetime,
            Field::Radius,
            Field::Rotation,
            Field::RotationSpeed,
            Field::Alpha,
            Field::CreationTime,
            Field::Sequence,
            Field::Sequence2,
            Field::TrailLength,
            Field::Yaw,
        ] {
            part.initial_values[field as usize] = get_field(&part, field);
        }
        self.born = self.born.wrapping_add(1);
        self.particles.push(part);
    }

    pub fn step(&mut self, dt: f32, origin: V3, budget: usize) {
        self.step_at(dt, &Points::unparented(origin), &[], budget);
    }

    pub fn step_at(&mut self, dt: f32, points: &Points, parents: &[(V3, V3)], budget: usize) {
        self.step_in(dt, points, parents, budget, &EmptyEnvironment);
    }

    pub fn step_in(&mut self, dt: f32, points: &Points, parents: &[(V3, V3)], budget: usize, world: &dyn Environment) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        let cap = self.plan.max.min(budget);
        let t0 = self.time;
        let t1 = t0 + dt;
        let mut to_emit = 0usize;
        if t0 == 0.0 && self.fired.iter().all(|f| !f) && self.born == 0 {
            to_emit += self.plan.initial;
        }
        let previous_models = std::mem::take(&mut self.previous_models);
        let frame = model::Frame::new(world, &previous_models);
        let world = &frame;
        let plan = self.plan.clone();
        let mut generated = std::borrow::Cow::Borrowed(points);
        for (op, state) in plan.point_traces.iter().zip(&mut self.point_traces) {
            control::update_point_trace(op, state, generated.to_mut(), world, t1);
        }
        let points = &*generated;
        for (i, e) in plan.emitters.iter().enumerate() {
            let window = |start: f32, duration: f32| {
                let end = if duration > 0.0 { start + duration } else { f32::INFINITY };
                (t1.min(end) - t0.max(start)).max(0.0)
            };
            match *e {
                Emit::Continuous { rate, start, duration } => self.acc[i] += rate * window(start, duration),
                Emit::Noise { min, max, speed, start, duration } => {
                    let n = 0.5 + 0.25 * (t1 * speed * 6.1).sin() + 0.25 * (t1 * speed * 2.3 + 1.7).sin();
                    self.acc[i] += lerp(min, max, n) * window(start, duration);
                }
                Emit::Instant { min, max, start } => {
                    if !self.fired[i] && t1 >= start {
                        self.fired[i] = true;
                        to_emit += if max > min { self.rng.integer(min, max) as usize } else { max as usize };
                    }
                }
            }
            if !matches!(e, Emit::Instant { .. }) {
                let n = self.acc[i].floor();
                self.acc[i] -= n;
                to_emit += n as usize;
            }
        }
        let room = cap.saturating_sub(self.particles.len());
        for _ in 0..to_emit.min(room) {
            self.spawn(points, parents, dt, world);
        }
        for part in &mut self.particles {
            part.age += dt;
            operate(&plan, part, points, self.previous_points.as_ref().unwrap_or(points), dt, world, &mut self.rng);
        }
        if plan.decays {
            self.particles.retain(|p| p.age < p.life);
        }
        self.particles.retain(|p| !p.dead && p.pos.iter().all(|c| c.is_finite() && c.abs() < 1.0e6));
        self.time = t1;
        self.previous_points = Some(points.clone());
        self.previous_models = frame.finish();
    }
}

fn get_field(p: &Particle, field: Field) -> f32 {
    match field {
        Field::Lifetime => p.life,
        Field::Radius => p.base_radius,
        Field::Rotation => p.rotation,
        Field::RotationSpeed => p.rotation_speed,
        Field::Alpha => p.base_alpha,
        Field::CreationTime => p.born,
        Field::Sequence => p.sequence as f32,
        Field::Sequence2 => p.sequence2 as f32,
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
        Field::Lifetime => &mut p.life,
        Field::Radius => &mut p.base_radius,
        Field::Rotation => &mut p.rotation,
        Field::RotationSpeed => &mut p.rotation_speed,
        Field::Alpha => &mut p.base_alpha,
        Field::TrailLength => &mut p.trail,
        Field::Yaw => &mut p.yaw,
        Field::Sequence | Field::Sequence2 => {
            let slot = if field == Field::Sequence { &mut p.sequence } else { &mut p.sequence2 };
            let value = if multiply { *slot as f32 * x } else { x };
            *slot = value.clamp(0.0, (MAX_SEQUENCES - 1) as f32) as u32;
            return;
        }
        Field::CreationTime | Field::Other => return,
    };
    *slot = if multiply { *slot * x } else { x };
    p.base_alpha = p.base_alpha.clamp(0.0, 1.0);
    p.base_radius = p.base_radius.max(0.0);
    p.life = p.life.clamp(0.0, 600.0);
}

fn osc_time(p: &Particle, life: f32, proportional: bool, t0: f32, t1: f32, se_prop: bool) -> Option<f32> {
    let window_t = if se_prop { p.age / life } else { p.age };
    if window_t < t0 || window_t > t1 {
        return None;
    }
    Some(if proportional { p.age / life } else { p.age })
}

#[allow(clippy::too_many_arguments)]
fn operate(
    plan: &Plan,
    part: &mut Particle,
    points: &Points,
    previous: &Points,
    dt: f32,
    world: &dyn Environment,
    rng: &mut Rng,
) {
    let life = if part.life > 0.0 { part.life } else { 1.0 };
    let t = (part.age / life).clamp(0.0, 1.0);
    let mut alpha = part.base_alpha;
    let mut alpha_written = false;
    let mut color = part.base_color;
    let mut color_written = false;
    let old = part.pos;
    let mut acceleration = [0.0; 3];
    for (k, op) in plan.ops.iter().enumerate() {
        let key = k as u32 * 7;
        let before_pos = part.pos;
        let before_vel = part.vel;
        match op {
            Op::BoneLock(lock) => model::apply_lock(lock, part, points, dt, world),
            Op::Scalar(remap) => {
                if alpha_written {
                    part.alpha = alpha + part.alpha_add;
                }
                part.radius = (part.base_radius * part.radius_scale + part.radius_add).max(0.0);
                if let Some(value) = scalar::evaluate(remap, part, points, world, part.born + part.age, true) {
                    match remap.field {
                        Field::Alpha => {
                            alpha = value;
                            part.alpha_add = 0.0;
                        }
                        Field::Radius => {
                            part.base_radius = value;
                            part.radius_scale = 1.0;
                            part.radius_add = 0.0;
                        }
                        _ => set_field(part, remap.field, value, false),
                    }
                }
            }
            Op::Lock(lock) => control::apply_lock(lock, part, points, previous, key),
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
            Op::FadeIn { proportional, ease: smooth, .. } => {
                let span = if *proportional { part.fade_in } else { part.fade_in / life };
                if span > 0.0 {
                    let x = (t / span).clamp(0.0, 1.0);
                    alpha *= if *smooth { ease(x) } else { x };
                }
            }
            Op::FadeOut { proportional, ease: smooth, .. } => {
                let span = if *proportional { part.fade_out } else { part.fade_out / life };
                if span > 0.0 {
                    let x = ((1.0 - t) / span).clamp(0.0, 1.0);
                    alpha *= if *smooth { ease(x) } else { x };
                }
            }
            Op::FadeAndDecay { start, end, in0, in1, out0, out1 } => {
                let mut a = 1.0;
                if t < *in1 && in1 > in0 {
                    a = lerp(*start, 1.0, ((t - in0) / (in1 - in0)).clamp(0.0, 1.0));
                } else if t > *out0 && out1 > out0 {
                    a = lerp(1.0, *end, ((t - out0) / (out1 - out0)).clamp(0.0, 1.0));
                }
                alpha *= a.clamp(0.0, 1.0);
            }
            Op::RadiusScale { t0, t1, s0, s1, bias: amount, ease: smooth } => {
                if t >= *t0 && t <= *t1 {
                    let mut x = if t1 > t0 { (t - t0) / (t1 - t0) } else { 1.0 };
                    if *smooth {
                        x = ease(x);
                    }
                    part.radius_scale = lerp(*s0, *s1, bias(x, *amount));
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
            Op::OscScalar { field, rate, freq, proportional, t0, t1, se_prop, mult, phase } => {
                let Some(time) = osc_time(part, life, *proportional, *t0, *t1, *se_prop) else {
                    continue;
                };
                let r = lerp(rate[0], rate[1], hash01(part.seed, key));
                let fr = lerp(freq[0], freq[1], hash01(part.seed, key + 1));
                let s = ((time * fr * mult + phase) * std::f32::consts::TAU).sin() * r * dt;
                match field {
                    Field::Radius => part.radius_add += s,
                    Field::Alpha => part.alpha_add += s,
                    Field::Rotation => part.rotation += s,
                    Field::Yaw => part.yaw += s,
                    _ => {}
                }
            }
            Op::OscVector { color: on_color, rate, freq, proportional, t0, t1, se_prop, mult, phase } => {
                let Some(time) = osc_time(part, life, *proportional, *t0, *t1, *se_prop) else {
                    continue;
                };
                let mut d = [0.0; 3];
                for c in 0..3 {
                    let r = lerp(rate[0][c], rate[1][c], hash01(part.seed, key + c as u32));
                    let fr = lerp(freq[0][c], freq[1][c], hash01(part.seed, key + 3 + c as u32));
                    d[c] = ((time * fr * mult + phase) * std::f32::consts::TAU).sin() * r * dt;
                }
                if *on_color {
                    part.color_add = add(part.color_add, d);
                } else {
                    part.pos = add(part.pos, d);
                }
            }
            Op::NoiseScalar { field, min, max, speed, additive } => {
                let x = noise::map(noise::scalar(scale(part.pos, *speed)), *min, *max, additive.then_some(dt));
                match field {
                    Field::Radius => {
                        let current = part.base_radius * part.radius_scale + part.radius_add;
                        part.base_radius = (x + if *additive { current } else { 0.0 }).max(0.0);
                        part.radius_scale = 1.0;
                        part.radius_add = 0.0;
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
            Op::Ramp { field, rate, t0, t1 } => {
                if t < *t0 || t > *t1 {
                    continue;
                }
                let r = lerp(rate[0], rate[1], hash01(part.seed, key)) * dt;
                match field {
                    Field::Radius => part.radius_add += r,
                    Field::Alpha => part.alpha_add += r,
                    Field::Rotation => part.rotation += r,
                    Field::Yaw => part.yaw += r,
                    _ => {}
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
            Op::FadeIn { .. } | Op::FadeOut { .. } | Op::FadeAndDecay { .. } => true,
            Op::Scalar(remap) => remap.field == Field::Alpha,
            Op::NoiseScalar { field, .. } | Op::OscScalar { field, .. } | Op::Ramp { field, .. } => {
                *field == Field::Alpha
            }
            _ => false,
        };
        if !matches!(
            op,
            Op::Movement { .. } | Op::Spatial(spatial::SpatialOp::Collision { .. }) | Op::NoiseVector { .. }
        ) {
            if part.vel != before_vel {
                part.previous_pos = sub(part.pos, scale(part.vel, part.previous_dt));
            } else if part.pos != before_pos {
                part.previous_pos = add(part.previous_pos, sub(part.pos, before_pos));
            }
        }
    }
    part.alpha = (alpha + part.alpha_add).clamp(0.0, 1.0);
    part.radius = (part.base_radius * part.radius_scale + part.radius_add).max(0.0);
    part.color = add(color, part.color_add).map(|c| c.clamp(0.0, 1.0));
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
            sims.push((delay, Sim::new(plan, seed.wrapping_mul(2_654_435_761).wrapping_add(n))));
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
                for spec in &parent.plan.child_points {
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

    pub fn particle_count(&self) -> usize {
        self.sims.iter().map(|(_, s)| s.particles.len()).sum()
    }
}
