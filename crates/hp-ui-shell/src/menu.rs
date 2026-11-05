use egui::{AtomExt, KeyboardShortcut, Response, RichText, Ui};
use hp_app::actions::{Category, MenuGroup, shortcut_label};
use hp_app::extend::MenuExtension;
use hp_app::i18n::{Lang, TrTable, set_lang};
use hp_app::state::RightTab;
use hp_app::{Action, AppState, t, tf};

use crate::common::{Check, accent_text, action_check, action_enabled, close_on_escape};

const MENU_MIN_WIDTH: f32 = 230.0;

pub const DIRTY_MARKER: &str = "\u{23FA}";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Recent,
    Language,
    FirstRun,
}

impl Builtin {
    const ALL: [Builtin; 3] = [Builtin::Recent, Builtin::Language, Builtin::FirstRun];

    fn slot(self) -> (Category, u16, u16) {
        match self {
            Builtin::Recent => (Category::File, 80, 0),
            Builtin::Language => (Category::View, 50, 10),
            Builtin::FirstRun => (Category::Help, 10, 10),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuItem {
    Separator,
    Action(Action),
    Submenu { title: &'static str, width: f32, actions: Vec<Action> },
    Builtin(Builtin),
    Extension(usize),
}

const PANEL_ORDER: [Action; 6] = [
    Action::ShowTextureBrowser,
    Action::ShowEntityProperties,
    Action::ShowVisgroups,
    Action::ShowOutliner,
    Action::ShowModelBrowser,
    Action::ShowConsole,
];

fn hammer_order(group: MenuGroup) -> &'static [Action] {
    use Action as A;
    match group {
        MenuGroup::ViewPanels => &PANEL_ORDER,
        MenuGroup::MapGrid => &[A::ToggleSnap, A::ToggleGrid, A::ToggleGrid3d, A::GridSmaller, A::GridLarger],
        MenuGroup::MapInfo => &[A::GoToBrush, A::EntityReport, A::EntityGallery, A::CheckProblems, A::CompareMaps],
        MenuGroup::MapProperties => &[A::MapInfo, A::MapProperties],
        MenuGroup::ToolsBrush => &[
            A::Carve,
            A::MakeHollow,
            A::Group,
            A::Ungroup,
            A::TieToEntity,
            A::MoveToWorld,
            A::ReplaceTextures,
            A::ToggleTextureLock,
            A::ToggleTextureScaleLock,
            A::TransformDialog,
            A::SnapToGrid,
            A::SnapToGridIndividually,
            A::CenterOrigins,
            A::RandomRotate,
            A::RandomRotateDialog,
        ],
        MenuGroup::ToolsPlacement => &[A::CreatePrefab],
        MenuGroup::ToolsSwitch => &TOOLS_MENU_ORDER,
        _ => &[],
    }
}

const TOOLS_MENU_ORDER: [Action; 21] = [
    Action::ToolSelect,
    Action::ToolCamera,
    Action::ToolEntity,
    Action::ToolBlock,
    Action::ToolTexture,
    Action::ApplyCurrentTexture,
    Action::ToolDecal,
    Action::ToolOverlay,
    Action::ToolClip,
    Action::ToolSlice,
    Action::ToolExtrude,
    Action::ToolBevel,
    Action::ToolVertex,
    Action::ToolPath,
    Action::ToolPolygon,
    Action::ToolCurve,
    Action::ToolSprinkle,
    Action::ToolDisplacement,
    Action::ToolCordon,
    Action::ToolRoad,
    Action::ToolPipe,
];

fn group_actions(cat: Category, group: MenuGroup) -> Vec<Action> {
    let mut actions: Vec<Action> =
        Action::ALL.iter().copied().filter(|a| a.category() == cat && a.group() == group).collect();
    let order = hammer_order(group);
    actions.sort_by_key(|a| order.iter().position(|p| p == a).unwrap_or(order.len()));
    actions
}

pub fn menu_layout(cat: Category, extensions: &[MenuExtension]) -> Vec<MenuItem> {
    let mut slots: Vec<(u16, u16, MenuItem)> = Vec::new();
    let mut seen: Vec<MenuGroup> = Vec::new();
    for a in Action::ALL.iter().filter(|a| a.category() == cat) {
        let group = a.group();
        if seen.contains(&group) {
            continue;
        }
        seen.push(group);
        let Some(info) = group.info() else { continue };
        let actions = group_actions(cat, group);
        match info.submenu {
            Some(title) => {
                slots.push((info.section, info.order, MenuItem::Submenu { title, width: info.width, actions }));
            }
            None => slots.extend(actions.into_iter().map(|a| (info.section, info.order, MenuItem::Action(a)))),
        }
    }
    for b in Builtin::ALL {
        let (menu, section, order) = b.slot();
        if menu == cat {
            slots.push((section, order, MenuItem::Builtin(b)));
        }
    }
    for (i, ext) in extensions.iter().enumerate() {
        if ext.menu == cat {
            slots.push((ext.section, ext.order, MenuItem::Extension(i)));
        }
    }
    slots.sort_by_key(|(section, order, _)| (*section, *order));
    let mut out = Vec::with_capacity(slots.len() + 4);
    let mut last_section = None;
    for (section, _, item) in slots {
        if last_section.is_some_and(|s| s != section) {
            out.push(MenuItem::Separator);
        }
        last_section = Some(section);
        out.push(item);
    }
    out
}

fn check_cell<'a>(ui: &Ui, check: Check) -> egui::Atom<'a> {
    let side = ui.text_style_height(&egui::TextStyle::Button);
    let glyph = match check {
        Check::Toggle(true) => "\u{2714}",
        Check::Radio(true) => "\u{23FA}",
        Check::None | Check::Toggle(false) | Check::Radio(false) => {
            return egui::Atom::paint(egui::vec2(side, side), |_, _| {});
        }
    };
    RichText::new(glyph).color(accent_text(ui.visuals())).atom_size(egui::vec2(side, side))
}

pub fn submenu<R>(ui: &mut Ui, label: &str, add_contents: impl FnOnce(&mut Ui) -> R) {
    let cell = check_cell(ui, Check::None);
    ui.menu_button((cell, label.to_owned()), add_contents);
}

pub fn undo_step_text(label: &str) -> String {
    let key = undo_key(label);
    if hp_app::i18n::has_key(&key) { hp_app::i18n::tr(&key).to_owned() } else { label.to_owned() }
}

pub fn undo_key(label: &str) -> String {
    let mut key = String::with_capacity(label.len() + 11);
    key.push_str("shell.undo.");
    for c in label.chars() {
        if c.is_ascii_alphanumeric() {
            key.push(c.to_ascii_lowercase());
        } else if !key.ends_with('_') {
            key.push('_');
        }
    }
    let len = key.trim_end_matches('_').len();
    key.truncate(len);
    key
}

pub fn menu_label(state: &AppState, action: Action) -> String {
    if state.tool == hp_app::ToolId::Road && state.runtime.brush.road_history.is_some() {
        match action {
            Action::Undo => return tf!("shell.undo_named", undo_step_text("Edit road")),
            Action::Redo => return tf!("shell.redo_named", undo_step_text("Edit road")),
            _ => {}
        }
    }
    match action {
        Action::Undo => match state.doc.undo_label() {
            Some(l) => tf!("shell.undo_named", undo_step_text(l)),
            None => action.label().to_owned(),
        },
        Action::Redo => match state.doc.redo_label() {
            Some(l) => tf!("shell.redo_named", undo_step_text(l)),
            None => action.label().to_owned(),
        },
        _ => action.label().to_owned(),
    }
}

pub fn menu_entry(
    ui: &mut Ui,
    label: &str,
    shortcut: Option<KeyboardShortcut>,
    enabled: bool,
    check: Check,
) -> Response {
    let mut button = egui::Button::new((check_cell(ui, check), label.to_owned()));
    if let Some(sc) = shortcut {
        button = button.shortcut_text(shortcut_label(&sc));
    }
    ui.add_enabled(enabled, button)
}

fn action_item(ui: &mut Ui, state: &mut AppState, action: Action) {
    if PANEL_ORDER.contains(&action) {
        panel_toggle(ui, state, action);
        return;
    }
    let label = menu_label(state, action);
    let enabled = action_enabled(state, action);
    let check = action_check(state, action);
    if menu_entry(ui, &label, state.keys.shortcut(action), enabled, check).clicked() {
        state.request(action);
    }
}

pub fn menu_bar(ui: &mut Ui, state: &mut AppState) {
    menu_bar_with(ui, state, &mut []);
}

fn menus(ui: &mut Ui, state: &mut AppState, extensions: &mut [MenuExtension]) -> f32 {
    hp_app::widgets::logo(ui, 18.0).on_hover_text(t!("app.name"));
    ui.add_space(2.0);
    for cat in Category::ALL {
        ui.menu_button(t!(cat.label_key()), |ui| {
            category_menu(ui, state, cat, extensions);
            close_on_escape(ui);
        });
    }
    mcp_menu(ui, state);
    ui.cursor().left()
}

pub fn title_menus_with(ui: &mut Ui, state: &mut AppState, extensions: &mut [MenuExtension]) -> f32 {
    egui::MenuBar::new().ui(ui, |ui| menus(ui, state, extensions)).inner
}

pub fn menu_bar_with(ui: &mut Ui, state: &mut AppState, extensions: &mut [MenuExtension]) {
    egui::MenuBar::new().ui(ui, |ui| {
        menus(ui, state, extensions);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let dirty = state.doc.is_dirty();
            let title = state.map_title();
            let text = if dirty { format!("{DIRTY_MARKER} {title}") } else { title };
            let mut rich = RichText::new(text).small();
            rich =
                if dirty { rich.color(accent_text(ui.visuals())) } else { rich.color(ui.visuals().weak_text_color()) };
            let r = ui.label(rich);
            let tip = match state.doc.path() {
                Some(p) => p.display().to_string(),
                None => t!("shell.menu.not_saved_yet").to_owned(),
            };
            let tip = if dirty { format!("{tip}\n{}", t!("shell.menu.unsaved_changes")) } else { tip };
            r.on_hover_text(tip);
        });
    });
}

fn mcp_menu(ui: &mut Ui, state: &mut AppState) {
    let on = state.runtime.mcp.running;
    let title = if on { "MCP ✔" } else { "MCP" };
    ui.menu_button(title, |ui| {
        ui.set_min_width(MENU_MIN_WIDTH);
        let mut running = on;
        if ui.checkbox(&mut running, hp_app::Action::ToggleMcpServer.label()).clicked() {
            state.request(hp_app::Action::ToggleMcpServer);
            ui.close();
        }
        let mcp = &state.runtime.mcp;
        let status = match (mcp.running, mcp.port) {
            (true, Some(port)) => {
                format!("{} - {port} - {}", t!("shell.mcp.pill_on"), crate::status::mcp::clients_text(mcp.clients))
            }
            (true, None) => t!("shell.mcp.pill_on").to_owned(),
            (false, _) => t!("shell.mcp.pill_off").to_owned(),
        };
        ui.label(RichText::new(status).small().color(ui.visuals().weak_text_color()));
        ui.separator();
        if ui.button(format!("⚙  {}", t!("shell.mcp.more"))).clicked() {
            state.request(hp_app::Action::McpPanel);
            ui.close();
        }
        close_on_escape(ui);
    })
    .response
    .on_hover_text(crate::status::mcp::tooltip(state));
}

fn category_menu(ui: &mut Ui, state: &mut AppState, cat: Category, extensions: &mut [MenuExtension]) {
    ui.set_min_width(MENU_MIN_WIDTH);
    let items = menu_layout(cat, extensions);
    let avail = (ui.ctx().content_rect().bottom() - ui.min_rect().top() - MENU_BOTTOM_MARGIN).max(120.0);
    let row_h = (ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y)
        .max(ui.spacing().interact_size.y)
        + ui.spacing().item_spacing.y;
    let sep_h = 6.0 + ui.spacing().item_spacing.y;
    let mut columns = menu_columns(items, row_h, sep_h, avail);
    if columns.len() <= 1 {
        for item in columns.pop().unwrap_or_default() {
            menu_item(ui, state, item, extensions);
        }
        return;
    }
    ui.horizontal_top(|ui| {
        for (k, column) in columns.into_iter().enumerate() {
            if k > 0 {
                ui.separator();
            }
            let width_id = ui.scope_id().with(("hp_menu_column_width", cat.label_key(), k));
            let width = ui.data(|d| d.get_temp::<f32>(width_id)).unwrap_or(MENU_MIN_WIDTH);
            let used = ui
                .allocate_ui_with_layout(
                    egui::vec2(width, avail),
                    egui::Layout::top_down_justified(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(width);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        for item in column {
                            menu_item(ui, state, item, extensions);
                        }
                    },
                )
                .response
                .rect
                .width();
            if (used - width).abs() > 0.5 {
                ui.data_mut(|d| d.insert_temp(width_id, used.max(MENU_MIN_WIDTH)));
                ui.ctx().request_repaint();
            }
        }
    });
}

const MENU_BOTTOM_MARGIN: f32 = 16.0;

fn menu_columns(items: Vec<MenuItem>, row_h: f32, sep_h: f32, avail: f32) -> Vec<Vec<MenuItem>> {
    let mut sections: Vec<Vec<MenuItem>> = vec![Vec::new()];
    for item in items {
        match item {
            MenuItem::Separator => {
                if sections.last().is_some_and(|s| !s.is_empty()) {
                    sections.push(Vec::new());
                }
            }
            item => sections.last_mut().expect("one section").push(item),
        }
    }
    sections.retain(|s| !s.is_empty());
    let height = |rows: usize| rows as f32 * row_h;
    let total: f32 =
        sections.iter().map(|s| height(s.len())).sum::<f32>() + sep_h * sections.len().saturating_sub(1) as f32;
    let join = |sections: Vec<Vec<MenuItem>>| {
        let mut out = Vec::new();
        for (i, s) in sections.into_iter().enumerate() {
            if i > 0 {
                out.push(MenuItem::Separator);
            }
            out.extend(s);
        }
        out
    };
    if total <= avail {
        return vec![join(sections)];
    }
    let mut columns: Vec<Vec<MenuItem>> = Vec::new();
    let mut current: Vec<MenuItem> = Vec::new();
    let mut current_h = 0.0f32;
    for section in sections {
        let section_h = height(section.len());
        let gap = if current.is_empty() { 0.0 } else { sep_h };
        if current_h + gap + section_h <= avail {
            if !current.is_empty() {
                current.push(MenuItem::Separator);
            }
            current.extend(section);
            current_h += gap + section_h;
        } else if section_h <= avail {
            columns.push(std::mem::take(&mut current));
            current.extend(section);
            current_h = section_h;
        } else {
            for (i, item) in section.into_iter().enumerate() {
                let gap = if i == 0 && !current.is_empty() { sep_h } else { 0.0 };
                if !current.is_empty() && current_h + gap + row_h > avail {
                    columns.push(std::mem::take(&mut current));
                    current_h = 0.0;
                } else if gap > 0.0 {
                    current.push(MenuItem::Separator);
                    current_h += gap;
                }
                current.push(item);
                current_h += row_h;
            }
        }
    }
    if !current.is_empty() {
        columns.push(current);
    }
    columns
}

fn menu_item(ui: &mut Ui, state: &mut AppState, item: MenuItem, extensions: &mut [MenuExtension]) {
    {
        match item {
            MenuItem::Separator => {
                ui.separator();
            }
            MenuItem::Action(a) => action_item(ui, state, a),
            MenuItem::Submenu { title, width, actions } => submenu(ui, t!(title), |ui| {
                ui.set_min_width(width);
                for a in actions {
                    action_item(ui, state, a);
                }
            }),
            MenuItem::Builtin(Builtin::Recent) => recent_menu(ui, state),
            MenuItem::Builtin(Builtin::Language) => submenu(ui, t!("menu.language"), |ui| {
                ui.set_min_width(140.0);
                language_items(ui, state);
            }),
            MenuItem::Builtin(Builtin::FirstRun) => {
                if menu_entry(ui, t!("shell.menu.first_run"), None, true, Check::None).clicked() {
                    state.ui.show_first_run = true;
                }
            }
            MenuItem::Extension(i) => (extensions[i].ui)(ui, state),
        }
    }
}

fn recent_menu(ui: &mut Ui, state: &mut AppState) {
    submenu(ui, t!("menu.recent"), |ui| {
        ui.set_min_width(MENU_MIN_WIDTH);
        if state.settings.recent_files.is_empty() {
            menu_entry(ui, t!("menu.recent_empty"), None, false, Check::None);
            return;
        }
        let mut open = None;
        for (i, path) in state.settings.recent_files.iter().enumerate() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let label = format!("{}  {name}", i + 1);
            let r = menu_entry(ui, &label, None, true, Check::None);
            if r.on_hover_text(path.display().to_string()).clicked() {
                open = Some(path.clone());
            }
        }
        if let Some(path) = open {
            state.requested_open = Some(path);
        }
        ui.separator();
        if menu_entry(ui, t!("shell.menu.clear_recent"), None, true, Check::None).clicked() {
            state.settings.recent_files.clear();
            state.settings_dirty = true;
        }
    });
}

fn panel_toggle(ui: &mut Ui, state: &mut AppState, action: Action) {
    let on = match action_check(state, action) {
        Check::Toggle(on) => on,
        _ => false,
    };
    if menu_entry(ui, action.label(), state.keys.shortcut(action), true, Check::Toggle(on)).clicked() {
        let ui_settings = &mut state.settings.ui;
        let (flag, tab) = match action {
            Action::ShowTextureBrowser => (&mut ui_settings.show_texture_browser, Some(RightTab::Textures)),
            Action::ShowEntityProperties => (&mut ui_settings.show_entity_properties, Some(RightTab::Entity)),
            Action::ShowVisgroups => (&mut ui_settings.show_visgroups, Some(RightTab::Visgroups)),
            Action::ShowOutliner => (&mut ui_settings.show_outliner, Some(RightTab::Outliner)),
            Action::ShowModelBrowser => (&mut ui_settings.show_model_browser, Some(RightTab::Models)),
            _ => (&mut ui_settings.show_console, None),
        };
        *flag = !*flag;
        if *flag && let Some(tab) = tab {
            state.ui.right_tab = tab;
        }
        state.settings_dirty = true;
    }
}

pub fn language_items(ui: &mut Ui, state: &mut AppState) {
    let current = hp_app::i18n::lang();
    for lang in Lang::ALL {
        let on = current == lang;
        if menu_entry(ui, lang.native_name(), None, true, Check::Radio(on)).clicked() && !on {
            switch_language(state, lang);
        }
    }
}

pub fn switch_language(state: &mut AppState, lang: Lang) {
    state.settings.language = lang;
    state.settings.language_chosen = true;
    set_lang(lang);
    state.settings_dirty = true;
}

pub fn menu_actions() -> Vec<Action> {
    let mut out = Vec::new();
    for cat in Category::ALL {
        for item in menu_layout(cat, &[]) {
            match item {
                MenuItem::Action(a) => out.push(a),
                MenuItem::Submenu { actions, .. } => out.extend(actions),
                _ => {}
            }
        }
    }
    out
}

pub(crate) const TR: TrTable = &[
    ("shell.menu.clear_recent", "Clear recent maps", "Vider la liste des maps récentes"),
    ("shell.menu.first_run", "Setup assistant...", "Assistant de configuration..."),
    ("shell.menu.not_saved_yet", "Not saved yet", "Pas encore enregistrée"),
    ("shell.menu.unsaved_changes", "Unsaved changes", "Modifications non enregistrées"),
    ("shell.undo_named", "Undo {0}", "Annuler : {0}"),
    ("shell.redo_named", "Redo {0}", "Rétablir : {0}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_list_each_entry_once() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        let listed = menu_actions();
        let mut seen = std::collections::HashSet::new();
        for a in &listed {
            assert!(seen.insert(*a), "{} is in the menus twice", a.id());
        }
        for lang in [Lang::En, Lang::Fr] {
            set_lang(lang);
            for cat in Category::ALL {
                let mut labels: Vec<(String, Vec<String>)> = vec![(cat.label_key().to_owned(), Vec::new())];
                for item in menu_layout(cat, &[]) {
                    match item {
                        MenuItem::Action(a) => labels[0].1.push(a.label().to_owned()),
                        MenuItem::Submenu { title, actions, .. } => {
                            labels[0].1.push(hp_app::i18n::tr(title).to_owned());
                            labels.push((title.to_owned(), actions.iter().map(|a| a.label().to_owned()).collect()));
                        }
                        _ => {}
                    }
                }
                for (menu, entries) in labels {
                    let mut seen = std::collections::HashSet::new();
                    for e in &entries {
                        assert!(seen.insert(e.to_lowercase()), "{lang:?}: \"{e}\" twice in {menu}");
                    }
                }
            }
        }
        set_lang(Lang::En);
    }

    #[test]
    fn every_action_is_in_a_menu() {
        let listed = menu_actions();
        for a in Action::ALL.iter().filter(|a| a.group() != MenuGroup::Hidden) {
            assert!(listed.contains(a), "{} is missing from the menus", a.id());
        }
    }

    #[test]
    fn menus_follow_hammer() {
        use Action as A;
        use MenuItem::{Action as I, Builtin as B, Separator as S};
        let about = Action::DECLARED.iter().position(|a| *a == Action::About).unwrap_or(90);
        let core: Vec<Action> = Action::DECLARED[..=about].to_vec();
        assert_eq!(core.last(), Some(&Action::About));
        let only_core = |items: Vec<MenuItem>| -> Vec<MenuItem> {
            let mut out: Vec<MenuItem> = Vec::new();
            for item in items {
                let item = match item {
                    MenuItem::Action(a) if !core.contains(&a) => continue,
                    MenuItem::Submenu { title, width, actions } => {
                        let actions: Vec<Action> = actions.into_iter().filter(|a| core.contains(a)).collect();
                        if actions.is_empty() {
                            continue;
                        }
                        MenuItem::Submenu { title, width, actions }
                    }
                    other => other,
                };
                if item == S && matches!(out.last(), None | Some(S)) {
                    continue;
                }
                out.push(item);
            }
            if out.last() == Some(&S) {
                out.pop();
            }
            out
        };
        let sub = |title: &'static str, width: f32, actions: &[Action]| MenuItem::Submenu {
            title,
            width,
            actions: actions.to_vec(),
        };
        let layout = |cat| only_core(menu_layout(cat, &[]));
        assert_eq!(
            layout(Category::File),
            vec![
                I(A::NewMap),
                I(A::OpenMap),
                S,
                I(A::SaveMap),
                I(A::SaveMapAs),
                S,
                I(A::ImportMap),
                I(A::ExportSelection),
                S,
                I(A::RunMap),
                S,
                B(Builtin::Recent),
                S,
                I(A::Exit),
            ]
        );
        assert_eq!(
            layout(Category::Edit),
            vec![
                I(A::Undo),
                I(A::Redo),
                S,
                I(A::FindEntities),
                S,
                I(A::Cut),
                I(A::Copy),
                I(A::Paste),
                I(A::PasteSpecial),
                I(A::Delete),
                I(A::Duplicate),
                I(A::ArrayDuplicate),
                S,
                I(A::SelectAll),
                I(A::SelectNone),
                I(A::InvertSelection),
                sub("menu.selection_mode", 160.0, &[A::SelectModeGroups, A::SelectModeObjects, A::SelectModeSolids]),
                sub(
                    "menu.select_element",
                    160.0,
                    &[A::SelectElementObject, A::SelectElementVertex, A::SelectElementEdge, A::SelectElementFace],
                ),
                S,
                I(A::Properties),
            ]
        );
        assert_eq!(
            layout(Category::Map),
            vec![
                I(A::ToggleSnap),
                I(A::ToggleGrid),
                I(A::GridSmaller),
                I(A::GridLarger),
                S,
                I(A::GoToBrush),
                I(A::EntityReport),
                I(A::CheckProblems),
                S,
                I(A::LoadPointfile),
                I(A::UnloadPointfile),
                S,
                I(A::MapInfo),
                I(A::MapProperties),
            ]
        );
        assert_eq!(
            layout(Category::View),
            vec![
                sub(
                    "menu.render_mode",
                    200.0,
                    &[A::RenderTextured, A::RenderFlat, A::RenderWireframe, A::RenderTexturedWire, A::RenderLit]
                ),
                S,
                I(A::ToggleModels),
                I(A::ToggleHelpers),
                I(A::ToggleToolTextures),
                S,
                I(A::HideSelected),
                I(A::HideUnselected),
                I(A::UnhideAll),
                S,
                I(A::Center2dOnSelection),
                I(A::Center3dOnSelection),
                I(A::MaximizeView),
                I(A::ToggleFullscreen),
                S,
                sub(
                    "menu.panels",
                    200.0,
                    &[
                        A::ShowTextureBrowser,
                        A::ShowEntityProperties,
                        A::ShowVisgroups,
                        A::ShowOutliner,
                        A::ShowModelBrowser
                    ]
                ),
                B(Builtin::Language),
            ]
        );
        assert_eq!(
            layout(Category::Tools),
            vec![
                I(A::Carve),
                I(A::MakeHollow),
                I(A::Group),
                I(A::Ungroup),
                I(A::TieToEntity),
                I(A::MoveToWorld),
                I(A::ReplaceTextures),
                I(A::ToggleTextureLock),
                I(A::ToggleTextureScaleLock),
                I(A::TransformDialog),
                I(A::SnapToGrid),
                I(A::MergeConvex),
                sub("menu.align", 160.0, &[A::AlignLeft, A::AlignRight, A::AlignTop, A::AlignBottom]),
                sub("menu.flip", 140.0, &[A::FlipX, A::FlipY, A::FlipZ]),
                S,
                I(A::ToolSelect),
                I(A::ToolCamera),
                I(A::ToolEntity),
                I(A::ToolBlock),
                I(A::ToolTexture),
                I(A::ApplyCurrentTexture),
                I(A::ToolDecal),
                I(A::ToolOverlay),
                I(A::ToolClip),
                I(A::ToolVertex),
                I(A::ToolDisplacement),
                S,
                I(A::CommandPalette),
                I(A::Settings),
            ]
        );
        assert_eq!(layout(Category::Window), vec![I(A::ShowConsole)]);
        assert_eq!(
            layout(Category::Help),
            vec![I(A::KeyboardShortcuts), I(A::OpenDocs), I(A::ReportIssue), B(Builtin::FirstRun), S, I(A::About)]
        );
        let tools: Vec<Action> = menu_layout(Category::Tools, &[])
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Action(a) if a.tool().is_some() || a == A::ApplyCurrentTexture => Some(a),
                _ => None,
            })
            .collect();
        let mut palette: Vec<Action> = hp_app::ToolId::ALL.iter().map(|t| t.action()).collect();
        palette.insert(5, A::ApplyCurrentTexture);
        assert_eq!(palette[4], A::ToolTexture);
        assert_eq!(tools, palette);
    }

    #[test]
    fn extensions_are_placed_by_section() {
        let ext = |section, order| MenuExtension::new(Category::Help, section, order, |_, _| {});
        let exts = [ext(10, 5), ext(95, 0)];
        let layout = menu_layout(Category::Help, &exts);
        let first_run = layout.iter().position(|i| *i == MenuItem::Builtin(Builtin::FirstRun)).unwrap();
        let ext0 = layout.iter().position(|i| *i == MenuItem::Extension(0)).unwrap();
        assert!(ext0 < first_run, "{layout:?}");
        assert_eq!(layout.last(), Some(&MenuItem::Extension(1)));
        assert_eq!(layout[layout.len() - 2], MenuItem::Separator);
        let actions = menu_actions();
        assert!(Action::DECLARED.iter().filter(|a| !a.is_ready()).all(|a| !actions.contains(a)));
        assert!(actions.contains(&Action::CloseMap), "Close came with the map tabs");
    }

    #[test]
    fn menus_render_with_extensions() {
        let mut state = hp_app::testing::test_state();
        let mut exts = vec![MenuExtension::new(Category::View, 40, 30, |ui, _| {
            ui.label("x");
        })];
        hp_app::testing::run_ui(&mut state, |ui, state| menu_bar_with(ui, state, &mut exts));
    }

    #[test]
    fn undo_labels_translate() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        hp_app::i18n::set_lang(Lang::En);
        assert_eq!(undo_step_text("Move"), "Move");
        assert_eq!(undo_step_text("Some custom step"), "Some custom step");
        assert_eq!(hp_app::i18n::tr_in("shell.undo.move", Lang::Fr), "Déplacement");
        assert_eq!(hp_app::i18n::tr_in("shell.undo.snap_to_grid", Lang::Fr), "Alignement sur la grille");
        assert_eq!(undo_key("Nudge vertices"), "shell.undo.nudge_vertices");
        assert_eq!(undo_key("Edit map properties "), "shell.undo.edit_map_properties");
    }

    #[test]
    fn road_point_history_controls_menu_labels_and_availability() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        hp_app::i18n::set_lang(Lang::En);
        let mut state = hp_app::testing::test_state();
        state.tool = hp_app::ToolId::Road;
        state.runtime.brush.road_history = Some((true, false));
        assert!(Action::Undo.enabled(&state));
        assert!(!Action::Redo.enabled(&state));
        assert_eq!(menu_label(&state, Action::Undo), tf!("shell.undo_named", "Edit road"));
        state.runtime.brush.road_history = Some((false, true));
        assert!(!Action::Undo.enabled(&state));
        assert!(Action::Redo.enabled(&state));
        state.tool = hp_app::ToolId::Select;
        assert_eq!(Action::Undo.enabled(&state), state.doc.can_undo());
    }

    const UNDO_CALLS: [&str; 8] = [
        ".begin(",
        ".begin_merge(",
        ".tx(",
        ".transaction(",
        ".edit_sides(",
        ".transform_impl(",
        ".apply_moves(",
        " in_tx(",
    ];

    fn first_literal(args: &str) -> Option<&str> {
        let mut depth = 0usize;
        for (i, c) in args.char_indices() {
            match c {
                '"' => {
                    let rest = &args[i + 1..];
                    return rest.find('"').map(|end| &rest[..end]);
                }
                '(' | '[' => depth += 1,
                ')' | ']' if depth == 0 => return None,
                ')' | ']' => depth -= 1,
                '|' | ';' | '{' => return None,
                _ => {}
            }
        }
        None
    }

    fn labels_in_source(text: &str) -> Vec<String> {
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        let mut out = Vec::new();
        for line in code.lines() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if t.contains("const ")
                && (t.contains("UNDO_") || t.contains("LABEL_"))
                && let Some(pos) = t.find(": &str = \"")
            {
                let rest = &t[pos + 10..];
                if let Some(end) = rest.find('"') {
                    out.push(rest[..end].to_owned());
                }
            }
        }
        for call in UNDO_CALLS {
            let mut rest = code;
            while let Some(pos) = rest.find(call) {
                let args = &rest[pos + call.len()..];
                if let Some(label) = first_literal(args)
                    && label.starts_with(|c: char| c.is_ascii_uppercase())
                {
                    out.push(label.to_owned());
                }
                rest = args;
            }
        }
        out
    }

    fn rust_sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "tests" && name != "target" {
                    rust_sources(&path, out);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" {
                out.push(path);
            }
        }
    }

    #[test]
    fn label_scanner_finds_every_form() {
        let src = r#"
            const UNDO_NUDGE: &str = "Nudge vertices";
            self.tx("Cut", |doc| doc.delete(&objs))?;
            doc.begin_merge("Change visgroup color", format!("visgroup_color_{id}"));
            in_tx(doc, "Edit flags", None, |doc| {
            doc.transaction(
                "Smoothing groups",
                |doc| {},
            );
            doc.begin(label);
            doc.begin_merge(label, tag);
            #[cfg(test)]
            doc.begin("Only in tests");
        "#;
        let mut labels = labels_in_source(src);
        labels.sort();
        assert_eq!(labels, ["Change visgroup color", "Cut", "Edit flags", "Nudge vertices", "Smoothing groups"]);
    }

    #[test]
    fn every_undo_label_of_every_crate_is_translated() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut files = Vec::new();
        rust_sources(&crates, &mut files);
        assert!(files.len() > 50, "sources not found in {}", crates.display());
        let mut seen = std::collections::BTreeSet::new();
        let mut missing = Vec::new();
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            for label in labels_in_source(&text) {
                if seen.insert(label.clone()) && !hp_app::i18n::has_key(&undo_key(&label)) {
                    missing.push(format!("{label} ({}, key {})", path.display(), undo_key(&label)));
                }
            }
        }
        assert!(seen.len() > 30, "too few undo labels found: {seen:?}");
        assert!(missing.is_empty(), "undo labels without translation: {missing:#?}");
        hp_app::i18n::set_lang(Lang::Fr);
        assert_eq!(undo_step_text("Nudge vertices"), "Décalage des sommets");
        hp_app::i18n::set_lang(Lang::En);
    }

    #[test]
    fn long_menus_split_into_columns_between_sections() {
        let act = |i: usize| MenuItem::Action(Action::ALL[i]);
        let sections = [
            vec![act(0), act(1), act(2), act(3)],
            vec![act(4), act(5), act(6), act(7)],
            vec![act(8), act(9), act(10), act(11)],
        ];
        let mut items = Vec::new();
        for (i, s) in sections.iter().enumerate() {
            if i > 0 {
                items.push(MenuItem::Separator);
            }
            items.extend(s.iter().cloned());
        }
        let actions = |cols: &[Vec<MenuItem>]| -> Vec<MenuItem> {
            cols.iter().flatten().filter(|i| !matches!(i, MenuItem::Separator)).cloned().collect()
        };
        let height = |col: &[MenuItem]| -> f32 {
            col.iter().map(|i| if matches!(i, MenuItem::Separator) { 5.0 } else { 10.0 }).sum()
        };
        let fits = menu_columns(items.clone(), 10.0, 5.0, 200.0);
        assert_eq!(fits, vec![items.clone()], "a menu that fits is unchanged");
        for avail in [95.0, 60.0, 25.0] {
            let cols = menu_columns(items.clone(), 10.0, 5.0, avail);
            assert!(cols.len() > 1, "{avail}: split");
            assert_eq!(actions(&cols), actions(std::slice::from_ref(&items)), "{avail}: every entry kept, in order");
            for col in &cols {
                assert!(height(col) <= avail, "{avail}: column of {} too tall", height(col));
                assert!(!matches!(col.first(), Some(MenuItem::Separator) | None));
                assert!(!matches!(col.last(), Some(MenuItem::Separator)));
            }
        }
        let cols = menu_columns(items.clone(), 10.0, 5.0, 95.0);
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[1], sections[2]);
    }
}
