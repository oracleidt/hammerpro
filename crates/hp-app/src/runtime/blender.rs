use std::path::PathBuf;
use std::time::Instant;

use crate::runtime::FileStamp;
use crate::settings::BlenderFormat;

#[derive(Clone, Debug, PartialEq)]
pub struct DispSent {
    pub solid: u32,
    pub side: u32,
    pub power: u32,
    pub positions: Vec<[f32; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlenderTarget {
    Brushes { solids: Vec<u32> },
    Prop { props: Vec<u32>, model: String, name: String },
    Displacements { faces: Vec<DispSent> },
}

impl BlenderTarget {
    pub fn id(&self) -> &'static str {
        match self {
            BlenderTarget::Brushes { .. } => "brushes",
            BlenderTarget::Prop { .. } => "prop",
            BlenderTarget::Displacements { .. } => "displacements",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlenderLink {
    pub file: PathBuf,
    pub format: BlenderFormat,
    pub target: BlenderTarget,
    pub baseline: FileStamp,
    pub candidate: Option<(FileStamp, Instant)>,
    pub pulls: u32,
    pub live: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlenderNote {
    pub ok: bool,
    pub text: String,
}

impl BlenderRuntime {
    pub fn blender_running(&mut self) -> bool {
        match self.child.as_mut().map(std::process::Child::try_wait) {
            Some(Ok(None)) => true,
            Some(_) => {
                self.child = None;
                false
            }
            None => false,
        }
    }
}

#[derive(Debug, Default)]
pub struct BlenderRuntime {
    pub link: Option<BlenderLink>,
    pub send_requested: bool,
    pub pull_requested: bool,
    pub stop_requested: bool,
    pub install_requested: bool,
    pub browse_requested: bool,
    pub detect_requested: bool,
    pub missing_open: bool,
    pub exe: Option<PathBuf>,
    pub exe_checked: bool,
    pub note: Option<BlenderNote>,
    pub pull_in_progress: bool,
    pub launched: u32,
    pub last_check: Option<Instant>,
    pub work_dir: Option<PathBuf>,
    pub child: Option<std::process::Child>,
}
