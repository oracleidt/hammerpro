use egui::{RichText, Ui};
use hp_app::actions::key_label;
use hp_app::runtime::HealthRuntime;
use hp_app::state::AssetStatus;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::{Document, ObjRef, Problem, Severity as ProblemSeverity};

use crate::util::{dialog_body, dialog_bottom_bar, escape_pressed, icon_title, plural};

fn objects_text(n: usize) -> String {
    tf!(plural(n, "problems.objects_one", "problems.objects"), n)
}

fn faces_text(n: usize) -> String {
    tf!(plural(n, "problems.faces_one", "problems.faces"), n)
}

pub(crate) fn summary_text(problems: usize, fixable: usize) -> String {
    format!(
        "{}, {}",
        tf!(plural(problems, "problems.count_one", "problems.count"), problems),
        tf!(plural(fixable, "problems.fixable_one", "problems.fixable"), fixable)
    )
}

pub(crate) fn severity_of(p: &Problem) -> Severity {
    match p.severity {
        ProblemSeverity::Error => Severity::Error,
        ProblemSeverity::Warning => Severity::Warning,
        ProblemSeverity::Info => Severity::Info,
    }
}

fn tool_in_message(message: &str) -> Option<&str> {
    let start = message.find("tools/")?;
    let rest = &message[start..];
    let end = rest.find(|c: char| c.is_whitespace() || c == ',').unwrap_or(rest.len());
    Some(&rest[..end])
}

fn listed_in_message(message: &str) -> Option<&str> {
    let (_, rest) = message.split_once("do not exist: ").or_else(|| message.split_once("does not exist: "))?;
    Some(rest.trim()).filter(|r| !r.is_empty())
}

fn count_after(message: &str, marker: &str) -> Option<usize> {
    let rest = &message[message.find(marker)? + marker.len()..];
    rest.split_whitespace().next()?.parse().ok()
}

fn first_entity(p: &Problem, doc: &Document) -> Option<(u32, String)> {
    p.objects.iter().find_map(|o| match o {
        ObjRef::Entity(id) => doc.scene().entity(*id).map(|n| (*id, n.entity.classname.clone())),
        ObjRef::Solid(_) => None,
    })
}

pub(crate) fn limit_name(id: &str) -> String {
    let key = format!("problems.limit.{id}");
    let text = t!(&key);
    if text == key { id.to_owned() } else { text.to_owned() }
}

pub(crate) fn describe(p: &Problem, doc: &Document) -> String {
    let n = p.objects.len();
    let faces = p.faces.len();
    let a = |i: usize| p.arg(i);
    match p.code {
        "invalid_solid" => match p.objects.first() {
            Some(o) => tf!("problems.invalid_solid", o.id()),
            None => p.message.clone(),
        },
        "renumbered_ids" => tf!(plural(n, "problems.renumbered_ids_one", "problems.renumbered_ids"), n),
        "duplicate_object_id" => tf!(plural(n, "problems.duplicate_object_id_one", "problems.duplicate_object_id"), n),
        "duplicate_side_id" => {
            tf!(plural(faces, "problems.duplicate_side_id_one", "problems.duplicate_side_id"), faces)
        }
        "unknown_class" => match first_entity(p, doc) {
            Some((_, class)) => {
                tf!("problems.unknown_class", class, tf!(plural(n, "problems.entities_one", "problems.entities"), n))
            }
            None => p.message.clone(),
        },
        "missing_output_target" => {
            let text = match first_entity(p, doc) {
                Some((id, class)) => tf!("problems.missing_output_target", id, class),
                None => t!("problems.missing_output_target_world").to_owned(),
            };
            if p.args.is_empty() { text } else { tf!("problems.with_list", text, p.args.join(", ")) }
        }
        "brush_entity_without_solids" => match first_entity(p, doc) {
            Some((id, class)) => tf!("problems.brush_entity_without_solids", id, class),
            None => p.message.clone(),
        },
        "disp_not_quad" => tf!(plural(faces, "problems.disp_not_quad_one", "problems.disp_not_quad"), faces),
        "no_player_spawn" => t!("problems.no_player_spawn").to_owned(),
        "tool_texture_on_world" => {
            let key = plural(n, "problems.tool_texture_on_world_one", "problems.tool_texture_on_world");
            tf!(key, n, tool_in_message(&p.message).unwrap_or("tools"))
        }
        "empty_map" => t!("problems.empty_map").to_owned(),
        "entity_out_of_bounds" => {
            tf!(plural(n, "problems.entity_out_of_bounds_one", "problems.entity_out_of_bounds"), n)
        }
        "entity_outside_map" => tf!(plural(n, "problems.entity_outside_map_one", "problems.entity_outside_map"), n),
        "missing_group" => tf!(plural(n, "problems.missing_group_one", "problems.missing_group"), n),
        "missing_visgroup" => tf!(plural(n, "problems.missing_visgroup_one", "problems.missing_visgroup"), n),
        "invalid_texture_axes" => {
            tf!(plural(faces, "problems.invalid_texture_axes_one", "problems.invalid_texture_axes"), faces)
        }
        "duplicate_plane" => tf!(plural(faces, "problems.duplicate_plane_one", "problems.duplicate_plane"), faces),
        "missing_target" => match (first_entity(p, doc), listed_in_message(&p.message)) {
            (Some((id, class)), Some(list)) => tf!("problems.missing_target", id, class, list),
            _ => p.message.clone(),
        },
        "overlay_missing_faces" => match first_entity(p, doc) {
            Some((id, class)) if p.severity == ProblemSeverity::Error => tf!("problems.overlay_no_face", id, class),
            Some((id, class)) => match count_after(&p.message, " lists ") {
                Some(k) => {
                    tf!(plural(k, "problems.overlay_missing_faces_one", "problems.overlay_missing_faces"), id, class, k)
                }
                None => p.message.clone(),
            },
            None => p.message.clone(),
        },
        "missing_material" => match (count_after(&p.message, " faces use "), listed_in_message(&p.message)) {
            (Some(k), Some(list)) => {
                tf!(plural(k, "problems.missing_material_one", "problems.missing_material"), k, list)
            }
            _ => p.message.clone(),
        },
        "missing_model" => match (first_entity(p, doc), listed_in_message(&p.message)) {
            (Some((id, class)), Some(model)) => tf!("problems.missing_model", id, class, model),
            _ => p.message.clone(),
        },
        "mixed_face_contents" => tf!(plural(n, "problems.mixed_face_contents_one", "problems.mixed_face_contents"), n),
        "disp_on_brush_entity" => tf!("problems.disp_on_brush_entity", a(0), a(1)),
        "too_many_sides" => tf!(plural(n, "problems.too_many_sides_one", "problems.too_many_sides"), n),
        "micro_brush" => tf!(plural(n, "problems.micro_brush_one", "problems.micro_brush"), n),
        "origin_brush_in_world" => {
            tf!(plural(n, "problems.origin_brush_in_world_one", "problems.origin_brush_in_world"), n)
        }
        "duplicate_singleton" => tf!("problems.duplicate_singleton", n, a(0)),
        "unknown_input" => tf!("problems.unknown_input", a(0), a(1), a(2)),
        "key_too_long" => tf!("problems.key_too_long", a(0), a(1), a(2)),
        "entity_in_solid" => tf!(plural(n, "problems.entity_in_solid_one", "problems.entity_in_solid"), n),
        "overlay_too_many_faces" => tf!("problems.overlay_too_many_faces", a(0), a(1), a(2)),
        "too_many_named_lights" => tf!("problems.too_many_named_lights", n, a(0)),
        "areaportal_brushes" => tf!("problems.areaportal_brushes", a(0), a(1), a(2)),
        "areaportal_not_door" => tf!("problems.areaportal_not_door", a(0), a(1), a(2)),
        "areaportal_water" => tf!("problems.areaportal_water", a(0), a(1)),
        "areaportal_occluder" => tf!("problems.areaportal_occluder", a(0), a(1)),
        "areaportal_door_state" => {
            let key = if a(2) == "1" {
                "problems.areaportal_open_door_closed"
            } else {
                "problems.areaportal_closed_door_open"
            };
            tf!(key, a(0), a(1))
        }
        "sky_without_sun" => t!("problems.sky_without_sun").to_owned(),
        "sky_camera_multiple" => tf!("problems.sky_camera_multiple", a(0), a(1)),
        "sky_camera_bad_scale" => tf!("problems.sky_camera_bad_scale", a(0), a(1)),
        "skybox_switch_bad_target" => tf!("problems.skybox_switch_bad_target", a(0), a(1)),
        "skybox_switch_per_player" => {
            tf!(plural(n, "problems.skybox_switch_per_player_one", "problems.skybox_switch_per_player"), n)
        }
        "missing_sound" => tf!("problems.missing_sound", a(0), a(1), a(2)),
        "missing_particle" => tf!("problems.missing_particle", a(0), a(1), a(2)),
        "bad_content_path" => tf!("problems.bad_content_path", a(0), a(1), a(2)),
        "bad_material_path" => {
            tf!(plural(faces, "problems.bad_material_path_one", "problems.bad_material_path"), faces, a(0))
        }
        "leak" => tf!("problems.leak", a(0), a(1), a(2)),
        "leak_check_incomplete" => t!("problems.leak_check_incomplete").to_owned(),
        "detail_candidate" => tf!(plural(n, "problems.detail_candidate_one", "problems.detail_candidate"), n),
        "limit_exceeded" => tf!("problems.limit_exceeded", limit_name(a(0)), a(1), a(2), a(3)),
        "limit_close" => tf!("problems.limit_close", limit_name(a(0)), a(1), a(2), a(3)),
        "edict_budget" => tf!("problems.edict_budget", a(0), a(1), a(2)),
        "duplicate_brush" => tf!(plural(n, "problems.duplicate_brush_one", "problems.duplicate_brush"), n),
        "contained_brush" => tf!(plural(n, "problems.contained_brush_one", "problems.contained_brush"), n),
        "duplicate_entity" => tf!(plural(n, "problems.duplicate_entity_one", "problems.duplicate_entity"), n),
        "floating_object" if a(2).is_empty() => tf!("problems.floating_object_nothing", a(0), a(1)),
        "floating_object" => tf!("problems.floating_object", a(0), a(1), a(2)),
        "sunk_object" if a(2).is_empty() => tf!("problems.sunk_object_nothing", a(0), a(1)),
        "sunk_object" => tf!("problems.sunk_object", a(0), a(1), a(2)),
        "overlapping_objects" => tf!("problems.overlapping_objects", a(0), a(2), a(1)),
        _ => p.message.clone(),
    }
}

pub(crate) fn fix_hint(code: &str) -> &'static str {
    match code {
        "invalid_solid" => t!("problems.fix.invalid_solid"),
        "duplicate_object_id" | "duplicate_side_id" => t!("problems.fix.renumber"),
        "missing_output_target" => t!("problems.fix.missing_output_target"),
        "brush_entity_without_solids" => t!("problems.fix.brush_entity_without_solids"),
        "disp_not_quad" => t!("problems.fix.disp_not_quad"),
        "missing_group" | "missing_visgroup" => t!("problems.fix.missing_refs"),
        "invalid_texture_axes" => t!("problems.fix.invalid_texture_axes"),
        "duplicate_plane" => t!("problems.fix.duplicate_plane"),
        "overlay_missing_faces" => t!("problems.fix.overlay_missing_faces"),
        "disp_on_brush_entity" => t!("problems.fix.disp_on_brush_entity"),
        "micro_brush" => t!("problems.fix.micro_brush"),
        "origin_brush_in_world" => t!("problems.fix.origin_brush_in_world"),
        "detail_candidate" => t!("problems.fix.detail_candidate"),
        "areaportal_door_state" => t!("problems.fix.areaportal_door_state"),
        "duplicate_brush" | "contained_brush" | "duplicate_entity" => t!("problems.fix.redundant"),
        "floating_object" => t!("problems.fix.floating_object"),
        "sunk_object" => t!("problems.fix.sunk_object"),
        "overlapping_objects" => t!("problems.fix.overlapping_objects"),
        _ => t!("problems.fix.generic"),
    }
}

fn kind_name(code: &str) -> String {
    let key = format!("problems.kind.{code}");
    let text = t!(&key);
    if text == key { code.replace('_', " ") } else { text.to_owned() }
}

pub(crate) fn go_to_problem(state: &mut AppState, p: &Problem) {
    if p.code == "leak" {
        state.runtime.health.show_leak = true;
    }
    let objs = HealthRuntime::existing_objects(&state.doc, p);
    if objs.is_empty() && p.faces.is_empty() {
        return;
    }
    state.doc.set_selection(&objs);
    if !p.faces.is_empty() {
        state.doc.select_faces(&p.faces, hp_editor::SelectOp::Replace);
    }
    crate::util::request_center(state);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RowsKey {
    revision: u64,
    doc: u64,
    errors: bool,
    warnings: bool,
    infos: bool,
    ignored: bool,
    visible_only: bool,
}

pub(crate) struct ProblemsDialog {
    was_open: bool,
    pub(crate) show_errors: bool,
    pub(crate) show_warnings: bool,
    pub(crate) show_info: bool,
    pub(crate) show_ignored: bool,
    pub(crate) rows: Vec<usize>,
    rows_key: Option<RowsKey>,
    fixable: Option<((u64, usize, usize), usize)>,
    selected: Option<String>,
    detail: detail::DetailWindow,
}

impl Default for ProblemsDialog {
    fn default() -> Self {
        Self {
            was_open: false,
            show_errors: true,
            show_warnings: true,
            show_info: true,
            show_ignored: false,
            rows: Vec::new(),
            rows_key: None,
            fixable: None,
            selected: None,
            detail: detail::DetailWindow::default(),
        }
    }
}

impl ProblemsDialog {
    pub(crate) fn update_rows(&mut self, state: &AppState) {
        let visible_only = state.settings.health.visible_only;
        let key = RowsKey {
            revision: state.runtime.health.revision,
            doc: if visible_only { state.doc.revision() } else { 0 },
            errors: self.show_errors,
            warnings: self.show_warnings,
            infos: self.show_info,
            ignored: self.show_ignored,
            visible_only,
        };
        if self.rows_key == Some(key) {
            return;
        }
        self.rows_key = Some(key);
        let rt = &state.runtime.health;
        self.rows = rt
            .problems
            .iter()
            .enumerate()
            .filter(|(_, p)| match p.severity {
                ProblemSeverity::Error => self.show_errors,
                ProblemSeverity::Warning => self.show_warnings,
                ProblemSeverity::Info => self.show_info,
            })
            .filter(|(_, p)| self.show_ignored || !rt.is_ignored(p))
            .filter(|(_, p)| !visible_only || HealthRuntime::on_visible(&state.doc, p))
            .map(|(i, _)| i)
            .collect();
    }

    pub(crate) fn fixable_count(&mut self, state: &AppState) -> usize {
        let rt = &state.runtime.health;
        let key = (rt.revision, rt.problems.as_ptr() as usize, rt.problems.len());
        if let Some((cached, n)) = self.fixable
            && cached == key
        {
            return n;
        }
        let n = rt.problems.iter().filter(|p| p.fixable && !rt.is_ignored(p)).count();
        self.fixable = Some((key, n));
        n
    }

    pub(crate) fn fix(&mut self, state: &mut AppState, index: usize) {
        let Some(p) = state.runtime.health.problems.get(index).cloned() else { return };
        match state.doc.fix_problem(&p) {
            Ok(true) => state.notify(Severity::Success, t!("problems.fixed_one")),
            Ok(false) => state.notify(Severity::Info, t!("problems.nothing_fixed")),
            Err(e) => state.notify(Severity::Error, tf!("problems.fix_failed", e)),
        }
        state.runtime.health.recheck = true;
    }

    pub(crate) fn fix_all(&mut self, state: &mut AppState) {
        let rt = &state.runtime.health;
        let fixable: Vec<Problem> = rt.problems.iter().filter(|p| p.fixable && !rt.is_ignored(p)).cloned().collect();
        if fixable.is_empty() {
            return;
        }
        match state.doc.fix_problems(&fixable) {
            Ok(n) => {
                state.notify(Severity::Success, tf!(plural(n, "problems.fixed_many_one", "problems.fixed_many"), n))
            }
            Err(e) => state.notify(Severity::Error, tf!("problems.fix_failed", e)),
        }
        state.runtime.health.recheck = true;
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.detail.show(ctx, state);
        if !state.ui.show_problems {
            self.was_open = false;
            return;
        }
        if !self.was_open {
            self.was_open = true;
            let rt = &state.runtime.health;
            if !state.settings.health.live_checks || (rt.checks.is_none() && !rt.checking) {
                state.runtime.health.recheck = true;
            }
        }
        let mut open = true;
        let response = hp_app::chrome::Window::new(icon_title("⚠", t!("problems.title")))
            .id(egui::Id::unique("hp_problems_window"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(720.0, 460.0)))
            .open(&mut open)
            .default_size([720.0, 460.0])
            .min_size([440.0, 240.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| self.window_ui(ui, state));
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if !open {
            state.ui.show_problems = false;
        }
        if state.runtime.health.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn header_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let counts = state.runtime.health.counts;
        let fixable = self.fixable_count(state);
        let mut refresh = false;
        let mut fix_all = false;
        ui.horizontal(|ui| {
            for (flag, icon, n, color, tip) in [
                (&mut self.show_errors, "❌", counts.errors, theme::error(), t!("problems.filter_errors")),
                (&mut self.show_warnings, "⚠", counts.warnings, theme::warning(), t!("problems.filter_warnings")),
                (&mut self.show_info, "ℹ", counts.infos, theme::info(), t!("problems.filter_info")),
            ] {
                if ui
                    .add(egui::Button::selectable(*flag, RichText::new(format!("{icon} {n}")).color(color)))
                    .on_hover_text(tip)
                    .clicked()
                {
                    *flag = !*flag;
                }
            }
            if counts.ignored > 0 {
                ui.toggle_value(&mut self.show_ignored, tf!("problems.show_ignored", counts.ignored))
                    .on_hover_text(t!("problems.show_ignored_tip"));
            }
            let mut visible = state.settings.health.visible_only;
            if ui
                .checkbox(&mut visible, t!("problems.visible_only"))
                .on_hover_text(t!("problems.visible_only_tip"))
                .changed()
            {
                state.settings.health.visible_only = visible;
                state.settings_dirty = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(fixable > 0, egui::Button::new(tf!("problems.fix_all", fixable)))
                    .on_hover_text(t!("problems.fix_all_tip"))
                    .clicked()
                {
                    fix_all = true;
                }
                if ui.button(format!("🔄 {}", t!("problems.refresh"))).clicked() {
                    refresh = true;
                }
            });
        });
        let rt = &state.runtime.health;
        if rt.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(t!("problems.checking")).weak().italics());
            });
        } else if rt.leak_checking {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(t!("problems.leak_checking")).weak().italics());
            });
        }
        if matches!(state.assets.status(), AssetStatus::Mounting) {
            ui.label(RichText::new(t!("problems.assets_pending")).weak().italics());
        }
        if refresh {
            state.runtime.health.recheck = true;
        }
        if fix_all {
            self.fix_all(state);
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.header_ui(ui, state);
        ui.separator();
        self.update_rows(state);
        let ignored = state.runtime.health.counts.ignored;
        let mut clear_ignored = false;
        let fixable = self.fixable_count(state);
        dialog_bottom_bar(ui, "hp_problems_bottom", |ui| {
            let rt = &state.runtime.health;
            let shown = rt.problems.len() - rt.counts.ignored;
            ui.label(RichText::new(summary_text(shown, fixable)).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("common.close")).on_hover_text(key_label(egui::Key::Escape)).clicked() {
                    state.ui.show_problems = false;
                }
                if ignored > 0 && ui.button(tf!("problems.unignore_all", ignored)).clicked() {
                    clear_ignored = true;
                }
            });
        });
        let mut action: Option<RowAction> = None;
        dialog_body(ui, |ui| {
            let rt = &state.runtime.health;
            if self.rows.is_empty() {
                ui.centered_and_justified(|ui| {
                    if rt.checks.is_none() {
                        ui.label(RichText::new(t!("problems.waiting")).weak());
                    } else if rt.problems.is_empty() {
                        ui.label(
                            RichText::new(format!("✔ {}", t!("problems.none"))).color(theme::success()).size(16.0),
                        );
                    } else {
                        ui.label(RichText::new(t!("problems.all_filtered")).weak());
                    }
                });
                return;
            }
            let row_h = 24.0;
            let doc = &state.doc;
            let any_fixable =
                self.rows.iter().filter_map(|&pi| rt.problems.get(pi)).any(|p| p.fixable && !rt.is_ignored(p));
            let fix_slot = if any_fixable { button_width(ui, t!("problems.fix")) } else { 0.0 };
            egui::ScrollArea::vertical().id_salt("hp_problems_list").auto_shrink([false, false]).show_rows(
                ui,
                row_h,
                self.rows.len(),
                |ui, range| {
                    for i in range {
                        let Some(&pi) = self.rows.get(i) else { continue };
                        let Some(p) = rt.problems.get(pi) else { continue };
                        let slots = RowSlots { row_h, fix: fix_slot };
                        if let Some(a) = row_ui(ui, slots, p, pi, doc, rt.is_ignored(p), &self.selected) {
                            action = Some(a);
                        }
                    }
                },
            );
        });
        if clear_ignored {
            HealthRuntime::clear_ignored(state);
        }
        match action {
            Some(RowAction::Fix(i)) => self.fix(state, i),
            Some(RowAction::GoTo(i)) => self.go_to(state, i),
            Some(RowAction::Ignore(i)) => {
                if let Some(p) = state.runtime.health.problems.get(i).cloned() {
                    HealthRuntime::ignore(state, &p);
                }
            }
            Some(RowAction::IgnoreKind(code)) => HealthRuntime::ignore_code(state, code),
            Some(RowAction::Unignore(i)) => {
                if let Some(p) = state.runtime.health.problems.get(i).cloned() {
                    HealthRuntime::unignore(state, &p);
                }
            }
            Some(RowAction::Copy(text)) => ui.ctx().copy_text(text),
            Some(RowAction::ShowDetail) => detail::open(state),
            None => {}
        }
    }

    pub(crate) fn go_to(&mut self, state: &mut AppState, index: usize) {
        if let Some(p) = state.runtime.health.problems.get(index).cloned() {
            self.selected = Some(p.key());
            go_to_problem(state, &p);
        }
    }
}

enum RowAction {
    ShowDetail,
    Fix(usize),
    GoTo(usize),
    Ignore(usize),
    IgnoreKind(&'static str),
    Unignore(usize),
    Copy(String),
}

#[derive(Clone, Copy, Debug)]
struct RowSlots {
    row_h: f32,
    fix: f32,
}

const COUNT_WIDTH: f32 = 84.0;

fn button_width(ui: &Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let w = ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE).size().x);
    w + 2.0 * ui.spacing().button_padding.x
}

fn row_ui(
    ui: &mut Ui,
    slots: RowSlots,
    p: &Problem,
    index: usize,
    doc: &Document,
    ignored: bool,
    selected: &Option<String>,
) -> Option<RowAction> {
    let mut action = None;
    let sev = severity_of(p);
    let row_h = slots.row_h;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());
    if selected.as_deref() == Some(p.key().as_str()) {
        ui.painter().rect_filled(rect, 3.0, ui.visuals().selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(6.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    if ignored {
        child.multiply_opacity(0.5);
    }
    child.label(RichText::new(sev.icon()).color(sev.color()));
    let desc = describe(p, doc);
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let slot = |ui: &mut Ui, width: f32, add: &mut dyn FnMut(&mut Ui)| {
            ui.allocate_ui_with_layout(
                egui::vec2(width, row_h),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.set_min_width(width);
                    add(ui);
                },
            );
        };
        if slots.fix > 0.0 {
            slot(ui, slots.fix, &mut |ui| {
                if p.fixable && !ignored && ui.button(t!("problems.fix")).on_hover_text(fix_hint(p.code)).clicked() {
                    action = Some(RowAction::Fix(index));
                }
            });
        }
        if p.code == "detail_candidate"
            && ui.button(t!("problems.detail.list")).on_hover_text(t!("problems.detail.list_tip")).clicked()
        {
            action = Some(RowAction::ShowDetail);
        }
        let can_go = !p.objects.is_empty() || !p.faces.is_empty();
        if ui.add_enabled(can_go, egui::Button::new(t!("problems.goto"))).clicked() {
            action = Some(RowAction::GoTo(index));
        }
        slot(ui, COUNT_WIDTH, &mut |ui| {
            if !p.objects.is_empty() || !p.faces.is_empty() {
                let objs = if p.faces.is_empty() { objects_text(p.objects.len()) } else { faces_text(p.faces.len()) };
                ui.add(egui::Label::new(RichText::new(objs).weak().small()).truncate());
            }
        });
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(desc.as_str()).truncate()).on_hover_text(&p.message);
        });
    });
    if resp.clicked() || resp.double_clicked() {
        action = Some(RowAction::GoTo(index));
    }
    resp.context_menu(|ui| {
        if ui.button(t!("problems.goto")).clicked() {
            action = Some(RowAction::GoTo(index));
            ui.close();
        }
        if p.fixable && !ignored && ui.button(t!("problems.fix")).on_hover_text(fix_hint(p.code)).clicked() {
            action = Some(RowAction::Fix(index));
            ui.close();
        }
        ui.separator();
        if ignored {
            if ui.button(t!("problems.unignore")).clicked() {
                action = Some(RowAction::Unignore(index));
                ui.close();
            }
        } else {
            if ui.button(t!("problems.ignore")).on_hover_text(t!("problems.ignore_tip")).clicked() {
                action = Some(RowAction::Ignore(index));
                ui.close();
            }
            if ui.button(tf!("problems.ignore_kind", kind_name(p.code))).clicked() {
                action = Some(RowAction::IgnoreKind(p.code));
                ui.close();
            }
        }
        ui.separator();
        if ui.button(t!("common.copy_text")).clicked() {
            action = Some(RowAction::Copy(desc.clone()));
            ui.close();
        }
    });
    action
}

mod detail;
#[cfg(test)]
mod tests;
