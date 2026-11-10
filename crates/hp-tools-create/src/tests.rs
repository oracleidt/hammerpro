#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Sense};
use glam::{DVec2, DVec3};
use hp_app::testing::{run_ctx, run_ui};
use hp_app::{Tool, ToolId, ViewInput, ViewKind};
use hp_render::Overlay;

use crate::tests_support::{fgd_state, info_2d, info_3d, info_3d_looking_down, settle};
use crate::{BlockTool, ClipTool, DecalTool, EntityTool, OverlayTool, Primitive, TR};

const SOURCES: [(&str, &str); 11] = [
    ("block_structures.rs", include_str!("block_structures.rs")),
    ("block.rs", include_str!("block.rs")),
    ("clip.rs", include_str!("clip.rs")),
    ("common.rs", include_str!("common.rs")),
    ("decal.rs", include_str!("decal.rs")),
    ("entity.rs", include_str!("entity.rs")),
    ("extrude.rs", include_str!("extrude.rs")),
    ("overlay_tool.rs", include_str!("overlay_tool.rs")),
    ("picker.rs", include_str!("picker.rs")),
    ("polygon.rs", include_str!("polygon.rs")),
    ("lib.rs", include_str!("lib.rs")),
];

#[test]
fn translation_table_is_clean() {
    let problems = hp_app::i18n::lint_table(TR);
    assert!(problems.is_empty(), "{problems:#?}");
    for (k, _, _) in TR {
        assert!(k.starts_with("create."), "{k} must use the create. prefix");
        assert!(!hp_app::strings::TR.iter().any(|(c, _, _)| c == k), "{k} collides with a core key");
    }
}

#[test]
fn placeholders_match_between_languages() {
    for (k, en, fr) in TR {
        for i in 0..4 {
            let p = format!("{{{i}}}");
            assert_eq!(en.contains(&p), fr.contains(&p), "{k}: placeholder {p}");
        }
    }
}

#[test]
fn every_used_key_is_translated() {
    let known = |k: &str| TR.iter().any(|(c, _, _)| *c == k) || hp_app::strings::TR.iter().any(|(c, _, _)| *c == k);
    let mut checked = 0;
    for (file, src) in SOURCES {
        for prefix in ["\"create.", "t!(\"", "tf!(\""] {
            let mut rest = src;
            while let Some(i) = rest.find(prefix) {
                let boundary = rest[..i].chars().next_back().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
                let start = i + prefix.len() - if prefix.starts_with('"') { prefix.len() - 1 } else { 0 };
                let tail = &rest[start..];
                let end = tail.find('"').unwrap_or(0);
                let key = &tail[..end];
                rest = &tail[end..];
                if !boundary || key.is_empty() || key.contains(' ') || !key.contains('.') || key.ends_with('.') {
                    continue;
                }
                assert!(known(key), "{file}: missing translation key {key}");
                checked += 1;
            }
        }
    }
    assert!(checked > 50, "only {checked} keys found");
}

#[test]
fn every_translation_is_used() {
    for (k, _, _) in TR {
        let quoted = format!("\"{k}\"");
        assert!(SOURCES.iter().any(|(_, src)| src.contains(&quoted)), "unused translation key {k}");
    }
}

#[test]
fn no_forbidden_dashes_in_sources() {
    for (file, src) in SOURCES {
        assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'), "{file} contains an em or en dash");
    }
}

#[test]
fn tool_list() {
    let ids: Vec<ToolId> = crate::tools().iter().map(|t| t.id()).collect();
    assert_eq!(
        ids,
        vec![
            ToolId::Block,
            ToolId::Polygon,
            ToolId::Curve,
            ToolId::Entity,
            ToolId::Clip,
            ToolId::Slice,
            ToolId::Extrude,
            ToolId::Bevel,
            ToolId::Decal,
            ToolId::Overlay,
            ToolId::Path,
            ToolId::Road,
            ToolId::Pipe
        ]
    );
}

fn all_views() -> Vec<hp_app::ViewInfo> {
    vec![
        info_3d(DVec3::new(-300.0, -300.0, 200.0), 45.0, -20.0),
        info_2d(ViewKind::Top, 0.5),
        info_2d(ViewKind::Front, 0.5),
        info_2d(ViewKind::Side, 0.5),
    ]
}

fn draw_all(state: &mut hp_app::AppState, tools: &mut [Box<dyn Tool>]) {
    let views = all_views();
    for tool in tools.iter() {
        for v in &views {
            let mut out = Overlay::default();
            tool.overlay(state, v, &mut out);
            let _ = tool.cursor(state, v, v.rect.center());
        }
        assert!(tool.status_hint(state).is_some_and(|h| !h.is_empty()));
    }
    run_ui(state, |ui, state| {
        for tool in tools.iter_mut() {
            ui.horizontal(|ui| tool.options_ui(ui, state));
            for v in &views {
                tool.paint(state, v, ui.painter());
            }
        }
    });
}

#[test]
fn every_tool_draws_without_gpu_or_definitions() {
    let _lang = hp_app::testing::lang_lock();
    let mut state = settle();
    assert!(state.gfx.is_none());
    let mut tools = crate::tools();
    for t in tools.iter_mut() {
        t.activate(&mut state);
    }
    draw_all(&mut state, &mut tools);
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    draw_all(&mut state, &mut tools);
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    for t in tools.iter_mut() {
        t.cancel(&mut state);
        t.deactivate(&mut state);
    }
}

#[test]
fn tools_draw_their_pending_state() {
    let mut state = fgd_state();
    state.doc.set_selection(&[hp_editor::ObjRef::Solid(2), hp_editor::ObjRef::Solid(3)]);
    let views = all_views();
    let top = &views[1];
    let down = info_3d_looking_down(DVec3::new(0.0, 0.0, 512.0));
    let floor_pt = down.world_to_screen(DVec3::new(10.0, 10.0, 0.0)).expect("screen");

    let mut block = BlockTool::new();
    block.set_pending_box(Some(hp_geom::Aabb::new(DVec3::ZERO, DVec3::splat(128.0))));
    let mut clip = ClipTool::new();
    clip.set_line(ViewKind::Top, DVec2::new(0.0, -400.0), DVec2::new(0.0, 400.0));
    let mut entity = EntityTool::new();
    let mut decal = DecalTool::new();
    let mut overlay = OverlayTool::new();
    use crate::common::Mouse;
    entity.handle_mouse(&mut state, &down, &Mouse::hover(floor_pt));
    decal.handle_mouse(&mut state, &down, &Mouse::hover(floor_pt));
    overlay.handle_mouse(&mut state, &down, &Mouse::hover(floor_pt));
    let mut tools: Vec<Box<dyn Tool>> =
        vec![Box::new(block), Box::new(clip), Box::new(entity), Box::new(decal), Box::new(overlay)];
    draw_all(&mut state, &mut tools);
    for (i, tool) in tools.iter().enumerate().take(2) {
        let mut out = Overlay::default();
        tool.overlay(&state, &views[0], &mut out);
        assert!(!out.is_empty(), "tool {i} draws in 3D");
    }
    let mut out = Overlay::default();
    tools[2].overlay(&state, top, &mut out);
    assert!(!out.is_empty(), "the ghost entity shows in 2D views");

    let mut block = BlockTool::new();
    block.set_pending_box(Some(hp_geom::Aabb::new(DVec3::ZERO, DVec3::new(256.0, 256.0, 64.0))));
    for kind in Primitive::ALL {
        block.options.kind = kind;
        let mut out = Overlay::default();
        block.overlay(&state, &views[0], &mut out);
        assert!(!out.lines.is_empty(), "{kind:?}");
        run_ui(&mut state, |ui, state| block.options_ui(ui, state));
    }
}

#[test]
fn polygon_double_click_with_real_egui_events() {
    let mut state = settle();
    state.doc.clear_selection();
    let mut tool = crate::PolygonTool::new();
    let view = info_2d(ViewKind::Top, 1.0);
    let at = |x: f64, y: f64| view.world_to_screen(DVec3::new(x, y, 0.0)).expect("screen");
    let press = |pos: Pos2, pressed: bool| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut frames: Vec<Vec<Event>> = Vec::new();
    for p in [at(0.0, 0.0), at(64.0, 0.0), at(64.0, 64.0)] {
        frames.extend([vec![Event::PointerMoved(p)], vec![press(p, true)], vec![press(p, false)], vec![]]);
    }
    let last = at(0.0, 64.0);
    frames.extend([
        vec![Event::PointerMoved(last)],
        vec![press(last, true)],
        vec![press(last, false)],
        vec![press(last, true)],
        vec![press(last, false)],
        vec![],
    ]);
    let ctx = egui::Context::default();
    let before = state.doc.stats().solids;
    for (i, events) in frames.into_iter().enumerate() {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events,
            time: Some(i as f64 * 0.02),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let resp = ui.allocate_rect(view.rect, Sense::click_and_drag());
            let pointer = ui.ctx().pointer_latest_pos();
            let input =
                ViewInput { view: &view, response: &resp, pointer, modifiers: Modifiers::NONE, navigating: false };
            tool.view_input(&mut state, &input);
        });
        out.textures_delta.clear();
    }
    assert_eq!(state.doc.stats().solids, before + 1, "points left: {:?}", tool.points());
}

#[test]
fn view_input_with_real_egui_events() {
    let mut state = settle();
    state.doc.clear_selection();
    let mut tool = BlockTool::new();
    let view = info_2d(ViewKind::Top, 1.0);
    let a = view.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).expect("a");
    let b = view.world_to_screen(DVec3::new(64.0, 64.0, 0.0)).expect("b");
    let mid = a + (b - a) * 0.5;
    let press = |pos: Pos2, pressed: bool| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let frames: Vec<Vec<Event>> = vec![
        vec![Event::PointerMoved(a)],
        vec![press(a, true)],
        vec![Event::PointerMoved(mid)],
        vec![Event::PointerMoved(b)],
        vec![press(b, false)],
        vec![],
    ];
    let ctx = egui::Context::default();
    for (i, events) in frames.into_iter().enumerate() {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events,
            time: Some(i as f64 * 0.05),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let resp = ui.allocate_rect(view.rect, Sense::click_and_drag());
            let pointer = ui.ctx().pointer_latest_pos();
            let input =
                ViewInput { view: &view, response: &resp, pointer, modifiers: Modifiers::NONE, navigating: false };
            tool.view_input(&mut state, &input);
        });
        out.textures_delta.clear();
    }
    assert_eq!(tool.pending_box(), Some(hp_geom::Aabb::new(DVec3::ZERO, DVec3::new(64.0, 64.0, 64.0))));

    let before = state.doc.stats().solids;
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        events: vec![Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut handled = false;
    let mut out = ctx.run_ui(input, |ui| handled = tool.key_input(&mut state, ui.ctx()));
    out.textures_delta.clear();
    assert!(handled);
    assert_eq!(state.doc.stats().solids, before + 1);
    assert!(tool.pending_box().is_none());
}

fn key_event(key: egui::Key, modifiers: Modifiers) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        events: vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }],
        ..Default::default()
    }
}

fn clip_frame(ctx: &egui::Context, tool: &mut ClipTool, state: &mut hp_app::AppState, input: RawInput) -> (bool, bool) {
    let mut handled = false;
    let mut clicked = false;
    let mut out = ctx.run_ui(input, |ui| {
        clicked = ui.button("File").clicked();
        let _ = ui.button("Edit");
        handled = tool.key_input(state, ui.ctx());
    });
    out.textures_delta.clear();
    (handled, clicked)
}

#[test]
fn clip_mode_cycles_with_tab_without_moving_keyboard_focus() {
    let mut state = settle();
    state.doc.set_selection(&[hp_editor::ObjRef::Solid(2)]);
    let mut tool = ClipTool::new();
    tool.activate(&mut state);
    tool.set_line(ViewKind::Top, DVec2::new(0.0, 300.0), DVec2::new(0.0, -300.0));
    let ctx = egui::Context::default();
    clip_frame(&ctx, &mut tool, &mut state, RawInput::default());

    let (handled, _) = clip_frame(&ctx, &mut tool, &mut state, key_event(egui::Key::Tab, Modifiers::NONE));
    assert!(handled);
    assert_eq!(tool.keep, hp_editor::ClipKeep::Front);
    assert_eq!(ctx.memory(|m| m.focused()), None, "Tab must not give a widget keyboard focus");
    let (handled, _) = clip_frame(&ctx, &mut tool, &mut state, key_event(egui::Key::Tab, Modifiers::SHIFT));
    assert!(handled);
    assert_eq!(tool.keep, hp_editor::ClipKeep::Both);
    assert_eq!(ctx.memory(|m| m.focused()), None);
    clip_frame(&ctx, &mut tool, &mut state, key_event(egui::Key::Tab, Modifiers::NONE));
    assert_eq!(tool.keep, hp_editor::ClipKeep::Front);

    let before = state.doc.stats().solids;
    let (handled, clicked) = clip_frame(&ctx, &mut tool, &mut state, key_event(egui::Key::Enter, Modifiers::NONE));
    assert!(handled);
    assert!(!clicked, "Enter must not reach the File button");
    assert_eq!(state.doc.stats().solids, before, "keep front: one piece replaces the floor");
    let b = state.doc.scene().solid(2).expect("floor").bounds;
    assert_eq!((b.min.x, b.max.x), (-256.0, 0.0));

    tool.clear();
    let (handled, _) = clip_frame(&ctx, &mut tool, &mut state, key_event(egui::Key::Escape, Modifiers::NONE));
    assert!(!handled);
}

#[test]
fn clip_tab_respects_deliberate_widget_focus() {
    let mut state = settle();
    let mut tool = ClipTool::new();
    let ctx = egui::Context::default();
    clip_frame(&ctx, &mut tool, &mut state, RawInput::default());
    let mut first = None;
    let mut out = ctx.run_ui(RawInput::default(), |ui| {
        let r = ui.button("File");
        r.request_focus();
        first = Some(r.id);
        let _ = ui.button("Edit");
        tool.key_input(&mut state, ui.ctx());
    });
    out.textures_delta.clear();
    let (handled, _) = clip_frame(&ctx, &mut tool, &mut state, key_event(egui::Key::Tab, Modifiers::NONE));
    assert!(!handled);
    assert_eq!(tool.keep, hp_editor::ClipKeep::Both);
    let focused = ctx.memory(|m| m.focused());
    assert!(focused.is_some() && focused != first, "egui moved the focus to the next widget");
}

#[test]
fn selecting_the_clip_tool_again_cycles_the_mode() {
    let mut state = settle();
    let mut tool = ClipTool::new();
    tool.activate(&mut state);
    for expected in [hp_editor::ClipKeep::Front, hp_editor::ClipKeep::Back, hp_editor::ClipKeep::Both] {
        tool.reactivate(&mut state);
        assert_eq!(tool.keep, expected);
    }
    let sc = state.keys.shortcut(hp_app::Action::ToolClip).expect("default Shift+X");
    let label = hp_app::actions::shortcut_label(&sc);
    let hint = tool.status_hint(&state).expect("hint");
    assert!(hint.contains(&label), "{hint}");
    tool.set_line(ViewKind::Top, DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0));
    let hint = tool.status_hint(&state).expect("hint");
    assert!(hint.contains(&label) && hint.contains(&hp_app::actions::key_label(egui::Key::Enter)), "{hint}");
}

#[test]
fn block_hints_use_localized_key_labels() {
    use hp_app::actions::key_label_in;
    use hp_app::i18n::Lang;
    let mut state = settle();
    let mut tool = BlockTool::new();
    tool.set_pending_box(Some(hp_geom::Aabb::new(DVec3::ZERO, DVec3::splat(64.0))));
    let hint = tool.status_hint(&state).expect("hint");
    let names = |key: egui::Key| Lang::ALL.map(|l| key_label_in(key, l));
    for key in [egui::Key::Enter, egui::Key::Escape] {
        assert!(names(key).iter().any(|n| hint.contains(n.as_str())), "{key:?}: {hint}");
    }
    run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
    for (k, en, fr) in TR.iter().copied() {
        for text in [en, fr] {
            assert!(!text.contains("Escape") && !text.contains("(Enter)"), "{k}: use key_label: {text}");
        }
    }
}

#[test]
fn dialogs_free_context_smoke() {
    let mut state = fgd_state();
    let mut tools = crate::tools();
    run_ctx(&mut state, |ctx, state| {
        egui::Window::new("options").show(ctx, |ui| {
            for t in tools.iter_mut() {
                t.options_ui(ui, state);
            }
        });
    });
}

#[test]
fn real_maps_clip_create_and_place() {
    let maps = [
        r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples\ttt_traps.vmf",
        r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples\ttt_weaponcheck.vmf",
        r"C:\Users\oracle\Desktop\hammermcptest\test.vmf",
    ];
    for path in maps.iter().map(std::path::Path::new) {
        if path.exists() {
            check_real_map(path);
        } else {
            eprintln!("skipped: {} not found", path.display());
        }
    }
}

fn check_real_map(path: &std::path::Path) {
    let mut state = settle();
    state.doc = hp_editor::Document::open(path).expect("sample map loads");
    let stats = state.doc.stats();
    let Some(bounds) = stats.bounds else { return };
    let center = bounds.center();

    state.doc.select_all();
    let selected = state.doc.selected_solid_ids().len();
    let mut clip = ClipTool::new();
    clip.set_line(
        ViewKind::Top,
        DVec2::new(center.x + 0.5, bounds.min.y - 64.0),
        DVec2::new(center.x + 0.5, bounds.max.y + 64.0),
    );
    let started = std::time::Instant::now();
    let crossing = clip.crossing_count(&state);
    eprintln!("{}: {selected} solids, {crossing} cut, preview in {:?}", path.display(), started.elapsed());
    let solids = state.doc.stats().solids;
    if crossing > 0 {
        assert!(clip.apply(&mut state));
        let after = state.doc.stats();
        assert_eq!(after.solids, solids + crossing, "keep both adds one piece per cut solid");
        assert!(after.invalid_solids <= stats.invalid_solids);
        assert!(state.doc.undo());
        assert_eq!(state.doc.stats().solids, solids);
    }

    let mut block = BlockTool::new();
    let base = DVec3::new(center.x.round(), center.y.round(), bounds.max.z.round() + 64.0);
    for (i, kind) in Primitive::ALL.into_iter().enumerate() {
        block.options.kind = kind;
        block.options.create_as =
            if i % 2 == 0 { crate::CreateAs::World } else { crate::CreateAs::Entity("func_detail".to_owned()) };
        let b = hp_geom::Aabb::new(base, base + DVec3::new(128.0, 128.0, 96.0));
        assert!(block.create(&mut state, b).is_some(), "{kind:?}");
    }
    assert_eq!(state.doc.stats().invalid_solids, stats.invalid_solids);

    let view = info_3d_looking_down(DVec3::new(center.x, center.y, bounds.max.z + 4096.0));
    if let Some(hit) = crate::common::pick_surface(&state, &view, view.rect.center()) {
        let mut tool = OverlayTool::new();
        let id = tool.place(&mut state, hit.side_id, hit.face_normal, hit.face_plane.project(hit.point), false);
        let e = &state.doc.scene().entity(id.expect("overlay")).expect("entity").entity;
        assert_eq!(e.get("sides"), Some(hit.side_id.to_string().as_str()));
    }
}

#[test]
fn polygon_entity_decal_and_overlay_options_are_scriptable() {
    let mut state = settle();
    let mut poly = crate::PolygonTool::new();
    assert!(poly.set_option(&mut state, "tex_scale", "0.5"));
    assert!(!poly.set_option(&mut state, "tex_scale", "100"));
    assert!(poly.set_option(&mut state, "create_as", "func_detail"));
    assert!(!poly.set_option(&mut state, "create_as", "bad class!"));
    assert!(poly.set_option(&mut state, "keep_active", "off"));
    let values = poly.option_values();
    let value = |k: &str| values.iter().find(|o| o.key == k).map(|o| o.value.clone());
    assert_eq!(value("tex_scale").as_deref(), Some("0.5"));
    assert_eq!(value("create_as").as_deref(), Some("func_detail"));
    assert_eq!(value("keep_active").as_deref(), Some("off"));
    let mut decal = DecalTool::new();
    assert!(decal.set_option(&mut state, "snap", "off"));
    assert!(!decal.snap_to_grid);
    assert!(!decal.set_option(&mut state, "snap", "maybe"));
    let mut overlay = OverlayTool::new();
    assert!(overlay.set_option(&mut state, "snap", "on"));
    assert_eq!(overlay.option_values()[0].value, "on");
    let mut entity = EntityTool::new();
    assert!(entity.set_option(&mut state, "class", "prop_physics"));
    assert_eq!(state.current_entity_class, "prop_physics");
    assert!(!entity.set_option(&mut state, "class", "prop physics"));
    assert!(entity.set_option(&mut state, "model", r"models\props_c17\oildrum001.mdl"));
    assert_eq!(state.current_model.as_deref(), Some("models/props_c17/oildrum001.mdl"));
    assert!(!entity.set_option(&mut state, "model", "../../secret.mdl"));
    assert!(!entity.set_option(&mut state, "model", "models/a.txt"));
    assert!(entity.set_option(&mut state, "skin", "2"));
    assert!(entity.set_option(&mut state, "align", "on"));
    let values = entity.option_values();
    let value = |k: &str| values.iter().find(|o| o.key == k).map(|o| o.value.clone());
    assert_eq!(value("class").as_deref(), Some("prop_physics"));
    assert_eq!(value("model").as_deref(), Some("models/props_c17/oildrum001.mdl"));
    assert_eq!(value("skin").as_deref(), Some("2"));
    assert_eq!(value("align").as_deref(), Some("on"));
    assert!(entity.set_option(&mut state, "model", ""));
    assert_eq!(state.current_model, None);
    assert!(entity.set_option(&mut state, "class", ""), "empty is the default point entity");
    for tool in [&mut entity as &mut dyn hp_app::Tool, &mut poly, &mut decal, &mut overlay] {
        for o in tool.option_values() {
            assert!(tool.set_option(&mut state, o.key, &o.value), "{} takes back {}", o.key, o.value);
        }
    }
}
