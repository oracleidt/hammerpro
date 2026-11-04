pub mod apply;
pub mod geom;
pub mod meta;
pub mod section;
mod tool;
mod tr;

#[cfg(test)]
mod tests;

pub use geom::{
    BuildError, CurveError, CurveLayout, CurveSpec, MAX_CURVE_POINTS, MAX_HEIGHT, MAX_SUBDIVISIONS, MAX_THICKNESS,
    MIN_HEIGHT, MIN_SUBDIVISIONS, MIN_THICKNESS, build_brushes, clamp_height, clamp_subdivisions, clamp_thickness,
    layout, nearest_on_spline, snap_point,
};
pub use meta::CurveMeta;
pub use section::{Profile, Section};
pub use tool::{CurveOptions, CurvePhase, CurveTool};
pub use tr::CURVE_TR;
