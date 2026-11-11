mod analyze;
mod font;
mod grid;
mod intersect;
mod mesh;
mod model;
mod raster;
mod sweep;

pub use analyze::{
    ContactPatch, HeightMap, OrientedBox, OrientedRect, Slice, contact_patch, footprint, height_map, hull_2d,
    min_area_rect, oriented_box, polygon_area, slices,
};
pub use intersect::{Overlap, contains_point, overlap, separation, touches_box};
pub use mesh::{TriMesh, round_to};
pub use model::{Attachment, ModelShape, attachments, collision_pieces, load_shape, render_mesh};
pub use raster::{Image, Layer, Marker, SheetOptions, ViewKind, render_sheet};
pub use sweep::{Contact, ContactKind, Frame, RayHit, exit_depth, ray_cast, sweep};

#[cfg(test)]
mod tests;
