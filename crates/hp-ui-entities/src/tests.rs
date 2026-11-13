use hp_app::testing::{run_ctx, run_ui, test_state};
use hp_editor::ObjRef;

use crate::test_fgd::test_fgd;
use crate::{EntityDialogs, EntityPanel, EntityTab, ModelBrowser, ModelPick, Target, model_pick, set_model_pick};

const TABS: [EntityTab; 7] = [
    EntityTab::Properties,
    EntityTab::Flags,
    EntityTab::Outputs,
    EntityTab::Inputs,
    EntityTab::Raw,
    EntityTab::Model,
    EntityTab::Visgroups,
];

fn setup() -> hp_app::AppState {
    let state = test_state();
    crate::register_translations();
    state
}

#[test]
fn every_used_key_is_translated() {
    crate::register_translations();
    hp_app::register_translations();
    let sources = [
        include_str!("browse.rs"),
        include_str!("catalog.rs"),
        include_str!("color.rs"),
        include_str!("cx.rs"),
        include_str!("dialogs.rs"),
        include_str!("display_color.rs"),
        include_str!("edit.rs"),
        include_str!("find_replace.rs"),
        include_str!("help.rs"),
        include_str!("inspect.rs"),
        include_str!("io_ui.rs"),
        include_str!("model_tools.rs"),
        include_str!("keyvalues.rs"),
        include_str!("models.rs"),
        include_str!("panel.rs"),
        include_str!("panel_tabs.rs"),
        include_str!("particles.rs"),
        include_str!("pick.rs"),
        include_str!("smartedit.rs"),
        include_str!("sound_browser.rs"),
        include_str!("soundscapes.rs"),
        include_str!("widgets.rs"),
    ];
    let mut missing = Vec::new();
    for src in sources {
        for marker in ["t!(\"", "tf!(\""] {
            for (pos, _) in src.match_indices(marker) {
                let before = src[..pos].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = &src[pos + marker.len()..];
                let Some(end) = rest.find('"') else { continue };
                let key = &rest[..end];
                if !hp_app::i18n::has_key(key) {
                    missing.push(key.to_owned());
                }
            }
        }
    }
    assert!(missing.is_empty(), "missing translations: {missing:?}");
}

#[test]
fn no_dash_characters_in_sources() {
    let sources = [
        include_str!("lib.rs"),
        include_str!("browse.rs"),
        include_str!("catalog.rs"),
        include_str!("color.rs"),
        include_str!("cx.rs"),
        include_str!("dialogs.rs"),
        include_str!("display_color.rs"),
        include_str!("edit.rs"),
        include_str!("find_replace.rs"),
        include_str!("help.rs"),
        include_str!("inspect.rs"),
        include_str!("io_ui.rs"),
        include_str!("model_tools.rs"),
        include_str!("keyvalues.rs"),
        include_str!("models.rs"),
        include_str!("names.rs"),
        include_str!("panel.rs"),
        include_str!("panel_tabs.rs"),
        include_str!("particles.rs"),
        include_str!("pick.rs"),
        include_str!("smartedit.rs"),
        include_str!("sound_browser.rs"),
        include_str!("sounds.rs"),
        include_str!("soundscapes.rs"),
        include_str!("test_fgd.rs"),
        include_str!("tr.rs"),
        include_str!("values.rs"),
        include_str!("view.rs"),
        include_str!("widgets.rs"),
    ];
    for src in sources {
        assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'));
    }
}

#[test]
fn glyphs_of_the_buttons_exist() {
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    let icons = [crate::smartedit::PICK_GLYPH, crate::smartedit::MARK_GLYPH, "☰", "◩", "♪", "▶", "■", "▾", "+"];
    let texts = [crate::smartedit::BROWSE_GLYPH, "?", "☰", "⊟", "🗗", "⊞", "⟳", "×", "°", "▶", "■"];
    for (font, glyphs) in [(egui::FontId::monospace(13.0), &icons[..]), (egui::FontId::proportional(13.0), &texts[..])]
    {
        let missing: Vec<&str> =
            glyphs.iter().copied().filter(|g| !ctx.fonts_mut(|f| f.has_glyphs(&font, g))).collect();
        assert!(missing.is_empty(), "missing glyphs in {font:?}: {missing:?}");
    }
}

#[test]
fn panel_typed_editors_and_eyedroppers_draw() {
    let mut state = setup();
    let fgd = test_fgd();
    let kv: Vec<(String, String)> = [
        ("part", "env_fire_large"),
        ("scn", "scenes/a.vcd"),
        ("scr", "a.nut"),
        ("sides", "1 2"),
        ("dest", "main_light"),
        ("mat", "decals/x"),
        ("org", "1 2 3"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
    let id = state.doc.create_point_entity("point_everything", glam::DVec3::ZERO, &kv).unwrap();
    state.doc.set_selection(&[ObjRef::Entity(id)]);
    state.doc.clear_history();
    let mut panel = EntityPanel::new();
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    let requester = "entities:entity_panel:dest".to_owned();
    state.pick = Some(hp_app::state::PickRequest { kind: hp_app::state::PickKind::Entity, requester, result: None });
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    assert!(state.pick.is_some());
    assert_eq!(state.doc.history_len().0, 0, "drawing never edits");
}

fn drawn_panel_width(
    state: &mut hp_app::AppState,
    panel: &mut EntityPanel,
    fgd: &hp_fgd::ResolvedFgd,
    width: f32,
) -> f32 {
    let mut used = 0.0f32;
    run_ui(state, |ui, state| {
        let r = ui.allocate_ui(egui::vec2(width, 700.0), |ui| {
            ui.set_max_width(width);
            panel.ui_with_fgd(ui, state, Some(fgd));
        });
        used = used.max(r.response.rect.width());
    });
    used
}

#[test]
fn panel_never_grows_wider_than_the_dock() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    let kv: Vec<(String, String)> = [("dest", "main_light"), ("part", "x"), ("sides", "1"), ("vec", "1 2 3")]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    let every = state.doc.create_point_entity("point_everything", glam::DVec3::ZERO, &kv).unwrap();
    let _lang = hp_app::testing::lang_lock();
    for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
        hp_app::i18n::set_lang(lang);
        for id in [every, 5, 6, 4] {
            state.doc.set_selection(&[ObjRef::Entity(id)]);
            for tab in TABS {
                panel.set_tab(tab);
                for width in [260.0, 300.0, 420.0] {
                    let used = drawn_panel_width(&mut state, &mut panel, &fgd, width);
                    if used > width + 0.5 {
                        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
                        panic!("{lang:?}, entity {id}, {tab:?}: {used} > {width}");
                    }
                }
            }
        }
    }
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
}

fn drawn_width(
    state: &mut hp_app::AppState,
    width: f32,
    mut f: impl FnMut(&mut egui::Ui, &mut hp_app::AppState),
) -> f32 {
    let mut used = 0.0f32;
    run_ui(state, |ui, state| {
        let r = ui.allocate_ui(egui::vec2(width, 800.0), |ui| {
            ui.set_max_width(width);
            f(ui, state);
        });
        used = used.max(r.response.rect.width());
    });
    used
}

#[test]
fn browsers_never_grow_wider_than_the_dock() {
    let mut state = setup();
    let _lang = hp_app::testing::lang_lock();
    let mut sounds = crate::SoundBrowser::new();
    let mut particles = crate::ParticleBrowser::new();
    let mut models = ModelBrowser::new();
    let target = Target::Entities(vec![5]);
    crate::pick::start_asset_pick(
        &mut state.runtime.entities.asset_pick,
        hp_app::runtime::AssetKind::Sound,
        &target,
        "message",
        "ambient_generic (message)".into(),
        "",
    );
    for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
        hp_app::i18n::set_lang(lang);
        for width in [260.0, 380.0] {
            for grid in [false, true] {
                state.settings.browser.model_grid = grid;
                let used = drawn_width(&mut state, width, |ui, s| models.ui(ui, s));
                assert!(used <= width + 0.5, "{lang:?} models (grid {grid}) {used} > {width}");
            }
            let used = drawn_width(&mut state, width, |ui, s| sounds.ui(ui, s));
            assert!(used <= width + 0.5, "{lang:?} sounds {used} > {width}");
            let used = drawn_width(&mut state, width, |ui, s| particles.ui(ui, s));
            assert!(used <= width + 0.5, "{lang:?} particles {used} > {width}");
        }
    }
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
}

#[test]
fn every_key_type_fits_a_narrow_dock() {
    let types = [
        "string",
        "integer",
        "float",
        "boolean",
        "color255",
        "color1",
        "studio",
        "sprite",
        "sound",
        "decal",
        "material",
        "scene",
        "target_source",
        "target_destination",
        "target_name_or_class",
        "origin",
        "angle",
        "angle_negative_pitch",
        "vecline",
        "vector",
        "axis",
        "sidelist",
        "npcclass",
        "filterclass",
        "pointentityclass",
        "instance_file",
        "particlesystem",
        "script",
        "scriptlist",
    ];
    let mut too_wide = Vec::new();
    for ty in types {
        let src = format!("@PointClass = one_key : \"One key.\" [ k({ty}) : \"The key\" ]");
        let fgd = hp_fgd::Fgd::parse(&src, None).resolve();
        let mut state = setup();
        let id = state.doc.create_point_entity("one_key", glam::DVec3::ZERO, &[]).unwrap();
        state.doc.set_selection(&[ObjRef::Entity(id)]);
        let mut panel = EntityPanel::new();
        for width in [260.0, 300.0, 420.0] {
            let used = drawn_panel_width(&mut state, &mut panel, &fgd, width);
            if used > width + 0.5 {
                too_wide.push(format!("{ty} at {width}: {used}"));
            }
        }
    }
    assert!(too_wide.is_empty(), "{too_wide:#?}");
}

#[test]
fn panel_world_properties_when_nothing_selected() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    for tab in TABS {
        panel.set_tab(tab);
        run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    }
    assert!(matches!(panel.tab(), EntityTab::Properties | EntityTab::Raw));
    run_ui(&mut state, |ui, state| panel.ui(ui, state));
    assert!(state.doc.history_len().0 == 0, "drawing never edits");
}

#[test]
fn panel_all_tabs_for_each_entity() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    for id in [4u32, 5, 6] {
        state.doc.set_selection(&[ObjRef::Entity(id)]);
        for tab in TABS {
            panel.set_tab(tab);
            run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
            run_ui(&mut state, |ui, state| panel.ui(ui, state));
        }
    }
    assert_eq!(state.doc.history_len().0, 0);
}

#[test]
fn panel_every_property_type() {
    let mut state = setup();
    let fgd = test_fgd();
    let kv: Vec<(String, String)> = [
        ("s", "text"),
        ("i", "12"),
        ("f", "bad"),
        ("b", "1"),
        ("c1", "1 0 0"),
        ("mat", "dev/dev_measuregeneric01b"),
        ("dec", "decals/x"),
        ("dest", "main_light"),
        ("noc", "nobody"),
        ("org", "1 2 3"),
        ("vec", "0 0 1"),
        ("vl", "garbage"),
        ("npc", "npc_citizen"),
        ("filt", "missing_filter"),
        ("inst", "instances/a.vmf"),
        ("odd", "x"),
        ("angles", "0 45 0"),
        ("custom_key", "custom"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
    let id = state.doc.create_point_entity("point_everything", glam::DVec3::ZERO, &kv).unwrap();
    let sprite = state.doc.create_point_entity("env_sprite", glam::DVec3::ONE, &[]).unwrap();
    let sound = state
        .doc
        .create_point_entity(
            "ambient_generic",
            glam::DVec3::ONE,
            &[("message".into(), "ambient/wind/wind1.wav".into())],
        )
        .unwrap();
    let mut panel = EntityPanel::new();
    for target in [vec![id], vec![sprite], vec![sound], vec![id, sprite, sound]] {
        let objs: Vec<ObjRef> = target.iter().map(|i| ObjRef::Entity(*i)).collect();
        state.doc.set_selection(&objs);
        let before = state.doc.history_len();
        for tab in TABS {
            panel.set_tab(tab);
            run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
        }
        assert_eq!(state.doc.history_len(), before, "drawing never edits");
    }
}

#[test]
fn panel_multi_selection_and_world_brushes() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(4), ObjRef::Entity(5), ObjRef::Solid(7)]);
    for tab in TABS {
        panel.set_tab(tab);
        run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    }
    state.doc.set_selection(&[ObjRef::Solid(2), ObjRef::Solid(3)]);
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    run_ui(&mut state, |ui, state| panel.ui(ui, state));
    assert_eq!(state.doc.history_len().0, 0);
}

#[test]
fn panel_follows_undo_and_external_edits() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(5)]);
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    crate::apply(
        &mut state.doc,
        Some(&fgd),
        crate::Op::SetKey {
            target: Target::Entities(vec![5]),
            key: "_light".into(),
            value: "1 2 3 4".into(),
            merge: true,
        },
    )
    .unwrap();
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    assert!(state.doc.undo());
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    state.doc.delete(&[ObjRef::Entity(5)]).unwrap();
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
}

#[test]
fn model_browser_smoke() {
    let mut state = setup();
    let mut browser = ModelBrowser::new();
    for _ in 0..50 {
        run_ui(&mut state, |ui, state| browser.ui(ui, state));
        if browser.row_count() > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(browser.row_count(), 4, "two folders with one model each");
    browser.set_search("drum");
    run_ui(&mut state, |ui, state| browser.ui(ui, state));
    assert_eq!(browser.row_count(), 2);
    browser.select("models/props_c17/oildrum001.mdl");
    run_ui(&mut state, |ui, state| browser.ui(ui, state));
    assert_eq!(browser.selected(), Some("models/props_c17/oildrum001.mdl"));
    let pick = ModelPick { target: Target::Entities(vec![5]), key: "model".into(), label: "light (model)".into() };
    let mut seen_pick = false;
    run_ui(&mut state, |ui, state| {
        if model_pick(ui.ctx()).is_none() {
            set_model_pick(ui.ctx(), Some(pick.clone()));
        } else {
            seen_pick = true;
        }
        browser.ui(ui, state);
    });
    assert!(seen_pick);
    browser.set_search("nothing matches this");
    run_ui(&mut state, |ui, state| browser.ui(ui, state));
    assert_eq!(browser.row_count(), 0);
}

#[test]
fn dialogs_smoke() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut dialogs = EntityDialogs::new();
    state.ui.show_find_entities = true;
    state.ui.show_entity_report = true;
    state.ui.show_map_properties = true;
    dialogs.set_find_query("light");
    run_ctx(&mut state, |ctx, state| dialogs.show_with_fgd(ctx, state, Some(&fgd)));
    assert_eq!(dialogs.find_result_count(), 1);
    assert_eq!(dialogs.report_row_count(), 3);
    assert!(state.ui.show_find_entities && state.ui.show_entity_report && state.ui.show_map_properties);
    dialogs.set_find_query("wait=1");
    run_ctx(&mut state, |ctx, state| dialogs.show(ctx, state));
    assert_eq!(dialogs.find_result_count(), 1);
    state.doc.delete(&[ObjRef::Entity(4)]).unwrap();
    run_ctx(&mut state, |ctx, state| dialogs.show(ctx, state));
    assert_eq!(dialogs.report_row_count(), 2);
    state.ui.show_find_entities = false;
    state.ui.show_entity_report = false;
    state.ui.show_map_properties = false;
    run_ctx(&mut state, |ctx, state| dialogs.show(ctx, state));
}

#[test]
fn escape_closes_the_top_dialog_and_is_consumed() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut dialogs = EntityDialogs::new();
    state.ui.show_entity_report = true;
    let ctx = egui::Context::default();
    let mut left_over = Vec::new();
    for i in 0..4 {
        let events = if i == 2 { vec![key(egui::Key::Escape)] } else { Vec::new() };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))),
            events,
            time: Some(f64::from(i) * 0.1),
            focused: true,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            dialogs.show_with_fgd(&ctx, &mut state, Some(&fgd));
            left_over.push(ctx.input(|inp| inp.key_pressed(egui::Key::Escape)));
        });
        out.textures_delta.clear();
    }
    assert!(!state.ui.show_entity_report, "Escape closes the window");
    assert_eq!(left_over, [false; 4], "the window consumed the key");
}

#[test]
fn french_ui_smoke() {
    let _lang = hp_app::testing::lang_lock();
    let mut state = setup();
    let fgd = test_fgd();
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(6)]);
    for tab in TABS {
        panel.set_tab(tab);
        run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    }
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
}

#[test]
fn real_gmod_fgd_and_maps() {
    let bin = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod/bin");
    let fgd_path = bin.join("garrysmod.fgd");
    if !fgd_path.exists() {
        eprintln!("Garry's Mod not found, skipping");
        return;
    }
    let start = std::time::Instant::now();
    let Ok(fgd) = hp_fgd::Fgd::load_with_include_dirs([&fgd_path], [bin]) else {
        eprintln!("FGD could not be read, skipping");
        return;
    };
    let fgd = fgd.resolve();
    eprintln!("FGD: {} classes in {:?}", fgd.len(), start.elapsed());
    let mut maps: Vec<std::path::PathBuf> = vec!["C:/Users/oracle/Desktop/hammermcptest/test.vmf".into()];
    let examples = "D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod/gamemodes/terrortown/mapexamples";
    if let Ok(dir) = std::fs::read_dir(examples) {
        maps.extend(dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "vmf")));
    }
    let mut state = setup();
    let mut panel = EntityPanel::new();
    let mut dialogs = EntityDialogs::new();
    for path in maps.iter().filter(|p| p.exists()) {
        let Ok(map) = hp_vmf::load(path) else { continue };
        state.doc = hp_editor::Document::from_map(map);
        let names = crate::MapNames::build(&state.doc);
        let ids: Vec<u32> = state.doc.scene().entities().keys().copied().collect();
        let t = std::time::Instant::now();
        let mut invalid = 0;
        for id in &ids {
            let view = crate::EntityView::build(&state.doc, Target::Entities(vec![*id]), Some(&fgd), &names);
            assert!(view.classes_known || view.single_class().is_some());
            invalid += view.conns.iter().filter(|c| c.issues.any()).count();
        }
        let all = crate::EntityView::build(&state.doc, Target::Entities(ids.clone()), Some(&fgd), &names);
        eprintln!(
            "{}: {} entities, views in {:?}, {} invalid outputs, {} common rows",
            path.display(),
            ids.len(),
            t.elapsed(),
            invalid,
            all.props.len()
        );
        for id in ids.iter().take(12) {
            state.doc.set_selection(&[ObjRef::Entity(*id)]);
            for tab in TABS {
                panel.set_tab(tab);
                run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
            }
        }
        let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Entity(*id)).collect();
        state.doc.set_selection(&objs);
        run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
        state.doc.clear_selection();
        state.ui.show_entity_report = true;
        state.ui.show_map_properties = true;
        state.ui.show_find_entities = true;
        dialogs.set_find_query("info_");
        run_ctx(&mut state, |ctx, state| dialogs.show_with_fgd(ctx, state, Some(&fgd)));
        assert_eq!(dialogs.report_row_count(), ids.len());
        assert_eq!(state.doc.history_len().0, 0, "drawing never edits");
    }
}

#[test]
fn glyphs_exist_in_default_fonts() {
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    let sources = [
        include_str!("io_ui.rs"),
        include_str!("model_tools.rs"),
        include_str!("models.rs"),
        include_str!("panel.rs"),
        include_str!("smartedit.rs"),
        include_str!("widgets.rs"),
    ];
    let mono = egui::FontId::monospace(13.0);
    let prop = egui::FontId::proportional(13.0);
    let mut missing = Vec::new();
    ctx.fonts_mut(|f| {
        for src in sources {
            for (pos, _) in src.match_indices("icon(\"") {
                let rest = &src[pos + 6..];
                let glyph = rest.split('"').next().unwrap_or("");
                for c in glyph.chars().filter(|c| !c.is_ascii()) {
                    if !f.has_glyph(&mono, c) {
                        missing.push(format!("icon {c}"));
                    }
                }
            }
        }
        for (key, en, fr) in crate::TR.iter().chain(
            hp_app::strings::TR.iter().filter(|(k, _, _)| k.starts_with("common.") || k.starts_with("entities.")),
        ) {
            for c in en.chars().chain(fr.chars()).filter(|c| !c.is_ascii()) {
                if !f.has_glyph(&prop, c) {
                    missing.push(format!("{key}: {c}"));
                }
            }
        }
        for c in ['×', '…', '·', '«', '»'] {
            if !f.has_glyph(&prop, c) {
                missing.push(format!("text {c}"));
            }
        }
    });
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    assert!(missing.is_empty(), "glyphs missing from the default fonts: {missing:?}");
}

fn drive(
    state: &mut hp_app::AppState,
    frames: Vec<Vec<egui::Event>>,
    mut before: impl FnMut(&egui::Context, usize),
    mut f: impl FnMut(&mut egui::Ui, &mut hp_app::AppState),
) -> egui::Context {
    let ctx = egui::Context::default();
    for (i, events) in frames.into_iter().enumerate() {
        before(&ctx, i);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))),
            events,
            time: Some(i as f64 * 0.1),
            focused: true,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| f(ui, state));
        });
        out.textures_delta.clear();
    }
    ctx
}

fn key(k: egui::Key) -> egui::Event {
    egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
}

#[test]
fn typing_a_name_and_enter_renames_with_references() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(5)]);
    let id = panel.field_id("targetname");
    let frames = vec![vec![], vec![], vec![egui::Event::Text("_2".into())], vec![key(egui::Key::Enter)], vec![]];
    drive(
        &mut state,
        frames,
        |ctx, i| {
            if i == 1 {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)),
    );
    let light = &state.doc.scene().entity(5).unwrap().entity;
    assert_eq!(light.get("targetname"), Some("main_light_2"));
    let trigger = &state.doc.scene().entity(6).unwrap().entity;
    assert_eq!(trigger.connections[0].target, "main_light_2", "outputs follow the rename");
    assert_eq!(state.doc.history_len().0, 1, "one undo step");
    assert!(state.doc.undo());
    assert_eq!(state.doc.scene().entity(5).unwrap().entity.get("targetname"), Some("main_light"));
}

#[test]
fn escape_cancels_typing() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(5)]);
    let id = panel.field_id("targetname");
    let frames =
        vec![vec![], vec![], vec![egui::Event::Text("xyz".into())], vec![key(egui::Key::Escape)], vec![], vec![]];
    drive(
        &mut state,
        frames,
        |ctx, i| {
            if i == 1 {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)),
    );
    assert_eq!(state.doc.scene().entity(5).unwrap().entity.get("targetname"), Some("main_light"));
    assert_eq!(state.doc.history_len().0, 0);
}

#[test]
fn invalid_numbers_are_not_applied() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(6)]);
    let id = panel.field_id("wait");
    let frames = vec![vec![], vec![], vec![egui::Event::Text("abc".into())], vec![key(egui::Key::Enter)], vec![]];
    drive(
        &mut state,
        frames,
        |ctx, i| {
            if i == 1 {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)),
    );
    assert_eq!(state.doc.scene().entity(6).unwrap().entity.get("wait"), Some("1"));
    assert_eq!(state.doc.history_len().0, 0);
}

#[test]
fn editing_an_output_target_in_the_editor() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(6)]);
    panel.set_tab(EntityTab::Outputs);
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    panel.io_mut().select_rows(&[0]);
    let id = panel.io_mut().target_field_id();
    let select_all = egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    };
    let frames = vec![
        vec![],
        vec![],
        vec![select_all.clone()],
        vec![egui::Event::Text("!activator".into())],
        vec![key(egui::Key::Escape)],
        vec![],
    ];
    drive(
        &mut state,
        frames,
        |ctx, i| {
            if i == 1 {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)),
    );
    assert_eq!(state.doc.scene().entity(6).unwrap().entity.connections[0].target, "main_light");
    let frames = vec![
        vec![],
        vec![],
        vec![select_all.clone()],
        vec![egui::Event::Text("lamp_post".into())],
        vec![key(egui::Key::Enter)],
        vec![],
    ];
    drive(
        &mut state,
        frames,
        |ctx, i| {
            if i == 1 {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)),
    );
    let c = &state.doc.scene().entity(6).unwrap().entity.connections[0];
    assert_eq!(c.target, "lamp_post");
    assert_eq!(state.doc.undo_label(), Some("Edit output"));
    assert_eq!(state.doc.history_len().0, 1);
}

fn type_in_field(
    state: &mut hp_app::AppState,
    fgd: &hp_fgd::ResolvedFgd,
    entity: u32,
    key: &str,
    text: &str,
    keys: &[egui::Key],
) {
    state.doc.set_selection(&[ObjRef::Entity(entity)]);
    let mut panel = EntityPanel::new();
    let id = panel.field_id(key);
    let mut frames = vec![vec![], vec![], vec![egui::Event::Text(text.into())], vec![]];
    for k in keys {
        frames.push(vec![self::key(*k)]);
    }
    frames.extend([vec![], vec![]]);
    drive(
        state,
        frames,
        |ctx, i| {
            if i == 1 {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(fgd)),
    );
}

#[test]
fn enter_keeps_the_typed_name_over_a_suggestion() {
    let mut state = setup();
    let fgd = test_fgd();
    let sound = state.doc.create_point_entity("ambient_generic", glam::DVec3::ZERO, &[]).unwrap();
    state.doc.clear_history();
    type_in_field(&mut state, &fgd, sound, "SourceEntityName", "lig", &[egui::Key::Enter]);
    let e = &state.doc.scene().entity(sound).unwrap().entity;
    assert_eq!(e.get("SourceEntityName"), Some("lig"), "{:?}", e.props);
    type_in_field(&mut state, &fgd, sound, "targetname", "main", &[egui::Key::Enter]);
    let e = &state.doc.scene().entity(sound).unwrap().entity;
    assert_eq!(e.get("targetname"), Some("main"), "no duplicate name, no rewired outputs");
    let trigger = &state.doc.scene().entity(6).unwrap().entity;
    assert_eq!(trigger.connections[0].target, "main_light");
}

#[test]
fn arrows_then_enter_or_tab_pick_a_suggestion() {
    let mut state = setup();
    let fgd = test_fgd();
    let sound = state.doc.create_point_entity("ambient_generic", glam::DVec3::ZERO, &[]).unwrap();
    state.doc.clear_history();
    type_in_field(&mut state, &fgd, sound, "SourceEntityName", "lig", &[egui::Key::ArrowDown, egui::Key::Enter]);
    let e = &state.doc.scene().entity(sound).unwrap().entity;
    assert_eq!(e.get("SourceEntityName"), Some("main_light"), "{:?}", e.props);
    assert_eq!(state.doc.history_len().0, 1, "one undo step");
    let other = state.doc.create_point_entity("ambient_generic", glam::DVec3::ONE, &[]).unwrap();
    type_in_field(&mut state, &fgd, other, "SourceEntityName", "main", &[egui::Key::Tab]);
    let e = &state.doc.scene().entity(other).unwrap().entity;
    assert_eq!(e.get("SourceEntityName"), Some("main_light"));
    type_in_field(&mut state, &fgd, other, "targetname", "mai", &[egui::Key::Tab]);
    assert_eq!(state.doc.scene().entity(other).unwrap().entity.get("targetname"), Some("mai"));
    type_in_field(&mut state, &fgd, sound, "targetname", "main", &[egui::Key::ArrowDown, egui::Key::Enter]);
    let name = state.doc.scene().entity(sound).unwrap().entity.get("targetname").map(str::to_owned);
    assert_eq!(name.as_deref(), Some("main_light"), "grouping under an existing name stays possible");
}

#[test]
fn escape_cancels_even_when_another_component_consumed_it() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(5)]);
    let id = panel.field_id("targetname");
    let frames =
        vec![vec![], vec![], vec![egui::Event::Text("xyz".into())], vec![key(egui::Key::Escape)], vec![], vec![]];
    drive(
        &mut state,
        frames,
        |ctx, i| {
            if i == 1 {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        },
        |ui, state| {
            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            panel.ui_with_fgd(ui, state, Some(&fgd));
        },
    );
    assert_eq!(state.doc.scene().entity(5).unwrap().entity.get("targetname"), Some("main_light"));
    assert_eq!(state.doc.history_len().0, 0);
}

fn find_text(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<egui::Pos2> {
    fn walk(shape: &egui::Shape, text: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.galley.rect.translate(t.pos.to_vec2()).center()),
            egui::Shape::Vec(list) => list.iter().find_map(|s| walk(s, text)),
            _ => None,
        }
    }
    shapes.iter().find_map(|c| walk(&c.shape, text))
}

fn drive_with_output(
    state: &mut hp_app::AppState,
    frames: usize,
    mut events: impl FnMut(usize, &[egui::epaint::ClippedShape]) -> Vec<egui::Event>,
    mut f: impl FnMut(&mut egui::Ui, &mut hp_app::AppState),
) {
    let ctx = egui::Context::default();
    let mut shapes: Vec<egui::epaint::ClippedShape> = Vec::new();
    for i in 0..frames {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))),
            events: events(i, &shapes),
            time: Some(i as f64 * 0.1),
            focused: true,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| f(ui, state));
        });
        out.textures_delta.clear();
        shapes = out.shapes;
    }
}

#[test]
fn spot_pitch_is_one_number() {
    let mut state = setup();
    let fgd = test_fgd();
    let spot =
        state.doc.create_point_entity("light_spot", glam::DVec3::ZERO, &[("pitch".into(), "-90".into())]).unwrap();
    state.doc.clear_history();
    state.doc.set_selection(&[ObjRef::Entity(spot)]);
    let mut panel = EntityPanel::new();
    let mut target = None;
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    drive_with_output(
        &mut state,
        9,
        |i, shapes| {
            if i == 2 {
                target = find_text(shapes, "-90");
                assert!(target.is_some() && find_text(shapes, "°").is_some(), "the pitch is shown as one number");
            }
            let Some(p) = target else { return Vec::new() };
            match i {
                2 => vec![egui::Event::PointerMoved(p)],
                3 => vec![button(p, true)],
                4 => vec![button(p, false)],
                6 => vec![
                    egui::Event::Key {
                        key: egui::Key::A,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::COMMAND,
                    },
                    egui::Event::Text("-45".into()),
                ],
                7 => vec![key(egui::Key::Enter)],
                _ => Vec::new(),
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)),
    );
    let e = &state.doc.scene().entity(spot).unwrap().entity;
    assert_eq!(e.get("pitch"), Some("-45"), "{:?}", e.props);
}

fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
}

#[test]
fn model_rows_select_on_click_and_apply_on_double_click() {
    let mut state = setup();
    let mut browser = ModelBrowser::new();
    for _ in 0..50 {
        run_ui(&mut state, |ui, state| browser.ui(ui, state));
        if browser.row_count() > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(browser.row_count() > 0);
    let mut row = None;
    drive_with_output(
        &mut state,
        12,
        |i, shapes| {
            if i == 1 {
                row = find_text(shapes, "oildrum001");
                assert!(row.is_some(), "the model row is listed");
            }
            let Some(p) = row else { return Vec::new() };
            match i {
                1 => vec![egui::Event::PointerMoved(p)],
                2 => vec![press(p, true)],
                3 => vec![press(p, false)],
                6 => vec![press(p, true)],
                7 => vec![press(p, false)],
                8 => vec![press(p, true)],
                9 => vec![press(p, false)],
                _ => Vec::new(),
            }
        },
        |ui, state| {
            browser.ui(ui, state);
        },
    );
    assert_eq!(browser.selected(), Some("models/props_c17/oildrum001.mdl"), "a click selects the row");
    assert_eq!(state.current_model.as_deref(), Some("models/props_c17/oildrum001.mdl"), "a double click uses it");
    assert_eq!(state.requested_tool, Some(hp_app::ToolId::Entity));
}

#[test]
fn model_browse_button_shows_the_models_tab() {
    let mut state = setup();
    let fgd = test_fgd();
    assert!(!state.settings.ui.show_model_browser, "hidden by default");
    let prop = state
        .doc
        .create_point_entity("prop_static", glam::DVec3::ZERO, &[("model".into(), "models/a.mdl".into())])
        .unwrap();
    state.doc.set_selection(&[ObjRef::Entity(prop)]);
    let mut panel = EntityPanel::new();
    let mut button = None;
    drive_with_output(
        &mut state,
        8,
        |i, shapes| {
            if i == 2 {
                button = find_text(shapes, "\u{2026}");
                assert!(button.is_some(), "the browse button is shown");
            }
            let Some(p) = button else { return Vec::new() };
            match i {
                2 => vec![egui::Event::PointerMoved(p)],
                3 => vec![press(p, true)],
                4 => vec![press(p, false)],
                _ => Vec::new(),
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)),
    );
    assert!(state.settings.ui.show_model_browser, "the Models tab is enabled");
    assert!(state.settings_dirty);
    assert_eq!(state.ui.right_tab, hp_app::state::RightTab::Models);
}

fn click_at(p: egui::Pos2) -> Vec<Vec<egui::Event>> {
    let button = |pressed| egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![vec![egui::Event::PointerMoved(p)], vec![button(true)], vec![button(false)]]
}

#[test]
fn class_picker_changes_the_class_from_the_keyboard() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(6)]);
    let mut frames = vec![vec![], vec![]];
    frames.extend(click_at(egui::pos2(300.0, 18.0)));
    frames.push(vec![]);
    frames.push(vec![egui::Event::Text("func_br".into())]);
    frames.push(vec![]);
    frames.push(vec![key(egui::Key::Enter)]);
    frames.push(vec![]);
    drive(&mut state, frames, |_, _| {}, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    let e = &state.doc.scene().entity(6).unwrap().entity;
    assert_eq!(e.classname, "func_brush");
    assert_eq!(e.get("solidity"), Some("0"), "defaults of the new class are added");
    assert_eq!(state.doc.undo_label(), Some("Change class"));
}

#[test]
fn tie_world_brushes_from_the_panel() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Solid(2), ObjRef::Solid(3)]);
    let mut frames = vec![vec![], vec![]];
    frames.extend(click_at(egui::pos2(1130.0, 107.0)));
    frames.push(vec![]);
    drive(&mut state, frames, |_, _| {}, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    let tied = state.doc.selected_entity_ids();
    assert_eq!(tied.len(), 1, "the new brush entity is selected");
    let e = &state.doc.scene().entity(tied[0]).unwrap().entity;
    assert_eq!(e.classname, "func_detail");
    assert_eq!(state.doc.scene().entity_solid_count(tied[0]), 2);
}

#[test]
fn large_map_performance() {
    let mut vmf = String::from("world\n{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n}\n");
    for i in 0..20_000u32 {
        vmf.push_str(&format!(
            "entity\n{{\n\"id\" \"{}\"\n\"classname\" \"light\"\n\"targetname\" \"lamp_{}\"\n\"origin\" \"{} 0 0\"\nconnections\n{{\n\"OnUser1\" \"lamp_{},TurnOff,,0,-1\"\n}}\n}}\n",
            i + 2,
            i,
            i,
            (i + 1) % 20_000
        ));
    }
    let Ok(map) = hp_vmf::parse(&vmf) else { panic!("synthetic map must parse") };
    let mut doc = hp_editor::Document::from_map(map);
    let fgd = test_fgd();
    let t = std::time::Instant::now();
    let names = crate::MapNames::build(&doc);
    let names_time = t.elapsed();
    let t = std::time::Instant::now();
    let one = crate::EntityView::build(&doc, Target::Entities(vec![2]), Some(&fgd), &names);
    let one_time = t.elapsed();
    let ids: Vec<u32> = doc.scene().entities().keys().copied().collect();
    let t = std::time::Instant::now();
    let all = crate::EntityView::build(&doc, Target::Entities(ids), Some(&fgd), &names);
    let all_time = t.elapsed();
    let t = std::time::Instant::now();
    let rows = crate::dialogs::report_rows(
        &doc,
        "",
        crate::KindFilter::All,
        false,
        crate::widgets::SortBy { column: 1, ascending: true },
    );
    let report_time = t.elapsed();
    let mut cached = names.clone();
    doc.set_selection(&[ObjRef::Entity(3)]);
    let t = std::time::Instant::now();
    cached.refresh(&doc);
    let refresh_time = t.elapsed();
    assert_eq!(cached.names().len(), 20_000);
    eprintln!(
        "names {names_time:?} (refresh {refresh_time:?}), one view {one_time:?}, all views {all_time:?}, report {report_time:?}"
    );
    assert_eq!(names.names().len(), 20_000);
    assert_eq!(one.inputs.len(), 1);
    assert_eq!(all.count(), 20_000);
    assert_eq!(rows.len(), 20_000);
    assert!(one_time.as_millis() < 200, "{one_time:?}");
}

fn find_text_by(shapes: &[egui::epaint::ClippedShape], pred: &dyn Fn(&str) -> bool) -> Option<egui::Pos2> {
    fn walk(shape: &egui::Shape, pred: &dyn Fn(&str) -> bool) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(t) if pred(t.galley.text()) => Some(t.galley.rect.translate(t.pos.to_vec2()).center()),
            egui::Shape::Vec(list) => list.iter().find_map(|s| walk(s, pred)),
            _ => None,
        }
    }
    shapes.iter().find_map(|c| walk(&c.shape, pred))
}

fn indexed_browser(state: &mut hp_app::AppState) -> ModelBrowser {
    let mut browser = ModelBrowser::new();
    for _ in 0..50 {
        run_ui(state, |ui, state| browser.ui(ui, state));
        if browser.row_count() > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(browser.row_count() > 0);
    browser
}

#[test]
fn use_for_new_props_button_takes_the_preview_skin() {
    let mut state = setup();
    state.current_model_skin = 5;
    let mut browser = indexed_browser(&mut state);
    browser.select("models/props_c17/oildrum001.mdl");
    browser.set_preview_skin(2);
    let mut button = None;
    drive_with_output(
        &mut state,
        6,
        |i, shapes| {
            if i == 1 {
                button = find_text_by(shapes, &|t| t.starts_with("+ "));
                assert!(button.is_some(), "the button is shown");
            }
            let Some(p) = button else { return Vec::new() };
            match i {
                1 => vec![egui::Event::PointerMoved(p)],
                2 => vec![press(p, true)],
                3 => vec![press(p, false)],
                _ => Vec::new(),
            }
        },
        |ui, state| browser.ui(ui, state),
    );
    assert_eq!(state.current_model.as_deref(), Some("models/props_c17/oildrum001.mdl"));
    assert_eq!(state.current_model_skin, 2, "the preview skin is used for new props");
    assert_eq!(state.requested_tool, Some(hp_app::ToolId::Entity));
}

#[test]
fn model_double_click_keeps_the_preview_skin() {
    let double_click = |state: &mut hp_app::AppState, browser: &mut ModelBrowser, name: &str| {
        let mut row = None;
        drive_with_output(
            state,
            10,
            |i, shapes| {
                if i == 1 {
                    row = find_text(shapes, name);
                    assert!(row.is_some(), "{name} is listed");
                }
                let Some(p) = row else { return Vec::new() };
                match i {
                    1 => vec![egui::Event::PointerMoved(p)],
                    2 | 4 => vec![press(p, true)],
                    3 | 5 => vec![press(p, false)],
                    _ => Vec::new(),
                }
            },
            |ui, state| browser.ui(ui, state),
        );
    };
    let mut state = setup();
    let mut browser = indexed_browser(&mut state);
    browser.select("models/props_c17/oildrum001.mdl");
    browser.set_preview_skin(1);
    double_click(&mut state, &mut browser, "oildrum001");
    assert_eq!(state.current_model.as_deref(), Some("models/props_c17/oildrum001.mdl"));
    assert_eq!(state.current_model_skin, 1);

    double_click(&mut state, &mut browser, "wood_crate001a");
    assert_eq!(state.current_model.as_deref(), Some("models/props_junk/wood_crate001a.mdl"));
    assert_eq!(state.current_model_skin, 0, "another model starts with its default skin");
}

pub(crate) struct FsAssets {
    pub(crate) inner: hp_app::testing::MockAssets,
    pub(crate) fs: hp_fs::GameFs,
}

impl hp_app::AssetsApi for FsAssets {
    fn status(&self) -> hp_app::state::AssetStatus {
        hp_app::state::AssetStatus::Ready
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
    fn remount(&mut self, _settings: &hp_app::settings::Settings) {}
    fn reload_fgd(&mut self, _settings: &hp_app::settings::Settings) {}
    fn materials(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.materials()
    }
    fn models(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.models()
    }
    fn sounds(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        self.inner.source_label(source)
    }
    fn material_info(&mut self, name: &str) -> Option<std::sync::Arc<hp_app::state::MaterialInfo>> {
        self.inner.material_info(name)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> hp_app::state::ThumbState {
        self.inner.thumbnail(ctx, name)
    }
    fn model_info(&mut self, path: &str) -> hp_app::state::ModelState {
        self.inner.model_info(path)
    }
    fn model_preview(
        &mut self,
        gfx: &mut hp_app::Gfx,
        path: &str,
        camera: &hp_app::state::PreviewCamera,
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
    fn import_images(
        &mut self,
        files: &[std::path::PathBuf],
        opts: &hp_app::state::ImportOptions,
    ) -> Vec<Result<String, String>> {
        self.inner.import_images(files, opts)
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut hp_app::Gfx>) {
        self.inner.begin_frame(ctx, gfx);
    }
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut hp_app::Gfx, opts: &hp_app::state::SceneOptions) {
        self.inner.sync_scene(doc, gfx, opts);
    }
    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId {
        self.inner.render_material_id(name)
    }
    fn status_line(&self) -> Option<String> {
        None
    }
}

fn mounted_state(name: &str) -> (hp_app::AppState, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("hp-ui-entities-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(dir.join("models").join("props"));
    let _ = std::fs::write(dir.join("models").join("props").join("crate.mdl"), b"IDST");
    let fs = hp_fs::GameFs::empty();
    assert!(fs.add_game_dir(&dir).is_ok());
    assert!(fs.exists("models/props/crate.mdl"));
    let mut state = setup();
    state.assets = Box::new(FsAssets { inner: hp_app::testing::MockAssets::default(), fs });
    (state, dir)
}

fn panel_shapes(state: &mut hp_app::AppState, fgd: &hp_fgd::ResolvedFgd) -> Vec<egui::epaint::ClippedShape> {
    let mut panel = EntityPanel::new();
    let mut shapes = Vec::new();
    drive_with_output(
        state,
        4,
        |i, previous| {
            if i == 3 {
                shapes = previous.to_vec();
            }
            Vec::new()
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(fgd)),
    );
    shapes
}

#[test]
fn missing_models_show_a_warning() {
    const WARN: &str = "\u{26a0}";
    let fgd = test_fgd();
    let select_new = |state: &mut hp_app::AppState, class: &str, model: &str| {
        let id = state.doc.create_point_entity(class, glam::DVec3::ZERO, &[("model".into(), model.into())]).unwrap();
        state.doc.set_selection(&[ObjRef::Entity(id)]);
    };
    let (mut state, dir) = mounted_state("missing-models");

    select_new(&mut state, "prop_static", r"Models\Props\Crate.mdl");
    assert!(find_text(&panel_shapes(&mut state, &fgd), WARN).is_none(), "the model exists");

    select_new(&mut state, "prop_static", "models/props/crate_missing.mdl");
    assert!(find_text(&panel_shapes(&mut state, &fgd), WARN).is_some(), "the model is missing");

    select_new(&mut state, "prop_unknown_class", "models/props/crate_missing.mdl");
    let shapes = panel_shapes(&mut state, &fgd);
    assert!(find_text(&shapes, WARN).is_some(), "unknown classes are checked too");
    assert!(find_text(&shapes, "\u{2026}").is_some(), "with the browse button of model fields");

    select_new(&mut state, "env_sprite", "sprites/missing.vmt");
    assert!(find_text(&panel_shapes(&mut state, &fgd), WARN).is_none());
    let _ = std::fs::remove_dir_all(&dir);

    let mut state = setup();
    select_new(&mut state, "prop_static", "models/props/crate_missing.mdl");
    assert!(find_text(&panel_shapes(&mut state, &fgd), WARN).is_none());
}

#[test]
fn add_key_hint_uses_the_key_label() {
    use hp_app::i18n::{Lang, tr_in};
    crate::register_translations();
    for (lang, expected) in [(Lang::En, "Add this key (Enter)"), (Lang::Fr, "Ajouter cette clé (Entrée)")] {
        let label = hp_app::actions::key_label_in(egui::Key::Enter, lang);
        assert_eq!(tr_in("entities.add_key", lang).replace("{0}", &label), expected);
    }
}

fn add_named(state: &mut hp_app::AppState, names: &[&str]) {
    for (i, name) in names.iter().enumerate() {
        let origin = glam::DVec3::new(i as f64 * 16.0, 0.0, 0.0);
        assert!(state.doc.create_point_entity("light", origin, &[("targetname".into(), (*name).into())]).is_ok());
    }
    state.doc.clear_history();
}

fn type_then_keys(
    state: &mut hp_app::AppState,
    fgd: &hp_fgd::ResolvedFgd,
    panel: &mut EntityPanel,
    id: egui::Id,
    select_all: bool,
    text: &str,
    keys: &[(egui::Key, egui::Modifiers)],
) -> egui::Context {
    let mut frames = vec![vec![], vec![]];
    if select_all {
        frames.push(vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }]);
    }
    frames.extend([vec![egui::Event::Text(text.into())], vec![], vec![]]);
    for (k, modifiers) in keys {
        frames.push(vec![egui::Event::Key {
            key: *k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: *modifiers,
        }]);
    }
    frames.extend([vec![], vec![]]);
    drive(
        state,
        frames,
        |ctx, i| {
            if i == 1 {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(fgd)),
    )
}

const TAB: (egui::Key, egui::Modifiers) = (egui::Key::Tab, egui::Modifiers::NONE);
const DOWN: (egui::Key, egui::Modifiers) = (egui::Key::ArrowDown, egui::Modifiers::NONE);

#[test]
fn tab_takes_the_suggestion_the_popup_shows() {
    let mut state = setup();
    let fgd = test_fgd();
    add_named(&mut state, &["aim", "spot_aim", "tttcond"]);
    let cases: [(&str, &[(egui::Key, egui::Modifiers)]); 3] =
        [("SourceEntityName", &[TAB]), ("SourceEntityName", &[DOWN, TAB]), ("targetname", &[DOWN, TAB])];
    for (field, keys) in cases {
        let sound = state.doc.create_point_entity("ambient_generic", glam::DVec3::ZERO, &[]).unwrap();
        state.doc.clear_history();
        state.doc.set_selection(&[ObjRef::Entity(sound)]);
        let mut panel = EntityPanel::new();
        let id = panel.field_id(field);
        let ctx = type_then_keys(&mut state, &fgd, &mut panel, id, false, "spot", keys);
        let e = &state.doc.scene().entity(sound).unwrap().entity;
        assert_eq!(e.get(field), Some("spot_aim"), "{field} {keys:?}: {:?}", e.props);
        assert_eq!(state.doc.history_len().0, 1, "{field} {keys:?}: one undo step");
        let focused = ctx.memory(|m| m.focused());
        assert!(focused.is_none() || focused == Some(id), "{field} {keys:?}: Tab completed, it did not move on");
    }
}

#[test]
fn tab_completes_an_output_target() {
    let mut state = setup();
    let fgd = test_fgd();
    add_named(&mut state, &["aim", "spot_aim", "tttcond"]);
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(6)]);
    panel.set_tab(EntityTab::Outputs);
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    panel.io_mut().select_rows(&[0]);
    let id = panel.io_mut().target_field_id();
    type_then_keys(&mut state, &fgd, &mut panel, id, true, "spot", &[TAB]);
    let c = &state.doc.scene().entity(6).unwrap().entity.connections[0];
    assert_eq!(c.target, "spot_aim");
    assert_eq!(state.doc.history_len().0, 1, "one undo step");
}

#[test]
fn shift_tab_leaves_a_field_with_suggestions() {
    let mut state = setup();
    let fgd = test_fgd();
    add_named(&mut state, &["spot_aim"]);
    let sound = state.doc.create_point_entity("ambient_generic", glam::DVec3::ZERO, &[]).unwrap();
    state.doc.clear_history();
    state.doc.set_selection(&[ObjRef::Entity(sound)]);
    let mut panel = EntityPanel::new();
    let id = panel.field_id("SourceEntityName");
    let back = (egui::Key::Tab, egui::Modifiers::SHIFT);
    let ctx = type_then_keys(&mut state, &fgd, &mut panel, id, false, "spo", &[back]);
    assert_eq!(state.doc.scene().entity(sound).unwrap().entity.get("SourceEntityName"), Some("spo"));
    let focused = ctx.memory(|m| m.focused());
    assert!(focused.is_some() && focused != Some(id), "the focus went to the previous widget: {focused:?}");
}

fn outputs_keys(
    state: &mut hp_app::AppState,
    fgd: &hp_fgd::ResolvedFgd,
    panel: &mut EntityPanel,
    hover: bool,
    events: Vec<Vec<egui::Event>>,
) -> bool {
    let mut pointer = None;
    let mut left = false;
    let frames = events.len() + 4;
    drive_with_output(
        state,
        frames,
        |i, shapes| {
            if i == 1 {
                let row = find_text(shapes, "OnTrigger");
                assert!(row.is_some(), "the output row is shown");
                pointer = row.map(|p| if hover { p } else { egui::pos2(p.x, 4.0) });
            }
            match (i, pointer) {
                (1, Some(p)) => vec![egui::Event::PointerMoved(p)],
                (i, Some(_)) if i >= 2 => events.get(i - 2).cloned().unwrap_or_default(),
                _ => Vec::new(),
            }
        },
        |ui, state| {
            panel.ui_with_fgd(ui, state, Some(fgd));
            left |= ui.input(|i| {
                i.events.iter().any(|e| {
                    matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::Copy | egui::Event::Paste(_))
                })
            });
        },
    );
    left
}

#[test]
fn table_keys_act_on_the_selected_outputs() {
    let mut state = setup();
    let fgd = test_fgd();
    let mut panel = EntityPanel::new();
    state.doc.set_selection(&[ObjRef::Entity(6)]);
    panel.set_tab(EntityTab::Outputs);
    run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
    let entities = state.doc.scene().entities().len();
    let conns = |state: &hp_app::AppState| state.doc.scene().entity(6).unwrap().entity.connections.len();
    let ctrl = |k: egui::Key| egui::Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
    };
    let delete = egui::Event::Key {
        key: egui::Key::Delete,
        physical_key: Some(egui::Key::Delete),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };

    panel.io_mut().select_rows(&[0]);
    let keys = vec![vec![ctrl(egui::Key::C)], vec![ctrl(egui::Key::V)]];
    assert!(!outputs_keys(&mut state, &fgd, &mut panel, true, keys), "the table took the keys");
    assert_eq!(conns(&state), 2, "the output was pasted");
    assert_eq!(state.doc.scene().entities().len(), entities, "no entity pasted");

    let events = vec![vec![egui::Event::Copy], vec![egui::Event::Paste("entity text".into())]];
    assert!(!outputs_keys(&mut state, &fgd, &mut panel, true, events));
    assert_eq!(conns(&state), 3);

    panel.io_mut().select_rows(&[0]);
    assert!(!outputs_keys(&mut state, &fgd, &mut panel, true, vec![vec![ctrl(egui::Key::A)]]));
    assert_eq!(panel.io_mut().selected_rows(), [0, 1, 2]);
    panel.io_mut().select_rows(&[1]);
    assert!(!outputs_keys(&mut state, &fgd, &mut panel, true, vec![vec![delete.clone()]]));
    assert_eq!(conns(&state), 2, "the selected output was deleted");
    assert_eq!(state.doc.scene().entities().len(), entities, "the entity is still there");

    panel.io_mut().select_rows(&[0]);
    assert!(outputs_keys(&mut state, &fgd, &mut panel, false, vec![vec![delete]]), "the application gets Delete");
    assert_eq!(conns(&state), 2);
}

fn many_models_state(folders: usize) -> (hp_app::AppState, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("hp-ui-entities-many-models-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for f in 0..folders {
        let folder = dir.join("models").join(format!("f{f:02}"));
        let _ = std::fs::create_dir_all(&folder);
        for m in 0..10 {
            let _ = std::fs::write(folder.join(format!("m{m}.mdl")), b"IDST");
        }
    }
    let _ = std::fs::create_dir_all(dir.join("models").join("zz"));
    let _ = std::fs::write(dir.join("models").join("zz").join("target_drum.mdl"), b"IDST");
    let fs = hp_fs::GameFs::empty();
    assert!(fs.add_game_dir(&dir).is_ok());
    let mut state = setup();
    state.assets = Box::new(FsAssets { inner: hp_app::testing::MockAssets::default(), fs });
    (state, dir)
}

fn count_text(shapes: &[egui::epaint::ClippedShape], text: &str) -> usize {
    fn walk(shape: &egui::Shape, text: &str) -> usize {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => 1,
            egui::Shape::Vec(list) => list.iter().map(|s| walk(s, text)).sum(),
            _ => 0,
        }
    }
    shapes.iter().map(|c| walk(&c.shape, text)).sum()
}

#[test]
fn a_model_pick_scrolls_the_list_to_the_current_model() {
    let (mut state, dir) = many_models_state(30);
    let model = ("model".to_owned(), "models/zz/target_drum.mdl".to_owned());
    let prop = state.doc.create_point_entity("prop_static", glam::DVec3::ZERO, &[model]).unwrap();
    state.settings.browser.model_grid = false;
    let mut browser = indexed_browser(&mut state);
    assert!(browser.row_count() > 300);
    assert_eq!(browser.list_offset(), 0.0);
    let pick = ModelPick { target: Target::Entities(vec![prop]), key: "model".into(), label: "prop (model)".into() };
    let mut pitches = None;
    drive_with_output(
        &mut state,
        3,
        |i, shapes| {
            if i == 2 {
                let y = |t: &str| find_text(shapes, t).map(|p| p.y);
                if let (Some(header), Some(m0), Some(m1)) = (y("f00"), y("m0"), y("m1")) {
                    pitches = Some((m0 - header, m1 - m0));
                }
            }
            Vec::new()
        },
        |ui, state| {
            hp_app::theme::apply(ui.ctx(), hp_app::settings::ThemeChoice::Dark, 1.0);
            browser.ui(ui, state);
        },
    );
    let mut drawn = 0;
    drive_with_output(
        &mut state,
        5,
        |i, shapes| {
            if i == 4 {
                drawn = count_text(shapes, "target_drum");
            }
            Vec::new()
        },
        |ui, state| {
            if model_pick(ui.ctx()).is_none() {
                set_model_pick(ui.ctx(), Some(pick.clone()));
            }
            browser.ui(ui, state);
        },
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(browser.selected(), Some("models/zz/target_drum.mdl"));
    assert!(browser.list_offset() > 0.0, "the list scrolled");
    assert_eq!(drawn, 2, "the row is drawn (and the name in the details)");
    let (a, b) = pitches.expect("the first rows are shown");
    assert!((a - b).abs() < 0.5, "same row pitch: {a} {b}");
}

#[test]
fn instance_parameters_in_the_properties_tab() {
    use hp_app::runtime::instances::InstanceCache;
    use hp_editor::instances::{LoadRequest, Loaded};
    let _lang = hp_app::testing::lang_lock();
    let mut state = setup();
    hp_app::i18n::register(crate::instance_ui::TR);
    let fgd = test_fgd();
    let dir = std::env::temp_dir().join(format!("hp_entities_instance_tab_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let text = "world\n{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n}\nentity\n{\n\t\"id\" \"2\"\n\t\"classname\" \"func_instance_parms\"\n\t\"parm1\" \"$speed integer 100\"\n\t\"parm2\" \"$tint color255 255 128 0\"\n\t\"parm3\" \"$label_of_the_lift_display string first floor\"\n\t\"parm4\" \"$on boolean 1\"\n}\n";
    std::fs::write(dir.join("lift.vmf"), text).expect("write");
    state.doc.set_path(Some(dir.join("main.vmf")));
    let kv = [
        ("file".to_owned(), "lift.vmf".to_owned()),
        ("replace01".to_owned(), "$undeclared_parameter value".to_owned()),
    ];
    let id = state.doc.create_point_entity("func_instance", glam::DVec3::ZERO, &kv).expect("instance");
    state.doc.set_selection(&[ObjRef::Entity(id)]);
    state.doc.clear_history();
    let mut panel = EntityPanel::new();
    let cache = InstanceCache::shared();
    let req = LoadRequest { file: "lift.vmf", containing: None, depth: 0 };
    for _ in 0..50 {
        run_ui(&mut state, |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)));
        if matches!(cache.load(&req, state.doc.path()), Loaded::Ready { .. }) {
            break;
        }
        cache.wait_idle(std::time::Duration::from_millis(100));
    }
    assert!(state.requested_actions.is_empty(), "drawing requests nothing");
    let edit = hp_app::Action::EditInstance.label();
    let (mut heading, mut button) = (None, None);
    drive_with_output(
        &mut state,
        5,
        |i, shapes| {
            if i == 1 {
                heading = find_text(shapes, hp_app::t!("entities.instance.parameters"));
                button = find_text(shapes, edit);
            }
            let Some(p) = button else { return Vec::new() };
            let pointer = |pressed| egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            match i {
                1 => vec![egui::Event::PointerMoved(p)],
                2 => vec![pointer(true)],
                3 => vec![pointer(false)],
                _ => Vec::new(),
            }
        },
        |ui, state| panel.ui_with_fgd(ui, state, Some(&fgd)),
    );
    for lang in hp_app::i18n::Lang::ALL {
        hp_app::i18n::set_lang(lang);
        let widths = panel_widths(&mut state, 8, |ui, s| panel.ui_with_fgd(ui, s, Some(&fgd)));
        let last = *widths.last().expect("frames");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(last <= 221.0, "{lang:?}: the dock grew: {widths:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(heading.is_some(), "the parameters are shown");
    assert!(button.is_some(), "the Edit instance button is shown");
    assert_eq!(state.requested_actions, [hp_app::Action::EditInstance]);
    assert_eq!(state.doc.history_len().0, 0, "drawing never edits");
}

fn panel_widths(
    state: &mut hp_app::AppState,
    frames: usize,
    mut f: impl FnMut(&mut egui::Ui, &mut hp_app::AppState),
) -> Vec<f32> {
    let ctx = egui::Context::default();
    hp_app::theme::apply(&ctx, hp_app::settings::ThemeChoice::Dark, 1.0);
    let mut widths = Vec::new();
    for i in 0..frames {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
            time: Some(i as f64 * 0.1),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let r = egui::Panel::right("hp_test_dock")
                .resizable(true)
                .default_size(220.0)
                .min_size(220.0)
                .show(ui, |ui| f(ui, state));
            widths.push(r.response.rect.width());
        });
        out.textures_delta.clear();
    }
    widths
}

#[test]
fn dock_tabs_never_grow_a_resizable_panel() {
    let mut state = setup();
    let fgd = test_fgd();
    let _lang = hp_app::testing::lang_lock();
    let mut panel = EntityPanel::new();
    let mut sounds = crate::SoundBrowser::new();
    let mut particles = crate::ParticleBrowser::new();
    let mut models = ModelBrowser::new();
    let check = |name: &str, widths: &[f32]| {
        let (first, last) = (widths[0], *widths.last().expect("frames"));
        assert!(last <= first + 0.5 && last <= 221.0, "{name}: the panel grew from {first} to {last}");
    };
    for lang in hp_app::i18n::Lang::ALL {
        hp_app::i18n::set_lang(lang);
        for id in [5, 6] {
            state.doc.set_selection(&[ObjRef::Entity(id)]);
            for tab in TABS {
                panel.set_tab(tab);
                let widths = panel_widths(&mut state, 8, |ui, s| panel.ui_with_fgd(ui, s, Some(&fgd)));
                check(&format!("{lang:?} entity {id} {tab:?}"), &widths);
            }
        }
        let selections: [&[ObjRef]; 3] = [
            &[ObjRef::Solid(2)],
            &[ObjRef::Solid(2), ObjRef::Solid(3)],
            &[ObjRef::Entity(4), ObjRef::Entity(5), ObjRef::Solid(7)],
        ];
        for (k, sel) in selections.into_iter().enumerate() {
            state.doc.set_selection(sel);
            for tab in TABS {
                panel.set_tab(tab);
                let widths = panel_widths(&mut state, 8, |ui, s| panel.ui_with_fgd(ui, s, Some(&fgd)));
                check(&format!("{lang:?} selection {k} {tab:?}"), &widths);
            }
        }
        for grid in [false, true] {
            state.settings.browser.model_grid = grid;
            check(&format!("{lang:?} models"), &panel_widths(&mut state, 8, |ui, s| models.ui(ui, s)));
        }
        check(&format!("{lang:?} sounds"), &panel_widths(&mut state, 8, |ui, s| sounds.ui(ui, s)));
        check(&format!("{lang:?} particles"), &panel_widths(&mut state, 8, |ui, s| particles.ui(ui, s)));
    }
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
}

#[test]
fn soundscape_choices_accept_custom_names_with_undo_and_vmf_roundtrip() {
    let fgd = hp_fgd::Fgd::parse(
        &format!(
            "{}{}",
            crate::test_fgd::TEST_FGD,
            r#"
@PointClass = env_soundscape
[
    soundscape(*choices) : "Soundscape" : "Automatic" = [ "Automatic" : "Automatic" "Nothing" : "Nothing" ]
]
@PointClass base(env_soundscape) = env_soundscape_triggerable []
"#
        ),
        None,
    )
    .resolve();
    for class in ["env_soundscape", "env_soundscape_triggerable"] {
        let mut state = setup();
        let entity = state
            .doc
            .create_point_entity(class, glam::DVec3::ZERO, &[("soundscape".into(), "Automatic".into())])
            .unwrap();
        state.doc.clear_history();
        state.doc.set_selection(&[ObjRef::Entity(entity)]);
        let mut panel = EntityPanel::new();
        let id = panel.field_id("soundscape");
        type_then_keys(
            &mut state,
            &fgd,
            &mut panel,
            id,
            true,
            "sunless_cavesound_entree",
            &[(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(
            state.doc.scene().entity(entity).unwrap().entity.get("soundscape"),
            Some("sunless_cavesound_entree")
        );
        assert_eq!(state.doc.history_len().0, 1);
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().entity(entity).unwrap().entity.get("soundscape"), Some("Automatic"));
        assert!(state.doc.redo());
        let map = hp_vmf::parse(&hp_vmf::write(&state.doc.to_map())).unwrap();
        assert_eq!(
            map.entities.iter().find(|value| value.id == entity).unwrap().get("soundscape"),
            Some("sunless_cavesound_entree")
        );
    }
}
