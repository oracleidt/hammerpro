#![forbid(unsafe_code)]

mod aabb;
mod angles;
mod bevel;
mod blend;
mod boolean;
mod brush;
mod coverage;
mod csg;
mod disp;
mod edge_net;
mod extrude;
mod hull;
mod inset;
mod pick;
mod plane;
mod primitive;
pub mod roadside;
mod slope;
mod terrain;
mod terrain_gen;
#[cfg(test)]
mod test_support;
mod texture;
mod transform;
mod util;

pub use aabb::{Aabb, Ray};
pub use angles::{
    align_angles_to_normal, align_to_normal, angle_matrix, angles_to_quat, angles_to_vectors, clean_angles,
    matrix_angles, quat_to_angles, transform_angles, transform_angles_mat,
};
pub use bevel::{
    BEVEL_SMOOTHING_GROUP, BevelIssue, BevelKind, BevelParams, BevelResult, MAX_BEVEL_RADIUS, MAX_BEVEL_SEGMENTS,
    MIN_BEVEL_CHORD, MIN_BEVEL_RADIUS, bevel_kinds, bevel_limit, bevel_opening, bevel_segments, bevel_solid,
    brush_edges,
};
pub use blend::{
    BLEND_CHANNELS, BlendFalloff, BlendMode, BlendParams, BlendPart, BlendResult, MAX_BLEND_WIDTH, MIN_BLEND_WIDTH,
    blend_sets,
};
pub use boolean::{UnionPart, intersect_traced, union_pieces};
pub use brush::{
    BASE_POLYGON_HALF_SIZE, FaceGeom, SolidGeom, base_polygon, build_solid, build_solid_from_planes, build_solids,
    clip_polygon, triangulate_fan, triangulate_fan_into, validate_solid,
};
pub use coverage::{CoverageSolid, HiddenFace, hidden_faces};
pub use csg::{
    ClipPiece, MIN_CLIP_PIECE, carve, carve_traced, clip_solid, clip_solid_traced, hollow, hollow_traced, merge_convex,
    merge_exact, offset_side, plane_splits, union_volume,
};
pub use disp::{
    DISP_FLAG_NO_HULL, DISP_FLAG_NO_PHYSICS, DISP_FLAG_NO_RAY, DISP_TAG_BUILDABLE, DISP_TAG_FORCE_BUILDABLE_BIT,
    DISP_TAG_FORCE_BUILDABLE_VAL, DISP_TAG_FORCE_REMOVE_BIT, DISP_TAG_FORCE_WALKABLE_BIT, DISP_TAG_FORCE_WALKABLE_VAL,
    DISP_TAG_WALKABLE, DISPTRI_TAG_BUILDABLE, DISPTRI_TAG_WALKABLE, DispMesh, DispWalk, Falloff, PaintBrush, PaintMode,
    add_height_fn, add_noise, build_disp, compiled_disp_tags, create_disp, create_disp_with_axes, disp_paint,
    disp_tag_state, disp_tri_states, disp_walk, invert_alpha, ray_disp, scale_disp, slope_walk, update_disp_tags,
    vmf_tags_from_compiled,
};
pub use edge_net::{EdgeNet, EdgeRun, NET_EPS, NetSolid, SlideVertex};
pub use extrude::{
    ExtrudeIssue, ExtrudeMode, FaceExtrudeParams, FaceExtrudeResult, FaceFrame, MAX_OUTLINE_POINTS, MIN_EXTRUDE_DEPTH,
    convex_decompose, face_extrude, face_prism, face_thickness, outline_is_simple, path_is_simple, signed_area,
};
pub use hull::{
    ConvexHull, FaceTopology, HullFace, VertexEdit, VertexEditOptions, convex_hull, is_convex, rebuild_from_points,
    rebuild_solid, split_edge, unique_vertices,
};
pub use inset::{
    InsetIssue, InsetParams, MAX_INSET, MIN_INSET, RegionRing, clean_polygon, face_inset, faces_inset, inset_corners,
    inset_polygon, inset_ring, offset_region, region_rings, solids_inset,
};
pub use pick::{ViewAxis, point_segment_distance, point_segment_distance_2d, ray_aabb, ray_solid, ray_triangle};
pub use plane::{Plane, PlaneSide};
pub use primitive::{
    MAX_DOOR_ARCH_SEGMENTS, MAX_PRIMITIVE_SIDES, MAX_SPIRAL_ARC, MAX_STAIR_ANGLE, MAX_STAIR_STEPS, Primitive,
    PrimitiveParams, RiseDirection, hull_solid, make_primitive, step_count,
};
pub use slope::{
    GRADIENT_BANDS, MAX_LIMIT_SWEEPS, MAX_RAMP_DEGREES, PlaneFit, RampPivot, SOURCE_WALKABLE_DEGREES, SlopeField,
    SlopeStats, incline_degrees, slope_band, slope_band_range, triangle_gradient, triangle_inclines,
    triangle_up_angles, up_angle_degrees,
};
pub use terrain::{
    HeightGraph, HydraulicParams, MAX_ITERATIONS, MAX_OCTAVES, NoiseParams, ThermalParams, fractal_noise, perlin,
};
pub use terrain_gen::{
    AutoTexture, FalloffShape, GenParams, HeightField, NoiseBasis, NoiseShape, TerrainPreset, auto_weights,
    generate_field, simplex2, slope_degrees,
};
pub use texture::{
    DEFAULT_TEXTURE_SCALE, Justify, TextureAlign, align_to_view, axes_quality, effective_scale, ensure_valid_axes,
    face_axes, fit_texture, justify, reset_axes, rotate_texture, scale_texture, set_rotation, set_texture_scale,
    shift_texture, texel_bounds, texel_uv, texture_axes_valid, uv_lock, world_axes, wrap_texture,
};
pub use transform::{transform_sides, transform_solid, transform_solid_anchored, translate_sides, translate_solid};
pub use util::{
    plane_points_from_polygon, polygon_area_vector, polygon_center, polygon_normal, snap, snap_vec3, to_hammer_winding,
};

pub const PLANE_EPS: f64 = 0.01;
pub const CLIP_EPS: f64 = 0.001;
pub const WELD_EPS: f64 = 0.001;
pub const NORMAL_EPS: f64 = 1e-6;
pub const MAX_COORD: f64 = 16384.0;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GeomError {
    #[error("degenerate plane: the points are collinear, coincident or not finite")]
    DegeneratePlane,
    #[error("solid has {0} faces, at least 4 are required")]
    TooFewFaces(usize),
    #[error("solid is not closed: its planes do not enclose a finite volume")]
    Unbounded,
    #[error("solid has no volume")]
    NoVolume,
    #[error("geometry contains non finite coordinates")]
    NotFinite,
    #[error("{0} distinct points, at least 4 non coplanar points are required")]
    TooFewPoints(usize),
    #[error("points are collinear or coplanar, there is no 3D convex hull")]
    DegenerateHull,
    #[error("convex hull construction failed: {0}")]
    HullFailed(&'static str),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("transform matrix is singular, not finite or not affine")]
    BadTransform,
    #[error("wall thickness is too large for this solid")]
    WallTooThick,
    #[error("displacements need a face with exactly 4 vertices, got {0}")]
    NotQuad(usize),
    #[error("unsupported displacement power {0} (expected 1 to 4)")]
    BadPower(u32),
}
