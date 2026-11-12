use egui::{RichText, Sense, Ui, vec2};
use glam::DVec3;
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::{Action, AppState, Severity, ToolId, t, tf, theme};
use hp_geom::Aabb;

use crate::dialogs::MapDialog;
use crate::util::{focus_select_all, plural};

const ROW_HEIGHT: f32 = 22.0;
const MAX_ROWS: usize = 256;
const NEW_HALF_SIZE: f64 = 256.0;

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(crate::visgroups::MoveToVisgroupDialog::default())]
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CordonAction {
    SetEnabled(bool),
    SetActive(usize, bool),
    StartRename(usize),
    CommitRename(usize, String),
    CancelRename,
    Delete(usize),
    Target(usize),
    EditBox(usize),
    Focus(usize),
    New,
}

#[derive(Clone, Debug)]
struct Rename {
    index: usize,
    text: String,
    focus_requested: bool,
}

#[derive(Debug, Default)]
pub(crate) struct CordonList {
    rename: Option<Rename>,
}

const MAX_SHOWN_NAME: usize = 96;

pub(crate) fn shown_name(name: &str) -> &str {
    match name.char_indices().nth(MAX_SHOWN_NAME) {
        Some((cut, _)) => &name[..cut],
        None => name,
    }
}

fn cordon_box(c: &hp_vmf::Cordon) -> Option<Aabb> {
    let b = Aabb::new(c.mins, c.maxs);
    (c.mins.is_finite() && c.maxs.is_finite() && b.size().min_element() > 1e-3).then_some(b)
}

pub(crate) fn new_cordon_box(state: &AppState) -> Aabb {
    let grid = if state.grid_size().is_finite() && state.grid_size() > 0.0 { state.grid_size() } else { 16.0 };
    if let Some(b) = state.doc.selection_bounds(&|e| state.entity_bounds(e))
        && b.min.is_finite()
        && b.max.is_finite()
    {
        let grow = DVec3::from_array([0, 1, 2].map(|i| if b.size()[i] < 1.0 { grid } else { 0.0 }));
        return Aabb::new(b.min - grow, b.max + grow);
    }
    let top = state.cameras.top.center;
    let front = state.cameras.front.center;
    let center = DVec3::new(top.x, top.y, front.y);
    let center = if center.is_finite() { (center / grid).round() * grid } else { DVec3::ZERO };
    Aabb::new(center - DVec3::splat(NEW_HALF_SIZE), center + DVec3::splat(NEW_HALF_SIZE))
}

impl CordonList {
    pub(crate) fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let mut actions: Vec<CordonAction> = Vec::new();
        let n = state.doc.scene().cordons().cordons.len();
        let header_id = ui.make_persistent_id("hp_cordons_header");
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), header_id, true)
            .show_header(ui, |ui| {
                ui.label(RichText::new(t!("cordons.title")).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("➕").on_hover_text(t!("cordons.new_tip")).clicked() {
                        actions.push(CordonAction::New);
                    }
                    let mut on = state.doc.scene().cordons().active;
                    if ui.checkbox(&mut on, t!("cordons.use")).on_hover_text(t!("cordons.use_tip")).changed() {
                        actions.push(CordonAction::SetEnabled(on));
                    }
                    ui.label(RichText::new(tf!(plural(n, "cordons.count_one", "cordons.count"), n)).weak().small());
                });
            })
            .body(|ui| {
                if n == 0 {
                    ui.label(RichText::new(t!("cordons.empty")).weak().italics());
                }
                let target = edited_cordon(state);
                for i in 0..n.min(MAX_ROWS) {
                    self.row_ui(ui, state, i, target == Some(i), &mut actions);
                }
                if n > MAX_ROWS {
                    ui.label(RichText::new(tf!("cordons.more", n - MAX_ROWS)).weak());
                }
            });
        self.apply(state, actions);
    }

    fn row_ui(&mut self, ui: &mut Ui, state: &AppState, i: usize, target: bool, actions: &mut Vec<CordonAction>) {
        let Some(c) = state.doc.scene().cordons().cordons.get(i) else { return };
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click());
        let painter = ui.painter_at(rect);
        if target {
            painter.rect_filled(rect, 3.0, ui.visuals().selection.bg_fill.gamma_multiply(0.6));
        } else if resp.hovered() {
            painter.rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
        }
        let mut x = rect.left() + 4.0;
        let check_rect = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 1.0), vec2(20.0, ROW_HEIGHT - 2.0));
        let mut active = c.active;
        let check =
            ui.put(check_rect, egui::Checkbox::without_text(&mut active)).on_hover_text(t!("cordons.active_tip"));
        if check.changed() {
            actions.push(CordonAction::SetActive(i, active));
        }
        x += 24.0;
        let button = |ui: &mut Ui, right: f32, icon: &str, tip: &str| {
            let r = egui::Rect::from_min_size(egui::pos2(right - 22.0, rect.top() + 1.0), vec2(22.0, ROW_HEIGHT - 2.0));
            ui.put(r, egui::Button::new(icon).frame(false)).on_hover_text(tip).clicked()
        };
        let mut right = rect.right() - 2.0;
        if button(ui, right, "✖", t!("cordons.delete")) {
            actions.push(CordonAction::Delete(i));
        }
        right -= 24.0;
        if button(ui, right, "🎯", t!("cordons.focus")) {
            actions.push(CordonAction::Focus(i));
        }
        right -= 24.0;
        if button(ui, right, "⛶", t!("cordons.edit_box")) {
            actions.push(CordonAction::EditBox(i));
        }
        right -= 26.0;
        let name_rect =
            egui::Rect::from_min_max(egui::pos2(x, rect.top() + 1.0), egui::pos2(right, rect.bottom() - 1.0));
        let renaming = self.rename.as_ref().is_some_and(|r| r.index == i);
        if renaming {
            if let Some(r) = &mut self.rename {
                let edit = ui.put(name_rect, egui::TextEdit::singleline(&mut r.text).desired_width(name_rect.width()));
                if !r.focus_requested {
                    focus_select_all(&edit, &r.text);
                    r.focus_requested = true;
                }
                if edit.lost_focus() {
                    let escape = ui.input_mut(|inp| {
                        let esc = inp.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                        inp.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                        esc
                    });
                    actions.push(if escape {
                        CordonAction::CancelRename
                    } else {
                        CordonAction::CommitRename(i, r.text.clone())
                    });
                }
            }
        } else {
            let color = if c.active { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
            let galley =
                painter.layout_no_wrap(shown_name(&c.name).to_owned(), egui::FontId::proportional(13.0), color);
            let pos = egui::pos2(name_rect.left(), rect.center().y - galley.size().y * 0.5);
            ui.painter().with_clip_rect(hp_app::widgets::clip_x(name_rect, ui.clip_rect())).galley(pos, galley, color);
        }
        let bounds = cordon_box(c);
        let resp = resp.on_hover_ui(|ui| {
            ui.label(match bounds {
                Some(b) => tf!(
                    "cordons.row_tip",
                    hp_app::widgets::fmt_vec3(b.min),
                    hp_app::widgets::fmt_vec3(b.max),
                    hp_app::widgets::fmt_vec3(b.size())
                ),
                None => t!("cordons.row_tip_empty").to_owned(),
            });
        });
        if resp.double_clicked() && !renaming {
            actions.push(CordonAction::StartRename(i));
        } else if resp.clicked() {
            actions.push(CordonAction::Target(i));
        }
        resp.context_menu(|ui| {
            for (label, action) in [
                (t!("cordons.edit_box"), CordonAction::EditBox(i)),
                (t!("cordons.focus"), CordonAction::Focus(i)),
                (t!("cordons.rename"), CordonAction::StartRename(i)),
            ] {
                if ui.button(label).clicked() {
                    actions.push(action);
                    ui.close();
                }
            }
            if ui.button(RichText::new(t!("cordons.delete")).color(theme::error())).clicked() {
                actions.push(CordonAction::Delete(i));
                ui.close();
            }
        });
    }

    pub(crate) fn apply(&mut self, state: &mut AppState, actions: Vec<CordonAction>) {
        for action in actions {
            let result = match action {
                CordonAction::SetEnabled(on) => {
                    if on != state.doc.scene().cordons().active {
                        state.request(Action::ToggleCordon);
                    }
                    Ok(())
                }
                CordonAction::SetActive(i, on) => state.doc.set_cordon_active(i, on),
                CordonAction::StartRename(i) => {
                    if let Some(c) = state.doc.scene().cordons().cordons.get(i) {
                        self.rename = Some(Rename { index: i, text: c.name.clone(), focus_requested: false });
                    }
                    Ok(())
                }
                CordonAction::CommitRename(i, name) => {
                    self.rename = None;
                    let name = name.trim();
                    if name.is_empty() { Ok(()) } else { state.doc.rename_cordon(i, name) }
                }
                CordonAction::CancelRename => {
                    self.rename = None;
                    Ok(())
                }
                CordonAction::Delete(i) => {
                    self.rename = None;
                    let r = state.doc.remove_cordon(i);
                    let edit = &mut state.runtime.visibility.edit_cordon;
                    *edit = match *edit {
                        Some(e) if e == i => None,
                        Some(e) if e > i => Some(e - 1),
                        other => other,
                    };
                    r
                }
                CordonAction::Target(i) => {
                    state.runtime.visibility.edit_cordon = Some(i);
                    Ok(())
                }
                CordonAction::EditBox(i) => {
                    state.runtime.visibility.edit_cordon = Some(i);
                    state.set_tool(ToolId::Cordon);
                    Ok(())
                }
                CordonAction::Focus(i) => {
                    state.runtime.visibility.focus_box =
                        state.doc.scene().cordons().cordons.get(i).and_then(cordon_box);
                    Ok(())
                }
                CordonAction::New => {
                    let b = new_cordon_box(state);
                    let name = state.doc.unique_cordon_name(t!("cordons.new_name"));
                    match state.doc.add_cordon(&name, b) {
                        Ok(i) => {
                            state.runtime.visibility.edit_cordon = Some(i);
                            self.rename = Some(Rename { index: i, text: name, focus_requested: false });
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
            };
            if let Err(e) = result {
                state.notify(Severity::Warning, tf!("cordons.error", e));
            }
        }
        if let Some(r) = &self.rename
            && r.index >= state.doc.scene().cordons().cordons.len()
        {
            self.rename = None;
        }
    }
}

fn edited_cordon(state: &AppState) -> Option<usize> {
    let n = state.doc.scene().cordons().cordons.len();
    match state.runtime.visibility.edit_cordon {
        Some(i) if i < n => Some(i),
        _ => (n > 0).then_some(0),
    }
}

#[derive(Debug, Default)]
pub struct CordonStatus;

impl StatusSegment for CordonStatus {
    fn id(&self) -> &'static str {
        "visibility_cordons"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::GRID + 50
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        if !state.doc.cordons_in_use() {
            return;
        }
        ui.separator();
        let n = state.doc.cordon_box_count();
        let text = RichText::new(format!("▣ {}", t!("cordons.status"))).small().color(theme::error());
        let tip = tf!(plural(n, "cordons.status_tip_one", "cordons.status_tip"), n);
        if ui.add(egui::Button::new(text).frame(false)).on_hover_text(tip).clicked() {
            state.request(Action::ToggleCordon);
        }
    }
}

#[cfg(test)]
mod tests {
    use hp_app::testing::{run_ui, test_state};
    use hp_editor::ObjRef;

    use super::*;

    #[test]
    fn actions_apply() {
        let mut state = test_state();
        let mut list = CordonList::default();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        list.apply(&mut state, vec![CordonAction::New]);
        let cordons = state.doc.scene().cordons().cordons.clone();
        assert_eq!(cordons.len(), 2);
        assert_eq!(
            (cordons[1].mins, cordons[1].maxs),
            (DVec3::new(-256.0, 256.0, 0.0), DVec3::new(256.0, 272.0, 128.0))
        );
        assert_eq!(state.runtime.visibility.edit_cordon, Some(1));
        assert!(list.rename.is_some(), "a new cordon starts a rename");
        list.apply(&mut state, vec![CordonAction::CommitRename(1, "  Room ".into())]);
        assert_eq!(state.doc.scene().cordons().cordons[1].name, "Room");
        list.apply(&mut state, vec![CordonAction::SetActive(1, false)]);
        assert!(!state.doc.scene().cordons().cordons[1].active);
        list.apply(&mut state, vec![CordonAction::Target(0)]);
        assert_eq!((state.runtime.visibility.edit_cordon, state.requested_tool), (Some(0), None));
        list.apply(&mut state, vec![CordonAction::EditBox(0)]);
        assert_eq!(state.requested_tool, Some(ToolId::Cordon));
        assert_eq!(state.runtime.visibility.edit_cordon, Some(0));
        list.apply(&mut state, vec![CordonAction::Focus(1)]);
        assert_eq!(state.runtime.visibility.focus_box.map(|b| b.max), Some(DVec3::new(256.0, 272.0, 128.0)));
        list.apply(&mut state, vec![CordonAction::SetEnabled(true)]);
        assert_eq!(state.requested_actions, vec![Action::ToggleCordon]);
        list.apply(&mut state, vec![CordonAction::Delete(0)]);
        assert_eq!(state.runtime.visibility.edit_cordon, None);
        assert_eq!(state.doc.scene().cordons().cordons[0].name, "Room");
        list.apply(&mut state, vec![CordonAction::Delete(9)]);
        assert!(!state.console.is_empty());
        state.doc.clear_selection();
        state.cameras.top.center = glam::DVec2::new(1001.0, 2000.0);
        state.cameras.front.center = glam::DVec2::new(2000.0, 500.0);
        let b = new_cordon_box(&state);
        assert_eq!(b.center(), DVec3::new(1008.0, 2000.0, 496.0), "snapped to the 16 grid");
    }

    #[test]
    fn icon_glyphs_exist() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let font = egui::FontId::proportional(13.0);
        for c in ['➕', '✖', '🎯', '⛶', '▣', '📂'] {
            assert!(ctx.fonts_mut(|f| f.has_glyph(&font, c)), "missing glyph {c}");
        }
    }

    #[test]
    fn long_names_are_cut_for_display() {
        let long = "é".repeat(500);
        assert_eq!(shown_name(&long).chars().count(), MAX_SHOWN_NAME);
        assert_eq!(shown_name("Room"), "Room");
        assert_eq!(shown_name(""), "");
    }

    #[test]
    fn section_and_badge_draw() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        let mut list = CordonList::default();
        run_ui(&mut state, |ui, state| list.ui(ui, state));
        state.doc.add_cordon("second", Aabb::new(DVec3::ZERO, DVec3::splat(64.0))).expect("add");
        state.doc.set_cordons_enabled(true).expect("on");
        list.apply(&mut state, vec![CordonAction::StartRename(1)]);
        let mut badge = CordonStatus;
        for lang in [hp_app::i18n::Lang::Fr, hp_app::i18n::Lang::En] {
            hp_app::i18n::set_lang(lang);
            run_ui(&mut state, |ui, state| {
                list.ui(ui, state);
                let mut cx = StatusContext::default();
                ui.horizontal(|ui| badge.ui(ui, state, &mut cx));
            });
        }
        assert!(list.rename.is_some());
    }
}
