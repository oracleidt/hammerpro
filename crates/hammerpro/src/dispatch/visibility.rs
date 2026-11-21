use hp_app::i18n::TrTable;
use hp_app::{Action, Severity, Tool, ToolId, t, tf};

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Visibility;

#[derive(Default)]
pub(crate) struct State {
    cordon_key: u64,
}

const HANDLED: &[Action] = &[
    Action::HideSelected,
    Action::HideUnselected,
    Action::UnhideAll,
    Action::ToggleCordon,
    Action::MoveToVisgroup,
    Action::RemoveFromAllVisgroups,
    Action::HiddenToVisgroup,
];

impl Package for Visibility {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::HideSelected => {
                app.state.doc.hide_selection();
            }
            Action::HideUnselected => {
                app.state.doc.hide_unselected();
            }
            Action::UnhideAll => {
                app.state.doc.unhide_all();
            }
            Action::ToggleCordon => toggle_cordons(app),
            Action::MoveToVisgroup => {
                if app.require_selection() {
                    app.state.runtime.visibility.move_to_visgroup_open = true;
                }
            }
            Action::RemoveFromAllVisgroups => {
                if app.require_selection() {
                    let objs = app.state.doc.selected_objects();
                    match app.state.doc.remove_from_all_visgroups(&objs) {
                        Ok(n) => app.state.notify(Severity::Success, tf!("visibility.removed_from_visgroups", n)),
                        Err(e) => app.state.notify(Severity::Info, tf!("visibility.remove_from_visgroups_failed", e)),
                    }
                }
            }
            Action::HiddenToVisgroup => hidden_to_visgroup(app),
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, _ctx: &egui::Context) {
        let key = app.state.doc.cordon_key();
        if key != app.pkg.visibility.cordon_key {
            app.pkg.visibility.cordon_key = key;
            app.state.doc.refresh_visibility();
        }
        if let Some(b) = app.state.runtime.visibility.focus_box.take()
            && b.min.is_finite()
            && b.max.is_finite()
        {
            let size = app.viewports_size_2d();
            app.state.cameras.frame_2d(b.min, b.max, size);
            let mut cam = app.state.cameras.cam3d;
            cam.look_at(b.center());
            cam.frame_box(b.min, b.max);
            app.state.cameras.cam3d = cam;
        }
    }

    fn init(&self, app: &mut HammerProApp) {
        app.shell.add_status_segment(Box::new(hp_ui_mapping::cordons::CordonStatus));
    }

    fn tools(&self, out: &mut Vec<Box<dyn Tool>>) {
        out.push(Box::new(hp_tools_create::CordonTool::new()));
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

fn toggle_cordons(app: &mut HammerProApp) {
    let on = !app.state.doc.scene().cordons().active;
    let result = app.state.doc.set_cordons_enabled(on);
    if app.report(Action::ToggleCordon, result).is_none() {
        return;
    }
    if on && !app.state.doc.cordons_in_use() {
        app.state.notify(Severity::Info, t!("visibility.no_active_cordon"));
        app.state.set_tool(ToolId::Cordon);
    }
}

fn hidden_to_visgroup(app: &mut HammerProApp) {
    let result = app.state.doc.hidden_to_visgroup();
    match app.report(Action::HiddenToVisgroup, result) {
        Some(Some((id, n))) => {
            let name = app.state.doc.visgroup(id).map(|g| g.name.clone()).unwrap_or_default();
            app.state.notify(Severity::Info, tf!("visibility.hidden_converted", n, name));
        }
        Some(None) => app.state.notify(Severity::Info, t!("visibility.nothing_hidden")),
        None => {}
    }
}

const TR: TrTable = &[
    (
        "visibility.no_active_cordon",
        "No active cordon box: draw one with the cordon tool",
        "Aucune boîte de cordon active : dessinez-en une avec l'outil cordon",
    ),
    (
        "visibility.hidden_converted",
        "{0} hidden objects moved to the hidden visgroup {1}",
        "{0} objets masqués placés dans le visgroup masqué {1}",
    ),
    ("visibility.nothing_hidden", "No object is hidden", "Aucun objet n'est masqué"),
    (
        "visibility.removed_from_visgroups",
        "{0} objects removed from every visgroup",
        "{0} objets retirés de tous les visgroups",
    ),
    ("visibility.remove_from_visgroups_failed", "Nothing to remove: {0}", "Rien à retirer : {0}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
        for (k, _, _) in TR {
            assert!(k.starts_with("visibility."));
        }
    }
}
