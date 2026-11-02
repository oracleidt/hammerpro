use hp_app::i18n::TrTable;
use hp_app::state::RightTab;
use hp_app::{Action, Severity, t};

use super::{DockTab, Package};
use crate::app::HammerProApp;

pub(crate) struct Entities;

#[derive(Default)]
pub(crate) struct State {
    fgd_watch: Option<(std::time::Instant, Vec<FileStamp>)>,
}

type FileStamp = (std::path::PathBuf, Option<std::time::SystemTime>);

const FGD_WATCH_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

fn watch_fgd(app: &mut HammerProApp) {
    let now = std::time::Instant::now();
    let watch = &mut app.pkg.entities.fgd_watch;
    if watch.as_ref().is_some_and(|(at, _)| now.duration_since(*at) < FGD_WATCH_EVERY) {
        return;
    }
    let report = app.state.assets.fgd_report();
    if report.loading || report.files.is_empty() {
        return;
    }
    let files: Vec<FileStamp> = report
        .files
        .iter()
        .take(64)
        .map(|f| (f.clone(), std::fs::metadata(f).and_then(|m| m.modified()).ok()))
        .collect();
    let changed = match watch.take() {
        Some((_, old)) => old.len() == files.len() && old.iter().zip(&files).any(|(a, b)| a.0 == b.0 && a.1 != b.1),
        None => false,
    };
    *watch = Some((now, files));
    if changed {
        app.state.assets.reload_fgd(&app.state.settings);
        app.state.log(Severity::Info, t!("entities.fgd_reloaded"));
    }
}

const HANDLED: &[Action] = &[
    Action::FindEntities,
    Action::ReplaceTextures,
    Action::EntityReport,
    Action::CopyKeyvalues,
    Action::PasteKeyvalues,
    Action::FindReplaceText,
    Action::EditColor,
    Action::SetDisplayColor,
    Action::RandomDisplayColors,
    Action::ResetDisplayColors,
    Action::ShowSoundBrowser,
    Action::ShowParticleBrowser,
    Action::EntityHelp,
    Action::EditSoundscapes,
    Action::SetMaxDistance,
    Action::PathInsertPoint,
    Action::PathRemovePoint,
];

fn selected_class(app: &HammerProApp) -> Option<String> {
    let summary = hp_ui_entities::SelectionSummary::of(&app.state.doc);
    let id = summary.entities.first()?;
    app.state.doc.scene().entity(*id).map(|n| n.entity.classname.clone())
}

impl Package for Entities {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::FindEntities => app.state.ui.show_find_entities = true,
            Action::ReplaceTextures => app.state.ui.show_replace_textures = true,
            Action::EntityReport => app.state.ui.show_entity_report = true,
            Action::CopyKeyvalues => {
                hp_ui_entities::copy_keyvalues(ctx, &mut app.state);
            }
            Action::PasteKeyvalues => {
                hp_ui_entities::paste_keyvalues(&mut app.state);
            }
            Action::FindReplaceText => app.state.runtime.entities.show_find_replace = true,
            Action::EditColor => app.state.runtime.entities.edit_color = true,
            Action::SetDisplayColor => app.state.runtime.entities.display_color_window = true,
            Action::RandomDisplayColors | Action::ResetDisplayColors => {
                let change = if action == Action::RandomDisplayColors {
                    hp_editor::display_color::DisplayColor::Random
                } else {
                    hp_editor::display_color::DisplayColor::Reset
                };
                let objs = app.state.doc.selected_objects();
                if let Err(e) = app.state.doc.set_display_color(&objs, change, false) {
                    app.state.notify(Severity::Warning, e.to_string());
                }
            }
            Action::SetMaxDistance => app.state.runtime.entities.set_max_distance = true,
            Action::EditSoundscapes => app.state.runtime.entities.show_soundscapes = true,
            Action::PathInsertPoint => {
                hp_tools_create::path::insert_after_selection(&mut app.state);
            }
            Action::PathRemovePoint => {
                hp_tools_create::path::remove_selected(&mut app.state, true);
            }
            Action::EntityHelp => {
                let class = selected_class(app).unwrap_or_else(|| app.state.current_entity_class.clone());
                if !class.trim().is_empty() {
                    app.state.runtime.entities.help_class = Some(class);
                }
            }
            Action::ShowSoundBrowser => {
                let rt = &mut app.state.runtime.entities;
                let shown = app.state.settings.browser.show_sound_browser || rt.sound_window;
                if shown {
                    app.state.settings.browser.show_sound_browser = false;
                    rt.sound_window = false;
                } else {
                    app.state.settings.browser.show_sound_browser = true;
                    app.state.ui.right_tab = RightTab::Package(hp_ui_entities::SOUND_TAB);
                }
                app.state.settings_dirty = true;
            }
            Action::ShowParticleBrowser => {
                let rt = &mut app.state.runtime.entities;
                let shown = app.state.settings.browser.show_particle_browser || rt.particle_window;
                if shown {
                    app.state.settings.browser.show_particle_browser = false;
                    rt.particle_window = false;
                } else {
                    app.state.settings.browser.show_particle_browser = true;
                    app.state.ui.right_tab = RightTab::Package(hp_ui_entities::PARTICLE_TAB);
                }
                app.state.settings_dirty = true;
            }
            _ => return false,
        }
        true
    }

    fn init(&self, _app: &mut HammerProApp) {
        hp_app::i18n::register(TR);
    }

    fn tick(&self, app: &mut HammerProApp, _ctx: &egui::Context) {
        hp_ui_entities::finish_asset_pick(&mut app.state);
        watch_fgd(app);
        hp_tools_create::path::sync_options(&app.state);
    }

    fn dock_tabs(&self, app: &HammerProApp, tabs: &mut Vec<DockTab>) {
        let b = &app.state.settings.browser;
        let rt = &app.state.runtime.entities;
        if b.show_sound_browser && !rt.sound_window {
            tabs.push(DockTab { id: hp_ui_entities::SOUND_TAB, label: t!("entities.dock_sounds").to_owned() });
        }
        if b.show_particle_browser && !rt.particle_window {
            tabs.push(DockTab { id: hp_ui_entities::PARTICLE_TAB, label: t!("entities.dock_particles").to_owned() });
        }
    }

    fn dock_ui(&self, app: &mut HammerProApp, ui: &mut egui::Ui, id: &'static str) -> bool {
        if id == hp_ui_entities::SOUND_TAB {
            app.entity_dialogs.sound_browser().ui(ui, &mut app.state);
        } else if id == hp_ui_entities::PARTICLE_TAB {
            app.entity_dialogs.particle_browser().ui(ui, &mut app.state);
        } else {
            return false;
        }
        true
    }
}

const TR: TrTable = &[(
    "entities.fgd_reloaded",
    "An entity definition file (FGD) changed: definitions reloaded",
    "Un fichier de définitions d'entités (FGD) a changé : définitions rechargées",
)];
