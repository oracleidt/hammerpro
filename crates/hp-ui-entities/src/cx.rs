use std::collections::HashMap;

use hp_app::actions::{Action, KeyBindings};
use hp_app::console::{Console, Toasts};
use hp_app::runtime::Runtime;
use hp_app::settings::Settings;
use hp_app::state::{AssetsApi, ParticleCatalog, PickRequest, RightTab, ThumbState, UiState};
use hp_app::{AppState, Severity, t, tf};
use hp_editor::Document;
use hp_fgd::ResolvedFgd;

use crate::edit::{Applied, Op, apply};
use crate::view::Target;

pub(crate) struct Cx<'a> {
    pub doc: &'a mut Document,
    pub fgd: Option<&'a ResolvedFgd>,
    pub assets: &'a dyn AssetsApi,
    pub ui: &'a mut UiState,
    pub settings: &'a mut Settings,
    pub settings_dirty: &'a mut bool,
    pub current_material: &'a str,
    pub requested_actions: &'a mut Vec<Action>,
    pub keys: &'a KeyBindings,
    pub console: &'a mut Console,
    pub toasts: &'a mut Toasts,
    pub thumbs: &'a HashMap<String, ThumbState>,
    pub pick: &'a mut Option<PickRequest>,
    pub runtime: &'a mut Runtime,
    pub particles: Option<&'a ParticleCatalog>,
}

impl<'a> Cx<'a> {
    pub fn new(
        state: &'a mut AppState,
        fgd_override: Option<&'a ResolvedFgd>,
        thumbs: &'a HashMap<String, ThumbState>,
    ) -> Cx<'a> {
        Self::with_particles(state, fgd_override, thumbs, None)
    }

    pub fn with_particles(
        state: &'a mut AppState,
        fgd_override: Option<&'a ResolvedFgd>,
        thumbs: &'a HashMap<String, ThumbState>,
        particles: Option<&'a ParticleCatalog>,
    ) -> Cx<'a> {
        let AppState {
            doc,
            assets,
            ui,
            settings,
            settings_dirty,
            current_material,
            requested_actions,
            keys,
            console,
            toasts,
            pick,
            runtime,
            ..
        } = state;
        let assets: &'a dyn AssetsApi = &**assets;
        let fgd = fgd_override.or_else(|| assets.fgd());
        Cx {
            doc,
            fgd,
            assets,
            ui,
            settings,
            settings_dirty,
            current_material: current_material.as_str(),
            requested_actions,
            keys,
            console,
            toasts,
            thumbs,
            pick,
            runtime,
            particles,
        }
    }

    pub fn request(&mut self, action: Action) {
        if !self.requested_actions.contains(&action) {
            self.requested_actions.push(action);
        }
    }

    pub fn show_models_tab(&mut self) {
        show_models_tab(self.settings, self.settings_dirty, self.ui);
    }

    pub fn notify(&mut self, severity: Severity, text: String) {
        self.console.push(severity, text.clone());
        self.toasts.push(severity, text);
    }

    pub fn apply_ops(&mut self, ops: Vec<Op>) -> Option<Applied> {
        let mut last = None;
        for op in ops {
            if op.writes_quote() {
                self.notify(Severity::Warning, t!("entities.no_quotes").to_owned());
                continue;
            }
            match apply(self.doc, self.fgd, op) {
                Ok(r) => last = Some(r),
                Err(e) => {
                    let text = tf!("entities.edit_failed", e);
                    self.notify(Severity::Error, text);
                }
            }
        }
        last
    }
}

pub(crate) fn show_models_tab(settings: &mut Settings, settings_dirty: &mut bool, ui: &mut UiState) {
    if !settings.ui.show_model_browser {
        settings.ui.show_model_browser = true;
        *settings_dirty = true;
    }
    ui.right_tab = RightTab::Models;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelPick {
    pub target: Target,
    pub key: String,
    pub label: String,
}

fn pick_id() -> egui::Id {
    egui::Id::unique("hp_ui_entities.model_pick")
}

fn recent_id() -> egui::Id {
    egui::Id::unique("hp_ui_entities.recent_models")
}

pub fn model_pick(ctx: &egui::Context) -> Option<ModelPick> {
    ctx.data(|d| d.get_temp::<Option<ModelPick>>(pick_id())).flatten()
}

pub fn set_model_pick(ctx: &egui::Context, pick: Option<ModelPick>) {
    ctx.data_mut(|d| d.insert_temp(pick_id(), pick));
}

pub fn recent_models(ctx: &egui::Context) -> Vec<String> {
    ctx.data(|d| d.get_temp::<Vec<String>>(recent_id())).unwrap_or_default()
}

pub fn recent_models_len(ctx: &egui::Context) -> usize {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<String>>(recent_id()).len())
}

pub const MAX_RECENT_MODELS: usize = 16;

pub fn push_recent_model(ctx: &egui::Context, path: &str) {
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    ctx.data_mut(|d| {
        let list = d.get_temp_mut_or_default::<Vec<String>>(recent_id());
        list.retain(|p| !p.eq_ignore_ascii_case(path));
        list.insert(0, path.to_owned());
        list.truncate(MAX_RECENT_MODELS);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_models_are_unique_and_bounded() {
        let ctx = egui::Context::default();
        assert!(recent_models(&ctx).is_empty());
        push_recent_model(&ctx, "models/a.mdl");
        push_recent_model(&ctx, "models/b.mdl");
        push_recent_model(&ctx, "MODELS/A.MDL");
        push_recent_model(&ctx, "  ");
        assert_eq!(recent_models(&ctx), ["MODELS/A.MDL", "models/b.mdl"]);
        for i in 0..40 {
            push_recent_model(&ctx, &format!("models/{i}.mdl"));
        }
        assert_eq!(recent_models(&ctx).len(), MAX_RECENT_MODELS);
    }

    #[test]
    fn model_pick_round_trip() {
        let ctx = egui::Context::default();
        assert_eq!(model_pick(&ctx), None);
        let pick = ModelPick { target: Target::Entities(vec![3]), key: "model".into(), label: "prop".into() };
        set_model_pick(&ctx, Some(pick.clone()));
        assert_eq!(model_pick(&ctx), Some(pick));
        set_model_pick(&ctx, None);
        assert_eq!(model_pick(&ctx), None);
    }
}
