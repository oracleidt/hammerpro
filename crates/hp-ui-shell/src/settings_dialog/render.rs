use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::{
    Disp2dSetting, Disp2dViewSetting, DispWalkMode, Models2dSetting, PerformanceMode, PropShadowSetting,
    RenderSettings, StaticPropShadows, ToneModeSetting, WaterDetailSetting,
};
use hp_app::{AppState, t, tf};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(RenderPage)]
}

struct RenderPage;

fn walk_mode_label(m: DispWalkMode) -> &'static str {
    match m {
        DispWalkMode::Source => t!("shell.render.walk_mode_source"),
        DispWalkMode::Limit => t!("shell.render.walk_mode_limit"),
        DispWalkMode::Gradient => t!("shell.render.walk_mode_gradient"),
    }
}

fn check(ui: &mut egui::Ui, value: &mut bool, label: &str, tip: &str) -> bool {
    ui.checkbox(value, label).on_hover_text(tip).changed()
}

fn subheading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).small().strong().color(ui.visuals().weak_text_color()));
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
        let compiled_range = state.settings.compile.lighting;
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
        subheading(ui, t!("shell.render.group_sky_water"));
        changed |= check(ui, &mut r.sky_preview, t!("shell.render.sky"), t!("shell.render.sky_tip"));
        changed |= check(ui, &mut r.skybox_3d, t!("shell.render.skybox_3d"), t!("shell.render.skybox_3d_tip"));
        changed |= check(
            ui,
            &mut r.sky_see_through,
            t!("shell.render.sky_see_through"),
            t!("shell.render.sky_see_through_tip"),
        );
        changed |= check(ui, &mut r.water_preview, t!("shell.render.water"), t!("shell.render.water_tip"));
        changed |= check(ui, &mut r.detail_preview, t!("shell.render.detail"), t!("shell.render.detail_tip"));
        ui.horizontal(|ui| {
            ui.label(t!("shell.render.water_detail")).on_hover_text(t!("shell.render.water_detail_tip"));
            egui::ComboBox::from_id_salt("hp_shell_render_water_detail")
                .selected_text(water_detail_label(r.water_detail))
                .show_ui(ui, |ui| {
                    for m in WaterDetailSetting::ALL {
                        changed |= ui
                            .selectable_value(&mut r.water_detail, m, water_detail_label(m))
                            .on_hover_text(t!("shell.render.water_detail_tip"))
                            .changed();
                    }
                });
        });
        subheading(ui, t!("shell.render.group_lighting"));
        changed |= check(ui, &mut r.normal_maps, t!("shell.render.normal_maps"), t!("shell.render.normal_maps_tip"));
        changed |= check(
            ui,
            &mut r.light_preview_match_compile,
            t!("shell.render.light_preview_match_compile"),
            t!("shell.render.light_preview_match_compile_tip"),
        );
        let mut hdr = r.preview_hdr(compiled_range);
        if check(ui, &mut hdr, t!("shell.render.light_preview_hdr"), t!("shell.render.light_preview_hdr_tip")) {
            r.light_preview_hdr = hdr;
            r.light_preview_match_compile = false;
            changed = true;
        }
        changed |= check(
            ui,
            &mut r.projected_textures,
            t!("shell.render.projected_textures"),
            t!("shell.render.projected_textures_tip"),
        );
        changed |=
            check(ui, &mut r.sky_occlusion, t!("shell.render.sky_occlusion"), t!("shell.render.sky_occlusion_tip"));
        changed |= check(ui, &mut r.lit_indirect, t!("shell.render.lit_indirect"), t!("shell.render.lit_indirect_tip"));
        changed |= check(ui, &mut r.model_lods, t!("shell.render.model_lods"), t!("shell.render.model_lods_tip"));
        changed |= check(ui, &mut r.sun_glow, t!("shell.render.sun_glow"), t!("shell.render.sun_glow_tip"));
        changed |= check(
            ui,
            &mut r.game_pixel_grid,
            t!("shell.render.game_pixel_grid"),
            t!("shell.render.game_pixel_grid_tip"),
        );
        ui.horizontal(|ui| {
            ui.label(t!("shell.render.tone_mapping")).on_hover_text(t!("shell.render.tone_mapping_tip"));
            egui::ComboBox::from_id_salt("hp_shell_render_tone_mapping")
                .selected_text(tone_label(r.tone_mapping))
                .show_ui(ui, |ui| {
                    for m in ToneModeSetting::ALL {
                        changed |= ui
                            .selectable_value(&mut r.tone_mapping, m, tone_label(m))
                            .on_hover_text(t!("shell.render.tone_mapping_tip"))
                            .changed();
                    }
                });
        });
        egui::Grid::new("hp_shell_render_tone").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.render.exposure"));
            changed |= ui
                .add_enabled(
                    r.tone_mapping == ToneModeSetting::Fixed,
                    egui::DragValue::new(&mut r.exposure).range(0.05..=16.0).speed(0.02).fixed_decimals(2),
                )
                .on_hover_text(t!("shell.render.exposure_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.render.exposure_min"));
            changed |= ui
                .add_enabled(
                    r.tone_mapping == ToneModeSetting::Auto,
                    egui::DragValue::new(&mut r.exposure_min).range(0.05..=16.0).speed(0.02).fixed_decimals(2),
                )
                .on_hover_text(t!("shell.render.exposure_min_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.render.exposure_max"));
            changed |= ui
                .add_enabled(
                    r.tone_mapping == ToneModeSetting::Auto,
                    egui::DragValue::new(&mut r.exposure_max).range(0.05..=16.0).speed(0.02).fixed_decimals(2),
                )
                .on_hover_text(t!("shell.render.exposure_max_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.render.bloom"));
            changed |= ui
                .add_enabled(
                    r.tone_mapping != ToneModeSetting::Off,
                    egui::DragValue::new(&mut r.bloom).range(0.0..=8.0).speed(0.02).fixed_decimals(2),
                )
                .on_hover_text(t!("shell.render.bloom_tip"))
                .changed();
            ui.end_row();
        });
        changed |= check(ui, &mut r.map_tonemap, t!("shell.render.map_tonemap"), t!("shell.render.map_tonemap_tip"));
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
        subheading(ui, t!("shell.render.group_fog"));
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
        subheading(ui, t!("shell.render.group_display"));
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
        egui::Grid::new("hp_shell_render_walk").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.render.walk_mode")).on_hover_text(t!("shell.render.walk_mode_tip"));
            egui::ComboBox::from_id_salt("hp_shell_render_walk_mode")
                .selected_text(walk_mode_label(r.disp_walk_mode))
                .show_ui(ui, |ui| {
                    for m in DispWalkMode::ALL {
                        changed |= ui.selectable_value(&mut r.disp_walk_mode, m, walk_mode_label(m)).changed();
                    }
                });
            ui.end_row();
            ui.label(t!("shell.render.walk_angle")).on_hover_text(t!("shell.render.walk_angle_tip"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut r.disp_walk_angle)
                        .speed(0.1)
                        .range(1.0..=89.0)
                        .max_decimals(2)
                        .suffix("°"),
                )
                .on_hover_text(t!("shell.render.walk_angle_tip"))
                .changed();
            ui.end_row();
        });

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
            ui.label(t!("shell.render.animation_fps"));
            changed |= ui
                .add(egui::DragValue::new(&mut r.animation_fps).range(0..=hp_app::settings::MAX_ANIMATION_FPS))
                .on_hover_text(t!("shell.render.animation_fps_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.render.animation_idle"));
            changed |= ui
                .add(egui::DragValue::new(&mut r.animation_idle_pause_s).range(0..=86_400).suffix(" s"))
                .on_hover_text(t!("shell.render.animation_idle_tip"))
                .changed();
            ui.end_row();
        });
        changed |= check(
            ui,
            &mut r.animate_in_background,
            t!("shell.render.animate_in_background"),
            t!("shell.render.animate_in_background_tip"),
        );
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
        changed |= check(
            ui,
            &mut r.texture_streaming,
            t!("shell.render.texture_streaming"),
            t!("shell.render.texture_streaming_tip"),
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

        heading(ui, t!("shell.render.perf"));
        let perf = &mut state.runtime.render.perf;
        egui::Grid::new("hp_shell_render_perf").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.render.perf_mode"));
            let before = r.performance_mode;
            egui::ComboBox::from_id_salt("hp_shell_render_perf_mode")
                .selected_text(perf_mode_label(r.performance_mode))
                .show_ui(ui, |ui| {
                    for m in PerformanceMode::ALL {
                        ui.selectable_value(&mut r.performance_mode, m, perf_mode_label(m))
                            .on_hover_text(perf_mode_tip(m));
                    }
                });
            changed |= before != r.performance_mode;
            ui.end_row();
            ui.label(t!("shell.render.render_scale"));
            changed |= ui
                .add(
                    egui::Slider::new(&mut r.render_scale, 0.25..=1.0)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                )
                .on_hover_text(t!("shell.render.render_scale_tip"))
                .changed();
            ui.end_row();
        });
        changed |= check(ui, &mut r.perf_hud, t!("shell.render.perf_hud"), t!("shell.render.perf_hud_tip"));
        let level = perf.level(r.performance_mode);
        ui.label(egui::RichText::new(tf!("shell.render.perf_now", level.id().replace('_', " "), perf.adapter)).weak());
        if perf.reduced_automatically(r.performance_mode) || perf.auto_locked {
            let label = if perf.auto_locked { t!("shell.render.perf_resume") } else { t!("shell.render.perf_restore") };
            if ui.button(label).clicked() {
                if perf.auto_locked {
                    perf.unlock();
                } else {
                    perf.restore_request = true;
                }
            }
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

fn water_detail_label(m: WaterDetailSetting) -> &'static str {
    match m {
        WaterDetailSetting::Simple => t!("shell.render.water_simple"),
        WaterDetailSetting::ReflectWorld => t!("shell.render.water_reflect_world"),
        WaterDetailSetting::ReflectAll => t!("shell.render.water_reflect_all"),
    }
}

fn perf_mode_label(m: PerformanceMode) -> &'static str {
    match m {
        PerformanceMode::Auto => t!("shell.render.perf_auto"),
        PerformanceMode::Off => t!("shell.render.perf_off"),
        PerformanceMode::Low => t!("shell.render.perf_low"),
        PerformanceMode::VeryLow => t!("shell.render.perf_very_low"),
    }
}

fn perf_mode_tip(m: PerformanceMode) -> &'static str {
    match m {
        PerformanceMode::Auto => t!("shell.render.perf_auto_tip"),
        PerformanceMode::Off => t!("shell.render.perf_off_tip"),
        PerformanceMode::Low => t!("shell.render.perf_low_tip"),
        PerformanceMode::VeryLow => t!("shell.render.perf_very_low_tip"),
    }
}

fn tone_label(m: ToneModeSetting) -> &'static str {
    match m {
        ToneModeSetting::Auto => t!("shell.render.tone_auto"),
        ToneModeSetting::Fixed => t!("shell.render.tone_fixed"),
        ToneModeSetting::Off => t!("shell.render.tone_off"),
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
    ("shell.render.group_sky_water", "Sky, water and details", "Ciel, eau et détails"),
    ("shell.render.group_lighting", "Lighting and materials", "Éclairage et matériaux"),
    ("shell.render.group_fog", "Fog and fish", "Brouillard et poissons"),
    ("shell.render.group_display", "Display", "Affichage"),
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
    ("shell.render.light_preview_match_compile", "Match compilation HDR/LDR", "Suivre le HDR/LDR de la compilation"),
    (
        "shell.render.light_preview_match_compile_tip",
        "Preview the lighting range selected for compilation. Changing HDR light values selects a manual comparison instead.",
        "Utilise la plage lumineuse choisie pour la compilation. Modifier les valeurs HDR active une comparaison manuelle.",
    ),
    ("shell.render.projected_textures", "Projected textures", "Textures projetées"),
    (
        "shell.render.projected_textures_tip",
        "The env_projectedtexture entities (the first sixteen) light the Lit mode, the baked preview and the player view with their texture, FOV, near and far Z, color and strength, like the game",
        "Les entités env_projectedtexture (les seize premières) éclairent le mode éclairé, l'aperçu précalculé et la vue joueur avec leur texture, FOV, Z proche et lointain, couleur et intensité, comme le jeu",
    ),
    (
        "shell.render.light_preview_hdr_tip",
        "Lit mode: the lights use their HDR values (_lightHDR, _lightscaleHDR, _ambientHDR, _AmbientScaleHDR) like \
         the game on a map compiled with HDR. Off: the LDR values (_light, _ambient).",
        "Mode éclairé : les lumières prennent leurs valeurs HDR (_lightHDR, _lightscaleHDR, _ambientHDR, \
         _AmbientScaleHDR) comme le jeu sur une map compilée en HDR. Désactivé : les valeurs LDR (_light, _ambient).",
    ),
    ("shell.render.sky_occlusion", "Sky light only under the sky", "Lumière du ciel seulement sous le ciel"),
    ("shell.render.lit_indirect", "Bounced light in Lit mode", "Lumière rebondie en mode éclairé"),
    (
        "shell.render.lit_indirect_tip",
        "Lit mode: a background bake of the map with the compiler gives the surfaces its sky light and the light          bounced by the other surfaces (the ambient part of the lightmaps, like the game); the lights and the sun          stay real time. It runs again a moment after you edit the map. Off: only the real time lights and an          approximate sky light, no bounce.",
        "Mode éclairé : une cuisson de la map en arrière-plan avec le compilateur donne aux surfaces leur lumière du          ciel et la lumière rebondie par les autres surfaces (la part ambiante des lightmaps, comme le jeu) ; les          lumières et le soleil restent en temps réel. Elle repart un instant après une modification de la map.          Désactivé : seulement les lumières en temps réel et une lumière du ciel approchée, sans rebond.",
    ),
    ("shell.render.model_lods", "Model levels of detail", "Niveaux de détail des modèles"),
    (
        "shell.render.model_lods_tip",
        "Draws every prop at the level of detail the game picks for it, from the switch distances of its model and          the size it takes on the screen. Off: the finest level of every model is always drawn, like Hammer.",
        "Dessine chaque prop au niveau de détail que le jeu choisit pour lui, d'après les distances de bascule de          son modèle et la taille qu'il occupe à l'écran. Désactivé : le niveau le plus fin de chaque modèle est          toujours dessiné, comme Hammer.",
    ),
    ("shell.render.game_pixel_grid", "Game pixel grid", "Grille de pixels du jeu"),
    (
        "shell.render.game_pixel_grid_tip",
        "The 3D views sample their pixels on the same grid as the game (Direct3D 9), half a pixel from the grid          Vulkan and Direct3D 12 use, so a screenshot of the preview lines up with a screenshot of the game.          Off: the mathematically centered grid of the graphics API.",
        "Les vues 3D échantillonnent leurs pixels sur la même grille que le jeu (Direct3D 9), à un demi pixel de          la grille de Vulkan et de Direct3D 12, pour qu’une capture de l’aperçu se superpose à une capture du jeu.          Désactivé : la grille centrée mathématiquement de l’API graphique.",
    ),
    ("shell.render.sun_glow", "env_sun glow", "Halo de l’env_sun"),
    (
        "shell.render.sun_glow_tip",
        "Draws the sun sprite and its glow of the env_sun entity in the 3D views like the game: its direction,          size and overlaysize, rendercolor and overlaycolor, material and overlaymaterial, HDRColorScale, the          fade over the 40 degrees around the middle of the view and the fade behind what hides the sun.",
        "Dessine le sprite du soleil et son halo de l’entité env_sun dans les vues 3D comme le jeu : sa direction,          ses size et overlaysize, ses rendercolor et overlaycolor, ses material et overlaymaterial, son          HDRColorScale, le fondu sur les 40 degrés autour du centre de la vue et le fondu derrière ce qui cache          le soleil.",
    ),
    ("shell.render.tone_mapping", "HDR tone mapping", "Tone mapping HDR"),
    (
        "shell.render.tone_mapping_tip",
        "How the lit views (Lit mode, baked preview and the player view with its compiled lighting) turn the light          into pixels, like the game: Auto adapts the exposure to the picture like the engine (mat_autoexposure_min          and max, 60% of the brightest pixels), Fixed keeps the exposure below (mat_force_tonemap_scale), Off          shows the light as it is, without exposure or bloom.",
        "Comment les vues éclairées (mode éclairé, aperçu précalculé et vue joueur avec son éclairage compilé)          transforment la lumière en pixels, comme le jeu : Auto adapte l'exposition à l'image comme le moteur          (mat_autoexposure_min et max, 60 % des pixels les plus clairs), Fixe garde l'exposition ci-dessous          (mat_force_tonemap_scale), Aucun montre la lumière telle quelle, sans exposition ni bloom.",
    ),
    ("shell.render.tone_auto", "Auto exposure", "Exposition automatique"),
    ("shell.render.tone_fixed", "Fixed exposure", "Exposition fixe"),
    ("shell.render.tone_off", "Off", "Aucun"),
    ("shell.render.exposure", "Exposure", "Exposition"),
    (
        "shell.render.exposure_tip",
        "Fixed exposure: the scale the light is multiplied by (1 is the light of the lightmaps as it is)",
        "Exposition fixe : le facteur par lequel la lumière est multipliée (1 : la lumière des lightmaps telle quelle)",
    ),
    ("shell.render.exposure_min", "Lowest auto exposure", "Exposition auto minimale"),
    (
        "shell.render.exposure_min_tip",
        "The auto exposure never goes under this (mat_autoexposure_min, 0.5 in the game)",
        "L'exposition automatique ne descend jamais sous cette valeur (mat_autoexposure_min, 0,5 dans le jeu)",
    ),
    ("shell.render.exposure_max", "Highest auto exposure", "Exposition auto maximale"),
    (
        "shell.render.exposure_max_tip",
        "The auto exposure never goes over this (mat_autoexposure_max, 2 in the game)",
        "L'exposition automatique ne dépasse jamais cette valeur (mat_autoexposure_max, 2 dans le jeu)",
    ),
    ("shell.render.bloom", "Bloom", "Bloom"),
    (
        "shell.render.bloom_tip",
        "Strength of the glow around bright surfaces (mat_bloom_scalefactor_scalar, 1 in the game; 0 turns it off).          It is multiplied by the bloom scale of the map's env_tonemap_controller when the option below is on.",
        "Intensité de la lueur autour des surfaces claires (mat_bloom_scalefactor_scalar, 1 dans le jeu ; 0 la          désactive). Elle est multipliée par l'échelle de bloom de l'env_tonemap_controller de la map quand l'option          ci-dessous est activée.",
    ),
    ("shell.render.map_tonemap", "Follow the map's tone controller", "Suivre le contrôleur de tonemap de la map"),
    (
        "shell.render.map_tonemap_tip",
        "The env_tonemap_controller of the map (SetAutoExposureMin, SetAutoExposureMax, SetBloomScale from its          logic_auto) sets the exposure range and the bloom, like in game",
        "L'env_tonemap_controller de la map (SetAutoExposureMin, SetAutoExposureMax, SetBloomScale depuis son          logic_auto) règle la plage d'exposition et le bloom, comme en jeu",
    ),
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
        "Lit mode: how many point and spot lights in view cast shadows (0: only the sun, at most 64, the brightest first;          beyond 8 the shadow maps are coarser). A light without a shadow map lights through walls.",
        "Mode éclairé : nombre de lumières ponctuelles et spots en vue qui projettent des ombres (0 : le soleil          seulement, 64 au plus, les plus brillantes d'abord ; au-delà de 8 les cartes d'ombre sont plus grossières).          Une lumière sans carte d'ombre éclaire à travers les murs.",
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
    ("shell.render.detail", "Detail props preview (grass)", "Aperçu des detail props (herbe)"),
    (
        "shell.render.detail_tip",
        "Scatters the detail sprites and models of the detail.vbsp group named by the %detailtype of a material (grass and the like) over the displacements and brush faces that use it, like the game does at compile time: only around the 3D camera, a bounded number, fading with the distance",
        "Répartit les sprites et modèles de détail du groupe du detail.vbsp nommé par le %detailtype d'un matériau (herbe et autres) sur les displacements et faces de brush qui l'utilisent, comme le jeu à la compilation : seulement autour de la caméra 3D, en nombre borné, avec un fondu selon la distance",
    ),
    (
        "shell.render.water_tip",
        "Draws water like in game: refraction of what is under it, water fog, reflection of the scene and the sky, \
         moving waves. Off: the water shows its tool texture.",
        "Dessine l'eau comme en jeu : réfraction de ce qui est dessous, brouillard de l'eau, reflet de la scène et \
         du ciel, vagues animées. Désactivé : l'eau montre sa texture d'outil.",
    ),
    ("shell.render.water_detail", "Water detail", "Détail de l'eau"),
    (
        "shell.render.water_detail_tip",
        "Like the Water Detail video option of the game. Reflect all detail (r_waterforceexpensive and          r_waterforcereflectentities): expensive water at every distance, reflecting the world, the props and the          brush entities, the sky and the 3D skybox drawn again mirrored about its surface. Reflect world detail:          expensive water near the camera (cheap past $cheapwaterenddistance), props and brush entities only with          $reflectentities. Simple reflections: cheap water everywhere.",
        "Comme l'option vidéo Détail de l'eau du jeu. Reflets complets (r_waterforceexpensive et          r_waterforcereflectentities) : eau coûteuse à toute distance, qui reflète le monde, les props et les          entités de brushes, le ciel et la skybox 3D dessinés une seconde fois en miroir par rapport à sa surface.          Reflet du monde : eau coûteuse près de la caméra (bon marché au-delà de $cheapwaterenddistance), props et          entités de brushes seulement avec $reflectentities. Reflets simples : eau bon marché partout.",
    ),
    ("shell.render.water_simple", "Simple reflections", "Reflets simples"),
    ("shell.render.water_reflect_world", "Reflect world", "Reflet du monde"),
    ("shell.render.water_reflect_all", "Reflect all", "Reflets complets"),
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
    ("shell.render.walk_mode", "Walkability colors", "Couleurs de praticabilité"),
    (
        "shell.render.walk_mode_tip",
        "Source rules (three classes), one limit angle (green below, red above) or an angle gradient with the limit in the middle",
        "Règles de Source (trois classes), un seul angle limite (vert en dessous, rouge au-dessus) ou un dégradé d'angle centré sur la limite",
    ),
    ("shell.render.walk_mode_source", "Source rules", "Règles de Source"),
    ("shell.render.walk_mode_limit", "Limit angle", "Angle limite"),
    ("shell.render.walk_mode_gradient", "Angle gradient", "Dégradé d'angle"),
    ("shell.render.walk_angle", "Walkable angle", "Angle praticable"),
    (
        "shell.render.walk_angle_tip",
        "Steepest slope shown as walkable by the limit and gradient modes, in degrees from flat. The Source limit is 45.57 (normal Z of 0.7)",
        "Pente la plus raide montrée comme praticable par les modes limite et dégradé, en degrés depuis l'horizontale. La limite de Source est 45,57 (normale Z de 0,7)",
    ),
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
    ("shell.render.animation_fps", "Animations", "Animations"),
    (
        "shell.render.animation_fps_tip",
        "Frames per second of animated materials, particle effects and flickering lights while nothing else moves (0: every frame)",
        "Images par seconde des matériaux animés, des effets de particules et des lumières qui clignotent quand rien d'autre ne bouge (0 : chaque image)",
    ),
    ("shell.render.animation_idle", "Pause animations after", "Mettre les animations en pause après"),
    (
        "shell.render.animation_idle_tip",
        "Animations stop after this many seconds without mouse or keyboard input and start again at the next input (0: never)",
        "Les animations s'arrêtent après ce nombre de secondes sans souris ni clavier et reprennent à la prochaine action (0 : jamais)",
    ),
    ("shell.render.animate_in_background", "Animate in the background", "Animer en arrière-plan"),
    (
        "shell.render.animate_in_background_tip",
        "Keep animated materials, particle effects and flickering lights moving while another window is active (uses processor time)",
        "Continuer d'animer les matériaux, les effets de particules et les lumières qui clignotent quand une autre fenêtre est active (utilise le processeur)",
    ),
    ("shell.render.vsync", "Vertical sync", "Synchronisation verticale"),
    (
        "shell.render.vsync_tip",
        "Waits for the screen refresh (applies immediately)",
        "Attend le rafraîchissement de l'écran (s'applique immédiatement)",
    ),
    ("shell.render.vram", "Graphics memory", "Mémoire graphique"),
    ("shell.render.vram_budget", "Graphics memory budget", "Budget de mémoire graphique"),
    ("shell.render.vram_auto", "Automatic", "Automatique"),
    (
        "shell.render.vram_budget_tip",
        "How much graphics card memory Hammer Pro may use. Automatic: 90% of what Windows gives the program (or 3 GB, 1.5 GB on integrated graphics, when the driver does not tell), at most 2 GB on integrated graphics, which use the system memory. Above 85% the maps open in other tabs are freed from the graphics card; above the budget textures are shown at a lower resolution until memory is free again.",
        "Quantité de mémoire de la carte graphique que Hammer Pro peut utiliser. Automatique : 90 % de ce que Windows donne au programme (ou 3 Go, 1,5 Go sur une puce graphique intégrée, quand le pilote ne le dit pas), au plus 2 Go sur une puce graphique intégrée, qui utilise la mémoire du système. Au-delà de 85 %, les maps ouvertes dans les autres onglets sont retirées de la carte graphique ; au-delà du budget, les textures sont affichées en plus basse résolution jusqu'à ce que la mémoire se libère.",
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
    (
        "shell.render.texture_streaming",
        "Load texture detail by distance",
        "Charger le détail des textures selon la distance",
    ),
    (
        "shell.render.texture_streaming_tip",
        "Textures used only far from every view are kept at a smaller size that looks the same at that distance, and come back to full size before the camera gets close. Saves graphics memory on big maps. The loading screen of a map stays until every texture close to the camera is at full size.",
        "Les textures utilisées seulement loin de toutes les vues sont gardées à une taille réduite qui rend pareil à cette distance, et reviennent en pleine taille avant que la caméra ne s'approche. Économise la mémoire graphique sur les grandes maps. L'écran de chargement d'une map reste jusqu'à ce que chaque texture proche de la caméra soit en pleine taille.",
    ),
    ("shell.render.vram_now", "In use: {0} MB of {1} MB ({2})", "Utilisée : {0} Mo sur {1} Mo ({2})"),
    ("shell.render.vram_driver", "measured by the driver", "mesurée par le pilote"),
    ("shell.render.vram_estimate", "estimated", "estimée"),
    ("shell.render.perf", "Performance", "Performance"),
    ("shell.render.perf_mode", "Performance mode", "Mode de performance"),
    ("shell.render.perf_auto", "Auto", "Auto"),
    ("shell.render.perf_off", "Off (full quality)", "Désactivé (qualité maximale)"),
    ("shell.render.perf_low", "Low", "Bas"),
    ("shell.render.perf_very_low", "Very low", "Très bas"),
    (
        "shell.render.perf_auto_tip",
        "Full quality. Only if the 3D view stays under 20 FPS for several seconds the quality is lowered one step, with a notice and a button to restore it.",
        "Qualité maximale. Seulement si la vue 3D reste sous 20 images par seconde pendant plusieurs secondes, la qualité baisse d'un cran, avec un avis et un bouton pour la rétablir.",
    ),
    (
        "shell.render.perf_off_tip",
        "Always full quality, never lowered automatically.",
        "Toujours la qualité maximale, jamais baissée automatiquement.",
    ),
    (
        "shell.render.perf_low_tip",
        "75% render resolution, no antialiasing, no bloom, 4 shadowed lights, a 2048 sun shadow, textures up to 1024 pixels, water reflects the world only.",
        "Résolution de rendu à 75 %, sans anticrénelage ni bloom, 4 lumières avec ombres, ombre du soleil en 2048, textures jusqu'à 1024 pixels, l'eau reflète seulement le monde.",
    ),
    (
        "shell.render.perf_very_low_tip",
        "50% render resolution, no antialiasing, bloom, light shadows, reflections, normal maps or fog volumes, a 1024 sun shadow, textures up to 512 pixels.",
        "Résolution de rendu à 50 %, sans anticrénelage, bloom, ombres des lumières, reflets, normal maps ni volumes de brouillard, ombre du soleil en 1024, textures jusqu'à 512 pixels.",
    ),
    ("shell.render.render_scale", "Render scale (3D view)", "Échelle de rendu (vue 3D)"),
    (
        "shell.render.render_scale_tip",
        "Draws the 3D view at this share of its size and stretches it to the window. 100% is full quality; lower is faster on weak graphics cards.",
        "Dessine la vue 3D à cette part de sa taille puis l'étire à la fenêtre. 100 % est la qualité maximale ; plus bas est plus rapide sur les cartes graphiques faibles.",
    ),
    (
        "shell.render.perf_hud",
        "Show frame time and memory in the 3D view",
        "Afficher le temps d'image et la mémoire dans la vue 3D",
    ),
    (
        "shell.render.perf_hud_tip",
        "FPS, CPU and GPU time per frame (shadows, scene, post effects, water), memory in use and the quality level.",
        "Images par seconde, temps CPU et GPU par image (ombres, scène, effets, eau), mémoire utilisée et niveau de qualité.",
    ),
    ("shell.render.perf_now", "Quality level now: {0} ({1})", "Niveau de qualité actuel : {0} ({1})"),
    ("shell.render.perf_restore", "Restore full quality", "Rétablir la qualité maximale"),
    ("shell.render.perf_resume", "Allow automatic reduction again", "Réautoriser la réduction automatique"),
    ("shell.render.vram_free", "Free graphics memory now", "Libérer la mémoire graphique maintenant"),
    (
        "shell.render.vram_free_tip",
        "Frees the maps of other tabs, the textures and models no open map uses, and the thumbnails",
        "Libère les maps des autres onglets, les textures et modèles qu'aucune map ouverte n'utilise, et les miniatures",
    ),
];
