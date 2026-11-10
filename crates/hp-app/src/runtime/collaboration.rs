use std::collections::VecDeque;
use std::time::Instant;

pub const PALETTE: [[u8; 3]; 8] = [
    [255, 159, 28],
    [61, 184, 255],
    [107, 224, 90],
    [242, 92, 208],
    [255, 225, 77],
    [167, 139, 250],
    [45, 224, 194],
    [255, 170, 200],
];

pub const CONFLICT_COLOR: [u8; 3] = [255, 72, 48];

pub fn preferred_color(name: &str) -> usize {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in name.bytes().map(|byte| byte.to_ascii_lowercase()) {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash as usize % PALETTE.len()
}

pub fn assign_colors(people: &[(u32, &str)]) -> Vec<[u8; 3]> {
    let mut order: Vec<usize> = (0..people.len()).collect();
    order.sort_by_key(|index| people[*index].0);
    let mut taken = [false; PALETTE.len()];
    let mut colors = vec![PALETTE[0]; people.len()];
    for index in order {
        let preferred = preferred_color(people[index].1);
        let slot = (0..PALETTE.len())
            .map(|step| (preferred + step) % PALETTE.len())
            .find(|slot| !taken[*slot])
            .unwrap_or(preferred);
        taken[slot] = true;
        colors[index] = PALETTE[slot];
    }
    colors
}

pub fn color_hex(color: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

#[derive(Clone, Debug, Default)]
pub struct Collaborator {
    pub id: u32,
    pub name: String,
    pub position: [f64; 3],
    pub direction: [f64; 3],
    pub selected: Vec<String>,
    pub latency_ms: u64,
    pub color: [u8; 3],
    pub upload: u32,
    pub download: u32,
    pub round_trip_ms: u32,
    pub editing: bool,
    pub selection_center: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollaborationStats {
    pub host: bool,
    pub session: String,
    pub status: String,
    pub invitation: String,
    pub revision: u64,
    pub pending: bool,
    pub loading: bool,
    pub join_units: u64,
    pub join_total: u64,
    pub join_bytes: u64,
    pub sent_bytes: u64,
    pub received_bytes: u64,
    pub edits_sent: u64,
    pub edits_received: u64,
    pub live_edits: u64,
    pub round_trip_ms: f64,
    pub round_trip_average_ms: f64,
    pub apply_ms: f64,
    pub apply_max_ms: f64,
    pub capture_ms: f64,
    pub capture_max_ms: f64,
    pub last_edit_bytes: u64,
    pub conflicts: usize,
    pub held: usize,
    pub join_ms: f64,
    pub local_only: Vec<String>,
    pub upload: u32,
    pub download: u32,
    pub reconnects: u32,
    pub offline_resent: usize,
    pub resyncs: u32,
    pub script_notices: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatLine {
    pub author: u32,
    pub name: String,
    pub color: [u8; 3],
    pub text: String,
    pub marker: Option<[f64; 3]>,
    pub at: Instant,
    pub own: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub author: u32,
    pub name: String,
    pub color: [u8; 3],
    pub position: [f64; 3],
    pub at: Instant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Highlight {
    pub author: u32,
    pub label: String,
    pub color: [u8; 3],
    pub keys: Vec<String>,
    pub until: Instant,
    pub conflict: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lock {
    pub participant: u32,
    pub name: String,
    pub color: [u8; 3],
    pub objects: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reconnecting {
    pub attempt: u32,
    pub attempts: u32,
    pub next: Option<Instant>,
    pub error: String,
    pub connecting: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    Teleport,
    Look,
    Follow,
    Selection,
    Stop,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CollaborationRequest {
    Host { session: String, nickname: String, password: String, direct: bool, port: u16 },
    Join { invitation: String, session: String, nickname: String, password: String, address: Option<String> },
    Leave,
    SaveRecovery { path: std::path::PathBuf },
    Chat { text: String },
    Ping { position: Option<[f64; 3]> },
    Camera { participant: u32, mode: CameraMode },
    Reconnect,
    StopReconnecting,
}

#[derive(Debug, Default)]
pub struct CollaborationRuntime {
    pub show: bool,
    pub connected: bool,
    pub active_map: bool,
    pub local_id: u32,
    pub can_undo: bool,
    pub can_redo: bool,
    pub participants: Vec<Collaborator>,
    pub stats: CollaborationStats,
    pub requests: Vec<CollaborationRequest>,
    pub following: Option<u32>,
    pub chat: VecDeque<ChatLine>,
    pub unread: usize,
    pub markers: Vec<Marker>,
    pub highlights: Vec<Highlight>,
    pub locks: Vec<Lock>,
    pub reconnecting: Option<Reconnecting>,
    pub overlay_serial: u64,
    pub keep_scripts_local: bool,
}

impl CollaborationRuntime {
    pub fn participant(&self, id: u32) -> Option<&Collaborator> {
        self.participants.iter().find(|peer| peer.id == id)
    }

    pub fn color_of(&self, id: u32) -> [u8; 3] {
        self.participant(id).map_or(PALETTE[id as usize % PALETTE.len()], |peer| peer.color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_distinct_follow_the_nickname_and_ignore_the_row_order() {
        let people =
            [(1, "Ana"), (2, "Bob"), (3, "Cleo"), (9, "Dima"), (10, "Eve"), (11, "Finn"), (12, "Gus"), (13, "Hal")];
        let colors = assign_colors(&people);
        let mut unique = colors.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), people.len(), "every participant gets its own color");
        let reversed: Vec<(u32, &str)> = people.iter().rev().copied().collect();
        let again = assign_colors(&reversed);
        for (index, (id, _)) in people.iter().enumerate() {
            let other = reversed.iter().position(|(value, _)| value == id).unwrap();
            assert_eq!(colors[index], again[other], "participant {id}");
        }
        let alone = assign_colors(&[(4, "Bob")])[0];
        assert_eq!(alone, PALETTE[preferred_color("bob")], "a nickname keeps its colour");
        assert_eq!(assign_colors(&[(1, "Ana"), (27, "BOB")])[1], alone, "reconnecting with a new id keeps the colour");
        assert_eq!(color_hex([255, 0, 16]), "#ff0010");
    }
}
