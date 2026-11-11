use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use hp_app::AssetsApi;
use hp_app::settings::Settings;
use hp_app::state::{Gfx, ImportOptions, MaterialInfo, ModelState, PreviewCamera, SceneOptions, ThumbState};
use hp_app::testing::MockAssets;
use hp_fgd::ResolvedFgd;

use super::*;

fn codes(problems: &[Problem]) -> Vec<&'static str> {
    problems.iter().map(|p| p.code).collect()
}

fn find<'p>(problems: &'p [Problem], code: &str, object: Option<ObjRef>) -> &'p Problem {
    problems
        .iter()
        .find(|p| p.code == code && object.is_none_or(|o| p.objects.contains(&o)))
        .unwrap_or_else(|| panic!("no {code} for {object:?} in {:?}", codes(problems)))
}

fn index_of(state: &AppState, code: &str) -> usize {
    state.runtime.health.problems.iter().position(|p| p.code == code).unwrap_or_else(|| {
        panic!("no {code} in {:?}", codes(&state.runtime.health.problems));
    })
}

#[test]
fn checks_on_test_map() {
    crate::register_translations();
    let mut state = hp_app::testing::test_state();
    HealthRuntime::check_now(&mut state);
    for p in &state.runtime.health.problems {
        assert!(!describe(p, &state.doc).is_empty());
    }
    state.doc.delete(&[ObjRef::Entity(4)]).ok();
    state.doc.set_entity_class(&[5], "func_door").ok();
    HealthRuntime::check_now(&mut state);
    let problems = state.runtime.health.problems.clone();
    assert!(codes(&problems).contains(&"no_player_spawn"), "{:?}", codes(&problems));
    let p = find(&problems, "brush_entity_without_solids", None).clone();
    let text = describe(&p, &state.doc);
    assert!(text.contains("func_door") && text.contains('5'), "{text}");
    assert_eq!(severity_of(&p), Severity::Error);
    assert!(!fix_hint(p.code).is_empty());

    go_to_problem(&mut state, &p);
    assert_eq!(state.doc.selected_objects(), vec![ObjRef::Entity(5)]);
    assert!(state.requested_actions.contains(&hp_app::Action::Center3dOnSelection));

    let mut dialog = ProblemsDialog::default();
    dialog.fix_all(&mut state);
    assert!(state.doc.scene().entity(5).is_none());
    assert!(state.runtime.health.recheck);
    HealthRuntime::check_now(&mut state);
    assert!(state.runtime.health.problems.iter().all(|p| p.code != "brush_entity_without_solids"));
}

#[test]
fn descriptions_for_every_code() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let state = hp_app::testing::test_state();
    let codes = [
        "invalid_solid",
        "renumbered_ids",
        "duplicate_object_id",
        "duplicate_side_id",
        "unknown_class",
        "missing_output_target",
        "brush_entity_without_solids",
        "disp_not_quad",
        "no_player_spawn",
        "tool_texture_on_world",
        "empty_map",
        "entity_out_of_bounds",
        "entity_outside_map",
        "missing_group",
        "missing_visgroup",
        "invalid_texture_axes",
        "duplicate_plane",
        "missing_target",
        "overlay_missing_faces",
        "missing_material",
        "missing_model",
        "mixed_face_contents",
        "disp_on_brush_entity",
        "too_many_sides",
        "micro_brush",
        "origin_brush_in_world",
        "duplicate_singleton",
        "unknown_input",
        "key_too_long",
        "entity_in_solid",
        "overlay_too_many_faces",
        "too_many_named_lights",
        "areaportal_brushes",
        "areaportal_not_door",
        "areaportal_water",
        "areaportal_occluder",
        "areaportal_door_state",
        "sky_without_sun",
        "sky_camera_multiple",
        "sky_camera_bad_scale",
        "skybox_switch_bad_target",
        "skybox_switch_per_player",
        "missing_sound",
        "missing_particle",
        "bad_content_path",
        "bad_material_path",
        "leak",
        "leak_check_incomplete",
        "detail_candidate",
        "limit_exceeded",
        "limit_close",
        "edict_budget",
        "duplicate_brush",
        "contained_brush",
        "duplicate_entity",
        "something_new",
    ];
    for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
        hp_app::i18n::set_lang(lang);
        for code in codes {
            let p = Problem::new(
                ProblemSeverity::Warning,
                code,
                format!("English {code} with tools/toolstrigger, which"),
                vec![ObjRef::Entity(5), ObjRef::Solid(2)],
                true,
            )
            .with_args(vec!["7".into(), "brushes".into(), "9000".into(), "110".into()]);
            let text = describe(&p, &state.doc);
            assert!(!text.is_empty());
            assert!(!text.contains("problems."), "{code}: missing translation: {text}");
            assert!(!text.contains('{'), "{code}: placeholder left: {text}");
            assert!(!fix_hint(code).starts_with("problems."), "{code}: missing fix hint");
            assert!(!kind_name(code).contains("problems."), "{code}");
            if code == "something_new" {
                assert!(text.starts_with("English"));
            }
        }
    }
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    assert_eq!(limit_name("brushes"), "Brushes");
    assert_eq!(limit_name("unknown_limit"), "unknown_limit");
    assert_eq!(tool_in_message("3 world solids use tools/toolstrigger, which only"), Some("tools/toolstrigger"));
    assert_eq!(tool_in_message("nothing"), None);
    assert_eq!(listed_in_message("x do not exist: a, b"), Some("a, b"));
    assert_eq!(listed_in_message("x does not exist: m.mdl"), Some("m.mdl"));
    assert_eq!(listed_in_message("x do not exist: "), None);
    assert_eq!(count_after("Entity 3 (x) lists 12 faces", " lists "), Some(12));
    assert_eq!(count_after("Entity 3 (x) lists many faces", " lists "), None);
    for code in ["duplicate_plane", "overlay_missing_faces", "micro_brush", "detail_candidate", "disp_on_brush_entity"]
    {
        assert_ne!(fix_hint(code), t!("problems.fix.generic"), "{code}");
    }
}

#[test]
fn single_fix_and_filters() {
    crate::register_translations();
    let mut state = hp_app::testing::test_state();
    state.doc.set_entity_class(&[5], "func_door").ok();
    HealthRuntime::check_now(&mut state);
    let mut d = ProblemsDialog::default();
    let idx = index_of(&state, "brush_entity_without_solids");
    d.fix(&mut state, idx);
    assert!(state.doc.scene().entity(5).is_none());
    state.doc.delete(&[ObjRef::Entity(4)]).ok();
    HealthRuntime::check_now(&mut state);
    d.update_rows(&state);
    assert!(!d.rows.is_empty());
    d.show_warnings = false;
    d.update_rows(&state);
    let problems = &state.runtime.health.problems;
    assert!(d.rows.iter().all(|i| problems[*i].severity != ProblemSeverity::Warning));
}

#[test]
fn rows_follow_ignores_and_visibility() {
    crate::register_translations();
    let mut state = hp_app::testing::test_state();
    state.doc.delete(&[ObjRef::Entity(4)]).ok();
    state.doc.set_entity_class(&[5], "func_door").ok();
    HealthRuntime::check_now(&mut state);
    let mut d = ProblemsDialog::default();
    d.update_rows(&state);
    let spawn = index_of(&state, "no_player_spawn");
    assert!(d.rows.contains(&spawn));
    let p = state.runtime.health.problems[spawn].clone();
    HealthRuntime::ignore(&mut state, &p);
    d.update_rows(&state);
    assert!(!d.rows.contains(&spawn));
    d.show_ignored = true;
    d.update_rows(&state);
    assert!(d.rows.contains(&spawn));
    HealthRuntime::unignore(&mut state, &p);
    assert!(!state.runtime.health.is_ignored(&p));
    let door = index_of(&state, "brush_entity_without_solids");
    state.doc.set_hidden(&[ObjRef::Entity(5)], true);
    state.settings.health.visible_only = true;
    d.update_rows(&state);
    assert!(!d.rows.contains(&door));
    assert!(d.rows.contains(&spawn), "problems about no object stay");
    state.settings.health.visible_only = false;
    d.update_rows(&state);
    assert!(d.rows.contains(&door));
}

fn box_sides(x0: f64, y0: f64, x1: f64, y1: f64) -> [String; 6] {
    [
        format!("({x0} {y1} 32) ({x1} {y1} 32) ({x1} {y0} 32)"),
        format!("({x0} {y0} 0) ({x1} {y0} 0) ({x1} {y1} 0)"),
        format!("({x0} {y1} 32) ({x0} {y0} 32) ({x0} {y0} 0)"),
        format!("({x1} {y1} 0) ({x1} {y0} 0) ({x1} {y0} 32)"),
        format!("({x1} {y1} 32) ({x0} {y1} 32) ({x0} {y1} 0)"),
        format!("({x1} {y0} 0) ({x0} {y0} 0) ({x0} {y0} 32)"),
    ]
}

fn solid_text(id: u32, first_side: u32, sides: &[(String, &str)]) -> String {
    let mut s = format!("solid {{ \"id\" \"{id}\"\n");
    for (i, (plane, material)) in sides.iter().enumerate() {
        let _ = writeln!(
            s,
            "side {{ \"id\" \"{}\" \"plane\" \"{plane}\" \"material\" \"{material}\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" }}",
            first_side + i as u32
        );
    }
    s.push_str("}\n");
    s
}

fn asset_problem_map() -> Document {
    let concrete = "concrete/concretefloor001a";
    let mut a: Vec<(String, &str)> = box_sides(0.0, 0.0, 64.0, 64.0).into_iter().map(|p| (p, concrete)).collect();
    a.push((a[0].0.clone(), concrete));
    let mut b: Vec<(String, &str)> = box_sides(128.0, 0.0, 192.0, 64.0).into_iter().map(|p| (p, concrete)).collect();
    b[0].1 = "liquids/water_test";
    b[1].1 = "MISSING/NOTHING";
    b[2].1 = "loading/later";
    let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\"\n");
    text.push_str(&solid_text(2, 1, &a));
    text.push_str(&solid_text(3, 101, &b));
    text.push_str("}\n");
    for (id, class, keys) in [
        (10, "info_player_start", ""),
        (20, "info_overlay", "\"sides\" \"9999\""),
        (21, "env_cubemap", "\"sides\" \"1 9998\""),
        (22, "prop_static", "\"model\" \"models/missing/nothing.mdl\""),
        (23, "prop_physics", "\"model\" \"models\\props_c17\\oildrum001.mdl\""),
        (24, "test_target_user", "\"landmark\" \"nowhere_lm\" \"note\" \"nowhere_note\" \"parentname\" \"real_one\""),
        (25, "point_teleport", "\"target\" \"ghost\""),
        (26, "info_target", "\"targetname\" \"real_one\""),
    ] {
        let _ = writeln!(text, "entity {{ \"id\" \"{id}\" \"classname\" \"{class}\" {keys} \"origin\" \"32 32 16\" }}");
    }
    let map = hp_vmf::parse(&text).unwrap_or_else(|e| panic!("test map: {e}\n{text}"));
    Document::from_map(map)
}

fn test_fgd() -> ResolvedFgd {
    let src = r#"
@PointClass = info_player_start : "Start" []
@PointClass = info_target : "Target" [ targetname(target_source) : "Name" ]
@PointClass = test_target_user : "Uses names" [
    landmark(target_destination) : "Landmark"
    note(string) : "Note"
    parentname(target_destination) : "Parent"
]
@PointClass = info_overlay : "Overlay" [ sides(sidelist) : "Sides" ]
@PointClass = env_cubemap : "Cubemap" [ sides(sidelist) : "Sides" ]
@PointClass = prop_static : "Static prop" [ model(studio) : "Model" ]
@PointClass = prop_physics : "Physics prop" [ model(studio) : "Model" ]
"#;
    hp_fgd::Fgd::parse(src, None).resolve()
}

fn fake_info(mock: &mut MockAssets, name: &str) -> Option<Arc<MaterialInfo>> {
    if name.to_ascii_lowercase().starts_with("loading/") {
        return None;
    }
    let mut info = (*mock.material_info(name)?).clone();
    let lower = name.to_ascii_lowercase();
    info.missing = lower.starts_with("missing/");
    info.is_water = lower.starts_with("liquids/");
    Some(Arc::new(info))
}

struct TestAssets {
    mock: MockAssets,
    status: AssetStatus,
    fs: Option<hp_fs::GameFs>,
    fgd: Option<ResolvedFgd>,
}

impl AssetsApi for TestAssets {
    fn status(&self) -> AssetStatus {
        self.status.clone()
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        self.fs.as_ref()
    }
    fn fgd(&self) -> Option<&ResolvedFgd> {
        self.fgd.as_ref()
    }
    fn generation(&self) -> u64 {
        self.mock.generation()
    }
    fn remount(&mut self, settings: &Settings) {
        self.mock.remount(settings);
    }
    fn reload_fgd(&mut self, settings: &Settings) {
        self.mock.reload_fgd(settings);
    }
    fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.mock.materials()
    }
    fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.mock.models()
    }
    fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.mock.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        self.mock.source_label(source)
    }
    fn material_info(&mut self, name: &str) -> Option<Arc<MaterialInfo>> {
        fake_info(&mut self.mock, name)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> ThumbState {
        self.mock.thumbnail(ctx, name)
    }
    fn model_info(&mut self, path: &str) -> ModelState {
        self.mock.model_info(path)
    }
    fn model_preview(
        &mut self,
        gfx: &mut Gfx,
        path: &str,
        camera: &PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        self.mock.model_preview(gfx, path, camera, size_px)
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        self.mock.entity_bounds(entity)
    }
    fn invalidate_material(&mut self, name: &str) {
        self.mock.invalidate_material(name);
    }
    fn import_images(&mut self, files: &[PathBuf], opts: &ImportOptions) -> Vec<Result<String, String>> {
        self.mock.import_images(files, opts)
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut Gfx>) {
        self.mock.begin_frame(ctx, gfx);
    }
    fn sync_scene(&mut self, doc: &mut Document, gfx: &mut Gfx, opts: &SceneOptions) {
        self.mock.sync_scene(doc, gfx, opts);
    }
    fn render_material_id(&mut self, name: &str) -> u32 {
        self.mock.render_material_id(name)
    }
    fn status_line(&self) -> Option<String> {
        self.mock.status_line()
    }
}

fn test_fs(name: &str) -> hp_fs::GameFs {
    let dir = crate::test_scratch(name);
    let model = dir.join("models/props_c17/oildrum001.mdl");
    std::fs::create_dir_all(model.parent().expect("parent")).expect("models dir");
    std::fs::write(&model, b"IDST").expect("model");
    let fs = hp_fs::GameFs::empty();
    fs.add_game_dir(&dir).expect("game dir");
    assert!(fs.exists("models/props_c17/oildrum001.mdl"));
    fs
}

#[test]
fn asset_and_fgd_checks() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let mut state = hp_app::testing::test_state();
    state.doc = asset_problem_map();
    state.settings.health.live_leak_check = false;
    state.assets = Box::new(TestAssets {
        mock: MockAssets::default(),
        status: AssetStatus::Ready,
        fs: Some(test_fs("problems_assets")),
        fgd: Some(test_fgd()),
    });
    HealthRuntime::check_now(&mut state);
    let problems = state.runtime.health.problems.clone();
    let doc = &state.doc;

    let dup = find(&problems, "duplicate_plane", None);
    assert_eq!(dup.faces.len(), 1);
    assert!(dup.fixable);
    let mixed = find(&problems, "mixed_face_contents", None);
    assert_eq!(mixed.objects, vec![ObjRef::Solid(3)]);
    let missing_mat = find(&problems, "missing_material", None);
    assert_eq!(missing_mat.faces.len(), 1, "the material still loading is not missing");
    let overlay = find(&problems, "overlay_missing_faces", Some(ObjRef::Entity(20)));
    assert_eq!(overlay.severity, ProblemSeverity::Error);
    let cubemap = find(&problems, "overlay_missing_faces", Some(ObjRef::Entity(21)));
    assert_eq!(cubemap.severity, ProblemSeverity::Warning);
    let model = find(&problems, "missing_model", Some(ObjRef::Entity(22)));
    assert_eq!(model.severity, ProblemSeverity::Error);
    assert!(
        !problems.iter().any(|p| p.code == "missing_model" && p.objects.contains(&ObjRef::Entity(23))),
        "the mounted model (backslashes, other case) exists"
    );
    let fgd_target = find(&problems, "missing_target", Some(ObjRef::Entity(24)));
    let common_target = find(&problems, "missing_target", Some(ObjRef::Entity(25)));

    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    let en: Vec<String> =
        [dup, mixed, missing_mat, overlay, cubemap, model, fgd_target, common_target].map(|p| describe(p, doc)).into();
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    let fr: Vec<String> =
        [dup, mixed, missing_mat, overlay, cubemap, model, fgd_target, common_target].map(|p| describe(p, doc)).into();
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    assert_eq!(
        en,
        [
            "1 face lies on the same plane as another face of its solid",
            "1 solid mixes water and solid faces (one content per brush)",
            "1 missing material: MISSING/NOTHING",
            "Overlay 20 (info_overlay) is on no existing face",
            "Entity 21 (env_cubemap) lists 1 face that does not exist",
            "Entity 22 (prop_static) uses a model that does not exist: models/missing/nothing.mdl",
            "Entity 24 (test_target_user) names entities that do not exist: landmark = 'nowhere_lm'",
            "Entity 25 (point_teleport) names entities that do not exist: target = 'ghost'",
        ]
    );
    assert_eq!(
        fr,
        [
            "1 face est sur le même plan qu'une autre face de son solide",
            "1 solide mélange des faces d'eau et des faces solides (un seul contenu par brush)",
            "1 matériau manquant : MISSING/NOTHING",
            "L'overlay 20 (info_overlay) n'est sur aucune face existante",
            "L'entité 21 (env_cubemap) liste 1 face inexistante",
            "L'entité 22 (prop_static) utilise un modèle inexistant : models/missing/nothing.mdl",
            "L'entité 24 (test_target_user) nomme des entités inexistantes : landmark = 'nowhere_lm'",
            "L'entité 25 (point_teleport) nomme des entités inexistantes : target = 'ghost'",
        ]
    );

    let fixes: Vec<Problem> = problems.iter().filter(|p| p.fixable).cloned().collect();
    assert!(state.doc.fix_problems(&fixes).is_ok_and(|n| n >= 2));
    assert_eq!(state.doc.scene().solid(2).map(|n| n.solid.sides.len()), Some(6));
    assert!(state.doc.scene().entity(20).is_none());
    assert_eq!(state.doc.scene().entity(21).and_then(|n| n.entity.get("sides")), Some("1"));
    HealthRuntime::check_now(&mut state);
    let after = codes(&state.runtime.health.problems);
    assert!(!after.contains(&"duplicate_plane") && !after.contains(&"overlay_missing_faces"), "{after:?}");
}

#[test]
fn nothing_is_missing_while_the_content_mounts() {
    crate::register_translations();
    let mut state = hp_app::testing::test_state();
    state.doc = asset_problem_map();
    state.settings.health.live_leak_check = false;
    for status in [AssetStatus::Mounting, AssetStatus::NotConfigured] {
        state.assets = Box::new(TestAssets { mock: MockAssets::default(), status, fs: None, fgd: None });
        HealthRuntime::check_now(&mut state);
        let found = codes(&state.runtime.health.problems);
        for code in ["missing_material", "mixed_face_contents"] {
            assert!(!found.contains(&code), "{code}: {found:?}");
        }
        assert!(
            !state
                .runtime
                .health
                .problems
                .iter()
                .any(|p| p.code == "missing_model" && p.objects.contains(&ObjRef::Entity(22)))
        );
    }
}

#[test]
fn dialog_draws_and_asks_for_a_check() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let mut state = hp_app::testing::test_state();
    state.settings.health.live_checks = false;
    state.ui.show_problems = true;
    let mut d = ProblemsDialog::default();
    hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    assert!(state.runtime.health.recheck, "no live checks: the dialog asks for one");
    state.doc.delete(&[ObjRef::Entity(4)]).ok();
    HealthRuntime::check_now(&mut state);
    let p = state.runtime.health.problems[0].clone();
    HealthRuntime::ignore(&mut state, &p);
    d.show_ignored = true;
    hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
}

#[test]
#[ignore = "benchmark"]
fn bench_checks_large_map() {
    crate::register_translations();
    let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\"\n");
    let materials = ["concrete/concretefloor001a", "dev/dev_measuregeneric01b", "tools/toolsnodraw", "brick/brick01"];
    for i in 0..40_000u32 {
        let (x0, y0) = (f64::from(i % 200) * 64.0, f64::from(i / 200) * 64.0);
        let sides: Vec<(String, &str)> = box_sides(x0, y0, x0 + 32.0, y0 + 32.0)
            .into_iter()
            .enumerate()
            .map(|(k, p)| (p, materials[(i as usize + k) % materials.len()]))
            .collect();
        text.push_str(&solid_text(i + 2, i * 6 + 1, &sides));
    }
    text.push_str("}\n");
    for e in 0..8_000u32 {
        let _ = writeln!(
            text,
            "entity {{ \"id\" \"{}\" \"classname\" \"prop_static\" \"model\" \"models/props/m{}.mdl\" \"target\" \"t{}\" \"origin\" \"{} 0 64\" }}",
            1_000_000 + e,
            e % 300,
            e % 50,
            e * 8
        );
    }
    let map = hp_vmf::parse(&text).expect("parse");
    let mut state = hp_app::testing::test_state();
    state.doc = Document::from_map(map);
    for round in 0..3 {
        let t0 = std::time::Instant::now();
        HealthRuntime::check_now(&mut state);
        eprintln!("round {round}: every check {:?} ({} problems)", t0.elapsed(), state.runtime.health.problems.len());
    }
}

#[test]
fn missing_output_targets_are_named() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let mut state = hp_app::testing::test_state();
    let relay = state.doc.create_point_entity("logic_relay", glam::DVec3::ZERO, &[]).expect("relay");
    for target in ["Ghost", "nobody"] {
        let c = hp_vmf::Connection::parse("OnTrigger", &format!("{target},Kill,,0,-1"));
        state.doc.add_connection(&[relay], &c).expect("output");
    }
    HealthRuntime::check_now(&mut state);
    let p = find(&state.runtime.health.problems, "missing_output_target", Some(ObjRef::Entity(relay))).clone();
    let text = describe(&p, &state.doc);
    assert!(text.ends_with(": ghost, nobody"), "{text}");
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    let text = describe(&p, &state.doc);
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    assert!(text.ends_with(" : ghost, nobody"), "{text}");
    let bare = Problem { args: Vec::new(), ..p };
    assert!(!describe(&bare, &state.doc).contains("nobody"));
}
