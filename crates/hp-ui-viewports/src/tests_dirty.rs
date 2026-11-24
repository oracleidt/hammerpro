use egui::{Rect, pos2, vec2};
use glam::DVec3;
use hp_app::testing::test_state;
use hp_render::{ViewOptions, ViewParams};

use super::*;

const KINDS: [ViewKind; 4] = [ViewKind::Perspective, ViewKind::Top, ViewKind::Front, ViewKind::Side];

fn rect() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 480.0))
}

fn view_key(state: &AppState, index: usize) -> (ViewParams, ViewOptions, u64) {
    let cfg = RenderConfig::from_state(state);
    let view = view_info(index, KINDS[index], rect(), &state.cameras, &cfg, 1.0);
    let mut layers = layers::default_layers();
    for layer in &mut layers {
        layer.prepare(state);
    }
    let mut overlay = Overlay::default();
    for layer in &layers {
        layer.add(state, &view, &mut overlay);
    }
    (view_params(&view, &cfg), view_options(&view, &cfg), render_cache::overlay_hash(&overlay))
}

fn keys(state: &AppState) -> Vec<(ViewParams, ViewOptions, u64)> {
    (0..4).map(|i| view_key(state, i)).collect()
}

fn selected_state() -> AppState {
    let mut state = test_state();
    state.settings.render.camera_in_2d = true;
    state.settings.helpers.show_entity_names = true;
    state.doc.set_selection(&[hp_editor::ObjRef::Entity(5)]);
    state
}

#[test]
fn moving_the_3d_camera_keeps_the_2d_views_as_they_are() {
    let mut state = selected_state();
    let before = keys(&state);
    let cam = &mut state.cameras.cam3d;
    cam.pos += DVec3::new(320.0, -64.0, 48.0);
    cam.yaw_deg += 35.0;
    cam.pitch_deg -= 10.0;
    let after = keys(&state);
    assert_ne!(before[0], after[0], "the 3D view follows its camera");
    for i in 1..4 {
        assert_eq!(before[i], after[i], "the {:?} view is not redrawn when only the 3D camera moves", KINDS[i]);
    }
}

#[test]
fn moving_a_2d_camera_only_redraws_that_view() {
    let mut state = selected_state();
    let before = keys(&state);
    state.cameras.cam2d_mut(ViewKind::Top).center += glam::DVec2::new(256.0, 128.0);
    let after = keys(&state);
    assert_ne!(before[1], after[1], "the top view follows its camera");
    for i in [0, 2, 3] {
        assert_eq!(before[i], after[i], "the {:?} view is not redrawn when the top view pans", KINDS[i]);
    }
    zoom_all_2d(&mut state, 2.0);
    let zoomed = keys(&state);
    assert_eq!(after[0], zoomed[0], "zooming the 2D views leaves the 3D view alone");
}

#[test]
fn an_idle_frame_reuses_every_view() {
    let state = selected_state();
    let first = keys(&state);
    let second = keys(&state);
    let mut cache = RenderCache::default();
    for (i, ((params, options, overlay), again)) in first.iter().zip(&second).enumerate() {
        let key = |(params, options, overlay): (ViewParams, ViewOptions, u64)| ViewKey {
            params,
            options,
            content: 3,
            lighting: 0,
            animation: 0,
            retained: 0,
            overlay,
            textures: 0,
        };
        cache.rendered(i, key((*params, *options, *overlay)));
        assert!(!cache.needs_render(i, &key(*again), true), "the {:?} view is reused on an idle frame", KINDS[i]);
    }
    assert_eq!(cache.rendered, 4);
}

#[test]
fn header_text_is_formatted_once_per_change() {
    let mut state = test_state();
    let cfg = RenderConfig::from_state(&state);
    let top = view_info(1, ViewKind::Top, rect(), &state.cameras, &cfg, 1.0);
    let mut slot = None;
    let first = header_text(&mut slot, &state, &top);
    assert!(first.as_deref().is_some_and(|t| t.contains('%')), "{first:?}");
    let cached = slot.clone().map(|(k, _)| k);
    assert_eq!(header_text(&mut slot, &state, &top), first);
    assert_eq!(slot.as_ref().map(|(k, _)| *k), cached, "the same zoom reuses the text");
    zoom_all_2d(&mut state, 2.0);
    let zoomed = view_info(1, ViewKind::Top, rect(), &state.cameras, &cfg, 1.0);
    let second = header_text(&mut slot, &state, &zoomed);
    assert_ne!(second, first, "a new zoom formats a new label");
    assert_eq!(second, Some(tf!("viewport.zoom", format_zoom_percent(zoomed.cam2d.zoom))));
    let persp = view_info(0, ViewKind::Perspective, rect(), &state.cameras, &cfg, 1.0);
    state.settings.grid.mode = hp_app::settings::GridMode::Global;
    assert_eq!(header_text(&mut slot, &state, &persp), None, "the 3D view shows no grid label in global mode");
    state.settings.grid.mode = hp_app::settings::GridMode::PerView;
    let grid = header_text(&mut slot, &state, &persp);
    assert_eq!(grid, Some(tf!("status.grid", fmt_coord(persp.grid))));
}
