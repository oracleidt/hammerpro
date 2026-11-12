use egui::Pos2;
use glam::{DVec2, DVec3};
use hp_app::ViewKind;
use hp_geom::Aabb;

use crate::BlockTool;
use crate::common::Mouse;
use crate::tests_support::{info_2d, settle};

fn scrolled(view: &hp_app::ViewInfo, by: DVec2) -> hp_app::ViewInfo {
    let mut v = *view;
    v.cam2d.center += by;
    v
}

#[test]
fn a_new_block_keeps_growing_while_the_view_scrolls() {
    let mut state = settle();
    state.doc.clear_selection();
    let mut tool = BlockTool::new();
    let top = info_2d(ViewKind::Top, 1.0);
    let a = top.world_to_screen(DVec3::ZERO).expect("a");
    let edge = Pos2::new(780.0, 108.0);
    tool.handle_mouse(&mut state, &top, &Mouse::drag_start(a, a));
    tool.handle_mouse(&mut state, &top, &Mouse::drag(a, edge));
    let mut view = top;
    for _ in 0..4 {
        view = scrolled(&view, DVec2::new(50.0, 0.0));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(a, edge));
    }
    tool.handle_mouse(&mut state, &view, &Mouse::drag_end(a, edge));
    let b = tool.pending_box().expect("pending box");
    assert_eq!((b.min.x, b.min.y), (0.0, 0.0), "the start corner stays where it was pressed");
    assert_eq!((b.max.x, b.max.y), (576.0, 192.0), "the far corner is the snapped point under the cursor");
}

#[test]
fn a_pending_box_handle_follows_the_cursor_while_the_view_scrolls() {
    let mut state = settle();
    state.doc.clear_selection();
    let mut tool = BlockTool::new();
    let front = info_2d(ViewKind::Front, 1.0);
    let a = front.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).expect("a");
    let c = front.world_to_screen(DVec3::new(0.0, 64.0, 64.0)).expect("c");
    tool.handle_mouse(&mut state, &front, &Mouse::drag_start(a, a));
    tool.handle_mouse(&mut state, &front, &Mouse::drag_end(a, c));
    let before = tool.pending_box().expect("pending box");
    assert_eq!((before.max.y, before.max.z), (64.0, 64.0));
    let handle = front.world_to_screen(DVec3::new(0.0, 64.0, 32.0)).expect("right edge handle");
    let edge = Pos2::new(795.0, handle.y);
    tool.handle_mouse(&mut state, &front, &Mouse::drag_start(handle, handle));
    tool.handle_mouse(&mut state, &front, &Mouse::drag(handle, edge));
    let view = scrolled(&front, DVec2::new(301.0, 0.0));
    tool.handle_mouse(&mut state, &view, &Mouse::drag(handle, edge));
    tool.handle_mouse(&mut state, &view, &Mouse::drag_end(handle, edge));
    let after: Aabb = tool.pending_box().expect("still pending");
    let under = view.screen_to_2d(edge).x;
    assert!((after.max.y - (under / 16.0).round() * 16.0).abs() <= 16.0, "{after:?} vs {under}");
    assert!(after.max.y > 600.0, "{after:?}");
    assert_eq!(after.max.y % 16.0, 0.0, "still on the grid: {after:?}");
    assert_eq!((after.min.y, after.min.z, after.max.z), (before.min.y, before.min.z, before.max.z));
}
