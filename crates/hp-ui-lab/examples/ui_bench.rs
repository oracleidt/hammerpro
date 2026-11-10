use std::path::PathBuf;
use std::time::Instant;

use hp_app::AppState;
use hp_app::testing::MockAssets;
use hp_fs::AssetEntry;

const WIDTH: f32 = 1600.0;
const HEIGHT: f32 = 900.0;
const FRAME_S: f64 = 1.0 / 60.0;

struct Args {
    map: Option<PathBuf>,
    fgd_dir: Option<PathBuf>,
    frames: usize,
    materials: usize,
    models: usize,
    sounds: usize,
    console: usize,
    only: Vec<String>,
}

fn args() -> Args {
    let mut a = Args {
        map: None,
        fgd_dir: None,
        frames: 200,
        materials: 60_000,
        models: 40_000,
        sounds: 50_000,
        console: 5_000,
        only: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().unwrap_or_default();
        match arg.as_str() {
            "--map" => a.map = Some(PathBuf::from(value())),
            "--fgd-dir" => a.fgd_dir = Some(PathBuf::from(value())),
            "--frames" => a.frames = value().parse().unwrap_or(a.frames),
            "--materials" => a.materials = value().parse().unwrap_or(a.materials),
            "--models" => a.models = value().parse().unwrap_or(a.models),
            "--sounds" => a.sounds = value().parse().unwrap_or(a.sounds),
            "--console" => a.console = value().parse().unwrap_or(a.console),
            "--only" => a.only = value().split(',').map(str::to_owned).collect(),
            _ => {}
        }
    }
    a
}

fn names(prefix: &str, ext: &str, count: usize) -> Vec<AssetEntry> {
    let dirs =
        ["concrete", "metal", "wood", "props_c17", "props_junk", "nature", "building", "decals", "tools", "glass"];
    let mut v: Vec<AssetEntry> = (0..count)
        .map(|i| {
            let dir = dirs[i % dirs.len()];
            AssetEntry { name: format!("{prefix}{dir}/sub{:02}/item_{i:06}{ext}", i % 37), source: (i % 5) as u32 }
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn state(a: &Args) -> AppState {
    let mut assets = MockAssets::with_lists(
        names("", "", a.materials),
        names("models/", ".mdl", a.models),
        names("sound/", ".wav", a.sounds),
    );
    if let Some(dir) = &a.fgd_dir {
        match hp_fgd::Fgd::load(dir.join("garrysmod.fgd")) {
            Ok(fgd) => assets.set_fgd(fgd.resolve()),
            Err(e) => eprintln!("fgd: {e}"),
        }
    }
    let mut state = hp_app::testing::test_state_with_assets(assets);
    if let Some(path) = &a.map {
        let t = Instant::now();
        let text = std::fs::read_to_string(path).expect("map");
        let map = hp_vmf::parse(&text).expect("vmf");
        state.doc = hp_editor::Document::from_map(map);
        let stats = state.doc.stats();
        eprintln!(
            "map {} loaded in {:.0} ms: {} solids, {} entities",
            path.display(),
            t.elapsed().as_secs_f64() * 1000.0,
            stats.solids,
            stats.entities
        );
    }
    for i in 0..a.console {
        match i % 4 {
            0 => state.console.info(format!("Loaded material concrete/floor_{i:05} from hl2_textures.vpk")),
            1 => state.console.warn(format!("Missing model models/props/thing_{i:05}.mdl")),
            2 => state.console.success(format!("Compile step {i} finished in 12 ms")),
            _ => state.console.error(format!("Leak detected near entity {i}")),
        }
    }
    hp_ui_shell::register_translations();
    hp_ui_mapping::register_translations();
    hp_ui_entities::register_translations();
    hp_ui_textures::register_translations();
    hp_ui_lab::register_translations();
    state
}

fn input(frame: usize) -> egui::RawInput {
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH, HEIGHT))),
        ..Default::default()
    };
    let y = 300.0 + (frame % 7) as f32;
    raw.events.push(egui::Event::PointerMoved(egui::pos2(WIDTH - 200.0, y)));
    raw
}

fn measure(label: &str, a: &Args, state: &mut AppState, mut f: impl FnMut(&mut egui::Ui, &mut AppState)) {
    if !a.only.is_empty() && !a.only.iter().any(|o| o == label) {
        return;
    }
    let ctx = egui::Context::default();
    ctx.set_visuals(hp_app::theme::visuals(hp_app::settings::ThemeChoice::Dark));
    for i in 0..20 {
        state.frame.time += FRAME_S;
        let mut out = ctx.run_ui(input(i), |ui| f(ui, state));
        out.textures_delta.clear();
    }
    let mut times: Vec<f64> = Vec::with_capacity(a.frames);
    for i in 0..a.frames {
        state.frame.time += FRAME_S;
        let t = Instant::now();
        let mut out = ctx.run_ui(input(i), |ui| f(ui, state));
        let _ = ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
        out.textures_delta.clear();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|x, y| x.total_cmp(y));
    let avg = times.iter().sum::<f64>() / times.len() as f64;
    let med = times[times.len() / 2];
    let p95 = times[times.len() * 95 / 100];
    println!("{label:<18} avg {avg:7.3} ms  median {med:7.3} ms  p95 {p95:7.3} ms");
}

fn dock(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui)) {
    let ctx = ui.ctx().clone();
    egui::Window::new("dock")
        .fixed_pos(egui::pos2(WIDTH - 400.0, 120.0))
        .fixed_size(egui::vec2(380.0, 760.0))
        .title_bar(false)
        .show(&ctx, |ui| {
            ui.set_min_size(egui::vec2(380.0, 760.0));
            f(ui);
        });
}

fn select_some(state: &mut AppState, class: &str) {
    let ids: Vec<hp_editor::ObjRef> = state
        .doc
        .scene()
        .entities()
        .values()
        .filter(|n| n.entity.classname == class)
        .take(1)
        .map(|n| hp_editor::ObjRef::Entity(n.entity.id))
        .collect();
    state.doc.set_selection(&ids);
}

fn main() {
    let a = args();
    let mut state = state(&a);
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);

    let mut shell = hp_ui_shell::Shell::new();
    measure("shell_bars", &a, &mut state, |ui, state| {
        egui::Panel::top("top").show(ui, |ui| {
            shell.menu_bar(ui, state);
            shell.toolbar(ui, state);
        });
        egui::Panel::bottom("status").show(ui, |ui| shell.status_bar(ui, state, None));
        egui::Panel::left("strip").show(ui, |ui| {
            shell.tool_strip(ui, state);
        });
    });
    measure("console", &a, &mut state, |ui, state| {
        egui::Panel::bottom("console").exact_size(240.0).show(ui, |ui| shell.console(ui, state));
    });

    let mut textures = hp_ui_textures::TextureBrowser::new();
    measure("textures", &a, &mut state, |ui, state| dock(ui, |ui| textures.ui(ui, state)));
    let mut textures_search = hp_ui_textures::TextureBrowser::new();
    textures_search.set_search("metal");
    measure("textures_search", &a, &mut state, |ui, state| dock(ui, |ui| textures_search.ui(ui, state)));

    const TYPED: [&str; 8] = ["c", "co", "con", "conc", "concr", "concre", "concret", "concrete"];
    let mut typing = hp_ui_textures::TextureBrowser::new();
    let mut n = 0usize;
    measure("textures_typing", &a, &mut state, |ui, state| {
        n += 1;
        typing.set_search(TYPED[n % TYPED.len()]);
        dock(ui, |ui| typing.ui(ui, state));
    });

    let mut models = hp_ui_entities::ModelBrowser::new();
    let ctx = egui::Context::default();
    let start = Instant::now();
    while models.row_count() == 0 && start.elapsed().as_secs() < 30 {
        let mut out = ctx.run_ui(input(0), |ui| dock(ui, |ui| models.ui(ui, &mut state)));
        out.textures_delta.clear();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    eprintln!("models: {} rows after {:.0} ms", models.row_count(), start.elapsed().as_secs_f64() * 1000.0);
    state.settings.browser.model_grid = true;
    measure("models_grid", &a, &mut state, |ui, state| dock(ui, |ui| models.ui(ui, state)));
    state.settings.browser.model_grid = false;
    measure("models_list", &a, &mut state, |ui, state| dock(ui, |ui| models.ui(ui, state)));
    let mut n = 0usize;
    measure("models_typing", &a, &mut state, |ui, state| {
        n += 1;
        models.set_search(TYPED[n % TYPED.len()]);
        dock(ui, |ui| models.ui(ui, state));
    });
    models.set_search("");

    let mut sounds = hp_ui_entities::SoundBrowser::new();
    sounds.set_mode(hp_ui_entities::SoundMode::Files);
    measure("sounds", &a, &mut state, |ui, state| dock(ui, |ui| sounds.ui(ui, state)));
    let mut n = 0usize;
    measure("sounds_typing", &a, &mut state, |ui, state| {
        n += 1;
        sounds.set_search(TYPED[n % TYPED.len()]);
        dock(ui, |ui| sounds.ui(ui, state));
    });
    let mut particles = hp_ui_entities::ParticleBrowser::new();
    measure("particles", &a, &mut state, |ui, state| dock(ui, |ui| particles.ui(ui, state)));

    let mut outliner = hp_ui_mapping::Outliner::new();
    measure("outliner", &a, &mut state, |ui, state| dock(ui, |ui| outliner.ui(ui, state)));
    let edited: Vec<hp_editor::Id> = state
        .doc
        .scene()
        .entities()
        .values()
        .find(|n| n.entity.classname == "light" || n.entity.classname == "info_player_start")
        .map(|n| vec![n.entity.id])
        .unwrap_or_default();
    let mut n = 0usize;
    measure("outliner_edit", &a, &mut state, |ui, state| {
        n += 1;
        let _ = state.doc.set_entity_key(&edited, "hp_bench", if n.is_multiple_of(2) { "a" } else { "b" });
        dock(ui, |ui| outliner.ui(ui, state));
    });
    let mut visgroups = hp_ui_mapping::VisgroupPanel::new();
    measure("visgroups", &a, &mut state, |ui, state| dock(ui, |ui| visgroups.ui(ui, state)));
    let mut lighting = hp_ui_mapping::lighting::LightingTab::default();
    measure("lighting", &a, &mut state, |ui, state| dock(ui, |ui| lighting.ui(ui, state)));
    let mut assets = hp_ui_mapping::mapassets::MapAssetsPanel::new();
    measure("map_assets", &a, &mut state, |ui, state| dock(ui, |ui| assets.ui(ui, state)));

    select_some(&mut state, "prop_static");
    let mut entity = hp_ui_entities::EntityPanel::new();
    measure("entity_prop", &a, &mut state, |ui, state| dock(ui, |ui| entity.ui(ui, state)));
    select_some(&mut state, "light");
    measure("entity_light", &a, &mut state, |ui, state| dock(ui, |ui| entity.ui(ui, state)));
    let lit: Vec<hp_editor::Id> = state.doc.selection().objects.iter().map(|o| o.id()).collect();
    let mut n = 0usize;
    measure("entity_edit", &a, &mut state, |ui, state| {
        n += 1;
        let _ = state.doc.set_entity_key(
            &lit,
            "_light",
            if n.is_multiple_of(2) { "255 255 255 200" } else { "255 240 200 300" },
        );
        dock(ui, |ui| entity.ui(ui, state));
    });
    state.doc.set_selection(&[]);

    let mut dialogs = hp_ui_mapping::MapDialogs::new();
    let t = Instant::now();
    hp_app::runtime::HealthRuntime::check_now(&mut state);
    eprintln!(
        "problems: {} found in {:.0} ms",
        state.runtime.health.problems.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    state.ui.show_problems = true;
    measure("problems", &a, &mut state, |ui, state| dialogs.show(&ui.ctx().clone(), state));
    state.ui.show_problems = false;

    let mut entity_dialogs = hp_ui_entities::EntityDialogs::new();
    state.ui.show_entity_report = true;
    measure("entity_report", &a, &mut state, |ui, state| entity_dialogs.show(&ui.ctx().clone(), state));
    state.ui.show_entity_report = false;
    state.ui.show_find_entities = true;
    entity_dialogs.set_find_query("prop");
    measure("find_entities", &a, &mut state, |ui, state| entity_dialogs.show(&ui.ctx().clone(), state));
    state.ui.show_find_entities = false;

    state.ui.show_settings = true;
    measure("settings", &a, &mut state, |ui, state| shell.dialogs(&ui.ctx().clone(), state));
    state.ui.show_settings = false;
}
