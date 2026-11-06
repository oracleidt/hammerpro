pub type Vector = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trace {
    pub fraction: f32,
    pub normal: Vector,
    pub start_solid: bool,
}

pub trait Environment {
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
