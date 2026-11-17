use std::time::Instant;

use glam::DMat4;
use hp_app::AppState;
use hp_editor::{Id, ObjRef};

use super::{Drag, SelectTool};

pub const LIVE_FRAME_BUDGET_MS: f64 = 12.0;
pub const LIVE_SLOW_FRAMES: u8 = 3;
const LIVE_LABEL: &str = "Live transform";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Live {
    pub(crate) matrix: DMat4,
    object_id: Id,
    side_id: Id,
}

pub(crate) fn live_count(state: &AppState, objs: &[ObjRef]) -> usize {
    let scene = state.doc.scene();
    scene.expand_solids(objs).len() + scene.expand_entities(objs).len()
}

pub(crate) fn wants_live(state: &AppState, objs: &[ObjRef]) -> bool {
    let s = &state.settings.selection;
    s.live_transform && !objs.is_empty() && live_count(state, objs) <= s.live_limit()
}

impl Drag {
    fn live_matrix(&self) -> Option<DMat4> {
        if !self.transforms() || self.elements || self.live_off || self.is_identity() {
            return None;
        }
        Some(self.matrix()).filter(|m| m.is_finite())
    }
}

impl SelectTool {
    pub(crate) fn is_live(&self) -> bool {
        self.live.is_some()
    }

    pub(super) fn end_live(&mut self, state: &mut AppState) {
        let Some(live) = self.live.take() else { return };
        if state.doc.in_transaction() {
            state.doc.cancel();
            state.doc.rewind_id_counters(live.object_id, live.side_id);
        }
        if let Some(d) = self.drag.as_mut() {
            d.revision = state.doc.revision();
        }
    }

    pub(super) fn drop_live(&mut self, state: &mut AppState) {
        self.end_live(state);
        if let Some(d) = self.drag.as_mut() {
            d.live_off = true;
        }
    }

    pub(super) fn sync_live(&mut self, state: &mut AppState) {
        let want = self.drag.as_ref().and_then(Drag::live_matrix);
        match (self.live.as_ref(), want) {
            (None, None) => return,
            (Some(l), Some(m)) if l.matrix == m => return,
            _ => {}
        }
        let started = Instant::now();
        self.end_live(state);
        let Some(m) = want else { return };
        if state.doc.in_transaction() || !self.follow_foreign(state) {
            if let Some(d) = self.drag.as_mut() {
                d.live_off = true;
            }
            return;
        }
        let Some(mut d) = self.drag.take() else { return };
        let (object_id, side_id) = (state.doc.peek_next_object_id(), state.doc.peek_next_side_id());
        state.doc.begin(LIVE_LABEL);
        let applied = self.apply_drag(state, &d, m).is_ok();
        if applied {
            self.live = Some(Live { matrix: m, object_id, side_id });
        } else {
            state.doc.cancel();
            state.doc.rewind_id_counters(object_id, side_id);
            d.live_off = true;
        }
        d.revision = state.doc.revision();
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        d.live_slow = if ms > LIVE_FRAME_BUDGET_MS { d.live_slow.saturating_add(1) } else { 0 };
        let too_slow = d.live_slow >= LIVE_SLOW_FRAMES;
        self.drag = Some(d);
        if too_slow {
            self.drop_live(state);
        }
    }
}
