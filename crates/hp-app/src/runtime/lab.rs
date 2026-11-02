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
    pub particles: ParticleReport,
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
    pub light_styles: LightStyleReport,
    pub train_requests: Vec<(Option<String>, Option<bool>)>,
    pub trains: Vec<TrainReport>,
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
            light_styles: LightStyleReport::default(),
            train_requests: Vec::new(),
            trains: Vec::new(),
        }
    }
}

pub const DEFAULT_PLAYER_MODEL: &str = "models/player/group01/male_07.mdl";

pub const DEFAULT_CAMERA_DISTANCE: f64 = 200.0;
