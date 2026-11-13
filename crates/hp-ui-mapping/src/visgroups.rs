use std::collections::HashSet;
use std::time::{Duration, Instant};

use egui::{Align2, FontId, RichText, Sense, Stroke, Ui, UiBuilder, vec2};
use hp_app::{Action, AppState, Severity, t, tf, theme};
use hp_editor::{Document, Id, ObjRef, VisgroupState};
use hp_vmf::VisGroup;

use crate::autovis::{self, AutoKind, AutoVisData, GroupCount};
use crate::dialogs::MapDialog;
use crate::util::{DocKey, enter_pressed, escape_pressed, focus_select_all, icon_title, plural};

const AUTO_RECOUNT_INTERVAL: Duration = Duration::from_millis(250);

const ROW_HEIGHT: f32 = 22.0;
const INDENT: f32 = 14.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VisRow {
    pub id: Id,
    pub depth: usize,
    pub name: String,
    pub color: [u8; 3],
    pub count: usize,
    pub total: usize,
    pub state: VisgroupState,
    pub has_children: bool,
    pub expanded: bool,
}

pub(crate) fn flatten_visgroups(doc: &Document, collapsed: &HashSet<Id>) -> Vec<VisRow> {
    let scene = doc.scene();
    let counts = doc.visgroup_member_counts();
    let hidden = scene.hidden_visgroups();
    let mut out = Vec::new();

    struct Ctx<'a> {
        counts: &'a hp_editor::IdMap<usize>,
        hidden: &'a hp_editor::IdSet,
        collapsed: &'a HashSet<Id>,
    }

    fn visit(
        c: &Ctx<'_>,
        g: &VisGroup,
        depth: usize,
        parent_hidden: bool,
        emit: bool,
        out: &mut Vec<VisRow>,
    ) -> (bool, usize) {
        let own_hidden = c.hidden.contains(&g.id);
        let effective_hidden = parent_hidden || own_hidden;
        let expanded = !c.collapsed.contains(&g.id);
        let index = out.len();
        let count = c.counts.get(&g.id).copied().unwrap_or(0);
        if emit {
            out.push(VisRow {
                id: g.id,
                depth,
                name: crate::cordons::shown_name(&g.name).to_owned(),
                color: g.color,
                count,
                total: count,
                state: VisgroupState::Shown,
                has_children: !g.children.is_empty(),
                expanded,
            });
        }
        let mut child_hidden = false;
        let mut total = count;
        for child in &g.children {
            let (h, t) = visit(c, child, depth + 1, effective_hidden, emit && expanded, out);
            child_hidden |= h;
            total += t;
        }
        if emit && let Some(row) = out.get_mut(index) {
            row.total = total;
            row.state = if effective_hidden {
                VisgroupState::Hidden
            } else if child_hidden {
                VisgroupState::Partial
            } else {
                VisgroupState::Shown
            };
        }
        (own_hidden || child_hidden, total)
    }

    let c = Ctx { counts: &counts, hidden, collapsed };
    for g in scene.visgroups() {
        visit(&c, g, 0, false, true, &mut out);
    }
    out
}

pub(crate) fn subtree_members(doc: &Document, id: Id) -> Vec<ObjRef> {
    let Some(root) = doc.visgroup(id) else { return Vec::new() };
    let mut ids = Vec::new();
    let mut stack = vec![root];
    while let Some(g) = stack.pop() {
        ids.push(g.id);
        stack.extend(g.children.iter());
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for v in ids {
        for o in doc.visgroup_members(v) {
            if seen.insert(o) {
                out.push(o);
            }
        }
    }
    out
}

pub(crate) fn unique_visgroup_name(doc: &Document, base: &str) -> String {
    let mut names = HashSet::new();
    let mut stack: Vec<&VisGroup> = doc.scene().visgroups().iter().collect();
    while let Some(g) = stack.pop() {
        names.insert(g.name.to_lowercase());
        stack.extend(g.children.iter());
    }
    if !names.contains(&base.to_lowercase()) {
        return base.to_owned();
    }
    (2..).map(|k| format!("{base} {k}")).find(|n| !names.contains(&n.to_lowercase())).unwrap_or_else(|| base.to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VisgroupDrag(Id);

#[derive(Clone, Debug, PartialEq)]
enum VisAction {
    SetVisible(Id, bool),
    SetColor(Id, [u8; 3]),
    StartRename(Id),
    CommitRename(Id, String),
    CancelRename,
    SelectMembers(Id),
    AddSelection(Id),
    RemoveSelection(Id),
    Delete(Id),
    NewChild(Id),
    Move(Id, Option<Id>),
    Reorder(Id, i32),
    MoveSelectionHere(Id),
    ToggleExpanded(Id),
    Focus(Id),
}

#[derive(Clone, Debug)]
struct Rename {
    id: Id,
    text: String,
    focus_requested: bool,
}

#[derive(Default)]
pub struct VisgroupPanel {
    rows_key: Option<DocKey>,
    rows: Vec<VisRow>,
    collapsed: HashSet<Id>,
    rename: Option<Rename>,
    focused: Option<Id>,
    auto: AutoVisData,
    auto_doc: Option<DocKey>,
    auto_fgd: Option<(u64, bool)>,
    last_recount: Option<Instant>,
    force_recount: bool,
    cordons: crate::cordons::CordonList,
}

impl VisgroupPanel {
    pub fn new() -> Self {
        Self::default()
    }

    fn refresh(&mut self, state: &AppState) -> bool {
        let key = DocKey::of(&state.doc);
        if self.rows_key != Some(key) {
            self.rows = flatten_visgroups(&state.doc, &self.collapsed);
            self.rows_key = Some(key);
            if let Some(r) = &self.rename
                && state.doc.visgroup(r.id).is_none()
            {
                self.rename = None;
            }
        }
        let fgd = state.assets.fgd();
        let fgd_key = (state.assets.generation(), fgd.is_some());
        if self.auto_fgd != Some(fgd_key) {
            self.auto = AutoVisData::new(fgd.map(|f| f.auto_visgroups.as_slice()).unwrap_or(&[]));
            self.auto_fgd = Some(fgd_key);
            self.auto_doc = None;
        }
        if self.auto_doc != Some(key) {
            let recent = self.last_recount.is_some_and(|t| t.elapsed() < AUTO_RECOUNT_INTERVAL);
            if recent && self.auto_doc.is_some() && !self.force_recount {
                return true;
            }
            self.auto.recount(&state.doc);
            self.auto_doc = Some(key);
            self.last_recount = Some(Instant::now());
            self.force_recount = false;
        }
        false
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        if self.refresh(state) {
            ui.ctx().request_repaint_after(AUTO_RECOUNT_INTERVAL);
        }
        let mut actions: Vec<VisAction> = Vec::new();

        self.toolbar(ui, state, &mut actions);
        ui.separator();

        egui::ScrollArea::vertical().id_salt("hp_visgroups_scroll").auto_shrink([false, false]).show(ui, |ui| {
            hp_app::widgets::section(ui, t!("visgroups.user"));
            if self.rows.is_empty() {
                ui.label(RichText::new(t!("visgroups.empty")).weak().italics());
            } else {
                for i in 0..self.rows.len() {
                    if let Some(row) = self.rows.get(i).cloned() {
                        self.row_ui(ui, state, &row, &mut actions);
                    }
                }
            }
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
            if resp.dnd_hover_payload::<VisgroupDrag>().is_some() {
                ui.painter().rect_stroke(
                    rect.shrink(1.0),
                    3.0,
                    Stroke::new(1.0, theme::accent()),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    t!("visgroups.drop_top_level"),
                    FontId::proportional(12.0),
                    ui.visuals().weak_text_color(),
                );
            }
            if let Some(drag) = resp.dnd_release_payload::<VisgroupDrag>() {
                actions.push(VisAction::Move(drag.0, None));
            }

            ui.add_space(6.0);
            self.cordons.ui(ui, state);
            ui.add_space(6.0);
            self.auto_ui(ui, state);
        });

        self.drag_preview(ui, state);
        self.apply(state, actions);
    }

    fn drag_preview(&self, ui: &Ui, state: &AppState) {
        let Some(drag) = egui::DragAndDrop::payload::<VisgroupDrag>(ui.ctx()) else { return };
        let Some(pos) = ui.ctx().pointer_latest_pos() else { return };
        let Some(g) = state.doc.visgroup(drag.0) else { return };
        let painter =
            ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("hp_visgroup_drag")));
        let galley =
            painter.layout_no_wrap(g.name.clone(), FontId::proportional(13.0), ui.visuals().strong_text_color());
        let rect = egui::Rect::from_min_size(pos + vec2(14.0, 6.0), galley.size() + vec2(12.0, 6.0));
        painter.rect_filled(rect, 4.0, ui.visuals().window_fill);
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0, theme::accent()), egui::StrokeKind::Inside);
        painter.galley(rect.min + vec2(6.0, 3.0), galley, ui.visuals().strong_text_color());
    }

    fn toolbar(&mut self, ui: &mut Ui, state: &mut AppState, actions: &mut Vec<VisAction>) {
        ui.horizontal(|ui| {
            let has_sel = !state.doc.selection().objects.is_empty();
            let tip = if has_sel { t!("visgroups.new_from_selection") } else { t!("visgroups.new_empty") };
            if ui
                .add(egui::Button::new(RichText::new("➕").size(15.0)).min_size(vec2(28.0, 24.0)))
                .on_hover_text(tip)
                .clicked()
            {
                let name = unique_visgroup_name(&state.doc, t!("visgroups.new_name"));
                let members = state.doc.selected_objects();
                match state.doc.create_visgroup(&name, None, &members) {
                    Ok(id) => {
                        actions.push(VisAction::Focus(id));
                        actions.push(VisAction::StartRename(id));
                    }
                    Err(e) => state.notify(Severity::Warning, tf!("visgroups.error", e)),
                }
            }
            if ui
                .add(egui::Button::new(RichText::new("👁").size(15.0)).min_size(vec2(28.0, 24.0)))
                .on_hover_text(t!("visgroups.show_all"))
                .clicked()
            {
                state.doc.show_all_visgroups();
            }
            if ui
                .add(egui::Button::new(RichText::new("⏷").size(15.0)).min_size(vec2(28.0, 24.0)))
                .on_hover_text(t!("visgroups.expand_all"))
                .clicked()
            {
                self.collapsed.clear();
                self.rows_key = None;
            }
            if ui
                .add(egui::Button::new(RichText::new("⏶").size(15.0)).min_size(vec2(28.0, 24.0)))
                .on_hover_text(t!("visgroups.collapse_all"))
                .clicked()
            {
                let mut stack: Vec<&VisGroup> = state.doc.scene().visgroups().iter().collect();
                while let Some(g) = stack.pop() {
                    if !g.children.is_empty() {
                        self.collapsed.insert(g.id);
                    }
                    stack.extend(g.children.iter());
                }
                self.rows_key = None;
            }
            if ui
                .add_enabled(has_sel, egui::Button::new(RichText::new("📂").size(15.0)).min_size(vec2(28.0, 24.0)))
                .on_hover_text(Action::MoveToVisgroup.label())
                .clicked()
            {
                state.request(Action::MoveToVisgroup);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let n = visgroup_total(&state.doc);
                let text = RichText::new(tf!(plural(n, "visgroups.count_one", "visgroups.count"), n)).weak();
                ui.add(egui::Label::new(text).truncate());
            });
        });
    }

    fn row_ui(&mut self, ui: &mut Ui, state: &mut AppState, row: &VisRow, actions: &mut Vec<VisAction>) {
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click_and_drag());
        let id = row.id;
        let painter = ui.painter_at(rect);
        let (sel_bg, hover_bg, text, strong, weak) = {
            let v = ui.visuals();
            (
                v.selection.bg_fill,
                v.widgets.hovered.weak_bg_fill,
                v.text_color(),
                v.strong_text_color(),
                v.weak_text_color(),
            )
        };
        let focused = self.focused == Some(id);
        if focused {
            painter.rect_filled(rect, 3.0, sel_bg);
        } else if resp.hovered() {
            painter.rect_filled(rect, 3.0, hover_bg.gamma_multiply(0.5));
        }

        if resp.drag_started() {
            resp.dnd_set_drag_payload(VisgroupDrag(id));
        }
        if let Some(drag) = resp.dnd_hover_payload::<VisgroupDrag>()
            && drag.0 != id
        {
            painter.rect_stroke(rect.shrink(1.0), 3.0, Stroke::new(1.5, theme::accent()), egui::StrokeKind::Inside);
        }
        if let Some(drag) = resp.dnd_release_payload::<VisgroupDrag>()
            && drag.0 != id
        {
            actions.push(VisAction::Move(drag.0, Some(id)));
        }

        let mut x = rect.left() + 2.0 + row.depth as f32 * INDENT;
        let cy = rect.center().y;

        let arrow_rect = egui::Rect::from_center_size(egui::pos2(x + 7.0, cy), vec2(14.0, ROW_HEIGHT));
        if row.has_children {
            let arrow_resp = ui.interact(arrow_rect, ui.scope_id().with(("vis_arrow", id)), Sense::click());
            let color = if arrow_resp.hovered() { strong } else { text };
            painter.text(
                arrow_rect.center(),
                Align2::CENTER_CENTER,
                if row.expanded { "⏷" } else { "⏵" },
                FontId::proportional(12.0),
                color,
            );
            if arrow_resp.clicked() {
                actions.push(VisAction::ToggleExpanded(id));
            }
        }
        x += 16.0;

        let check_rect = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 1.0), vec2(20.0, ROW_HEIGHT - 2.0));
        let mut checked = row.state != VisgroupState::Hidden;
        let check = ui
            .put(
                check_rect,
                egui::Checkbox::without_text(&mut checked).indeterminate(row.state == VisgroupState::Partial),
            )
            .on_hover_text(t!("visgroups.toggle_tip"));
        if check.changed() || check.clicked() {
            let visible = row.state != VisgroupState::Shown;
            actions.push(VisAction::SetVisible(id, visible));
        }
        x += 22.0;

        let swatch_rect = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 3.0), vec2(18.0, ROW_HEIGHT - 6.0));
        let mut color = row.color;
        let mut swatch_ui = ui.new_child(UiBuilder::new().max_rect(swatch_rect));
        swatch_ui.spacing_mut().interact_size = vec2(18.0, ROW_HEIGHT - 6.0);
        let swatch = egui::color_picker::color_edit_button_srgb(&mut swatch_ui, &mut color)
            .on_hover_text(t!("visgroups.color_tip"));
        if swatch.changed() && color != row.color {
            actions.push(VisAction::SetColor(id, color));
        }
        x += 24.0;

        let count_text = if row.has_children && row.total != row.count {
            format!("{} / {}", row.count, row.total)
        } else {
            row.count.to_string()
        };
        let count_galley = painter.layout_no_wrap(count_text, FontId::proportional(11.5), weak);
        let count_pos = egui::pos2(rect.right() - 6.0 - count_galley.size().x, cy - count_galley.size().y * 0.5);
        let name_right = count_pos.x - 6.0;

        let name_rect =
            egui::Rect::from_min_max(egui::pos2(x, rect.top() + 1.0), egui::pos2(name_right, rect.bottom() - 1.0));
        let renaming = self.rename.as_ref().is_some_and(|r| r.id == id);
        if renaming {
            if let Some(r) = &mut self.rename {
                let edit = ui.put(name_rect, egui::TextEdit::singleline(&mut r.text).desired_width(name_rect.width()));
                if !r.focus_requested {
                    focus_select_all(&edit, &r.text);
                    r.focus_requested = true;
                }
                if edit.lost_focus() {
                    let (escape, _enter) = ui.input_mut(|i| {
                        (
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                        )
                    });
                    if escape {
                        actions.push(VisAction::CancelRename);
                    } else {
                        actions.push(VisAction::CommitRename(id, r.text.clone()));
                    }
                }
            }
        } else {
            let text_color = match row.state {
                VisgroupState::Hidden => weak,
                _ => text,
            };
            let galley = painter.layout_no_wrap(row.name.clone(), FontId::proportional(13.0), text_color);
            ui.painter().with_clip_rect(hp_app::widgets::clip_x(name_rect, ui.clip_rect())).galley(
                egui::pos2(name_rect.left(), cy - galley.size().y * 0.5),
                galley,
                text_color,
            );
        }
        painter.galley(count_pos, count_galley, weak);

        let resp = resp.on_hover_text(tf!("visgroups.row_tip", row.count, row.total));
        if resp.clicked() {
            actions.push(VisAction::Focus(id));
        }
        if resp.double_clicked() && !renaming {
            actions.push(VisAction::StartRename(id));
        }
        resp.context_menu(|ui| {
            let has_sel = !state.doc.selection().objects.is_empty();
            if ui.button(t!("visgroups.select_members")).clicked() {
                actions.push(VisAction::SelectMembers(id));
                ui.close();
            }
            if ui.add_enabled(has_sel, egui::Button::new(t!("visgroups.add_selection"))).clicked() {
                actions.push(VisAction::AddSelection(id));
                ui.close();
            }
            if ui.add_enabled(has_sel, egui::Button::new(t!("visgroups.remove_selection"))).clicked() {
                actions.push(VisAction::RemoveSelection(id));
                ui.close();
            }
            if ui
                .add_enabled(has_sel, egui::Button::new(t!("visgroups.move_selection_here")))
                .on_hover_text(t!("visgroups.move_selection_here_tip"))
                .clicked()
            {
                actions.push(VisAction::MoveSelectionHere(id));
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(state.doc.can_reorder_visgroup(id, -1), egui::Button::new(t!("visgroups.move_up")))
                .clicked()
            {
                actions.push(VisAction::Reorder(id, -1));
                ui.close();
            }
            if ui
                .add_enabled(state.doc.can_reorder_visgroup(id, 1), egui::Button::new(t!("visgroups.move_down")))
                .clicked()
            {
                actions.push(VisAction::Reorder(id, 1));
                ui.close();
            }
            ui.separator();
            if ui.button(t!("visgroups.new_child")).clicked() {
                actions.push(VisAction::NewChild(id));
                ui.close();
            }
            if ui.button(t!("visgroups.rename")).clicked() {
                actions.push(VisAction::StartRename(id));
                ui.close();
            }
            if ui.button(RichText::new(t!("visgroups.delete")).color(theme::error())).clicked() {
                actions.push(VisAction::Delete(id));
                ui.close();
            }
        });
    }

    fn auto_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let mut changed = false;
        let header_id = ui.make_persistent_id("hp_autovis_header");
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), header_id, true)
            .show_header(ui, |ui| {
                ui.label(RichText::new(t!("autovis.title")).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("👁").on_hover_text(t!("autovis.show_all")).clicked() {
                        autovis::show_all_auto(&mut state.doc);
                        changed = true;
                    }
                });
            })
            .body(|ui| {
                for (parent, children) in AutoKind::TREE {
                    let count = self.auto.builtin_count(parent);
                    if parent == AutoKind::Hidden {
                        changed |= hidden_row(ui, state, count);
                        continue;
                    }
                    let auto = &self.auto;
                    let listed = || children.iter().copied().filter(|k| auto.builtin_count(*k).total > 0);
                    let mut toggle: Option<(AutoKind, bool)> = None;
                    if listed().next().is_none() {
                        toggle = ui
                            .horizontal(|ui| {
                                ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
                                auto_checkbox(ui, t!(parent.label_key()), t!(parent.tooltip_key()), count)
                            })
                            .inner
                            .map(|v| (parent, v));
                    } else {
                        let id = ui.make_persistent_id(("hp_autovis_kind", parent.label_key()));
                        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                            .show_header(ui, |ui| {
                                if let Some(v) =
                                    auto_checkbox(ui, t!(parent.label_key()), t!(parent.tooltip_key()), count)
                                {
                                    toggle = Some((parent, v));
                                }
                            })
                            .body(|ui| {
                                for k in listed() {
                                    let c = auto.builtin_count(k);
                                    if let Some(v) = auto_row(ui, t!(k.label_key()), t!(k.tooltip_key()), c) {
                                        toggle = Some((k, v));
                                    }
                                }
                            });
                    }
                    if let Some((kind, visible)) = toggle {
                        autovis::set_builtin_visible(&mut state.doc, kind, visible);
                        changed = true;
                    }
                }
                if self.auto.fgd.is_empty() {
                    if state.assets.fgd().is_none() {
                        ui.label(RichText::new(t!("autovis.fgd_not_loaded")).weak().italics());
                    }
                    return;
                }
                ui.add_space(4.0);
                ui.label(RichText::new(t!("autovis.fgd_groups")).weak());
                for (gi, group) in self.auto.fgd.iter().enumerate() {
                    let id = ui.make_persistent_id(("hp_autovis_fgd", gi));
                    let total = group.count();
                    let mut toggled: Option<bool> = None;
                    let mut sub_toggle: Option<(usize, bool)> = None;
                    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                        .show_header(ui, |ui| {
                            let n = total.total;
                            let tip = tf!(plural(n, "autovis.fgd_group_tip_one", "autovis.fgd_group_tip"), n);
                            toggled = auto_checkbox(ui, &group.name, &tip, total);
                        })
                        .body(|ui| {
                            for (si, sub) in group.subs.iter().enumerate() {
                                let tip = sub.classes.join(", ");
                                if let Some(v) = auto_row(ui, &sub.name, &tip, sub.count) {
                                    sub_toggle = Some((si, v));
                                }
                            }
                        });
                    if let Some(v) = toggled {
                        autovis::set_classes_visible(&mut state.doc, &group.all_classes(), v);
                        changed = true;
                    }
                    if let Some((si, v)) = sub_toggle
                        && let Some(sub) = group.subs.get(si)
                    {
                        autovis::set_classes_visible(&mut state.doc, &sub.classes, v);
                        changed = true;
                    }
                }
            });
        self.force_recount |= changed;
    }

    fn apply(&mut self, state: &mut AppState, actions: Vec<VisAction>) {
        for action in actions {
            let result = match action {
                VisAction::SetVisible(id, v) => state.doc.set_visgroup_visible(id, v),
                VisAction::SetColor(id, c) => {
                    state.doc.begin_merge("Change visgroup color", format!("visgroup_color_{id}"));
                    let r = state.doc.set_visgroup_color(id, c);
                    state.doc.commit();
                    r
                }
                VisAction::StartRename(id) => {
                    if let Some(g) = state.doc.visgroup(id) {
                        self.rename = Some(Rename { id, text: g.name.clone(), focus_requested: false });
                    }
                    Ok(())
                }
                VisAction::CommitRename(id, name) => {
                    self.rename = None;
                    let name = name.trim();
                    if name.is_empty() { Ok(()) } else { state.doc.rename_visgroup(id, name) }
                }
                VisAction::CancelRename => {
                    self.rename = None;
                    Ok(())
                }
                VisAction::SelectMembers(id) => {
                    let all = subtree_members(&state.doc, id);
                    let objs: Vec<ObjRef> = all.iter().copied().filter(|o| state.doc.is_visible(*o)).collect();
                    if objs.is_empty() && !all.is_empty() {
                        state.notify(Severity::Info, t!("visgroups.members_hidden"));
                    } else {
                        state.doc.set_selection(&objs);
                    }
                    Ok(())
                }
                VisAction::AddSelection(id) => {
                    let objs = state.doc.selected_objects();
                    state.doc.add_to_visgroup(id, &objs).map(|_| ())
                }
                VisAction::RemoveSelection(id) => {
                    let objs = state.doc.selected_objects();
                    state.doc.remove_from_visgroup(id, &objs).map(|_| ())
                }
                VisAction::Delete(id) => {
                    if self.focused == Some(id) {
                        self.focused = None;
                    }
                    state.doc.delete_visgroup(id)
                }
                VisAction::NewChild(parent) => {
                    let name = unique_visgroup_name(&state.doc, t!("visgroups.new_name"));
                    match state.doc.create_visgroup(&name, Some(parent), &[]) {
                        Ok(id) => {
                            self.collapsed.remove(&parent);
                            self.rows_key = None;
                            self.focused = Some(id);
                            self.rename = Some(Rename { id, text: name, focus_requested: false });
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                VisAction::Move(id, parent) => {
                    if let Some(p) = parent {
                        self.collapsed.remove(&p);
                        self.rows_key = None;
                    }
                    state.doc.move_visgroup(id, parent)
                }
                VisAction::Reorder(id, delta) => state.doc.reorder_visgroup(id, delta).map(|_| ()),
                VisAction::MoveSelectionHere(id) => {
                    let objs = state.doc.selected_objects();
                    state.doc.move_to_visgroup(&objs, Some(id), "", true, false).map(|_| ())
                }
                VisAction::ToggleExpanded(id) => {
                    if !self.collapsed.remove(&id) {
                        self.collapsed.insert(id);
                    }
                    self.rows_key = None;
                    Ok(())
                }
                VisAction::Focus(id) => {
                    self.focused = Some(id);
                    Ok(())
                }
            };
            if let Err(e) = result {
                state.notify(Severity::Warning, tf!("visgroups.error", e));
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct MoveToVisgroupDialog {
    pub(crate) target: Option<Id>,
    pub(crate) new_name: String,
    rows: Vec<VisRow>,
    rows_key: Option<DocKey>,
    error: Option<String>,
    was_open: bool,
    focus_name: bool,
}

impl MoveToVisgroupDialog {
    pub(crate) fn submit(&mut self, state: &mut AppState) -> bool {
        let objs = state.doc.selected_objects();
        let remove = state.settings.visibility.move_remove_others;
        let hide = state.settings.visibility.move_hide;
        let target = self.target.filter(|id| state.doc.visgroup(*id).is_some());
        match state.doc.move_to_visgroup(&objs, target, &self.new_name, remove, hide) {
            Ok(id) => {
                let name = state.doc.visgroup(id).map(|g| g.name.clone()).unwrap_or_default();
                state.log(Severity::Info, tf!("visgroups.moved", objs.len(), name));
                self.target = Some(id);
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(tf!("visgroups.error", e));
                false
            }
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) -> bool {
        let n = state.doc.selected_objects().len();
        ui.label(tf!(plural(n, "visgroups.move_prompt_one", "visgroups.move_prompt"), n));
        egui::ScrollArea::vertical().id_salt("hp_move_visgroup_list").max_height(240.0).show(ui, |ui| {
            if ui.selectable_label(self.target.is_none(), t!("visgroups.move_new")).clicked() {
                self.target = None;
                self.focus_name = true;
            }
            for row in &self.rows {
                ui.horizontal(|ui| {
                    ui.add_space(row.depth as f32 * INDENT);
                    let [r, g, b] = row.color;
                    let (swatch, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().rect_filled(swatch, 2.0, egui::Color32::from_rgb(r, g, b));
                    if ui.selectable_label(self.target == Some(row.id), &row.name).clicked() {
                        self.target = Some(row.id);
                    }
                });
            }
        });
        let mut go = false;
        if self.target.is_none() {
            ui.horizontal(|ui| {
                ui.label(t!("visgroups.move_name"));
                let edit = ui.add(egui::TextEdit::singleline(&mut self.new_name).desired_width(200.0));
                if std::mem::take(&mut self.focus_name) {
                    focus_select_all(&edit, &self.new_name);
                }
                go |= edit.lost_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            });
        }
        let s = &mut state.settings.visibility;
        let mut changed = ui.checkbox(&mut s.move_remove_others, t!("visgroups.move_remove_others")).changed();
        changed |= ui
            .checkbox(&mut s.move_hide, t!("visgroups.move_hide"))
            .on_hover_text(t!("visgroups.move_hide_tip"))
            .changed();
        if changed {
            state.settings_dirty = true;
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(theme::error()));
        }
        ui.separator();
        let mut close = false;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let key = hp_app::actions::key_label;
                if ui.button(t!("common.cancel")).on_hover_text(key(egui::Key::Escape)).clicked() {
                    close = true;
                }
                let ready = n > 0 && (self.target.is_some() || !self.new_name.trim().is_empty());
                if ui
                    .add_enabled(ready, egui::Button::new(t!("visgroups.move_go")))
                    .on_hover_text(key(egui::Key::Enter))
                    .clicked()
                {
                    go = true;
                }
            });
        });
        if go && self.submit(state) {
            close = true;
        }
        close
    }
}

impl MapDialog for MoveToVisgroupDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.visibility.move_to_visgroup_open {
            self.was_open = false;
            return;
        }
        if !self.was_open {
            self.was_open = true;
            self.error = None;
            self.new_name = unique_visgroup_name(&state.doc, t!("visgroups.new_name"));
            if self.target.is_some_and(|id| state.doc.visgroup(id).is_none()) {
                self.target = None;
            }
            self.focus_name = self.target.is_none();
        }
        let key = DocKey::of(&state.doc);
        if self.rows_key != Some(key) {
            self.rows = flatten_visgroups(&state.doc, &HashSet::new());
            self.rows_key = Some(key);
        }
        let mut open = true;
        let mut close = false;
        let response = hp_app::chrome::Window::new(icon_title("📂", t!("visgroups.move_title")))
            .id(egui::Id::unique("hp_move_to_visgroup"))
            .default_pos(crate::util::centered_pos(ctx, vec2(340.0, 360.0)))
            .open(&mut open)
            .default_width(340.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| close = self.window_ui(ui, state));
        if let Some(r) = &response {
            if escape_pressed(ctx, &r.response) {
                open = false;
            } else if !close && enter_pressed(ctx, &r.response) && self.submit(state) {
                close = true;
            }
        }
        if !open || close {
            state.runtime.visibility.move_to_visgroup_open = false;
        }
    }
}

pub(crate) fn visgroup_total(doc: &Document) -> usize {
    let mut n = 0;
    let mut stack: Vec<&VisGroup> = doc.scene().visgroups().iter().collect();
    while let Some(g) = stack.pop() {
        n += 1;
        stack.extend(g.children.iter());
    }
    n
}

fn auto_checkbox(ui: &mut Ui, label: &str, tooltip: &str, count: GroupCount) -> Option<bool> {
    let state = count.state();
    let mut checked = state != Some(false);
    let count_text = RichText::new(count.total.to_string()).weak().small();
    let count_w = egui::WidgetText::from(count_text.clone())
        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Small)
        .size()
        .x;
    let max_w = (ui.available_width() - count_w - ui.spacing().item_spacing.x).max(ui.spacing().icon_width);
    let size = egui::vec2(max_w, ui.spacing().interact_size.y);
    let resp = ui
        .allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            ui.add_enabled(count.total > 0, egui::Checkbox::new(&mut checked, label).indeterminate(state.is_none()))
        })
        .inner
        .on_hover_text(tooltip);
    ui.label(count_text);
    (resp.clicked() || resp.changed()).then_some(state != Some(true))
}

fn auto_row(ui: &mut Ui, label: &str, tooltip: &str, count: GroupCount) -> Option<bool> {
    ui.horizontal(|ui| auto_checkbox(ui, label, tooltip, count)).inner
}

fn hidden_row(ui: &mut Ui, state: &mut AppState, count: GroupCount) -> bool {
    ui.horizontal_wrapped(|ui| {
        ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
        let label = t!(AutoKind::Hidden.label_key());
        let color = if count.total > 0 { theme::warning() } else { ui.visuals().weak_text_color() };
        ui.label(RichText::new(label).color(color)).on_hover_text(t!(AutoKind::Hidden.tooltip_key()));
        ui.label(RichText::new(count.total.to_string()).weak().small());
        if ui
            .add_enabled(count.total > 0, egui::Button::new(t!("autovis.unhide_all")).small())
            .on_hover_text(t!("action.unhide_all"))
            .clicked()
        {
            return autovis::set_builtin_visible(&mut state.doc, AutoKind::Hidden, true) > 0;
        }
        if ui
            .add_enabled(count.total > 0, egui::Button::new(t!("autovis.to_visgroup")).small())
            .on_hover_text(Action::HiddenToVisgroup.label())
            .clicked()
        {
            state.request(Action::HiddenToVisgroup);
        }
        false
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_tree() -> AppState {
        let mut state = hp_app::testing::test_state();
        let a = state.doc.create_visgroup("Walls", None, &[ObjRef::Solid(2)]).unwrap_or(0);
        let b = state.doc.create_visgroup("Lights", Some(a), &[ObjRef::Entity(5)]).unwrap_or(0);
        let _c = state.doc.create_visgroup("Spawns", None, &[ObjRef::Entity(4), ObjRef::Solid(3)]).unwrap_or(0);
        assert!(a > 0 && b > 0);
        state
    }

    #[test]
    fn flatten_tree_and_collapse() {
        let state = doc_with_tree();
        let rows = flatten_visgroups(&state.doc, &HashSet::new());
        let names: Vec<(&str, usize)> = rows.iter().map(|r| (r.name.as_str(), r.depth)).collect();
        assert_eq!(names, vec![("Walls", 0), ("Lights", 1), ("Spawns", 0)]);
        assert_eq!(rows[0].count, 1);
        assert_eq!(rows[0].total, 2);
        assert!(rows[0].has_children && rows[0].expanded);
        assert_eq!(rows[2].count, 2);

        let mut collapsed = HashSet::new();
        collapsed.insert(rows[0].id);
        let rows2 = flatten_visgroups(&state.doc, &collapsed);
        assert_eq!(rows2.len(), 2);
        assert!(!rows2[0].expanded);
        assert_eq!(rows2[0].total, 2, "totals include collapsed children");
    }

    #[test]
    fn states_match_document() {
        let mut state = doc_with_tree();
        let rows = flatten_visgroups(&state.doc, &HashSet::new());
        let (walls, lights) = (rows[0].id, rows[1].id);
        state.doc.set_visgroup_visible(lights, false).ok();
        let mut collapsed = HashSet::new();
        for coll in [false, true] {
            if coll {
                collapsed.insert(walls);
            }
            let rows = flatten_visgroups(&state.doc, &collapsed);
            for r in &rows {
                assert_eq!(r.state, state.doc.visgroup_state(r.id), "{}", r.name);
            }
            assert_eq!(rows[0].state, VisgroupState::Partial);
        }
        state.doc.set_visgroup_visible(walls, false).ok();
        let rows = flatten_visgroups(&state.doc, &HashSet::new());
        assert_eq!(rows[0].state, VisgroupState::Hidden);
        assert_eq!(rows[1].state, VisgroupState::Hidden);
    }

    #[test]
    fn members_and_names() {
        let state = doc_with_tree();
        let rows = flatten_visgroups(&state.doc, &HashSet::new());
        let members = subtree_members(&state.doc, rows[0].id);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&ObjRef::Solid(2)) && members.contains(&ObjRef::Entity(5)));
        assert_eq!(unique_visgroup_name(&state.doc, "Walls"), "Walls 2");
        assert_eq!(unique_visgroup_name(&state.doc, "Other"), "Other");
        assert_eq!(visgroup_total(&state.doc), 3);
    }

    #[test]
    fn actions_apply() {
        let mut state = doc_with_tree();
        let mut panel = VisgroupPanel::new();
        panel.refresh(&state);
        let walls = panel.rows[0].id;
        let lights = panel.rows[1].id;
        panel.apply(&mut state, vec![VisAction::SelectMembers(walls)]);
        assert_eq!(state.doc.selection().objects.len(), 2);
        state.doc.set_visgroup_visible(lights, false).ok();
        panel.apply(&mut state, vec![VisAction::SelectMembers(walls)]);
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(2)]);
        state.doc.set_visgroup_visible(walls, false).ok();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        panel.apply(&mut state, vec![VisAction::SelectMembers(walls)]);
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(3)], "nothing visible: selection kept");
        state.doc.set_visgroup_visible(walls, true).ok();
        state.doc.set_visgroup_visible(lights, true).ok();
        panel.apply(&mut state, vec![VisAction::CommitRename(walls, "  Big walls ".into())]);
        assert_eq!(state.doc.visgroup(walls).map(|g| g.name.as_str()), Some("Big walls"));
        panel.apply(&mut state, vec![VisAction::Move(lights, None)]);
        assert_eq!(state.doc.visgroup_parent(lights).ok(), Some(None));
        panel.apply(&mut state, vec![VisAction::SetColor(walls, [1, 2, 3])]);
        assert_eq!(state.doc.visgroup(walls).map(|g| g.color), Some([1, 2, 3]));
        panel.apply(&mut state, vec![VisAction::NewChild(walls)]);
        assert!(panel.rename.is_some());
        panel.apply(&mut state, vec![VisAction::CancelRename, VisAction::Delete(walls)]);
        assert!(state.doc.visgroup(walls).is_none());
        panel.apply(&mut state, vec![VisAction::Delete(9999)]);
        assert!(!state.console.is_empty());
    }

    #[test]
    fn reorder_and_move_the_selection_here() {
        let mut state = doc_with_tree();
        let mut panel = VisgroupPanel::new();
        panel.refresh(&state);
        let (walls, spawns) = (panel.rows[0].id, panel.rows[2].id);
        panel.apply(&mut state, vec![VisAction::Reorder(spawns, -1)]);
        let names: Vec<&str> = state.doc.scene().visgroups().iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["Spawns", "Walls"]);
        assert_eq!(state.doc.undo_label(), Some("Reorder visgroup"));
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        panel.apply(&mut state, vec![VisAction::MoveSelectionHere(walls)]);
        assert!(state.doc.visgroup_members(walls).contains(&ObjRef::Solid(3)));
        assert!(!state.doc.visgroup_members(spawns).contains(&ObjRef::Solid(3)));
        assert_eq!(state.doc.undo_label(), Some("Move to visgroup"));
        state.doc.clear_selection();
        let before = state.console.len();
        panel.apply(&mut state, vec![VisAction::MoveSelectionHere(walls)]);
        assert!(state.console.len() > before);
    }

    #[test]
    fn the_panel_fits_a_narrow_dock() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = doc_with_tree();
        let mut panel = VisgroupPanel::new();
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            for selected in [false, true] {
                if selected {
                    state.doc.set_selection(&[ObjRef::Solid(2)]);
                } else {
                    state.doc.clear_selection();
                }
                let ctx = egui::Context::default();
                let mut width = 0.0f32;
                for _ in 0..4 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 900.0))),
                        ..Default::default()
                    };
                    let mut out = ctx.run_ui(input, |ui| {
                        let r = egui::Panel::right("hp_test_dock")
                            .resizable(true)
                            .default_size(220.0)
                            .min_size(220.0)
                            .show(ui, |ui| panel.ui(ui, &mut state));
                        width = r.response.rect.width();
                    });
                    out.textures_delta.clear();
                }
                hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
                assert!(width <= 220.5, "{lang:?} selected {selected}: {width}");
                hp_app::i18n::set_lang(lang);
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }

    #[test]
    fn move_to_visgroup_window() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = doc_with_tree();
        let mut dialog = MoveToVisgroupDialog::default();
        hp_app::testing::run_ctx(&mut state, |ctx, state| dialog.show(ctx, state));
        state.doc.set_selection(&[ObjRef::Solid(2), ObjRef::Entity(4)]);
        state.runtime.visibility.move_to_visgroup_open = true;
        for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
            hp_app::i18n::set_lang(lang);
            hp_app::testing::run_ctx(&mut state, |ctx, state| dialog.show(ctx, state));
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(state.runtime.visibility.move_to_visgroup_open);
        assert_eq!(dialog.target, None);
        assert_eq!(dialog.new_name, "New visgroup");
        state.settings.visibility.move_hide = true;
        assert!(dialog.submit(&mut state));
        let id = dialog.target.expect("new visgroup");
        assert_eq!(state.doc.visgroup_members(id).len(), 2);
        assert!(!state.doc.is_visible(ObjRef::Solid(2)), "hidden with its visgroup");
        assert!(state.doc.selection().is_empty());
        let walls = state.doc.scene().visgroups()[0].id;
        state.settings.visibility.move_hide = false;
        state.settings.visibility.move_remove_others = false;
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        dialog.target = Some(walls);
        assert!(dialog.submit(&mut state));
        assert!(state.doc.visgroup_members(walls).contains(&ObjRef::Solid(3)));
        assert_eq!(state.doc.scene().solid(3).map(|n| n.solid.editor.visgroup_ids.len()), Some(2));
        state.doc.clear_selection();
        assert!(!dialog.submit(&mut state));
        assert!(dialog.error.is_some());
    }

    #[test]
    fn rename_replaces_the_default_name() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        let mut panel = VisgroupPanel::new();
        let id = state.doc.create_visgroup("New visgroup", None, &[ObjRef::Solid(2)]).expect("visgroup");
        panel.apply(&mut state, vec![VisAction::StartRename(id)]);
        let ctx = egui::Context::default();
        let frame = |events: Vec<egui::Event>, state: &mut AppState, panel: &mut VisgroupPanel| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 800.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| panel.ui(ui, state));
            out.textures_delta.clear();
        };
        frame(Vec::new(), &mut state, &mut panel);
        frame(vec![egui::Event::Text("MyTriggers".into())], &mut state, &mut panel);
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![enter], &mut state, &mut panel);
        frame(Vec::new(), &mut state, &mut panel);
        assert_eq!(state.doc.visgroup(id).map(|g| g.name.as_str()), Some("MyTriggers"));
        assert!(panel.rename.is_none());
    }
}
