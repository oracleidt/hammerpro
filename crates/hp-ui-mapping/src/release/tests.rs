use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use hp_app::settings::{ContentSettings, ReleaseMode};
use hp_fs::deps::{DepRoot, DepsCache, RootKind};
use hp_fs::{GameFs, MountOptions};
use hp_gma::{AddonJson, GmaArchive};

use super::pipeline::{self, NavOutcome, ReleaseEvent, ReleaseInput, ReleaseSummary};
use super::*;

fn write(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, bytes).unwrap();
    p
}

fn vtf() -> Vec<u8> {
    let img = hp_vtf::RgbaImage::new(4, 4, [9u8, 9, 9, 255].repeat(16)).unwrap();
    hp_vtf::encode(&img, &hp_vtf::EncodeOptions::default()).unwrap()
}

fn setup(root: &Path) -> (GameFs, PathBuf) {
    write(
        root,
        "gmod/garrysmod/gameinfo.txt",
        b"GameInfo { FileSystem { SearchPaths { game garrysmod game |all_source_engine_paths|sourceengine } } }",
    );
    write(
        root,
        "gmod/sourceengine/materials/base/wall.vmt",
        b"\"LightmappedGeneric\" { \"$basetexture\" \"base/wall\" }",
    );
    write(root, "gmod/sourceengine/materials/base/wall.vtf", &vtf());
    write(
        root,
        "gmod/garrysmod/materials/custom/brick.vmt",
        b"\"LightmappedGeneric\" { \"$basetexture\" \"custom/brick\" }",
    );
    write(root, "gmod/garrysmod/materials/custom/brick.vtf", &vtf());
    write(
        root,
        "gmod/garrysmod/scripts/soundscapes_gm_rel.txt",
        b"\"Rel.Cave\" { \"playlooping\" { \"wave\" \"custom/hum.wav\" } }",
    );
    write(root, "gmod/garrysmod/sound/custom/hum.wav", b"RIFF0000WAVE");
    let opts = MountOptions { mount_workshop: false, mount_cfg: false, ..MountOptions::default() };
    let fs = GameFs::for_gmod(&root.join("gmod"), &opts).unwrap();

    let vmf = write(root, "maps/gm_rel.vmf", hp_app::testing::TEST_VMF.as_bytes());
    let mut pak = hp_bsp::PakWriter::new();
    pak.add("materials/maps/gm_rel/custom/brick_wvt_patch.vmt", b"patch".to_vec()).unwrap();
    let bsp = hp_bsp::synthetic_bsp(
        20,
        &[
            (hp_bsp::lump::ENTITIES, b"{\n\"classname\" \"worldspawn\"\n}\n\0".to_vec()),
            (1, vec![1; 40]),
            (hp_bsp::lump::VISIBILITY, vec![0; 8]),
            (hp_bsp::lump::LIGHTING, vec![0; 16]),
            (hp_bsp::lump::PAKFILE, pak.finish(hp_bsp::PakOptions::default()).unwrap()),
        ],
    );
    write(root, "maps/gm_rel.bsp", &bsp);
    (fs, vmf)
}

fn roots() -> Vec<DepRoot> {
    let r = |kind, name: &str| DepRoot { kind, name: name.to_owned(), why: "test".to_owned() };
    vec![r(RootKind::Material, "custom/brick"), r(RootKind::Material, "base/wall"), r(RootKind::Soundscape, "Rel.Cave")]
}

fn input(fs: &GameFs, vmf: &Path, out: &Path, mode: ReleaseMode) -> ReleaseInput {
    let thumb = image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]));
    ReleaseInput {
        vmf: vmf.to_path_buf(),
        map: "gm_rel".to_owned(),
        out_root: out.to_path_buf(),
        addon_folder: "gm_rel".to_owned(),
        settings: ContentSettings { mode, compile_first: false, lzma_level: 1, ..ContentSettings::default() },
        compile: None,
        fs: fs.clone(),
        roots: roots(),
        instances: Vec::new(),
        key_kinds: Arc::new(Default::default()),
        addon: AddonJson { title: "Release test".into(), tags: vec!["build".into()], ..AddonJson::default() },
        thumb_png: thumbnail::encode_png(&thumbnail::square(&thumb, thumbnail::THUMB_SIZE)).ok(),
        icon_jpg: thumbnail::encode_jpeg(&thumbnail::square(&thumb, thumbnail::ICON_SIZE), 90).ok(),
        cache: Arc::new(DepsCache::new()),
        cache_dir: out.join("cache"),
        game_session: false,
        gmod_root: None,
        nav_fingerprint: None,
        protect_salt: "test-salt".to_owned(),
        workshop_id: Some(123),
    }
}

fn run(input: ReleaseInput) -> (Result<ReleaseSummary, String>, Vec<String>) {
    let (tx, rx) = mpsc::channel();
    pipeline::run(input, tx, Arc::new(AtomicBool::new(false)), Arc::new(|| {}));
    let mut log = Vec::new();
    let mut result = None;
    for e in rx.try_iter() {
        match e {
            ReleaseEvent::Log(_, text) => log.push(text),
            ReleaseEvent::Done(r) => result = Some(*r),
            _ => {}
        }
    }
    (result.expect("Done event"), log)
}

#[test]
fn workshop_addon_release_end_to_end() {
    let _lang = hp_app::testing::lang_lock();
    hp_app::register_translations();
    crate::register_translations();
    let dir = crate::test_scratch("release_addon");
    let (fs, vmf) = setup(&dir);
    let out = dir.join("out");
    let (result, log) = run(input(&fs, &vmf, &out, ReleaseMode::WorkshopAddon));
    let summary = result.unwrap_or_else(|e| panic!("{e}\n{log:#?}"));
    let addon = out.join("gm_rel");
    let bsp = hp_bsp::Bsp::read(addon.join("maps/gm_rel.bsp")).unwrap();
    let pak = bsp.pakfile().unwrap();
    assert!(pak.contains("materials/maps/gm_rel/custom/brick_wvt_patch.vmt"));
    assert!(pak.contains("scripts/soundscapes_gm_rel.txt"));
    assert!(!pak.contains("materials/custom/brick.vmt"));
    assert!(!pak.contains("materials/base/wall.vmt"), "base content is never shipped");
    assert!(addon.join("materials/custom/brick.vtf").is_file());
    assert!(addon.join("sound/custom/hum.wav").is_file());
    assert!(addon.join("maps/thumb/gm_rel.png").is_file());
    assert!(AddonJson::parse(&std::fs::read_to_string(addon.join("addon.json")).unwrap()).unwrap().is_valid());
    let gma = GmaArchive::open(out.join("gm_rel.gma")).unwrap();
    for p in ["maps/gm_rel.bsp", "materials/custom/brick.vmt", "maps/thumb/gm_rel.png", "sound/custom/hum.wav"] {
        assert!(gma.contains(p), "{p}");
    }
    assert!(!gma.contains("addon.json"));
    assert_eq!(gma.name(), "Release test");
    assert!(thumbnail::icon_ok(&std::fs::read(out.join("gm_rel_icon.jpg")).unwrap()));
    assert_eq!(summary.nav, NavOutcome::None);
    assert!(summary.gma_bytes > 0);

    let (again, log) = run(input(&fs, &vmf, &out, ReleaseMode::WorkshopAddon));
    let again = again.unwrap();
    assert!(again.up_to_date >= 3, "{log:#?}");
    assert!(log.iter().any(|l| l == hp_app::t!("release.gma_up_to_date")), "{log:#?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn self_contained_and_server_kit() {
    hp_app::register_translations();
    crate::register_translations();
    let dir = crate::test_scratch("release_bsp");
    let (fs, vmf) = setup(&dir);
    let out = dir.join("out");
    let (result, log) = run(input(&fs, &vmf, &out, ReleaseMode::ServerKit));
    let summary = result.unwrap_or_else(|e| panic!("{e}\n{log:#?}"));
    let bsp = hp_bsp::Bsp::read(out.join("gm_rel/maps/gm_rel.bsp")).unwrap();
    let pak = bsp.pakfile().unwrap();
    assert!(pak.contains("materials/custom/brick.vmt"));
    assert!(pak.contains("sound/custom/hum.wav"));
    assert!(!pak.find("sound/custom/hum.wav").unwrap().is_compressed(), "audio stays stored");
    assert!(!out.join("gm_rel/materials").exists());
    let fastdl = summary.fastdl.unwrap();
    assert!(fastdl.join("maps/gm_rel.bsp").is_file());
    let lua = std::fs::read_to_string(fastdl.join("lua/autorun/server/gm_rel_resources.lua")).unwrap();
    assert!(lua.contains("resource.AddWorkshop(\"123\")"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_files_block_and_nav_is_fixed_up() {
    hp_app::register_translations();
    crate::register_translations();
    let dir = crate::test_scratch("release_missing");
    let (fs, vmf) = setup(&dir);
    let out = dir.join("out");
    let mut i = input(&fs, &vmf, &out, ReleaseMode::WorkshopAddon);
    i.roots.push(DepRoot { kind: RootKind::Model, name: "models/nope.mdl".into(), why: "test".into() });
    let (result, _) = run(i);
    assert!(result.is_err());
    assert!(!out.join("gm_rel.gma").exists(), "nothing is written when files are missing");

    let fp = hp_bsp::Bsp::read(vmf.with_extension("bsp")).unwrap().walkable_fingerprint().unwrap();
    let mut nav = hp_bsp::nav::NAV_MAGIC.to_le_bytes().to_vec();
    nav.extend_from_slice(&16u32.to_le_bytes());
    nav.extend_from_slice(&1u32.to_le_bytes());
    nav.extend_from_slice(&7u32.to_le_bytes());
    nav.extend_from_slice(&[1, 0, 0, 0]);
    write(&dir, "maps/gm_rel.nav", &nav);
    let mut i = input(&fs, &vmf, &out, ReleaseMode::WorkshopAddon);
    i.nav_fingerprint = Some(fp);
    let (result, log) = run(i);
    let summary = result.unwrap_or_else(|e| panic!("{e}\n{log:#?}"));
    assert_eq!(summary.nav, NavOutcome::Shipped { fixed: true });
    let shipped = std::fs::read(out.join("gm_rel/maps/gm_rel.nav")).unwrap();
    let size = std::fs::metadata(out.join("gm_rel/maps/gm_rel.bsp")).unwrap().len();
    assert_eq!(hp_bsp::nav::read_nav_header(&shipped).unwrap().bsp_size, Some(size as u32));
    let mut i = input(&fs, &vmf, &out, ReleaseMode::WorkshopAddon);
    i.nav_fingerprint = Some(fp ^ 1);
    let (result, _) = run(i);
    assert_eq!(result.unwrap().nav, NavOutcome::Stale);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn analyze_resolves_without_writing() {
    hp_app::register_translations();
    crate::register_translations();
    let dir = crate::test_scratch("release_analyze");
    let (fs, vmf) = setup(&dir);
    let out = dir.join("out");
    let (tx, rx) = mpsc::channel();
    pipeline::analyze(
        input(&fs, &vmf, &out, ReleaseMode::WorkshopAddon),
        tx,
        Arc::new(AtomicBool::new(false)),
        Arc::new(|| {}),
    );
    let mut graph = None;
    for e in rx.try_iter() {
        if let ReleaseEvent::Graph(g) = e {
            graph = Some(g);
        }
    }
    let graph = graph.unwrap();
    assert!(graph.find("materials/custom/brick.vmt").is_some());
    assert!(graph.missing.is_empty());
    assert!(!out.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn roots_of_the_open_map() {
    let state = hp_app::testing::test_state();
    let kinds = key_kinds(None);
    let (roots, instances) = collect_roots(&state, &kinds);
    assert!(roots.iter().any(|r| r.kind == RootKind::Material));
    assert!(instances.is_empty());
    assert!(roots.iter().all(|r| !r.why.is_empty()));
}

#[test]
fn window_tabs_draw_headless() {
    let mut state = hp_app::testing::test_state();
    crate::register_translations();
    state.runtime.content.release_open = true;
    let mut win = ReleaseWindow::default();
    for tab in [Tab::Build, Tab::Report, Tab::Checklist, Tab::Addon, Tab::Publish] {
        win.tab = tab;
        hp_app::testing::run_ctx(&mut state, |ctx, state| win.show(ctx, state));
    }
    state.runtime.content.capture = hp_app::runtime::CaptureState::Requested;
    hp_app::testing::run_ctx(&mut state, |ctx, state| win.show(ctx, state));
    assert_eq!(state.runtime.content.capture, hp_app::runtime::CaptureState::Requested);
    let img = hp_app::runtime::CapturedImage { width: 8, height: 6, rgba: vec![200; 8 * 6 * 4] };
    state.runtime.content.capture = hp_app::runtime::CaptureState::Ready(img);
    hp_app::testing::run_ctx(&mut state, |ctx, state| win.show(ctx, state));
    assert!(win.thumb.is_some());
    assert_eq!(state.runtime.content.capture, hp_app::runtime::CaptureState::Idle);
    win.side.title = "Hello".into();
    win.save_sidecar(&mut state);
    assert_eq!(state.sidecar.get::<ReleaseSidecar>().title, "Hello");
    assert!(win.job.is_none() && win.publishing.is_none());
}

#[test]
fn protected_release_ships_a_matching_server_copy() {
    let _lang = hp_app::testing::lang_lock();
    hp_app::register_translations();
    crate::register_translations();
    let dir = crate::test_scratch("release_protect");
    let (fs, vmf) = setup(&dir);
    let ents = "{\n\"classname\" \"worldspawn\"\n}\n\
                {\n\"classname\" \"logic_relay\"\n\"targetname\" \"r\"\n\"OnTrigger\" \"d,Open,,0,-1\"\n}\n\
                {\n\"classname\" \"func_door\"\n\"targetname\" \"d\"\n\"model\" \"*1\"\n\"parentname\" \"t\"\n}\n\0";
    let mut data = Vec::new();
    let mut table = Vec::new();
    for n in ["custom/brick", "base/wall"] {
        table.extend_from_slice(&(data.len() as u32).to_le_bytes());
        data.extend_from_slice(n.as_bytes());
        data.push(0);
    }
    let mut pak = hp_bsp::PakWriter::new();
    pak.add("materials/maps/gm_rel/custom/brick_wvt_patch.vmt", b"patch".to_vec()).unwrap();
    let bsp = hp_bsp::synthetic_bsp(
        20,
        &[
            (hp_bsp::lump::ENTITIES, ents.as_bytes().to_vec()),
            (1, vec![1; 40]),
            (hp_bsp::lump::VISIBILITY, vec![0; 8]),
            (hp_bsp::lump::LIGHTING, vec![0; 16]),
            (hp_bsp::lump::ORIGINAL_FACES, vec![3; 56]),
            (hp_bsp::lump::TEXDATA_STRING_DATA, data),
            (hp_bsp::lump::TEXDATA_STRING_TABLE, table),
            (hp_bsp::lump::PAKFILE, pak.finish(hp_bsp::PakOptions::default()).unwrap()),
        ],
    );
    write(&dir, "maps/gm_rel.bsp", &bsp);
    let fp = hp_bsp::Bsp::read(vmf.with_extension("bsp")).unwrap().walkable_fingerprint().unwrap();
    let mut nav = hp_bsp::nav::NAV_MAGIC.to_le_bytes().to_vec();
    nav.extend_from_slice(&16u32.to_le_bytes());
    nav.extend_from_slice(&1u32.to_le_bytes());
    nav.extend_from_slice(&7u32.to_le_bytes());
    nav.extend_from_slice(&[1, 0, 0, 0]);
    write(&dir, "maps/gm_rel.nav", &nav);

    let out = dir.join("out");
    let mut i = input(&fs, &vmf, &out, ReleaseMode::WorkshopAddon);
    i.nav_fingerprint = Some(fp);
    i.settings.protect = true;
    i.settings.protect_watermark = "my server".into();
    i.settings.protect_strip_original_faces = true;
    i.settings.protect_randomize = true;
    i.settings.protect_lock = true;
    i.settings.protect_lock_key = "secret".into();
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    let (result, log) = run(i);
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    result.unwrap_or_else(|e| panic!("{e}\n{log:#?}"));

    let client = hp_bsp::Bsp::read(out.join("gm_rel/maps/gm_rel.bsp")).unwrap();
    let server_dir = out.join("server/garrysmod/maps");
    let server = hp_bsp::Bsp::read(server_dir.join("gm_rel.bsp")).unwrap();
    for i in (0..hp_bsp::LUMP_COUNT).filter(|&i| i != hp_bsp::lump::ENTITIES) {
        assert_eq!(client.lump_raw(i).unwrap(), server.lump_raw(i).unwrap(), "lump {i} differs");
    }
    let class_of = |b: &hp_bsp::Bsp, c: &str| b.entities().unwrap().into_iter().find(|e| e.classname() == c);
    assert!(class_of(&client, "logic_relay").is_none(), "players get no logic");
    assert_eq!(class_of(&server, "logic_relay").unwrap().get("OnTrigger"), Some("d,Open,,0,-1"));
    assert_eq!(class_of(&server, "func_door").unwrap().get("parentname"), Some("t"), "parenting kept");
    assert!(client.lump_raw(hp_bsp::lump::ORIGINAL_FACES).unwrap().is_empty(), "decompile data stripped");

    assert!(!out.join("gm_rel/maps/gm_rel.nav").exists());
    let gma = GmaArchive::open(out.join("gm_rel.gma")).unwrap();
    assert!(!gma.contains("maps/gm_rel.nav"));
    let server_nav = std::fs::read(server_dir.join("gm_rel.nav")).expect("server nav");
    let server_size = std::fs::metadata(server_dir.join("gm_rel.bsp")).unwrap().len();
    assert_eq!(hp_bsp::nav::read_nav_header(&server_nav).unwrap().bsp_size, Some(server_size as u32));
    assert!(out.join("gm_rel").join(hp_bsp::LOCK_LUA_PATH).is_file());
    let readme = std::fs::read_to_string(out.join("server/README.txt")).expect("notice");
    for needed in ["garrysmod/maps/gm_rel.bsp", "gm_rel.nav", "addon", "hammerpro_key", "server.cfg"] {
        assert!(readme.contains(needed), "the notice mentions {needed}:\n{readme}");
    }
    assert!(readme.contains("Copie le dossier"), "written in the UI language:\n{readme}");
    assert!(!readme.contains('\u{2014}') && !readme.contains('\u{2013}'), "no long dashes");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn build_window_keeps_its_width() {
    let _lang = hp_app::testing::lang_lock();
    hp_app::register_translations();
    crate::register_translations();
    let mut state = hp_app::testing::test_state();
    state.doc.set_path(Some(std::env::temp_dir().join("a_rather_long_map_name_for_the_test.vmf")));
    state.settings.content.protect = true;
    state.settings.content.protect_lock = true;
    state.runtime.content.release_open = true;
    let long = PathBuf::from("C:/Users/someone/Downloads/a_rather_long_map_name_for_the_test_content");
    for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
        hp_app::i18n::set_lang(lang);
        let mut win = ReleaseWindow::default();
        win.side.output_dir = Some(long.clone());
        win.summary = Some(Ok(ReleaseSummary {
            bsp: long.join("maps/a_rather_long_map_name_for_the_test.bsp"),
            addon_dir: long.clone(),
            ..ReleaseSummary::default()
        }));
        win.log = vec![(LogSeverity::Info, format!("{} {}", t!("release.title"), "x".repeat(400)))];
        let ctx = egui::Context::default();
        hp_app::theme::apply(&ctx, hp_app::settings::ThemeChoice::Dark, 1.0);
        let mut widths = Vec::new();
        for _ in 0..40 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1017.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| win.show(ui.ctx(), &mut state));
            out.textures_delta.clear();
            if let Some(r) = ctx.memory(|m| m.area_rect(egui::Id::unique("hp_release_window"))) {
                widths.push(r.width());
            }
        }
        let (first, last) = (widths[4], *widths.last().expect("window shown"));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(last <= first + 0.5, "{lang:?}: the window grew from {first} to {last}");
    }
}
