use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Instant;

use egui::{Modifiers, Pos2, RawInput, Rect, vec2};
use glam::{DVec2, DVec3};
use hp_app::settings::{SelectModeSetting, Settings};
use hp_app::testing::{MockAssets, test_state};
use hp_app::view::{Camera2d, Camera3d};
use hp_app::{AppState, AssetsApi, Tool, ViewInfo, ViewKind};
use hp_editor::{Document, ObjRef, SelectOp};

use crate::input::PointerEvent;
use crate::math::{Handle, HandleMode, handle_pos};
use crate::select::SelectTool;

fn box_solid(id: u32, min: [f64; 3], max: [f64; 3], editor: &str) -> String {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let planes = [
        ([x0, y1, z1], [x1, y1, z1], [x1, y0, z1]),
        ([x0, y0, z0], [x1, y0, z0], [x1, y1, z0]),
        ([x0, y1, z1], [x0, y0, z1], [x0, y0, z0]),
        ([x1, y1, z0], [x1, y0, z0], [x1, y0, z1]),
        ([x1, y1, z1], [x0, y1, z1], [x0, y1, z0]),
        ([x1, y0, z0], [x0, y0, z0], [x0, y0, z1]),
    ];
    let mut s = format!("solid\n{{\n\"id\" \"{id}\"\n");
    for (k, (a, b, c)) in planes.iter().enumerate() {
        let p = |v: &[f64; 3]| format!("({} {} {})", v[0], v[1], v[2]);
        let _ = write!(
            s,
            "side\n{{\n\"id\" \"{}\"\n\"plane\" \"{} {} {}\"\n\"material\" \"DEV/DEV_MEASUREGENERIC01B\"\n\
             \"uaxis\" \"[1 0 0 0] 0.25\"\n\"vaxis\" \"[0 -1 0 0] 0.25\"\n\"rotation\" \"0\"\n\
             \"lightmapscale\" \"16\"\n\"smoothing_groups\" \"0\"\n}}\n",
            id * 10 + k as u32,
            p(a),
            p(b),
            p(c)
        );
    }
    s.push_str(editor);
    s.push_str("}\n");
    s
}

fn editor(group: Option<u32>) -> String {
    let g = group.map(|g| format!("\"groupid\" \"{g}\"\n")).unwrap_or_default();
    format!("editor\n{{\n\"color\" \"0 175 108\"\n{g}\"visgroupshown\" \"1\"\n\"visgroupautoshown\" \"1\"\n}}\n")
}

fn boxes_vmf() -> String {
    let mut world = String::new();
    world.push_str(&box_solid(10, [0.0, 0.0, 0.0], [64.0, 64.0, 64.0], &editor(None)));
    world.push_str(&box_solid(20, [128.0, 0.0, 0.0], [192.0, 64.0, 64.0], &editor(None)));
    world.push_str(&box_solid(30, [0.0, 128.0, 0.0], [64.0, 192.0, 64.0], &editor(Some(500))));
    world.push_str(&box_solid(40, [128.0, 128.0, 0.0], [192.0, 192.0, 64.0], &editor(Some(500))));
    format!(
        "versioninfo\n{{\n\"editorversion\" \"400\"\n\"mapversion\" \"1\"\n\"formatversion\" \"100\"\n\"prefab\" \"0\"\n}}\n\
         world\n{{\n\"id\" \"1\"\n\"mapversion\" \"1\"\n\"classname\" \"worldspawn\"\n{world}\
         group\n{{\n\"id\" \"500\"\n{}}}\n}}\n\
         entity\n{{\n\"id\" \"100\"\n\"classname\" \"func_detail\"\n{}{}{}}}\n\
         entity\n{{\n\"id\" \"200\"\n\"classname\" \"info_player_start\"\n\"angles\" \"0 0 0\"\n\"origin\" \"-128 32 0\"\n{}}}\n",
        editor(None),
        box_solid(110, [256.0, 0.0, 0.0], [320.0, 64.0, 64.0], ""),
        box_solid(120, [256.0, 128.0, 0.0], [320.0, 192.0, 64.0], ""),
        editor(None),
        editor(None),
    )
}

fn state_with(vmf: &str, assets: Box<dyn AssetsApi>) -> AppState {
    let _ = test_state();
    crate::register_translations();
    let mut state = AppState::new(Settings::default(), assets);
    match hp_vmf::parse(vmf) {
        Ok(map) => state.doc = Document::from_map(map),
        Err(e) => panic!("test map must parse: {e}"),
    }
    state
}

fn state_from(vmf: &str) -> AppState {
    state_with(vmf, Box::new(MockAssets::default()))
}

const A: ObjRef = ObjRef::Solid(10);
const C: ObjRef = ObjRef::Solid(30);
const D: ObjRef = ObjRef::Solid(40);
const DETAIL: ObjRef = ObjRef::Entity(100);
const S110: ObjRef = ObjRef::Solid(110);
const S120: ObjRef = ObjRef::Solid(120);
const START: ObjRef = ObjRef::Entity(200);

fn top(zoom: f64, center: DVec2) -> ViewInfo {
    ViewInfo {
        index: 1,
        kind: ViewKind::Top,
        rect: Rect::from_min_size(Pos2::new(10.0, 20.0), vec2(800.0, 600.0)),
        cam3d: Camera3d::default(),
        cam2d: Camera2d { center, zoom },
        pixels_per_point: 1.0,
        grid: 16.0,
    }
}

fn scr(view: &ViewInfo, p: DVec3) -> Pos2 {
    view.world_to_screen(p).expect("point in front of the camera")
}

fn click(tool: &mut SelectTool, state: &mut AppState, view: &ViewInfo, at: Pos2, mods: Modifiers) {
    tool.handle_event(state, view, PointerEvent::Click(at), mods);
}

fn drag(tool: &mut SelectTool, state: &mut AppState, view: &ViewInfo, from: Pos2, to: Pos2, mods: Modifiers) {
    tool.handle_event(state, view, PointerEvent::DragStart { origin: from, pos: from }, mods);
    for i in 1..=4 {
        let t = i as f32 / 4.0;
        tool.handle_event(state, view, PointerEvent::Drag(from + (to - from) * t), mods);
    }
    tool.handle_event(state, view, PointerEvent::DragEnd(to), mods);
}

fn sorted(mut v: Vec<ObjRef>) -> Vec<ObjRef> {
    v.sort();
    v
}

fn paint(ctx: &egui::Context, state: &AppState, tool: &SelectTool, view: &ViewInfo) -> (usize, f64) {
    let input =
        RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))), ..Default::default() };
    let start = Instant::now();
    let mut out = ctx.run_ui(input, |ui| {
        let painter = ui.painter_at(view.rect);
        tool.paint(state, view, &painter);
    });
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    out.textures_delta.clear();
    (out.shapes.len(), ms)
}

fn two_entities(far: &str) -> String {
    format!(
        "world\n{{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n}}\n\
         entity\n{{\n\"id\" \"3\"\n\"classname\" \"info_target\"\n\"origin\" \"0 0 0\"\n}}\n\
         entity\n{{\n\"id\" \"4\"\n\"classname\" \"info_target\"\n\"origin\" \"{far}\"\n}}\n"
    )
}

const THREE_SIDES: &str = r#"world
{
"id" "1"
"classname" "worldspawn"
solid
{
"id" "3"
side { "id" "300" "plane" "(-64 64 64) (64 64 64) (64 -64 64)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
side { "id" "301" "plane" "(-64 -64 0) (64 -64 0) (64 64 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
side { "id" "302" "plane" "(-64 64 64) (-64 -64 64) (-64 -64 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
}
}
"#;

#[test]
fn selection_drawing_cost_does_not_depend_on_its_screen_size() {
    let sky = format!(
        "world\n{{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n{}}}\n",
        box_solid(2, [-16384.0, -16384.0, -64.0], [16384.0, 16384.0, 0.0], &editor(None))
    );
    let mut maps: Vec<(String, String)> = ["1e6 0 0", "1e10 0 0", "1e20 0 0", "1e30 0 0", "-1e30 1e30 1e30"]
        .iter()
        .map(|far| (format!("two entities, far one at {far}"), two_entities(far)))
        .collect();
    maps.push(("skybox floor".to_owned(), sky));
    maps.push(("open solid".to_owned(), THREE_SIDES.to_owned()));
    let mut small = state_from(&two_entities("64 0 0"));
    small.doc.select_all();
    let tool = SelectTool::new();
    let ctx = egui::Context::default();
    let (reference, _) = paint(&ctx, &small, &tool, &top(0.25, DVec2::ZERO));
    assert!(reference > 0);

    for (name, vmf) in maps {
        let mut state = state_from(&vmf);
        state.doc.select_all();
        assert!(!state.doc.selection().is_empty(), "{name}: something is selected");
        for zoom in [Camera2d::MIN_ZOOM, 0.25, 4.0, Camera2d::MAX_ZOOM] {
            for center in [DVec2::ZERO, DVec2::new(5e9, 0.0), DVec2::new(16384.0, 100.0)] {
                let view = top(zoom, center);
                let mut tool = SelectTool::new();
                for mode in HandleMode::ALL {
                    tool.set_handle_mode(mode);
                    let (shapes, ms) = paint(&ctx, &state, &tool, &view);
                    assert!(shapes <= reference + 800, "{name} zoom {zoom} {center:?}: {shapes} shapes");
                    assert!(ms < 500.0, "{name} zoom {zoom}: {ms:.1} ms");
                }
                let at = view.rect.center();
                tool.handle_event(&mut state, &view, PointerEvent::DragStart { origin: at, pos: at }, Modifiers::NONE);
                tool.handle_event(&mut state, &view, PointerEvent::Drag(at + vec2(30.0, 10.0)), Modifiers::NONE);
                assert!(tool.is_dragging());
                let (shapes, _) = paint(&ctx, &state, &tool, &view);
                assert!(shapes <= reference + 1600, "{name} drag: {shapes} shapes");
                assert!(tool.cancel(&mut state));
                assert!(!state.doc.selection().is_empty());
                tool.handle_event(&mut state, &view, PointerEvent::Hover(at), Modifiers::NONE);
                assert!(tool.cursor(&state, &view, at).is_some());
            }
        }
        let v3 = ViewInfo { index: 0, kind: ViewKind::Perspective, ..top(1.0, DVec2::ZERO) };
        let tool = SelectTool::new();
        let _ = paint(&ctx, &state, &tool, &v3);
        let mut out = hp_render::Overlay::default();
        tool.overlay(&state, &v3, &mut out);
    }
}

#[test]
fn band_drawing_is_clipped_too() {
    let mut state = state_from(&boxes_vmf());
    let mut tool = SelectTool::new();
    let view = top(Camera2d::MAX_ZOOM, DVec2::ZERO);
    let far = Pos2::new(-5e7, -5e7);
    tool.handle_event(&mut state, &view, PointerEvent::DragStart { origin: far, pos: far }, Modifiers::NONE);
    tool.handle_event(&mut state, &view, PointerEvent::Drag(Pos2::new(5e7, 5e7)), Modifiers::NONE);
    let ctx = egui::Context::default();
    let (shapes, ms) = paint(&ctx, &state, &tool, &view);
    assert!(shapes < 1000, "{shapes} shapes");
    assert!(ms < 500.0, "{ms:.1} ms");
    tool.handle_event(&mut state, &view, PointerEvent::Drag(Pos2::new(5e7, 5e7)), Modifiers::ALT);
    let (shapes, _) = paint(&ctx, &state, &tool, &view);
    assert!(shapes < 1000, "{shapes} shapes");
}

#[test]
fn solids_mode_picks_one_solid_of_a_selected_brush_entity() {
    let mut state = state_from(&boxes_vmf());
    state.settings.edit.select_mode = SelectModeSetting::Solids;
    let mut tool = SelectTool::new();
    let v = top(0.5, DVec2::new(128.0, 96.0));
    let edge = scr(&v, DVec3::new(256.0, 32.0, 0.0));

    state.doc.select(&[DETAIL], SelectOp::Replace);
    click(&mut tool, &mut state, &v, edge, Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![S120]);
    click(&mut tool, &mut state, &v, edge + vec2(0.0, 6.0), Modifiers::COMMAND);
    assert_eq!(sorted(state.doc.selected_objects()), vec![S110, S120]);

    state.doc.select(&[DETAIL], SelectOp::Replace);
    tool.set_handle_mode(HandleMode::Scale);
    click(&mut tool, &mut state, &v, edge, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![S110]);
    assert_eq!(tool.handle_mode(), HandleMode::Scale);
    click(&mut tool, &mut state, &v, edge + vec2(0.0, 8.0), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![S110]);
    assert_eq!(tool.handle_mode(), HandleMode::Rotate);
    click(&mut tool, &mut state, &v, edge + vec2(0.0, -8.0), Modifiers::COMMAND);
    assert!(state.doc.selected_objects().is_empty());
    click(&mut tool, &mut state, &v, edge + vec2(0.0, 12.0), Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![S110]);

    state.settings.edit.select_mode = SelectModeSetting::Groups;
    state.doc.select(&[A], SelectOp::Replace);
    let c_edge = scr(&v, DVec3::new(0.0, 160.0, 0.0));
    click(&mut tool, &mut state, &v, c_edge, Modifiers::COMMAND);
    assert_eq!(sorted(state.doc.selected_objects()), vec![A, C, D]);
    click(&mut tool, &mut state, &v, c_edge + vec2(0.0, 8.0), Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![A]);
    assert_eq!(state.doc.history_len().0, 0);
}

#[test]
fn solids_mode_ctrl_click_in_3d_toggles_one_solid() {
    let mut state = state_from(&boxes_vmf());
    state.settings.edit.select_mode = SelectModeSetting::Solids;
    let mut tool = SelectTool::new();
    let cam = Camera3d { pos: DVec3::new(288.0, 32.0, 600.0), yaw_deg: 90.0, pitch_deg: -89.9, ..Camera3d::default() };
    let v = ViewInfo { index: 0, kind: ViewKind::Perspective, cam3d: cam, ..top(1.0, DVec2::ZERO) };
    let at = v.rect.center();
    state.doc.select(&[DETAIL], SelectOp::Replace);
    click(&mut tool, &mut state, &v, at, Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![S120]);
    click(&mut tool, &mut state, &v, at, Modifiers::COMMAND);
    assert_eq!(sorted(state.doc.selected_objects()), vec![S110, S120]);
    click(&mut tool, &mut state, &v, at, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![S110]);
}

#[test]
fn hover_outline_follows_select_mode_changes() {
    let mut state = state_from(&boxes_vmf());
    state.settings.view.hover_outline = true;
    let mut tool = SelectTool::new();
    let cam = Camera3d { pos: DVec3::new(32.0, 160.0, 600.0), yaw_deg: 90.0, pitch_deg: -89.9, ..Camera3d::default() };
    let v = ViewInfo { index: 0, kind: ViewKind::Perspective, cam3d: cam, ..top(1.0, DVec2::ZERO) };
    let at = v.rect.center();
    let outline = |tool: &SelectTool, state: &AppState| {
        let mut out = hp_render::Overlay::default();
        tool.overlay(state, &v, &mut out);
        out.lines_on_top.len()
    };
    tool.handle_event(&mut state, &v, PointerEvent::Hover(at), Modifiers::NONE);
    assert_eq!(sorted(tool.hovered_objects().to_vec()), vec![C, D]);
    assert_eq!(outline(&tool, &state), 24, "both boxes of the group");
    state.settings.edit.select_mode = SelectModeSetting::Solids;
    assert_eq!(outline(&tool, &state), 0);
    tool.handle_event(&mut state, &v, PointerEvent::Hover(at), Modifiers::NONE);
    assert_eq!(tool.hovered_objects(), &[C]);
    assert_eq!(outline(&tool, &state), 12);
    state.settings.edit.select_mode = SelectModeSetting::Groups;
    tool.handle_event(&mut state, &v, PointerEvent::Hover(at), Modifiers::NONE);
    assert_eq!(sorted(tool.hovered_objects().to_vec()), vec![C, D]);
    assert_eq!(outline(&tool, &state), 24);
}

#[test]
fn inside_box_selection_takes_only_whole_units() {
    let mut state = state_from(&boxes_vmf());
    state.settings.selection.box_confirm = false;
    let mut tool = SelectTool::new();
    tool.set_box_inside(true);
    let v = top(0.5, DVec2::new(128.0, 96.0));
    let band = |tool: &mut SelectTool, state: &mut AppState, a: [f64; 2], b: [f64; 2], mods: Modifiers| {
        if !mods.command {
            state.doc.clear_selection();
        }
        let from = scr(&v, DVec3::new(a[0], a[1], 0.0));
        let to = scr(&v, DVec3::new(b[0], b[1], 0.0));
        drag(tool, state, &v, from, to, mods);
        assert!(!tool.is_dragging());
        sorted(state.doc.selected_objects())
    };
    assert!(band(&mut tool, &mut state, [-16.0, 112.0], [80.0, 208.0], Modifiers::NONE).is_empty());
    assert_eq!(band(&mut tool, &mut state, [-16.0, 112.0], [208.0, 208.0], Modifiers::NONE), vec![C, D]);
    assert_eq!(band(&mut tool, &mut state, [-16.0, 112.0], [80.0, 208.0], Modifiers::ALT), vec![C, D]);
    assert!(band(&mut tool, &mut state, [240.0, -16.0], [336.0, 80.0], Modifiers::NONE).is_empty());
    assert_eq!(band(&mut tool, &mut state, [240.0, -16.0], [336.0, 208.0], Modifiers::NONE), vec![DETAIL]);
    state.settings.edit.select_mode = SelectModeSetting::Objects;
    assert_eq!(band(&mut tool, &mut state, [-16.0, 112.0], [80.0, 208.0], Modifiers::NONE), vec![C]);
    assert!(band(&mut tool, &mut state, [240.0, -16.0], [336.0, 80.0], Modifiers::NONE).is_empty());
    state.settings.edit.select_mode = SelectModeSetting::Solids;
    assert_eq!(band(&mut tool, &mut state, [240.0, -16.0], [336.0, 80.0], Modifiers::NONE), vec![S110]);
    state.settings.edit.select_mode = SelectModeSetting::Groups;
    assert_eq!(band(&mut tool, &mut state, [-160.0, 0.0], [-96.0, 64.0], Modifiers::NONE), vec![START]);
    state.doc.select(&[START], SelectOp::Replace);
    let added = band(&mut tool, &mut state, [-16.0, -16.0], [80.0, 208.0], Modifiers::COMMAND);
    assert_eq!(added, vec![A, START]);
}

struct OffsetAssets(MockAssets);

impl AssetsApi for OffsetAssets {
    fn status(&self) -> hp_app::state::AssetStatus {
        self.0.status()
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        None
    }
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        None
    }
    fn generation(&self) -> u64 {
        self.0.generation()
    }
    fn remount(&mut self, settings: &Settings) {
        self.0.remount(settings);
    }
    fn reload_fgd(&mut self, settings: &Settings) {
        self.0.reload_fgd(settings);
    }
    fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.0.materials()
    }
    fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.0.models()
    }
    fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.0.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        self.0.source_label(source)
    }
    fn material_info(&mut self, name: &str) -> Option<Arc<hp_app::state::MaterialInfo>> {
        self.0.material_info(name)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> hp_app::state::ThumbState {
        self.0.thumbnail(ctx, name)
    }
    fn model_info(&mut self, path: &str) -> hp_app::state::ModelState {
        self.0.model_info(path)
    }
    fn model_preview(
        &mut self,
        gfx: &mut hp_app::Gfx,
        path: &str,
        camera: &hp_app::state::PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        self.0.model_preview(gfx, path, camera, size_px)
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        let o = entity.origin()?;
        if entity.classname == "info_player_start" {
            return Some(hp_geom::Aabb::new(o + DVec3::new(-6.8, -21.0, 0.0), o + DVec3::new(6.8, 5.25, 71.27)));
        }
        self.0.entity_bounds(entity)
    }
    fn invalidate_material(&mut self, name: &str) {
        self.0.invalidate_material(name);
    }
    fn import_images(
        &mut self,
        files: &[std::path::PathBuf],
        opts: &hp_app::state::ImportOptions,
    ) -> Vec<Result<String, String>> {
        self.0.import_images(files, opts)
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut hp_app::Gfx>) {
        self.0.begin_frame(ctx, gfx);
    }
    fn sync_scene(&mut self, doc: &mut Document, gfx: &mut hp_app::Gfx, opts: &hp_app::state::SceneOptions) {
        self.0.sync_scene(doc, gfx, opts);
    }
    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId {
        self.0.render_material_id(name)
    }
    fn status_line(&self) -> Option<String> {
        self.0.status_line()
    }
}

fn start_entity(state: &AppState) -> (DVec3, DVec3) {
    let node = state.doc.scene().entity(200).expect("player start");
    (node.entity.origin().expect("origin"), node.entity.angles().expect("angles"))
}

#[test]
fn rotating_a_single_point_entity_keeps_its_origin() {
    let mut state = state_with(&boxes_vmf(), Box::new(OffsetAssets(MockAssets::default())));
    let mut tool = SelectTool::new();
    let v = top(1.0, DVec2::new(-128.0, 32.0));
    state.doc.select(&[START], SelectOp::Replace);
    tool.set_handle_mode(HandleMode::Rotate);
    let b = state.doc.object_bounds(START, &|e| state.entity_bounds(e)).expect("bounds");
    let rect = Rect::from_two_pos(scr(&v, b.min), scr(&v, b.max));
    let corner = handle_pos(rect, Handle::new(1, 1));
    let c = scr(&v, DVec3::new(-128.0, 32.0, 0.0));
    let r = corner - c;
    let a = 92f32.to_radians();
    let rotated = c + vec2(r.x * a.cos() + r.y * a.sin(), -r.x * a.sin() + r.y * a.cos());
    drag(&mut tool, &mut state, &v, corner, rotated, Modifiers::NONE);
    let (origin, angles) = start_entity(&state);
    assert_eq!(origin, DVec3::new(-128.0, 32.0, 0.0), "the origin stays on the grid");
    assert!((angles.y - 90.0).abs() < 1e-6, "{angles:?}");
    assert_eq!(state.doc.undo_label(), Some("Rotate"));

    let mut tool = SelectTool::new();
    tool.set_gizmo_rotate(true);
    let cam = Camera3d { pos: DVec3::new(-128.0, 32.0, 700.0), yaw_deg: 90.0, pitch_deg: -89.0, ..Camera3d::default() };
    let v3 = ViewInfo { index: 0, kind: ViewKind::Perspective, cam3d: cam, ..top(1.0, DVec2::ZERO) };
    let g = crate::gizmo::GizmoLayout::new(&v3, DVec3::new(-128.0, 32.0, 0.0)).expect("gizmo in view");
    let from = scr(&v3, g.ring_point(2, 8));
    assert_eq!(g.hit(&v3, from, crate::gizmo::GizmoMode::Rotate), Some(crate::gizmo::GizmoPart::Ring(2)));
    let to = scr(&v3, g.ring_point(2, 24));
    drag(&mut tool, &mut state, &v3, from, to, Modifiers::NONE);
    let (origin, angles) = start_entity(&state);
    assert!((origin - DVec3::new(-128.0, 32.0, 0.0)).length() < 1e-6, "{origin:?}");
    assert!((angles.y - 180.0).abs() < 1e-6 || (angles.y + 180.0).abs() < 1e-6, "{angles:?}");

    state.doc.select(&[START, A], SelectOp::Replace);
    let both = state.doc.selection_bounds(&|e| state.entity_bounds(e)).expect("bounds");
    let mut tool = SelectTool::new();
    tool.set_handle_mode(HandleMode::Rotate);
    let rect = Rect::from_two_pos(scr(&v, both.min), scr(&v, both.max));
    let corner = handle_pos(rect, Handle::new(1, 1));
    let c = scr(&v, both.center());
    let r = corner - c;
    let rotated = c + vec2(r.x * a.cos() + r.y * a.sin(), -r.x * a.sin() + r.y * a.cos());
    drag(&mut tool, &mut state, &v, corner, rotated, Modifiers::NONE);
    let (origin, _) = start_entity(&state);
    let expected = both.center() + DVec3::new(0.0, -(both.center().x + 128.0), 0.0);
    assert!((origin.truncate() - expected.truncate()).length() < 1e-6, "{origin:?} {expected:?}");
}

fn start_view_3d() -> ViewInfo {
    let cam = Camera3d { pos: DVec3::new(-128.0, -324.0, 36.0), yaw_deg: 90.0, pitch_deg: 0.0, ..Camera3d::default() };
    ViewInfo { index: 0, kind: ViewKind::Perspective, cam3d: cam, ..top(1.0, DVec2::ZERO) }
}

#[test]
fn double_click_on_a_floor_standing_entity_opens_its_properties() {
    let mut state = state_with(&boxes_vmf(), Box::new(OffsetAssets(MockAssets::default())));
    let mut tool = SelectTool::new();
    let v = start_view_3d();
    let center = v.rect.center();
    click(&mut tool, &mut state, &v, center, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![START]);
    let g = crate::gizmo::GizmoLayout::new(&v, DVec3::new(-128.0, 32.0, 0.0)).expect("gizmo in view");
    assert_eq!(
        g.hit(&v, center, crate::gizmo::GizmoMode::Move),
        Some(crate::gizmo::GizmoPart::Axis(2)),
        "repro: the arrow covers the entity"
    );
    tool.handle_event(&mut state, &v, PointerEvent::DoubleClick(center), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![START]);
    assert!(state.requested_actions.contains(&hp_app::Action::Properties), "properties open");
}

#[test]
fn ctrl_click_picks_through_the_gizmo() {
    let mut state = state_with(&boxes_vmf(), Box::new(OffsetAssets(MockAssets::default())));
    let mut tool = SelectTool::new();
    let v = start_view_3d();
    let center = v.rect.center();
    click(&mut tool, &mut state, &v, center, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![START]);
    click(&mut tool, &mut state, &v, center, Modifiers::COMMAND);
    assert!(state.doc.selected_objects().is_empty(), "Ctrl+click deselects through the gizmo");
    click(&mut tool, &mut state, &v, center, Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![START]);
    state.doc.select(&[START, A], SelectOp::Replace);
    let both = state.doc.selection_bounds(&|e| state.entity_bounds(e)).expect("bounds");
    let g = crate::gizmo::GizmoLayout::new(&v, both.center()).expect("gizmo in view");
    let arrow = scr(&v, g.center + DVec3::Z * g.size * 0.6);
    assert!(g.hit(&v, arrow, crate::gizmo::GizmoMode::Move).is_some());
    click(&mut tool, &mut state, &v, arrow, Modifiers::NONE);
    assert_eq!(sorted(state.doc.selected_objects()), sorted(vec![START, A]));
    state.requested_actions.clear();
    tool.handle_event(&mut state, &v, PointerEvent::DoubleClick(arrow), Modifiers::NONE);
    assert_eq!(sorted(state.doc.selected_objects()), sorted(vec![START, A]));
    assert!(state.requested_actions.is_empty());
    let before = state.doc.revision();
    drag(&mut tool, &mut state, &v, arrow, arrow - vec2(0.0, 40.0), Modifiers::NONE);
    assert_ne!(state.doc.revision(), before, "the gizmo drag moved the selection");
    assert_eq!(state.doc.undo_label(), Some("Move"));
}
