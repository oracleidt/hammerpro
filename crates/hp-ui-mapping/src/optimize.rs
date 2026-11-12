use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::{Action, AppState, t, tf, theme};
use hp_editor::optimize::{Finding, Impact, Topic};

use crate::dialogs::MapDialog;
use crate::util::{escape_pressed, icon_title};

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(OptimizeWindow::default())]
}

#[derive(Default)]
pub(crate) struct OptimizeWindow {
    topic: Option<Topic>,
}

const TOPICS: [Topic; 5] = [Topic::Visleaves, Topic::Detail, Topic::Hints, Topic::Areaportals, Topic::Lights];

fn topic_label(t: Topic) -> &'static str {
    match t {
        Topic::Visleaves => t!("optimize.topic.visleaves"),
        Topic::Detail => t!("optimize.topic.func_detail"),
        Topic::Hints => t!("optimize.topic.hints"),
        Topic::Areaportals => t!("optimize.topic.areaportals"),
        Topic::Lights => t!("optimize.topic.lights"),
    }
}

fn why(t: Topic) -> &'static str {
    match t {
        Topic::Visleaves => t!("optimize.why.visleaves"),
        Topic::Detail => t!("optimize.why.func_detail"),
        Topic::Hints => t!("optimize.why.hints"),
        Topic::Areaportals => t!("optimize.why.areaportals"),
        Topic::Lights => t!("optimize.why.lights"),
    }
}

fn impact_label(i: Impact) -> (&'static str, egui::Color32) {
    match i {
        Impact::High => (t!("optimize.impact.high"), theme::error()),
        Impact::Medium => (t!("optimize.impact.medium"), theme::warning()),
        Impact::Low => (t!("optimize.impact.low"), theme::text_weak()),
    }
}

pub fn finding_text(f: &Finding) -> String {
    let key = format!("optimize.f.{}", f.code);
    let template = t!(&key);
    if template == key {
        return f.message.clone();
    }
    let mut out = template.to_owned();
    for i in 0..8 {
        out = out.replace(&format!("{{{i}}}"), f.arg(i));
    }
    out
}

pub fn go_to(state: &mut AppState, f: &Finding) {
    if !f.objects.is_empty() {
        let objs: Vec<_> = f.objects.iter().copied().filter(|o| state.doc.scene().contains(*o)).collect();
        if !objs.is_empty() {
            state.doc.set_selection(&objs);
            state.request(Action::Center2dOnSelection);
            state.request(Action::Center3dOnSelection);
            return;
        }
    }
    let b = f.focus();
    if b.is_empty() {
        return;
    }
    let c = b.center();
    let d = b.size().max_element().max(64.0) * 1.2 + 96.0;
    state.cameras.cam3d.pos = c + DVec3::new(-d * 0.7, -d * 0.7, d * 0.5);
    state.cameras.cam3d.look_at(c);
    state.cameras.center_2d(c);
}

const DETAIL_CODE: &str = "detail_candidates";

pub fn select_detail(state: &mut AppState, f: &Finding) -> usize {
    let scene = state.doc.scene();
    let objs: Vec<hp_editor::ObjRef> = f
        .objects
        .iter()
        .copied()
        .filter(|o| match *o {
            hp_editor::ObjRef::Solid(id) => scene.solid(id).is_some_and(|n| n.owner == hp_editor::Owner::World),
            hp_editor::ObjRef::Entity(_) => false,
        })
        .collect();
    let n = objs.len();
    if n == 0 {
        state.notify(hp_app::Severity::Info, t!("optimize.select_none"));
    } else {
        state.doc.set_selection(&objs);
        state.notify(hp_app::Severity::Info, tf!("optimize.selected_detail", n));
    }
    n
}

impl MapDialog for OptimizeWindow {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.optimize.show {
            return;
        }
        let mut open = true;
        let screen = ctx.content_rect();
        let response = hp_app::chrome::Window::new(icon_title("⚙", t!("optimize.title")))
            .id(egui::Id::unique("hp_optimize_window"))
            .default_pos(egui::pos2(screen.right() - 560.0, screen.top() + 120.0))
            .default_size(egui::vec2(500.0, 560.0))
            .resizable(true)
            .collapsible(true)
            .open(&mut open)
            .show(ctx, |ui| self.body(ui, state));
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if !open {
            state.runtime.optimize.show = false;
        }
        if state.runtime.optimize.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

impl OptimizeWindow {
    fn body(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.label(RichText::new(t!("optimize.intro")).weak().small());
        ui.horizontal_wrapped(|ui| {
            if state.runtime.optimize.running {
                ui.spinner();
                ui.label(t!("optimize.running"));
            } else if ui.button(t!("optimize.refresh")).on_hover_text(t!("optimize.refresh_tip")).clicked() {
                state.request(Action::AnalyzeOptimization);
            }
            let mut view = state.runtime.optimize.view;
            if ui.checkbox(&mut view, Action::ToggleVisleaves.label()).on_hover_text(t!("optimize.view_tip")).changed()
            {
                state.request(Action::ToggleVisleaves);
            }
            let mut heat = state.settings.health.portal_heat;
            if ui.checkbox(&mut heat, t!("optimize.heat")).changed() {
                state.settings.health.portal_heat = heat;
                state.settings_dirty = true;
            }
        });
        if let Some(e) = &state.runtime.optimize.error {
            ui.label(RichText::new(tf!("optimize.error", e)).color(theme::error()));
        }
        let Some(report) = state.runtime.optimize.report.clone() else {
            if !state.runtime.optimize.running {
                ui.label(RichText::new(t!("optimize.none")).weak());
            }
            return;
        };
        if state.runtime.optimize.stale(state.doc.scene_revision()) {
            ui.label(RichText::new(t!("optimize.stale")).color(theme::warning()).small());
        }
        if let Some(v) = &report.vis {
            ui.label(tf!(
                "optimize.summary",
                v.clusters,
                v.portals,
                v.areas,
                format!("{:.0}", v.average_pvs),
                v.max_pvs
            ));
        }
        if let Some(e) = &report.compile_error {
            ui.label(RichText::new(tf!("optimize.compile_failed", e)).color(theme::warning()).small());
        }
        ui.label(tf!(
            "optimize.counts",
            report.count(Impact::High),
            report.count(Impact::Medium),
            report.count(Impact::Low)
        ));
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(self.topic.is_none(), t!("optimize.all")).clicked() {
                self.topic = None;
            }
            for t in TOPICS {
                let n = report.findings.iter().filter(|f| f.topic == t).count();
                let text = format!("{} ({n})", topic_label(t));
                if ui.selectable_label(self.topic == Some(t), text).clicked() {
                    self.topic = Some(t);
                }
            }
        });
        ui.separator();
        if report.findings.is_empty() {
            ui.label(RichText::new(t!("optimize.clean")).color(theme::success()));
            return;
        }
        let topic = self.topic;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, f) in report.findings.iter().enumerate().filter(|(_, f)| topic.is_none_or(|t| f.topic == t)) {
                let selected = state.runtime.optimize.selected == Some(i);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (label, color) = impact_label(f.impact);
                        ui.label(RichText::new(label).color(color).strong());
                        ui.label(RichText::new(topic_label(f.topic)).weak());
                        let can_go = !f.objects.is_empty() || !f.focus().is_empty();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(can_go, egui::Button::new(t!("optimize.go_to")).selected(selected))
                                .on_hover_text(t!("optimize.go_to_tip"))
                                .clicked()
                            {
                                state.runtime.optimize.selected = Some(i);
                                go_to(state, f);
                            }
                            if f.code == DETAIL_CODE
                                && ui
                                    .button(t!("optimize.select_these"))
                                    .on_hover_text(t!("optimize.select_these_tip"))
                                    .clicked()
                            {
                                state.runtime.optimize.selected = Some(i);
                                select_detail(state, f);
                            }
                        });
                    });
                    ui.label(finding_text(f));
                    ui.label(RichText::new(why(f.topic)).weak().small());
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use hp_app::testing::{run_ctx, test_state};
    use hp_editor::optimize::{Finding, Impact, OptimizeReport, Topic};

    use super::*;

    #[test]
    fn window_draws_every_state_and_goes_to_findings() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        let mut w = OptimizeWindow::default();
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        state.runtime.optimize.show = true;
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        state.runtime.optimize.running = true;
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        state.runtime.optimize.running = false;
        let point = Finding::new(Topic::Hints, "hint_doorway_top", Impact::Medium, "m".to_owned())
            .points(vec![DVec3::new(100.0, 200.0, 300.0)])
            .args(vec!["100 200 300".to_owned(), "z".to_owned(), "300".to_owned(), "40".to_owned()]);
        let obj = Finding::new(Topic::Lights, "light_dark", Impact::Low, "m".to_owned())
            .objects(vec![hp_editor::ObjRef::Solid(2)])
            .args(vec!["2".to_owned(), "0 0 0".to_owned()]);
        let unknown = Finding::new(Topic::Visleaves, "no_such_code", Impact::High, "raw message".to_owned());
        state.runtime.optimize.report = Some(Arc::new(OptimizeReport {
            findings: vec![unknown.clone(), point.clone(), obj.clone()],
            vis: Some(hp_editor::optimize::vis::VisReport {
                clusters: 12,
                portals: 20,
                areas: 2,
                ..Default::default()
            }),
            revision: state.doc.scene_revision(),
            ..Default::default()
        }));
        for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr, hp_app::i18n::Lang::Ru] {
            hp_app::i18n::set_lang(lang);
            run_ctx(&mut state, |ctx, state| w.show(ctx, state));
            w.topic = Some(Topic::Lights);
            run_ctx(&mut state, |ctx, state| w.show(ctx, state));
            w.topic = None;
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(finding_text(&unknown), "raw message");
        assert!(finding_text(&point).contains("100 200 300"));
        go_to(&mut state, &point);
        assert!((state.cameras.cam3d.pos - DVec3::new(100.0, 200.0, 300.0)).length() > 10.0);
        go_to(&mut state, &obj);
        assert!(state.doc.selection().objects.iter().eq([hp_editor::ObjRef::Solid(2)].iter()));
        let detail = Finding::new(Topic::Detail, DETAIL_CODE, Impact::Medium, "m".to_owned())
            .objects(vec![hp_editor::ObjRef::Solid(2), hp_editor::ObjRef::Solid(999_999)]);
        let before = state.doc.scene_revision();
        let n = select_detail(&mut state, &detail);
        let world_two = state.doc.scene().solid(2).is_some_and(|s| s.owner == hp_editor::Owner::World);
        assert_eq!(n, usize::from(world_two));
        assert_eq!(state.doc.scene_revision(), before, "selecting never edits the map");
        state.runtime.optimize.error = Some("x".to_owned());
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
    }
}
