use hp_app::i18n::TrTable;
use hp_app::{Action, Severity, Tool, t, tf};
use hp_editor::{EditError, EditResult, ObjRef};

use crate::app::HammerProApp;

mod blender;
mod brush;
mod compile;
mod content;
mod entities;
mod file_watch;
mod health;
mod helpers;
mod inset;
mod instances;
pub(crate) mod lab;
mod launch;
pub(crate) mod lighting;
pub(crate) mod mcp;
mod mesh;
pub(crate) mod mesh_disp;
mod placement;
mod placement_ops;
mod render;
mod roadside;
mod selection;
mod shell;
mod startup;
mod visibility;
mod workflow;

pub(crate) use placement::TAB as PREFABS_TAB;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockTab {
    pub id: &'static str,
    pub label: String,
}

pub(crate) trait Package: Sync {
    fn handled(&self) -> &'static [Action] {
        &[]
    }

    fn handle(&self, _app: &mut HammerProApp, _ctx: &egui::Context, _action: Action) -> bool {
        false
    }

    fn tick(&self, _app: &mut HammerProApp, _ctx: &egui::Context) {}

    fn init(&self, _app: &mut HammerProApp) {}

    fn map_loaded(&self, _app: &mut HammerProApp) {}

    fn dialogs(&self, _app: &mut HammerProApp, _ctx: &egui::Context) {}

    fn dock_tabs(&self, _app: &HammerProApp, _tabs: &mut Vec<DockTab>) {}

    fn dock_ui(&self, _app: &mut HammerProApp, _ui: &mut egui::Ui, _id: &'static str) -> bool {
        false
    }

    fn tools(&self, _out: &mut Vec<Box<dyn Tool>>) {}

    fn translations(&self) -> TrTable {
        &[]
    }
}

const PACKAGES: &[&dyn Package] = &[
    &shell::Shell,
    &selection::Selection,
    &brush::Brush,
    &workflow::Workflow,
    &instances::Instances,
    &visibility::Visibility,
    &helpers::Helpers,
    &mesh::Mesh,
    &mesh_disp::MeshDisp,
    &entities::Entities,
    &render::Render,
    &roadside::Roadside,
    &lighting::Lighting,
    &placement::Placement,
    &compile::Compile,
    &startup::Startup,
    &launch::Launch,
    &content::Content,
    &health::Health,
    &lab::Lab,
    &blender::Bridge,
    &mcp::Mcp,
];

#[allow(dead_code)]
#[derive(Default)]
pub(crate) struct Packages {
    pub shell: shell::State,
    pub selection: selection::State,
    pub brush: brush::State,
    pub workflow: workflow::State,
    pub instances: instances::State,
    pub visibility: visibility::State,
    pub helpers: helpers::State,
    pub mesh: mesh::State,
    pub mesh_disp: mesh_disp::State,
    pub entities: entities::State,
    pub render: render::State,
    pub roadside: roadside::State,
    pub lighting: lighting::State,
    pub placement: placement::State,
    pub compile: compile::State,
    pub startup: startup::State,
    pub launch: launch::State,
    pub content: content::State,
    pub health: health::State,
    pub lab: lab::State,
    pub blender: blender::State,
    pub mcp: mcp::State,
}

pub fn register_translations() {
    hp_app::i18n::register(TR);
    hp_app::i18n::register(placement_ops::TR);
    for package in PACKAGES {
        hp_app::i18n::register(package.translations());
    }
}

pub(crate) fn package_tools(out: &mut Vec<Box<dyn Tool>>) {
    for package in PACKAGES {
        package.tools(out);
    }
}

pub(crate) fn init(app: &mut HammerProApp) {
    for package in PACKAGES {
        package.init(app);
    }
}

pub(crate) fn tick(app: &mut HammerProApp, ctx: &egui::Context) {
    for package in PACKAGES {
        package.tick(app, ctx);
    }
}

pub(crate) fn map_loaded(app: &mut HammerProApp) {
    for package in PACKAGES {
        package.map_loaded(app);
    }
}

pub(crate) fn dialogs(app: &mut HammerProApp, ctx: &egui::Context) {
    for package in PACKAGES {
        package.dialogs(app, ctx);
    }
}

pub(crate) fn dock_tabs(app: &HammerProApp) -> Vec<DockTab> {
    let mut tabs = Vec::new();
    for package in PACKAGES {
        package.dock_tabs(app, &mut tabs);
    }
    tabs
}

pub(crate) fn dock_ui(app: &mut HammerProApp, ui: &mut egui::Ui, id: &'static str) {
    for package in PACKAGES {
        if package.dock_ui(app, ui, id) {
            return;
        }
    }
}

impl HammerProApp {
    pub(crate) fn execute(&mut self, ctx: &egui::Context, action: Action) {
        if self.state.runtime.collaboration.active_map && self.collaboration.history(&mut self.state, action) {
            return;
        }
        if let Some(tool) = action.tool()
            && self.has_tool(tool)
        {
            self.switch_tool(tool);
            return;
        }
        if action == Action::SnapToGrid && self.bevel_shortcut(ctx) {
            return;
        }
        if action == Action::InsetInteractive && self.inset_shortcut(ctx) {
            return;
        }
        if self.active_tool_action(action) {
            return;
        }
        for package in PACKAGES {
            let done = package.handle(self, ctx, action);
            debug_assert_eq!(done, package.handled().contains(&action), "handled() is out of date for {}", action.id());
            if done {
                return;
            }
        }
        self.not_implemented(action);
    }

    pub(crate) fn not_implemented(&mut self, action: Action) {
        let text = tf!("app.not_implemented", action.label());
        if self.not_implemented_seen.insert(action) {
            log::info!("action {} is not implemented yet", action.id());
            self.state.log(Severity::Info, text.clone());
        }
        self.state.toasts.push(Severity::Info, text);
    }

    pub(crate) fn selected(&self) -> Vec<ObjRef> {
        self.state.doc.selected_objects()
    }

    pub(crate) fn selected_solids(&self) -> Vec<hp_vmf::Id> {
        self.state
            .doc
            .selected_objects()
            .into_iter()
            .filter_map(|o| match o {
                ObjRef::Solid(id) => Some(id),
                ObjRef::Entity(_) => None,
            })
            .collect()
    }

    pub(crate) fn report<T>(&mut self, action: Action, result: EditResult<T>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(e) => {
                self.state.notify(Severity::Warning, tf!("app.op_failed", action.label(), edit_error_text(&e)));
                None
            }
        }
    }

    pub(crate) fn require_selection(&mut self) -> bool {
        if self.state.doc.selected_objects().is_empty() {
            self.state.notify(Severity::Info, t!("app.nothing_selected"));
            false
        } else {
            true
        }
    }

    pub(crate) fn entity_bounds_fn(&self) -> impl Fn(&hp_vmf::Entity) -> Option<hp_geom::Aabb> + '_ {
        move |e| self.state.assets.entity_bounds(e)
    }
}

pub fn edit_error_text(e: &EditError) -> String {
    match e {
        EditError::EmptyClipboard => t!("app.err.empty_clipboard").to_owned(),
        EditError::NotFound(_) | EditError::FaceNotFound(..) => t!("app.err.not_found").to_owned(),
        EditError::GroupNotFound(id) => tf!("app.err.group_not_found", id),
        EditError::VisgroupNotFound(id) => tf!("app.err.visgroup_not_found", id),
        EditError::Invalid(detail) => tf!("app.err.invalid", detail),
        EditError::Nothing(detail) => tf!("app.err.nothing", detail),
        EditError::TransactionOpen => t!("app.err.busy").to_owned(),
        EditError::NoPath => t!("app.err.no_path").to_owned(),
        other => other.to_string(),
    }
}

pub const TR: TrTable = &[("app.not_implemented", "{0} is not implemented yet", "{0} n'est pas encore disponible")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty());
        for package in PACKAGES {
            assert!(hp_app::i18n::lint_table(package.translations()).is_empty());
        }
    }

    #[test]
    fn every_ready_action_has_one_handler() {
        let mut owner = std::collections::HashMap::new();
        for (i, package) in PACKAGES.iter().enumerate() {
            for a in package.handled() {
                assert!(owner.insert(*a, i).is_none(), "{} is handled twice", a.id());
            }
        }
        let missing: Vec<&str> =
            Action::ALL.iter().filter(|a| a.tool().is_none() && !owner.contains_key(*a)).map(|a| a.id()).collect();
        assert!(missing.is_empty(), "ready actions without a handler: {missing:?}");
    }
}
