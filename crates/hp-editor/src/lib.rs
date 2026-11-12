#![forbid(unsafe_code)]

mod array;
pub mod background;
mod changes;
mod clipboard;
mod copy_names;
mod curve_meta;
mod document;
mod extra;
mod glob;
mod grids;
mod history;
mod ids;
mod ops;
mod pick;
pub mod placement_fix;
mod problems;
mod roadside_meta;
mod scene;
mod selection;
mod shared_points;
mod stats;
mod visibility;

pub mod compile_export;
pub mod deps_roots;
pub mod describe;
pub mod diff;
pub mod disp_resample;
pub mod display_color;
pub mod duplicates;
pub mod edicts;
pub mod fingerprint;
pub mod gallery;
pub mod instances;
pub mod limits;
pub mod luxels;
pub mod max_distance;
pub mod mesh_disp;
pub mod optimize;
pub mod prefab;
pub mod prop_export;
pub mod side_index;
pub mod sky;
pub mod terrain_ops;
pub mod unprop;

#[cfg(test)]
mod tests;

pub use array::{ArrayAxis, ArrayFrame, ArrayPlan, ArraySpec, MAX_ARRAY_AXIS_COUNT, MAX_ARRAY_COPIES};
pub use changes::Changes;
pub use clipboard::{PREVIEW_MAX_LINES, PastePreview, PasteSpecial};
pub use curve_meta::{CURVE_KEY, CurveRecord, editor_curve_tag};
pub use document::{Document, EditOptions, ManifestLink, SaveTicket, SnapshotJob};
pub use glob::{glob_match, glob_replace};
pub use grids::{GRIDS_BLOCK, MAX_GRID_NAME, OBJECT_GRID_KEY, clean_grid_name, editor_grid};
pub use history::{FACE_SELECTION_STEP, SELECTION_STEP, UndoLimits};
pub use ids::{IdBuildHasher, IdHasher, IdMap, IdSet};
pub use ops::{AlignMode, BevelOutcome, ClipKeep, EdgeRef, EntityQuery, FaceEdit, ReplaceScope};
pub use ops::{ConnPart, MAX_TEXT_LEN, TextField, TextMatch, TextQuery, find_ranges};
pub use ops::{LoopScope, MAX_LOOP_SOLIDS, MAX_SLICE_PLANES, RunKind};
pub use ops::{MAX_JITTER_DEG, RandomRotate, RandomRotateReport};
pub use pick::{EntityRay, Hit, PickOptions, RectMode};
pub use problems::{
    BrushClassFn, MaterialKind, MaterialKindFn, Problem, ProblemAssets, ProblemContext, Severity, TargetKeyFn,
    parent_name,
};
pub use roadside_meta::{ROADSIDE_KEY, RoadsideRecord, editor_roadside_tag};
pub use scene::{EntityNode, GroupNode, Owner, Scene, SolidNode};
pub use selection::{FaceRef, ObjRef, SelectMode, SelectOp, Selection};
pub use shared_points::{SHARED_UPDATE_LABEL, SharedExport, SharedTouched, SharedUpdate};
pub use stats::MapStats;
pub use visibility::VisgroupState;

pub type Id = hp_vmf::Id;

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("{0}")]
    Vmf(#[from] hp_vmf::VmfError),
    #[error("{0}")]
    Geom(#[from] hp_geom::GeomError),
    #[error("object not found: {0:?}")]
    NotFound(ObjRef),
    #[error("face not found: solid {0}, side {1}")]
    FaceNotFound(Id, usize),
    #[error("group {0} not found")]
    GroupNotFound(Id),
    #[error("visgroup {0} not found")]
    VisgroupNotFound(Id),
    #[error("invalid argument: {0}")]
    Invalid(String),
    #[error("nothing to do: {0}")]
    Nothing(&'static str),
    #[error("the clipboard does not contain map objects")]
    EmptyClipboard,
    #[error("a transaction is open")]
    TransactionOpen,
    #[error("the document has no file path")]
    NoPath,
}

pub type EditResult<T> = Result<T, EditError>;
