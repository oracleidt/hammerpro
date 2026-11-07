use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::settings::{Disp2dSetting, RenderModeSetting};
use crate::state::AppState;

action_list! { render_actions {
    ToggleGrid3d => "toggle_grid_3d", Map, MapGrid, sc(NONE, Key::P), Ready;
    FarPlaneCloser => "far_plane_closer", View, ViewNav, None, Ready;
    FarPlaneFarther => "far_plane_farther", View, ViewNav, None, Ready;
    RenderUnshaded => "render_unshaded", View, ViewRender, sc(ALT, Key::Num6), Ready;
    RenderSmoothingGroups => "render_smoothing_groups", View, ViewRender, sc(ALT, Key::Num7), Ready;
    RenderColorCorrected => "render_color_corrected", View, ViewRender, sc(ALT, Key::Num9), Ready;
    ToggleRenderEffects => "toggle_render_effects", View, ViewDisplay, None, Ready;
    ToggleSkyPreview => "toggle_sky_preview", View, ViewDisplay, None, Ready;
    ToggleSkybox3d => "toggle_skybox_3d", View, ViewDisplay, sc(ALT, Key::V), Ready;
    ToggleCollisionModels => "toggle_collision_models", View, ViewDisplay, None, Ready;
    ToggleFogPreview => "toggle_fog_preview", View, ViewDisplay, None, Ready;
    ToggleAnimatedMaterials => "toggle_animated_materials", View, ViewDisplay, None, Ready;
    ToggleModelFade => "toggle_model_fade", View, ViewDisplay, None, Ready;
    ToggleOpaqueMaterials => "toggle_opaque_materials", View, ViewDisplay, None, Ready;
    ToggleWalkable => "toggle_walkable", View, ViewDisplay, None, Ready;
    ToggleDisplacements => "toggle_displacements", View, ViewDisplay, None, Ready;
    BackgroundImages => "background_images", View, ViewShow, None, Ready;
    ToggleRadiusCulling => "toggle_radius_culling", View, ViewDisplay, None, Ready;
    ToggleInvalidBrushes => "toggle_invalid_brushes", View, ViewDisplay, None, Ready;
    UseSkyCameraInGame => "use_sky_camera_in_game", View, ViewDisplay, None, Ready;
    ToggleDispMesh2d => "toggle_disp_mesh_2d", View, ViewDisplay, None, Ready;
    ToggleDispWalkable => "toggle_disp_walkable", View, ViewDisplay, None, Ready;
    ToggleNodraw => "toggle_nodraw", View, ViewDisplay, None, Ready;
    ToggleCollisionOnly => "toggle_collision_only", View, ViewDisplay, None, Ready;
    ToggleWaterPreview => "toggle_water_preview", View, ViewDisplay, None, Ready;
    ToggleDetailPreview => "toggle_detail_preview", View, ViewDisplay, None, Ready;
    ToggleFogVolumes => "toggle_fog_volumes", View, ViewDisplay, None, Ready;
    ToggleFogVolumeTexture => "toggle_fog_volume_texture", View, ViewDisplay, None, Ready;
    ToggleFishPreview => "toggle_fish_preview", View, ViewDisplay, None, Ready;
    ToggleFishSwim => "toggle_fish_swim", View, ViewDisplay, None, Ready;
    ToggleCubemaps => "toggle_cubemaps", View, ViewDisplay, None, Ready;
    ToggleObjectColors2d => "toggle_object_colors_2d", View, ViewDisplay, None, Ready;
    ToggleObjectColors3d => "toggle_object_colors_3d", View, ViewDisplay, None, Ready;
    RebuildCubemaps => "rebuild_cubemaps", Map, MapLighting, None, Ready;
    WriteCubemaps => "write_cubemaps", Map, MapLighting, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[Action::FarPlaneCloser, Action::FarPlaneFarther];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::RebuildCubemaps => Some(state.settings.render.cubemaps),
        Action::WriteCubemaps => Some(state.settings.render.cubemaps && state.assets.play_cubemap_sources().is_some()),
        Action::UseSkyCameraInGame => Some(
            crate::runtime::render::previewed_sky_camera(state)
                .is_some_and(|id| hp_editor::sky::game_sky_camera(&state.doc) != Some(id)),
        ),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    let r = &state.settings.render;
    let mode = state.settings.view.render_mode;
    Some(match action {
        Action::ToggleGrid3d => Check::Toggle(r.grid_3d),
        Action::RenderUnshaded => Check::Radio(mode == RenderModeSetting::Unshaded),
        Action::RenderSmoothingGroups => Check::Radio(mode == RenderModeSetting::SmoothingGroups),
        Action::RenderColorCorrected => Check::Radio(mode == RenderModeSetting::ColorCorrected),
        Action::ToggleRenderEffects => Check::Toggle(r.render_effects),
        Action::ToggleSkyPreview => Check::Toggle(r.sky_preview),
        Action::ToggleSkybox3d => Check::Toggle(r.skybox_3d),
        Action::ToggleCollisionModels => Check::Toggle(r.collision_models),
        Action::ToggleCollisionOnly => Check::Toggle(r.collision_only),
        Action::ToggleFogPreview => Check::Toggle(r.fog_preview),
        Action::ToggleWaterPreview => Check::Toggle(r.water_preview),
        Action::ToggleDetailPreview => Check::Toggle(r.detail_preview),
        Action::ToggleFogVolumes => Check::Toggle(r.fog_volume_preview),
        Action::ToggleFogVolumeTexture => Check::Toggle(r.fog_volume_texture),
        Action::ToggleFishPreview => Check::Toggle(r.fish_preview),
        Action::ToggleFishSwim => Check::Toggle(r.fish_swim),
        Action::ToggleAnimatedMaterials => Check::Toggle(r.animate_materials),
        Action::ToggleModelFade => Check::Toggle(r.model_fade),
        Action::ToggleOpaqueMaterials => Check::Toggle(r.force_opaque),
        Action::ToggleWalkable => Check::Toggle(r.walkable),
        Action::ToggleDisplacements => Check::Toggle(r.show_displacements),
        Action::ToggleRadiusCulling => Check::Toggle(r.radius_culling),
        Action::ToggleInvalidBrushes => Check::Toggle(r.highlight_invalid),
        Action::ToggleDispMesh2d => Check::Toggle(r.displacements_2d != Disp2dSetting::Outline),
        Action::ToggleDispWalkable => Check::Toggle(r.disp_walkable),
        Action::ToggleNodraw => Check::Toggle(!r.hide_nodraw),
        Action::ToggleCubemaps => Check::Toggle(r.cubemaps),
        Action::ToggleObjectColors2d => Check::Toggle(r.object_colors_2d),
        Action::ToggleObjectColors3d => Check::Toggle(r.object_colors_3d),
        _ => return None,
    })
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::ToggleGrid3d => Some("⊞"),
        Action::ToggleSkyPreview | Action::ToggleSkybox3d => Some("☁"),
        Action::ToggleDispMesh2d => Some("#"),
        Action::ToggleDispWalkable => Some("🚶"),
        Action::ToggleNodraw => Some("🚫"),
        Action::ToggleCollisionOnly => Some("🔲"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.toggle_cubemaps", "Cubemap reflections", "Reflets des cubemaps"),
    ("action.toggle_object_colors_2d", "Object colors in 2D", "Couleurs des objets en 2D"),
    (
        "action.toggle_object_colors_3d",
        "Object colors in 3D wireframe and flat",
        "Couleurs des objets en 3D filaire et plat",
    ),
    ("action.rebuild_cubemaps", "Rebuild cubemaps", "Reconstruire les cubemaps"),
    ("action.write_cubemaps", "Write cubemaps into the compiled map", "Écrire les cubemaps dans la map compilée"),
    ("action.toggle_nodraw", "Show nodraw faces", "Afficher le nodraw"),
    ("action.toggle_grid_3d", "3D grid on faces", "Grille 3D sur les faces"),
    ("action.far_plane_closer", "Shorter draw distance", "Distance d'affichage plus courte"),
    ("action.far_plane_farther", "Longer draw distance", "Distance d'affichage plus longue"),
    ("action.render_unshaded", "Textured, unshaded", "Texturé sans ombrage"),
    ("action.render_smoothing_groups", "Smoothing groups", "Groupes de lissage"),
    ("action.render_color_corrected", "3D Color Correction", "3D Correction colorimétrique"),
    ("action.toggle_render_effects", "Render effects", "Effets de rendu"),
    ("action.toggle_sky_preview", "Sky preview", "Aperçu du ciel"),
    ("action.toggle_skybox_3d", "3D skybox", "Skybox 3D"),
    ("action.toggle_collision_models", "Collision models", "Modèles de collision"),
    ("action.toggle_collision_only", "Show model collisions", "Afficher les collisions des modèles"),
    ("action.toggle_fog_preview", "Fog preview", "Aperçu du brouillard"),
    ("action.toggle_water_preview", "Water preview", "Aperçu de l'eau"),
    ("action.toggle_detail_preview", "Detail props preview (grass)", "Aperçu des detail props (herbe)"),
    ("action.toggle_fog_volumes", "Fog volume preview", "Aperçu des fog_volume"),
    ("action.toggle_fog_volume_texture", "Fog volume texture", "Texture des fog_volume"),
    ("action.toggle_fish_preview", "Fish preview", "Aperçu des poissons"),
    ("action.toggle_fish_swim", "Swimming fish", "Poissons qui nagent"),
    ("action.toggle_animated_materials", "Animated materials", "Matériaux animés"),
    ("action.toggle_model_fade", "Model fade", "Fondu des modèles"),
    ("action.toggle_opaque_materials", "Opaque materials", "Matériaux opaques"),
    ("action.toggle_walkable", "Walkable slopes", "Pentes praticables"),
    ("action.toggle_displacements", "Show displacements", "Afficher les displacements"),
    ("action.background_images", "Background images...", "Images de fond..."),
    ("action.toggle_radius_culling", "Radius culling", "Limite de distance circulaire"),
    ("action.toggle_invalid_brushes", "Highlight invalid brushes", "Surligner les brushes invalides"),
    ("shell.undo.edit_background_images", "Edit background images", "Modifier les images de fond"),
    ("action.use_sky_camera_in_game", "Use the previewed sky camera in game", "Utiliser en jeu la sky_camera affichée"),
    ("shell.undo.use_sky_camera_in_game", "Use sky camera in game", "Utilisation d'une sky_camera en jeu"),
    ("action.toggle_disp_mesh_2d", "Displacement mesh in 2D views", "Maillage des displacements en vue 2D"),
    ("action.toggle_disp_walkable", "Displacement walkability", "Praticabilité des displacements"),
];
