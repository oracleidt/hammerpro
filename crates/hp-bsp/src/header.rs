use crate::error::{BspError, Result};

pub const IDENT: [u8; 4] = *b"VBSP";
pub const LUMP_COUNT: usize = 64;
pub const HEADER_LEN: usize = 8 + LUMP_COUNT * 16 + 4;
pub const MIN_VERSION: i32 = 19;
pub const MAX_VERSION: i32 = 21;

pub mod lump {
    pub const ENTITIES: usize = 0;
    pub const TEXDATA: usize = 2;
    pub const VISIBILITY: usize = 4;
    pub const FACES: usize = 7;
    pub const ORIGINAL_FACES: usize = 27;
    pub const LIGHTING: usize = 8;
    pub const LEAFS: usize = 10;
    pub const MODELS: usize = 14;
    pub const WORLDLIGHTS: usize = 15;
    pub const GAME_LUMP: usize = 35;
    pub const PAKFILE: usize = 40;
    pub const CUBEMAPS: usize = 42;
    pub const TEXDATA_STRING_DATA: usize = 43;
    pub const TEXDATA_STRING_TABLE: usize = 44;
    pub const OVERLAYS: usize = 45;
    pub const LIGHTING_HDR: usize = 53;
    pub const WORLDLIGHTS_HDR: usize = 54;
    pub const FACES_HDR: usize = 58;
    pub const DISP_MULTIBLEND: usize = 63;
}

pub const LUMP_NAMES: [&str; LUMP_COUNT] = [
    "entities",
    "planes",
    "texdata",
    "vertexes",
    "visibility",
    "nodes",
    "texinfo",
    "faces",
    "lighting",
    "occlusion",
    "leafs",
    "faceids",
    "edges",
    "surfedges",
    "models",
    "worldlights",
    "leaffaces",
    "leafbrushes",
    "brushes",
    "brushsides",
    "areas",
    "areaportals",
    "propcollision",
    "prophulls",
    "prophullverts",
    "proptris",
    "dispinfo",
    "originalfaces",
    "physdisp",
    "physcollide",
    "vertnormals",
    "vertnormalindices",
    "disp_lightmap_alphas",
    "disp_verts",
    "disp_lightmap_sample_positions",
    "game_lump",
    "leafwaterdata",
    "primitives",
    "primverts",
    "primindices",
    "pakfile",
    "clipportalverts",
    "cubemaps",
    "texdata_string_data",
    "texdata_string_table",
    "overlays",
    "leafmindisttowater",
    "face_macro_texture_info",
    "disp_tris",
    "physcollidesurface",
    "wateroverlays",
    "leaf_ambient_index_hdr",
    "leaf_ambient_index",
    "lighting_hdr",
    "worldlights_hdr",
    "leaf_ambient_lighting_hdr",
    "leaf_ambient_lighting",
    "xzippakfile",
    "faces_hdr",
    "map_flags",
    "overlay_fades",
    "overlay_system_levels",
    "physlevel",
    "disp_multiblend",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LumpLayout {
    #[default]
    Standard,
    VersionFirst,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LumpInfo {
    pub offset: u32,
    pub len: u32,
    pub version: u32,
    pub four_cc: [u8; 4],
}

impl LumpInfo {
    pub fn uncompressed_len(&self) -> Option<u32> {
        let n = u32::from_le_bytes(self.four_cc);
        (n != 0).then_some(n)
    }

    pub fn range(&self) -> std::ops::Range<usize> {
        let start = self.offset as usize;
        start..start + self.len as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: i32,
    pub lumps: [LumpInfo; LUMP_COUNT],
    pub map_revision: i32,
    pub layout: LumpLayout,
}

fn le_i32(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn read_table(data: &[u8], layout: LumpLayout) -> [LumpInfo; LUMP_COUNT] {
    let mut lumps = [LumpInfo::default(); LUMP_COUNT];
    for (i, lump) in lumps.iter_mut().enumerate() {
        let at = 8 + i * 16;
        let a = le_i32(data, at) as u32;
        let b = le_i32(data, at + 4) as u32;
        let c = le_i32(data, at + 8) as u32;
        let four_cc = [data[at + 12], data[at + 13], data[at + 14], data[at + 15]];
        *lump = match layout {
            LumpLayout::Standard => LumpInfo { offset: a, len: b, version: c, four_cc },
            LumpLayout::VersionFirst => LumpInfo { version: a, offset: b, len: c, four_cc },
        };
    }
    lumps
}

fn table_fits(lumps: &[LumpInfo; LUMP_COUNT], file_len: u64) -> bool {
    lumps.iter().all(|l| {
        l.len == 0 || (u64::from(l.offset) >= HEADER_LEN as u64 && u64::from(l.offset) + u64::from(l.len) <= file_len)
    })
}

impl Header {
    pub fn parse(data: &[u8]) -> Result<Header> {
        if data.len() < 4 || data[..4] != IDENT {
            return Err(BspError::NotBsp);
        }
        if data.len() < HEADER_LEN {
            return Err(BspError::corrupt("the file is shorter than the BSP header"));
        }
        let version = le_i32(data, 4);
        if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
            return Err(BspError::UnsupportedVersion(version));
        }
        let file_len = data.len() as u64;
        let standard = read_table(data, LumpLayout::Standard);
        let (lumps, layout) = if table_fits(&standard, file_len) {
            (standard, LumpLayout::Standard)
        } else {
            let swapped = read_table(data, LumpLayout::VersionFirst);
            if version == 21 && table_fits(&swapped, file_len) {
                (swapped, LumpLayout::VersionFirst)
            } else {
                let bad = standard
                    .iter()
                    .position(|l| {
                        l.len != 0
                            && (u64::from(l.offset) < HEADER_LEN as u64
                                || u64::from(l.offset) + u64::from(l.len) > file_len)
                    })
                    .unwrap_or(0);
                return Err(BspError::corrupt(format!(
                    "lump {bad} ({}) lies outside of the file (truncated download?)",
                    LUMP_NAMES[bad]
                )));
            }
        };
        let map_revision = le_i32(data, HEADER_LEN - 4);
        Ok(Header { version, lumps, map_revision, layout })
    }

    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[..4].copy_from_slice(&IDENT);
        out[4..8].copy_from_slice(&self.version.to_le_bytes());
        for (i, l) in self.lumps.iter().enumerate() {
            let at = 8 + i * 16;
            let (a, b, c) = match self.layout {
                LumpLayout::Standard => (l.offset, l.len, l.version),
                LumpLayout::VersionFirst => (l.version, l.offset, l.len),
            };
            out[at..at + 4].copy_from_slice(&a.to_le_bytes());
            out[at + 4..at + 8].copy_from_slice(&b.to_le_bytes());
            out[at + 8..at + 12].copy_from_slice(&c.to_le_bytes());
            out[at + 12..at + 16].copy_from_slice(&l.four_cc);
        }
        out[HEADER_LEN - 4..].copy_from_slice(&self.map_revision.to_le_bytes());
        out
    }
}

pub fn lump_name(index: usize) -> &'static str {
    LUMP_NAMES.get(index).copied().unwrap_or("unknown")
}
