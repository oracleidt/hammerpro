#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActiveSoundscape {
    pub entity: u32,
    pub class: String,
    pub name: String,
    pub kept: bool,
    pub dsp: Option<i32>,
    pub by_trigger: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreviewRequest {
    Play { name: String, volume: f32 },
    Stop,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordRequest {
    pub path: std::path::PathBuf,
    pub seconds: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RecordState {
    Recording { path: std::path::PathBuf, seconds: f32 },
    Written { path: std::path::PathBuf, seconds: f32, peak: f32, rms: [f32; 2] },
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropHull {
    pub id: u32,
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub tris: Vec<[[f64; 3]; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropHulls {
    pub key: Option<(u64, u64)>,
    pub bodies: Vec<PropHull>,
    pub waiting: usize,
}

#[derive(Debug, Default)]
pub struct SoundscapeRuntime {
    pub active: Option<ActiveSoundscape>,
    pub hearing_voices: usize,
    pub hearing_ambient: usize,
    pub hearing_note: Option<String>,
    pub dsp: (i32, i32),
    pub triggers: Vec<u32>,
    pub preview: Option<String>,
    pub preview_request: Option<PreviewRequest>,
    pub record_request: Option<RecordRequest>,
    pub record: Option<RecordState>,
    pub library_serial: u64,
    pub unsaved: std::collections::BTreeMap<String, String>,
    pub select: Option<String>,
    pub props: std::sync::Arc<PropHulls>,
    pub io: Vec<(u32, String, String)>,
}
