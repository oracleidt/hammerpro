mod bevel;
mod boolean;
mod create;
mod csg;
mod disp;
mod edge_loops;
mod entity;
mod extrude;
mod group;
mod inset;
mod material;
mod transform;

mod find_replace;
mod instance;
mod origins;
mod path_nodes;
mod random_rotate;
mod slice;
mod visgroup;

pub use bevel::{BevelOutcome, EdgeRef};
pub use csg::ClipKeep;
pub use edge_loops::{LoopScope, MAX_LOOP_SOLIDS, RunKind};
pub use entity::EntityQuery;
pub use find_replace::{ConnPart, MAX_TEXT_LEN, TextField, TextMatch, TextQuery, find_ranges};
pub use material::{FaceEdit, ReplaceScope};
pub use random_rotate::{MAX_JITTER_DEG, RandomRotate, RandomRotateReport};
pub use slice::MAX_SLICE_PLANES;
pub use transform::AlignMode;

pub(crate) use create::Fragment;

use glam::DVec3;

pub(crate) fn clean_vec(v: DVec3) -> DVec3 {
    fn c(x: f64) -> f64 {
        if !x.is_finite() {
            return 0.0;
        }
        let r = x.round();
        if (x - r).abs() < 1e-6 { r + 0.0 } else { x + 0.0 }
    }
    DVec3::new(c(v.x), c(v.y), c(v.z))
}

pub(crate) fn solid_color(id: crate::Id) -> [u8; 3] {
    let h = u64::from(id).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    [0, 100 + ((h >> 24) % 156) as u8, 100 + ((h >> 40) % 156) as u8]
}

pub(crate) const ENTITY_COLOR: [u8; 3] = [220, 30, 220];
