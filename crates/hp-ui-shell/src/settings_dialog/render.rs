use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::{
    Disp2dSetting, Disp2dViewSetting, Models2dSetting, PropShadowSetting, RenderSettings, StaticPropShadows,
};
use hp_app::{AppState, t, tf};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(RenderPage)]
}

struct RenderPage;

fn check(ui: &mut egui::Ui, value: &mut bool, label: &str, tip: &str) -> bool {
    ui.checkbox(value, label).on_hover_text(tip).changed()
}

impl SettingsPage for RenderPage {
    fn id(&self) -> &'static str {
        "render"
    }

    fn label(&self) -> String {
        t!("shell.render.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "🎨"
    }

    fn order(&self) -> i32 {
        page_order::VIEWPORT + 10
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        let compiled = state.settings.compile.static_prop_shadows();
        let r = &mut state.settings.render;
        heading(ui, t!("shell.render.view_3d"));
        egui::Grid::new("hp_shell_render_3d").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.render.near_plane"));
            changed |= ui
                .add(egui::DragValue::new(&mut r.near_plane).range(0.5..=16.0).speed(0.1).fixed_decimals(1))
                .on_hover_text(t!("shell.render.near_plane_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.render.model_distance"));
            changed |= ui
                .add(egui::DragValue::new(&mut r.model_distance).range(0.0..=65536.0).speed(16.0))
                .on_hover_text(t!("shell.render.model_distance_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.render.grid_3d_opacity"));
            changed |= ui.add(egui::Slider::new(&mut r.grid_3d_opacity, 0.05..=1.0)).changed();
            ui.end_row();
            ui.label(t!("shell.render.cull_radius"));
            changed |= ui
                .add(egui::DragValue::new(&mut r.cull_radius).range(256.0..=65536.0).speed(32.0))
                .on_hover_text(t!("shell.render.cull_radius_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.render.light_shadows"));
            changed |= ui
                .add(egui::DragValue::new(&mut r.light_shadows).range(0..=hp_render::MAX_SHADOWED_LIGHTS))
                .on_hover_text(t!("shell.render.light_shadows_tip"))
                .changed();
            ui.end_row();
        });
        changed |= check(ui, &mut r.grid_3d, t!("shell.render.grid_3d"), t!("shell.render.grid_3d_tip"));
        changed |=
            check(ui, &mut r.radius_culling, t!("shell.render.radius_culling"), t!("shell.render.cull_radius_tip"));
        changed |= check(ui, &mut r.occlusion_culling, t!("shell.render.occlusion"), t!("shell.render.occlusion_tip"));
        changed |= check(ui, &mut r.sky_preview, t!("shell.render.sky"), t!("shell.render.sky_tip"));
        changed |= check(ui, &mut r.skybox_3d, t!("shell.render.skybox_3d"), t!("shell.render.skybox_3d_tip"));
        changed |= check(
            ui,
            &mut r.sky_see_through,
            t!("shell.render.sky_see_through"),
            t!("shell.render.sky_see_through_tip"),
        );
        changed |= check(ui, &mut r.water_preview, t!("shell.render.water"), t!("shell.render.water_tip"));
        changed |= check(ui, &mut r.normal_maps, t!("shell.render.normal_maps"), t!("shell.render.normal_maps_tip"));
        changed |= check(
            ui,
            &mut r.light_preview_hdr,
            t!("shell.render.light_preview_hdr"),
            t!("shell.render.light_preview_hdr_tip"),
        );
        changed |=
            check(ui, &mut r.sky_occlusion, t!("shell.render.sky_occlusion"), t!("shell.render.sky_occlusion_tip"));
        ui.horizontal(|ui| {
            ui.label(t!("shell.render.prop_shadows")).on_hover_text(t!("shell.render.prop_shadows_tip"));
            egui::ComboBox::from_id_salt("hp_shell_render_prop_shadows")
                .selected_text(prop_shadows_label(r.prop_shadows, compiled))
                .show_ui(ui, |ui| {
                    for m in PropShadowSetting::ALL {
                        changed |= ui
                            .selectable_value(&mut r.prop_shadows, m, prop_shadows_label(m, compiled))
                            .on_hover_text(t!("shell.render.prop_shadows_tip"))
                            .changed();
                    }
                });
        });
        changed |= check(ui, &mut r.cubemaps, t!("shell.render.cubemaps"), t!("shell.render.cubemaps_tip"));
        ui.add_enabled_ui(r.cubemaps, |ui| {
            changed |=
                check(ui, &mut r.cubemaps_auto, t!("shell.render.cubemaps_auto"), t!("shell.render.cubemaps_auto_tip"));
        });
        changed |= check(ui, &mut r.fog_preview, t!("shell.render.fog"), t!("shell.render.fog_tip"));
        changed |=
            check(ui, &mut r.fog_volume_preview, t!("shell.render.fog_volumes"), t!("shell.render.fog_volumes_tip"));
        changed |= check(
            ui,
            &mut r.fog_volume_texture,
            t!("shell.render.fog_volume_texture"),
            t!("shell.render.fog_volume_texture_tip"),
        );
        changed |= check(ui, &mut r.fish_preview, t!("shell.render.fish"), t!("shell.render.fish_tip"));
        ui.add_enabled_ui(r.fish_preview, |ui| {
            changed |= check(ui, &mut r.fish_swim, t!("shell.render.fish_swim"), t!("shell.render.fish_swim_tip"));
        });
        changed |= check(
            ui,
            &mut r.player_compiled_lighting,
            t!("shell.render.player_lighting"),
            t!("shell.render.player_lighting_tip"),
        );
        changed |= check(ui, &mut r.render_effects, t!("shell.render.effects"), t!("shell.render.effects_tip"));
        changed |= check(ui, &mut r.model_fade, t!("shell.render.model_fade"), t!("shell.render.model_fade_tip"));
        changed |= check(ui, &mut r.force_opaque, t!("shell.render.opaque"), t!("shell.render.opaque_tip"));
        changed |= check(ui, &mut r.walkable, t!("shell.render.walkable"), t!("shell.render.walkable_tip"));
        changed |= check(ui, &mut r.selection_outline, t!("shell.render.outline"), t!("shell.render.outline_tip"));
        changed |= check(ui, &mut r.collision_models, t!("shell.render.collision"), t!("shell.render.collision_tip"));

        heading(ui, t!("shell.render.views_2d"));
        egui::Grid::new("hp_shell_render_2d").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.render.models_2d"));
            egui::ComboBox::from_id_salt("hp_shell_render_models_2d")
                .selected_text(models_2d_label(r.models_in_2d))
                .show_ui(ui, |ui| {
                    for m in [Models2dSetting::SelectedOnly, Models2dSetting::Wireframe, Models2dSetting::Boxes] {
                        changed |= ui.selectable_value(&mut r.models_in_2d, m, models_2d_label(m)).changed();
                    }
                });
            ui.end_row();
            ui.label(t!("shell.render.fill_opacity"));
            changed |= ui.add(egui::Slider::new(&mut r.fill_selected_2d_opacity, 0.02..=0.6)).changed();
            ui.end_row();
            ui.label(t!("shell.render.icon_size"));
            changed |=
                ui.add(egui::Slider::new(&mut r.icons_in_2d_px, 8.0..=48.0).step_by(1.0).suffix(" px")).changed();
            ui.end_row();
            ui.label(t!("shell.render.grid_min_px"));
            changed |= ui
                .add(egui::Slider::new(&mut r.grid_min_px, 2.0..=32.0).step_by(1.0).suffix(" px"))
                .on_hover_text(t!("shell.render.grid_min_px_tip"))
                .changed();
            ui.end_row();
        });
        changed |= check(ui, &mut r.fill_selected_2d, t!("shell.render.fill"), t!("shell.render.fill_tip"));
        changed |= check(ui, &mut r.vertex_dots_2d, t!("shell.render.dots"), t!("shell.render.dots_tip"));
        changed |= check(ui, &mut r.icons_in_2d, t!("shell.render.icons"), t!("shell.render.icons_tip"));
        changed |= check(ui, &mut r.camera_in_2d, t!("shell.render.camera"), t!("shell.render.camera_tip"));
        changed |= check(ui, &mut r.grid_dotted, t!("shell.render.grid_dotted"), t!("shell.render.grid_dotted_tip"));
        changed |= check(ui, &mut r.grid_highlight_64, t!("shell.render.grid_64"), t!("shell.render.grid_64_tip"));
        changed |= check(
            ui,
            &mut r.use_visgroup_colors,
            t!("shell.render.visgroup_colors"),
            t!("shell.render.visgroup_colors_tip"),
        );
        changed |= check(
            ui,
            &mut r.object_colors_2d,
            t!("shell.render.object_colors_2d"),
            t!("shell.render.object_colors_2d_tip"),
        );
        changed |= check(
            ui,
            &mut r.object_colors_3d,
            t!("shell.render.object_colors_3d"),
            t!("shell.render.object_colors_3d_tip"),
        );

        heading(ui, t!("shell.render.tools"));
        ui.horizontal_wrapped(|ui| {
            changed |= ui.checkbox(&mut r.hide_nodraw, t!("shell.render.hide_nodraw")).changed();
            changed |= ui.checkbox(&mut r.hide_clips, t!("shell.render.hide_clips")).changed();
            changed |= ui.checkbox(&mut r.hide_triggers, t!("shell.render.hide_triggers")).changed();
            changed |= ui.checkbox(&mut r.hide_hints, t!("shell.render.hide_hints")).changed();
        });
        heading(ui, t!("shell.render.brushes"));
        changed |= check(ui, &mut r.highlight_invalid, t!("shell.render.invalid"), t!("shell.render.invalid_tip"));
        changed |= check(ui, &mut r.show_displacements, t!("shell.render.disps"), t!("shell.render.disps_tip"));
        changed |= check(ui, &mut r.displacements_in_2d, t!("shell.render.disps_2d"), t!("shell.render.disps_2d_tip"));
        changed |=
            check(ui, &mut r.displacements_only, t!("shell.render.disps_only"), t!("shell.render.disps_only_tip"));
        egui::Grid::new("hp_shell_render_disps").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.render.disps_2d_mode")).on_hover_text(t!("shell.render.disps_2d_mode_tip"));
            egui::ComboBox::from_id_salt("hp_shell_render_disps_2d")
                .selected_text(disp_2d_label(r.displacements_2d))
                .show_ui(ui, |ui| {
                    for m in Disp2dSetting::ALL {
                        changed |= ui.selectable_value(&mut r.displacements_2d, m, disp_2d_label(m)).changed();
                    }
                });
            ui.end_row();
        });
        if r.displacements_2d_views.iter().any(|v| *v != Disp2dViewSetting::Global)
            && ui
                .button(t!("shell.render.disps_2d_reset"))
                .on_hover_text(t!("shell.render.disps_2d_reset_tip"))
                .clicked()
        {
            r.displacements_2d_views = [Disp2dViewSetting::Global; 4];
            changed = true;
        }
        changed |=
            check(ui, &mut r.disp_source_2d, t!("shell.render.disp_source_2d"), t!("shell.render.disp_source_2d_tip"));
        changed |=
            check(ui, &mut r.disp_walkable, t!("shell.render.disp_walkable"), t!("shell.render.disp_walkable_tip"));

        heading(ui, t!("shell.render.frame_rate"));
        egui::Grid::new("hp_shell_render_fps").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.render.max_fps"));
            changed |= ui
                .add(egui::DragValue::new(&mut r.max_fps).range(0..=1000))
                .on_hover_text(t!("shell.render.max_fps_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.render.unfocused_fps"));
            changed |= ui
                .add(egui::DragValue::new(&mut r.unfocused_fps).range(0..=1000))
                .on_hover_text(t!("shell.render.unfocused_fps_tip"))
                .changed();
            ui.end_row();
        });
        changed |= check(ui, &mut r.vsync, t!("shell.render.vsync"), t!("shell.render.vsync_tip"));

        heading(ui, t!("shell.render.vram"));
        egui::Grid::new("hp_shell_render_vram").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.render.vram_budget"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut r.vram_budget_mb)
                        .range(0..=hp_app::settings::MAX_VRAM_BUDGET_MB)
                        .speed(16.0)
                        .suffix(" MB")
                        .custom_formatter(|v, _| {
                            if v < 0.5 { t!("shell.render.vram_auto").to_owned() } else { format!("{v:.0} MB") }
                        }),
                )
                .on_hover_text(t!("shell.render.vram_budget_tip"))
                .changed();
            ui.end_row();
        });
        changed |= check(
            ui,
            &mut r.keep_background_maps_on_gpu,
            t!("shell.render.vram_keep_tabs"),
            t!("shell.render.vram_keep_tabs_tip"),
        );
        let g = &state.runtime.gpu;
        if g.budget > 0 {
            let source = if g.source == hp_app::runtime::VramSource::Driver {
                t!("shell.render.vram_driver")
            } else {
                t!("shell.render.vram_estimate")
            };
            ui.label(egui::RichText::new(tf!("shell.render.vram_now", g.usage >> 20, g.budget >> 20, source)).weak());
        }
        if ui.button(t!("shell.render.vram_free")).on_hover_text(t!("shell.render.vram_free_tip")).clicked() {
            state.runtime.gpu.release_request = Some(false);
        }
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.render = RenderSettings::default();
        state.settings_dirty = true;
    }
}

fn disp_2d_label(m: Disp2dSetting) -> &'static str {
    match m {
        Disp2dSetting::Mesh => t!("shell.render.disps_mesh"),
        Disp2dSetting::SelectedMesh => t!("shell.render.disps_selected"),
        Disp2dSetting::Outline => t!("shell.render.disps_outline"),
    }
}

fn prop_shadows_label(m: PropShadowSetting, compiled: StaticPropShadows) -> String {
    let shape = |s: StaticPropShadows| match s {
        StaticPropShadows::None => t!("shell.render.prop_shadows_none"),
        StaticPropShadows::Collision => t!("shell.render.prop_shadows_collision"),
        StaticPropShadows::Mesh => t!("shell.render.prop_shadows_mesh"),
    };
    match m {
        PropShadowSetting::Auto => tf!("shell.render.prop_shadows_auto", shape(compiled)),
        PropShadowSetting::Mesh => shape(StaticPropShadows::Mesh).to_owned(),
        PropShadowSetting::Collision => shape(StaticPropShadows::Collision).to_owned(),
    }
}

fn models_2d_label(m: Models2dSetting) -> &'static str {
    match m {
        Models2dSetting::Boxes => t!("shell.render.models_boxes"),
        Models2dSetting::Wireframe => t!("shell.render.models_wire"),
        Models2dSetting::SelectedOnly => t!("shell.render.models_selected"),
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.render.page", "Rendering", "Rendu"),
    ("shell.render.view_3d", "3D view", "Vue 3D"),
    ("shell.render.near_plane", "Near plane", "Plan proche"),
    (
        "shell.render.near_plane_tip",
        "Closest distance the 3D view draws (raise it if far surfaces flicker)",
        "Distance la plus proche affichée par la vue 3D (augmentez-la si des surfaces lointaines scintillent)",
    ),
    ("shell.render.model_distance", "Model distance", "Distance des modèles"),
    (
        "shell.render.model_distance_tip",
        "Models farther than this are drawn as boxes (0: no limit)",
        "Les modèles plus loin que cela sont dessinés en boîtes (0 : aucune limite)",
    ),
    ("shell.render.grid_3d", "3D grid on faces (P)", "Grille 3D sur les faces (P)"),
    (
        "shell.render.grid_3d_tip",
        "Draws the grid on the faces of brushes, every grid step, brighter every major step",
        "Dessine la grille sur les faces des brushes, à chaque pas de grille, plus claire à chaque pas majeur",
    ),
    ("shell.render.grid_3d_opacity", "3D grid opacity", "Opacité de la grille 3D"),
    ("shell.render.cull_radius", "Culling radius", "Rayon de limite"),
    (
        "shell.render.cull_radius_tip",
        "Radius culling: nothing farther than this from the camera is drawn (circle in 2D views)",
        "Limite circulaire : rien n'est dessiné au-delà de ce rayon autour de la caméra (cercle dans les vues 2D)",
    ),
    ("shell.render.radius_culling", "Radius culling", "Limite de distance circulaire"),
    ("shell.render.light_shadows", "Shadowed lights", "Lumières avec ombres"),
    ("shell.render.light_preview_hdr", "HDR light values", "Valeurs de lumière HDR"),
    (
        "shell.render.light_preview_hdr_tip",
        "Lit mode: the lights use their HDR values (_lightHDR, _lightscaleHDR, _ambientHDR, _AmbientScaleHDR) like \
         the game on a map compiled with HDR. Off: the LDR values (_light, _ambient).",
        "Mode éclairé : les lumières prennent leurs valeurs HDR (_lightHDR, _lightscaleHDR, _ambientHDR, \
         _AmbientScaleHDR) comme le jeu sur une map compilée en HDR. Désactivé : les valeurs LDR (_light, _ambient).",
    ),
    ("shell.render.sky_occlusion", "Sky light only under the sky", "Lumière du ciel seulement sous le ciel"),
    ("shell.render.prop_shadows", "Static prop shadows", "Ombres des props statiques"),
    (
        "shell.render.prop_shadows_tip",
        "Lit mode: what the prop_static entities cast shadows with. Auto follows the compile: the triangles they draw \
         when the lighting step runs with -StaticPropPolys, their collision model otherwise (like VRAD), none with \
         -nostaticpropshadows. Props with disableshadows 1 never cast.",
        "Mode éclairé : ce avec quoi les prop_static projettent leurs ombres. Auto suit la compilation : les \
         triangles qu'ils dessinent quand l'étape d'éclairage tourne avec -StaticPropPolys, leur modèle de collision \
         sinon (comme VRAD), aucune avec -nostaticpropshadows. Les props avec disableshadows 1 n'en projettent jamais.",
    ),
    ("shell.render.prop_shadows_auto", "Auto ({0})", "Auto ({0})"),
    ("shell.render.prop_shadows_mesh", "Model triangles", "Triangles du modèle"),
    ("shell.render.prop_shadows_collision", "Collision model", "Modèle de collision"),
    ("shell.render.prop_shadows_none", "None", "Aucune"),
    (
        "shell.render.sky_occlusion_tip",
        "Lit mode: the ambient light of the light_environment (_ambient) only reaches what sees the sky, like VRAD: \
         interiors and covered places get none, walls get half of it. Off: the whole map gets it.",
        "Mode éclairé : la lumière ambiante du light_environment (_ambient) n'atteint que ce qui voit le ciel, comme \
         VRAD : les intérieurs et les endroits couverts n'en reçoivent pas, les murs en reçoivent la moitié. \
         Désactivé : toute la map la reçoit.",
    ),
    (
        "shell.render.light_shadows_tip",
        "Lit mode: how many point and spot lights near the camera cast shadows (0: only the sun)",
        "Mode éclairé : nombre de lumières ponctuelles et spots proches de la caméra qui projettent des ombres (0 : le soleil seulement)",
    ),
    ("shell.render.occlusion", "Skip hidden geometry", "Ignorer la géométrie cachée"),
    (
        "shell.render.occlusion_tip",
        "Parts of the map whose objects are all hidden are not drawn at all",
        "Les parties de la carte dont tous les objets sont masqués ne sont pas dessinées du tout",
    ),
    ("shell.render.sky", "Sky preview", "Aperçu du ciel"),
    (
        "shell.render.sky_tip",
        "Draws the sky of the map (skyname) behind the 3D view; sky faces show it",
        "Dessine le ciel de la carte (skyname) derrière la vue 3D ; les faces de ciel le laissent voir",
    ),
    ("shell.render.skybox_3d", "3D skybox (Alt+V)", "Skybox 3D (Alt+V)"),
    ("shell.render.sky_see_through", "See the sky through toolsskybox", "Voir le ciel à travers toolsskybox"),
    (
        "shell.render.sky_see_through_tip",
        "Hides the toolsskybox faces so the sky previews show through them. Off: they keep their tool texture.",
        "Masque les faces toolsskybox pour voir le ciel à travers. Désactivé : elles gardent leur texture d'outil.",
    ),
    (
        "shell.render.skybox_3d_tip",
        "Draws the room around the sky_camera at its scale around the map, like in game",
        "Dessine la pièce autour de la sky_camera à son échelle autour de la carte, comme en jeu",
    ),
    ("shell.render.water", "Water preview", "Aperçu de l'eau"),
    (
        "shell.render.water_tip",
        "Draws water like in game: refraction of what is under it, water fog, reflection of the scene and the sky, \
         moving waves. Off: the water shows its tool texture.",
        "Dessine l'eau comme en jeu : réfraction de ce qui est dessous, brouillard de l'eau, reflet de la scène et \
         du ciel, vagues animées. Désactivé : l'eau montre sa texture d'outil.",
    ),
    ("shell.render.normal_maps", "Relief (normal maps)", "Relief (normal maps)"),
    (
        "shell.render.normal_maps_tip",
        "The normal maps of the materials ($bumpmap, $ssbump) give the surfaces their relief in the 3D view: \
         editor shading, lighting preview, baked lighting and player view with their bumped lightmaps, props and model browser. Off: flat \
         surfaces and less video memory.",
        "Les normal maps des matériaux ($bumpmap, $ssbump) donnent leur relief aux surfaces dans la vue 3D : \
         ombrage de l'éditeur, aperçu de l'éclairage, éclairage précalculé et vue joueur avec leurs lightmaps bumpées, props et navigateur de modèles. \
         Désactivé : surfaces plates et moins de mémoire vidéo.",
    ),
    ("shell.render.cubemaps", "Cubemap reflections", "Reflets des cubemaps"),
    (
        "shell.render.cubemaps_tip",
        "Materials with $envmap reflect their cubemap in the 3D view (textured, lit and baked modes) and the \
         player view: the editor draws a cubemap at every env_cubemap (the sky without any), each face takes the \
         nearest env_cubemap in front of it like the compiler, props the nearest one. The player view uses the \
         cubemaps built in game when the compiled map has them. Map > Rebuild cubemaps draws them again.",
        "Les matériaux avec $envmap reflètent leur cubemap dans la vue 3D (modes texturé, éclairé et précalculé) \
         et la vue joueur : l'éditeur dessine une cubemap à chaque env_cubemap (le ciel s'il n'y en a aucun), \
         chaque face prend l'env_cubemap le plus proche devant elle comme le compilateur, les props le plus \
         proche. La vue joueur utilise les cubemaps construites en jeu quand la map compilée en a. Carte > \
         Reconstruire les cubemaps les redessine.",
    ),
    (
        "shell.render.cubemaps_auto",
        "Redraw the cubemaps after edits",
        "Redessiner les cubemaps après les modifications",
    ),
    (
        "shell.render.cubemaps_auto_tip",
        "The cubemaps are drawn again on their own a moment after the map, its lighting or the view mode change, \
         a few faces a frame. Off: only with Map > Rebuild cubemaps (new and moved env_cubemap entities still \
         get theirs).",
        "Les cubemaps sont redessinées d'elles-mêmes un instant après un changement de la map, de son éclairage \
         ou du mode de la vue, quelques faces par image. Désactivé : seulement avec Carte > Reconstruire les \
         cubemaps (les env_cubemap nouveaux ou déplacés ont quand même la leur).",
    ),
    ("shell.render.fog", "Fog preview", "Aperçu du brouillard"),
    (
        "shell.render.fog_tip",
        "Applies the fog of an env_fog_controller to the 3D views and the player view, live from its keys \
         (colors, blend, start, end, max density, Fog Enable, Far Z Clip). Choose the controller in View > Fog \
         preview; one with Fog Enable at No gives no fog, like in game.",
        "Applique le brouillard d'un env_fog_controller aux vues 3D et à la vue joueur, en direct depuis ses \
         clés (couleurs, mélange, début, fin, densité max, Fog Enable, Far Z Clip). Choisissez le contrôleur dans \
         Affichage > Aperçu du brouillard ; avec Fog Enable à No il n'y a pas de brouillard, comme en jeu.",
    ),
    ("shell.render.fog_volumes", "Fog volume preview", "Aperçu des fog_volume"),
    (
        "shell.render.fog_volumes_tip",
        "Fills each fog_volume with its fog: the one of the env_fog_controller its Fog Name points to, or a \
         default fog look with a warning label when that controller is missing or has its fog disabled. The \
         label above the volume also shows its Post Process and Color Correction links. In the player view \
         the fog of the controller replaces the map fog while the eyes are inside the volume, like in game.",
        "Remplit chaque fog_volume de son brouillard : celui de l'env_fog_controller que vise son Fog Name, \
         ou un brouillard par défaut avec une étiquette d'avertissement quand ce contrôleur manque ou a son \
         brouillard désactivé. L'étiquette au-dessus du volume montre aussi ses liens Post Process et Color \
         Correction. Dans la vue joueur le brouillard du contrôleur remplace celui de la carte tant que les \
         yeux sont dans le volume, comme en jeu.",
    ),
    ("shell.render.fog_volume_texture", "Fog volume texture", "Texture des fog_volume"),
    (
        "shell.render.fog_volume_texture_tip",
        "Draws the material of the fog_volume brushes (tools/toolsfog, tools/toolstrigger...) in the 3D view, \
         like Hammer. Off: only the fog shows.",
        "Dessine le matériau des brushes fog_volume (tools/toolsfog, tools/toolstrigger...) dans la vue 3D, \
         comme Hammer. Désactivé : seul le brouillard apparaît.",
    ),
    ("shell.render.fish", "Fish preview", "Aperçu des poissons"),
    (
        "shell.render.fish_tip",
        "Draws the fish of each func_fish_pool: its model fish_count times, scattered within max_range of the \
         entity and kept under the water surface.",
        "Dessine les poissons de chaque func_fish_pool : son modèle fish_count fois, dispersé à moins de \
         max_range de l'entité et gardé sous la surface de l'eau.",
    ),
    ("shell.render.fish_swim", "Swimming fish", "Poissons qui nagent"),
    (
        "shell.render.fish_swim_tip",
        "The fish swim slow circles around their place. Off: they stay still.",
        "Les poissons nagent en cercles lents autour de leur place. Désactivé : ils restent immobiles.",
    ),
    ("shell.render.player_lighting", "Player view: compiled lighting", "Vue joueur : éclairage compilé"),
    (
        "shell.render.player_lighting_tip",
        "The player view lights the compiled map with its lightmaps and the props with their baked lighting, like Garry's Mod (L in the view)",
        "La vue joueur éclaire la map compilée avec ses lightmaps et les props avec leur éclairage pré-calculé, comme Garry's Mod (L dans la vue)",
    ),
    ("shell.render.effects", "Render effects", "Effets de rendu"),
    (
        "shell.render.effects_tip",
        "Shows render amount and color, body groups and default animations of entities",
        "Montre l'opacité et la couleur de rendu, les groupes de corps et les animations par défaut des entités",
    ),
    ("shell.render.model_fade", "Model fade", "Fondu des modèles"),
    (
        "shell.render.model_fade_tip",
        "Props fade out between their fade distances, like in game",
        "Les props disparaissent entre leurs distances de fondu, comme en jeu",
    ),
    ("shell.render.opaque", "Opaque materials", "Matériaux opaques"),
    (
        "shell.render.opaque_tip",
        "Draws translucent materials opaque",
        "Dessine les matériaux translucides comme opaques",
    ),
    ("shell.render.walkable", "Walkable slopes", "Pentes praticables"),
    (
        "shell.render.walkable_tip",
        "Tints the surfaces too steep for players to stand on",
        "Teinte les surfaces trop pentues pour que les joueurs s'y tiennent",
    ),
    ("shell.render.outline", "Selection outline", "Contour de la sélection"),
    (
        "shell.render.outline_tip",
        "Selected objects get an outline in the selection color, visible through walls",
        "Les objets sélectionnés ont un contour de la couleur de sélection, visible à travers les murs",
    ),
    ("shell.render.collision", "Collision models", "Modèles de collision"),
    (
        "shell.render.collision_tip",
        "Draws the collision hull of props over them (also in the model browser preview)",
        "Dessine l'enveloppe de collision des props par-dessus (aussi dans l'aperçu du navigateur de modèles)",
    ),
    ("shell.render.views_2d", "2D views", "Vues 2D"),
    ("shell.render.models_2d", "Models", "Modèles"),
    ("shell.render.models_boxes", "Boxes", "Boîtes"),
    ("shell.render.models_wire", "Wireframe", "Fil de fer"),
    ("shell.render.models_selected", "Wireframe when selected", "Fil de fer si sélectionné"),
    ("shell.render.fill", "Fill selected brushes", "Remplir les brushes sélectionnés"),
    (
        "shell.render.fill_tip",
        "Selected brushes are filled with the selection color in 2D views",
        "Les brushes sélectionnés sont remplis de la couleur de sélection dans les vues 2D",
    ),
    ("shell.render.fill_opacity", "Fill opacity", "Opacité du remplissage"),
    ("shell.render.dots", "Vertex dots", "Points des sommets"),
    (
        "shell.render.dots_tip",
        "Brush vertices are drawn as dots when zoomed in",
        "Les sommets des brushes sont dessinés en points quand on zoome",
    ),
    ("shell.render.icons", "Entity icons", "Icônes des entités"),
    (
        "shell.render.icons_tip",
        "Entity icons and sprites are drawn in 2D views too",
        "Les icônes et sprites des entités sont aussi dessinés dans les vues 2D",
    ),
    ("shell.render.icon_size", "Icon size", "Taille des icônes"),
    ("shell.render.camera", "3D camera", "Caméra 3D"),
    (
        "shell.render.camera_tip",
        "Shows where the 3D camera is and where it looks",
        "Montre où se trouve la caméra 3D et où elle regarde",
    ),
    ("shell.render.grid_min_px", "Grid density limit", "Densité maximale de la grille"),
    (
        "shell.render.grid_min_px_tip",
        "Grid lines closer than this are hidden (the next larger step shows instead)",
        "Les lignes de grille plus proches que cela sont masquées (le pas supérieur s'affiche à la place)",
    ),
    ("shell.render.grid_dotted", "Dotted grid", "Grille en points"),
    (
        "shell.render.grid_dotted_tip",
        "The 2D grid is drawn as dots where its lines cross",
        "La grille 2D est dessinée en points aux croisements de ses lignes",
    ),
    ("shell.render.grid_64", "Highlight every 64 units", "Surligner tous les 64 unités"),
    (
        "shell.render.grid_64_tip",
        "Lines every 64 units are brighter in 2D views",
        "Les lignes tous les 64 unités sont plus claires dans les vues 2D",
    ),
    ("shell.render.visgroup_colors", "Visgroup colors", "Couleurs des groupes de visibilité"),
    (
        "shell.render.visgroup_colors_tip",
        "Brushes are drawn in the color of their visgroup",
        "Les brushes sont dessinés de la couleur de leur groupe de visibilité",
    ),
    ("shell.render.object_colors_2d", "Use object colors in 2D", "Couleurs des objets en 2D"),
    (
        "shell.render.object_colors_2d_tip",
        "Draw brushes and entities in the display color chosen for them; off: the classic look",
        "Dessine les brushes et entités dans la couleur d'affichage choisie ; désactivé : l'aspect classique",
    ),
    (
        "shell.render.object_colors_3d",
        "Object colors in 3D wireframe and flat",
        "Couleurs des objets en 3D filaire et plat",
    ),
    (
        "shell.render.object_colors_3d_tip",
        "The 3D view in wireframe or flat mode also uses the display colors",
        "La vue 3D en mode filaire ou plat utilise aussi les couleurs d'affichage",
    ),
    ("shell.render.tools", "Tool textures hidden in 3D", "Textures outils masquées en 3D"),
    ("shell.render.hide_nodraw", "Nodraw", "Nodraw"),
    ("shell.render.hide_clips", "Clips", "Clips"),
    ("shell.render.hide_triggers", "Triggers", "Déclencheurs"),
    ("shell.render.hide_hints", "Hints and skips", "Hints et skips"),
    ("shell.render.brushes", "Brushes", "Brushes"),
    ("shell.render.invalid", "Highlight invalid brushes", "Surligner les brushes invalides"),
    (
        "shell.render.invalid_tip",
        "Brushes that are not closed and convex are drawn in red in every view",
        "Les brushes qui ne sont pas fermés et convexes sont dessinés en rouge dans toutes les vues",
    ),
    ("shell.render.disps", "Displacements", "Displacements"),
    ("shell.render.disps_tip", "Shows displacements", "Affiche les displacements"),
    ("shell.render.disps_2d", "Displacements in 2D", "Displacements en 2D"),
    (
        "shell.render.disps_2d_tip",
        "Displacements are drawn in 2D views (their grid can be dense)",
        "Les displacements sont dessinés dans les vues 2D (leur grille peut être dense)",
    ),
    ("shell.render.disps_only", "Displacements only", "Displacements seuls"),
    ("shell.render.disps_2d_mode", "Displacements in 2D views", "Displacements dans les vues 2D"),
    (
        "shell.render.disps_2d_mode_tip",
        "Draws the triangle grid of displacements or only their outline (each 2D view can choose its own mode in its view menu)",
        "Dessine la grille de triangles des displacements ou seulement leur contour (chaque vue 2D peut choisir son propre mode dans son menu de vue)",
    ),
    ("shell.render.disps_mesh", "Full mesh", "Maillage complet"),
    ("shell.render.disps_selected", "Mesh of the selection", "Maillage de la sélection"),
    ("shell.render.disps_outline", "Outline only", "Contour seulement"),
    ("shell.render.disp_source_2d", "Source brushes in 2D views", "Brushes d'origine dans les vues 2D"),
    (
        "shell.render.disp_source_2d_tip",
        "2D views draw the brush each displacement was built from as a faint outline",
        "Les vues 2D dessinent en contour discret le brush à partir duquel chaque displacement a été créé",
    ),
    ("shell.render.disps_2d_reset", "Use this mode in every 2D view", "Utiliser ce mode dans toutes les vues 2D"),
    (
        "shell.render.disps_2d_reset_tip",
        "Some 2D views have their own displacement mode: they follow this one again",
        "Certaines vues 2D ont leur propre mode de displacements : elles suivent à nouveau celui-ci",
    ),
    ("shell.render.disp_walkable", "Displacement walkability", "Praticabilité des displacements"),
    (
        "shell.render.disp_walkable_tip",
        "Colors displacement triangles like Hammer: walkable and buildable (slope normal Z of 0.8 or more), walkable (0.7 or more), not walkable. Forced tags win, like in the compiled map",
        "Colore les triangles des displacements comme Hammer : praticable et constructible (normale Z de la pente d'au moins 0,8), praticable (au moins 0,7), non praticable. Les tags forcés l'emportent, comme dans la map compilée",
    ),
    (
        "shell.render.disps_only_tip",
        "Hides every brush that is not a displacement",
        "Masque tous les brushes qui ne sont pas des displacements",
    ),
    ("shell.render.frame_rate", "Frame rate", "Images par seconde"),
    ("shell.render.max_fps", "Frame rate limit", "Limite d'images par seconde"),
    (
        "shell.render.max_fps_tip",
        "Most frames per second (0: no limit; with vertical sync the display limits it)",
        "Nombre maximal d'images par seconde (0 : aucune limite ; avec la synchronisation verticale, l'écran la limite)",
    ),
    ("shell.render.unfocused_fps", "In the background", "En arrière-plan"),
    (
        "shell.render.unfocused_fps_tip",
        "Frame rate limit while the window is not active (0: no limit)",
        "Limite d'images par seconde quand la fenêtre n'est pas active (0 : aucune limite)",
    ),
    ("shell.render.vsync", "Vertical sync", "Synchronisation verticale"),
    (
        "shell.render.vsync_tip",
        "Waits for the screen refresh (applies after a restart)",
        "Attend le rafraîchissement de l'écran (s'applique après un redémarrage)",
    ),
    ("shell.render.vram", "Graphics memory", "Mémoire graphique"),
    ("shell.render.vram_budget", "Graphics memory budget", "Budget de mémoire graphique"),
    ("shell.render.vram_auto", "Automatic", "Automatique"),
    (
        "shell.render.vram_budget_tip",
        "How much graphics card memory Hammer Pro may use. Automatic: 90% of what Windows gives the program (or 3 GB, 1.5 GB on integrated graphics, when the driver does not tell). Above 85% the maps open in other tabs are freed from the graphics card; above the budget textures are shown at a lower resolution until memory is free again.",
        "Quantité de mémoire de la carte graphique que Hammer Pro peut utiliser. Automatique : 90 % de ce que Windows donne au programme (ou 3 Go, 1,5 Go sur une puce graphique intégrée, quand le pilote ne le dit pas). Au-delà de 85 %, les maps ouvertes dans les autres onglets sont retirées de la carte graphique ; au-delà du budget, les textures sont affichées en plus basse résolution jusqu'à ce que la mémoire se libère.",
    ),
    (
        "shell.render.vram_keep_tabs",
        "Keep other tabs on the graphics card",
        "Garder les autres onglets sur la carte graphique",
    ),
    (
        "shell.render.vram_keep_tabs_tip",
        "Switching tabs is instant, but each open map uses graphics memory. Off: a map is loaded again on the graphics card when you show its tab.",
        "Le changement d'onglet est instantané, mais chaque map ouverte utilise de la mémoire graphique. Désactivé : une map est rechargée sur la carte graphique quand vous affichez son onglet.",
    ),
    ("shell.render.vram_now", "In use: {0} MB of {1} MB ({2})", "Utilisée : {0} Mo sur {1} Mo ({2})"),
    ("shell.render.vram_driver", "measured by the driver", "mesurée par le pilote"),
    ("shell.render.vram_estimate", "estimated", "estimée"),
    ("shell.render.vram_free", "Free graphics memory now", "Libérer la mémoire graphique maintenant"),
    (
        "shell.render.vram_free_tip",
        "Frees the maps of other tabs, the textures and models no open map uses, and the thumbnails",
        "Libère les maps des autres onglets, les textures et modèles qu'aucune map ouverte n'utilise, et les miniatures",
    ),
];
