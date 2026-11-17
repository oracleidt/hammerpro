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

const TOPICS: [Topic; 7] = [
    Topic::Visleaves,
    Topic::Detail,
    Topic::Hints,
    Topic::Areaportals,
    Topic::Lights,
    Topic::Cubemaps,
    Topic::Overlays,
];

fn topic_label(t: Topic) -> &'static str {
    match t {
        Topic::Visleaves => t!("optimize.topic.visleaves"),
        Topic::Detail => t!("optimize.topic.func_detail"),
        Topic::Hints => t!("optimize.topic.hints"),
        Topic::Areaportals => t!("optimize.topic.areaportals"),
        Topic::Lights => t!("optimize.topic.lights"),
        Topic::Cubemaps => t!("optimize.topic.cubemaps"),
        Topic::Overlays => t!("optimize.topic.overlays"),
    }
}

fn why(t: Topic) -> &'static str {
    match t {
        Topic::Visleaves => t!("optimize.why.visleaves"),
        Topic::Detail => t!("optimize.why.func_detail"),
        Topic::Hints => t!("optimize.why.hints"),
        Topic::Areaportals => t!("optimize.why.areaportals"),
        Topic::Lights => t!("optimize.why.lights"),
        Topic::Cubemaps => t!("optimize.why.cubemaps"),
        Topic::Overlays => t!("optimize.why.overlays"),
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

fn is_overlay_fix(f: &Finding) -> bool {
    f.fix == Some(hp_editor::optimize::overlays::FIX)
}

fn finding_overlays(f: &Finding) -> impl Iterator<Item = hp_editor::Id> + '_ {
    f.objects.iter().filter_map(|o| match *o {
        hp_editor::ObjRef::Entity(id) => Some(id),
        hp_editor::ObjRef::Solid(_) => None,
    })
}

pub fn fix_overlays(state: &mut AppState, ids: &[hp_editor::Id]) -> Option<hp_editor::optimize::overlays::FixReport> {
    let fresh = !state.runtime.optimize.stale(state.doc.scene_revision());
    match state.doc.fix_overlay_faces(ids) {
        Ok(r) if r.overlays.is_empty() => {
            state.notify(hp_app::Severity::Info, t!("optimize.overlay_fix_nothing"));
            Some(r)
        }
        Ok(r) => {
            state.notify(
                hp_app::Severity::Success,
                tf!("optimize.overlay_fixed", r.overlays.len(), r.faces_before, r.faces_after, r.created),
            );
            let fixed: std::collections::HashSet<hp_editor::Id> = r.overlays.iter().map(|o| o.id).collect();
            let revision = state.doc.scene_revision();
            if let Some(report) = state.runtime.optimize.report.as_mut() {
                let report = std::sync::Arc::make_mut(report);
                report.findings.retain(|f| !(is_overlay_fix(f) && finding_overlays(f).any(|id| fixed.contains(&id))));
                if fresh {
                    report.revision = revision;
                }
            }
            state.runtime.optimize.selected = None;
            Some(r)
        }
        Err(e) => {
            state.notify(hp_app::Severity::Warning, tf!("optimize.overlay_fix_failed", e));
            None
        }
    }
}

pub fn fix_all_overlays(state: &mut AppState) -> Option<hp_editor::optimize::overlays::FixReport> {
    let ids: Vec<hp_editor::Id> = state
        .runtime
        .optimize
        .report
        .as_ref()
        .map(|r| r.findings.iter().filter(|f| is_overlay_fix(f)).flat_map(finding_overlays).collect())
        .unwrap_or_default();
    if ids.is_empty() {
        state.notify(hp_app::Severity::Info, t!("optimize.overlay_fix_nothing"));
        return None;
    }
    fix_overlays(state, &ids)
}

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
        let fixable = report.findings.iter().filter(|f| is_overlay_fix(f)).count();
        if fixable > 0
            && self.topic.is_none_or(|t| t == Topic::Overlays)
            && ui
                .button(tf!("optimize.fix_all_overlays", fixable))
                .on_hover_text(t!("optimize.fix_all_overlays_tip"))
                .clicked()
        {
            fix_all_overlays(state);
        }
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
                            if is_overlay_fix(f)
                                && ui.button(t!("optimize.fix")).on_hover_text(t!("optimize.fix_overlay_tip")).clicked()
                            {
                                let ids: Vec<hp_editor::Id> = finding_overlays(f).collect();
                                fix_overlays(state, &ids);
                            }
                            if f.topic == Topic::Cubemaps
                                && ui
                                    .button(t!("optimize.open_cubemaps"))
                                    .on_hover_text(t!("optimize.open_cubemaps_tip"))
                                    .clicked()
                            {
                                state.request(Action::ShowCubemaps);
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
            findings: vec![
                unknown.clone(),
                point.clone(),
                obj.clone(),
                Finding::new(Topic::Cubemaps, "cubemap_far", Impact::Low, "m".to_owned())
                    .args(vec!["2".to_owned(), "1024".to_owned()]),
                Finding::new(Topic::Overlays, "overlay_many_faces", Impact::Medium, "m".to_owned())
                    .objects(vec![hp_editor::ObjRef::Entity(1)])
                    .args(vec!["1".to_owned(), "20".to_owned(), "19".to_owned(), "x".to_owned(), "64".to_owned()])
                    .fix(hp_editor::optimize::overlays::FIX),
            ],
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

    #[test]
    fn fix_buttons_trim_overlays_in_one_step_and_drop_their_findings() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let ov = state
            .doc
            .create_point_entity(
                "info_overlay",
                DVec3::ZERO,
                &[
                    kv("material", "overlays/road01"),
                    kv("sides", "1 7"),
                    kv("BasisOrigin", "0 0 0"),
                    kv("BasisNormal", "0 0 1"),
                    kv("BasisU", "1 0 0"),
                    kv("BasisV", "0 1 0"),
                    kv("uv0", "-32 -32 0"),
                    kv("uv1", "-32 32 0"),
                    kv("uv2", "32 32 0"),
                    kv("uv3", "32 -32 0"),
                ],
            )
            .expect("overlay");
        let findings = hp_editor::optimize::overlays::findings(&state.doc);
        assert_eq!(findings.len(), 1);
        assert!(is_overlay_fix(&findings[0]));
        state.runtime.optimize.report =
            Some(Arc::new(OptimizeReport { findings, revision: state.doc.scene_revision(), ..Default::default() }));
        state.runtime.optimize.show = true;
        let mut w = OptimizeWindow::default();
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        let undo = state.doc.history_len().0;
        let r = fix_all_overlays(&mut state).expect("fixed");
        assert_eq!((r.faces_before, r.faces_after, r.created), (2, 1, 0));
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
        assert_eq!(state.doc.scene().entity(ov).unwrap().entity.get("sides"), Some("1"));
        let report = state.runtime.optimize.report.clone().expect("report");
        assert!(report.findings.is_empty(), "the fixed finding is gone");
        assert!(!state.runtime.optimize.stale(state.doc.scene_revision()));
        assert!(fix_all_overlays(&mut state).is_none());
        assert!(fix_overlays(&mut state, &[ov]).is_some_and(|r| r.overlays.is_empty()));
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
    }
}
