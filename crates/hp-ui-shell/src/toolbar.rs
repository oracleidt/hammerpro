use egui::{Color32, RichText, Ui};
use hp_app::settings::{GRID_MAX, GRID_MIN, Models2dSetting, RenderModeSetting, SelectModeSetting};
use hp_app::{Action, AppState, t, tf};

use crate::common::{
    Check, GRID_SIZES, action_check, action_enabled, action_icon, close_on_escape, fmt_grid, with_action_tooltip,
};

const BUTTON_SIZE: egui::Vec2 = egui::vec2(24.0, 24.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolbarItem {
    Action(Action),
    GridSize,
    Compile,
    RenderMode,
    Props2d,
    PrimitiveStats,
}

pub const TOOLBAR: &[&[ToolbarItem]] = {
    use Action as A;
    use ToolbarItem::{Action as I, Compile, GridSize, PrimitiveStats, Props2d, RenderMode};
    &[
        &[I(A::NewMap), I(A::OpenMap), I(A::SaveMap)],
        &[I(A::ToggleGrid), I(A::ToggleGrid3d), GridSize, I(A::ToggleSnap)],
        &[I(A::Undo), I(A::Redo)],
        &[I(A::Carve), I(A::Group), I(A::Ungroup), I(A::ToggleIgnoreGroups)],
        &[I(A::HideSelected), I(A::HideUnselected), I(A::UnhideAll)],
        &[I(A::Cut), I(A::Copy), I(A::Paste)],
        &[I(A::ToggleCordon), I(A::ToolCordon), I(A::ToggleRadiusCulling)],
        &[
            I(A::ToggleDispMesh2d),
            I(A::ToggleDispWalkable),
            I(A::ToggleNodraw),
            I(A::ToggleCollisionOnly),
            I(A::ToggleTextureLock),
            I(A::ToggleTextureScaleLock),
        ],
        &[Compile],
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
        &[RenderMode, I(A::TogglePlayerView), I(A::CheckProblems), PrimitiveStats],
    ]
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Icon {
    Glyph(&'static str),
    Eye,
    EyeCrossed,
    EyeCrossedFramed,
    Group,
    Ungroup,
}

fn button_icon(action: Action) -> Icon {
    match action {
        Action::HideSelected => Icon::EyeCrossed,
        Action::HideUnselected => Icon::EyeCrossedFramed,
        Action::UnhideAll => Icon::Eye,
        Action::Group => Icon::Group,
        Action::Ungroup => Icon::Ungroup,
        other => Icon::Glyph(toolbar_glyph(other).or_else(|| action_icon(other)).unwrap_or("?")),
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
    for group in TOOLBAR {
        for item in *group {
            if let ToolbarItem::Action(a) = item
                && let Icon::Glyph(g) = button_icon(*a)
            {
                out.push(g);
            }
        }
    }
    for item in palette_items() {
        if let PaletteItem::Action(a) = item
            && let Icon::Glyph(g) = button_icon(a)
        {
            out.push(g);
        }
    }
    out
}

fn is_checked(state: &AppState, action: Action) -> bool {
    matches!(action_check(state, action), Check::Toggle(true) | Check::Radio(true))
}

fn action_icon_button(ui: &mut Ui, state: &mut AppState, action: Action) {
    let enabled = action_enabled(state, action);
    let selected = is_checked(state, action);
    let r = match button_icon(action) {
        Icon::Glyph(g) => ui.add_enabled(
            enabled,
            egui::Button::selectable(selected, RichText::new(g).size(15.0)).min_size(BUTTON_SIZE),
        ),
        painted => {
            let r = ui.add_enabled(enabled, egui::Button::selectable(selected, "").min_size(BUTTON_SIZE));
            let color = if enabled { ui.style().interact(&r).fg_stroke.color } else { ui.visuals().weak_text_color() };
            paint_icon(ui.painter(), r.rect.center(), color, painted, 0.8);
            r
        }
    };
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
}

fn toolbar_hint(action: Action) -> Option<&'static str> {
    match action {
        Action::ToggleCollisionOnly => Some(t!("shell.toolbar.collision_only_tip")),
        _ => None,
    }
}

fn group_separator(ui: &mut Ui) {
    ui.separator();
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

pub fn toolbar(ui: &mut Ui, state: &mut AppState) {
    if !state.settings.workflow.show_toolbar {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.spacing_mut().item_spacing.x = 1.0;
        ui.spacing_mut().button_padding.x = 3.0;
        for (i, group) in TOOLBAR.iter().enumerate() {
            if i > 0 {
                group_separator(ui);
            }
            for item in *group {
                match *item {
                    ToolbarItem::Action(a) => action_icon_button(ui, state, a),
                    ToolbarItem::GridSize => grid_controls(ui, state),
                    ToolbarItem::Compile => run_button(ui, state),
                    ToolbarItem::RenderMode => render_mode_dropdown(ui, state),
                    ToolbarItem::Props2d => {
                        props_2d_button(ui, state);
                    }
                    ToolbarItem::PrimitiveStats => {
                        let on = state.settings.render.primitive_stats;
                        if ui
                            .add(egui::Button::selectable(on, "Σ").min_size(BUTTON_SIZE))
                            .on_hover_text(t!("shell.toolbar.primitives_tip"))
                            .clicked()
                        {
                            state.settings.render.primitive_stats = !on;
                            state.settings_dirty = true;
                        }
                    }
                }
            }
        }
    });
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

fn grid_controls(ui: &mut Ui, state: &mut AppState) {
    let target = state.target_grid();
    let label = crate::grids::source_label(&target);
    let custom = matches!(target.source, hp_app::grid::GridSource::Map(_) | hp_app::grid::GridSource::Preset(_));
    let text = RichText::new(label);
    let text = if custom { text.strong() } else { text.color(ui.visuals().weak_text_color()) };
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
    button.on_hover_text(crate::grids::source_tip(state, &target));
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
        egui::TextEdit::singleline(&mut text).id(id.with("field")).font(egui::TextStyle::Monospace).desired_width(48.0),
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
    field.on_hover_text(format!(
        "{}
{}",
        t!("shell.toolbar.grid_size_tip"),
        tf!("shell.grid.tip_steps", crate::grids::steps_text(target.frame.step.to_array()))
    ));
    let mut chosen = None;
    ui.menu_button(RichText::new("⏷").small(), |ui| {
        ui.set_min_width(70.0);
        for g in GRID_SIZES {
            if ui.selectable_label((g - current).abs() < 1e-9, RichText::new(fmt_grid(g)).monospace()).clicked() {
                chosen = Some(g);
                ui.close();
            }
        }
        close_on_escape(ui);
    })
    .response
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
        .on_hover_text(format!("{}\n{current_label}", t!("menu.render_mode")));
    if let Some(action) = chosen {
        state.request(action);
    }
}

fn run_button(ui: &mut Ui, state: &mut AppState) {
    let r = hp_app::widgets::accent_button(ui, t!("shell.toolbar.compile"));
    if with_action_tooltip(r, state, Action::RunMap).clicked() {
        state.request(Action::RunMap);
    }
}

pub use crate::tool_strip::{PaletteItem, palette_items, tool_palette};

pub(crate) fn button_glyph(action: Action) -> &'static str {
    match button_icon(action) {
        Icon::Glyph(g) => g,
        _ => "?",
    }
}

pub(crate) fn paint_eye(painter: &egui::Painter, c: egui::Pos2, color: Color32, scale: f32) {
    paint_icon(painter, c, color, Icon::Eye, scale);
}

fn paint_icon(painter: &egui::Painter, c: egui::Pos2, color: egui::Color32, icon: Icon, scale: f32) {
    let s = scale;
    let c = hp_app::widgets::crisp_center(painter, c);
    let stroke = hp_app::widgets::crisp_stroke(painter, 1.6, color);
    let thin = hp_app::widgets::crisp_stroke(painter, 1.0, color);
    let heavy = hp_app::widgets::crisp_stroke(painter, 2.0, color);
    let eye = |painter: &egui::Painter| {
        let (w, h) = (9.0 * s, 5.5 * s);
        let lid = |sign: f32| -> Vec<egui::Pos2> {
            (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.0;
                    let x = -w + 2.0 * w * t;
                    let y = sign * h * (1.0 - (x / w).powi(2));
                    egui::pos2(c.x + x, c.y + y)
                })
                .collect()
        };
        painter.add(egui::Shape::line(lid(-1.0), stroke));
        painter.add(egui::Shape::line(lid(1.0), stroke));
        painter.circle_stroke(c, 3.6 * s, stroke);
        painter.circle_filled(c, 1.8 * s, color);
    };
    let slash = |painter: &egui::Painter| {
        let d = egui::vec2(8.0 * s, 8.0 * s);
        painter.line_segment([c - d, c + d], heavy);
    };
    match icon {
        Icon::Glyph(_) => {}
        Icon::Eye => eye(painter),
        Icon::EyeCrossed => {
            eye(painter);
            slash(painter);
        }
        Icon::EyeCrossedFramed => {
            eye(painter);
            slash(painter);
            let r = egui::Rect::from_center_size(c, egui::vec2(24.0 * s, 20.0 * s));
            painter.rect_stroke(hp_app::widgets::crisp_rect(painter, r), 2.0, thin, egui::StrokeKind::Inside);
        }
        Icon::Group | Icon::Ungroup => {
            let apart = if icon == Icon::Ungroup { 3.5 } else { 0.0 };
            let size = egui::vec2(8.0 * s, 8.0 * s);
            let a = egui::Rect::from_center_size(c + egui::vec2(-3.0 * s - apart, -2.0 * s), size);
            let b = egui::Rect::from_center_size(c + egui::vec2(3.0 * s + apart, 2.0 * s), size);
            painter.rect_filled(hp_app::widgets::crisp_rect(painter, a), 1.0, color);
            painter.rect_stroke(hp_app::widgets::crisp_rect(painter, b), 1.0, stroke, egui::StrokeKind::Inside);
            if icon == Icon::Group {
                let frame = a.union(b).expand(2.5 * s);
                painter.rect_stroke(hp_app::widgets::crisp_rect(painter, frame), 2.0, thin, egui::StrokeKind::Outside);
            }
        }
    }
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    (
        "shell.toolbar.primitives_tip",
        "Map primitives and measured editor CPU allocations",
        "Primitives de la map et allocations CPU mesurées de l'éditeur",
    ),
    ("shell.toolbar.compile", "Compile", "Compiler"),
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
    fn toolbar_follows_hammer() {
        use Action as A;
        let items: Vec<ToolbarItem> = TOOLBAR.iter().flat_map(|g| g.iter().copied()).collect();
        let pos = |item: ToolbarItem| items.iter().position(|i| *i == item).unwrap_or_else(|| panic!("{item:?}"));
        let hammer = [
            ToolbarItem::Action(A::ToggleGrid),
            ToolbarItem::Action(A::ToggleGrid3d),
            ToolbarItem::GridSize,
            ToolbarItem::Action(A::Undo),
            ToolbarItem::Action(A::Redo),
            ToolbarItem::Action(A::Carve),
            ToolbarItem::Action(A::Group),
            ToolbarItem::Action(A::Ungroup),
            ToolbarItem::Action(A::ToggleIgnoreGroups),
            ToolbarItem::Action(A::HideSelected),
            ToolbarItem::Action(A::HideUnselected),
            ToolbarItem::Action(A::UnhideAll),
            ToolbarItem::Action(A::Cut),
            ToolbarItem::Action(A::Copy),
            ToolbarItem::Action(A::Paste),
            ToolbarItem::Action(A::ToggleCordon),
            ToolbarItem::Action(A::ToolCordon),
            ToolbarItem::Action(A::ToggleRadiusCulling),
            ToolbarItem::Action(A::ToggleDispWalkable),
            ToolbarItem::Action(A::ToggleNodraw),
            ToolbarItem::Action(A::ToggleCollisionOnly),
            ToolbarItem::Action(A::ToggleTextureLock),
            ToolbarItem::Action(A::ToggleTextureScaleLock),
            ToolbarItem::Compile,
            ToolbarItem::Action(A::ToggleHelpers),
            ToolbarItem::Action(A::ToggleModels),
            ToolbarItem::Action(A::ToggleToolTextures),
        ];
        let positions: Vec<usize> = hammer.iter().map(|i| pos(*i)).collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{hammer:?} at {positions:?}");
        let mut seen = std::collections::HashSet::new();
        for item in &items {
            assert!(seen.insert(*item), "{item:?} twice");
            if let ToolbarItem::Action(a) = item {
                assert!(a.is_ready(), "{a:?}");
                assert!(!matches!(a, A::SelectModeGroups | A::SelectModeObjects | A::SelectModeSolids));
            }
        }
        assert_eq!(
            TOOLBAR[0],
            &[ToolbarItem::Action(A::NewMap), ToolbarItem::Action(A::OpenMap), ToolbarItem::Action(A::SaveMap)]
        );
        assert!(TOOLBAR.last().unwrap().contains(&ToolbarItem::RenderMode));
        for group in TOOLBAR {
            let icons: Vec<Icon> = group
                .iter()
                .filter_map(|i| match i {
                    ToolbarItem::Action(a) => Some(button_icon(*a)),
                    _ => None,
                })
                .collect();
            let unique: std::collections::HashSet<&Icon> = icons.iter().collect();
            assert_eq!(unique.len(), icons.len(), "{group:?}");
        }
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
