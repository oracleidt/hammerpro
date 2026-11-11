use egui::{Color32, RichText, Ui};
use hp_app::settings::{GRID_MAX, GRID_MIN, Models2dSetting, RenderModeSetting, SelectModeSetting, WorkflowSettings};
use hp_app::{Action, AppState, t, tf};

use crate::common::{
    Check, GRID_SIZES, action_check, action_enabled, action_icon, close_on_escape, fmt_grid, with_action_tooltip,
};
use crate::icons::Icon;

const BUTTON_SIZE: egui::Vec2 = egui::vec2(24.0, 24.0);
const ROW_GAP: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolbarItem {
    Action(Action),
    GridSize,
    Compile,
    RenderMode,
    Props2d,
    PrimitiveStats,
}

impl ToolbarItem {
    pub fn id(self) -> &'static str {
        match self {
            ToolbarItem::Action(a) => a.id(),
            ToolbarItem::GridSize => "grid_size",
            ToolbarItem::Compile => "compile_button",
            ToolbarItem::RenderMode => "render_mode",
            ToolbarItem::Props2d => "props_2d",
            ToolbarItem::PrimitiveStats => "primitive_stats",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ToolbarItem::Action(a) => a.label(),
            ToolbarItem::GridSize => t!("shell.toolbar.grid_size"),
            ToolbarItem::Compile => t!("shell.toolbar.compile"),
            ToolbarItem::RenderMode => t!("menu.render_mode"),
            ToolbarItem::Props2d => t!("shell.toolbar.props_2d"),
            ToolbarItem::PrimitiveStats => t!("shell.toolbar.primitives"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolbarGroup {
    pub id: &'static str,
    pub label_key: &'static str,
    pub items: &'static [ToolbarItem],
}

impl ToolbarGroup {
    pub fn label(&self) -> &'static str {
        t!(self.label_key)
    }
}

pub const GROUPS: usize = 10;
pub const COMPILE_GROUP: usize = 9;

pub const TOOLBAR: [ToolbarGroup; GROUPS] = {
    use Action as A;
    use ToolbarItem::{Action as I, Compile, GridSize, PrimitiveStats, Props2d, RenderMode};
    const fn g(id: &'static str, label_key: &'static str, items: &'static [ToolbarItem]) -> ToolbarGroup {
        ToolbarGroup { id, label_key, items }
    }
    [
        g("file", "shell.toolbar.group_file", &[I(A::NewMap), I(A::OpenMap), I(A::SaveMap)]),
        g("edit", "shell.toolbar.group_edit", &[I(A::Undo), I(A::Redo), I(A::Cut), I(A::Copy), I(A::Paste)]),
        g("grid", "shell.toolbar.group_grid", &[I(A::ToggleGrid), I(A::ToggleGrid3d), GridSize, I(A::ToggleSnap)]),
        g("groups", "shell.toolbar.group_groups", &[I(A::Carve), I(A::Group), I(A::Ungroup), I(A::ToggleIgnoreGroups)]),
        g("hide", "shell.toolbar.group_hide", &[I(A::HideSelected), I(A::HideUnselected), I(A::UnhideAll)]),
        g("cordon", "shell.toolbar.group_cordon", &[I(A::ToggleCordon), I(A::ToolCordon), I(A::ToggleRadiusCulling)]),
        g(
            "display",
            "shell.toolbar.group_display",
            &[
                I(A::ToggleDispMesh2d),
                I(A::ToggleDispWalkable),
                I(A::ToggleNodraw),
                I(A::ToggleCollisionOnly),
                I(A::ToggleTextureLock),
                I(A::ToggleTextureScaleLock),
            ],
        ),
        g(
            "show",
            "shell.toolbar.group_show",
            &[
                I(A::ToggleHelpers),
                I(A::ToggleModels),
                Props2d,
                I(A::ToggleEntityNames),
                I(A::ToggleToolTextures),
                I(A::ToggleEditorObjects),
                I(A::ToggleSkybox3d),
                I(A::ToggleFogPreview),
                I(A::ToggleFogVolumes),
                I(A::ToggleDetailPreview),
                I(A::ToggleParticles),
                I(A::SimulateRopes),
            ],
        ),
        g(
            "render",
            "shell.toolbar.group_render",
            &[RenderMode, I(A::TogglePlayerView), I(A::CheckProblems), PrimitiveStats],
        ),
        g("compile", "shell.toolbar.group_compile", &[Compile]),
    ]
};

pub const HIDE_ORDER: [usize; GROUPS] = [7, 6, 5, 4, 3, 8, 1, 2, 0, 9];

pub fn group_index(id: &str) -> Option<usize> {
    TOOLBAR.iter().position(|g| g.id == id)
}

pub fn item_by_id(id: &str) -> Option<ToolbarItem> {
    TOOLBAR.iter().flat_map(|g| g.items.iter().copied()).find(|i| i.id() == id)
}

pub fn is_known_id(id: &str) -> bool {
    group_index(id).is_some() || item_by_id(id).is_some()
}

pub fn group_order(w: &WorkflowSettings) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::with_capacity(GROUPS);
    for id in &w.toolbar_order {
        if let Some(i) = group_index(id)
            && i != COMPILE_GROUP
            && !out.contains(&i)
        {
            out.push(i);
        }
    }
    for i in 0..GROUPS {
        if i != COMPILE_GROUP && !out.contains(&i) {
            out.push(i);
        }
    }
    out
}

fn is_hidden(w: &WorkflowSettings, id: &str) -> bool {
    w.toolbar_hidden.iter().any(|h| h == id)
}

pub fn group_switched_on(w: &WorkflowSettings, group: usize) -> bool {
    !is_hidden(w, TOOLBAR[group].id)
}

pub fn visible_items(w: &WorkflowSettings, group: usize) -> Vec<ToolbarItem> {
    if !group_switched_on(w, group) {
        return Vec::new();
    }
    TOOLBAR[group].items.iter().copied().filter(|i| !is_hidden(w, i.id())).collect()
}

pub fn shows_item(w: &WorkflowSettings, item: ToolbarItem) -> bool {
    w.show_toolbar && (0..GROUPS).any(|g| TOOLBAR[g].items.contains(&item) && visible_items(w, g).contains(&item))
}

pub fn shows_action(state: &AppState, action: Action) -> bool {
    shows_item(&state.settings.workflow, ToolbarItem::Action(action))
}

pub fn set_hidden(state: &mut AppState, id: &str, hidden: bool) {
    if !is_known_id(id) {
        return;
    }
    let w = &mut state.settings.workflow;
    let present = is_hidden(w, id);
    if hidden && !present {
        w.toolbar_hidden.push(id.to_owned());
    } else if !hidden && present {
        w.toolbar_hidden.retain(|h| h != id);
    } else {
        return;
    }
    state.settings_dirty = true;
}

pub fn move_group(state: &mut AppState, group: usize, delta: isize) {
    if group == COMPILE_GROUP {
        return;
    }
    let mut order = group_order(&state.settings.workflow);
    let Some(at) = order.iter().position(|g| *g == group) else { return };
    let to = at as isize + delta;
    if to < 0 || to as usize >= order.len() {
        return;
    }
    order.swap(at, to as usize);
    state.settings.workflow.toolbar_order = order.iter().map(|g| TOOLBAR[*g].id.to_owned()).collect();
    state.settings_dirty = true;
}

pub fn set_rows(state: &mut AppState, rows: u8) {
    let rows = rows.clamp(1, 2);
    if state.settings.workflow.toolbar_rows != rows {
        state.settings.workflow.toolbar_rows = rows;
        state.settings_dirty = true;
    }
}

fn custom_key(w: &WorkflowSettings) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    w.toolbar_hidden.hash(&mut h);
    w.toolbar_order.hash(&mut h);
    w.toolbar_rows.hash(&mut h);
    h.finish()
}

fn button_icon(action: Action) -> Option<Icon> {
    match action {
        Action::HideSelected
        | Action::HideUnselected
        | Action::UnhideAll
        | Action::Group
        | Action::Ungroup
        | Action::Carve => crate::icons::action_icon(action),
        _ => None,
    }
}

fn toolbar_glyph(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::ToggleGrid => "#",
        Action::ToggleGrid3d => "🌐",
        Action::ToggleSnap => "⊞",
        Action::Carve => "⛏",
        Action::ToggleIgnoreGroups => "ig",
        Action::ToggleRadiusCulling => "◎",
        Action::ToggleDispMesh2d => "🕸",
        Action::ToggleNodraw => "🚫",
        Action::ToggleTextureScaleLock => "🔐",
        Action::ToggleHelpers => "🔧",
        Action::ToggleModels => "📦",
        Action::ToggleEntityNames => "Aa",
        Action::ToggleToolTextures => "🚧",
        Action::ToggleEditorObjects => "📍",
        Action::ToggleSkybox3d => "☁",
        Action::ToggleFogPreview => "🌁",
        Action::ToggleFogVolumes => "▣",
        Action::ToggleDetailPreview => "🌿",
        Action::ToggleParticles => "🎆",
        Action::SimulateRopes => "〰",
        Action::ApplyCurrentTexture => "✏",
        Action::BrushesToProp => "🗿",
        Action::PropToBrushes => "🔳",
        Action::SendToBlender => "🔗",
        _ => return None,
    })
}

#[cfg(test)]
pub(crate) fn button_glyphs() -> Vec<&'static str> {
    let mut out = Vec::new();
    for group in &TOOLBAR {
        for item in group.items {
            if let ToolbarItem::Action(a) = item
                && button_icon(*a).is_none()
            {
                out.push(button_glyph(*a));
            }
        }
    }
    for item in palette_items() {
        if let PaletteItem::Action(a) = item {
            out.push(button_glyph(a));
        }
    }
    out
}

fn is_checked(state: &AppState, action: Action) -> bool {
    matches!(action_check(state, action), Check::Toggle(true) | Check::Radio(true))
}

fn action_icon_button(ui: &mut Ui, state: &mut AppState, action: Action, nested: bool) {
    let enabled = action_enabled(state, action);
    let selected = is_checked(state, action);
    let r = match button_icon(action) {
        None => ui.add_enabled(
            enabled,
            egui::Button::selectable(selected, RichText::new(button_glyph(action)).size(15.0)).min_size(BUTTON_SIZE),
        ),
        Some(icon) => {
            let r = ui.add_enabled(enabled, egui::Button::selectable(selected, "").min_size(BUTTON_SIZE));
            let color = if enabled { ui.style().interact(&r).fg_stroke.color } else { ui.visuals().weak_text_color() };
            crate::icons::paint(ui.painter(), r.rect.center(), color, icon, 0.8);
            r
        }
    };
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, action.label()));
    let r = match toolbar_hint(action) {
        Some(hint) => {
            let tip = crate::common::action_tooltip(state, action);
            let show = |ui: &mut Ui| {
                ui.label(&tip);
                ui.label(RichText::new(hint).small().color(ui.visuals().weak_text_color()));
            };
            r.on_hover_ui(show).on_disabled_hover_ui(show)
        }
        None => with_action_tooltip(r, state, action),
    };
    if r.clicked() {
        state.request(action);
    }
    if !nested {
        customize_popup(&r, state);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowPlan {
    pub rows: Vec<Vec<usize>>,
    pub hidden: Vec<usize>,
}

fn flow(
    shown: &[usize],
    widths: &[f32],
    separator: f32,
    first: f32,
    other: f32,
    rows: usize,
    overflow: f32,
) -> Option<Vec<Vec<usize>>> {
    let cap = |row: usize| if row == 0 { first } else { other };
    let mut out: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for &g in shown {
        let w = widths.get(g).copied().unwrap_or(0.0);
        let row = out.len() - 1;
        let extra = if out[row].is_empty() { w } else { separator + w };
        if used + extra <= cap(row) {
            out[row].push(g);
            used += extra;
            continue;
        }
        if out[row].is_empty() || out.len() >= rows || w > cap(row + 1) {
            return None;
        }
        out.push(vec![g]);
        used = w;
    }
    if overflow > 0.0 && used + overflow > cap(out.len() - 1) {
        return None;
    }
    Some(out)
}

#[allow(clippy::too_many_arguments)]
pub fn plan_rows(
    order: &[usize],
    widths: &[f32],
    hide_order: &[usize],
    separator: f32,
    first: f32,
    other: f32,
    rows: usize,
    overflow: f32,
) -> RowPlan {
    let rows = rows.max(1);
    let mut shown: Vec<usize> = order.to_vec();
    let mut hide = hide_order.iter().copied().filter(|i| order.contains(i));
    let plan = |shown: &[usize], rows: Vec<Vec<usize>>| RowPlan {
        rows,
        hidden: order.iter().copied().filter(|i| !shown.contains(i)).collect(),
    };
    loop {
        let reserve = if shown.len() < order.len() { overflow } else { 0.0 };
        if let Some(out) = flow(&shown, widths, separator, first, other, rows, reserve) {
            return plan(&shown, out);
        }
        if shown.len() <= 1 {
            return plan(&shown, vec![shown.clone()]);
        }
        match hide.next() {
            Some(i) => shown.retain(|g| *g != i),
            None => return plan(&shown, vec![shown.clone()]),
        }
    }
}

const OVERFLOW_WIDTH: f32 = 28.0;
const SEPARATOR_ESTIMATE: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ToolbarWidths {
    key: (hp_app::i18n::Lang, u32, u64),
    groups: [Option<f32>; GROUPS],
    separator: Option<f32>,
}

impl ToolbarWidths {
    fn new(key: (hp_app::i18n::Lang, u32, u64)) -> Self {
        Self { key, groups: [None; GROUPS], separator: None }
    }

    fn widths(&self, items: &[Vec<ToolbarItem>; GROUPS]) -> [f32; GROUPS] {
        std::array::from_fn(|i| self.groups[i].unwrap_or_else(|| estimate_width(&items[i])))
    }
}

fn estimate_width(group: &[ToolbarItem]) -> f32 {
    group
        .iter()
        .map(|item| match item {
            ToolbarItem::GridSize => 160.0,
            ToolbarItem::Compile => 100.0,
            ToolbarItem::RenderMode => 150.0,
            _ => BUTTON_SIZE.x + 1.0,
        })
        .sum()
}

fn toolbar_spacing(ui: &mut Ui) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
    ui.spacing_mut().item_spacing.x = 1.0;
    ui.spacing_mut().button_padding.x = 3.0;
}

fn group_items(ui: &mut Ui, state: &mut AppState, group: &[ToolbarItem], nested: bool) {
    for item in group {
        match *item {
            ToolbarItem::Action(a) => action_icon_button(ui, state, a, nested),
            ToolbarItem::GridSize => grid_controls(ui, state, nested),
            ToolbarItem::Compile => run_button(ui, state, nested),
            ToolbarItem::RenderMode if nested => render_mode_submenu(ui, state),
            ToolbarItem::RenderMode => render_mode_dropdown(ui, state),
            ToolbarItem::Props2d if nested => props_2d_submenu(ui, state),
            ToolbarItem::Props2d => {
                props_2d_button(ui, state);
            }
            ToolbarItem::PrimitiveStats => {
                let on = state.settings.render.primitive_stats;
                let r = ui
                    .add(egui::Button::selectable(on, "Σ").min_size(BUTTON_SIZE))
                    .on_hover_text(t!("shell.toolbar.primitives_tip"));
                if r.clicked() {
                    state.settings.render.primitive_stats = !on;
                    state.settings_dirty = true;
                }
                if !nested {
                    customize_popup(&r, state);
                }
            }
        }
    }
}

fn measured_group(ui: &mut Ui, state: &mut AppState, group: &[ToolbarItem], nested: bool) -> f32 {
    ui.scope(|ui| group_items(ui, state, group, nested)).response.rect.width()
}

fn toolbar_widths_id() -> egui::Id {
    egui::Id::unique("hp_shell_toolbar_widths")
}

pub fn toolbar_overflow_id() -> egui::Id {
    egui::Id::unique("hp_shell_toolbar_overflow")
}

pub fn toolbar(ui: &mut Ui, state: &mut AppState) {
    if !state.settings.workflow.show_toolbar {
        return;
    }
    let w = &state.settings.workflow;
    let rows = if w.toolbar_two_rows() { 2 } else { 1 };
    let items: [Vec<ToolbarItem>; GROUPS] = std::array::from_fn(|i| visible_items(w, i));
    let order: Vec<usize> = group_order(w).into_iter().filter(|g| !items[*g].is_empty()).collect();
    let compile = !items[COMPILE_GROUP].is_empty();
    let memo_id = toolbar_widths_id();
    let key = (hp_app::i18n::lang(), ui.ctx().pixels_per_point().to_bits(), custom_key(w));
    let mut memo: ToolbarWidths =
        ui.data(|d| d.get_temp::<ToolbarWidths>(memo_id)).filter(|m| m.key == key).unwrap_or(ToolbarWidths::new(key));
    let separator = memo.separator.unwrap_or(SEPARATOR_ESTIMATE);
    let widths = memo.widths(&items);
    let available = ui.available_width();
    let reserved = if compile { widths[COMPILE_GROUP] + separator } else { 0.0 };
    let plan =
        plan_rows(&order, &widths, &HIDE_ORDER, separator, available - reserved, available, rows, OVERFLOW_WIDTH);
    let background = ui
        .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            let row_count = plan.rows.len().max(1);
            for r in 0..row_count {
                ui.horizontal(|ui| {
                    toolbar_spacing(ui);
                    ui.set_min_height(BUTTON_SIZE.y);
                    let row = plan.rows.get(r).map(Vec::as_slice).unwrap_or_default();
                    for (n, &g) in row.iter().enumerate() {
                        if n > 0 {
                            let start = ui.cursor().min.x;
                            ui.separator();
                            memo.separator = Some(ui.cursor().min.x - start);
                        }
                        memo.groups[g] = Some(measured_group(ui, state, &items[g], false));
                    }
                    if r + 1 == row_count && !plan.hidden.is_empty() {
                        overflow_button(ui, state, &plan.hidden, &items, &mut memo);
                    }
                    if r == 0 && compile {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            memo.groups[COMPILE_GROUP] = Some(measured_group(ui, state, &items[COMPILE_GROUP], false));
                            if !row.is_empty() || !plan.hidden.is_empty() {
                                ui.separator();
                            }
                        });
                    }
                });
            }
        })
        .response;
    customize_popup(&background, state);
    ui.data_mut(|d| *d.get_temp_mut_or_insert_with(memo_id, || memo) = memo);
}

fn overflow_button(
    ui: &mut Ui,
    state: &mut AppState,
    hidden: &[usize],
    items: &[Vec<ToolbarItem>; GROUPS],
    memo: &mut ToolbarWidths,
) {
    let button = ui.add(egui::Button::new(RichText::new("»").size(15.0).strong()).min_size(BUTTON_SIZE));
    let button = crate::common::lazy_hover_text(button, || {
        let count: usize = hidden.iter().map(|i| items[*i].len()).sum();
        tf!("shell.toolbar.more_tip", count)
    });
    egui::Popup::menu(&button)
        .id(toolbar_overflow_id())
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            for &i in hidden {
                ui.horizontal(|ui| {
                    toolbar_spacing(ui);
                    memo.groups[i] = Some(measured_group(ui, state, &items[i], true));
                });
            }
            close_on_escape(ui);
        });
}

pub fn customize_popup(response: &egui::Response, state: &mut AppState) {
    egui::Popup::context_menu(response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| customize_menu(ui, state));
}

pub fn customize_menu(ui: &mut Ui, state: &mut AppState) {
    let mut two = state.settings.workflow.toolbar_two_rows();
    if ui.checkbox(&mut two, t!("shell.toolbar.two_rows")).on_hover_text(t!("shell.toolbar.two_rows_tip")).changed() {
        set_rows(state, if two { 2 } else { 1 });
    }
    ui.separator();
    ui.label(RichText::new(t!("shell.toolbar.customize")).strong());
    let mut order = group_order(&state.settings.workflow);
    order.push(COMPILE_GROUP);
    let movable = order.len() - 1;
    for (pos, &g) in order.iter().enumerate() {
        let on = group_switched_on(&state.settings.workflow, g);
        let text = RichText::new(TOOLBAR[g].label());
        let text = if on { text } else { text.color(ui.visuals().weak_text_color()) };
        ui.menu_button(text, |ui| group_menu(ui, state, g, pos, movable));
    }
    ui.separator();
    if ui.button(t!("action.reset_toolbar")).clicked() {
        state.settings.workflow.reset_toolbar();
        state.settings_dirty = true;
        ui.close();
    }
    if ui.button(t!("shell.toolbar.hide")).clicked() {
        state.settings.workflow.show_toolbar = false;
        state.settings_dirty = true;
        ui.close();
    }
    close_on_escape(ui);
}

fn group_menu(ui: &mut Ui, state: &mut AppState, group: usize, pos: usize, movable: usize) {
    let id = TOOLBAR[group].id;
    let mut on = group_switched_on(&state.settings.workflow, group);
    if ui.checkbox(&mut on, t!("shell.toolbar.show_group")).changed() {
        set_hidden(state, id, !on);
    }
    if group == COMPILE_GROUP {
        ui.separator();
        ui.label(RichText::new(t!("shell.toolbar.compile_pinned")).small().color(ui.visuals().weak_text_color()));
        return;
    }
    ui.separator();
    ui.add_enabled_ui(on, |ui| {
        for item in TOOLBAR[group].items {
            let mut shown = !is_hidden(&state.settings.workflow, item.id());
            if ui.checkbox(&mut shown, item.label()).changed() {
                set_hidden(state, item.id(), !shown);
            }
        }
    });
    ui.separator();
    if ui.add_enabled(pos > 0, egui::Button::new(t!("shell.toolbar.move_left"))).clicked() {
        move_group(state, group, -1);
    }
    if ui.add_enabled(pos + 1 < movable, egui::Button::new(t!("shell.toolbar.move_right"))).clicked() {
        move_group(state, group, 1);
    }
}

fn toolbar_hint(action: Action) -> Option<&'static str> {
    match action {
        Action::ToggleCollisionOnly => Some(t!("shell.toolbar.collision_only_tip")),
        _ => None,
    }
}

pub const RENDER_MODES: [(RenderModeSetting, Action); 10] = [
    (RenderModeSetting::Textured, Action::RenderTextured),
    (RenderModeSetting::Unshaded, Action::RenderUnshaded),
    (RenderModeSetting::Flat, Action::RenderFlat),
    (RenderModeSetting::Wireframe, Action::RenderWireframe),
    (RenderModeSetting::TexturedWire, Action::RenderTexturedWire),
    (RenderModeSetting::Lit, Action::RenderLit),
    (RenderModeSetting::SmoothingGroups, Action::RenderSmoothingGroups),
    (RenderModeSetting::Baked, Action::RenderBaked),
    (RenderModeSetting::LightmapGrid, Action::RenderLightmapGrid),
    (RenderModeSetting::ColorCorrected, Action::RenderColorCorrected),
];

pub const SELECT_MODES: [(SelectModeSetting, Action); 3] = [
    (SelectModeSetting::Groups, Action::SelectModeGroups),
    (SelectModeSetting::Objects, Action::SelectModeObjects),
    (SelectModeSetting::Solids, Action::SelectModeSolids),
];

pub fn set_grid_size(state: &mut AppState, size: f64) {
    if !size.is_finite() || size <= 0.0 {
        return;
    }
    if let Err(e) = state.set_target_grid_size(size) {
        state.notify(hp_app::Severity::Warning, e);
    }
}

pub fn parse_grid_size(text: &str) -> Option<f64> {
    let text = text.trim().replace(',', ".");
    let v: f64 = text.parse().ok()?;
    (v.is_finite() && v >= GRID_MIN).then(|| v.min(GRID_MAX))
}

fn grid_edit_id() -> egui::Id {
    egui::Id::unique("hp_shell_grid_edit")
}

fn props_2d_button(ui: &mut Ui, state: &mut AppState) -> egui::Response {
    let on = state.settings.render.models_in_2d != Models2dSetting::Boxes;
    let response = ui
        .add(egui::Button::selectable(on, "2D").min_size(BUTTON_SIZE))
        .on_hover_text(t!("shell.toolbar.props_2d_tip"));
    if response.clicked() {
        state.settings.render.models_in_2d = if on { Models2dSetting::Boxes } else { Models2dSetting::Wireframe };
        state.settings_dirty = true;
    }
    response.context_menu(|ui| {
        for (mode, key) in [
            (Models2dSetting::Boxes, "shell.render.models_boxes"),
            (Models2dSetting::Wireframe, "shell.render.models_wire"),
            (Models2dSetting::SelectedOnly, "shell.render.models_selected"),
        ] {
            if ui.selectable_label(state.settings.render.models_in_2d == mode, t!(key)).clicked() {
                state.settings.render.models_in_2d = mode;
                state.settings_dirty = true;
                ui.close();
            }
        }
    });
    response
}

pub fn selection_bar(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        select_mode_control(ui, state);
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(t!("shell.toolbar.move_selected")).color(ui.visuals().weak_text_color()));
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (action, key) in
                [(Action::MoveToWorld, "shell.toolbar.to_world"), (Action::TieToEntity, "shell.toolbar.to_entity")]
            {
                let enabled = action_enabled(state, action);
                let r = ui.add_enabled(enabled, egui::Button::new(t!(key)).min_size(egui::vec2(0.0, 24.0)));
                if with_action_tooltip(r, state, action).clicked() {
                    state.request(action);
                }
            }
        });
    });
}

pub fn mcp_pill(ui: &mut Ui, state: &mut AppState) -> egui::Response {
    let mcp = &state.runtime.mcp;
    let on = mcp.running;
    let visuals = ui.visuals();
    let (fg, bg, stroke) = if on {
        (visuals.panel_fill, visuals.strong_text_color(), visuals.strong_text_color())
    } else {
        (visuals.weak_text_color(), Color32::TRANSPARENT, visuals.widgets.inactive.fg_stroke.color.gamma_multiply(0.5))
    };
    let text = match (on, mcp.port) {
        (true, Some(port)) => {
            format!("{} · {port} · {}", t!("shell.mcp.pill_on"), crate::status::mcp::clients_text(mcp.clients))
        }
        (true, None) => t!("shell.mcp.pill_on").to_owned(),
        (false, _) => t!("shell.mcp.pill_off").to_owned(),
    };
    let more = ui.add(egui::Button::new(RichText::new("⏷").small()).frame(false).min_size(egui::vec2(16.0, 22.0)));
    if more.on_hover_text(t!("shell.mcp.more")).clicked() {
        state.request(Action::McpPanel);
    }
    let pill = egui::Button::new(RichText::new(text).strong().color(fg))
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(11.0)
        .min_size(egui::vec2(0.0, 22.0));
    let tip = crate::status::mcp::tooltip(state);
    let r = ui.add(pill).on_hover_text(tip);
    if r.clicked() {
        state.request(Action::ToggleMcpServer);
    }
    r.context_menu(|ui| {
        if ui.button(t!("shell.mcp.more")).clicked() {
            state.request(Action::McpPanel);
            ui.close();
        }
    });
    r
}

fn grid_controls(ui: &mut Ui, state: &mut AppState, nested: bool) {
    let target = state.target_grid();
    let label = crate::grids::source_label(&target);
    let custom = matches!(target.source, hp_app::grid::GridSource::Map(_) | hp_app::grid::GridSource::Preset(_));
    let text = RichText::new(label);
    let text = if custom { text.strong() } else { text.color(ui.visuals().weak_text_color()) };
    if nested {
        let button = ui
            .menu_button(text, |ui| {
                crate::grids::grid_menu(ui, state);
                close_on_escape(ui);
            })
            .response;
        crate::common::lazy_hover_text(button, || crate::grids::source_tip(state, &target));
    } else {
        let button = ui
            .scope(|ui| {
                ui.spacing_mut().button_padding.x = 2.0;
                ui.add(egui::Button::new(text).frame_when_inactive(false).min_size(egui::vec2(0.0, 24.0)))
            })
            .inner;
        egui::Popup::menu(&button).show(|ui| {
            crate::grids::grid_menu(ui, state);
            close_on_escape(ui);
        });
        crate::common::lazy_hover_text(button, || crate::grids::source_tip(state, &target));
    }
    let smaller = ui.add_enabled(
        state.grid_size() > GRID_MIN + 1e-9,
        egui::Button::new(RichText::new("−").strong()).min_size(egui::vec2(20.0, 24.0)),
    );
    if with_action_tooltip(smaller, state, Action::GridSmaller).clicked() {
        state.request(Action::GridSmaller);
    }
    let current = state.grid_size();
    let id = grid_edit_id();
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| fmt_grid(current));
    let field = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id.with("field"))
            .font(egui::TextStyle::Monospace)
            .align(egui::Align2::CENTER_CENTER)
            .min_size(egui::vec2(48.0, BUTTON_SIZE.y))
            .desired_width(48.0),
    );
    if field.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    } else {
        if field.lost_focus() && !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            match parse_grid_size(&text) {
                Some(g) => set_grid_size(state, g),
                None => state.notify(hp_app::Severity::Warning, t!("shell.toolbar.grid_invalid")),
            }
        }
        ui.data_mut(|d| d.remove::<String>(id));
    }
    crate::common::lazy_hover_text(field, || {
        format!(
            "{}\n{}",
            t!("shell.toolbar.grid_size_tip"),
            tf!("shell.grid.tip_steps", crate::grids::steps_text(target.frame.step.to_array()))
        )
    });
    let mut chosen = None;
    let sizes = egui::Button::new(RichText::new("⏷").small()).min_size(egui::vec2(20.0, BUTTON_SIZE.y));
    egui::containers::menu::MenuButton::from_button(sizes)
        .ui(ui, |ui| {
            ui.set_min_width(70.0);
            for g in GRID_SIZES {
                if ui.selectable_label((g - current).abs() < 1e-9, RichText::new(fmt_grid(g)).monospace()).clicked() {
                    chosen = Some(g);
                    ui.close();
                }
            }
            close_on_escape(ui);
        })
        .0
        .on_hover_text(t!("shell.toolbar.grid_size"));
    if let Some(g) = chosen {
        set_grid_size(state, g);
    }
    let larger = ui.add_enabled(
        state.grid_size() < GRID_MAX - 1e-9,
        egui::Button::new(RichText::new("+").strong()).min_size(egui::vec2(20.0, 24.0)),
    );
    if with_action_tooltip(larger, state, Action::GridLarger).clicked() {
        state.request(Action::GridLarger);
    }
}

fn select_mode_control(ui: &mut Ui, state: &mut AppState) {
    ui.label(RichText::new(t!("select_mode.label")).color(ui.visuals().weak_text_color()));
    let current = state.settings.edit.select_mode;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (mode, action) in SELECT_MODES {
            let on = current == mode;
            let b = egui::Button::selectable(on, action.label()).min_size(egui::vec2(0.0, 24.0));
            let tip = match mode {
                SelectModeSetting::Groups => t!("shell.toolbar.mode_groups_tip"),
                SelectModeSetting::Objects => t!("shell.toolbar.mode_objects_tip"),
                SelectModeSetting::Solids => t!("shell.toolbar.mode_solids_tip"),
            };
            if ui.add(b).on_hover_text(tip).clicked() && !on {
                state.settings.edit.select_mode = mode;
                state.settings_dirty = true;
            }
        }
    });
}

fn render_mode_dropdown(ui: &mut Ui, state: &mut AppState) {
    let current = state.settings.view.render_mode;
    let current_label = RENDER_MODES.iter().find(|(m, _)| *m == current).map(|(_, a)| a.label()).unwrap_or("");
    let mut chosen = None;
    let label = egui::WidgetText::from(format!("🎨 {current_label}")).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        110.0,
        egui::TextStyle::Button,
    );
    egui::ComboBox::from_id_salt("hp_shell_render_mode")
        .selected_text(label)
        .width(130.0)
        .wrap_mode(egui::TextWrapMode::Truncate)
        .show_ui(ui, |ui| {
            ui.set_min_width(220.0);
            for (mode, action) in RENDER_MODES {
                let mut b = egui::Button::selectable(mode == current, action.label());
                if let Some(sc) = state.keys.shortcut(action) {
                    b = b.shortcut_text(hp_app::actions::shortcut_label(&sc));
                }
                if ui.add(b).clicked() {
                    chosen = Some(action);
                }
            }
            close_on_escape(ui);
        })
        .response
        .on_hover_text(t!("menu.render_mode"));
    if let Some(action) = chosen {
        state.request(action);
    }
}

fn render_mode_submenu(ui: &mut Ui, state: &mut AppState) {
    let current = state.settings.view.render_mode;
    let current_label = RENDER_MODES.iter().find(|(m, _)| *m == current).map(|(_, a)| a.label()).unwrap_or("");
    let mut chosen = None;
    ui.menu_button(format!("🎨 {current_label}"), |ui| {
        for (mode, action) in RENDER_MODES {
            let mut b = egui::Button::selectable(mode == current, action.label());
            if let Some(sc) = state.keys.shortcut(action) {
                b = b.shortcut_text(hp_app::actions::shortcut_label(&sc));
            }
            if ui.add(b).clicked() {
                chosen = Some(action);
                ui.close();
            }
        }
        close_on_escape(ui);
    })
    .response
    .on_hover_text(t!("menu.render_mode"));
    if let Some(action) = chosen {
        state.request(action);
    }
}

fn props_2d_submenu(ui: &mut Ui, state: &mut AppState) {
    let on = state.settings.render.models_in_2d != Models2dSetting::Boxes;
    ui.menu_button(
        RichText::new("2D").strong().color(if on {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        }),
        |ui| {
            for (mode, key) in [
                (Models2dSetting::Boxes, "shell.render.models_boxes"),
                (Models2dSetting::Wireframe, "shell.render.models_wire"),
                (Models2dSetting::SelectedOnly, "shell.render.models_selected"),
            ] {
                if ui.selectable_label(state.settings.render.models_in_2d == mode, t!(key)).clicked() {
                    state.settings.render.models_in_2d = mode;
                    state.settings_dirty = true;
                    ui.close();
                }
            }
        },
    )
    .response
    .on_hover_text(t!("shell.toolbar.props_2d_tip"));
}

fn run_button(ui: &mut Ui, state: &mut AppState, nested: bool) {
    let r = with_action_tooltip(compile_button(ui), state, Action::RunMap);
    if r.clicked() {
        state.request(Action::RunMap);
    }
    if !nested {
        customize_popup(&r, state);
    }
}

const HAMMER_ICON: f32 = 14.0;

fn compile_button(ui: &mut Ui) -> egui::Response {
    use egui::emath::GuiRounding as _;
    let text = t!("shell.toolbar.compile");
    let on_accent = hp_app::theme::on_accent();
    let galley = egui::WidgetText::from(RichText::new(text).strong().color(on_accent)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let pad = 7.0;
    let gap = 5.0;
    let size = egui::vec2(pad + HAMMER_ICON + gap + galley.size().x + pad, BUTTON_SIZE.y);
    let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
    if ui.is_rect_visible(rect) {
        let (hover, press) = hp_app::widgets::hover_amounts(ui, &r);
        let accent = hp_app::theme::accent();
        let fill = accent.lerp_to_gamma(on_accent, (hover * 0.14 + press * 0.12).min(0.3));
        let painter = ui.painter();
        painter.rect_filled(rect, hp_app::theme::RADIUS, fill);
        painter.rect_stroke(rect, hp_app::theme::RADIUS, egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
        let icon = egui::pos2(rect.left() + pad + HAMMER_ICON * 0.5, rect.center().y);
        crate::icons::paint(painter, icon, on_accent, Icon::Hammer, 0.95);
        let origin = egui::pos2(rect.left() + pad + HAMMER_ICON + gap, rect.center().y - galley.size().y * 0.5)
            .round_to_pixels(ui.ctx().pixels_per_point());
        painter.galley(origin, galley, on_accent);
        if r.has_focus() {
            painter.rect_stroke(
                rect.expand(1.0),
                f32::from(hp_app::theme::RADIUS) + 1.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Outside,
            );
        }
    }
    r
}

pub use crate::tool_strip::{PaletteItem, palette_items, tool_palette};

pub(crate) fn button_glyph(action: Action) -> &'static str {
    toolbar_glyph(action).or_else(|| action_icon(action)).unwrap_or("?")
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    (
        "shell.toolbar.primitives_tip",
        "Map primitives and measured editor CPU allocations",
        "Primitives de la map et allocations CPU mesurées de l'éditeur",
    ),
    ("shell.toolbar.compile", "Compile", "Compiler"),
    (
        "shell.toolbar.more_tip",
        "More buttons ({0}): the toolbar is too narrow to show them all",
        "Autres boutons ({0}) : la barre d'outils est trop étroite pour tous les afficher",
    ),
    (
        "shell.toolbar.props_2d_tip",
        "Prop wireframes in 2D: show or hide model edges. When hidden, bounding boxes remain selectable. Right-click to show wireframes only for selected props.",
        "Wireframe des props en 2D : afficher ou masquer les arêtes des modèles. Quand elles sont masquées, les boîtes restent sélectionnables. Clic droit pour afficher uniquement les props sélectionnés en wireframe.",
    ),
    (
        "shell.toolbar.isolate_on",
        "Isolate the selection (hide everything else)",
        "Isoler la sélection (masquer tout le reste)",
    ),
    ("shell.toolbar.isolate_off", "Show everything again", "Tout réafficher"),
    ("shell.toolbar.isolate_none", "Select something to isolate it", "Sélectionnez quelque chose pour l'isoler"),
    ("shell.toolbar.grid_size", "Grid size", "Taille de la grille"),
    (
        "shell.toolbar.grid_size_tip",
        "Grid size: type any size (2.5 or 80 for example) and press Enter",
        "Taille de la grille : saisissez n'importe quelle taille (2,5 ou 80 par exemple) puis appuyez sur Entrée",
    ),
    (
        "shell.toolbar.grid_invalid",
        "The grid size must be a number from 0.01 to 4096",
        "La taille de la grille doit être un nombre de 0,01 à 4096",
    ),
    (
        "shell.toolbar.collision_only_tip",
        "3D views show each model as its collision model only (what the player bumps into); an orange box marks a model that has no collision model",
        "Les vues 3D montrent chaque modèle par son seul modèle de collision (ce que le joueur percute) ; une boîte orange signale un modèle sans modèle de collision",
    ),
    ("shell.toolbar.move_selected", "Move selected:", "Déplacer la sélection :"),
    ("shell.toolbar.to_world", "To world", "Vers le monde"),
    ("shell.toolbar.to_entity", "To entity", "Vers une entité"),
    (
        "shell.toolbar.propper_needs",
        "Select the brushes to turn into a prop",
        "Sélectionnez les brushes à transformer en prop",
    ),
    (
        "shell.toolbar.unpropper_needs",
        "Select a prop to turn back into brushes",
        "Sélectionnez un prop à retransformer en brushes",
    ),
    (
        "shell.toolbar.blender_needs",
        "Select displacements, brushes or a prop to send to Blender",
        "Sélectionnez des displacements, des brushes ou un prop à envoyer vers Blender",
    ),
    (
        "shell.toolbar.apply_texture_needs",
        "Select the brushes or faces to texture",
        "Sélectionnez les brushes ou les faces à texturer",
    ),
    ("shell.toolbar.mode_groups_tip", "Clicks select whole groups", "Un clic sélectionne le groupe entier"),
    (
        "shell.toolbar.mode_objects_tip",
        "Clicks select whole entities, ignoring groups",
        "Un clic sélectionne l'entité entière, sans tenir compte des groupes",
    ),
    (
        "shell.toolbar.mode_solids_tip",
        "Clicks select single solids, even inside entities",
        "Un clic sélectionne un seul solide, même dans une entité",
    ),
    ("shell.toolbar.props_2d", "Prop wireframes in 2D", "Wireframe des props en 2D"),
    ("shell.toolbar.primitives", "Map primitives", "Primitives de la map"),
    ("shell.toolbar.group_file", "File", "Fichier"),
    ("shell.toolbar.group_edit", "Edit", "Édition"),
    ("shell.toolbar.group_grid", "Grid", "Grille"),
    ("shell.toolbar.group_groups", "Carve and groups", "Creuser et groupes"),
    ("shell.toolbar.group_hide", "Hide and show", "Masquer et afficher"),
    ("shell.toolbar.group_cordon", "Cordons", "Cordons"),
    ("shell.toolbar.group_display", "Displacements and texture lock", "Displacements et verrouillage des textures"),
    ("shell.toolbar.group_show", "Show in the views", "Afficher dans les vues"),
    ("shell.toolbar.group_render", "Render mode and checks", "Mode de rendu et vérifications"),
    ("shell.toolbar.group_compile", "Compile", "Compilation"),
    ("shell.toolbar.two_rows", "Wrap on two rows", "Passer sur deux lignes"),
    (
        "shell.toolbar.two_rows_tip",
        "When the window is too narrow, the toolbar continues on a second row instead of moving buttons into the » menu",
        "Quand la fenêtre est trop étroite, la barre d'outils continue sur une deuxième ligne au lieu de ranger des boutons dans le menu »",
    ),
    ("shell.toolbar.customize", "Customize toolbar", "Personnaliser la barre d'outils"),
    ("shell.toolbar.show_group", "Show this group", "Afficher ce groupe"),
    ("shell.toolbar.move_left", "Move left", "Déplacer vers la gauche"),
    ("shell.toolbar.move_right", "Move right", "Déplacer vers la droite"),
    ("shell.toolbar.hide", "Hide the toolbar", "Masquer la barre d'outils"),
    (
        "shell.toolbar.compile_pinned",
        "Compile stays at the right end of the first row",
        "Compiler reste au bout à droite de la première ligne",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_size_clamps_and_marks_dirty() {
        let mut state = hp_app::testing::test_state();
        set_grid_size(&mut state, 64.0);
        assert_eq!(state.settings.edit.grid_size, 64.0);
        assert!(state.settings_dirty);
        set_grid_size(&mut state, 100000.0);
        assert_eq!(state.settings.edit.grid_size, 4096.0);
        set_grid_size(&mut state, 0.001);
        assert_eq!(state.settings.edit.grid_size, GRID_MIN);
        set_grid_size(&mut state, 80.0);
        assert_eq!(state.settings.edit.grid_size, 80.0);
        set_grid_size(&mut state, 0.0);
        set_grid_size(&mut state, f64::NAN);
        assert_eq!(state.settings.edit.grid_size, 80.0, "bad values are ignored");
    }

    #[test]
    fn hidden_toolbar_draws_nothing() {
        let mut state = hp_app::testing::test_state();
        state.settings.workflow.show_toolbar = false;
        let ctx = egui::Context::default();
        let mut height = f32::NAN;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let before = ui.cursor().top();
            toolbar(ui, &mut state);
            height = ui.cursor().top() - before;
        });
        out.textures_delta.clear();
        assert_eq!(height, 0.0);
    }

    #[test]
    fn toolbar_fits_a_full_hd_screen() {
        let _guard = crate::tests::lang_guard();
        crate::register_translations();
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            for width in [1920.0, 1536.0] {
                let mut state = hp_app::testing::test_state();
                let ctx = egui::Context::default();
                let mut height = 0.0;
                let mut right = 0.0;
                for _ in 0..2 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 1080.0))),
                        ..Default::default()
                    };
                    let mut out = ctx.run_ui(input, |ui| {
                        let before = ui.cursor().top();
                        toolbar(ui, &mut state);
                        height = ui.cursor().top() - before;
                        right = ui.min_rect().right();
                    });
                    out.textures_delta.clear();
                }
                assert!(height < 40.0, "{lang:?}, width {width}: toolbar wrapped to {height} points");
                assert!(right <= width, "{lang:?}, width {width}: toolbar extends to {right}");
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }

    #[test]
    fn props_2d_toggle_hides_selected_wireframes_and_restores_all_wireframes() {
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        for expected in [None, Some(Models2dSetting::Boxes), Some(Models2dSetting::Wireframe)] {
            let events = expected.map_or_else(Vec::new, |_| {
                [true, false]
                    .map(|pressed| egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .into()
            });
            let mut out = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
                rect = props_2d_button(ui, &mut state).rect;
            });
            out.textures_delta.clear();
            if let Some(expected) = expected {
                assert_eq!(state.settings.render.models_in_2d, expected);
                assert!(state.settings_dirty);
                state.settings_dirty = false;
            }
        }
    }

    #[test]
    fn typed_grid_sizes() {
        assert_eq!(parse_grid_size("16"), Some(16.0));
        assert_eq!(parse_grid_size(" 2,5 "), Some(2.5));
        assert_eq!(parse_grid_size("0.01"), Some(0.01));
        assert_eq!(parse_grid_size("5000"), Some(4096.0));
        assert_eq!(parse_grid_size("0.001"), None);
        assert_eq!(parse_grid_size("-8"), None);
        assert_eq!(parse_grid_size("abc"), None);
        assert_eq!(parse_grid_size("NaN"), None);
        assert_eq!(parse_grid_size("inf"), None);
        assert_eq!(parse_grid_size(""), None);
    }

    #[test]
    fn grid_field_edits_the_grid() {
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let field = grid_edit_id().with("field");
        let frame = |state: &mut AppState, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| toolbar(ui, state));
            out.textures_delta.clear();
        };
        frame(&mut state, vec![]);
        ctx.memory_mut(|m| m.request_focus(field));
        frame(&mut state, vec![]);
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut state,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        frame(&mut state, vec![egui::Event::Text("80".into())]);
        assert_eq!(state.settings.edit.grid_size, 16.0, "applied on Enter only");
        frame(&mut state, vec![key(egui::Key::Enter, true)]);
        frame(&mut state, vec![key(egui::Key::Enter, false)]);
        assert_eq!(state.settings.edit.grid_size, 80.0);
    }

    #[test]
    fn mode_tables_cover_settings() {
        let listed = |m: RenderModeSetting| RENDER_MODES.iter().filter(|(x, _)| *x == m).count();
        for m in [
            RenderModeSetting::Textured,
            RenderModeSetting::Unshaded,
            RenderModeSetting::Flat,
            RenderModeSetting::Wireframe,
            RenderModeSetting::TexturedWire,
            RenderModeSetting::Lit,
            RenderModeSetting::SmoothingGroups,
            RenderModeSetting::Baked,
            RenderModeSetting::LightmapGrid,
            RenderModeSetting::ColorCorrected,
        ] {
            match m {
                RenderModeSetting::Textured
                | RenderModeSetting::Unshaded
                | RenderModeSetting::Flat
                | RenderModeSetting::Wireframe
                | RenderModeSetting::TexturedWire
                | RenderModeSetting::Lit
                | RenderModeSetting::SmoothingGroups
                | RenderModeSetting::Baked
                | RenderModeSetting::LightmapGrid
                | RenderModeSetting::ColorCorrected => assert_eq!(listed(m), 1, "{m:?}"),
            }
        }
        let actions: std::collections::HashSet<_> = RENDER_MODES.iter().map(|(_, a)| *a).collect();
        assert_eq!(actions.len(), RENDER_MODES.len());
        assert_eq!(SELECT_MODES.len(), 3);
    }

    #[test]
    fn the_compile_options_are_not_in_the_toolbar() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let mut shapes = Vec::new();
        for frame in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
                time: Some(f64::from(frame) * 0.1),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| toolbar(ui, &mut state));
            });
            shapes = out.shapes;
        }
        for text in ["Put in GMod", "Mettre dans GMod", "Launch the game", "Lancer le jeu"] {
            assert!(find_text(&shapes, text).is_none(), "{text} is not in the toolbar");
        }
        assert!(find_text(&shapes, t!("shell.toolbar.compile")).is_some(), "the Compile button stays");
    }

    #[test]
    fn toolbar_reads_file_then_edit_then_the_rest() {
        use Action as A;
        let items: Vec<ToolbarItem> = TOOLBAR.iter().flat_map(|g| g.items.iter().copied()).collect();
        let pos = |item: ToolbarItem| items.iter().position(|i| *i == item).unwrap_or_else(|| panic!("{item:?}"));
        let expected = [
            ToolbarItem::Action(A::NewMap),
            ToolbarItem::Action(A::OpenMap),
            ToolbarItem::Action(A::SaveMap),
            ToolbarItem::Action(A::Undo),
            ToolbarItem::Action(A::Redo),
            ToolbarItem::Action(A::Cut),
            ToolbarItem::Action(A::Copy),
            ToolbarItem::Action(A::Paste),
            ToolbarItem::Action(A::ToggleGrid),
            ToolbarItem::Action(A::ToggleGrid3d),
            ToolbarItem::GridSize,
            ToolbarItem::Action(A::Carve),
            ToolbarItem::Action(A::Group),
            ToolbarItem::Action(A::Ungroup),
            ToolbarItem::Action(A::ToggleIgnoreGroups),
            ToolbarItem::Action(A::HideSelected),
            ToolbarItem::Action(A::HideUnselected),
            ToolbarItem::Action(A::UnhideAll),
            ToolbarItem::Action(A::ToggleCordon),
            ToolbarItem::Action(A::ToolCordon),
            ToolbarItem::Action(A::ToggleRadiusCulling),
            ToolbarItem::Action(A::ToggleDispWalkable),
            ToolbarItem::Action(A::ToggleNodraw),
            ToolbarItem::Action(A::ToggleCollisionOnly),
            ToolbarItem::Action(A::ToggleTextureLock),
            ToolbarItem::Action(A::ToggleTextureScaleLock),
            ToolbarItem::Action(A::ToggleHelpers),
            ToolbarItem::Action(A::ToggleModels),
            ToolbarItem::Action(A::ToggleToolTextures),
            ToolbarItem::RenderMode,
            ToolbarItem::Compile,
        ];
        let positions: Vec<usize> = expected.iter().map(|i| pos(*i)).collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{expected:?} at {positions:?}");
        let mut seen = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        for item in &items {
            assert!(seen.insert(*item), "{item:?} twice");
            assert!(ids.insert(item.id()), "{} twice", item.id());
            assert!(item_by_id(item.id()) == Some(*item));
            if let ToolbarItem::Action(a) = item {
                assert!(a.is_ready(), "{a:?}");
                assert!(!matches!(a, A::SelectModeGroups | A::SelectModeObjects | A::SelectModeSolids));
            }
        }
        for (i, group) in TOOLBAR.iter().enumerate() {
            assert_eq!(group_index(group.id), Some(i));
            assert!(ids.insert(group.id), "the group id {} is also a button id", group.id);
            let icons: Vec<String> = group
                .items
                .iter()
                .filter_map(|i| match i {
                    ToolbarItem::Action(a) => {
                        Some(button_icon(*a).map_or_else(|| button_glyph(*a).to_owned(), |i| format!("{i:?}")))
                    }
                    _ => None,
                })
                .collect();
            let unique: std::collections::HashSet<&String> = icons.iter().collect();
            assert_eq!(unique.len(), icons.len(), "{group:?}");
        }
        assert_eq!(TOOLBAR[COMPILE_GROUP].items, &[ToolbarItem::Compile]);
        assert_eq!(button_icon(A::Carve), Some(Icon::Carve), "Carve has a drawn icon, not a toolbox");
    }

    #[test]
    fn rows_hide_the_least_useful_groups_first() {
        let widths = [100.0; GROUPS];
        let order: Vec<usize> = (0..COMPILE_GROUP).collect();
        let plan = plan_rows(&order, &widths, &HIDE_ORDER, 8.0, 5000.0, 5000.0, 1, 28.0);
        assert_eq!(plan.rows, vec![order.clone()]);
        assert!(plan.hidden.is_empty());
        let full = 9.0 * 100.0 + 8.0 * 8.0;
        let plan = plan_rows(&order, &widths, &HIDE_ORDER, 8.0, full - 1.0, full - 1.0, 1, 28.0);
        assert_eq!(plan.hidden, vec![HIDE_ORDER[0]], "{plan:?}");
        assert_eq!(plan.rows.len(), 1);
        let plan = plan_rows(&order, &widths, &HIDE_ORDER, 8.0, 10.0, 10.0, 1, 28.0);
        assert_eq!(plan.rows, vec![vec![0]], "the file group is the last of the row to go: {plan:?}");
        let mut sorted = HIDE_ORDER;
        sorted.sort_unstable();
        assert_eq!(sorted, std::array::from_fn(|i| i), "every group is in the hide order once");
        assert_eq!(HIDE_ORDER[GROUPS - 1], COMPILE_GROUP, "the Compile button is the last to go");
    }

    #[test]
    fn two_rows_wrap_in_order_before_hiding() {
        let widths = [100.0; GROUPS];
        let order: Vec<usize> = (0..COMPILE_GROUP).collect();
        let plan = plan_rows(&order, &widths, &HIDE_ORDER, 8.0, 540.0, 650.0, 2, 28.0);
        assert!(plan.hidden.is_empty(), "{plan:?}");
        assert_eq!(plan.rows, vec![vec![0, 1, 2, 3, 4], vec![5, 6, 7, 8]]);
        let plan = plan_rows(&order, &widths, &HIDE_ORDER, 8.0, 320.0, 320.0, 2, 28.0);
        assert_eq!(plan.rows.len(), 2);
        assert!(!plan.hidden.is_empty() && plan.hidden.iter().all(|h| HIDE_ORDER[..5].contains(h)), "{plan:?}");
        let flat: Vec<usize> = plan.rows.concat();
        assert!(flat.windows(2).all(|w| w[0] < w[1]), "the groups keep their order: {plan:?}");
        let reordered = vec![8, 0, 1, 2, 3, 4, 5, 6, 7];
        let plan = plan_rows(&reordered, &widths, &HIDE_ORDER, 8.0, 5000.0, 5000.0, 2, 28.0);
        assert_eq!(plan.rows, vec![reordered], "a customized order is followed");
    }

    fn toolbar_frames(state: &mut AppState, size: egui::Vec2) -> (egui::Rect, Vec<(String, egui::Rect)>) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut row = egui::Rect::NOTHING;
        let mut tree = None;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                row = egui::Panel::top("bar").show(ui, |ui| toolbar(ui, state)).response.rect;
            });
            out.textures_delta.clear();
            tree = out.platform_output.accesskit_update;
        }
        let tree = tree.expect("an accessibility tree");
        let buttons = tree
            .nodes
            .iter()
            .map(|(_, n)| n)
            .filter(|n| n.role() == egui::Role::Button)
            .filter_map(|n| {
                let b = n.bounds()?;
                Some((
                    n.label().unwrap_or_default().to_owned(),
                    egui::Rect::from_min_max(
                        egui::pos2(b.x0 as f32, b.y0 as f32),
                        egui::pos2(b.x1 as f32, b.y1 as f32),
                    ),
                ))
            })
            .collect();
        (row, buttons)
    }

    #[test]
    fn compile_is_the_main_action_at_the_right_end() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        for width in [1280.0f32, 1600.0, 1920.0] {
            let mut state = hp_app::testing::test_state();
            let (row, buttons) = toolbar_frames(&mut state, egui::vec2(width, 800.0));
            let find = |label: &str| buttons.iter().find(|b| b.0 == label).map(|b| b.1);
            let compile = find(t!("shell.toolbar.compile")).unwrap_or_else(|| panic!("{width}: {buttons:?}"));
            assert!(row.right() - compile.right() < 16.0, "{width}: Compile {compile:?} at the right of {row:?}");
            for b in &buttons {
                if b.0 != t!("shell.toolbar.compile") && (b.1.center().y - compile.center().y).abs() < 4.0 {
                    assert!(b.1.right() <= compile.left() + 0.5, "{width}: {b:?} left of Compile {compile:?}");
                }
            }
            let undo = find(Action::Undo.label()).expect("Undo is shown");
            let save = find(Action::SaveMap.label()).expect("Save is shown");
            let copy = find(Action::Copy.label()).expect("Copy is shown");
            let carve = find(Action::Carve.label()).expect("Carve is shown");
            assert!(save.left() < undo.left() && undo.left() < copy.left() && copy.left() < carve.left());
            for b in buttons.iter().filter(|b| (b.1.center().y - undo.center().y).abs() < 4.0) {
                assert!((b.1.center().y - undo.center().y).abs() <= 0.5, "{width}: {b:?} is aligned on the row");
            }
        }
    }

    #[test]
    fn two_rows_keep_every_button_at_1280() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.settings.workflow.toolbar_rows = 2;
        let (row, buttons) = toolbar_frames(&mut state, egui::vec2(1280.0, 800.0));
        assert!(row.height() > BUTTON_SIZE.y * 1.8 && row.height() < BUTTON_SIZE.y * 3.0, "{row:?}");
        assert!(!buttons.iter().any(|b| b.0 == "»"), "no » button: {buttons:?}");
        for label in [Action::ToggleHelpers.label(), Action::SimulateRopes.label(), Action::ToggleCordon.label()] {
            assert!(buttons.iter().any(|b| b.0 == label), "{label} in {buttons:?}");
        }
        let (wide, _) = toolbar_frames(&mut state, egui::vec2(2400.0, 800.0));
        assert!(wide.height() < BUTTON_SIZE.y * 2.0, "a wide window stays on one row: {wide:?}");
    }

    #[test]
    fn customization_hides_and_reorders() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        set_hidden(&mut state, "cordon", true);
        set_hidden(&mut state, Action::Cut.id(), true);
        set_hidden(&mut state, "nonsense", true);
        assert_eq!(state.settings.workflow.toolbar_hidden, vec!["cordon".to_owned(), "cut".to_owned()]);
        assert!(state.settings_dirty);
        move_group(&mut state, group_index("render").unwrap(), -1);
        move_group(&mut state, COMPILE_GROUP, -1);
        move_group(&mut state, 0, -1);
        let order = group_order(&state.settings.workflow);
        assert_eq!(order.len(), GROUPS - 1);
        assert_eq!(order[..2], [0, 1]);
        assert_eq!(order[GROUPS - 3..], [8, 7], "render moved left of show");
        assert!(!shows_action(&state, Action::Cut) && shows_action(&state, Action::Copy));
        assert!(!shows_action(&state, Action::ToggleCordon));
        let (_, buttons) = toolbar_frames(&mut state, egui::vec2(2400.0, 800.0));
        let has = |label: &str| buttons.iter().any(|b| b.0 == label);
        assert!(!has(Action::Cut.label()) && has(Action::Copy.label()) && !has(Action::ToolCordon.label()));
        let x = |label: &str| buttons.iter().find(|b| b.0 == label).map(|b| b.1.left()).unwrap();
        assert!(x(Action::CheckProblems.label()) < x(Action::ToggleHelpers.label()), "the order is followed");
        set_hidden(&mut state, "compile", true);
        let (_, buttons) = toolbar_frames(&mut state, egui::vec2(1600.0, 800.0));
        assert!(!buttons.iter().any(|b| b.0 == t!("shell.toolbar.compile")));
        state.settings.workflow.reset_toolbar();
        assert_eq!(group_order(&state.settings.workflow), (0..COMPILE_GROUP).collect::<Vec<_>>());
        state.settings.workflow.toolbar_order = vec!["show".into(), "show".into(), "bogus".into(), "compile".into()];
        let order = group_order(&state.settings.workflow);
        assert_eq!(order[0], 7);
        assert_eq!(order.len(), GROUPS - 1, "duplicates and unknown ids are ignored");
        for g in &TOOLBAR {
            assert!(!g.label().is_empty() && g.items.iter().all(|i| !i.label().is_empty()));
        }
    }

    fn right_click_menu(at: impl Fn(&[(String, egui::Rect)]) -> egui::Pos2) -> Vec<egui::epaint::ClippedShape> {
        let mut state = hp_app::testing::test_state();
        let size = egui::vec2(1600.0, 800.0);
        let (_, buttons) = toolbar_frames(&mut state, size);
        let pos = at(&buttons);
        let ctx = egui::Context::default();
        let mut shapes = Vec::new();
        for frame in 0..5 {
            let events = match frame {
                2 => [true, false]
                    .map(|pressed| egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .into(),
                0 => vec![],
                _ => vec![egui::Event::PointerMoved(pos)],
            };
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Panel::top("bar").show(ui, |ui| toolbar(ui, &mut state));
            });
            out.textures_delta.clear();
            shapes = out.shapes;
        }
        assert!(state.requested_actions.is_empty(), "a right click runs nothing: {:?}", state.requested_actions);
        shapes
    }

    #[test]
    fn right_click_opens_the_customize_menu() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let on_undo = right_click_menu(|buttons| {
            buttons.iter().find(|b| b.0 == Action::Undo.label()).expect("Undo is shown").1.center()
        });
        let in_gap = right_click_menu(|buttons| {
            let compile = buttons.iter().find(|b| b.0 == t!("shell.toolbar.compile")).expect("Compile").1;
            let last = buttons
                .iter()
                .filter(|b| b.1.right() <= compile.left() && (b.1.center().y - compile.center().y).abs() < 4.0)
                .map(|b| b.1.right())
                .fold(0.0f32, f32::max);
            assert!(compile.left() - last > 20.0, "room between the groups and Compile at 1600: {last} {compile:?}");
            egui::pos2((last + compile.left()) * 0.5, compile.center().y)
        });
        for shapes in [on_undo, in_gap] {
            for text in [t!("shell.toolbar.two_rows"), t!("shell.toolbar.customize"), t!("action.reset_toolbar")] {
                assert!(find_text(&shapes, text).is_some(), "{text} in the menu");
            }
            for g in &TOOLBAR {
                assert!(find_text(&shapes, g.label()).is_some(), "{} in the menu", g.label());
            }
        }
    }

    #[test]
    fn toolbar_never_wraps_and_keeps_every_button() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let items: [Vec<ToolbarItem>; GROUPS] = std::array::from_fn(|i| TOOLBAR[i].items.to_vec());
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            for width in [640.0f32, 900.0, 1024.0, 1280.0, 1600.0, 2400.0] {
                let ctx = egui::Context::default();
                let mut row = egui::Rect::NOTHING;
                let mut overflow = false;
                for frame in 0..4 {
                    if frame == 3 {
                        egui::Popup::open_id(&ctx, toolbar_overflow_id());
                    }
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 600.0))),
                        ..Default::default()
                    };
                    let mut out = ctx.run_ui(input, |ui| {
                        let r = egui::Panel::top("bar").show(ui, |ui| toolbar(ui, &mut state));
                        row = r.response.rect;
                    });
                    out.textures_delta.clear();
                    overflow = egui::Popup::is_id_open(&ctx, toolbar_overflow_id());
                }
                assert!(row.height() < BUTTON_SIZE.y * 2.0, "{lang:?} {width}: the toolbar wraps ({row:?})");
                let memo: ToolbarWidths = ctx.data(|d| d.get_temp(toolbar_widths_id())).expect("widths are remembered");
                let measured = memo.groups.iter().filter(|g| g.is_some()).count();
                let w = memo.widths(&items);
                let total: f32 = w.iter().sum::<f32>() + memo.separator.unwrap_or(0.0) * (GROUPS - 1) as f32;
                if total <= width - 20.0 {
                    assert!(!overflow || measured == GROUPS, "{lang:?} {width}");
                } else {
                    assert!(overflow, "{lang:?} {width}: the hidden buttons are in the overflow menu");
                }
                assert_eq!(measured, GROUPS, "{lang:?} {width}: every group was drawn");
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }

    #[test]
    fn selection_bar_switches_the_mode() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let mut shapes = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
                ..Default::default()
            };
            shapes = ctx.run_ui(input, |ui| selection_bar(ui, &mut state)).shapes;
        }
        for text in ["Groups", "Objects", "Solids", "To world", "To entity"] {
            assert!(find_text(&shapes, text).is_some(), "{text}");
        }
    }

    fn find_text(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<egui::Rect> {
        shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == text => Some(t.visual_bounding_rect()),
            _ => None,
        })
    }
}
