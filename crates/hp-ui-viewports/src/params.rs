use egui::Rect;
use hp_app::AppState;
use hp_app::grid::GridFrame;
use hp_app::settings::{Disp2dSetting, Models2dSetting};
use hp_app::state::Cameras;
use hp_app::view::{ViewInfo, ViewKind};
use hp_render::{Detail2d, Grid3d, GridParams, Models2d, OrthoAxis, RenderMode, ViewOptions, ViewParams};

use crate::layout::pixel_size;

pub const SELECTION_TINT: f32 = 0.4;
pub const VERTEX_DOTS_MIN_ZOOM: f32 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderConfig {
    pub fov_deg: f64,
    pub far: f64,
    pub grid: Option<GridParams>,
    pub grid_size: f64,
    pub frames: [GridFrame; 4],
    pub background_2d: [f32; 3],
    pub background_3d: [f32; 3],
    pub selection: [f32; 4],
    pub mode: RenderMode,
    pub show_sprites: bool,
    pub show_models: bool,
    pub msaa: bool,
    pub game_pixel_grid: bool,
    pub near: f64,
    pub options_3d: ViewOptions,
    pub options_2d: ViewOptions,
    pub disp_2d: [Detail2d; 4],
}

pub fn detail_2d(mode: Disp2dSetting) -> Detail2d {
    match mode {
        Disp2dSetting::Mesh => Detail2d::All,
        Disp2dSetting::SelectedMesh => Detail2d::Selected,
        Disp2dSetting::Outline => Detail2d::None,
    }
}

fn rgb_f32(c: [u8; 3]) -> [f32; 3] {
    [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0]
}

impl RenderConfig {
    pub fn from_state(state: &AppState) -> Self {
        let v = &state.settings.view;
        let e = &state.settings.edit;
        let frames = FRAME_KINDS.map(|k| state.display_grid_frame(k));
        let grid_size = frames[0].nominal();
        let fov = f64::from(v.fov_deg);
        let far = f64::from(v.far_plane);
        let sel = rgb_f32(v.selection_color);
        let r = &state.settings.render;
        let finite = |x: f32, d: f32| if x.is_finite() { x } else { d };
        let options_3d = ViewOptions {
            grid_3d: r.grid_3d.then(|| Grid3d {
                size: grid_size as f32,
                major_every: v.grid_highlight_every.max(1),
                fade_cells: finite(r.grid_3d_fade_cells, 64.0).clamp(4.0, 4096.0),
                opacity: finite(r.grid_3d_opacity, 0.45).clamp(0.0, 1.0),
            }),
            model_distance: finite(r.model_distance, 0.0).max(0.0),
            show_sky: r.sky_preview,
            show_sky_3d: r.skybox_3d,
            sky_see_through: r.sky_see_through,
            water_preview: r.water_preview,
            water_detail: r.water_detail.detail(),
            normal_maps: r.normal_maps,
            cubemaps: r.cubemaps,
            light_shadows: r.light_shadows.min(hp_render::MAX_SHADOWED_LIGHTS),
            indirect_light: r.lit_indirect,
            model_lods: r.model_lods,
            sun_glow: r.sun_glow,
            show_fog: r.fog_preview,
            fog_volumes: r.fog_volume_preview,
            occlusion_culling: r.occlusion_culling,
            force_opaque: r.force_opaque,
            walkable: r.walkable,
            walk_marks: r.disp_walkable,
            cull_radius: if r.radius_culling { finite(r.cull_radius, 0.0).max(0.0) } else { 0.0 },
            model_fade: r.model_fade,
            collision_models: r.collision_models,
            collision_only: r.collision_only,
            selection_outline: r.selection_outline,
            game_wireframe: state.game_wireframe(),
            classic_colors: !r.object_colors_3d,
            ..ViewOptions::default()
        };
        let options_2d = ViewOptions {
            models_2d: match r.models_in_2d {
                Models2dSetting::Boxes => Models2d::Boxes,
                Models2dSetting::Wireframe => Models2d::Wireframe,
                Models2dSetting::SelectedOnly => Models2d::SelectedOnly,
            },
            fill_selected_2d: if r.fill_selected_2d {
                finite(r.fill_selected_2d_opacity, 0.2).clamp(0.0, 1.0)
            } else {
                0.0
            },
            vertex_dots_min_zoom: if r.vertex_dots_2d { VERTEX_DOTS_MIN_ZOOM } else { 0.0 },
            sprites_2d_px: if r.icons_in_2d && v.show_helpers {
                finite(r.icons_in_2d_px, 16.0).clamp(4.0, 64.0)
            } else {
                0.0
            },
            grid_dotted: r.grid_dotted,
            grid_highlight_64: r.grid_highlight_64,
            grid_min_px: finite(r.grid_min_px, 4.0).clamp(2.0, 64.0),
            detail_2d: detail_2d(r.displacements_2d),
            walk_marks: r.disp_walkable,
            source_brush_2d: r.disp_source_2d,
            classic_colors: !r.object_colors_2d,
            ..ViewOptions::default()
        };
        Self {
            fov_deg: if fov.is_finite() { fov.clamp(10.0, 150.0) } else { 75.0 },
            far: if far.is_finite() { far.clamp(256.0, 1.0e6) } else { 32768.0 },
            grid: e.show_grid.then_some(GridParams {
                size: grid_size as f32,
                major_every: v.grid_highlight_every.max(1),
                show_1024: v.show_1024_grid,
                cells: true,
                opacity: 1.0,
            }),
            grid_size,
            frames,
            background_2d: rgb_f32(v.background_2d),
            background_3d: rgb_f32(v.background_3d),
            selection: [sel[0], sel[1], sel[2], SELECTION_TINT],
            mode: state.render_mode(),
            show_sprites: v.show_helpers,
            show_models: v.show_models,
            msaa: v.msaa,
            game_pixel_grid: r.game_pixel_grid,
            near: f64::from(r.near()),
            options_3d,
            options_2d,
            disp_2d: std::array::from_fn(|i| detail_2d(r.displacements_2d_of(i))),
        }
    }
}

const FRAME_KINDS: [ViewKind; 4] = [ViewKind::Perspective, ViewKind::Top, ViewKind::Front, ViewKind::Side];

impl RenderConfig {
    pub fn frame(&self, kind: ViewKind) -> &GridFrame {
        let i = FRAME_KINDS.iter().position(|k| *k == kind).unwrap_or(0);
        &self.frames[i]
    }
}

pub fn view_info(
    index: usize,
    kind: ViewKind,
    rect: Rect,
    cameras: &Cameras,
    cfg: &RenderConfig,
    ppp: f32,
) -> ViewInfo {
    let mut cam3d = cameras.cam3d;
    cam3d.fov_y_deg = cfg.fov_deg;
    cam3d.near = if cfg.near.is_finite() && cfg.near > 0.0 { cfg.near } else { 1.0 };
    cam3d.far = cfg.far.max(cam3d.near * 2.0);
    ViewInfo {
        index,
        kind,
        rect,
        cam3d,
        cam2d: cameras.cam2d(kind),
        pixels_per_point: if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 },
        grid: cfg.frame(kind).nominal(),
    }
}

pub fn ortho_axis(kind: ViewKind) -> Option<OrthoAxis> {
    match kind {
        ViewKind::Top => Some(OrthoAxis::Top),
        ViewKind::Front => Some(OrthoAxis::Front),
        ViewKind::Side => Some(OrthoAxis::Side),
        _ => None,
    }
}

fn f32x3(v: glam::DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

pub fn view_options(view: &ViewInfo, cfg: &RenderConfig) -> ViewOptions {
    if !view.is_2d() {
        return cfg.options_3d;
    }
    let detail_2d = cfg.disp_2d.get(view.index).copied().unwrap_or(cfg.options_2d.detail_2d);
    ViewOptions { detail_2d, ..cfg.options_2d }
}

pub fn view_params(view: &ViewInfo, cfg: &RenderConfig) -> ViewParams {
    let size = pixel_size(view.rect, view.pixels_per_point);
    let kind = match ortho_axis(view.kind) {
        Some(axis) => hp_render::ViewKind::Ortho {
            axis,
            center: f32x3(view.kind.from_2d(view.cam2d.center, 0.0)),
            pixels_per_unit: (view.cam2d.zoom * f64::from(view.pixels_per_point)) as f32,
        },
        None => {
            let cam = &view.cam3d;
            hp_render::ViewKind::Perspective {
                eye: f32x3(cam.pos),
                forward: f32x3(cam.forward()),
                up: [0.0, 0.0, 1.0],
                fov_y_deg: cam.fov_y_deg as f32,
                near: cam.near as f32,
                far: cam.far as f32,
            }
        }
    };
    let is_2d = view.is_2d();
    ViewParams {
        size,
        kind,
        mode: cfg.mode,
        grid: if is_2d {
            let frame = cfg.frame(view.kind);
            let shader = if frame.is_plain() {
                Some((frame.step.x, frame.major_every, true))
            } else if frame.bounds.is_some() {
                Some((frame.outside, 0, true))
            } else {
                Some((frame.step.x, 0, false))
            };
            cfg.grid.zip(shader).map(|(g, (size, major, cells))| GridParams {
                size: size as f32,
                major_every: if major > 0 { major } else { g.major_every },
                cells,
                ..g
            })
        } else {
            None
        },
        background: if is_2d { cfg.background_2d } else { cfg.background_3d },
        selection_color: cfg.selection,
        show_sprites: cfg.show_sprites,
        show_models: cfg.show_models,
        msaa: cfg.msaa,
        game_pixel_grid: cfg.game_pixel_grid && !is_2d,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::pixel_align;
    use egui::{Pos2, pos2, vec2};
    use glam::{DVec2, DVec3};
    use hp_app::view::{Camera2d, Camera3d};

    fn cfg() -> RenderConfig {
        let state = hp_app::testing::test_state();
        RenderConfig::from_state(&state)
    }

    fn cameras() -> Cameras {
        Cameras {
            cam3d: Camera3d {
                pos: DVec3::new(-300.0, -220.0, 150.0),
                yaw_deg: 33.0,
                pitch_deg: -17.0,
                ..Camera3d::default()
            },
            top: Camera2d { center: DVec2::new(37.0, -12.5), zoom: 0.73 },
            front: Camera2d { center: DVec2::new(-5.0, 64.0), zoom: 1.0 },
            side: Camera2d { center: DVec2::new(128.0, 3.25), zoom: 2.5 },
        }
    }

    const POINTS: [[f64; 3]; 6] = [
        [0.0, 0.0, 0.0],
        [64.0, -32.0, 16.0],
        [-100.0, 50.0, 80.0],
        [200.0, 120.0, -40.0],
        [10.5, 3.25, 7.75],
        [-37.0, -61.0, 12.0],
    ];

    fn max_error(view: &ViewInfo, params: &ViewParams) -> f32 {
        let ppp = view.pixels_per_point;
        let mut worst = 0.0f32;
        for p in POINTS {
            let p = DVec3::from_array(p);
            let (Some(a), Some(b)) = (view.world_to_screen(p), params.world_to_screen(p.as_vec3().to_array())) else {
                continue;
            };
            let a_px = [(a.x - view.rect.left()) * ppp, (a.y - view.rect.top()) * ppp];
            worst = worst.max((a_px[0] - b[0]).abs()).max((a_px[1] - b[1]).abs());
        }
        worst
    }

    fn rects() -> Vec<Rect> {
        vec![
            Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)),
            Rect::from_min_size(pos2(100.0, 40.0), vec2(801.0, 599.0)),
            Rect::from_min_size(pos2(13.0, 7.0), vec2(512.0, 383.0)),
            Rect::from_min_size(pos2(250.5, 33.25), vec2(640.4, 480.6)),
        ]
    }

    #[test]
    fn the_game_pixel_grid_offsets_the_drawn_view_by_half_a_pixel() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let view = view_info(0, ViewKind::Perspective, rect, &cameras(), &cfg(), 1.0);
        let on = view_params(&view, &cfg());
        let off = view_params(&view, &RenderConfig { game_pixel_grid: false, ..cfg() });
        assert!(on.game_pixel_grid && !off.game_pixel_grid);
        assert!(max_error(&view, &off) < 0.01, "without it the picking rays and the drawn image agree");
        let err = max_error(&view, &on);
        assert!((err - 0.5).abs() < 0.01, "with it the drawn image moves half a pixel: {err}");
        let flat = view_info(0, ViewKind::Top, rect, &cameras(), &cfg(), 1.0);
        assert!(!view_params(&flat, &cfg()).game_pixel_grid, "the 2D views keep the plain grid");
    }

    #[test]
    fn view_info_and_render_agree_3d() {
        let cfg = RenderConfig { game_pixel_grid: false, ..cfg() };
        for ppp in [1.0f32, 1.5] {
            for rect in rects() {
                let rect = pixel_align(rect, ppp, false);
                let view = view_info(0, ViewKind::Perspective, rect, &cameras(), &cfg, ppp);
                let params = view_params(&view, &cfg);
                let err = max_error(&view, &params);
                assert!(err < 0.01, "3D {rect:?} ppp {ppp}: {err} px");
                for s in [rect.center(), rect.min + vec2(10.0, 20.0), rect.max - vec2(3.0, 7.0)] {
                    let (o1, d1) = view.ray(s);
                    let px = [(s.x - rect.left()) * ppp, (s.y - rect.top()) * ppp];
                    let (o2, d2) = params.screen_ray(px);
                    assert!((o1 - DVec3::from_array(o2.map(f64::from))).length() < 1e-3);
                    let d2 = DVec3::from_array(d2.map(f64::from));
                    assert!((d1 - d2).length() < 1e-4, "ray {d1:?} vs {d2:?}");
                }
            }
        }
    }

    #[test]
    fn view_info_and_render_agree_2d() {
        let cfg = cfg();
        for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
            for ppp in [1.0f32, 1.5] {
                for rect in rects() {
                    let raw = pixel_align(rect, ppp, false);
                    let view = view_info(1, kind, raw, &cameras(), &cfg, ppp);
                    let params = view_params(&view, &cfg);
                    let err = max_error(&view, &params);
                    assert!(err <= 0.5 + 0.01, "{kind:?} {raw:?} ppp {ppp}: {err} px");
                    let size = pixel_size(raw, ppp);
                    if size[0] % 2 == 1 && size[1] % 2 == 1 {
                        assert!(err < 0.01, "odd sizes must agree exactly: {err}");
                    }
                    let odd = pixel_align(rect, ppp, true);
                    let view = view_info(1, kind, odd, &cameras(), &cfg, ppp);
                    let params = view_params(&view, &cfg);
                    let err = max_error(&view, &params);
                    assert!(err < 0.01, "{kind:?} odd {odd:?} ppp {ppp}: {err} px");
                    let s: Pos2 = odd.center() + vec2(17.0, -9.0);
                    let px = [(s.x - odd.left()) * ppp, (s.y - odd.top()) * ppp];
                    let a = view.screen_to_2d(s);
                    let b = params.screen_to_world_2d(px).map(|w| kind.to_2d(DVec3::from_array(w.map(f64::from))));
                    let Some(b) = b else { panic!("2D view must map screen to world") };
                    assert!((a - b).length() * view.cam2d.zoom * f64::from(ppp) < 0.01, "{a:?} vs {b:?}");
                }
            }
        }
    }

    #[test]
    fn even_size_discrepancy_is_half_a_pixel() {
        let cfg = cfg();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let view = view_info(1, ViewKind::Top, rect, &cameras(), &cfg, 1.0);
        let params = view_params(&view, &cfg);
        let c = view.kind.from_2d(view.cam2d.center, 0.0);
        let Some(b) = params.world_to_screen(c.as_vec3().to_array()) else { panic!("ortho always projects") };
        assert!((b[0] - 400.5).abs() < 1e-3 && (b[1] - 300.5).abs() < 1e-3, "{b:?}");
        let Some(a) = view.world_to_screen(c) else { panic!("ortho always projects") };
        assert!((a.x - 400.0).abs() < 1e-3 && (a.y - 300.0).abs() < 1e-3);
    }

    #[test]
    fn params_follow_settings() {
        let mut state = hp_app::testing::test_state();
        state.settings.edit.show_grid = false;
        state.settings.view.fov_deg = 90.0;
        state.settings.view.msaa = false;
        let cfg = RenderConfig::from_state(&state);
        assert!(cfg.grid.is_none());
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 200.0));
        let v3 = view_info(0, ViewKind::Perspective, rect, &state.cameras, &cfg, 1.5);
        assert_eq!(v3.cam3d.fov_y_deg, 90.0);
        let p3 = view_params(&v3, &cfg);
        assert_eq!(p3.size, [450, 300]);
        assert!(!p3.msaa && p3.grid.is_none());
        assert!(matches!(p3.kind, hp_render::ViewKind::Perspective { fov_y_deg, .. } if fov_y_deg == 90.0));
        state.settings.edit.show_grid = true;
        let cfg = RenderConfig::from_state(&state);
        let v2 = view_info(1, ViewKind::Side, rect, &state.cameras, &cfg, 1.0);
        let p2 = view_params(&v2, &cfg);
        assert!(p2.grid.is_some());
        assert!(matches!(p2.kind, hp_render::ViewKind::Ortho { axis: OrthoAxis::Side, .. }));
        assert_eq!(p2.background, rgb_f32(state.settings.view.background_2d));
    }

    #[test]
    fn object_colors_per_view_kind() {
        let mut state = hp_app::testing::test_state();
        let cfg = RenderConfig::from_state(&state);
        assert!(!cfg.options_2d.classic_colors, "2D views use object colors by default");
        assert!(cfg.options_3d.classic_colors, "3D keeps the classic colors by default");
        state.settings.render.object_colors_2d = false;
        state.settings.render.object_colors_3d = true;
        let cfg = RenderConfig::from_state(&state);
        assert!(cfg.options_2d.classic_colors && !cfg.options_3d.classic_colors);
    }

    #[test]
    fn displacement_modes_per_view() {
        use hp_app::settings::Disp2dViewSetting;
        let mut state = hp_app::testing::test_state();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 200.0));
        let opts = |state: &AppState, index: usize, kind: ViewKind| {
            let cfg = RenderConfig::from_state(state);
            view_options(&view_info(index, kind, rect, &state.cameras, &cfg, 1.0), &cfg)
        };
        let o = opts(&state, 1, ViewKind::Top);
        assert_eq!((o.detail_2d, o.walk_marks), (Detail2d::All, false));
        let r = &mut state.settings.render;
        r.displacements_2d = Disp2dSetting::Outline;
        r.displacements_2d_views[2] = Disp2dViewSetting::Own(Disp2dSetting::SelectedMesh);
        r.disp_walkable = true;
        assert_eq!(opts(&state, 1, ViewKind::Top).detail_2d, Detail2d::None);
        assert_eq!(opts(&state, 2, ViewKind::Front).detail_2d, Detail2d::Selected);
        assert_eq!(opts(&state, 3, ViewKind::Side).detail_2d, Detail2d::None);
        assert!(opts(&state, 1, ViewKind::Top).walk_marks);
        let o3 = opts(&state, 0, ViewKind::Perspective);
        assert!(o3.walk_marks && o3.detail_2d == Detail2d::All, "3D views always show the grid");
    }
}
