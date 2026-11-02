#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapLimit {
    pub name: &'static str,
    pub valve: &'static str,
    pub sdk: u64,
    pub gmod: u64,
    pub record: u64,
}

impl MapLimit {
    pub const fn max(&self) -> u64 {
        self.gmod
    }

    pub const fn max_usize(&self) -> usize {
        self.gmod as usize
    }

    pub const fn max_bytes(&self) -> u64 {
        self.gmod * self.record
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LimitSet {
    #[default]
    Gmod,
    Sdk,
}

impl LimitSet {
    pub const fn id(self) -> &'static str {
        match self {
            LimitSet::Gmod => "gmod",
            LimitSet::Sdk => "sdk",
        }
    }

    pub fn from_id(id: &str) -> Option<LimitSet> {
        [LimitSet::Gmod, LimitSet::Sdk].into_iter().find(|s| s.id().eq_ignore_ascii_case(id.trim()))
    }
}

impl MapLimit {
    pub const fn max_in(&self, set: LimitSet) -> u64 {
        match set {
            LimitSet::Gmod => self.gmod,
            LimitSet::Sdk => self.sdk,
        }
    }

    pub const fn max_usize_in(&self, set: LimitSet) -> usize {
        self.max_in(set) as usize
    }

    pub const fn max_bytes_in(&self, set: LimitSet) -> u64 {
        self.max_in(set) * self.record
    }
}

const fn limit(name: &'static str, valve: &'static str, sdk: u64, gmod: u64, record: u64) -> MapLimit {
    MapLimit { name, valve, sdk, gmod, record }
}

pub const MODELS: MapLimit = limit("models", "MAX_MAP_MODELS", 1_024, 4_096, 48);
pub const BRUSHES: MapLimit = limit("brushes", "MAX_MAP_BRUSHES", 8_192, 65_535, 12);
pub const BRUSHSIDES: MapLimit = limit("brushsides", "MAX_MAP_BRUSHSIDES", 65_536, 655_350, 8);
pub const PLANES: MapLimit = limit("planes", "MAX_MAP_PLANES", 65_536, 65_536, 20);
pub const VERTEXES: MapLimit = limit("vertexes", "MAX_MAP_VERTS", 65_536, 65_536, 12);
pub const NODES: MapLimit = limit("nodes", "MAX_MAP_NODES", 65_536, 65_536, 32);
pub const TEXINFO: MapLimit = limit("texinfo", "MAX_MAP_TEXINFO", 12_288, 32_768, 72);
pub const TEXDATA: MapLimit = limit("texdata", "MAX_MAP_TEXDATA", 2_048, 8_192, 32);
pub const DISPINFO: MapLimit = limit("dispinfo", "MAX_MAP_DISPINFO", 2_048, 16_384, 176);
pub const DISP_VERTS: MapLimit = limit("disp_verts", "MAX_MAP_DISP_VERTS", 2_048 * 289, 16_384 * 289, 20);
pub const DISP_TRIS: MapLimit = limit("disp_tris", "MAX_MAP_DISP_TRIS", 2_048 * 512, 16_384 * 512, 2);
pub const FACES: MapLimit = limit("faces", "MAX_MAP_FACES", 65_536, 65_536, 56);
pub const ORIGFACES: MapLimit = limit("origfaces", "MAX_MAP_ORIGFACES", 65_536, 65_536, 56);
pub const LEAFS: MapLimit = limit("leaves", "MAX_MAP_LEAFS", 65_536, 65_536, 32);
pub const LEAFFACES: MapLimit = limit("leaffaces", "MAX_MAP_LEAFFACES", 65_536, 65_536, 2);
pub const LEAFBRUSHES: MapLimit = limit("leafbrushes", "MAX_MAP_LEAFBRUSHES", 65_536, 65_536, 2);
pub const AREAS: MapLimit = limit("areas", "MAX_MAP_AREAS", 256, 1_024, 8);
pub const AREAPORTALS: MapLimit = limit("areaportals", "MAX_MAP_AREAPORTALS", 1_024, 1_024, 12);
pub const PORTALVERTS: MapLimit = limit("portalverts", "MAX_MAP_PORTALVERTS", 128_000, 128_000, 12);
pub const SURFEDGES: MapLimit = limit("surfedges", "MAX_MAP_SURFEDGES", 512_000, 512_000, 4);
pub const EDGES: MapLimit = limit("edges", "MAX_MAP_EDGES", 256_000, 256_000, 4);
pub const VERTNORMALS: MapLimit = limit("vertnormals", "MAX_MAP_VERTNORMALS", 256_000, 256_000, 12);
pub const VERTNORMALINDICES: MapLimit = limit("vertnormalindices", "MAX_MAP_VERTNORMALINDICES", 256_000, 256_000, 2);
pub const LEAFWATERDATA: MapLimit = limit("leafwaterdata", "MAX_MAP_LEAFWATERDATA", 32_768, 32_768, 12);
pub const CUBEMAPSAMPLES: MapLimit = limit("cubemapsamples", "MAX_MAP_CUBEMAPSAMPLES", 1_024, 1_024, 16);
pub const OVERLAYS: MapLimit = limit("overlays", "MAX_MAP_OVERLAYS", 512, 8_192, 352);
pub const WATEROVERLAYS: MapLimit = limit("wateroverlays", "MAX_MAP_WATEROVERLAYS", 16_384, 16_384, 1_120);
pub const OVERLAY_FACES: MapLimit = limit("overlay faces", "OVERLAY_BSP_FACE_COUNT", 64, 64, 0);
pub const WATEROVERLAY_FACES: MapLimit = limit("water overlay faces", "WATEROVERLAY_BSP_FACE_COUNT", 256, 256, 0);
pub const WORLDLIGHTS: MapLimit = limit("worldlights", "MAX_MAP_WORLDLIGHTS", 8_192, 8_192, 88);
pub const ENTITIES: MapLimit = limit("entities", "MAX_MAP_ENTITIES", 8_192, 65_535, 0);
pub const ENTSTRING: MapLimit = limit("entdata", "MAX_MAP_ENTSTRING", 384 * 1_024, 0x40_0000, 0);
pub const VISIBILITY: MapLimit = limit("visdata", "MAX_MAP_VISIBILITY", 0x100_0000, 0x100_0000, 0);
pub const LUMP_MAX_BYTES: u64 = i32::MAX as u64;
pub const LIGHTING: MapLimit = limit("lightdata", "dface_t lightofs", LUMP_MAX_BYTES, LUMP_MAX_BYTES, 0);
pub const STATIC_PROPS: MapLimit = limit("static props", "MAX_STATIC_PROPS", 65_535, 16_384, 0);
pub const SWITCHED_LIGHTS: MapLimit = limit("switched lights", "MAX_SWITCHED_LIGHTS", 32, 96, 0);
pub const PRIMITIVES: MapLimit = limit("primitives", "MAX_MAP_PRIMITIVES", 32_768, 32_768, 10);
pub const PRIMINDICES: MapLimit = limit("primindices", "MAX_MAP_PRIMINDICES", 65_536, 65_536, 2);
pub const PHYSCOLLIDE: MapLimit = limit("physics", "MAX_MAP_PHYSCOLLIDE", 0, 0, 0);
pub const PHYSDISP: MapLimit = limit("physics terrain", "MAX_MAP_PHYSDISP", 0, 0, 0);
pub const LIGHTMAP_EXTENT: MapLimit = limit("lightmap extent", "MAX_LIGHTMAP_DIM_WITHOUT_BORDER + 1", 33, 126, 0);

pub const ALL: &[MapLimit] = &[
    MODELS,
    BRUSHES,
    BRUSHSIDES,
    PLANES,
    VERTEXES,
    NODES,
    TEXINFO,
    TEXDATA,
    DISPINFO,
    DISP_VERTS,
    DISP_TRIS,
    FACES,
    ORIGFACES,
    LEAFS,
    LEAFFACES,
    LEAFBRUSHES,
    AREAS,
    AREAPORTALS,
    PORTALVERTS,
    SURFEDGES,
    EDGES,
    VERTNORMALS,
    VERTNORMALINDICES,
    LEAFWATERDATA,
    CUBEMAPSAMPLES,
    OVERLAYS,
    WATEROVERLAYS,
    OVERLAY_FACES,
    WATEROVERLAY_FACES,
    WORLDLIGHTS,
    ENTITIES,
    ENTSTRING,
    VISIBILITY,
    LIGHTING,
    STATIC_PROPS,
    SWITCHED_LIGHTS,
    PRIMITIVES,
    PRIMINDICES,
    PHYSCOLLIDE,
    PHYSDISP,
    LIGHTMAP_EXTENT,
];

pub fn by_name(name: &str) -> Option<&'static MapLimit> {
    ALL.iter().find(|l| l.name.eq_ignore_ascii_case(name))
}

pub fn by_valve(valve: &str) -> Option<&'static MapLimit> {
    ALL.iter().find(|l| l.valve.eq_ignore_ascii_case(valve))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_found() {
        for (i, a) in ALL.iter().enumerate() {
            assert!(ALL[i + 1..].iter().all(|b| b.name != a.name && b.valve != a.valve), "{} twice", a.name);
            assert_eq!(by_name(a.name), Some(a));
            assert_eq!(by_valve(a.valve), Some(a));
        }
        assert_eq!(by_valve("max_map_brushes").map(MapLimit::max), Some(65_535));
    }

    #[test]
    fn every_game_reads_its_own_column() {
        assert_eq!(BRUSHES.max_in(LimitSet::Gmod), 65_535);
        assert_eq!(BRUSHES.max_in(LimitSet::Sdk), 8_192);
        assert_eq!(MODELS.max_usize_in(LimitSet::Sdk), 1_024);
        assert_eq!(SWITCHED_LIGHTS.max_in(LimitSet::Sdk), 32);
        assert_eq!(SWITCHED_LIGHTS.max_in(LimitSet::Gmod), 96);
        assert_eq!(LIGHTMAP_EXTENT.max_in(LimitSet::Sdk), 33);
        assert_eq!(VERTEXES.max_bytes_in(LimitSet::Sdk), 65_536 * 12);
        for l in ALL {
            assert_eq!(l.max_in(LimitSet::Gmod), l.max(), "{}", l.name);
        }
        assert_eq!(LimitSet::from_id(" SDK "), Some(LimitSet::Sdk));
        assert_eq!(LimitSet::from_id("gmod"), Some(LimitSet::Gmod));
        assert_eq!(LimitSet::from_id("x"), None);
        assert_eq!(LimitSet::default(), LimitSet::Gmod);
    }

    #[test]
    fn gmod_raised_what_its_binaries_say() {
        assert_eq!((MODELS.max(), BRUSHES.max(), BRUSHSIDES.max(), DISPINFO.max()), (4_096, 65_535, 655_350, 16_384));
        assert_eq!((TEXINFO.max(), TEXDATA.max(), OVERLAYS.max(), AREAS.max()), (32_768, 8_192, 8_192, 1_024));
        assert_eq!((VERTEXES.max(), PLANES.max(), FACES.max(), EDGES.max()), (65_536, 65_536, 65_536, 256_000));
        assert_eq!(LIGHTMAP_EXTENT.max(), 126);
        assert_eq!(VERTEXES.max_bytes(), 65_536 * 12);
        for l in ALL {
            assert!(l.gmod >= l.sdk || l.gmod == 0 || *l == STATIC_PROPS, "{}", l.name);
        }
    }

    #[test]
    fn lightmaps_are_only_bounded_by_the_face_offset() {
        for set in [LimitSet::Gmod, LimitSet::Sdk] {
            assert_eq!(LIGHTING.max_in(set), i32::MAX as u64);
            assert!(LIGHTING.max_in(set) > 81_214_012, "Valve compiled maps load with 81 MB of lightmaps");
        }
    }
}
