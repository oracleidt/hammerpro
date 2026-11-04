mod build;
mod error;
mod junction;
mod profile;
mod spine;
mod sweep;
mod terrain;

pub use build::{Feature, MAX_GAP, MAX_RADIUS, build, split_crossings};
pub use error::RoadsideError;
pub use junction::{Corner, MIN_JOIN_ANGLE, Plan, Sides, keep_ranges, plan};
pub use profile::{
    CurbProfile, GutterProfile, GutterShape, MAX_DEPTH, MAX_HEIGHT, MAX_WIDTH, Poly, Profile, SidewalkProfile,
};
pub use spine::{Frame, MIN_STRIP, Nearest, Spine};
pub use sweep::{MIN_EDGE, mitre_vectors, sweep};
pub use terrain::{
    AlphaMode, BlendShape, HeightMode, MAX_MARGIN, MAX_SHOULDER, MAX_SINK, RoadField, Sample, ShoulderFalloff,
};
