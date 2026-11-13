use std::time::Instant;

use hp_livelink::{Session, Snapshot};

#[derive(Clone, Debug, PartialEq)]
pub struct FireForm {
    pub target: String,
    pub input: String,
    pub param: String,
    pub delay: f32,
}

impl Default for FireForm {
    fn default() -> Self {
        Self { target: String::new(), input: "Open".to_owned(), param: String::new(), delay: 0.0 }
    }
}

#[derive(Debug, Default)]
pub struct LiveLinkRuntime {
    pub session: Option<Session>,
    pub panel_open: bool,
    pub fire_open: bool,
    pub fire: FireForm,
    pub start_requested: bool,
    pub stop_requested: bool,
    pub resync_requested: bool,
    pub remove_addon_requested: bool,
    pub teleport_requested: bool,
    pub back_requested: bool,
    pub wake: Option<egui::Context>,
    pub current: Option<Snapshot>,
    pub snapshot_revision: u64,
    pub last_snapshot: Option<Instant>,
    pub highlight_sig: u64,
    pub camera_sent: Option<([f64; 3], [f64; 3], Instant)>,
    pub camera_from_game_applied: Option<([f64; 3], [f64; 3])>,
    pub was_compiling: bool,
    pub note: Option<String>,
}

impl LiveLinkRuntime {
    pub fn is_on(&self) -> bool {
        self.session.is_some()
    }

    pub fn is_connected(&self) -> bool {
        self.session.as_ref().is_some_and(Session::is_connected)
    }
}
