use hp_app::i18n::TrTable;
use hp_app::runtime::helpers::{SceneDisplay, publish_scene_display};
use hp_app::settings::{ConnectionsMode, HelperScope};
use hp_app::{Action, Severity, t};
use hp_editor::{ObjRef, Scene, SelectOp};

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Helpers;

#[derive(Default)]
pub(crate) struct State {}

const HANDLED: &[Action] = &[
    Action::ToggleHelpers,
    Action::ToggleOverlays,
    Action::ToggleConnections,
    Action::ToggleEditorObjects,
    Action::TogglePointEntities,
    Action::ToggleEntityNames,
    Action::ToggleBrushCenters,
    Action::SimulateRopes,
    Action::ToggleHelpersAll,
    Action::ToggleRopes,
];

fn is_point_entity(scene: &Scene, obj: ObjRef) -> bool {
    matches!(obj, ObjRef::Entity(id) if !scene.is_brush_entity(id))
}

fn set_point_entities_shown(app: &mut HammerProApp, shown: bool) {
    let doc = &mut app.state.doc;
    doc.begin(if shown { "Show point entities" } else { "Hide point entities" });
    let changed = doc.set_auto_visibility(&is_point_entity, shown);
    if !shown {
        let scene = doc.scene();
        let selected: Vec<ObjRef> =
            doc.selection().objects.iter().copied().filter(|o| is_point_entity(scene, *o)).collect();
        if !selected.is_empty() {
            doc.select(&selected, SelectOp::Remove);
        }
    }
    doc.commit();
    if changed == 0 && !shown {
        app.state.notify(Severity::Info, t!("helpers.no_point_entities"));
    }
}

impl Package for Helpers {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        let h = &mut app.state.settings.helpers;
        match action {
            Action::ToggleHelpers => {
                app.state.settings.view.show_helpers = !app.state.settings.view.show_helpers;
            }
            Action::ToggleOverlays => h.show_overlays = !h.show_overlays,
            Action::ToggleConnections => {
                h.connections = h.connections.next();
                let text = match h.connections {
                    ConnectionsMode::Off => t!("helpers.connections_off"),
                    ConnectionsMode::Selected => t!("helpers.connections_selected"),
                    ConnectionsMode::All => t!("helpers.connections_all"),
                };
                app.state.toasts.push(Severity::Info, text);
            }
            Action::ToggleEditorObjects => h.show_editor_objects = !h.show_editor_objects,
            Action::ToggleEntityNames => h.show_entity_names = !h.show_entity_names,
            Action::ToggleBrushCenters => h.show_brush_centers = !h.show_brush_centers,
            Action::SimulateRopes => h.simulate_ropes = !h.simulate_ropes,
            Action::ToggleRopes => h.show_ropes = !h.show_ropes,
            Action::ToggleHelpersAll => {
                h.helper_scope = match h.helper_scope {
                    HelperScope::Selected => HelperScope::All,
                    HelperScope::All => HelperScope::Selected,
                };
            }
            Action::TogglePointEntities => {
                let hidden = app.state.runtime.helpers.point_entities_hidden(&app.state.doc);
                set_point_entities_shown(app, hidden);
                return true;
            }
            _ => return false,
        }
        app.state.settings_dirty = true;
        true
    }

    fn tick(&self, app: &mut HammerProApp, _ctx: &egui::Context) {
        let game_look = app.state.runtime.lab.player.active;
        publish_scene_display(SceneDisplay { game_look, ..SceneDisplay::from_settings(&app.state.settings) });
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

const TR: TrTable = &[
    ("helpers.connections_off", "Connections: hidden", "Connexions : masquées"),
    ("helpers.connections_selected", "Connections: selected entities", "Connexions : entités sélectionnées"),
    ("helpers.connections_all", "Connections: every entity", "Connexions : toutes les entités"),
    ("helpers.no_point_entities", "The map has no point entity", "La carte n'a aucune entité ponctuelle"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
    }
}
