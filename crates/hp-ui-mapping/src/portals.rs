use egui::RichText;
use hp_app::state::{PickKind, PickRequest, PickResult};
use hp_app::{Action, AppState, t, tf, theme};

use crate::dialogs::MapDialog;
use crate::util::{escape_pressed, icon_title};

pub const PICK_REQUESTER: &str = "health:pvs";

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(PortalWindow::default())]
}

#[derive(Default)]
pub(crate) struct PortalWindow {
    detached: bool,
    native: crate::native_window::NativeWindow,
    builder: Option<egui::ViewportBuilder>,
}

pub(crate) fn poll_pick(state: &mut AppState) {
    if let Some(req) = &state.pick
        && req.requester == PICK_REQUESTER
        && let Some(result) = req.result.clone()
    {
        state.pick = None;
        if let PickResult::Point(p) = result {
            let rt = &mut state.runtime.health;
            rt.pvs_point = Some(p);
            rt.pvs = true;
            rt.pvs_cluster = None;
            rt.pvs_visible = None;
        }
    }
}

impl MapDialog for PortalWindow {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        poll_pick(state);
        let rt = &state.runtime.health;
        if !rt.show_portal_window && rt.portal_offer.is_none() {
            return;
        }
        let mut open = true;
        if self.detached {
            let mut attach = false;
            let native = state.settings.ui.native_frames;
            let builder = self.builder.get_or_insert_with(|| {
                crate::native_window::builder(
                    ctx,
                    t!("portals.title"),
                    egui::vec2(360.0, 560.0),
                    egui::vec2(280.0, 240.0),
                    state.settings.ui.portals_window.as_ref(),
                    native,
                )
            });
            *builder = hp_app::chrome::builder(builder.clone(), native);
            let builder = builder.clone();
            let title = builder.title.clone().unwrap_or_default();
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("hp_portals_viewport"),
                builder,
                |ui, _class| {
                    let ctx = ui.ctx().clone();
                    let restore = state.settings.ui.portals_window.clone();
                    if let Some(now) = self.native.follow(&ctx, &title, restore.as_ref(), None)
                        && restore.as_ref() != Some(&now)
                    {
                        state.settings.ui.portals_window = Some(now);
                        state.settings_dirty = true;
                    }
                    let spec = hp_app::chrome::ChromeSpec::dialog(t!("portals.title"), native);
                    let ((), frame) = hp_app::chrome::dialog(ui, spec, |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            if native {
                                if ui.button(t!("portals.attach")).clicked() {
                                    attach = true;
                                }
                                ui.separator();
                            }
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| window_ui(ui, state));
                        });
                    });
                    match frame.clicked {
                        Some(hp_app::chrome::ChromeButton::Place) => attach = true,
                        Some(hp_app::chrome::ChromeButton::Close) => open = false,
                        _ => {}
                    }
                    if ctx.input(|i| i.viewport().close_requested()) {
                        open = false;
                    }
                },
            );
            if attach || !open {
                self.detached = false;
                self.native.forget();
                self.builder = None;
            }
        } else {
            open = self.attached_window(ctx, state);
        }
        if !open {
            let rt = &mut state.runtime.health;
            rt.show_portal_window = false;
            rt.portal_offer = None;
            if rt.portal_build_running {
                rt.portal_cancel_request = true;
            }
        }
        if state.runtime.health.portal_build_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

impl PortalWindow {
    fn attached_window(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let mut detach = false;
        let screen = ctx.content_rect();
        let response = hp_app::chrome::Window::new(icon_title("▣", t!("portals.title")))
            .id(egui::Id::unique("hp_portals_window"))
            .pivot(egui::Align2::RIGHT_TOP)
            .default_pos(egui::pos2(screen.right() - 420.0, screen.top() + 140.0))
            .default_width(320.0)
            .resizable(false)
            .collapsible(true)
            .open(&mut open)
            .show(ctx, |ui| {
                if ui.small_button(t!("portals.detach")).on_hover_text(t!("portals.detach_tip")).clicked() {
                    detach = true;
                }
                window_ui(ui, state);
            });
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if detach {
            self.detached = true;
        }
        open
    }
}

fn window_ui(ui: &mut egui::Ui, state: &mut AppState) {
    if let Some(path) = state.runtime.health.portal_offer.clone() {
        ui.label(RichText::new(t!("portals.offer")).strong());
        ui.label(RichText::new(path.display().to_string()).weak().small());
        ui.horizontal(|ui| {
            if ui.button(t!("portals.load")).clicked() {
                state.runtime.health.portal_load_request = Some(path.clone());
                state.runtime.health.portal_offer = None;
            }
            if ui.button(t!("portals.dismiss")).clicked() {
                state.runtime.health.portal_offer = None;
            }
        });
        if state.runtime.health.portals.is_none() {
            return;
        }
        ui.separator();
    }
    let rt = &state.runtime.health;
    match &rt.portals {
        Some(file) => {
            let n = file.portals.len();
            ui.label(tf!(crate::util::plural(n, "portals.summary_one", "portals.summary"), n, file.clusters));
            if file.declared_portals > file.portals.len() {
                ui.label(
                    RichText::new(tf!("portals.truncated", file.declared_portals)).color(theme::warning()).small(),
                );
            }
            if let Some(p) = &rt.portal_path {
                ui.label(RichText::new(p.display().to_string()).weak().small());
            }
        }
        None => {
            ui.label(RichText::new(t!("portals.none")).weak());
        }
    }
    ui.separator();
    let s = &mut state.settings.health;
    let mut width = s.portal_width_px();
    ui.horizontal(|ui| {
        ui.label(t!("portals.width"));
        if ui.add(egui::Slider::new(&mut width, 1.0..=6.0).step_by(0.1).fixed_decimals(1).suffix(" px")).changed() {
            s.portal_width = width;
            state.settings_dirty = true;
        }
    });
    let mut xray = state.settings.health.portal_xray;
    if ui.checkbox(&mut xray, t!("portals.xray")).on_hover_text(t!("portals.xray_tip")).changed() {
        state.settings.health.portal_xray = xray;
        state.settings_dirty = true;
    }
    let has_heat = state.runtime.health.costs.as_ref().is_some_and(|c| !c.heat().is_empty());
    let mut heat = state.settings.health.portal_heat;
    if ui.checkbox(&mut heat, t!("portals.heat")).on_hover_text(t!("portals.heat_tip")).changed() {
        state.settings.health.portal_heat = heat;
        state.settings_dirty = true;
    }
    if heat && !has_heat && state.runtime.health.vis.is_none() {
        ui.label(RichText::new(t!("portals.heat_needs_vis")).weak().small());
    }
    let mut grid = state.settings.health.leaf_grid;
    if ui.checkbox(&mut grid, t!("portals.leaf_grid")).on_hover_text(t!("portals.leaf_grid_tip")).changed() {
        state.settings.health.leaf_grid = grid;
        state.settings_dirty = true;
        state.runtime.health.portal_version = state.runtime.health.portal_version.wrapping_add(1);
    }
    ui.add_enabled_ui(grid, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(t!("portals.leaf_width"));
            let mut w = state.settings.health.leaf_width_px();
            if ui.add(egui::Slider::new(&mut w, 1.0..=6.0).step_by(0.1).fixed_decimals(1).suffix(" px")).changed() {
                state.settings.health.leaf_width = w;
                state.settings_dirty = true;
            }
        });
    });
    ui.separator();
    let mut pvs = state.runtime.health.pvs;
    if ui.checkbox(&mut pvs, Action::TogglePvs.label()).on_hover_text(t!("portals.pvs_tip")).changed() {
        state.request(Action::TogglePvs);
    }
    if state.runtime.health.pvs {
        ui.horizontal(|ui| {
            let from_camera = state.runtime.health.pvs_point.is_none();
            if ui.radio(from_camera, t!("portals.from_camera")).clicked() {
                let rt = &mut state.runtime.health;
                rt.pvs_point = None;
                rt.pvs_cluster = None;
                rt.pvs_visible = None;
            }
            if ui.radio(!from_camera, t!("portals.from_point")).on_hover_text(t!("portals.pick_tip")).clicked() {
                state.pick =
                    Some(PickRequest { kind: PickKind::Point, requester: PICK_REQUESTER.to_owned(), result: None });
            }
        });
        let rt = &state.runtime.health;
        match (&rt.vis, rt.pvs_cluster, &rt.pvs_visible) {
            (None, ..) => {
                ui.label(RichText::new(t!("portals.no_vis")).weak());
            }
            (Some(_), Some(c), Some(v)) => {
                let seen = v.iter().filter(|x| **x).count();
                ui.label(tf!("portals.visible", c, seen, v.len()));
                if let Some(cost) = rt.pvs_cost {
                    ui.label(tf!(
                        "portals.cost",
                        cost.faces,
                        cost.triangles,
                        cost.displacements,
                        cost.props,
                        cost.overlays
                    ))
                    .on_hover_text(t!("portals.cost_tip"));
                }
            }
            (Some(_), _, _) => {
                ui.label(RichText::new(t!("portals.in_solid")).weak());
            }
        }
    }
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        let building = state.runtime.health.portal_build_running;
        if building {
            hp_app::widgets::spinner(ui);
            ui.label(t!("portals.building"));
        } else if ui.button(t!("portals.vbsp_only")).on_hover_text(t!("portals.vbsp_only_tip")).clicked() {
            state.request(Action::BuildPortals);
        }
        let edited = state.runtime.health.portal_doc_revision.is_some_and(|r| r != state.doc.revision());
        if state.runtime.health.portal_path.is_some()
            && !building
            && ui.button(t!("portals.reload")).on_hover_text(t!("portals.reload_tip")).clicked()
        {
            if edited {
                state.request(Action::BuildPortals);
            } else {
                state.runtime.health.portal_reload_request = true;
            }
        }
        if ui.button(Action::LoadPortalFile.label()).clicked() {
            state.request(Action::LoadPortalFile);
        }
        if state.runtime.health.portals.is_some() && ui.button(Action::UnloadPortalFile.label()).clicked() {
            state.request(Action::UnloadPortalFile);
        }
    });
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use hp_app::testing::{run_ctx, test_state};
    use hp_editor::limits::portals::parse_prt;

    use super::*;

    #[test]
    fn window_draws_every_state_and_takes_picks() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        let mut w = PortalWindow::default();
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        state.runtime.health.portal_offer = Some("C:/maps/x.prt".into());
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        let prt = "PRT1\n2\n1\n3 0 1 (0 0 0) (1 0 0) (0 1 0)\n";
        state.runtime.health.set_portals("C:/maps/x.prt".into(), parse_prt(prt).expect("prt"));
        state.runtime.health.show_portal_window = true;
        state.runtime.health.pvs = true;
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        state.pick = Some(PickRequest {
            kind: PickKind::Point,
            requester: PICK_REQUESTER.to_owned(),
            result: Some(PickResult::Point(DVec3::new(1.0, 2.0, 3.0))),
        });
        poll_pick(&mut state);
        assert!(state.pick.is_none());
        assert_eq!(state.runtime.health.pvs_point, Some(DVec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn render_cost_and_heat_map_options() {
        use hp_editor::limits::cost::{RenderCost, RenderCosts};
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        let mut w = PortalWindow::default();
        let prt = "PRT1
2
1
3 0 1 (0 0 0) (1 0 0) (0 1 0)
";
        state.runtime.health.set_portals("C:/maps/x.prt".into(), parse_prt(prt).expect("prt"));
        state.runtime.health.show_portal_window = true;
        state.settings.health.portal_heat = true;
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        let rt = &mut state.runtime.health;
        rt.set_vis("C:/maps/x.bsp".into(), hp_editor::limits::portals::BspVis::default());
        rt.set_costs(RenderCosts::from_heat(vec![5, 9]));
        rt.pvs = true;
        rt.pvs_cluster = Some(0);
        rt.pvs_visible = Some(std::sync::Arc::new(vec![true, true]));
        rt.pvs_cost = Some(RenderCost { faces: 10, triangles: 40, displacements: 1, props: 2, overlays: 3 });
        for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
            hp_app::i18n::set_lang(lang);
            run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(
            tf!("portals.cost", 10, 40, 1, 2, 3),
            "Drawn from here: faces 10, triangles 40, displacements 1, props 2, overlays 3"
        );
        assert!(state.settings.health.portal_heat);
    }
}
