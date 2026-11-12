use std::collections::HashSet;

use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::runtime::HealthRuntime;
use hp_app::{Action, AppState, Severity, t, tf};
use hp_editor::limits::checks::{DetailReason, detail_reason};
use hp_editor::{ObjRef, Owner, Problem, Severity as ProblemSeverity};

use crate::util::{dialog_body, dialog_bottom_bar, escape_pressed, icon_title, plural, request_center};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DetailRow {
    pub(crate) id: u32,
    pub(crate) size: DVec3,
    pub(crate) faces: usize,
    pub(crate) reason: Option<DetailReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DetailStatus {
    Checking,
    Leaks,
    Incomplete,
    NoWorld,
    Nothing,
    List,
}

#[derive(Debug, Default)]
pub(crate) struct DetailWindow {
    pub(crate) rows: Vec<DetailRow>,
    pub(crate) status: Option<DetailStatus>,
    pub(crate) unchecked: HashSet<u32>,
    key: Option<(u64, u64)>,
    selected: Option<u32>,
}

impl DetailWindow {
    pub(crate) fn update_rows(&mut self, state: &AppState) {
        let rt = &state.runtime.health;
        let key = (rt.revision, state.doc.revision());
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        let Some(leak) = &rt.leak else {
            self.rows.clear();
            self.status = Some(DetailStatus::Checking);
            return;
        };
        let report = &leak.report;
        let status = if report.incomplete {
            DetailStatus::Incomplete
        } else if report.leaks() {
            DetailStatus::Leaks
        } else if report.no_world {
            DetailStatus::NoWorld
        } else {
            DetailStatus::List
        };
        let candidates: HashSet<u32> = rt
            .detail_candidates()
            .iter()
            .filter_map(|o| if let ObjRef::Solid(id) = o { Some(*id) } else { None })
            .collect();
        self.rows.clear();
        if status == DetailStatus::List && !candidates.is_empty() {
            let scene = state.doc.scene();
            let still_world = |id: u32| scene.solid(id).is_some_and(|n| n.owner == Owner::World);
            if let Some(checks) = &rt.checks {
                for s in checks.snapshot.solids.iter().filter(|s| candidates.contains(&s.id) && still_world(s.id)) {
                    let size = s.geom.aabb.size();
                    let extent = size.x.max(size.y).max(size.z);
                    let faces = s.geom.faces.len();
                    self.rows.push(DetailRow { id: s.id, size, faces, reason: detail_reason(extent, faces, s.axial) });
                }
            }
            let listed: HashSet<u32> = self.rows.iter().map(|r| r.id).collect();
            for &id in candidates.iter().filter(|id| !listed.contains(id)) {
                let Some(n) = scene.solid(id).filter(|n| n.owner == Owner::World) else { continue };
                self.rows.push(DetailRow { id, size: n.geom.aabb.size(), faces: n.geom.faces.len(), reason: None });
            }
            self.rows.sort_by_key(|r| r.id);
        }
        self.status =
            Some(if status == DetailStatus::List && self.rows.is_empty() { DetailStatus::Nothing } else { status });
        let ids: HashSet<u32> = self.rows.iter().map(|r| r.id).collect();
        self.unchecked.retain(|id| ids.contains(id));
    }

    pub(crate) fn checked(&self) -> Vec<ObjRef> {
        self.rows.iter().filter(|r| !self.unchecked.contains(&r.id)).map(|r| ObjRef::Solid(r.id)).collect()
    }

    pub(crate) fn convert(&mut self, state: &mut AppState) {
        let objects = self.checked();
        if objects.is_empty() {
            return;
        }
        let n = objects.len();
        let p = Problem::new(ProblemSeverity::Info, "detail_candidate", String::new(), objects, true);
        match state.doc.fix_problem(&p) {
            Ok(true) => state.notify(
                Severity::Success,
                tf!(plural(n, "problems.detail.converted_one", "problems.detail.converted"), n),
            ),
            Ok(false) => state.notify(Severity::Info, t!("problems.nothing_fixed")),
            Err(e) => state.notify(Severity::Error, tf!("problems.fix_failed", e)),
        }
        state.runtime.health.recheck = true;
    }

    pub(crate) fn go_to(&mut self, state: &mut AppState, id: u32) {
        self.selected = Some(id);
        if state.doc.scene().contains(ObjRef::Solid(id)) {
            state.doc.set_selection(&[ObjRef::Solid(id)]);
            request_center(state);
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.health.show_detail {
            return;
        }
        let mut open = true;
        let response = hp_app::chrome::Window::new(icon_title("⚙", t!("problems.detail.title")))
            .id(egui::Id::unique("hp_detail_suggestions"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(560.0, 420.0)))
            .open(&mut open)
            .default_size([560.0, 420.0])
            .min_size([380.0, 220.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| self.window_ui(ui, state));
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if !open {
            state.runtime.health.show_detail = false;
        }
        let rt = &state.runtime.health;
        if rt.checking || rt.leak_checking {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.update_rows(state);
        dialog_bottom_bar(ui, "hp_detail_bottom", |ui| {
            ui.horizontal(|ui| {
                let checked = self.checked();
                let n = checked.len();
                let convert = ui
                    .add_enabled(n > 0, egui::Button::new(tf!("problems.detail.convert", n)))
                    .on_hover_text(t!("problems.detail.convert_tip"));
                if convert.clicked() {
                    self.convert(state);
                }
                if ui.add_enabled(n > 0, egui::Button::new(t!("problems.detail.select"))).clicked() {
                    state.doc.set_selection(&checked);
                    request_center(state);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t!("common.close")).on_hover_text(key_label(egui::Key::Escape)).clicked() {
                        state.runtime.health.show_detail = false;
                    }
                });
            });
        });
        dialog_body(ui, |ui| self.body_ui(ui, state));
    }

    fn body_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.label(RichText::new(t!("problems.detail.intro")).small());
        ui.add_space(4.0);
        let busy = state.runtime.health.checking || state.runtime.health.leak_checking;
        let message = match self.status {
            None | Some(DetailStatus::Checking) => Some(t!("problems.detail.checking")),
            Some(DetailStatus::Leaks) => Some(t!("problems.detail.leaks")),
            Some(DetailStatus::Incomplete) => Some(t!("problems.detail.incomplete")),
            Some(DetailStatus::NoWorld) => Some(t!("problems.detail.no_world")),
            Some(DetailStatus::Nothing) => Some(t!("problems.detail.nothing")),
            Some(DetailStatus::List) => None,
        };
        if let Some(text) = message {
            ui.horizontal(|ui| {
                if busy {
                    ui.spinner();
                }
                ui.label(RichText::new(text).weak());
            });
            if self.status == Some(DetailStatus::Leaks) && ui.button(t!("problems.detail.show_leak")).clicked() {
                state.request(Action::CheckLeaks);
            }
            return;
        }
        ui.horizontal(|ui| {
            let total = self.rows.len();
            let checked = total - self.unchecked.len().min(total);
            ui.label(tf!(plural(total, "problems.detail.count_one", "problems.detail.count"), total, checked));
            if busy {
                ui.spinner();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(t!("problems.detail.check_none")).clicked() {
                    self.unchecked = self.rows.iter().map(|r| r.id).collect();
                }
                if ui.small_button(t!("problems.detail.check_all")).clicked() {
                    self.unchecked.clear();
                }
            });
        });
        ui.separator();
        let row_h = ui.spacing().interact_size.y;
        let mut go: Option<u32> = None;
        let rows = &self.rows;
        let unchecked = &mut self.unchecked;
        let selected = self.selected;
        egui::ScrollArea::vertical().id_salt("hp_detail_rows").auto_shrink([false, false]).show_rows(
            ui,
            row_h,
            rows.len(),
            |ui, range| {
                for r in rows.get(range).unwrap_or(&[]) {
                    ui.horizontal(|ui| {
                        let mut on = !unchecked.contains(&r.id);
                        if ui.checkbox(&mut on, "").changed() {
                            if on {
                                unchecked.remove(&r.id);
                            } else {
                                unchecked.insert(r.id);
                            }
                        }
                        let label = tf!("problems.detail.row", r.id);
                        let resp = ui
                            .selectable_label(selected == Some(r.id), label)
                            .on_hover_text(t!("problems.detail.row_tip"));
                        if resp.clicked() {
                            go = Some(r.id);
                        }
                        let size = format!("{:.0} x {:.0} x {:.0}", r.size.x, r.size.y, r.size.z);
                        ui.label(RichText::new(size).monospace().small());
                        let faces = tf!(plural(r.faces, "problems.detail.faces_one", "problems.detail.faces"), r.faces);
                        ui.label(RichText::new(faces).weak().small());
                        let reason = match r.reason {
                            Some(DetailReason::Small) => t!("problems.detail.small"),
                            Some(DetailReason::Complex) => t!("problems.detail.complex"),
                            None => "",
                        };
                        if !reason.is_empty() {
                            ui.label(RichText::new(reason).weak().small());
                        }
                    });
                }
            },
        );
        if let Some(id) = go {
            self.go_to(state, id);
        }
    }
}

pub(crate) fn open(state: &mut AppState) {
    HealthRuntime::show_detail_suggestions(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::testing::{run_ctx, test_state};

    fn box_sides(min: DVec3, max: DVec3) -> Vec<hp_vmf::Side> {
        let (x0, y0, z0, x1, y1, z1) = (min.x, min.y, min.z, max.x, max.y, max.z);
        let v = DVec3::new;
        let faces = [
            ([v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)], (DVec3::X, DVec3::NEG_Y)),
            ([v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)], (DVec3::X, DVec3::NEG_Y)),
            ([v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)], (DVec3::X, DVec3::NEG_Z)),
            ([v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)], (DVec3::X, DVec3::NEG_Z)),
            ([v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)], (DVec3::Y, DVec3::NEG_Z)),
            ([v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)], (DVec3::Y, DVec3::NEG_Z)),
        ];
        faces
            .iter()
            .map(|(plane, (u, w))| hp_vmf::Side {
                plane: *plane,
                material: "BRICK/WALL".to_owned(),
                uaxis: hp_vmf::TexAxis { axis: *u, shift: 0.0, scale: 0.25 },
                vaxis: hp_vmf::TexAxis { axis: *w, shift: 0.0, scale: 0.25 },
                ..Default::default()
            })
            .collect()
    }

    fn sealed_room(state: &mut AppState) -> u32 {
        let mut doc = hp_editor::Document::new_empty();
        let (s, w) = (256.0, 16.0);
        let wall = |doc: &mut hp_editor::Document, a: DVec3, b: DVec3| {
            doc.create_solid(box_sides(a, b), Owner::World).expect("wall")
        };
        wall(&mut doc, DVec3::new(-s - w, -s - w, -s - w), DVec3::new(-s, s + w, s + w));
        wall(&mut doc, DVec3::new(s, -s - w, -s - w), DVec3::new(s + w, s + w, s + w));
        wall(&mut doc, DVec3::new(-s, -s - w, -s - w), DVec3::new(s, -s, s + w));
        wall(&mut doc, DVec3::new(-s, s, -s - w), DVec3::new(s, s + w, s + w));
        wall(&mut doc, DVec3::new(-s, -s, -s - w), DVec3::new(s, s, -s));
        wall(&mut doc, DVec3::new(-s, -s, s), DVec3::new(s, s, s + w));
        let pillar = wall(&mut doc, DVec3::new(-32.0, -32.0, -256.0), DVec3::new(32.0, 32.0, -200.0));
        doc.create_point_entity("info_player_start", DVec3::new(100.0, 100.0, -240.0), &[]).expect("spawn");
        state.doc = doc;
        pillar
    }

    #[test]
    fn suggestions_list_go_to_and_convert() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        let pillar = sealed_room(&mut state);
        let mut w = DetailWindow::default();
        w.update_rows(&state);
        assert_eq!(w.status, Some(DetailStatus::Checking));
        state.runtime.health.show_detail = true;
        HealthRuntime::check_now(&mut state);
        w.update_rows(&state);
        assert_eq!(w.status, Some(DetailStatus::List));
        assert_eq!(w.rows.len(), 1);
        assert_eq!((w.rows[0].id, w.rows[0].faces, w.rows[0].reason), (pillar, 6, Some(DetailReason::Small)));
        for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
            hp_app::i18n::set_lang(lang);
            run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        w.go_to(&mut state, pillar);
        assert_eq!(state.doc.selection().objects.iter().copied().collect::<Vec<_>>(), vec![ObjRef::Solid(pillar)]);
        w.unchecked.insert(pillar);
        assert!(w.checked().is_empty());
        w.convert(&mut state);
        assert_eq!(state.doc.scene().solid(pillar).map(|n| n.owner), Some(Owner::World));
        w.unchecked.clear();
        w.convert(&mut state);
        assert!(matches!(state.doc.scene().solid(pillar).map(|n| n.owner), Some(Owner::Entity(_))));
        assert_eq!(state.doc.undo_label(), Some("Convert to func_detail"));
        assert!(hp_app::i18n::has_key("shell.undo.convert_to_func_detail"));
        assert!(state.runtime.health.recheck);
        HealthRuntime::check_now(&mut state);
        w.update_rows(&state);
        assert_eq!(w.status, Some(DetailStatus::Nothing));
        assert!(w.rows.is_empty() && w.unchecked.is_empty());
        state.runtime.health.show_detail = false;
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
    }

    #[test]
    fn leaking_maps_get_no_suggestions() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        HealthRuntime::check_now(&mut state);
        let mut w = DetailWindow::default();
        w.update_rows(&state);
        assert!(matches!(w.status, Some(DetailStatus::Leaks | DetailStatus::NoWorld)), "{:?}", w.status);
        assert!(w.rows.is_empty());
        state.runtime.health.show_detail = true;
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        let mut state = test_state();
        state.settings.health.live_leak_check = false;
        open(&mut state);
        assert!(state.runtime.health.show_detail);
        assert!(state.runtime.health.leak_requested && state.runtime.health.recheck);
    }
}
