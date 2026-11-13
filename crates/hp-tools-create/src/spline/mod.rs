pub mod apply;
pub mod build;
pub mod curve;
pub mod model;
pub mod store;
mod tool;
mod tr;

#[cfg(test)]
mod compile_tests;
#[cfg(test)]
mod tests;

pub use apply::{FinishReport, Finisher, Generated, PropFinish, SplineError, generate, set_finisher};
pub use build::{EntityRole, GenEnv, GenError, Plan, plan};
pub use model::{CableStyle, FenceStyle, Interp, Mode, ModelAxis, Profile, Sides, SplineMeta, SplinePoint};
pub use store::{SplineEntry, anchor_of, bake, list, read, selected_anchor};
pub use tool::{Part, SplineTool};
pub use tr::SPLINE_TR;
