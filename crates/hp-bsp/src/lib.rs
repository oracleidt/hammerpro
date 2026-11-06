mod bsp;
pub mod cubemaps;
mod edit;
pub mod entities;
mod error;
mod facts;
pub mod format;
pub mod gamelump;
mod header;
pub mod layout;
pub mod limits;
pub mod lzma;
pub mod nav;
pub mod pak;
pub mod physics;
pub mod protect;
pub mod rename;
pub mod reorder;
mod rng;
pub mod staticprops;

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod protect_tests;
#[cfg(test)]
mod tests;

pub use bsp::Bsp;
pub use edit::BspEdit;
pub use entities::{Entity, parse_entities, write_entities};
pub use error::{BspError, Result};
pub use facts::LimitUsage;
pub use format::{Branch, BspFormat};
pub use gamelump::{GameLump, GameLumps};
pub use header::{
    HEADER_LEN, Header, IDENT, LUMP_COUNT, LUMP_NAMES, LumpInfo, LumpLayout, MAX_VERSION, MIN_VERSION, lump, lump_name,
};
pub use layout::{LayoutOptions, LayoutReport, relayout};
pub use pak::{PakArchive, PakCompression, PakEntry, PakOptions, PakWriter, normalize_entry_name};
pub use protect::{LOCK_LUA, LOCK_LUA_PATH, Lock, ProtectOptions, ProtectReport, lock_hash, protect, server_copy};
pub use reorder::{ReorderOptions, ReorderReport};
pub use rng::Rng;

pub fn synthetic_bsp(version: i32, lumps: &[(usize, Vec<u8>)]) -> Vec<u8> {
    let mut header =
        Header { version, lumps: [LumpInfo::default(); LUMP_COUNT], map_revision: 1, layout: LumpLayout::Standard };
    let mut body = Vec::new();
    let mut sorted: Vec<&(usize, Vec<u8>)> = lumps.iter().filter(|(i, _)| *i < LUMP_COUNT).collect();
    sorted.sort_by_key(|(i, _)| (*i == lump::PAKFILE, *i));
    for (i, data) in sorted {
        while !(HEADER_LEN + body.len()).is_multiple_of(4) {
            body.push(0);
        }
        let offset = (HEADER_LEN + body.len()) as u32;
        let data = if *i == lump::GAME_LUMP {
            GameLumps::parse(data, 0).and_then(|g| g.to_bytes(offset)).unwrap_or_else(|_| data.clone())
        } else {
            data.clone()
        };
        header.lumps[*i] = LumpInfo { offset, len: data.len() as u32, version: 0, four_cc: [0; 4] };
        body.extend_from_slice(&data);
    }
    let mut out = header.to_bytes().to_vec();
    out.extend_from_slice(&body);
    out
}
