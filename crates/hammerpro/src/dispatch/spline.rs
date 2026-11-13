use hp_app::{Action, Severity, ToolId, t, tf};
use hp_tools_create::spline::store;

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Spline;

const HANDLED: &[Action] = &[Action::SplineEdit, Action::SplineBake];

impl Package for Spline {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn init(&self, _app: &mut HammerProApp) {
        super::mcp::install_spline_finisher();
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        let Some(anchor) = store::selected_anchor(&app.state.doc) else {
            app.state.notify(Severity::Info, t!("spline.edit_need"));
            return true;
        };
        match action {
            Action::SplineEdit => app.state.set_tool(ToolId::Spline),
            Action::SplineBake => match store::bake(&mut app.state.doc, anchor, true) {
                Ok(objs) => app.state.log(Severity::Info, tf!("spline.baked", objs.len())),
                Err(e) => app.state.notify(Severity::Warning, tf!("spline.failed", e)),
            },
            _ => return false,
        }
        true
    }
}
