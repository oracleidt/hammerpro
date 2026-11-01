use egui::Modifiers;
use hp_app::{Action, AppState, Severity, ToolFlow, ToolMenuEntry, ViewInfo, t, tf};
use hp_editor::{Id, ObjRef};

use super::SelectTool;

const WHEEL_SCALE_STEP: f64 = 1.1;
const MODEL_SCALE_RANGE: (f64, f64) = (0.01, 100.0);

const CONTEXT_ACTIONS: [Action; 6] = [
    Action::MoveToFloor,
    Action::MoveToCeiling,
    Action::MoveToFloorAligned,
    Action::MoveToCeilingAligned,
    Action::CenterOrigins,
    Action::TogglePivot,
];

pub(super) fn context_entries(state: &AppState) -> Vec<ToolMenuEntry> {
    if state.doc.selection().objects.is_empty() {
        return Vec::new();
    }
    CONTEXT_ACTIONS
        .iter()
        .enumerate()
        .filter(|(_, a)| a.is_ready())
        .filter(|(_, a)| **a != Action::CenterOrigins || a.enabled(state))
        .map(|(i, a)| ToolMenuEntry { label: a.label().to_owned(), id: i as u32, enabled: a.enabled(state) })
        .collect()
}

pub(super) fn context_command(state: &mut AppState, id: u32) {
    if let Some(a) = CONTEXT_ACTIONS.get(id as usize) {
        state.request(*a);
    }
}

pub(super) fn path_link(state: &AppState, objs: &[ObjRef]) -> Option<(Id, &'static str)> {
    hp_app::runtime::helpers::path_clone_link(state, objs)
}

fn scale_key(state: &AppState, class: &str) -> Result<Option<&'static str>, ()> {
    let lower = class.to_ascii_lowercase();
    let fgd_has =
        |key: &str| state.assets.fgd().and_then(|f| f.class(class)).is_some_and(|c| c.property(key).is_some());
    if lower.starts_with("prop_static") || lower == "prop_detail" {
        return if fgd_has("uniformscale") { Ok(Some("uniformscale")) } else { Err(()) };
    }
    if lower.starts_with("prop_dynamic") || lower.starts_with("prop_physics") || fgd_has("modelscale") {
        return Ok(Some("modelscale"));
    }
    Ok(None)
}

fn fmt_scale(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    format!("{r}")
}

impl SelectTool {
    pub(super) fn wheel_scale(
        &mut self,
        state: &mut AppState,
        _view: &ViewInfo,
        notches: f32,
        mods: Modifiers,
    ) -> ToolFlow {
        if !mods.alt || mods.ctrl || mods.command || mods.shift || !notches.is_finite() || notches == 0.0 {
            return ToolFlow::Ignored;
        }
        let scene = state.doc.scene();
        let mut changes: Vec<(Id, &'static str, String)> = Vec::new();
        let mut refused = 0usize;
        for id in scene.expand_entities(&state.doc.selected_objects()) {
            if scene.is_brush_entity(id) {
                continue;
            }
            let Some(node) = scene.entity(id) else { continue };
            let key = match scale_key(state, &node.entity.classname) {
                Ok(Some(k)) => k,
                Ok(None) => continue,
                Err(()) => {
                    refused += 1;
                    continue;
                }
            };
            let current = node
                .entity
                .get(key)
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(1.0);
            let factor = WHEEL_SCALE_STEP.powf(f64::from(notches.clamp(-20.0, 20.0)));
            let new = (current * factor).clamp(MODEL_SCALE_RANGE.0, MODEL_SCALE_RANGE.1);
            changes.push((id, key, fmt_scale(new)));
        }
        if changes.is_empty() {
            if refused == 0 {
                return ToolFlow::Ignored;
            }
            self.wheel_note = Some((t!("seltool.hint.static_scale").to_owned(), state.frame.time));
            state.notify(Severity::Warning, t!("seltool.static_scale_unsupported"));
            return ToolFlow::Handled;
        }
        let tag = format!("modelscale:{:016x}", self.selection_sig(state));
        let shown = changes.first().map(|c| c.2.clone()).unwrap_or_default();
        state.doc.begin_merge("Scale models", tag);
        let mut result = Ok(());
        for (id, key, value) in &changes {
            if let Err(e) = state.doc.set_entity_key(&[*id], key, value) {
                result = Err(e);
                break;
            }
        }
        if result.is_ok() {
            state.doc.commit();
        } else {
            state.doc.cancel();
        }
        Self::report(state, result);
        self.wheel_note = Some((tf!("seltool.hint.model_scale", shown), state.frame.time));
        ToolFlow::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_values() {
        assert_eq!(fmt_scale(1.1), "1.1");
        assert_eq!(fmt_scale(1.0 / 3.0), "0.333");
        assert_eq!(fmt_scale(2.0), "2");
    }
}
