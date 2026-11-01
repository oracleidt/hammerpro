pub mod geom;
mod tool;
mod tr;

#[cfg(test)]
mod tests;

pub use geom::{
    BuildError, CurveError, CurveLayout, CurveSpec, MAX_CURVE_POINTS, MAX_HEIGHT, MAX_SUBDIVISIONS, MAX_THICKNESS,
    MIN_HEIGHT, MIN_SUBDIVISIONS, MIN_THICKNESS, build_brushes, clamp_height, clamp_subdivisions, clamp_thickness,
    layout, snap_point,
};
pub use tool::{CurveOptions, CurvePhase, CurveTool};
pub use tr::CURVE_TR;
