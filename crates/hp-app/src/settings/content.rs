use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReleaseMode {
    #[default]
    WorkshopAddon,
    SelfContained,
    ServerKit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentSettings {
    pub mode: ReleaseMode,
    pub compile_first: bool,
    pub compress_pakfile: bool,
    pub lzma_level: u32,
    pub pack_workshop: bool,
    pub pack_mounted: bool,
    pub keep_cubemaps: bool,
    pub game_session: bool,
    pub write_gma: bool,
    pub open_folder: bool,
    pub protect: bool,
    pub protect_watermark: String,
    pub protect_strip_original_faces: bool,
    pub protect_lock: bool,
    pub protect_lock_key: String,
    pub protect_lock_block: bool,
    pub protect_randomize: bool,
    pub protect_shuffle_lumps: bool,
    pub protect_junk_gaps: bool,
    pub protect_decoys: bool,
    pub protect_scramble_header: bool,
    pub protect_shuffle_game_lumps: bool,
    pub protect_random_pak: bool,
    pub protect_reorder_tables: bool,
    pub protect_reorder_world: bool,
    pub protect_layout_seed: String,
    pub show_map_assets: bool,
    pub collect_folder: String,
}

impl Default for ContentSettings {
    fn default() -> Self {
        Self {
            mode: ReleaseMode::WorkshopAddon,
            compile_first: true,
            compress_pakfile: false,
            lzma_level: 6,
            pack_workshop: true,
            pack_mounted: false,
            keep_cubemaps: true,
            game_session: false,
            write_gma: true,
            open_folder: false,
            protect: false,
            protect_watermark: String::new(),
            protect_strip_original_faces: true,
            protect_lock: false,
            protect_lock_key: String::new(),
            protect_lock_block: false,
            protect_randomize: true,
            protect_shuffle_lumps: true,
            protect_junk_gaps: true,
            protect_decoys: true,
            protect_scramble_header: true,
            protect_shuffle_game_lumps: true,
            protect_random_pak: true,
            protect_reorder_tables: true,
            protect_reorder_world: true,
            protect_layout_seed: String::new(),
            show_map_assets: false,
            collect_folder: String::new(),
        }
    }
}
