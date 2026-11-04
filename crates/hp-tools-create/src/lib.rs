#![forbid(unsafe_code)]

pub mod bevel;
mod block;
mod block_preview;
mod block_structures;
mod clip;
mod clip3d;
mod common;
mod cordon;
pub mod curve;
mod decal;
mod entity;
mod extrude;
mod overlay_tool;
pub mod path;
mod picker;
mod polygon;
pub mod road;
mod tr;

pub mod model_bvh;
pub mod prefab_place;
pub mod roadside;
mod sprinkle;
pub mod sprinkle_def;

#[cfg(test)]
mod fuzz3d;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_support;

pub use bevel::{BEVEL_TR, BevelOptions, BevelTool};
pub use block::{BlockOptions, BlockTool, CreateAs, DEFAULT_DEPTH, Primitive, new_depth_range};
pub use clip::{ClipTool, clip_plane_2d, next_keep};
pub use clip3d::{CutAxis, SnapKind, auto_axis, axis_plane, face_cut_plane};
pub use cordon::{CORDON_TR, CordonTool, target_cordon};
pub use curve::{CURVE_TR, CurveTool};
pub use decal::{DECAL_SURFACE_OFFSET, DecalTool, decal_basis, decal_origin};
pub use entity::{EntityTool, align_to_normal, is_prop_class, rest_on_surface};
pub use extrude::{ExtrudeOptions, ExtrudeTool, MAX_EXTRUDE_DEPTH};
pub use overlay_tool::{OverlayTool, overlay_basis, overlay_keyvalues};
pub use path::{PATH_TR, PathTool};
pub use polygon::{MAX_POLYGON_POINTS, PointCheck, PolygonOptions, PolygonTool, check_point, is_convex_outline};
pub use prefab_place::{PREFAB_TR, insert_prefab};
pub use road::{ROAD_TR, RoadTool};
pub use sprinkle::{SPRINKLE_TR, SprinkleTool, user_sprinkle_dir};
pub use tr::TR;

pub fn register_translations() {
    hp_app::i18n::register(TR);
    hp_app::i18n::register(CORDON_TR);
    hp_app::i18n::register(SPRINKLE_TR);
    hp_app::i18n::register(PREFAB_TR);
    hp_app::i18n::register(PATH_TR);
    hp_app::i18n::register(CURVE_TR);
    hp_app::i18n::register(ROAD_TR);
    hp_app::i18n::register(BEVEL_TR);
}

pub fn tools() -> Vec<Box<dyn hp_app::Tool>> {
    vec![
        Box::new(BlockTool::new()),
        Box::new(PolygonTool::new()),
        Box::new(CurveTool::new()),
        Box::new(EntityTool::new()),
        Box::new(ClipTool::new()),
        Box::new(ExtrudeTool::new()),
        Box::new(BevelTool::new()),
        Box::new(DecalTool::new()),
        Box::new(OverlayTool::new()),
        Box::new(PathTool::new()),
        Box::new(RoadTool::new()),
    ]
}
