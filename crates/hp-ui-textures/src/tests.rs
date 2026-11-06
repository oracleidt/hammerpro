use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hp_app::AppState;
use hp_app::settings::Settings;
use hp_app::state::{
    AssetStatus, AssetsApi, Gfx, ImportOptions, MaterialInfo, ModelState, PreviewCamera, SceneOptions, ThumbState,
};
use hp_app::testing::{MockAssets, run_ctx, run_ui, test_state};

use crate::search::{Category, SourceFilter};
use crate::{TR, TextureBrowser, TextureDialogs};

fn setup() -> AppState {
    let state = test_state();
    crate::register_translations();
    state
}

pub(crate) fn scratch(name: &str) -> PathBuf {
    let root = std::env::var_os("HP_TEST_SCRATCH").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target")
                .join("hp-ui-textures")
                .join("scratch")
        },
        PathBuf::from,
    );
    let dir = root.join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

struct FsAssets {
    inner: MockAssets,
    fs: hp_fs::GameFs,
    remounts: usize,
    async_infos: bool,
}

impl AssetsApi for FsAssets {
    fn status(&self) -> AssetStatus {
        AssetStatus::Ready
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        Some(&self.fs)
    }
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        None
    }
    fn generation(&self) -> u64 {
        self.fs.generation()
    }
    fn remount(&mut self, _settings: &Settings) {
        self.remounts += 1;
        self.fs.refresh();
    }
    fn reload_fgd(&mut self, _settings: &Settings) {}
    fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.materials()
    }
    fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.models()
    }
    fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        self.fs.source(source).map(|s| s.label).unwrap_or_default()
    }
    fn material_info(&mut self, name: &str) -> Option<Arc<MaterialInfo>> {
        if self.async_infos {
            return None;
        }
        let m = self.fs.material(name)?;
        Some(Arc::new(MaterialInfo {
            name: name.to_owned(),
            width: 64,
            height: 64,
            shader: m.shader.clone(),
            keywords: m.keywords().map(str::to_owned).collect(),
            surface_prop: m.surface_prop().map(str::to_owned),
            ..MaterialInfo::default()
        }))
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> ThumbState {
        self.inner.thumbnail(ctx, name)
    }
    fn model_info(&mut self, path: &str) -> ModelState {
        self.inner.model_info(path)
    }
    fn model_preview(
        &mut self,
        gfx: &mut Gfx,
        path: &str,
        camera: &PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        self.inner.model_preview(gfx, path, camera, size_px)
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        self.inner.entity_bounds(entity)
    }
    fn invalidate_material(&mut self, name: &str) {
        self.inner.invalidate_material(name);
    }
    fn import_images(&mut self, files: &[PathBuf], opts: &ImportOptions) -> Vec<Result<String, String>> {
        self.inner.import_images(files, opts)
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut Gfx>) {
        self.inner.begin_frame(ctx, gfx);
    }
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut Gfx, opts: &SceneOptions) {
        self.inner.sync_scene(doc, gfx, opts);
    }
    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId {
        self.inner.render_material_id(name)
    }
    fn status_line(&self) -> Option<String> {
        None
    }
}

fn fs_state(name: &str) -> (AppState, PathBuf) {
    let dir = scratch(name);
    let folder = dir.join("My Bricks");
    let _ = std::fs::create_dir_all(folder.join("red"));
    let _ = std::fs::write(
        folder.join("red").join("wall.vmt"),
        "// wall\n\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"custom/my_bricks/red/wall\"\n\t\"%keywords\" \"urban,brick\"\n}\n",
    );
    let _ = std::fs::write(folder.join("dot.png"), PNG_1X1);
    let mut settings = Settings::default();
    crate::folders::add_custom_dir(&mut settings, &folder);
    let fs = hp_fs::GameFs::empty();
    let custom = hp_fs::CustomDir::with_prefix(&folder, "custom/my_bricks");
    assert!(fs.add_custom_dir(custom).is_ok());
    let assets = FsAssets { inner: MockAssets::default(), fs, remounts: 0, async_infos: false };
    let mut state = AppState::new(settings, Box::new(assets));
    if let Ok(map) = hp_vmf::parse(hp_app::testing::TEST_VMF) {
        state.doc = hp_editor::Document::from_map(map);
    }
    crate::register_translations();
    (state, folder)
}

pub(crate) fn custom_state(
    name: &str,
    folder_name: &str,
    files: &[(&str, &[u8])],
    async_infos: bool,
) -> (AppState, PathBuf) {
    let dir = scratch(name);
    let folder = dir.join(folder_name);
    for (rel, bytes) in files {
        let path = folder.join(rel);
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(&folder));
        let _ = std::fs::write(path, bytes);
    }
    let _ = std::fs::create_dir_all(&folder);
    let mut settings = Settings::default();
    crate::folders::add_custom_dir(&mut settings, &folder);
    let fs = hp_fs::GameFs::empty();
    let prefix = settings.game.custom_texture_dirs[0].prefix.clone();
    assert!(fs.add_custom_dir(hp_fs::CustomDir::with_prefix(&folder, prefix)).is_ok());
    let assets = FsAssets { inner: MockAssets::default(), fs, remounts: 0, async_infos };
    let mut state = AppState::new(settings, Box::new(assets));
    if let Ok(map) = hp_vmf::parse(hp_app::testing::TEST_VMF) {
        state.doc = hp_editor::Document::from_map(map);
    }
    crate::register_translations();
    (state, folder)
}

pub(crate) fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([120, 60, 30, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    let _ = img.write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

fn frame_with_events(
    ctx: &egui::Context,
    state: &mut AppState,
    pointer: Option<egui::Pos2>,
    events: Vec<egui::Event>,
    mut f: impl FnMut(&mut egui::Ui, &mut AppState),
) -> Vec<egui::Event> {
    let mut all: Vec<egui::Event> = pointer.map(egui::Event::PointerMoved).into_iter().collect();
    all.extend(events);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 900.0))),
        events: all,
        ..Default::default()
    };
    let mut left = Vec::new();
    let mut out = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| f(ui, state));
        left = ui.ctx().input(|i| i.events.clone());
    });
    out.textures_delta.clear();
    left
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
}

fn has_key(events: &[egui::Event], wanted: egui::Key, mods: egui::Modifiers) -> bool {
    events.iter().any(|e| matches!(e, egui::Event::Key { key, modifiers, .. } if *key == wanted && *modifiers == mods))
}

fn click(pos: egui::Pos2) -> Vec<egui::Event> {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![button(true), button(false)]
}

#[test]
fn translation_table_is_clean() {
    let problems = hp_app::i18n::lint_table(TR);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn no_collision_with_core_keys() {
    let core: HashSet<&str> = hp_app::strings::TR.iter().map(|(k, _, _)| *k).collect();
    for (k, _, _) in TR {
        assert!(!core.contains(k), "key {k} collides with hp-app");
        assert!(k.starts_with("tex."), "key {k} must start with tex.");
    }
}

#[test]
fn every_used_key_is_translated() {
    let keys: HashSet<&str> = TR.iter().map(|(k, _, _)| *k).collect();
    let core: HashSet<&str> = hp_app::strings::TR.iter().map(|(k, _, _)| *k).collect();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&src).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") || path.ends_with("tr.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for macro_name in ["t!(\"", "tf!(\""] {
            for (i, _) in text.match_indices(macro_name) {
                let before = text[..i].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = &text[i + macro_name.len()..];
                let Some(end) = rest.find('"') else { continue };
                let key = &rest[..end];
                if !keys.contains(key) && !core.contains(key) {
                    missing.push(format!("{}: {key}", path.display()));
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn no_forbidden_dashes_in_sources() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(&src).into_iter().flatten().flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'), "{}", entry.path().display());
    }
}

#[test]
fn french_texts_differ_where_expected() {
    let fr = |k: &str| TR.iter().find(|(key, _, _)| *key == k).map(|(_, _, f)| *f);
    assert_eq!(fr("tex.me.title"), Some("Éditeur de matériaux"));
    assert_eq!(fr("tex.folders.add"), Some("Ajouter un dossier..."));
}

#[test]
fn browser_smoke_with_mock_assets() {
    let mut state = setup();
    let mut b = TextureBrowser::new();
    state.ui.focus_texture_search = true;
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert!(!state.ui.focus_texture_search, "the focus request is consumed");
    assert_eq!(b.shown_count(), 5);

    b.set_search("brick");
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_names(), ["custom/mytextures/brick01"]);

    b.set_search("");
    b.set_category(Category::Tools);
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_count(), 2);

    b.set_category(Category::Used);
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_count(), 3);

    state.settings.browser.hide_tool_textures = true;
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_count(), 2);

    state.settings.browser.favorites.push("concrete/concretefloor001a".into());
    b.set_category(Category::Favorites);
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_names(), ["concrete/concretefloor001a"]);

    state.use_material("custom/mytextures/brick01");
    state.use_material("missing/texture");
    b.set_category(Category::Recent);
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_names()[0], "missing/texture");

    for size in [48.0, 128.0, 256.0] {
        state.settings.ui.thumbnail_size = size;
        b.set_category(Category::All);
        run_ui(&mut state, |ui, s| b.ui(ui, s));
    }
    b.set_search("zzzz_nothing");
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_count(), 0);
}

#[test]
fn browser_with_custom_folder_source() {
    let (mut state, folder) = fs_state("browser_fs");
    let mut b = TextureBrowser::new();
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_names(), ["custom/my_bricks/dot", "custom/my_bricks/red/wall"]);
    let id = state.assets.fs().and_then(|fs| fs.sources().first().map(|s| s.id)).unwrap_or_default();
    b.set_source_filter(SourceFilter::Source(id));
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_count(), 2);
    b.set_search("src:bricks red");
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_names(), ["custom/my_bricks/red/wall"]);
    b.set_search("kw:urban");
    b.set_source_filter(SourceFilter::All);
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    for _ in 0..200 {
        if !b.indexing_keywords() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        run_ui(&mut state, |ui, s| b.ui(ui, s));
    }
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_names(), ["custom/my_bricks/red/wall"]);
    b.set_search("kw:nothing_like_it");
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_count(), 0);
    let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
}

#[test]
fn picker_kw_and_is_searches() {
    use crate::replace::ReplaceField;
    let glass: &[u8] = b"\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"x/glass\"\n\t\"$translucent\" \"1\"\n\t\"%keywords\" \"windows,clear\"\n}\n";
    let wall: &[u8] = b"\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"x/wall\"\n\t\"%keywords\" \"urban,brick\"\n}\n";
    let (mut state, folder) =
        custom_state("picker_kw_is", "Picker Pack", &[("glass.vmt", glass), ("wall.vmt", wall)], false);
    let ctx = egui::Context::default();
    let mut d = TextureDialogs::new();
    let frame = |state: &mut AppState, d: &mut TextureDialogs| {
        frame_with_events(&ctx, state, None, Vec::new(), |ui, s| d.show(ui.ctx(), s));
    };
    state.ui.show_replace_textures = true;
    frame(&mut state, &mut d);
    frame(&mut state, &mut d);
    d.replace_dialog().open_picker(&ctx, &state, ReplaceField::Replace);
    d.replace_dialog().picker(ReplaceField::Replace).set_category(Category::All);
    let search = |state: &mut AppState, d: &mut TextureDialogs, text: &str| -> Vec<String> {
        d.replace_dialog().picker(ReplaceField::Replace).set_search(text);
        frame(state, d);
        for _ in 0..400 {
            if !d.replace_dialog().picker(ReplaceField::Replace).indexing() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            frame(state, d);
        }
        frame(state, d);
        assert!(d.replace_dialog().picker(ReplaceField::Replace).is_open(&ctx), "the picker stays open");
        d.replace_dialog().picker(ReplaceField::Replace).shown_names()
    };
    let leaf =
        |names: Vec<String>| -> Vec<String> { names.iter().map(|n| crate::util::leaf_name(n).to_owned()).collect() };
    assert_eq!(leaf(search(&mut state, &mut d, "picker")), ["glass", "wall"]);
    assert!(!d.replace_dialog().picker(ReplaceField::Replace).indexing());
    assert_eq!(leaf(search(&mut state, &mut d, "kw:urban")), ["wall"]);
    assert_eq!(leaf(search(&mut state, &mut d, "kw:windows")), ["glass"]);
    assert!(search(&mut state, &mut d, "kw:nothing_like_it").is_empty());
    assert_eq!(leaf(search(&mut state, &mut d, "picker is:translucent")), ["glass"]);
    assert_eq!(leaf(search(&mut state, &mut d, "picker -is:translucent")), ["wall"]);
    assert_eq!(leaf(search(&mut state, &mut d, "is:translucent kw:windows")), ["glass"]);
    assert!(search(&mut state, &mut d, "is:translucent kw:urban").is_empty());
    assert_eq!(leaf(search(&mut state, &mut d, "picker shader:lightmappedgeneric")), ["glass", "wall"]);

    let mut b = TextureBrowser::new();
    b.set_search("kw:urban is:translucent");
    frame_with_events(&ctx, &mut state, None, Vec::new(), |ui, s| b.ui(ui, s));
    assert!(!b.indexing_keywords() && !b.indexing_flags(), "no second index is built");
    frame_with_events(&ctx, &mut state, None, Vec::new(), |ui, s| b.ui(ui, s));
    assert_eq!(b.shown_count(), 0);
    b.set_search("kw:urban");
    frame_with_events(&ctx, &mut state, None, Vec::new(), |ui, s| b.ui(ui, s));
    assert_eq!(leaf(b.shown_names()), ["wall"]);
    let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
}

#[test]
fn tool_panel_smoke() {
    let mut state = setup();
    let mut tool = crate::TextureTool::new();
    run_ui(&mut state, |ui, s| hp_app::Tool::options_ui(&mut tool, ui, s));
    state.doc.select_faces(
        &[hp_editor::FaceRef::new(2, 0), hp_editor::FaceRef::new(3, 0), hp_editor::FaceRef::new(7, 1)],
        hp_editor::SelectOp::Replace,
    );
    run_ui(&mut state, |ui, s| hp_app::Tool::options_ui(&mut tool, ui, s));
    assert!(hp_app::Tool::wants_side_panel(&tool));
    assert!(hp_app::Tool::status_hint(&tool, &state).is_some());
    state.doc.create_displacements(&[hp_editor::FaceRef::new(2, 0)], 2).ok();
    run_ui(&mut state, |ui, s| hp_app::Tool::options_ui(&mut tool, ui, s));
    let boxed = crate::texture_tool();
    assert_eq!(boxed.id(), hp_app::ToolId::Texture);
}

#[test]
fn dialogs_smoke() {
    let mut state = setup();
    let mut d = TextureDialogs::new();
    run_ctx(&mut state, |ctx, s| d.show(ctx, s));

    let mut first = true;
    run_ctx(&mut state, |ctx, s| {
        if first {
            crate::shared::post_replace(
                ctx,
                crate::shared::ReplacePrefill { find: "tools/toolstrigger".into(), replace: "tools/toolsclip".into() },
            );
            first = false;
        }
        d.show(ctx, s);
    });
    assert!(state.ui.show_replace_textures);
    assert_eq!(d.replace_dialog().fields(), ("tools/toolstrigger", "tools/toolsclip"));
    assert_eq!(d.replace_dialog().match_count(&state.doc).faces.len(), 6);
    assert_eq!(d.replace_dialog().run(&mut state), Some(6));

    state.ui.show_material_editor = true;
    let mut first = true;
    run_ctx(&mut state, |ctx, s| {
        if first {
            crate::shared::post_editor(
                ctx,
                vec!["dev/dev_measuregeneric01b".into(), "concrete/concretefloor001a".into()],
            );
            first = false;
        }
        d.show(ctx, s);
    });
    assert_eq!(d.material_editor().len(), 2);

    run_ctx(&mut state, |ctx, s| {
        crate::shared::post_import(ctx, vec![PathBuf::from("x.png")]);
        d.show(ctx, s);
    });
    assert!(d.import_dialog().is_active());
    let dir = scratch("dialogs_import");
    crate::folders::add_custom_dir(&mut state.settings, &dir);
    d.import_dialog().set_destination(0);
    assert!(d.import_dialog().start(&mut state));
    run_ctx(&mut state, |ctx, s| d.show(ctx, s));
    assert!(!d.import_dialog().is_active());
    assert_eq!(state.current_material, "x");

    state.ui.show_replace_textures = false;
    state.ui.show_material_editor = false;
    run_ctx(&mut state, |ctx, s| d.show(ctx, s));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn material_editor_edits_files_and_virtual_materials() {
    let (mut state, folder) = fs_state("matedit_fs");
    let fs = state.assets.fs().cloned().unwrap_or_default();
    let wall = crate::matedit::load_vmt(&fs, "custom/my_bricks/red/wall");
    assert_eq!(wall.location, crate::matedit::VmtLocation::File(folder.join("red").join("wall.vmt")));
    let dot = crate::matedit::load_vmt(&fs, "custom/my_bricks/dot");
    let dot_path = folder.join("dot.vmt");
    assert_eq!(dot.location, crate::matedit::VmtLocation::Virtual(dot_path.clone()));
    let missing = crate::matedit::load_vmt(&fs, "nothing/here");
    assert_eq!(missing.location, crate::matedit::VmtLocation::Missing);

    let items = crate::matedit::plan(&[&wall, &dot, &missing], None);
    assert_eq!(items.len(), 2);
    let ops = vec![("$surfaceprop".to_owned(), crate::matedit::ParamOp::Set("brick".into()))];
    for item in &items {
        let r = crate::matedit::apply_item(item, &ops);
        assert!(matches!(r, Ok(Some(_))), "{r:?}");
    }
    let text = std::fs::read_to_string(folder.join("red").join("wall.vmt")).unwrap_or_default();
    assert!(text.starts_with("// wall") && text.contains("brick"), "{text}");
    assert!(crate::matedit::backup_path(&folder.join("red").join("wall.vmt")).is_file());
    let created = std::fs::read_to_string(&dot_path).unwrap_or_default();
    assert!(hp_vmt::parse(&created).is_ok_and(|m| m.get("$surfaceprop") == Some("brick")), "{created}");

    let mut d = TextureDialogs::new();
    state.ui.show_material_editor = true;
    run_ctx(&mut state, |ctx, s| {
        crate::shared::post_editor(ctx, vec!["custom/my_bricks/red/wall".into()]);
        d.show(ctx, s);
    });
    for _ in 0..50 {
        run_ctx(&mut state, |ctx, s| d.show(ctx, s));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(d.material_editor().names(), ["custom/my_bricks/red/wall"]);
    let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
}

fn gmod_root() -> Option<PathBuf> {
    let p = PathBuf::from(r"D:\SteamLibrary\steamapps\common\GarrysMod");
    if p.join("garrysmod").is_dir() { Some(p) } else { hp_fs::detect_gmod_root() }
}

#[test]
fn real_gmod_materials_filter_fast() {
    let Some(root) = gmod_root() else {
        eprintln!("Garry's Mod not found, skipping");
        return;
    };
    let opts = hp_fs::MountOptions { mount_workshop: false, mount_cfg: false, ..hp_fs::MountOptions::default() };
    let Ok(fs) = hp_fs::GameFs::for_gmod(&root, &opts) else {
        eprintln!("cannot mount Garry's Mod, skipping");
        return;
    };
    let list = fs.materials();
    assert!(list.len() > 5_000, "{} materials", list.len());
    eprintln!("real Garry's Mod content: {} materials from {} sources", list.len(), fs.sources().len());
    let index = crate::search::SearchIndex::build(&list);
    let mut table = crate::search::SourceTable::default();
    for s in fs.sources() {
        table.insert(s.id, Some(crate::search::SourceGroup::of(s.origin)), &s.label);
    }
    let infos = std::collections::HashMap::new();
    let q = crate::search::Query::parse("concrete floor");
    let start = std::time::Instant::now();
    let r = crate::search::filter_materials(
        &list,
        &index,
        &crate::search::FilterParams {
            query: &q,
            source: SourceFilter::All,
            category: Category::All,
            hide_tools: true,
            members: &[],
            infos: &infos,
            keywords: None,
            sources: &table,
            flags: None,
            excluded: &[],
            hidden_shaders: &[],
        },
    );
    let elapsed = start.elapsed();
    assert!(!r.is_empty());
    assert!(elapsed.as_millis() < 200, "{elapsed:?} for {} materials", list.len());
    if let Some(e) = r.get(&list, 0) {
        let loaded = crate::matedit::load_vmt(&fs, &e.name);
        assert!(loaded.text.is_some());
    }
}

#[test]
fn real_sample_maps_used_and_replace_counts() {
    let dir = Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "vmf"))
        .collect();
    files.push(PathBuf::from(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf"));
    let mut tested = 0;
    for f in files {
        let Ok(doc) = hp_editor::Document::open(&f) else { continue };
        tested += 1;
        let used = crate::search::used_materials(&doc);
        for m in used.iter().take(5) {
            let c = crate::replace::count_matches(&doc, m, crate::replace::MatchMode::Exact, false);
            assert!(!c.faces.is_empty(), "{m} in {}", f.display());
        }
        let all = crate::replace::count_matches(&doc, "*", crate::replace::MatchMode::Wildcard, false);
        let total: usize = doc.scene().solids().values().map(|n| n.solid.sides.len()).sum();
        assert_eq!(all.faces.len(), total);
        assert_eq!(all.materials.len(), used.len());
    }
    if tested == 0 {
        eprintln!("sample maps not found, skipping");
    }
}

struct BigAssets {
    inner: MockAssets,
    list: Arc<Vec<hp_fs::AssetEntry>>,
    status: AssetStatus,
}

impl AssetsApi for BigAssets {
    fn status(&self) -> AssetStatus {
        self.status.clone()
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        None
    }
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        None
    }
    fn generation(&self) -> u64 {
        1
    }
    fn remount(&mut self, _settings: &Settings) {}
    fn reload_fgd(&mut self, _settings: &Settings) {}
    fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.list.clone()
    }
    fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.inner.models()
    }
    fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.inner.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        format!("pack{source}.vpk")
    }
    fn material_info(&mut self, name: &str) -> Option<Arc<MaterialInfo>> {
        self.inner.material_info(name)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> ThumbState {
        self.inner.thumbnail(ctx, name)
    }
    fn model_info(&mut self, path: &str) -> ModelState {
        self.inner.model_info(path)
    }
    fn model_preview(
        &mut self,
        gfx: &mut Gfx,
        path: &str,
        camera: &PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        self.inner.model_preview(gfx, path, camera, size_px)
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        self.inner.entity_bounds(entity)
    }
    fn invalidate_material(&mut self, name: &str) {
        self.inner.invalidate_material(name);
    }
    fn import_images(&mut self, files: &[PathBuf], opts: &ImportOptions) -> Vec<Result<String, String>> {
        self.inner.import_images(files, opts)
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut Gfx>) {
        self.inner.begin_frame(ctx, gfx);
    }
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut Gfx, opts: &SceneOptions) {
        self.inner.sync_scene(doc, gfx, opts);
    }
    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId {
        self.inner.render_material_id(name)
    }
    fn status_line(&self) -> Option<String> {
        None
    }
}

#[test]
fn browser_frames_stay_fast_with_60k_materials() {
    let mut list: Vec<hp_fs::AssetEntry> = (0..60_000)
        .map(|i| hp_fs::AssetEntry { name: format!("folder{:03}/material_{i:05}", i % 400), source: (i % 7) as u32 })
        .collect();
    list.sort();
    let assets = BigAssets { inner: MockAssets::default(), list: Arc::new(list), status: AssetStatus::Ready };
    let mut state = AppState::new(Settings::default(), Box::new(assets));
    crate::register_translations();
    let mut b = TextureBrowser::new();
    let ctx = egui::Context::default();
    let frame = |state: &mut AppState, b: &mut TextureBrowser| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 1000.0))),
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| b.ui(ui, state));
        });
        out.textures_delta.clear();
        start.elapsed()
    };
    let first = frame(&mut state, &mut b);
    let mut steady = std::time::Duration::ZERO;
    for _ in 0..10 {
        steady = steady.max(frame(&mut state, &mut b));
    }
    b.set_search("folder12 material_1");
    let search = frame(&mut state, &mut b);
    assert!(b.shown_count() > 0);
    eprintln!("60k materials: first frame {first:?}, steady frame max {steady:?}, search frame {search:?}");
    assert!(steady.as_millis() < 250, "{steady:?}");
    assert!(search.as_millis() < 500, "{search:?}");
}

#[test]
fn grid_takes_its_keys_only_while_hovered() {
    let mut state = setup();
    let mut b = TextureBrowser::new();
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    let ui_frame = |state: &mut AppState, b: &mut TextureBrowser, pointer, events| {
        frame_with_events(&ctx, state, pointer, events, |ui, s| b.ui(ui, s))
    };
    ui_frame(&mut state, &mut b, None, Vec::new());
    ui_frame(&mut state, &mut b, None, Vec::new());
    let grid = b.grid_rect().expect("grid shown");
    let names = b.shown_names();
    state.use_material(&names[0]);
    let inside = grid.left_top() + egui::vec2(10.0, 10.0);
    let outside = egui::pos2(grid.center().x, grid.top() - 30.0);

    let left = ui_frame(
        &mut state,
        &mut b,
        Some(inside),
        vec![
            key(egui::Key::ArrowRight, none),
            key(egui::Key::Enter, egui::Modifiers::ALT),
            key(egui::Key::ArrowLeft, egui::Modifiers::CTRL),
        ],
    );
    assert!(b.grid_has_keyboard());
    assert_eq!(state.current_material, names[1]);
    assert!(!has_key(&left, egui::Key::ArrowRight, none));
    assert!(has_key(&left, egui::Key::Enter, egui::Modifiers::ALT));
    assert!(has_key(&left, egui::Key::ArrowLeft, egui::Modifiers::CTRL));

    ui_frame(&mut state, &mut b, Some(inside), vec![key(egui::Key::ArrowRight, egui::Modifiers::SHIFT)]);
    assert_eq!(b.marked(), &names[1..=2]);
    let left =
        ui_frame(&mut state, &mut b, Some(inside), vec![key(egui::Key::End, none), key(egui::Key::PageUp, none)]);
    assert!(!left.iter().any(|e| matches!(e, egui::Event::Key { .. })), "{left:?}");
    assert!(b.marked().is_empty());

    ui_frame(&mut state, &mut b, Some(outside), Vec::new());
    let left = ui_frame(&mut state, &mut b, Some(outside), vec![key(egui::Key::ArrowDown, none)]);
    assert!(has_key(&left, egui::Key::ArrowDown, none));
    assert!(!b.grid_has_keyboard());

    state.doc.select(&[hp_editor::ObjRef::Solid(3)], hp_editor::SelectOp::Replace);
    let material_of = |state: &AppState| {
        state.doc.face_side(hp_editor::FaceRef::new(3, 0)).map(|s| s.material.clone()).unwrap_or_default()
    };
    let before = material_of(&state);
    ui_frame(&mut state, &mut b, Some(inside), click(inside));
    assert_eq!(state.current_material, names[0]);
    assert_ne!(before, names[0]);
    let left = ui_frame(
        &mut state,
        &mut b,
        Some(outside),
        vec![key(egui::Key::Enter, none), key(egui::Key::ArrowRight, none), key(egui::Key::PageDown, none)],
    );
    assert!(!b.grid_has_keyboard());
    assert!(has_key(&left, egui::Key::Enter, none), "Enter reaches the tool");
    assert!(has_key(&left, egui::Key::ArrowRight, none), "arrows nudge");
    assert!(has_key(&left, egui::Key::PageDown, none));
    assert_eq!(state.current_material, names[0]);
    assert_eq!(material_of(&state), before, "the selection was not retextured");

    let left = ui_frame(&mut state, &mut b, Some(inside), vec![key(egui::Key::Enter, none)]);
    assert!(!has_key(&left, egui::Key::Enter, none));
    assert_eq!(material_of(&state), names[0]);

    state.ui.focus_texture_search = true;
    ui_frame(&mut state, &mut b, Some(inside), Vec::new());
    let left = ui_frame(&mut state, &mut b, Some(inside), vec![key(egui::Key::ArrowDown, none)]);
    assert_eq!(state.current_material, names[0]);
    let _ = left;
}

#[test]
fn grid_cells_and_labels_are_not_in_the_tab_order() {
    let mut state = setup();
    let mut b = TextureBrowser::new();
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    let mut ui_frame = |state: &mut AppState, events| frame_with_events(&ctx, state, None, events, |ui, s| b.ui(ui, s));
    ui_frame(&mut state, Vec::new());
    ui_frame(&mut state, Vec::new());
    let mut focused = Vec::new();
    for _ in 0..60 {
        ui_frame(&mut state, vec![key(egui::Key::Tab, none)]);
        if let Some(id) = ctx.memory(|m| m.focused()) {
            focused.push(id);
        }
    }
    assert!(!focused.is_empty(), "the search field and the buttons take the focus");
    let names = b.shown_names();
    for name in &names {
        let cell = egui::Id::unique(("hp_tex_cell", name.as_str()));
        assert!(!focused.contains(&cell), "{name} took the keyboard focus");
    }
}

#[test]
fn custom_folder_without_materials_shows_zero() {
    let _lang = hp_app::testing::lang_lock();
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    let (mut state, folder) = custom_state("folder_no_materials", "mdlonly", &[("models/fuzz/a.mdl", b"IDST")], false);
    let _ = std::fs::create_dir_all(folder.join("materials"));
    state.assets.remount(&state.settings);
    let mut b = TextureBrowser::new();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        painted_texts(&ctx, &mut state, |ui, s| b.ui(ui, s));
    }
    let title = hp_app::tf!("tex.folders.title", 1);
    let mut header = None;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 900.0))),
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| b.ui(ui, &mut state));
    });
    for clipped in &out.shapes {
        if let egui::Shape::Text(t) = &clipped.shape
            && t.galley.text() == title
        {
            header = Some(t.pos + egui::vec2(4.0, 4.0));
        }
    }
    let pos = header.expect("the custom folders header is painted");
    for events in [click(pos), Vec::new(), Vec::new()] {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| b.ui(ui, &mut state));
        });
        out.textures_delta.clear();
    }
    let texts = painted_texts(&ctx, &mut state, |ui, s| b.ui(ui, s));
    let zero = hp_app::tf!("tex.folders.count", 0);
    let not_loaded = hp_app::t!("tex.folders.not_mounted");
    assert!(texts.iter().any(|t| t == &zero), "{texts:?}");
    assert!(!texts.iter().any(|t| t == not_loaded), "{texts:?}");
    state.settings.game.custom_texture_dirs[0].enabled = false;
    let texts = painted_texts(&ctx, &mut state, |ui, s| b.ui(ui, s));
    assert!(texts.iter().any(|t| t == hp_app::t!("common.disabled")), "{texts:?}");
    let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
}

fn painted_texts(
    ctx: &egui::Context,
    state: &mut AppState,
    mut f: impl FnMut(&mut egui::Ui, &mut AppState),
) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 900.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| f(ui, state));
    });
    out.textures_delta.clear();
    let mut texts = Vec::new();
    for clipped in &out.shapes {
        collect(&clipped.shape, &mut texts);
    }
    texts
}

#[test]
fn grid_key_parsing() {
    let none = egui::Modifiers::NONE;
    let mut events = vec![
        key(egui::Key::ArrowUp, none),
        key(egui::Key::ArrowDown, egui::Modifiers::SHIFT),
        key(egui::Key::Enter, egui::Modifiers::SHIFT),
        key(egui::Key::PageDown, none),
        key(egui::Key::Home, none),
        key(egui::Key::A, none),
        key(egui::Key::ArrowLeft, egui::Modifiers::ALT),
    ];
    let keys = crate::browser::take_grid_keys(&mut events);
    use crate::browser::GridKey;
    assert_eq!(
        keys,
        [
            GridKey::Move { dx: 0, dy: -1, extend: false },
            GridKey::Move { dx: 0, dy: 1, extend: true },
            GridKey::Page { dir: 1, extend: false },
            GridKey::Edge { end: false, extend: false },
        ]
    );
    assert_eq!(events.len(), 3);
}

#[test]
fn escape_closes_the_top_most_dialog_wherever_the_pointer_is() {
    let mut state = setup();
    let mut d = TextureDialogs::new();
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    let escape = || vec![key(egui::Key::Escape, none)];
    let frame = |state: &mut AppState, d: &mut TextureDialogs, pointer, events, other: bool| {
        frame_with_events(&ctx, state, pointer, events, |ui, s| {
            d.show(ui.ctx(), s);
            if other {
                egui::Window::new("other").id(egui::Id::unique("hp_test_other")).show(ui.ctx(), |ui| ui.label("x"));
            }
        })
    };
    let replace_rect = |ctx: &egui::Context| ctx.memory(|m| m.area_rect(egui::Id::unique("hp_textures_replace")));
    state.ui.show_replace_textures = true;
    frame(&mut state, &mut d, None, Vec::new(), false);
    frame(&mut state, &mut d, None, Vec::new(), false);
    let rect = replace_rect(&ctx).expect("window shown");
    let away = egui::pos2(rect.right() + 40.0, rect.bottom() + 40.0);
    assert!(ctx.memory(|m| m.focused()).is_some(), "the find field takes the focus when the dialog opens");

    let left = frame(&mut state, &mut d, Some(away), escape(), false);
    assert!(!has_key(&left, egui::Key::Escape, none));
    assert!(!state.ui.show_replace_textures);
    let left = frame(&mut state, &mut d, Some(away), escape(), false);
    assert!(has_key(&left, egui::Key::Escape, none));

    state.ui.show_replace_textures = true;
    frame(&mut state, &mut d, None, Vec::new(), false);
    state.ui.show_material_editor = true;
    frame(&mut state, &mut d, None, Vec::new(), false);
    frame(&mut state, &mut d, None, Vec::new(), false);
    let left = frame(&mut state, &mut d, Some(away), escape(), false);
    assert!(!has_key(&left, egui::Key::Escape, none));
    assert!(state.ui.show_replace_textures);
    assert!(!state.ui.show_material_editor);

    frame(&mut state, &mut d, None, Vec::new(), true);
    frame(&mut state, &mut d, None, Vec::new(), true);
    let left = frame(&mut state, &mut d, Some(away), escape(), true);
    assert!(state.ui.show_replace_textures);
    assert!(has_key(&left, egui::Key::Escape, none));
    frame(&mut state, &mut d, None, Vec::new(), false);
    frame(&mut state, &mut d, None, Vec::new(), false);
    let left = frame(&mut state, &mut d, Some(away), escape(), false);
    assert!(!has_key(&left, egui::Key::Escape, none));
    assert!(!state.ui.show_replace_textures);
}

#[test]
fn escape_closes_an_open_menu_before_the_dialog() {
    let mut state = setup();
    let mut d = TextureDialogs::new();
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    let menu_rect = std::cell::Cell::new(None);
    let frame = |state: &mut AppState, d: &mut TextureDialogs, events| {
        frame_with_events(&ctx, state, None, events, |ui, s| {
            ui.add_space(700.0);
            let r = ui.menu_button("menu", |ui| ui.label("item"));
            menu_rect.set(Some(r.response.rect));
            d.show(ui.ctx(), s);
        })
    };
    state.ui.show_replace_textures = true;
    frame(&mut state, &mut d, Vec::new());
    frame(&mut state, &mut d, Vec::new());
    let at = menu_rect.get().expect("menu drawn").center();
    let mut open = click(at);
    open.insert(0, egui::Event::PointerMoved(at));
    frame(&mut state, &mut d, open);
    frame(&mut state, &mut d, Vec::new());
    assert!(egui::Popup::is_any_open(&ctx), "the menu opened");
    frame(&mut state, &mut d, vec![key(egui::Key::Escape, none)]);
    frame(&mut state, &mut d, Vec::new());
    assert!(!egui::Popup::is_any_open(&ctx));
    assert!(state.ui.show_replace_textures);
    let left = frame(&mut state, &mut d, vec![key(egui::Key::Escape, none)]);
    assert!(!has_key(&left, egui::Key::Escape, none));
    assert!(!state.ui.show_replace_textures);
}

#[test]
fn ready_made_pack_works_without_setting_a_prefix() {
    let vmt = b"LightmappedGeneric { $basetexture \"mypack/brick01\" }".as_slice();
    let broken = b"LightmappedGeneric { $basetexture \"mypack/nowhere\" }".as_slice();
    let image = png(64, 64);
    let files: Vec<(&str, &[u8])> =
        vec![("brick01.vmt", vmt), ("brick01.png", &image), ("broken.vmt", broken), ("Bad Name.vmt", vmt)];
    let (mut state, folder) = custom_state("pack_ui", "mypack", &files, false);
    let mut b = TextureBrowser::new();
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert!(b.shown_names().contains(&"mypack/brick01".to_owned()), "{:?}", b.shown_names());
    let fs = state.assets.fs().cloned().unwrap_or_default();
    assert_eq!(fs.material_texture_size("mypack/brick01"), Some((64, 64)));
    let id = fs.sources().first().map(|s| s.id).unwrap_or_default();
    assert_eq!(fs.custom_dir_layout(id).map(|l| l.mode), Some(hp_fs::CustomMode::Detected));
    for _ in 0..200 {
        if !b.checking_folders() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        run_ui(&mut state, |ui, s| b.ui(ui, s));
    }
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    let report = b.folder_report(id).cloned().unwrap_or_default();
    assert_eq!(report.missing_textures, [("mypack/broken".to_owned(), "mypack/nowhere".to_owned())]);
    assert_eq!(report.unsafe_names, ["Bad Name.vmt"]);
    let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
}

#[test]
fn accented_image_names_are_consistent_everywhere() {
    let image = png(32, 32);
    let files: Vec<(&str, &[u8])> = vec![("Mur Brique Été.png", &image)];
    let (mut state, folder) = custom_state("accents", "Textures Été", &files, false);
    let mut b = TextureBrowser::new();
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    let name = "custom/textures_ete/mur_brique_ete";
    assert_eq!(b.shown_names(), [name]);
    state.doc.select(&[hp_editor::ObjRef::Solid(2)], hp_editor::SelectOp::Replace);
    assert_eq!(crate::browser::apply_to_selection(&mut state, name), Some(6));
    state.use_material("dev/dev_measuregeneric01b");
    let face = hp_editor::FaceRef::new(2, 0);
    crate::tool::click_face(&mut state, face, crate::tool::ClickKind::Eyedropper);
    assert_eq!(state.current_material, name);
    b.set_category(Category::Used);
    run_ui(&mut state, |ui, s| b.ui(ui, s));
    assert!(b.shown_names().contains(&name.to_owned()));
    let summary = crate::faces::summarize(&state.doc, &[face]);
    assert_eq!(summary.material.value().as_deref(), Some(name));
    let old = "custom/textures_ete/mur brique Été";
    state.doc.apply_material(&[face], "Custom/Textures_Ete/Mur Brique Été").ok();
    crate::tool::click_face(&mut state, face, crate::tool::ClickKind::Eyedropper);
    assert_eq!(state.current_material, old);
    assert!(state.assets.fs().is_some_and(|fs| fs.material(old).is_some()));
    assert!(crate::search::used_materials(&state.doc).contains(&old.to_owned()));
    let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
}

#[test]
fn replace_keeps_world_size_before_infos_are_loaded() {
    let small = png(128, 128);
    let big = png(256, 256);
    let files: Vec<(&str, &[u8])> = vec![("small.png", &small), ("big.png", &big)];
    let (mut state, folder) = custom_state("replace_sizes", "rs", &files, true);
    let faces = state.doc.all_faces_of(&[hp_editor::ObjRef::Solid(2)]);
    state.doc.apply_material(&faces, "custom/rs/small").ok();
    let mut d = crate::replace::ReplaceDialog::default();
    d.set("custom/rs/small", "custom/rs/big", crate::replace::MatchMode::Exact, false, true);
    assert_eq!(d.run(&mut state), Some(6));
    let side = state.doc.face_side(hp_editor::FaceRef::new(2, 0)).cloned().unwrap_or_default();
    assert_eq!(side.material, "custom/rs/big");
    assert!((side.uaxis.scale - 0.125).abs() < 1e-9, "{}", side.uaxis.scale);
    let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
}

#[test]
fn material_editor_shows_the_files_after_apply() {
    let (mut state, folder) = fs_state("matedit_reload");
    let mut d = TextureDialogs::new();
    let ctx = egui::Context::default();
    let mut frame = |state: &mut AppState, d: &mut TextureDialogs| {
        frame_with_events(&ctx, state, None, Vec::new(), |ui, s| d.show(ui.ctx(), s));
    };
    state.ui.show_material_editor = true;
    let names = vec!["custom/my_bricks/dot".to_owned(), "custom/my_bricks/red/wall".to_owned()];
    d.material_editor().set_materials(&egui::Context::default(), &state, names);
    let wait =
        |state: &mut AppState, d: &mut TextureDialogs, frame: &mut dyn FnMut(&mut AppState, &mut TextureDialogs)| {
            for _ in 0..400 {
                frame(state, d);
                if !d.material_editor().is_busy() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("the material editor never finished");
        };
    wait(&mut state, &mut d, &mut frame);
    let dot = d.material_editor().loaded("custom/my_bricks/dot").map(|l| l.location.clone());
    assert!(matches!(dot, Some(crate::matedit::VmtLocation::Virtual(_))), "{dot:?}");

    if let Some(row) = d.material_editor().rows_mut().iter_mut().find(|r| r.key == "$surfaceprop") {
        row.op = crate::matedit::OpChoice::Set;
        row.value = "grass".into();
    }
    d.material_editor().apply(&egui::Context::default(), &state);
    wait(&mut state, &mut d, &mut frame);
    for name in ["custom/my_bricks/dot", "custom/my_bricks/red/wall"] {
        let loaded = d.material_editor().loaded(name).cloned();
        let loaded = loaded.unwrap_or_else(|| panic!("{name} loaded"));
        assert!(matches!(loaded.location, crate::matedit::VmtLocation::File(_)), "{name}: {:?}", loaded.location);
        assert_eq!(loaded.material.as_ref().and_then(|m| m.get("$surfaceprop")), Some("grass"), "{name}");
    }
    let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
}

#[test]
fn browser_handles_every_asset_status() {
    for status in [AssetStatus::NotConfigured, AssetStatus::Mounting, AssetStatus::Failed("broken gameinfo".into())] {
        let assets = BigAssets { inner: MockAssets::default(), list: Arc::new(Vec::new()), status: status.clone() };
        let mut state = AppState::new(Settings::default(), Box::new(assets));
        crate::register_translations();
        let mut b = TextureBrowser::new();
        run_ui(&mut state, |ui, s| b.ui(ui, s));
        assert_eq!(b.shown_count(), 0, "{status:?}");
        let mut tool = crate::TextureTool::new();
        run_ui(&mut state, |ui, s| hp_app::Tool::options_ui(&mut tool, ui, s));
        let mut d = TextureDialogs::new();
        state.ui.show_replace_textures = true;
        state.ui.show_material_editor = true;
        run_ctx(&mut state, |ctx, s| d.show(ctx, s));
        assert!(state.gfx.is_none());
    }
}

#[test]
fn layer_edits_for_mcp() {
    use crate::uv_layers::{Layer, LayerApplied, LayerEdit, LayerTarget, mcp_apply_layer, mcp_layers};
    let tex = png(8, 8);
    let brick = b"\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"custom/uvl/walls/brick_c\"\n\t\"$bumpmap\" \"custom/uvl/walls/brick_n\"\n}\n";
    let lamp = b"\"VertexLitGeneric\"\n{\n\t\"$basetexture\" \"custom/uvl/walls/brick_c\"\n\t\"$detail\" \"custom/uvl/walls/brick_c\"\n}\n";
    let files: Vec<(&str, &[u8])> = vec![
        ("walls/brick_c.png", &tex),
        ("walls/brick_n.png", &tex),
        ("walls/brick.vmt", brick),
        ("props/lamp.vmt", lamp),
    ];
    let (mut state, folder) = custom_state("mcp_layers", "uvl", &files, false);
    let layers = mcp_layers(&state, "custom/uvl/walls/brick").expect("material");
    let names: Vec<&str> = layers.iter().map(|l| l.layer.mcp_name()).collect();
    assert_eq!(names, ["base", "bump"]);
    assert_eq!(layers[1].support, "follows_base");
    assert!(layers[1].bakes);
    assert_eq!(layers[1].transform_key, "$bumptransform");
    assert!(mcp_layers(&state, "custom/uvl/nope").is_none());

    let face = hp_editor::FaceRef::new(2, 0);
    state.doc.apply_material(&[face], "custom/uvl/walls/brick").ok();
    let edit = LayerEdit { rotate: Some(90.0), ..LayerEdit::default() };
    let done = mcp_apply_layer(
        &mut state,
        "custom/uvl/walls/brick",
        Layer::Bump,
        &edit,
        &LayerTarget::Faces(vec![face]),
        "MCP: edit_uv_layer",
    )
    .expect("variant");
    let LayerApplied::Variants(v) = done else { panic!("variants expected") };
    assert!(v.baked && v.faces == 1, "{v:?}");
    let material = state.doc.face_side(face).map(|s| s.material.clone()).unwrap_or_default();
    assert!(material.contains("hp_variants/brick_"), "{material}");
    assert_eq!(state.doc.undo_label(), Some("MCP: edit_uv_layer"));
    let baked = std::fs::read_dir(folder.join("hp_variants"))
        .map(|d| d.filter_map(Result::ok).any(|e| e.path().extension().is_some_and(|x| x == "vtf")))
        .unwrap_or(false);
    assert!(baked, "a moved copy of the bump map is written");

    let edit = LayerEdit { scale: Some([2.0, 2.0]), detail_scale: Some([3.0, 3.0]), ..LayerEdit::default() };
    let done = mcp_apply_layer(&mut state, "custom/uvl/props/lamp", Layer::Detail, &edit, &LayerTarget::Material, "x")
        .expect("material");
    let LayerApplied::Material(m) = done else { panic!("material expected") };
    assert!(!m.baked);
    let text = std::fs::read_to_string(folder.join("props/lamp.vmt")).unwrap();
    assert!(text.contains("$detailtexturetransform") && text.contains("scale 2 2"), "{text}");
    assert!(text.contains("$detailscale"), "{text}");
    let again = mcp_apply_layer(&mut state, "custom/uvl/props/lamp", Layer::Detail, &edit, &LayerTarget::Material, "x");
    assert!(again.is_err(), "nothing changes the second time");

    let any = LayerEdit { rotate: Some(45.0), ..LayerEdit::default() };
    assert!(
        mcp_apply_layer(&mut state, "custom/uvl/walls/brick", Layer::Base, &any, &LayerTarget::Material, "x").is_err()
    );
    assert!(
        mcp_apply_layer(&mut state, "custom/uvl/walls/brick", Layer::EnvMask, &any, &LayerTarget::Material, "x")
            .is_err()
    );
    let zero = LayerEdit { scale: Some([0.0, 1.0]), ..LayerEdit::default() };
    assert!(
        mcp_apply_layer(&mut state, "custom/uvl/props/lamp", Layer::Detail, &zero, &LayerTarget::Material, "x")
            .is_err()
    );
    let ds = LayerEdit { detail_scale: Some([2.0, 2.0]), ..LayerEdit::default() };
    assert!(
        mcp_apply_layer(&mut state, "custom/uvl/walls/brick", Layer::Bump, &ds, &LayerTarget::Material, "x").is_err()
    );
    assert!(Layer::from_mcp_name("envmask") == Some(Layer::EnvMask) && Layer::from_mcp_name("x").is_none());
    drop(state);
    let _ = std::fs::remove_dir_all(folder.parent().unwrap_or(&folder));
}
