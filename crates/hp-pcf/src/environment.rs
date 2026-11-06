pub type Vector = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trace {
    pub fraction: f32,
    pub normal: Vector,
    pub start_solid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RayEnvironment {
    Precipitation,
    PrecipitationBlocker,
}

impl RayEnvironment {
    pub fn from_name(name: &str) -> Self {
        if name.eq_ignore_ascii_case("PRECIPITATIONBLOCKER") { Self::PrecipitationBlocker } else { Self::Precipitation }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub distance: f32,
    pub normal: Vector,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelHitbox {
    pub minimum: Vector,
    pub maximum: Vector,
    pub transform: [[f32; 4]; 3],
}

pub trait Environment {
    fn model_hitboxes(&self, _object: u32, _set: &str) -> Option<std::sync::Arc<[ModelHitbox]>> {
        None
    }
    fn previous_model_hitboxes(&self, _object: u32, _set: &str) -> Option<std::sync::Arc<[ModelHitbox]>> {
        None
    }
    fn inside_model(&self, _object: u32, _position: Vector) -> bool {
        false
    }

    /// The distance is a ray parameter; the direction is deliberately not renormalized.
    fn trace_environment(
        &self,
        _environment: RayEnvironment,
        _start: Vector,
        _direction: Vector,
        _maximum: f32,
    ) -> Option<RayHit> {
        None
    }

    fn trace(&self, _start: Vector, _end: Vector, _radius: f32, _brush_only: bool) -> Option<Trace> {
        None
    }

    fn wind(&self, _position: Vector) -> Vector {
        [0.0; 3]
    }

    fn lighting(&self, _position: Vector) -> Vector {
        [1.0; 3]
    }
}

pub struct EmptyEnvironment;

impl Environment for EmptyEnvironment {}
