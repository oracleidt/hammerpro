use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { content_actions {
    SaveAsNewVersion => "save_as_new_version", File, FileSave, None, Ready;
    OpenArchive => "open_archive", File, FileImportOpen, None, Ready;
    BuildContent => "build_content", File, FilePublish, None, Ready;
    PublishWorkshop => "publish_workshop", File, FilePublish, None, Ready;
    ShowMapAssets => "show_map_assets", Map, MapInfo, None, Ready;
    ShaderEditor => "shader_editor", Tools, ToolsContent, None, Ready;
    VtfEditor => "vtf_editor", Tools, ToolsContent, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(_state: &AppState, _action: Action) -> Option<bool> {
    None
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::ShowMapAssets => Some(Check::Toggle(state.settings.content.show_map_assets)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::OpenArchive => Some("🗄"),
        Action::BuildContent => Some("📦"),
        Action::PublishWorkshop => Some("☁"),
        Action::ShowMapAssets => Some("🗀"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.save_as_new_version", "Save as new version...", "Enregistrer comme nouvelle version..."),
    ("action.open_archive", "Open archive...", "Ouvrir une archive..."),
    ("action.build_content", "Build and save as content...", "Construire et enregistrer en contenu..."),
    ("action.publish_workshop", "Publish on the Workshop...", "Publier sur le Workshop..."),
    ("action.show_map_assets", "Map assets", "Ressources de la map"),
    ("action.shader_editor", "Shader editor...", "Éditeur de shaders..."),
    ("action.vtf_editor", "VTF editor...", "Éditeur de VTF..."),
];
