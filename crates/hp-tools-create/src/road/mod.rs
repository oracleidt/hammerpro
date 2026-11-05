pub mod api;
pub mod apply;
pub mod build;
pub mod junction;
pub mod model;
mod preview;
pub mod spline;
mod tool;
mod tr;

#[cfg(test)]
mod tests;

pub use api::{RoadSummary, summarize};
pub use build::{
    LateralSide, LineRole, Notch, PieceRole, Road, RoadEnv, RoadError, RoadPiece, RoadSample, UvFrame, build,
};
pub use model::{Attach, Junction, JunctionEnd, LineKind, RoadMeta, RoadPoint};
pub use tool::{RoadOptions, RoadTool};
pub use tr::ROAD_TR;
