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
            Builtin::Recent => (Category::File, 10, 5),
            Builtin::Language => (Category::View, 50, 10),
            Builtin::FirstRun => (Category::Help, 20, 10),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuItem {
    Separator,
    Action(Action),
    Submenu { title: &'static str, width: f32, items: Vec<MenuItem> },
    Builtin(Builtin),
    Extension(usize),
}

impl MenuItem {
    pub fn actions(&self) -> Vec<Action> {
        match self {
            MenuItem::Action(a) => vec![*a],
            MenuItem::Submenu { items, .. } => items.iter().flat_map(MenuItem::actions).collect(),
            _ => Vec::new(),
        }
    }

    pub fn is_entry(&self) -> bool {
        !matches!(self, MenuItem::Separator)
    }
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
        MenuGroup::FileSave => &[A::SaveMap, A::SaveMapAs, A::SaveAsNewVersion],
        MenuGroup::ViewDisplay => &[
            A::TogglePointEntities,
            A::ToggleEditorObjects,
            A::ToggleHelpersAll,
            A::ToggleConnections,
            A::ToggleOverlays,
            A::ToggleDisplacements,
            A::ToggleNodraw,
            A::ToggleBoundsOverlay,
        ],
        MenuGroup::ViewDisplayInfo => &[A::ToggleInvalidBrushes, A::ToggleViewInfo, A::BackgroundImages],
        MenuGroup::ViewEffects => &[
            A::ToggleRenderEffects,
            A::ToggleEntityFx,
            A::ToggleAnimatedMaterials,
            A::ToggleModelFade,
            A::ToggleOpaqueMaterials,
            A::ToggleRadiusCulling,
        ],
        MenuGroup::ViewEffectsObjects => {
            &[A::ToggleParticles, A::ToggleParticlesSelected, A::ToggleRopes, A::SimulateRopes]
        }
        MenuGroup::ToolsAlign => &[
            A::AlignLeft,
            A::AlignRight,
            A::AlignTop,
            A::AlignBottom,
            A::AlignCenterHorizontal,
            A::AlignCenterVertical,
        ],
        MenuGroup::FileImport => &[
            A::ImportMap,
            A::ImportModel,
            A::ImportModelsBatch,
            A::ImportFromGame,
            A::ImportHeightmap,
            A::ImportBlendMasks,
        ],
        MenuGroup::FileImportOpen => &[A::OpenArchive, A::OpenBsp, A::OpenBspSniffer],
        MenuGroup::FileExport => {
            &[A::ExportSelection, A::ExportCopy, A::ExportAgain, A::ExportObj, A::ExportHeightmap, A::ExportBlendMasks]
        }
        MenuGroup::FileRun => &[A::RunMap, A::CompileNow, A::CancelCompile, A::BatchCompile],
        MenuGroup::FileGame => &[A::LaunchGame, A::ReloadInGame],
        MenuGroup::FilePublish => &[A::BuildContent, A::PublishWorkshop],
        MenuGroup::EditFind => &[A::FindEntities, A::FindReplaceText],
        MenuGroup::EditProperties => &[A::Properties, A::CopyKeyvalues, A::PasteKeyvalues, A::SetMaxDistance],
        MenuGroup::EditColors => &[A::EditColor, A::SetDisplayColor, A::RandomDisplayColors, A::ResetDisplayColors],
        MenuGroup::EditFaceHistory => &[A::FaceSelectionBack, A::FaceSelectionForward],
        MenuGroup::ViewPanels => &PANEL_ORDER,
        MenuGroup::ViewType => &[
            A::ViewTop,
            A::ViewSide,
            A::ViewFront,
            A::View3dTextured,
            A::View3dFlat,
            A::CycleView2d,
            A::ViewLogical,
            A::ShowLogicalView,
        ],
        MenuGroup::ViewRender => &[
            A::RenderTextured,
            A::RenderFlat,
            A::RenderWireframe,
            A::RenderTexturedWire,
            A::RenderLit,
            A::RenderBaked,
            A::RenderUnshaded,
            A::RenderSmoothingGroups,
            A::RenderLightmapGrid,
            A::RenderColorCorrected,
        ],
        MenuGroup::ViewShow => &[A::ToggleModels, A::ToggleHelpers, A::ToggleToolTextures],
        MenuGroup::ViewSky => &[A::ToggleSkyPreview, A::ToggleSkybox3d, A::UseSkyCameraInGame],
        MenuGroup::ViewFog => &[A::ToggleFogPreview, A::ToggleFogVolumes, A::ToggleFogVolumeTexture],
        MenuGroup::ViewHide => {
            &[A::HideSelected, A::HideUnselected, A::UnhideAll, A::ToggleCordon, A::MoveToVisgroup, A::HiddenToVisgroup]
        }
        MenuGroup::ViewNav => &[A::Center2dOnSelection, A::Center3dOnSelection, A::MaximizeView, A::ToggleFullscreen],
        MenuGroup::ViewZoom => &[A::Zoom2dIn, A::Zoom2dOut, A::FarPlaneCloser, A::FarPlaneFarther, A::ResetViewSplits],
        MenuGroup::MapGrid => &[A::ToggleSnap, A::ToggleGrid, A::ToggleGrid3d, A::GridSmaller, A::GridLarger],
        MenuGroup::MapInfo => &[
            A::CheckProblems,
            A::ShowOptimize,
            A::CheckLeaks,
            A::GoToBrush,
            A::EntityReport,
            A::ShowMapAssets,
            A::CompareMaps,
        ],
        MenuGroup::MapProperties => &[A::MapInfo, A::MapProperties, A::EditSoundscapes, A::EntityGallery],
        MenuGroup::MapPointfile => &[A::LoadPointfile, A::UnloadPointfile],
        MenuGroup::MapPortals => &[A::ShowPortalViewer, A::BuildPortals, A::LoadPortalFile, A::UnloadPortalFile],
        MenuGroup::MapOptimize => {
            &[A::AnalyzeOptimization, A::ToggleVisleaves, A::ShowDetailSuggestions, A::SealAreaportal]
        }
        MenuGroup::MapLighting => &[A::BakeLighting, A::BakeAroundCamera, A::StopBake],
        MenuGroup::MapLights => &[
            A::SoloSelectedLights,
            A::MuteSelectedLights,
            A::UnmuteAllLights,
            A::MeasureLightCosts,
            A::SelectTinyLightmapFaces,
        ],
        MenuGroup::MapCubemaps => &[A::RebuildCubemaps, A::WriteCubemaps],
        MenuGroup::ToolsBrush => &[A::TransformDialog, A::SnapToGrid],
        MenuGroup::ToolsFlip => &[A::FlipHorizontal, A::FlipVertical, A::FlipX, A::FlipY, A::FlipZ],
        MenuGroup::ToolsArrange => &[A::MoveToFloor, A::MoveToCeiling, A::MoveToFloorAligned, A::MoveToCeilingAligned],
        MenuGroup::ToolsOrigin => {
            &[A::SnapToGridIndividually, A::CenterOrigins, A::TogglePivot, A::RandomRotate, A::RandomRotateDialog]
        }
        MenuGroup::ToolsGroup => &[A::Group, A::Ungroup, A::TieToEntity, A::MoveToWorld],
        MenuGroup::ToolsCsg => &[A::Carve, A::MakeHollow],
        MenuGroup::ToolsBrushOps => &[A::MergeBrushes, A::IntersectBrushes, A::UnionBrushes, A::MergeConvex],
        MenuGroup::ToolsFaceOps => &[
            A::ExtrudeInteractive,
            A::InsetInteractive,
            A::InsetFace,
            A::ExtrudeAlongNormal,
            A::ExtrudeCycleDirection,
            A::ToggleExtrudeGrow,
        ],
        MenuGroup::ToolsBrushMisc => {
            &[A::ConvertToDetail, A::SelectDetailCandidates, A::TieEachToEntity, A::NodrawHiddenFaces]
        }
        MenuGroup::ToolsDisp => {
            &[A::DispSubdivide, A::DispSmoothLoop, A::DispLoopHeight, A::DispInvertAlpha, A::DispSelectBySlope]
        }
        MenuGroup::ToolsTerrain => {
            &[A::GenerateTerrain, A::DispTileSelection, A::MeshToDisp, A::DispConformMesh, A::DispToMesh]
        }
        MenuGroup::ToolsRoads => &[A::RoadEdit, A::RoadsideEdit, A::RoadSidewalk, A::RoadCurb, A::RoadTerrain],
        MenuGroup::ToolsTextures => &[A::ReplaceTextures, A::ToggleTextureLock, A::ToggleTextureScaleLock],
        MenuGroup::ToolsPlacement => &[A::CreatePrefab, A::InsertOriginalPrefab],
        MenuGroup::ToolsScatter => &[A::PhysicsPlacement, A::ScatterByMaterial],
        MenuGroup::ToolsProps => &[A::BrushesToProp, A::PropToBrushes, A::CombineProps, A::PropperOptions],
        MenuGroup::ToolsPaths => &[A::PathInsertPoint, A::PathRemovePoint, A::GenerateNavMesh],
        MenuGroup::ToolsContent => {
            &[A::ShaderEditor, A::VtfEditor, A::OpenSoundEditor, A::OpenColorCorrectionEditor, A::ParticleEditor]
        }
        MenuGroup::ToolsBlender => &[A::SendToBlender, A::PullFromBlender, A::StopBlenderLink, A::InstallBlenderAddon],
        MenuGroup::ToolsMcp => &[A::ToggleMcpServer, A::McpPanel, A::McpInstall],
        MenuGroup::ToolsLiveLink => &[
            A::LiveLinkStart,
            A::LiveLinkStop,
            A::LiveLinkPanel,
            A::LiveLinkFireInput,
            A::LiveLinkTeleport,
            A::LiveLinkTeleportBack,
            A::LiveLinkCameraToGame,
            A::LiveLinkCameraFromGame,
            A::LiveLinkResync,
            A::LiveLinkRemoveAddon,
        ],
        MenuGroup::ToolsSwitch => &TOOLS_MENU_ORDER,
        MenuGroup::HelpDocs => &[A::OpenDocs, A::KeyboardShortcuts, A::EntityHelp],
        MenuGroup::HelpSupport => &[A::ReportIssue, A::CheckForUpdates],
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

struct SubmenuSlot {
    title: &'static str,
    width: f32,
    parts: Vec<((u16, u16), Vec<MenuItem>)>,
}

impl SubmenuSlot {
    fn key(&self) -> (u16, u16) {
        self.parts.iter().map(|(k, _)| *k).min().unwrap_or((u16::MAX, 0))
    }

    fn part(&mut self, key: (u16, u16)) -> &mut Vec<MenuItem> {
        let i = match self.parts.iter().position(|(k, _)| *k == key) {
            Some(i) => i,
            None => {
                self.parts.push((key, Vec::new()));
                self.parts.len() - 1
            }
        };
        &mut self.parts[i].1
    }

    fn into_item(mut self) -> MenuItem {
        self.parts.sort_by_key(|(k, _)| *k);
        let mut items = Vec::new();
        for (_, part) in self.parts.into_iter().filter(|(_, p)| !p.is_empty()) {
            if !items.is_empty() {
                items.push(MenuItem::Separator);
            }
            items.extend(part);
        }
        MenuItem::Submenu { title: self.title, width: self.width, items }
    }
}

fn submenu_slot<'a>(subs: &'a mut Vec<SubmenuSlot>, title: &'static str, width: f32) -> &'a mut SubmenuSlot {
    let i = match subs.iter().position(|s| s.title == title) {
        Some(i) => i,
        None => {
            subs.push(SubmenuSlot { title, width, parts: Vec::new() });
            subs.len() - 1
        }
    };
    &mut subs[i]
}

pub fn menu_layout(cat: Category, extensions: &[MenuExtension]) -> Vec<MenuItem> {
    let mut slots: Vec<(u16, u16, MenuItem)> = Vec::new();
    let mut subs: Vec<SubmenuSlot> = Vec::new();
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
            Some(title) => submenu_slot(&mut subs, title, info.width)
                .part((info.section, info.order))
                .extend(actions.into_iter().map(MenuItem::Action)),
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
        if ext.menu != cat {
            continue;
        }
        match ext.submenu {
            Some(title) => submenu_slot(&mut subs, title, MENU_MIN_WIDTH)
                .part((ext.section, ext.order))
                .push(MenuItem::Extension(i)),
            None => slots.push((ext.section, ext.order, MenuItem::Extension(i))),
        }
    }
    for sub in subs {
        let (section, order) = sub.key();
        slots.push((section, order, sub.into_item()));
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

pub fn menu_has_entries(cat: Category, extensions: &[MenuExtension]) -> bool {
    Action::ALL.iter().any(|a| a.category() == cat && a.group().is_visible())
        || Builtin::ALL.iter().any(|b| b.slot().0 == cat)
        || extensions.iter().any(|e| e.menu == cat)
}

pub fn menu_path(action: Action) -> Option<&'static [&'static str]> {
    static PATHS: std::sync::OnceLock<Vec<(Action, Vec<&'static str>)>> = std::sync::OnceLock::new();
    let paths = PATHS.get_or_init(|| {
        fn walk(items: &[MenuItem], path: &mut Vec<&'static str>, out: &mut Vec<(Action, Vec<&'static str>)>) {
            for item in items {
                match item {
                    MenuItem::Action(a) => out.push((*a, path.clone())),
                    MenuItem::Submenu { title, items, .. } => {
                        path.push(title);
                        walk(items, path, out);
                        path.pop();
                    }
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        for cat in Category::ALL {
            walk(&menu_layout(cat, &[]), &mut vec![cat.label_key()], &mut out);
        }
        out
    });
    paths.iter().find(|(a, _)| *a == action).map(|(_, p)| p.as_slice())
}

pub fn menu_path_text(action: Action, lang: Option<Lang>) -> Option<String> {
    let path = menu_path(action)?;
    let tr = |key: &str| match lang {
        Some(lang) => hp_app::i18n::tr_in(key, lang),
        None => hp_app::i18n::tr(key),
    };
    Some(path.iter().map(|k| tr(k)).collect::<Vec<_>>().join(" > "))
}

fn check_cell<'a>(ui: &Ui, check: Check) -> egui::Atom<'a> {
    let side = egui::TextStyle::Button.resolve(ui.style()).size;
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
        if !menu_has_entries(cat, extensions) {
            continue;
        }
        ui.menu_button(t!(cat.label_key()), |ui| {
            category_menu(ui, state, cat, extensions);
            close_on_escape(ui);
        });
    }
    let unread = state.runtime.collaboration.unread;
    let team = if unread > 0 { format!("Team Mapping ({unread})") } else { "Team Mapping".to_owned() };
    ui.menu_button(team, |ui| {
        ui.set_min_width(MENU_MIN_WIDTH);
        if ui.button("Team Mapping / Participants").clicked() {
            state.runtime.collaboration.show = true;
            ui.close();
        }
        if state.runtime.collaboration.connected {
            ui.separator();
            for participant in &state.runtime.collaboration.participants {
                ui.colored_label(
                    hp_app::theme::rgb(participant.color),
                    format!("{} | {} ms", participant.name, participant.latency_ms),
                );
            }
        }
        close_on_escape(ui);
    });
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
            MenuItem::Submenu { title, width, items } => submenu(ui, t!(title), |ui| {
                ui.set_min_width(width);
                for item in items {
                    menu_item(ui, state, item, extensions);
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
            out.extend(item.actions());
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
        fn labels(menu: &str, items: &[MenuItem], out: &mut Vec<(String, Vec<String>)>) {
            let mut here = Vec::new();
            for item in items {
                match item {
                    MenuItem::Action(a) => here.push(a.label().to_owned()),
                    MenuItem::Submenu { title, items, .. } => {
                        here.push(hp_app::i18n::tr(title).to_owned());
                        labels(title, items, out);
                    }
                    _ => {}
                }
            }
            out.push((menu.to_owned(), here));
        }
        for lang in Lang::ALL {
            set_lang(lang);
            for cat in Category::ALL {
                let mut all = Vec::new();
                labels(cat.label_key(), &menu_layout(cat, &[]), &mut all);
                for (menu, entries) in all {
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
            assert!(menu_path(*a).is_some_and(|p| !p.is_empty()), "{} has no menu path", a.id());
        }
    }

    #[test]
    fn every_entry_of_the_2_0_2_menus_is_still_in_the_menus() {
        let listed = menu_actions();
        let old: Vec<&str> =
            include_str!("menu_ids_2_0_2.txt").lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        assert_eq!(old.len(), 344);
        for id in old {
            let action = Action::from_id(id).unwrap_or_else(|| panic!("{id} was in the 2.0.2 menus and is gone"));
            assert!(action.is_ready(), "{id}");
            assert!(listed.contains(&action), "{id} was in the 2.0.2 menus and is not in the menus now");
        }
    }

    fn entries(items: &[MenuItem]) -> usize {
        items.iter().filter(|i| i.is_entry()).count()
    }

    fn check_heights(title: &str, items: &[MenuItem], out: &mut Vec<(String, usize)>) {
        out.push((title.to_owned(), entries(items)));
        assert!(items.first().is_some_and(MenuItem::is_entry), "{title} starts with a separator");
        assert!(items.last().is_some_and(MenuItem::is_entry), "{title} ends with a separator");
        assert!(items.windows(2).all(|w| w[0].is_entry() || w[1].is_entry()), "{title}: double separator");
        for item in items {
            if let MenuItem::Submenu { title, items, .. } = item {
                check_heights(title, items, out);
            }
        }
    }

    #[test]
    fn no_menu_is_taller_than_about_twenty_entries() {
        let mut all = Vec::new();
        for cat in Category::ALL {
            let items = menu_layout(cat, &[]);
            if items.is_empty() {
                continue;
            }
            check_heights(cat.label_key(), &items, &mut all);
        }
        for (title, n) in &all {
            assert!(*n <= 22, "{title} has {n} entries");
        }
        let top = |key: &str| all.iter().find(|(t, _)| t == key).map(|(_, n)| *n).unwrap_or(0);
        for key in ["menu.file", "menu.edit", "menu.map", "menu.view", "menu.tools", "menu.window", "menu.help"] {
            assert!(top(key) <= 21, "{key} has {} entries", top(key));
        }
        assert!(menu_layout(Category::Other, &[]).is_empty(), "the particle editor moved to Tools");
    }

    #[test]
    fn menus_follow_hammer() {
        use Action as A;
        use MenuItem::{Action as I, Builtin as B, Separator as S};
        let sub = |title: &'static str, items: &[MenuItem]| -> MenuItem {
            let width = menu_layout(Category::ALL[0], &[])
                .into_iter()
                .chain(Category::ALL.iter().flat_map(|c| menu_layout(*c, &[])))
                .find_map(|i| match i {
                    MenuItem::Submenu { title: t, width, .. } if t == title => Some(width),
                    _ => None,
                })
                .unwrap_or(0.0);
            MenuItem::Submenu { title, width, items: items.to_vec() }
        };
        let acts = |list: &[Action]| -> Vec<MenuItem> { list.iter().copied().map(I).collect() };
        let file = menu_layout(Category::File, &[]);
        assert_eq!(
            file,
            vec![
                I(A::NewMap),
                I(A::OpenMap),
                B(Builtin::Recent),
                I(A::CloseMap),
                I(A::CloseOtherMaps),
                S,
                I(A::SaveMap),
                I(A::SaveMapAs),
                I(A::SaveAsNewVersion),
                S,
                sub(
                    "menu.import",
                    &[
                        acts(&[
                            A::ImportMap,
                            A::ImportModel,
                            A::ImportModelsBatch,
                            A::ImportFromGame,
                            A::ImportHeightmap,
                            A::ImportBlendMasks
                        ]),
                        vec![S],
                        acts(&[A::OpenArchive, A::OpenBsp, A::OpenBspSniffer]),
                    ]
                    .concat()
                ),
                sub(
                    "menu.export",
                    &acts(&[
                        A::ExportSelection,
                        A::ExportCopy,
                        A::ExportAgain,
                        A::ExportObj,
                        A::ExportHeightmap,
                        A::ExportBlendMasks
                    ])
                ),
                S,
                sub(
                    "menu.compile",
                    &[
                        acts(&[A::RunMap, A::CompileNow, A::CancelCompile, A::BatchCompile]),
                        vec![S],
                        acts(&[A::LaunchGame, A::ReloadInGame]),
                    ]
                    .concat()
                ),
                sub("menu.publish", &acts(&[A::BuildContent, A::PublishWorkshop])),
                S,
                I(A::Exit),
            ]
        );
        let edit = menu_layout(Category::Edit, &[]);
        assert_eq!(
            edit[..12],
            [
                I(A::Undo),
                I(A::Redo),
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
            ]
        );
        assert_eq!(
            edit[12..],
            [
                I(A::SelectNone),
                I(A::InvertSelection),
                sub(
                    "menu.selection_mode",
                    &[
                        acts(&[A::SelectModeGroups, A::SelectModeObjects, A::SelectModeSolids]),
                        vec![S],
                        acts(&[A::ToggleIgnoreGroups])
                    ]
                    .concat()
                ),
                sub(
                    "menu.select_element",
                    &[
                        acts(&[
                            A::SelectElementObject,
                            A::SelectElementVertex,
                            A::SelectElementEdge,
                            A::SelectElementFace
                        ]),
                        vec![S],
                        acts(&[
                            A::EdgeSelectLoop,
                            A::EdgeSelectRing,
                            A::EdgeGrow,
                            A::EdgeShrink,
                            A::EdgeSelectBoundary
                        ]),
                        vec![S],
                        acts(&[A::FaceSelectionBack, A::FaceSelectionForward]),
                    ]
                    .concat()
                ),
                S,
                I(A::FindEntities),
                I(A::FindReplaceText),
                S,
                I(A::Properties),
                I(A::CopyKeyvalues),
                I(A::PasteKeyvalues),
                I(A::SetMaxDistance),
                sub(
                    "menu.colors",
                    &acts(&[A::EditColor, A::SetDisplayColor, A::RandomDisplayColors, A::ResetDisplayColors])
                ),
            ]
        );
        let titles = |items: &[MenuItem]| -> Vec<String> {
            items
                .iter()
                .map(|i| match i {
                    MenuItem::Separator => "|".to_owned(),
                    MenuItem::Action(a) => a.id().to_owned(),
                    MenuItem::Submenu { title, .. } => format!("{title} >"),
                    MenuItem::Builtin(b) => format!("{b:?}"),
                    MenuItem::Extension(i) => format!("ext {i}"),
                })
                .collect()
        };
        assert_eq!(
            titles(&menu_layout(Category::Map, &[])),
            [
                A::ToggleSnap.id(),
                A::ToggleGrid.id(),
                A::ToggleGrid3d.id(),
                A::GridSmaller.id(),
                A::GridLarger.id(),
                "menu.grids >",
                "|",
                A::CheckProblems.id(),
                A::ShowOptimize.id(),
                A::CheckLeaks.id(),
                A::GoToBrush.id(),
                A::EntityReport.id(),
                A::ShowMapAssets.id(),
                A::CompareMaps.id(),
                "menu.leaks_visibility >",
                "|",
                A::MapInfo.id(),
                A::MapProperties.id(),
                A::EditSoundscapes.id(),
                A::EntityGallery.id(),
                "|",
                "menu.lighting >",
            ]
        );
        assert_eq!(
            titles(&menu_layout(Category::View, &[])),
            [
                "menu.view_type >",
                "menu.render_mode >",
                A::TogglePlayerView.id(),
                "|",
                "menu.display >",
                "menu.environment >",
                "menu.effects >",
                "menu.diagnostics >",
                "|",
                A::HideSelected.id(),
                A::HideUnselected.id(),
                A::UnhideAll.id(),
                A::ToggleCordon.id(),
                A::MoveToVisgroup.id(),
                A::HiddenToVisgroup.id(),
                "|",
                A::Center2dOnSelection.id(),
                A::Center3dOnSelection.id(),
                A::MaximizeView.id(),
                A::ToggleFullscreen.id(),
                "menu.zoom_distance >",
                "|",
                "menu.panels >",
                "Language",
            ]
        );
        assert_eq!(
            titles(&menu_layout(Category::Tools, &[])),
            [
                "menu.switch_tool >",
                "|",
                A::TransformDialog.id(),
                A::SnapToGrid.id(),
                "menu.arrange >",
                "|",
                A::Group.id(),
                A::Ungroup.id(),
                A::TieToEntity.id(),
                A::MoveToWorld.id(),
                "|",
                A::Carve.id(),
                A::MakeHollow.id(),
                "menu.brush_ops >",
                "menu.vertices >",
                "menu.displacements >",
                "menu.curves_roads >",
                "menu.textures >",
                "|",
                "menu.placement >",
                "menu.instancing >",
                "menu.content >",
                "menu.integrations >",
                "|",
                A::CommandPalette.id(),
                A::Settings.id(),
            ]
        );
        assert_eq!(
            titles(&menu_layout(Category::Window, &[])),
            ["menu.layout >", "|", A::ShowConsole.id(), "|", A::NextMap.id(), A::PreviousMap.id()]
        );
        assert_eq!(
            titles(&menu_layout(Category::Help, &[])),
            [
                A::OpenDocs.id(),
                A::KeyboardShortcuts.id(),
                A::EntityHelp.id(),
                "|",
                A::ReportIssue.id(),
                A::CheckForUpdates.id(),
                "FirstRun",
                "|",
                A::About.id(),
            ]
        );
        let tools: Vec<Action> = menu_layout(Category::Tools, &[])
            .into_iter()
            .find_map(|i| match i {
                MenuItem::Submenu { title: "menu.switch_tool", items, .. } => Some(items),
                _ => None,
            })
            .unwrap_or_default()
            .into_iter()
            .flat_map(|i| i.actions())
            .collect();
        let mut palette: Vec<Action> =
            hp_app::ToolId::ALL.iter().filter(|t| **t != hp_app::ToolId::Spline).map(|t| t.action()).collect();
        assert_eq!(menu_path(A::ToolSpline), Some(&["menu.tools", "menu.curves_roads"][..]));
        palette.insert(5, A::ApplyCurrentTexture);
        palette.retain(|a| *a != A::ToolFoliage);
        assert_eq!(palette[4], A::ToolTexture);
        assert_eq!(tools, palette);
        assert_eq!(menu_path(A::ToolFoliage), Some(&["menu.tools", "menu.placement"][..]));
        assert_eq!(menu_path(A::Carve), Some(&["menu.tools"][..]));
        assert_eq!(menu_path(A::ImportModel), Some(&["menu.file", "menu.import"][..]));
        assert_eq!(menu_path(A::FlipX), Some(&["menu.tools", "menu.arrange"][..]));
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        assert_eq!(menu_path_text(A::ExportObj, Some(Lang::En)).as_deref(), Some("File > Export"));
        assert_eq!(menu_path_text(A::ExportObj, Some(Lang::Fr)).as_deref(), Some("Fichier > Exporter"));
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
    fn extensions_join_submenus() {
        let exts = [
            MenuExtension::new(Category::View, 20, 11, |_, _| {}).in_submenu("menu.environment"),
            MenuExtension::new(Category::View, 20, 0, |_, _| {}).in_submenu("menu.display"),
            MenuExtension::new(Category::View, 20, 99, |_, _| {}).in_submenu("menu.effects"),
        ];
        let layout = menu_layout(Category::View, &exts);
        let items = |key: &str| -> Vec<MenuItem> {
            layout
                .iter()
                .find_map(|i| match i {
                    MenuItem::Submenu { title, items, .. } if *title == key => Some(items.clone()),
                    _ => None,
                })
                .unwrap_or_default()
        };
        let env = items("menu.environment");
        let fog = env.iter().position(|i| *i == MenuItem::Action(Action::ToggleFogVolumeTexture)).unwrap();
        assert_eq!(env[fog + 1], MenuItem::Extension(0), "{env:?}");
        assert_eq!(env[fog + 2], MenuItem::Separator);
        let display = items("menu.display");
        let tools = display.iter().position(|i| *i == MenuItem::Action(Action::ToggleToolTextures)).unwrap();
        assert_eq!(display[tools + 1], MenuItem::Extension(1));
        let effects = items("menu.effects");
        assert_eq!(effects[effects.len() - 2..], [MenuItem::Separator, MenuItem::Extension(2)]);
        assert!(!layout.contains(&MenuItem::Extension(0)), "not at the top level");
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
