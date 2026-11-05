use glam::{DMat4, DVec3};
use hp_app::settings::{SelectElement, SelectModeSetting};
use hp_app::state::RightTab;
use hp_app::{Action, Severity, t, tf};

use super::{Package, edit_error_text};
use crate::app::HammerProApp;
use crate::files;

pub(crate) struct Shell;

#[derive(Default)]
pub(crate) struct State {}

const HANDLED: &[Action] = &[
    Action::NewMap,
    Action::OpenMap,
    Action::SaveMap,
    Action::SaveMapAs,
    Action::ImportMap,
    Action::ExportSelection,
    Action::Exit,
    Action::Undo,
    Action::Redo,
    Action::Cut,
    Action::Copy,
    Action::Paste,
    Action::Delete,
    Action::Duplicate,
    Action::SelectAll,
    Action::SelectNone,
    Action::FaceSelectionBack,
    Action::FaceSelectionForward,
    Action::InvertSelection,
    Action::SelectModeGroups,
    Action::SelectModeObjects,
    Action::SelectModeSolids,
    Action::SelectElementObject,
    Action::SelectElementVertex,
    Action::SelectElementEdge,
    Action::SelectElementFace,
    Action::Properties,
    Action::CommandPalette,
    Action::Settings,
    Action::Group,
    Action::Ungroup,
    Action::MapProperties,
    Action::GoToBrush,
    Action::GridSmaller,
    Action::GridLarger,
    Action::ToggleGrid,
    Action::ToggleSnap,
    Action::ToggleTextureLock,
    Action::ToggleTextureScaleLock,
    Action::Center2dOnSelection,
    Action::Center3dOnSelection,
    Action::MaximizeView,
    Action::ToggleFullscreen,
    Action::ShowTextureBrowser,
    Action::ShowModelBrowser,
    Action::ShowEntityProperties,
    Action::ShowVisgroups,
    Action::ShowOutliner,
    Action::ShowConsole,
    Action::ApplyCurrentTexture,
    Action::KeyboardShortcuts,
    Action::About,
    Action::OpenDocs,
    Action::ReportIssue,
    Action::CheckForUpdates,
    Action::InstallUpdate,
];

impl Package for Shell {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::NewMap => app.new_map(ctx),
            Action::OpenMap => {
                if let Some(path) = files::pick_open_vmf(&app.state) {
                    app.request_open(ctx, path);
                }
            }
            Action::SaveMap => {
                files::save_map(&mut app.state);
            }
            Action::SaveMapAs => {
                files::save_map_as(&mut app.state);
            }
            Action::ImportMap => files::import_map(&mut app.state),
            Action::ExportSelection => files::export_selection(&mut app.state),
            Action::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),

            Action::Undo => {
                app.state.doc.undo();
            }
            Action::Redo => {
                app.state.doc.redo();
            }
            Action::Cut => app.cut_selection(ctx),
            Action::Copy => app.copy_selection(ctx),
            Action::Paste => app.paste_from_clipboard(),
            Action::Delete => {
                if !app.state.doc.selected_objects().is_empty() {
                    let result = app.state.doc.delete_selection();
                    app.report(action, result);
                }
            }
            Action::Duplicate => {
                if app.require_selection() {
                    let g = app.state.grid_size();
                    let objs = app.selected();
                    let m = DMat4::from_translation(DVec3::new(g, g, 0.0));
                    let result =
                        hp_app::runtime::helpers::duplicate_linking_paths(&mut app.state, &objs, m, DVec3::ZERO);
                    if let Some(new) = app.report(action, result) {
                        app.state.doc.select(&new, hp_editor::SelectOp::Replace);
                    }
                }
            }
            Action::SelectAll => app.state.doc.select_all(),
            Action::SelectNone => app.state.doc.clear_selection(),
            Action::InvertSelection => app.state.doc.invert_selection(),
            Action::FaceSelectionBack | Action::FaceSelectionForward => {
                if app.state.tool != hp_app::ToolId::Texture {
                    app.switch_tool(hp_app::ToolId::Texture);
                }
                let moved = if action == Action::FaceSelectionBack {
                    app.state.doc.face_selection_back()
                } else {
                    app.state.doc.face_selection_forward()
                };
                if !moved {
                    app.state.notify(hp_app::Severity::Info, hp_app::t!("shell.no_face_selection_step").to_owned());
                }
            }
            Action::SelectModeGroups => app.set_select_mode(SelectModeSetting::Groups),
            Action::SelectModeObjects => app.set_select_mode(SelectModeSetting::Objects),
            Action::SelectModeSolids => app.set_select_mode(SelectModeSetting::Solids),
            Action::SelectElementObject => app.set_select_element(SelectElement::Object),
            Action::SelectElementVertex => app.set_select_element(SelectElement::Vertex),
            Action::SelectElementEdge => app.set_select_element(SelectElement::Edge),
            Action::SelectElementFace => app.set_select_element(SelectElement::Face),
            Action::Properties => app.show_panel(RightTab::Entity),
            Action::CommandPalette => app.state.ui.show_command_palette = true,
            Action::Settings => app.state.ui.show_settings = true,

            Action::Group => {
                if app.require_selection() {
                    let objs = app.selected();
                    let result = app.state.doc.group(&objs);
                    app.report(action, result);
                }
            }
            Action::Ungroup => {
                if app.require_selection() {
                    let objs = app.selected();
                    let result = app.state.doc.ungroup(&objs);
                    app.report(action, result);
                }
            }
            Action::MapProperties => app.state.ui.show_map_properties = true,
            Action::GoToBrush => app.state.ui.show_goto = true,

            Action::GridSmaller | Action::GridLarger => {
                if let Err(e) = app.state.step_target_grid(action == Action::GridLarger) {
                    app.state.notify(Severity::Warning, e);
                }
            }
            Action::ToggleGrid => {
                app.state.settings.edit.show_grid = !app.state.settings.edit.show_grid;
                app.state.settings_dirty = true;
            }
            Action::ToggleSnap => {
                app.state.settings.edit.snap_to_grid = !app.state.settings.edit.snap_to_grid;
                app.state.settings_dirty = true;
            }
            Action::ToggleTextureLock => {
                app.state.settings.edit.texture_lock = !app.state.settings.edit.texture_lock;
                app.state.settings_dirty = true;
                files::apply_doc_options(&mut app.state);
            }
            Action::ToggleTextureScaleLock => {
                app.state.settings.edit.texture_scale_lock = !app.state.settings.edit.texture_scale_lock;
                app.state.settings_dirty = true;
                files::apply_doc_options(&mut app.state);
            }
            Action::Center2dOnSelection => app.center_2d(),
            Action::Center3dOnSelection => app.center_3d(),
            Action::ToggleFullscreen => {
                let on = !ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(on));
                app.state.ui.fullscreen = on;
            }
            Action::MaximizeView => {
                app.state.ui.maximized_view = match app.state.ui.maximized_view {
                    Some(_) => None,
                    None => {
                        let ui = &app.state.ui;
                        ui.hovered_view.or(ui.hovered_logical).or(ui.focused_view).or(Some(0))
                    }
                };
            }
            Action::ShowTextureBrowser => {
                app.show_panel(RightTab::Textures);
                app.state.ui.focus_texture_search = true;
            }
            Action::ShowModelBrowser => app.show_panel(RightTab::Models),
            Action::ShowEntityProperties => app.show_panel(RightTab::Entity),
            Action::ShowVisgroups => app.show_panel(RightTab::Visgroups),
            Action::ShowOutliner => app.show_panel(RightTab::Outliner),
            Action::ShowConsole => {
                app.state.settings.ui.show_console = !app.state.settings.ui.show_console;
                app.state.settings_dirty = true;
            }

            Action::ApplyCurrentTexture => app.apply_current_texture(),

            Action::KeyboardShortcuts => app.state.ui.show_shortcuts = true,
            Action::About => app.state.ui.show_about = true,
            Action::OpenDocs => ctx.open_url(egui::OpenUrl::new_tab(docs_url(hp_app::i18n::lang()))),
            Action::ReportIssue => ctx.open_url(egui::OpenUrl::new_tab(ISSUES_URL)),
            Action::CheckForUpdates => {
                if !app.state.runtime.update.status.is_busy() {
                    app.state.runtime.update.check_requested = Some(true);
                }
            }
            Action::InstallUpdate => app.state.runtime.update.install_requested = true,
            _ => return false,
        }
        true
    }
}

impl HammerProApp {
    fn set_clipboard(&mut self, ctx: &egui::Context, text: String) {
        match self.clipboard.as_mut() {
            Some(clipboard) => {
                if let Err(e) = clipboard.set_text(text.clone()) {
                    log::warn!("clipboard: {e}");
                    ctx.copy_text(text);
                }
            }
            None => ctx.copy_text(text),
        }
    }

    pub(crate) fn copy_selection(&mut self, ctx: &egui::Context) {
        if self.state.doc.selected_objects().is_empty() {
            return;
        }
        let text = self.state.doc.copy_selection();
        self.set_clipboard(ctx, text);
    }

    pub(crate) fn cut_selection(&mut self, ctx: &egui::Context) {
        if self.state.doc.selected_objects().is_empty() {
            return;
        }
        let result = self.state.doc.cut_selection();
        if let Some(text) = self.report(Action::Cut, result) {
            self.set_clipboard(ctx, text);
        }
    }

    pub(crate) fn paste_text(&mut self, text: &str) {
        let offset = self.paste_at_cursor(text).unwrap_or_else(|| {
            if self.state.settings.edit.paste_offset {
                let g = self.state.grid_size();
                DVec3::new(g, g, 0.0)
            } else {
                DVec3::ZERO
            }
        });
        let fgd = self.state.assets.fgd().filter(|f| !f.is_empty());
        let targets =
            |class: &str, key: &str| fgd.and_then(|f| hp_app::runtime::helpers::fgd_target_key(f, class, key));
        match self.state.doc.paste_with(text, offset, Some(&targets)) {
            Ok(objs) if !objs.is_empty() => {
                self.state.doc.select(&objs, hp_editor::SelectOp::Replace);
            }
            Ok(_) => {}
            Err(e) => self.state.notify(Severity::Info, tf!("app.paste_failed", edit_error_text(&e))),
        }
    }

    fn paste_at_cursor(&self, text: &str) -> Option<DVec3> {
        let index = self.state.ui.hovered_view?;
        let point = self.state.hover_world.filter(|p| p.is_finite())?;
        let bounds = hp_editor::Document::clipboard_bounds(text).ok()?;
        let center = bounds.center();
        let kind = hp_app::QuadrantView::from_index(self.state.settings.ui.quadrants.get(index).copied().unwrap_or(0))
            .kind()?;
        let mut offset = match kind.axes_2d() {
            Some((_, _, depth)) => {
                let mut o = point - center;
                o[depth] = 0.0;
                o
            }
            None => DVec3::new(point.x - center.x, point.y - center.y, point.z - bounds.min.z),
        };
        let snapped = if kind.axes_2d().is_some() { 3 } else { 2 };
        for i in 0..snapped {
            offset[i] = self.state.snap(offset[i]);
        }
        offset.is_finite().then_some(offset)
    }

    fn paste_from_clipboard(&mut self) {
        let text = self.paste_event_text.take().or_else(|| self.clipboard.as_mut().and_then(|c| c.get_text().ok()));
        match text {
            Some(text) => self.paste_text(&text),
            None => self.state.notify(Severity::Info, tf!("app.paste_failed", t!("app.err.empty_clipboard"))),
        }
    }

    fn apply_current_texture(&mut self) {
        let material = self.state.current_material.clone();
        let faces = self.state.doc.selected_faces();
        let result = if faces.is_empty() {
            let objs = self.selected();
            if objs.is_empty() {
                self.state.notify(Severity::Info, t!("app.nothing_selected"));
                return;
            }
            self.state.doc.apply_material_to_objects(&objs, &material)
        } else {
            self.state.doc.apply_material(&faces, &material)
        };
        if self.report(Action::ApplyCurrentTexture, result).is_some() {
            self.state.use_material(&material);
        }
    }

    fn center_2d(&mut self) {
        let bounds = self.state.doc.selection_bounds(&self.entity_bounds_fn());
        if let Some(b) = bounds {
            let size = self.viewports_size_2d();
            self.state.cameras.frame_2d(b.min, b.max, size);
        }
    }

    fn center_3d(&mut self) {
        let bounds = self.state.doc.selection_bounds(&self.entity_bounds_fn());
        if let Some(b) = bounds {
            let mut cam = self.state.cameras.cam3d;
            cam.look_at(b.center());
            cam.frame_box(b.min, b.max);
            self.state.cameras.cam3d = cam;
        }
    }

    pub(crate) fn show_panel(&mut self, tab: RightTab) {
        let ui = &mut self.state.settings.ui;
        match tab {
            RightTab::Textures => ui.show_texture_browser = true,
            RightTab::Entity => ui.show_entity_properties = true,
            RightTab::Models => ui.show_model_browser = true,
            RightTab::Visgroups => ui.show_visgroups = true,
            RightTab::Outliner => ui.show_outliner = true,
            RightTab::Tool | RightTab::Package(_) => {}
        }
        self.state.ui.right_tab = tab;
        self.state.settings_dirty = true;
    }

    fn set_select_element(&mut self, element: SelectElement) {
        self.state.set_pick_element(element);
    }

    fn set_select_mode(&mut self, mode: SelectModeSetting) {
        self.state.settings.edit.select_mode = mode;
        self.state.settings_dirty = true;
    }
}

pub fn docs_url(lang: hp_app::i18n::Lang) -> String {
    let file = match lang {
        hp_app::i18n::Lang::Fr => "README.fr.md",
        hp_app::i18n::Lang::En => "README.md",
    };
    format!("{}/blob/HEAD/{file}", hp_ui_shell::help::REPOSITORY_URL)
}

pub const ISSUES_URL: &str = "https://github.com/hammerpro/hammerpro/issues";

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::i18n::Lang;

    #[test]
    fn documentation_follows_the_ui_language() {
        let fr = docs_url(Lang::Fr);
        let en = docs_url(Lang::En);
        assert!(fr.starts_with(hp_ui_shell::help::REPOSITORY_URL), "{fr}");
        assert!(fr.ends_with("/blob/HEAD/README.fr.md"), "{fr}");
        assert!(en.ends_with("/blob/HEAD/README.md"), "{en}");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for url in [&fr, &en] {
            let file = url.rsplit('/').next().expect("file name");
            assert!(root.join(file).is_file(), "{file} is missing");
        }
    }
}
