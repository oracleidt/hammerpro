use std::path::PathBuf;

use hp_app::runtime::ShaderRequest;
use serde_json::json;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let root = std::env::var_os("HP_SCRATCH").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join("hp-ui-textures-shader").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn png(path: &Path, w: u32, h: u32) {
    let px: Vec<u8> = (0..w * h).flat_map(|i| [(i % w * 255 / w) as u8, (i / w * 255 / h) as u8, 90, 255]).collect();
    image::save_buffer(path, &px, w, h, image::ColorType::Rgba8).expect("write test png");
}

fn frame(ctx: &egui::Context, editor: &mut ShaderEditor, state: &mut AppState) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 900.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        let ctx = ui.ctx().clone();
        editor.show(&ctx, state);
    });
    out.textures_delta.clear();
}

fn settle(ctx: &egui::Context, editor: &mut ShaderEditor, state: &mut AppState) {
    for _ in 0..400 {
        frame(ctx, editor, state);
        if !editor.preview.busy() && editor.preview.result.is_some() {
            frame(ctx, editor, state);
            if !editor.preview.busy() {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the preview never finished");
}

fn setup() -> (egui::Context, ShaderEditor, AppState) {
    let state = hp_app::testing::test_state();
    crate::register_translations();
    (egui::Context::default(), ShaderEditor::default(), state)
}

#[test]
fn opens_an_image_and_previews_it() {
    let dir = scratch("open_image");
    let img = dir.join("brick.png");
    png(&img, 64, 32);
    let (ctx, mut editor, mut state) = setup();
    state.runtime.shader.open(ShaderRequest { path: Some(img.clone()), ..Default::default() });
    frame(&ctx, &mut editor, &mut state);
    assert!(state.runtime.shader.show && state.runtime.shader.request.is_none());
    assert_eq!(editor.graph().nodes.len(), 2);
    assert_eq!(editor.export.name, "brick");
    settle(&ctx, &mut editor, &mut state);
    let result = editor.preview.result.as_ref().unwrap();
    assert_eq!((result.width, result.height), (256, 128), "the long edge follows the preview size");
    assert!(editor.preview.image.is_some());
    assert!(!editor.preview.thumbs.is_empty());
    assert!(editor.diagnostics.iter().all(|d| d.severity != hp_shader::Severity::Error), "{:?}", editor.diagnostics);
}

#[test]
fn presets_open_from_requests_and_a_companion_file_wins() {
    let dir = scratch("companion");
    let img = dir.join("rock.png");
    png(&img, 16, 16);
    let (ctx, mut editor, mut state) = setup();
    state.runtime.shader.open(ShaderRequest {
        path: Some(img.clone()),
        preset: Some("normal_from_height".to_owned()),
        ..Default::default()
    });
    frame(&ctx, &mut editor, &mut state);
    assert_eq!(editor.graph().output_ids().len(), 2);
    let mut g = hp_shader::preset("tint", "rock.png").unwrap();
    g.settings.width = 128;
    hp_shader::save_file(&hp_shader::companion_path(&img), &g).unwrap();
    state.runtime.shader.open(ShaderRequest { path: Some(img), ..Default::default() });
    frame(&ctx, &mut editor, &mut state);
    assert_eq!(editor.graph().settings.width, 128, "the graph saved next to the image is opened");
    assert!(editor.path().is_some());
}

#[test]
fn edits_are_undone_in_steps() {
    let (ctx, mut editor, mut state) = setup();
    state.runtime.shader.open(ShaderRequest { preset: Some("tint".to_owned()), ..Default::default() });
    frame(&ctx, &mut editor, &mut state);
    let tint = editor.graph().nodes.iter().find(|n| n.kind == "tint").map(|n| n.id).unwrap();
    editor.doc.graph.set_param(tint, "amount", json!(0.25)).unwrap();
    editor.doc.commit();
    editor.doc.graph.set_param(tint, "amount", json!(0.5)).unwrap();
    editor.doc.graph.set_param(tint, "amount", json!(0.75)).unwrap();
    editor.doc.commit();
    assert!(editor.doc.dirty());
    editor.do_undo();
    assert_eq!(editor.graph().param_value(tint, "amount"), Some(json!(0.25)));
    editor.do_redo();
    assert_eq!(editor.graph().param_value(tint, "amount"), Some(json!(0.75)));
    editor.do_undo();
    editor.do_undo();
    assert_eq!(editor.graph().param_value(tint, "amount"), Some(json!(1.0)));
    assert!(!editor.doc.dirty());
    editor.do_undo();
    assert_eq!(editor.graph().nodes.len(), 3, "nothing left to undo");
}

#[test]
fn new_from_a_preset_can_be_undone() {
    let (_, mut editor, _) = setup();
    editor.doc.replace(hp_shader::preset("seamless", "").unwrap());
    editor.doc.commit();
    assert_eq!(editor.graph().nodes.len(), 3);
    editor.do_undo();
    assert!(editor.graph().nodes.is_empty());
}

#[test]
fn saving_and_opening_round_trip_with_relative_sources() {
    let dir = scratch("save_open");
    let img = dir.join("tex").join("wall.png");
    std::fs::create_dir_all(img.parent().unwrap()).unwrap();
    png(&img, 8, 8);
    let (ctx, mut editor, mut state) = setup();
    state.runtime.shader.open(ShaderRequest { path: Some(img.clone()), ..Default::default() });
    frame(&ctx, &mut editor, &mut state);
    let file = dir.join("graph.hpshader");
    editor.save_to(&mut state, file.clone());
    assert!(!editor.doc.dirty());
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("tex/wall.png") && !text.contains(&dir.to_string_lossy().replace('\\', "/")), "{text}");
    let (ctx2, mut other, mut state2) = setup();
    state2.runtime.shader.open(ShaderRequest { path: Some(file), ..Default::default() });
    frame(&ctx2, &mut other, &mut state2);
    settle(&ctx2, &mut other, &mut state2);
    assert!(other.diagnostics.iter().all(|d| d.code != "missing_texture"), "{:?}", other.diagnostics);
    assert!(other.preview.image.is_some(), "relative sources resolve against the graph file");
}

#[test]
fn a_broken_file_is_reported_not_loaded() {
    let dir = scratch("broken");
    let file = dir.join("bad.hpshader");
    std::fs::write(&file, "{ not json").unwrap();
    let (ctx, mut editor, mut state) = setup();
    editor.doc.replace(hp_shader::preset("blank", "").unwrap());
    state.runtime.shader.open(ShaderRequest { path: Some(file), ..Default::default() });
    frame(&ctx, &mut editor, &mut state);
    assert_eq!(editor.graph().nodes.len(), 2, "the current graph is kept");
}

#[test]
fn the_editor_ui_renders_every_section() {
    let dir = scratch("render");
    let img = dir.join("a.png");
    png(&img, 16, 16);
    let (ctx, mut editor, mut state) = setup();
    state.runtime.shader.open(ShaderRequest {
        path: Some(img),
        preset: Some("tile_blend".to_owned()),
        ..Default::default()
    });
    frame(&ctx, &mut editor, &mut state);
    settle(&ctx, &mut editor, &mut state);
    for id in editor.graph().nodes.iter().map(|n| n.id).collect::<Vec<_>>() {
        editor.canvas.selected = vec![id];
        frame(&ctx, &mut editor, &mut state);
    }
    editor.canvas.selected = editor.graph().nodes.iter().map(|n| n.id).collect();
    frame(&ctx, &mut editor, &mut state);
    editor.canvas.open_library_center();
    frame(&ctx, &mut editor, &mut state);
    assert!(editor.canvas.library_open());
    editor.tile = 3;
    editor.channel = Channel::Alpha;
    editor.node_previews = false;
    frame(&ctx, &mut editor, &mut state);
}

#[test]
fn opening_over_unsaved_changes_can_be_undone() {
    let (ctx, mut editor, mut state) = setup();
    state.runtime.shader.open(ShaderRequest { preset: Some("tint".to_owned()), ..Default::default() });
    frame(&ctx, &mut editor, &mut state);
    let tint = editor.graph().nodes.iter().find(|n| n.kind == "tint").map(|n| n.id).unwrap();
    editor.doc.graph.set_param(tint, "amount", json!(0.1)).unwrap();
    assert!(editor.doc.dirty());
    state.runtime.shader.open(ShaderRequest { preset: Some("seamless".to_owned()), ..Default::default() });
    frame(&ctx, &mut editor, &mut state);
    assert!(!editor.doc.dirty());
    assert!(editor.graph().nodes.iter().all(|n| n.kind != "tint"));
    editor.do_undo();
    let amount = editor.graph().param_value(tint, "amount").and_then(|v| v.as_f64()).unwrap();
    assert!((amount - 0.1).abs() < 1e-6, "{amount}");
}

#[test]
fn graphs_can_be_pushed_in_and_read_back_live() {
    let (ctx, mut editor, mut state) = setup();
    let g = hp_shader::preset("seamless", "a.png").unwrap();
    state.runtime.shader.open(ShaderRequest {
        graph_json: Some(g.to_json()),
        name: Some("My Tex".to_owned()),
        ..Default::default()
    });
    frame(&ctx, &mut editor, &mut state);
    assert_eq!(editor.graph().nodes.len(), 3);
    assert_eq!(editor.export.name, "my_tex");
    let live = state.runtime.shader.live.clone().expect("the open graph is mirrored for MCP");
    assert_eq!(hp_shader::Graph::parse(&live.json).unwrap().0, *editor.graph());
    let ops = vec![json!({ "op": "add_node", "type": "blur" })];
    editor.apply_ops(&ops).unwrap();
    frame(&ctx, &mut editor, &mut state);
    let live = state.runtime.shader.live.clone().unwrap();
    assert!(live.dirty);
    assert_eq!(hp_shader::Graph::parse(&live.json).unwrap().0.nodes.len(), 4);
}

#[test]
fn every_key_used_by_the_editor_is_translated() {
    crate::register_translations();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("shader");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if path.extension().is_none_or(|e| e != "rs") || name == "tr.rs" || name == "tests.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for marker in ["t!(\"", "tf!(\""] {
            for (i, _) in text.match_indices(marker) {
                if text[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric()) {
                    continue;
                }
                let rest = &text[i + marker.len()..];
                let Some(end) = rest.find('"') else { continue };
                if !hp_app::i18n::has_key(&rest[..end]) {
                    missing.push(format!("{name}: {}", &rest[..end]));
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}
