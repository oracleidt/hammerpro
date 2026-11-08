use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpeningStatus {
    pub name: String,
    pub elapsed: Duration,
}

#[derive(Debug, Default)]
pub struct StartupRuntime {
    pub opening: Option<OpeningStatus>,
    pub resources: Option<ResourceLoading>,
    pub cancel_open: bool,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct ResourceLoading {
    pub done: usize,
    pub total: usize,
    pub current: String,
    pub log: Vec<(String, bool)>,
    pub installing: bool,
    pub installed_frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreloadMaterial {
    World,
    Icon,
    Sprite,
    AdditiveSprite,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreloadResource {
    Material { name: String, context: PreloadMaterial },
    Model { name: String, skin: u32, body: u32, animation: Option<String> },
    Sky(String),
    Particle(String),
}

impl PreloadResource {
    pub fn name(&self) -> &str {
        match self {
            Self::Material { name, .. } | Self::Model { name, .. } | Self::Sky(name) | Self::Particle(name) => name,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreloadResult {
    Pending,
    Ready,
    Missing,
}
