use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_app::AppState;
use hp_app::runtime::AssetCollectRequest;
use hp_app::testing::{MockAssets, run_ui, test_state};
use hp_fs::deps::DepKind;
use hp_fs::{GameFs, MountOptions, SourceOrigin};

use super::*;

fn vtf(v: u8) -> Vec<u8> {
    let img = hp_vtf::RgbaImage::new(4, 4, [v, v, v, 255].repeat(16)).unwrap();
    hp_vtf::encode(&img, &hp_vtf::EncodeOptions::default()).unwrap()
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn vpk(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut tree = Vec::new();
    for (path, data) in files {
        let (dir, file) = path.rsplit_once('/').unwrap();
        let (name, ext) = file.rsplit_once('.').unwrap();
        for part in [ext, dir, name] {
            tree.extend_from_slice(part.as_bytes());
            tree.push(0);
        }
        tree.extend_from_slice(&0u32.to_le_bytes());
        tree.extend_from_slice(&(data.len() as u16).to_le_bytes());
        tree.extend_from_slice(&0x7FFFu16.to_le_bytes());
        tree.extend_from_slice(&[0; 8]);
        tree.extend_from_slice(&0xFFFFu16.to_le_bytes());
        tree.extend_from_slice(data);
        tree.extend_from_slice(&[0, 0]);
    }
    tree.push(0);
    let mut out = Vec::new();
    out.extend_from_slice(&0x55AA_1234u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
    out.extend_from_slice(&tree);
    out
}

fn mdl() -> Vec<u8> {
    let mut b = vec![0u8; 1200];
    let mut put = |at: usize, bytes: &[u8]| b[at..at + bytes.len()].copy_from_slice(bytes);
    put(0, b"IDST");
    put(4, &48i32.to_le_bytes());
    put(12, b"custom/crate.mdl\0");
    put(204, &2i32.to_le_bytes());
    put(208, &500i32.to_le_bytes());
    put(500, &300i32.to_le_bytes());
    put(800, b"crate\0");
    put(564, &250i32.to_le_bytes());
    put(814, b"crate_red\0");
    put(212, &1i32.to_le_bytes());
    put(216, &640i32.to_le_bytes());
    put(640, &840i32.to_le_bytes());
    put(840, b"models\\custom\\\0");
    put(220, &1i32.to_le_bytes());
    put(224, &2i32.to_le_bytes());
    put(228, &700i32.to_le_bytes());
    put(700, &0u16.to_le_bytes());
    put(702, &1u16.to_le_bytes());
    b
}

const VMF: &str = r#"
world
{
	"id" "1"
	"classname" "worldspawn"
	"skyname" "sky1"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(0 0 0) (0 1 0) (1 1 0)" "material" "CUSTOM/BRICK" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "2" "plane" "(0 0 8) (1 1 8) (0 1 8)" "material" "base/wall" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "3" "plane" "(0 0 8) (1 0 8) (1 1 8)" "material" "custom/nope" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side
		{
			"id" "4"
			"plane" "(-64 -64 0) (-64 64 0) (64 64 0)"
			"material" "custom/blend"
			"uaxis" "[1 0 0 0] 0.25"
			"vaxis" "[0 -1 0 0] 0.25"
			"rotation" "0"
			"lightmapscale" "16"
			"smoothing_groups" "0"
			dispinfo
			{
				"power" "2"
				"startposition" "[-64 -64 0]"
				"flags" "0"
				"elevation" "0"
				"subdiv" "0"
			}
		}
	}
}
entity
{
	"id" "10"
	"classname" "prop_static"
	"model" "models/custom/crate.mdl"
	"skin" "1"
	"origin" "0 0 0"
}
entity
{
	"id" "11"
	"classname" "info_overlay"
	"material" "custom/decal"
	"origin" "0 0 0"
}
entity
{
	"id" "12"
	"classname" "infodecal"
	"texture" "pack/rock"
	"origin" "0 0 0"
}
entity
{
	"id" "13"
	"classname" "ambient_generic"
	"message" "Custom.Hum"
	"origin" "0 0 0"
}
"#;

fn game(root: &Path) -> GameFs {
    write(root, "gmod/garrysmod/gameinfo.txt", b"GameInfo { FileSystem { SearchPaths { game garrysmod } } }");
    let mut base = vec![
        ("materials/base/wall.vmt".to_owned(), b"\"LightmappedGeneric\" { \"$basetexture\" \"base/wall\" }".to_vec()),
        ("materials/base/wall.vtf".to_owned(), vtf(1)),
        ("materials/skybox/sky.vtf".to_owned(), vtf(2)),
    ];
    for face in ["up", "dn", "lf", "rt", "ft", "bk"] {
        base.push((
            format!("materials/skybox/sky1{face}.vmt"),
            b"\"UnlitGeneric\" { \"$basetexture\" \"skybox/sky\" }".to_vec(),
        ));
    }
    write(root, "gmod/sourceengine/base_dir.vpk", &vpk(&base));
    let g = "gmod/garrysmod/";
    write(
        root,
        &format!("{g}materials/custom/brick.vmt"),
        b"\"LightmappedGeneric\" { \"$basetexture\" \"custom/brick\" \"$bumpmap\" \"custom/brick_n\" \"%tooltexture\" \"custom/brick\" }",
    );
    write(root, &format!("{g}materials/custom/brick.vtf"), &vtf(3));
    write(root, &format!("{g}materials/custom/brick_n.vtf"), &vtf(4));
    write(
        root,
        &format!("{g}materials/custom/blend.vmt"),
        b"\"WorldVertexTransition\" { \"$basetexture\" \"custom/grass\" \"$basetexture2\" \"custom/dirt\" \"$blendmodulatetexture\" \"custom/blendmod\" }",
    );
    for t in ["grass", "dirt", "blendmod", "decal"] {
        write(root, &format!("{g}materials/custom/{t}.vtf"), &vtf(5));
    }
    write(
        root,
        &format!("{g}materials/custom/decal.vmt"),
        b"\"LightmappedGeneric\" { \"$basetexture\" \"custom/decal\" }",
    );
    write(root, &format!("{g}models/custom/crate.mdl"), &mdl());
    write(root, &format!("{g}models/custom/crate.vvd"), b"IDSV");
    write(root, &format!("{g}models/custom/crate.dx90.vtx"), b"vtx");
    let mut phy = Vec::new();
    phy.extend_from_slice(&16i32.to_le_bytes());
    phy.extend_from_slice(&[0; 12]);
    phy.extend_from_slice(b"break { \"model\" \"custom/crate_gib\" }");
    write(root, &format!("{g}models/custom/crate.phy"), &phy);
    for skin in ["crate", "crate_red"] {
        write(
            root,
            &format!("{g}materials/models/custom/{skin}.vmt"),
            format!("\"VertexLitGeneric\" {{ \"$basetexture\" \"models/custom/{skin}\" }}").as_bytes(),
        );
        write(root, &format!("{g}materials/models/custom/{skin}.vtf"), &vtf(6));
    }
    write(root, &format!("{g}maps/gm_assets_level_sounds.txt"), b"\"Custom.Hum\" { \"wave\" \"custom/hum.wav\" }");
    write(root, &format!("{g}sound/custom/hum.wav"), b"RIFF0000WAVE");
    let mut gma = hp_gma::GmaWriter::new(hp_gma::AddonJson { title: "Pack".into(), ..Default::default() });
    gma.add(
        "materials/pack/rock.vmt",
        hp_gma::GmaSource::Bytes(b"\"LightmappedGeneric\" { \"$basetexture\" \"pack/rock\" }".to_vec()),
    )
    .unwrap();
    gma.add("materials/pack/rock.vtf", hp_gma::GmaSource::Bytes(vtf(7))).unwrap();
    std::fs::create_dir_all(root.join("gmod/garrysmod/addons")).unwrap();
    gma.write(&root.join("gmod/garrysmod/addons/pack.gma")).unwrap();
    let opts = MountOptions { mount_workshop: false, mount_cfg: false, ..MountOptions::default() };
    GameFs::for_gmod(&root.join("gmod"), &opts).unwrap()
}

fn state(root: &Path) -> AppState {
    let mut s = test_state();
    s.assets = Box::new(MockAssets::with_fs(game(root)));
    s.doc = hp_editor::Document::from_map(hp_vmf::parse(VMF).unwrap());
    s.doc.set_path(Some(root.join("maps/gm_assets.vmf")));
    s
}

fn tree_of(state: &AppState) -> AssetTree {
    resolve_tree(&ResolveInput::of(state, new_cache()).unwrap()).unwrap()
}

fn entry<'a>(tree: &'a AssetTree, name: &str) -> &'a Entry {
    tree.entries.iter().find(|e| tree.name(e.item) == name).unwrap_or_else(|| panic!("no entry {name}"))
}

fn names(tree: &AssetTree, items: &[Item]) -> Vec<String> {
    let mut v: Vec<String> = items.iter().map(|&i| tree.name(i).to_owned()).collect();
    v.sort();
    v
}

#[test]
fn lists_every_category_with_its_files() {
    let dir = crate::test_scratch("mapassets_list");
    let s = state(&dir);
    let tree = tree_of(&s);
    let brick = entry(&tree, "materials/custom/brick.vmt");
    assert_eq!(brick.group, Group::Materials);
    assert_eq!(names(&tree, tree.children(brick.item)), ["materials/custom/brick.vtf", "materials/custom/brick_n.vtf"]);
    let blend = entry(&tree, "materials/custom/blend.vmt");
    assert_eq!(
        names(&tree, tree.children(blend.item)),
        ["materials/custom/blendmod.vtf", "materials/custom/dirt.vtf", "materials/custom/grass.vtf"]
    );
    entry(&tree, "materials/custom/decal.vmt");
    let rock = entry(&tree, "materials/pack/rock.vmt");
    assert_eq!(tree.node(rock.item).unwrap().origin, Some(SourceOrigin::Addon));
    let sky: Vec<&Entry> =
        tree.entries.iter().filter(|e| tree.name(e.item).starts_with("materials/skybox/sky1")).collect();
    assert_eq!(sky.len(), 6);
    assert!(sky.iter().all(|e| !e.custom && !e.missing));
    let crate_ = entry(&tree, "models/custom/crate.mdl");
    assert_eq!(crate_.group, Group::Models);
    assert_eq!(
        names(&tree, tree.children(crate_.item)),
        [
            "materials/models/custom/crate.vmt",
            "materials/models/custom/crate_red.vmt",
            "models/custom/crate.dx90.vtx",
            "models/custom/crate.phy",
            "models/custom/crate.vvd",
            "models/custom/crate_gib.mdl",
        ]
    );
    assert!(crate_.missing && crate_.custom);
    assert!(crate_.closure.iter().any(|&i| tree.name(i) == "materials/models/custom/crate_red.vtf"));
    let hum = tree.entries.iter().find(|e| e.group == Group::Sounds).expect("a sound entry");
    let files = names(&tree, &hum.closure);
    assert!(files.contains(&"sound/custom/hum.wav".to_owned()), "{files:?}");
    assert!(files.contains(&"maps/gm_assets_level_sounds.txt".to_owned()), "{files:?}");
    let nope = entry(&tree, "materials/custom/nope.vmt");
    assert!(nope.missing && tree.node(nope.item).is_none());
    let shown = |f: &Filter| -> Vec<String> {
        tree.visible(f).into_iter().map(|i| tree.name(tree.entries[i].item).to_owned()).collect()
    };
    let custom = shown(&Filter::default());
    assert!(custom.contains(&"materials/custom/brick.vmt".to_owned()));
    assert!(!custom.iter().any(|n| n.starts_with("materials/skybox/") || n == "materials/base/wall.vmt"));
    let all = shown(&Filter { show: Show::All, ..Filter::default() });
    assert!(all.contains(&"materials/base/wall.vmt".to_owned()) && all.len() > custom.len());
    let missing = shown(&Filter { show: Show::Missing, ..Filter::default() });
    assert_eq!(missing, ["materials/custom/nope.vmt", "models/custom/crate.mdl"]);
    let mut groups = [false; 5];
    groups[Group::Sounds.index()] = true;
    assert!(shown(&Filter { groups, ..Filter::default() }).iter().all(|n| !n.starts_with("materials/")));
    let found = shown(&Filter { text: "CRATE_RED".into(), ..Filter::default() });
    assert_eq!(found, ["models/custom/crate.mdl"]);
    let t = tree.totals(&tree.visible(&Filter::default()), false);
    assert_eq!(t.missing, 2);
    assert!(t.textures >= 9 && t.model_files == 4, "{t:?}");
    assert!(tree.graph.nodes.iter().any(|n| n.kind == DepKind::Sound));
    let _ = std::fs::remove_dir_all(&dir);
}

fn wait(panel: &mut MapAssetsPanel, s: &mut AppState, until: impl Fn(&MapAssetsPanel, &AppState) -> bool) {
    let ctx = egui::Context::default();
    let start = Instant::now();
    while !until(panel, s) {
        assert!(start.elapsed() < Duration::from_secs(60), "timed out");
        panel.tick(&ctx, s, true);
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read(p: PathBuf) -> Vec<u8> {
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn collects_a_clean_tree_with_a_report() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let dir = crate::test_scratch("mapassets_collect");
    let mut s = state(&dir);
    let mut panel = MapAssetsPanel::new();
    wait(&mut panel, &mut s, |p, _| p.tree.is_some());
    let out = dir.join("out");
    panel.rows_dirty = true;
    panel.rebuild_rows();
    let selected = panel.selected();
    let p = panel.plan(&s, &selected, &out, false, false).unwrap();
    assert_eq!(p.existing, 0);
    panel.start(p.input);
    wait(&mut panel, &mut s, |p, _| p.collect.is_none());
    let summary = s.runtime.content.map_assets.last_collect.clone().unwrap();
    assert!(summary.error.is_none() && summary.failed.is_empty(), "{summary:?}");
    for f in [
        "materials/custom/brick.vmt",
        "materials/custom/brick_n.vtf",
        "materials/custom/dirt.vtf",
        "materials/pack/rock.vtf",
        "materials/models/custom/crate_red.vtf",
        "models/custom/crate.mdl",
        "models/custom/crate.phy",
        "sound/custom/hum.wav",
        "maps/gm_assets_level_sounds.txt",
    ] {
        assert!(out.join(f).is_file(), "{f}");
    }
    assert!(!out.join("materials/base/wall.vmt").exists() && !out.join("materials/skybox").exists());
    assert_eq!(read(out.join("materials/pack/rock.vtf")), vtf(7));
    let report = std::fs::read_to_string(summary.report.clone().unwrap()).unwrap();
    assert!(report.contains("materials/custom/brick.vmt") && report.contains("copied"), "{report}");
    assert!(report.contains("materials/custom/nope.vmt") && report.contains("models/custom/crate_gib.mdl"));
    assert_eq!(summary.missing.len(), 2);
    let all = tree_of(&s);
    let every: Vec<usize> = (0..all.entries.len()).collect();
    let p = panel.plan(&s, &every, &out, true, false).unwrap();
    assert!(p.existing > 0);
    panel.start(p.input);
    wait(&mut panel, &mut s, |p, _| p.collect.is_none());
    assert_eq!(read(out.join("materials/base/wall.vtf")), vtf(1));
    assert!(out.join("materials/skybox/sky1up.vmt").is_file());
    std::fs::write(out.join("materials/custom/brick.vmt"), b"mine").unwrap();
    let p = panel.plan(&s, &every, &out, true, false).unwrap();
    panel.start(p.input);
    wait(&mut panel, &mut s, |p, _| p.collect.is_none());
    let again = s.runtime.content.map_assets.last_collect.clone().unwrap();
    assert_eq!((again.copied, again.skipped), (0, 1), "{again:?}");
    assert_eq!(read(out.join("materials/custom/brick.vmt")), b"mine");
    assert_ne!(again.report, summary.report);
    panel.output = out.display().to_string();
    panel.base_game = true;
    panel.replace = true;
    panel.collect_clicked(&mut s);
    assert!(panel.pending.is_some() && panel.collect.is_none());
    let pending = panel.pending.take().unwrap();
    panel.start(pending.input);
    wait(&mut panel, &mut s, |p, _| p.collect.is_none());
    assert_eq!(s.runtime.content.map_assets.last_collect.as_ref().unwrap().replaced, 1);
    assert_ne!(read(out.join("materials/custom/brick.vmt")), b"mine");
    assert!(panel.plan(&s, &every, Path::new("relative/out"), false, false).is_err());
    assert!(panel.plan(&s, &every, &dir.join("gmod/garrysmod"), false, false).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mcp_requests_and_unused_files() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let dir = crate::test_scratch("mapassets_mcp");
    let mut s = state(&dir);
    let mut panel = MapAssetsPanel::new();
    let out = dir.join("mcp_out");
    s.runtime.content.map_assets.collect_request = Some(AssetCollectRequest {
        output: out.clone(),
        groups: vec!["sounds".into()],
        ..AssetCollectRequest::default()
    });
    let ctx = egui::Context::default();
    let start = Instant::now();
    while s.runtime.content.map_assets.last_collect.is_none() {
        assert!(start.elapsed() < Duration::from_secs(60));
        panel.tick(&ctx, &mut s, false);
        std::thread::sleep(Duration::from_millis(10));
    }
    let summary = s.runtime.content.map_assets.last_collect.clone().unwrap();
    assert!(summary.error.is_none(), "{summary:?}");
    assert!(out.join("sound/custom/hum.wav").is_file());
    assert!(!out.join("materials").exists());
    write(&dir, "addon/materials/custom/brick.vmt", b"\"LightmappedGeneric\" { \"$basetexture\" \"custom/brick\" }");
    write(&dir, "addon/materials/custom/brick.vtf", b"x");
    write(&dir, "addon/materials/old/stuff.vtf", b"xx");
    write(&dir, "addon/sound/old/beep.wav", b"RIFF");
    let tree = panel.tree().unwrap();
    let report = unused_report(tree, &dir.join("addon")).unwrap();
    let unused: Vec<&str> = report.unused.iter().map(|u| u.path.as_str()).collect();
    assert_eq!(unused, ["materials/old/stuff.vtf", "sound/old/beep.wav"]);
    assert!(unused_report(tree, &dir.join("nowhere")).is_err());
    panel.unused_folder = dir.join("addon").display().to_string();
    panel.check_unused();
    wait(&mut panel, &mut s, |p, _| p.unused_job.is_none());
    assert_eq!(panel.unused.as_ref().unwrap().as_ref().unwrap().unused.len(), 2);
    let mut bare = test_state();
    let mut p2 = MapAssetsPanel::new();
    bare.runtime.content.map_assets.collect_request =
        Some(AssetCollectRequest { output: out.clone(), ..AssetCollectRequest::default() });
    p2.tick(&ctx, &mut bare, false);
    assert!(bare.runtime.content.map_assets.last_collect.as_ref().unwrap().error.is_some());
    assert!(output_problem(&bare, &out).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn draws_in_both_languages() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let dir = crate::test_scratch("mapassets_ui");
    let mut s = state(&dir);
    let mut panel = MapAssetsPanel::new();
    run_ui(&mut s, |ui, s| panel.ui(ui, s));
    wait(&mut panel, &mut s, |p, _| p.tree.is_some());
    let first = panel.tree().unwrap().entries[0].item;
    let name = panel.tree().unwrap().name(first).to_owned();
    panel.expanded.insert(name);
    panel.expanded.insert("models/custom/crate.mdl".into());
    panel.rows_dirty = true;
    panel.unused = Some(Err("x".into()));
    for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
        hp_app::i18n::set_lang(lang);
        run_ui(&mut s, |ui, s| panel.ui(ui, s));
        panel.filter.show = Show::Missing;
        panel.rows_dirty = true;
        run_ui(&mut s, |ui, s| panel.ui(ui, s));
        panel.filter.show = Show::Custom;
        panel.collapsed[0] = true;
        panel.rows_dirty = true;
        run_ui(&mut s, |ui, s| panel.ui(ui, s));
        panel.collapsed[0] = false;
    }
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    assert!(panel.rows.iter().any(|r| matches!(r, Row::Child(..))));
    let out = dir.join("ui_out");
    std::fs::create_dir_all(out.join("materials/custom")).unwrap();
    std::fs::write(out.join("materials/custom/brick.vmt"), b"mine").unwrap();
    panel.output = out.display().to_string();
    panel.replace = true;
    panel.collect_clicked(&mut s);
    assert!(panel.pending.is_some());
    run_ui(&mut s, |ui, s| panel.ui(ui, s));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fits_a_narrow_dock() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let dir = crate::test_scratch("mapassets_narrow");
    let mut s = state(&dir);
    let mut panel = MapAssetsPanel::new();
    run_ui(&mut s, |ui, s| panel.ui(ui, s));
    wait(&mut panel, &mut s, |p, _| p.tree.is_some());
    for lang in hp_app::i18n::Lang::ALL {
        hp_app::i18n::set_lang(lang);
        let ctx = egui::Context::default();
        let mut width = 0.0f32;
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let r = egui::Panel::right("hp_test_dock")
                    .resizable(true)
                    .default_size(220.0)
                    .min_size(220.0)
                    .show(ui, |ui| panel.ui(ui, &mut s));
                width = r.response.rect.width();
            });
            out.textures_delta.clear();
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(width <= 220.5, "{lang:?}: {width}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
