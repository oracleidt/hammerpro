use std::time::{Duration, Instant};

use hp_app::actions::{Category, Check};
use hp_app::extend::MenuExtension;
use hp_app::i18n::TrTable;
use hp_app::runtime::render::{RenderDisplay, publish_render_display};
use hp_app::settings::{Disp2dSetting, RenderModeSetting};
use hp_app::{Action, AppState, Severity, t, tf};

use super::Package;
use crate::app::HammerProApp;

mod bgimages;

pub(crate) struct Render;

#[derive(Default)]
pub(crate) struct State {
    last_frame: Option<Instant>,
    bg: bgimages::BgImages,
}

impl State {
    pub(crate) fn forget_gpu(&mut self) {
        self.bg.forget_gpu();
    }
}

const FAR_STEP: f32 = 1.5;
const FAR_RANGE: (f32, f32) = (256.0, 1.0e6);
const MAX_SLEEP: Duration = Duration::from_millis(100);

const HANDLED: &[Action] = &[
    Action::RenderTextured,
    Action::RenderFlat,
    Action::RenderWireframe,
    Action::RenderTexturedWire,
    Action::RenderLit,
    Action::ToggleModels,
    Action::ToggleToolTextures,
    Action::ToggleGrid3d,
    Action::FarPlaneCloser,
    Action::FarPlaneFarther,
    Action::RenderUnshaded,
    Action::RenderSmoothingGroups,
    Action::RenderColorCorrected,
    Action::ToggleDetailPreview,
    Action::ToggleRenderEffects,
    Action::ToggleSkyPreview,
    Action::ToggleSkybox3d,
    Action::ToggleFogPreview,
    Action::ToggleWaterPreview,
    Action::ToggleFogVolumes,
    Action::ToggleFogVolumeTexture,
    Action::ToggleFishPreview,
    Action::ToggleFishSwim,
    Action::ToggleModelFade,
    Action::ToggleOpaqueMaterials,
    Action::ToggleWalkable,
    Action::ToggleDisplacements,
    Action::ToggleRadiusCulling,
    Action::ToggleInvalidBrushes,
    Action::ToggleCollisionModels,
    Action::ToggleCollisionOnly,
    Action::ToggleAnimatedMaterials,
    Action::BackgroundImages,
    Action::UseSkyCameraInGame,
    Action::ToggleDispMesh2d,
    Action::ToggleDispWalkable,
    Action::ToggleNodraw,
    Action::ToggleCubemaps,
    Action::ToggleObjectColors2d,
    Action::ToggleObjectColors3d,
    Action::RebuildCubemaps,
    Action::WriteCubemaps,
];

impl Package for Render {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        let state = &mut app.state;
        let r = &mut state.settings.render;
        match action {
            Action::RenderTextured => set_render_mode(state, RenderModeSetting::Textured),
            Action::RenderFlat => set_render_mode(state, RenderModeSetting::Flat),
            Action::RenderWireframe => set_render_mode(state, RenderModeSetting::Wireframe),
            Action::RenderTexturedWire => set_render_mode(state, RenderModeSetting::TexturedWire),
            Action::RenderLit => set_render_mode(state, RenderModeSetting::Lit),
            Action::RenderUnshaded => set_render_mode(state, RenderModeSetting::Unshaded),
            Action::RenderSmoothingGroups => set_render_mode(state, RenderModeSetting::SmoothingGroups),
            Action::RenderColorCorrected => set_render_mode(state, RenderModeSetting::ColorCorrected),
            Action::ToggleModels => state.settings.view.show_models = !state.settings.view.show_models,
            Action::ToggleToolTextures => {
                state.settings.view.show_tool_textures = !state.settings.view.show_tool_textures;
            }
            Action::ToggleGrid3d => r.grid_3d = !r.grid_3d,
            Action::FarPlaneCloser | Action::FarPlaneFarther => {
                let far = step_far_plane(state.settings.view.far_plane, action == Action::FarPlaneFarther);
                state.settings.view.far_plane = far;
                state.notify(Severity::Info, tf!("render.draw_distance", format!("{far:.0}")));
            }
            Action::ToggleRenderEffects => r.render_effects = !r.render_effects,
            Action::ToggleSkyPreview => r.sky_preview = !r.sky_preview,
            Action::ToggleSkybox3d => {
                r.skybox_3d = !r.skybox_3d;
                let on = r.skybox_3d;
                state.notify(Severity::Info, t!(if on { "render.skybox_3d_on" } else { "render.skybox_3d_off" }));
            }
            Action::ToggleFogPreview => r.fog_preview = !r.fog_preview,
            Action::ToggleWaterPreview => r.water_preview = !r.water_preview,
            Action::ToggleDetailPreview => r.detail_preview = !r.detail_preview,
            Action::ToggleFogVolumes => r.fog_volume_preview = !r.fog_volume_preview,
            Action::ToggleFogVolumeTexture => r.fog_volume_texture = !r.fog_volume_texture,
            Action::ToggleFishPreview => r.fish_preview = !r.fish_preview,
            Action::ToggleFishSwim => r.fish_swim = !r.fish_swim,
            Action::ToggleModelFade => r.model_fade = !r.model_fade,
            Action::ToggleOpaqueMaterials => r.force_opaque = !r.force_opaque,
            Action::ToggleWalkable => r.walkable = !r.walkable,
            Action::ToggleDisplacements => r.show_displacements = !r.show_displacements,
            Action::ToggleRadiusCulling => r.radius_culling = !r.radius_culling,
            Action::ToggleInvalidBrushes => r.highlight_invalid = !r.highlight_invalid,
            Action::ToggleDispMesh2d => {
                r.displacements_2d = match r.displacements_2d {
                    Disp2dSetting::Outline => Disp2dSetting::Mesh,
                    Disp2dSetting::Mesh | Disp2dSetting::SelectedMesh => Disp2dSetting::Outline,
                };
            }
            Action::ToggleDispWalkable => r.disp_walkable = !r.disp_walkable,
            Action::ToggleNodraw => r.hide_nodraw = !r.hide_nodraw,
            Action::ToggleCubemaps => r.cubemaps = !r.cubemaps,
            Action::ToggleObjectColors2d => r.object_colors_2d = !r.object_colors_2d,
            Action::ToggleObjectColors3d => r.object_colors_3d = !r.object_colors_3d,
            Action::RebuildCubemaps => {
                if let Some(gfx) = state.gfx.as_mut() {
                    gfx.renderer.rebake_cubemaps();
                    let faces = gfx.renderer.cubemap_stats().pending_faces;
                    state.notify(Severity::Info, tf!("render.cubemaps_rebuilding", faces / 6));
                }
                return true;
            }
            Action::ToggleCollisionModels => r.collision_models = !r.collision_models,
            Action::ToggleCollisionOnly => r.collision_only = !r.collision_only,
            Action::ToggleAnimatedMaterials => r.animate_materials = !r.animate_materials,
            Action::BackgroundImages => {
                app.pkg.render.bg.open = !app.pkg.render.bg.open;
                return true;
            }
            Action::UseSkyCameraInGame => {
                use_sky_camera_in_game(state);
                return true;
            }
            Action::WriteCubemaps => {
                match write_cubemaps(state) {
                    Ok(msg) => state.notify(Severity::Success, msg),
                    Err(msg) => state.notify(Severity::Warning, msg),
                }
                return true;
            }
            _ => return false,
        }
        state.settings_dirty = true;
        true
    }

    fn init(&self, app: &mut HammerProApp) {
        hp_app::i18n::register(bgimages::TR);
        app.shell
            .add_menu_extension(MenuExtension::new(Category::View, 20, 11, fog_menu).in_submenu("menu.environment"));
        app.shell
            .add_menu_extension(MenuExtension::new(Category::View, 20, 10, sky_menu).in_submenu("menu.environment"));
        app.shell.add_menu_extension(MenuExtension::new(Category::View, 20, 0, tools_menu).in_submenu("menu.display"));
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.pkg.render.bg.window(ctx, &mut app.state);
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        publish_render_display(RenderDisplay::from_settings(&app.state.settings, &app.state.runtime.render));
        app.pkg.render.bg.tick(&mut app.state);
        let r = &app.state.settings.render;
        let focused = ctx.input(|i| {
            i.viewport().focused.unwrap_or(true) || i.raw.viewports.values().any(|v| v.focused == Some(true))
        });
        let cap = if focused { r.max_fps } else { r.unfocused_fps };
        let now = Instant::now();
        if let Some(wait) = frame_wait(app.pkg.render.last_frame, now, cap) {
            std::thread::sleep(wait);
        }
        app.pkg.render.last_frame = Some(Instant::now());
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

pub(crate) fn write_cubemaps(state: &mut AppState) -> Result<String, String> {
    let targets = state.assets.play_cubemap_targets();
    let Some(path) = state.runtime.health.exact.as_ref().map(|e| e.path.clone()) else {
        return Err(t!("render.cubemaps_no_map").to_owned());
    };
    if targets.is_empty() {
        return Err(t!("render.cubemaps_no_samples").to_owned());
    }
    let Some(gfx) = state.gfx.as_ref() else { return Err(t!("render.cubemaps_no_map").to_owned()) };
    let r = &gfx.renderer;
    let Some(mode) = r.cubemap_lighting() else { return Err(t!("render.cubemaps_not_like_game").to_owned()) };
    let pending = r.cubemap_stats().pending_faces;
    if pending > 0 {
        return Err(tf!("render.cubemaps_pending", pending));
    }
    let hdr = mode == hp_render::CompiledLightMode::Hdr;
    let map = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut files: Vec<hp_bsp::cubemaps::CubemapFiles> = Vec::new();
    let mut skipped = 0;
    for (origin, id) in targets {
        let Some((size, faces)) = id.and_then(|id| r.read_cubemap(id).ok()) else {
            skipped += 1;
            continue;
        };
        let faces: [Vec<[f32; 3]>; 6] = faces.map(|f| f.iter().map(|t| [t[0], t[1], t[2]]).collect());
        let ldr = hp_bsp::cubemaps::cubemap_vtf(size, &faces, false);
        let high = hdr.then(|| hp_bsp::cubemaps::cubemap_vtf(size, &faces, true));
        files.push((origin, ldr, high));
    }
    if files.is_empty() {
        return Err(t!("render.cubemaps_no_samples").to_owned());
    }
    let written =
        hp_bsp::cubemaps::write_cubemaps(&path, &map, &files).map_err(|e| tf!("render.cubemaps_failed", e))?;
    log::info!("cubemaps: {written} files written into {} ({skipped} samples left)", path.display());
    state.runtime.health.bsp_reload_request = true;
    Ok(tf!("render.cubemaps_written", files.len(), path.display(), skipped))
}

fn set_render_mode(state: &mut AppState, mode: RenderModeSetting) {
    state.settings.view.render_mode = mode;
}

fn step_far_plane(far: f32, farther: bool) -> f32 {
    let far = if far.is_finite() { far.clamp(FAR_RANGE.0, FAR_RANGE.1) } else { 32768.0 };
    let next = if farther { far * FAR_STEP } else { far / FAR_STEP };
    next.clamp(FAR_RANGE.0, FAR_RANGE.1).round()
}

fn frame_wait(last: Option<Instant>, now: Instant, cap: u32) -> Option<Duration> {
    if cap == 0 {
        return None;
    }
    let frame = Duration::from_secs_f64(1.0 / f64::from(cap.clamp(5, 1000)));
    let elapsed = now.checked_duration_since(last?)?;
    frame.checked_sub(elapsed).filter(|w| !w.is_zero()).map(|w| w.min(MAX_SLEEP))
}

fn fog_menu(ui: &mut egui::Ui, state: &mut AppState) {
    use hp_ui_shell::menu::{menu_entry, submenu};
    submenu(ui, t!("render.fog_menu"), |ui| {
        let on = state.settings.render.fog_preview;
        let chosen = state.runtime.render.fog_controller;
        if menu_entry(ui, t!("render.fog_off"), None, true, Check::Radio(!on)).clicked() {
            state.settings.render.fog_preview = false;
            state.settings_dirty = true;
        }
        let mut controllers: Vec<(hp_editor::Id, String)> = state
            .doc
            .scene()
            .entities()
            .iter()
            .filter(|(_, n)| hp_app::runtime::fog::is_fog_controller(&n.entity.classname))
            .map(|(id, n)| {
                let name = n.entity.get("targetname").map(str::trim).filter(|s| !s.is_empty());
                let name = name.map_or_else(|| tf!("render.fog_unnamed", id), str::to_owned);
                let label =
                    if hp_app::runtime::fog::fog_enabled(&n.entity) { name } else { tf!("render.fog_disabled", name) };
                (*id, label)
            })
            .collect();
        if controllers.is_empty() {
            ui.add_enabled(false, egui::Button::new(t!("render.fog_none")));
            return;
        }
        controllers.sort_by_key(|(id, _)| *id);
        let shown = hp_app::runtime::fog::global_controller(&state.doc, chosen).map(|(id, _)| id);
        for (id, label) in controllers {
            let checked = on && shown == Some(id);
            if menu_entry(ui, &label, None, true, Check::Radio(checked)).clicked() {
                state.settings.render.fog_preview = true;
                state.runtime.render.fog_controller = Some(id);
                state.settings_dirty = true;
            }
        }
    });
}

fn use_sky_camera_in_game(state: &mut AppState) {
    let Some(id) = hp_app::runtime::render::previewed_sky_camera(state) else { return };
    match state.doc.make_sky_camera_active(id) {
        Ok(true) => state.notify(Severity::Success, tf!("render.sky_in_game", sky_label(state, id))),
        Ok(false) => {}
        Err(e) => state.notify(Severity::Error, e.to_string()),
    }
}

fn sky_label(state: &AppState, id: hp_editor::Id) -> String {
    let cam = state.doc.scene().entity(id).and_then(|n| hp_editor::sky::sky_camera_of(&n.entity));
    let scale = |c: &hp_editor::sky::SkyCamera| {
        if c.scale_valid { c.scale.to_string() } else { tf!("render.sky_bad_scale", c.scale_text) }
    };
    match cam {
        Some(c) if !c.name.is_empty() => tf!("render.sky_named", c.name, c.id, scale(&c)),
        Some(c) => tf!("render.sky_unnamed", c.id, scale(&c)),
        None => tf!("render.sky_unnamed", id, hp_editor::sky::DEFAULT_SKY_SCALE),
    }
}

fn sky_menu(ui: &mut egui::Ui, state: &mut AppState) {
    use hp_ui_shell::menu::{menu_entry, submenu};
    let cams = hp_editor::sky::sky_cameras(&state.doc);
    if cams.len() < 2 && state.runtime.render.sky_camera.is_none() {
        return;
    }
    submenu(ui, t!("render.sky_menu"), |ui| {
        let game = hp_editor::sky::game_sky_camera(&state.doc);
        let chosen = hp_app::runtime::render::previewed_sky_camera(state);
        let game_text = game.map_or_else(|| t!("render.sky_none").to_owned(), |id| sky_label(state, id));
        if menu_entry(ui, &tf!("render.sky_follow_game", game_text), None, true, Check::Radio(chosen.is_none()))
            .clicked()
        {
            state.runtime.render.sky_camera = None;
            state.settings.render.skybox_3d = true;
        }
        ui.separator();
        for c in &cams {
            let mut label = sky_label(state, c.id);
            if Some(c.id) == game {
                label = tf!("render.sky_is_game", label);
            }
            if menu_entry(ui, &label, None, true, Check::Radio(chosen == Some(c.id))).clicked() {
                state.runtime.render.sky_camera = Some(c.id);
                state.settings.render.skybox_3d = true;
            }
        }
        ui.separator();
        let can = chosen.is_some_and(|id| Some(id) != game);
        if menu_entry(ui, t!("action.use_sky_camera_in_game"), None, can, Check::None)
            .on_hover_text(t!("render.sky_in_game_tip"))
            .clicked()
        {
            use_sky_camera_in_game(state);
        }
    });
}

fn tools_menu(ui: &mut egui::Ui, state: &mut AppState) {
    use hp_ui_shell::menu::{menu_entry, submenu};
    submenu(ui, t!("render.tools_menu"), |ui| {
        let r = &mut state.settings.render;
        let mut changed = false;
        for (value, key) in [
            (&mut r.hide_nodraw, "render.hide_nodraw"),
            (&mut r.hide_clips, "render.hide_clips"),
            (&mut r.hide_triggers, "render.hide_triggers"),
            (&mut r.hide_hints, "render.hide_hints"),
        ] {
            if menu_entry(ui, t!(key), None, true, Check::Toggle(*value)).clicked() {
                *value = !*value;
                changed = true;
            }
        }
        if changed {
            state.settings_dirty = true;
        }
    });
}

const TR: TrTable = &[
    ("render.draw_distance", "Draw distance: {0} units", "Distance d'affichage : {0} unités"),
    ("render.skybox_3d_on", "3D skybox shown", "Skybox 3D affichée"),
    (
        "render.cubemaps_no_map",
        "No compiled map: compile the map, then open the player view",
        "Pas de map compilée : compilez la map, puis ouvrez la vue joueur",
    ),
    (
        "render.cubemaps_no_samples",
        "The compiled map shown has no env_cubemap the editor draws: open the player view on a map with env_cubemap entities",
        "La map compilée affichée n'a aucun env_cubemap dessiné par l'éditeur : ouvrez la vue joueur sur une map avec des env_cubemap",
    ),
    (
        "render.cubemaps_not_like_game",
        "The cubemaps are drawn with the editor shading: open the player view with its compiled lighting (L) so they are drawn like in game, then write them",
        "Les cubemaps sont dessinées avec l'ombrage de l'éditeur : ouvrez la vue joueur avec son éclairage compilé (L) pour qu'elles soient dessinées comme en jeu, puis écrivez-les",
    ),
    (
        "render.cubemaps_pending",
        "The cubemaps are still being drawn ({0} faces left): try again in a moment",
        "Les cubemaps sont encore en cours de dessin ({0} faces restantes) : réessayez dans un instant",
    ),
    ("render.cubemaps_failed", "The cubemaps could not be written: {0}", "Impossible d'écrire les cubemaps : {0}"),
    (
        "render.cubemaps_written",
        "{0} cubemaps written into {1} ({2} kept as they were); recompiles keep them",
        "{0} cubemaps écrites dans {1} ({2} gardées telles quelles) ; les recompilations les conservent",
    ),
    (
        "render.cubemaps_rebuilding",
        "Drawing the cubemaps again ({0} env_cubemap and the sky)",
        "Les cubemaps sont redessinées ({0} env_cubemap et le ciel)",
    ),
    ("render.skybox_3d_off", "3D skybox hidden", "Skybox 3D masquée"),
    ("render.fog_menu", "Previewed fog controller", "Contrôleur de brouillard prévisualisé"),
    ("render.sky_menu", "3D skybox camera", "Caméra de la skybox 3D"),
    ("render.sky_follow_game", "Like in game: {0}", "Comme en jeu : {0}"),
    ("render.sky_none", "none", "aucune"),
    ("render.sky_named", "{0} (#{1}, scale {2})", "{0} (n°{1}, échelle {2})"),
    ("render.sky_unnamed", "sky_camera #{0} (scale {1})", "sky_camera n°{0} (échelle {1})"),
    ("render.sky_is_game", "{0}, used in game", "{0}, utilisée en jeu"),
    ("render.sky_bad_scale", "{0}, invalid", "{0}, non valide"),
    ("render.sky_in_game", "The game now uses {0}", "Le jeu utilise maintenant {0}"),
    (
        "render.sky_in_game_tip",
        "The game shows the 3D skybox of the last sky_camera of the map: this moves the previewed one after the others",
        "Le jeu affiche la skybox 3D de la dernière sky_camera de la carte : celle affichée passe après les autres",
    ),
    ("render.fog_off", "Off", "Désactivé"),
    ("render.fog_none", "No env_fog_controller in the map", "Aucun env_fog_controller dans la carte"),
    ("render.fog_unnamed", "Controller #{0}", "Contrôleur n°{0}"),
    ("render.fog_disabled", "{0} (Fog Enable: No)", "{0} (Fog Enable : No)"),
    ("render.tools_menu", "Hidden tool textures", "Textures outils masquées"),
    ("render.hide_nodraw", "Nodraw", "Nodraw"),
    ("render.hide_clips", "Clips", "Clips"),
    ("render.hide_triggers", "Triggers", "Déclencheurs"),
    ("render.hide_hints", "Hints and skips", "Hints et skips"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
    }

    #[test]
    fn far_plane_steps_stay_in_range() {
        assert_eq!(step_far_plane(32768.0, true), 49152.0);
        assert_eq!(step_far_plane(32768.0, false), 21845.0);
        assert_eq!(step_far_plane(300.0, false), 256.0);
        assert_eq!(step_far_plane(9.0e5, true), 1.0e6);
        assert_eq!(step_far_plane(f32::NAN, true), 49152.0);
    }

    #[test]
    fn frame_cap_waits_the_rest_of_the_frame() {
        let t0 = Instant::now();
        assert_eq!(frame_wait(Some(t0), t0, 0), None, "no cap");
        assert_eq!(frame_wait(None, t0, 60), None, "first frame");
        let w = frame_wait(Some(t0), t0 + Duration::from_millis(10), 50).expect("wait");
        assert!((w.as_secs_f64() - 0.010).abs() < 1e-6, "{w:?}");
        assert_eq!(frame_wait(Some(t0), t0 + Duration::from_millis(30), 50), None, "late frame");
        assert_eq!(frame_wait(Some(t0), t0, 1), Some(MAX_SLEEP), "capped sleep");
    }
}
