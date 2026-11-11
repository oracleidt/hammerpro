use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::settings::RenderModeSetting;
use crate::state::AppState;

action_list! { lighting_actions {
    RenderLightmapGrid => "render_lightmap_grid", View, ViewRender, sc(ALT, Key::Num8), Ready;
    RenderBaked => "render_baked", View, ViewRender, sc(CTRL, Key::F5), Ready;
    BakeLighting => "bake_lighting", Map, MapLighting, None, Ready;
    BakeAroundCamera => "bake_around_camera", Map, MapLighting, None, Ready;
    StopBake => "stop_bake", Map, MapLighting, None, Ready;
    SelectTinyLightmapFaces => "select_tiny_lightmap_faces", Map, MapLights, None, Ready;
    ShowLighting => "show_lighting", View, ViewPanels, None, Ready;
    SoloSelectedLights => "solo_selected_lights", Map, MapLights, None, Ready;
    MuteSelectedLights => "mute_selected_lights", Map, MapLights, None, Ready;
    UnmuteAllLights => "unmute_all_lights", Map, MapLights, None, Ready;
    MeasureLightCosts => "measure_light_costs", Map, MapLights, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::StopBake => Some(state.runtime.lighting.progress.is_some()),
        Action::UnmuteAllLights => Some(!state.doc.light_plus().excluded_entities.is_empty()),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    let mode = state.settings.view.render_mode;
    match action {
        Action::RenderBaked => Some(Check::Radio(mode == RenderModeSetting::Baked)),
        Action::RenderLightmapGrid => Some(Check::Radio(mode == RenderModeSetting::LightmapGrid)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::RenderBaked | Action::ShowLighting => Some("💡"),
        Action::RenderLightmapGrid => Some("⊞"),
        Action::BakeLighting | Action::BakeAroundCamera => Some("☀"),
        Action::StopBake => Some("⏹"),
        Action::SelectTinyLightmapFaces => Some("🔍"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.render_lightmap_grid", "Lightmap grid", "Grille de lightmap"),
    ("action.render_baked", "Baked lighting preview", "Aperçu de l'éclairage précalculé"),
    ("action.bake_lighting", "Bake lighting", "Précalculer l'éclairage"),
    ("action.bake_around_camera", "Bake around the camera", "Précalculer autour de la caméra"),
    ("action.stop_bake", "Stop baking", "Arrêter le précalcul"),
    (
        "action.select_tiny_lightmap_faces",
        "Select faces with a tiny lightmap scale",
        "Sélectionner les faces à petite échelle de lightmap",
    ),
    ("action.show_lighting", "Lighting", "Éclairage"),
    ("action.solo_selected_lights", "Solo the selected lights", "Isoler les lumières sélectionnées"),
    ("action.mute_selected_lights", "Mute the selected lights", "Couper les lumières sélectionnées"),
    ("action.unmute_all_lights", "Unmute all lights", "Réactiver toutes les lumières"),
    ("action.measure_light_costs", "Measure the cost of each light", "Mesurer le coût de chaque lumière"),
    ("shell.undo.edit_lighting_options", "Edit lighting options", "Modifier les options d'éclairage"),
    ("shell.undo.edit_color_correction", "Edit color correction", "Modifier la correction des couleurs"),
];
