use hp_app::i18n::TrTable;
use hp_app::{Action, AppState, Severity};

use super::Package;
use crate::app::HammerProApp;
use crate::livelink::{self, TickInput, ui};

pub(crate) struct LiveLink;

#[derive(Default)]
pub(crate) struct State;

const HANDLED: &[Action] = &[
    Action::LiveLinkStart,
    Action::LiveLinkStop,
    Action::LiveLinkPanel,
    Action::LiveLinkFireInput,
    Action::LiveLinkTeleport,
    Action::LiveLinkTeleportBack,
    Action::LiveLinkCameraToGame,
    Action::LiveLinkCameraFromGame,
    Action::LiveLinkResync,
    Action::LiveLinkRemoveAddon,
];

fn report(state: &mut AppState, result: Result<String, String>) {
    match result {
        Ok(text) => state.notify(Severity::Success, text),
        Err(e) => state.notify(Severity::Warning, e),
    }
}

fn open_fire(state: &mut AppState) {
    if let Some(target) = ui::selected_target(state) {
        state.runtime.livelink.fire.target = target;
    }
    state.runtime.livelink.fire_open = true;
}

fn stop(state: &mut AppState) {
    if livelink::stop(state) {
        state.notify(Severity::Info, hp_app::t!("livelink.stopped"));
    }
}

impl Package for LiveLink {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        let state = &mut app.state;
        match action {
            Action::LiveLinkStart => state.runtime.livelink.start_requested = true,
            Action::LiveLinkStop => state.runtime.livelink.stop_requested = true,
            Action::LiveLinkPanel => state.runtime.livelink.panel_open = !state.runtime.livelink.panel_open,
            Action::LiveLinkFireInput => open_fire(state),
            Action::LiveLinkTeleport => state.runtime.livelink.teleport_requested = true,
            Action::LiveLinkTeleportBack => state.runtime.livelink.back_requested = true,
            Action::LiveLinkCameraToGame => {
                let s = &mut state.settings.livelink;
                s.camera_to_game = !s.camera_to_game;
                if s.camera_to_game {
                    s.camera_from_game = false;
                }
                state.settings_dirty = true;
            }
            Action::LiveLinkCameraFromGame => {
                let s = &mut state.settings.livelink;
                s.camera_from_game = !s.camera_from_game;
                if s.camera_from_game {
                    s.camera_to_game = false;
                }
                state.settings_dirty = true;
            }
            Action::LiveLinkResync => state.runtime.livelink.resync_requested = true,
            Action::LiveLinkRemoveAddon => state.runtime.livelink.remove_addon_requested = true,
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        let compiling = app.compile.is_running();
        let state = &mut app.state;
        if state.runtime.livelink.wake.is_none() {
            state.runtime.livelink.wake = Some(ctx.clone());
        }
        let rt = &mut state.runtime.livelink;
        let (start, stop_req, resync, remove, teleport, back) = (
            std::mem::take(&mut rt.start_requested),
            std::mem::take(&mut rt.stop_requested),
            std::mem::take(&mut rt.resync_requested),
            std::mem::take(&mut rt.remove_addon_requested),
            std::mem::take(&mut rt.teleport_requested),
            std::mem::take(&mut rt.back_requested),
        );
        if start {
            let r = livelink::start(state);
            report(state, r);
        }
        if stop_req {
            stop(state);
        }
        if resync {
            let r = livelink::resync(state);
            report(state, r);
        }
        if remove {
            let r = livelink::remove_addon(state);
            report(state, r);
        }
        if teleport {
            let r = livelink::teleport(state, None, true);
            report(state, r);
        }
        if back {
            let r = livelink::back(state);
            report(state, r);
        }
        livelink::tick(state, &TickInput { compiling });
        if state.runtime.livelink.session.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        let state = &mut app.state;
        for request in ui::panel(state, ctx) {
            match request {
                ui::Request::Start => state.runtime.livelink.start_requested = true,
                ui::Request::Stop => state.runtime.livelink.stop_requested = true,
                ui::Request::Teleport => state.runtime.livelink.teleport_requested = true,
                ui::Request::Back => state.runtime.livelink.back_requested = true,
                ui::Request::OpenFire => open_fire(state),
                ui::Request::Resync => state.runtime.livelink.resync_requested = true,
                ui::Request::MarkCompiled => {
                    let r = livelink::mark_compiled(state);
                    report(state, r);
                }
                ui::Request::RemoveAddon => state.runtime.livelink.remove_addon_requested = true,
            }
        }
        ui::fire_dialog(state, ctx);
    }

    fn translations(&self) -> TrTable {
        livelink::tr::TR
    }
}
