pub mod apply;
pub mod meta;
pub mod source;

#[cfg(test)]
mod tests;

pub use apply::{
    ApplyError, Part, Piece, build_pieces, build_solids, create, features, lightmap_of, piece_of, read, replace,
};
pub use meta::{Kind, MAX_SOURCES, Materials, RoadsideMeta};
pub use source::{
    Source, frame_of, resolve, resolve_all, resolve_frame, source_of_piece, sources_of_objects, sources_of_selection,
};
