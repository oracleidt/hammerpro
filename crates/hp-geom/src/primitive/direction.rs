use glam::DVec3;

use crate::Aabb;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum RiseDirection {
    #[default]
    PlusY,
    MinusY,
    PlusX,
    MinusX,
    Longest,
}

impl RiseDirection {
    pub const ALL: [RiseDirection; 5] = [
        RiseDirection::PlusY,
        RiseDirection::MinusY,
        RiseDirection::PlusX,
        RiseDirection::MinusX,
        RiseDirection::Longest,
    ];

    pub fn id(self) -> &'static str {
        match self {
            RiseDirection::PlusY => "+y",
            RiseDirection::MinusY => "-y",
            RiseDirection::PlusX => "+x",
            RiseDirection::MinusX => "-x",
            RiseDirection::Longest => "longest",
        }
    }

    pub fn from_id(id: &str) -> Option<RiseDirection> {
        let id = id.trim();
        RiseDirection::ALL.into_iter().find(|d| d.id().eq_ignore_ascii_case(id))
    }

    pub(super) fn resolve(self, size: DVec3) -> RiseDirection {
        match self {
            RiseDirection::Longest if size.x > size.y => RiseDirection::PlusX,
            RiseDirection::Longest => RiseDirection::PlusY,
            other => other,
        }
    }

    pub(super) fn start_angle(self) -> f64 {
        match self {
            RiseDirection::PlusX => 90.0,
            RiseDirection::MinusY => 180.0,
            RiseDirection::MinusX => 270.0,
            _ => 0.0,
        }
    }

    fn along_x(self) -> bool {
        matches!(self, RiseDirection::PlusX | RiseDirection::MinusX)
    }

    pub(super) fn generation_box(self, b: &Aabb) -> Aabb {
        if self.along_x() {
            Aabb::new(DVec3::new(b.min.y, b.min.x, b.min.z), DVec3::new(b.max.y, b.max.x, b.max.z))
        } else {
            *b
        }
    }

    pub(super) fn map_points(self, b: &Aabb, sets: &mut [Vec<DVec3>]) {
        let (sx, sy) = (b.min.x + b.max.x, b.min.y + b.max.y);
        for p in sets.iter_mut().flatten() {
            *p = match self {
                RiseDirection::MinusY => DVec3::new(p.x, sy - p.y, p.z),
                RiseDirection::PlusX => DVec3::new(p.y, p.x, p.z),
                RiseDirection::MinusX => DVec3::new(sx - p.y, p.x, p.z),
                _ => *p,
            };
        }
    }
}
