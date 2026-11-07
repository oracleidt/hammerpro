use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct LabRuntime {
    pub logic_open: bool,
    pub open_bsp: Option<PathBuf>,
    pub decompiled_from: Option<PathBuf>,
    pub nav_file: Option<PathBuf>,
    pub nav_summary: Option<(PathBuf, usize, usize)>,
    pub decompiling: Option<String>,
    pub player: PlayerState,
    pub unprop_open: bool,
    pub propper_options_open: bool,
    pub propper_create_open: bool,
    pub propper_create_confirmed: bool,
    pub propper_create_request: Option<Box<PropCreationRequest>>,
    pub particles: ParticleReport,
    pub particle_preview: ParticlePreviewRuntime,
    pub fly_texture: FlyTextureRuntime,
    pub sniffer: SnifferRuntime,
    pub sound_color: super::soundcolor::SoundColorRuntime,
}

/// The exact geometry and settings accepted in the prop creation preview.
#[derive(Clone, Debug)]
pub struct PropCreationRequest {
    pub source_serial: u64,
    pub solid_ids: Vec<hp_editor::Id>,
    pub geometry: hp_editor::prop_export::PropGeometry,
    pub options: hp_editor::prop_export::PropOptions,
    pub directives: hp_editor::prop_export::PropDirectives,
    pub model: String,
    pub source_dir: PathBuf,
    pub destination: PathBuf,
    pub surface_prop: String,
    pub lod: crate::settings::ModelLodSettings,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticlePreviewRuntime {
    pub open: bool,
    pub selected: Option<String>,
    pub search: String,
    pub fx_active: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlyTextureRuntime {
    pub active: bool,
    pub view: usize,
    pub target: Option<hp_editor::FaceRef>,
    pub material: String,
    pub key: Option<(glam::DVec3, f64, f64, u64)>,
    pub last: Option<FlyTextureOp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlyTextureOp {
    Apply,
    ApplyBrush,
    Pick,
    Align,
}

#[derive(Debug, Default)]
pub struct SnifferRuntime {
    pub open: bool,
    pub consent_open: bool,
    pub handle: Option<std::sync::Arc<hp_sniffer::session::SnifferHandle>>,
    pub log: Vec<hp_sniffer::session::LogLine>,
    pub log_cursor: usize,
    pub open_vmf: Option<PathBuf>,
    pub open_content: Option<PathBuf>,
    pub opened: Option<PathBuf>,
    pub outcome: Option<std::sync::Arc<hp_sniffer::session::Outcome>>,
    pub error: Option<String>,
    pub show_settings: bool,
    pub show_log: bool,
    pub follow_log: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleReport {
    pub playing: Vec<(u32, String)>,
    pub particles: usize,
    pub loaded: std::collections::BTreeMap<String, String>,
    pub missing: std::collections::BTreeSet<String>,
    pub unsupported: std::collections::BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub active: bool,
    pub start_request: bool,
    pub feet: glam::DVec3,
    pub velocity: glam::DVec3,
    pub on_ground: bool,
    pub crouched: bool,
    pub duck: f64,
    pub third_person: bool,
    pub noclip: bool,
    pub wireframe: u8,
    pub hide_counters: bool,
    pub hide_tools: bool,
    pub editor_world: bool,
    pub model: String,
    pub saved_camera: Option<crate::view::Camera3d>,
    pub camera_distance: f64,
    pub anim: String,
    pub anim_time: f64,
    pub air_time: f64,
    pub in_teleport: bool,
    pub light_requests: Vec<(String, Option<bool>)>,
    pub place_request: Option<PlayerPlace>,
    pub light_styles: LightStyleReport,
    pub train_requests: Vec<(Option<String>, Option<bool>)>,
    pub trains: Vec<TrainReport>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerPlace {
    pub feet: Option<glam::DVec3>,
    pub eye: Option<glam::DVec3>,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub noclip: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrainReport {
    pub id: u32,
    pub name: String,
    pub class: String,
    pub active: bool,
    pub moving: bool,
    pub speed: f64,
    pub max_speed: f64,
    pub origin: glam::DVec3,
    pub angles: glam::DVec3,
    pub node: String,
    pub next: String,
    pub user_control: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightStyleReport {
    pub time: f64,
    pub styles: Vec<(u8, String, f32)>,
    pub lights: Vec<(String, u8, bool, String)>,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            active: false,
            start_request: false,
            feet: glam::DVec3::ZERO,
            velocity: glam::DVec3::ZERO,
            on_ground: false,
            crouched: false,
            duck: 0.0,
            third_person: false,
            noclip: false,
            wireframe: 0,
            hide_counters: false,
            hide_tools: true,
            editor_world: false,
            model: DEFAULT_PLAYER_MODEL.to_owned(),
            saved_camera: None,
            camera_distance: DEFAULT_CAMERA_DISTANCE,
            anim: String::new(),
            anim_time: 0.0,
            air_time: 0.0,
            in_teleport: false,
            light_requests: Vec::new(),
            place_request: None,
            light_styles: LightStyleReport::default(),
            train_requests: Vec::new(),
            trains: Vec::new(),
        }
    }
}

pub const DEFAULT_PLAYER_MODEL: &str = "models/player/group01/male_07.mdl";

pub const DEFAULT_CAMERA_DISTANCE: f64 = 200.0;
