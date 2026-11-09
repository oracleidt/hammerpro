#[derive(Clone, Debug, Default)]
pub struct Collaborator {
    pub id: u32,
    pub name: String,
    pub position: [f64; 3],
    pub direction: [f64; 3],
    pub selected: Vec<String>,
    pub latency_ms: u64,
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
}
