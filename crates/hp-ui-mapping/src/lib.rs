mod autovis;
mod compile;
mod dialogs;
pub mod native_window;
mod outliner;
mod tr;
mod util;
mod visgroups;

pub mod areaportals;
pub mod cordons;
pub mod cubemaps;
pub mod inspector;
pub mod lighting;
pub mod mapassets;
pub mod optimize;
pub mod portals;
pub mod prefabs;
pub mod propper;
pub mod release;
mod scatter;

pub use compile::{
    CompileDialog, bsp_compile_args, compile_content_args, compile_export_of, compile_options_ui, compiler_commands,
    compiler_list, compiler_name, compiler_problems, compiler_tools, compilers_settings_ui, effective_step_flags,
    valve_flag_text, valve_group_text, valve_managed_text, viewport_id as compile_viewport_id,
};
pub use dialogs::{MapDialogs, go_to_leak_start};
pub use outliner::Outliner;
pub use tr::TABLES;
pub use visgroups::VisgroupPanel;

pub fn register_translations() {
    for (_, table) in TABLES {
        hp_app::i18n::register(table);
    }
}

fn package_dialogs() -> Vec<Box<dyn dialogs::MapDialog>> {
    let mut list = dialogs::instance::dialogs();
    list.extend(cordons::dialogs());
    list.extend(portals::dialogs());
    list.extend(areaportals::dialogs());
    list.extend(optimize::dialogs());
    list.extend(cubemaps::dialogs());
    list.extend(release::dialogs());
    list.extend(inspector::dialogs());
    list.extend(lighting::dialogs());
    list.extend(prefabs::dialogs());
    list.extend(propper::dialogs());
    list.extend(scatter::dialogs());
    list
}

#[cfg(test)]
pub(crate) fn lang_lock() -> std::sync::MutexGuard<'static, ()> {
    hp_app::testing::lang_lock()
}

#[cfg(test)]
pub(crate) fn test_scratch(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/hp-ui-mapping/scratch")
        .join(format!("{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        panic!("cannot create scratch dir {}: {e}", dir.display());
    }
    dir
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::Path;

    use hp_app::testing::{run_ui, test_state};
    use hp_editor::ObjRef;

    use super::*;

    #[test]
    fn translation_table_is_clean() {
        let core: HashSet<&str> = hp_app::strings::TR.iter().map(|(k, _, _)| *k).collect();
        let mut seen = HashSet::new();
        for (prefix, table) in TABLES {
            let problems = hp_app::i18n::lint_table(table);
            assert!(problems.is_empty(), "{prefix}: {problems:#?}");
            for (k, _, _) in *table {
                assert!(!core.contains(k), "key {k} collides with a core key");
                assert!(
                    k.starts_with(&format!("{prefix}.")),
                    "key {k} of the {prefix} table must start with {prefix}."
                );
                assert!(seen.insert(*k), "key {k} is in two tables");
            }
        }
        let mut prefixes: Vec<&str> = TABLES.iter().map(|(p, _)| *p).collect();
        prefixes.sort_unstable();
        prefixes.dedup();
        assert_eq!(prefixes.len(), TABLES.len(), "one table per prefix");
    }

    #[test]
    fn every_used_key_exists() {
        let ours: HashSet<&str> = TABLES.iter().flat_map(|(_, t)| t.iter().map(|(k, _, _)| *k)).collect();
        let core: HashSet<&str> = hp_app::strings::TR.iter().map(|(k, _, _)| *k).collect();
        let prefixes: Vec<String> = TABLES.iter().map(|(p, _)| format!("{p}.")).collect();
        let mut files = Vec::new();
        let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|e| e == "rs") && !p.parent().is_some_and(|d| d.ends_with("tr")) {
                    files.push(p);
                }
            }
        }
        assert!(files.len() > 5);
        let mut missing = Vec::new();
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap_or_default();
            for marker in ["t!(\"", "tf!(\""] {
                for (pos, _) in text.match_indices(marker) {
                    let boundary = text[..pos].chars().next_back().is_none_or(|c| !c.is_alphanumeric() && c != '_');
                    if !boundary {
                        continue;
                    }
                    let rest = &text[pos + marker.len()..];
                    let key = rest.split('"').next().unwrap_or_default();
                    if !ours.contains(key) && !core.contains(key) {
                        missing.push(format!("{}: {key}", f.display()));
                    }
                }
            }
            for (pos, _) in text.match_indices('"') {
                let rest = &text[pos + 1..];
                let lit = rest.split('"').next().unwrap_or_default();
                let looks_like_key = prefixes.iter().any(|p| lit.starts_with(p))
                    && lit.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
                    && !lit.ends_with('.');
                if looks_like_key && !ours.contains(lit) {
                    missing.push(format!("{}: {lit}", f.display()));
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "missing translations: {missing:#?}");
    }

    #[test]
    fn no_forbidden_dashes_in_sources() {
        let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|e| e == "rs" || e == "toml") {
                    let text = std::fs::read_to_string(&p).unwrap_or_default();
                    assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'), "dash in {}", p.display());
                }
            }
        }
    }

    fn frame(ctx: &egui::Context, f: impl FnMut(&mut egui::Ui)) {
        let mut out = ctx.run_ui(egui::RawInput::default(), f);
        out.textures_delta.clear();
    }

    #[test]
    fn icon_glyphs_exist() {
        let ctx = egui::Context::default();
        frame(&ctx, |_| {});
        let glyphs = [
            '➕', '👁', '🌐', '🔗', '⚙', '📂', '▣', '⚫', '▪', '✖', '📋', '⏹', '▶', '○', '✔', '❌', '⊗', '⚠', 'ℹ', '🔄',
            '⟲', '🎯', '🎥', '⏮', '◀', '⏷', '⏵', '⏶', '🗗', '⊟', '🗁',
        ];
        assert!(ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(13.0), 'a')), "fonts are loaded");
        let missing: Vec<char> = glyphs
            .iter()
            .copied()
            .filter(|c| !ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(13.0), *c)))
            .collect();
        assert!(missing.is_empty(), "missing glyphs: {missing:?}");
    }

    #[test]
    fn smoke_visgroup_panel() {
        let _lang = lang_lock();
        register_translations();
        let mut state = test_state();
        let mut panel = VisgroupPanel::new();
        run_ui(&mut state, |ui, state| panel.ui(ui, state));
        let a = state.doc.create_visgroup("Walls", None, &[ObjRef::Solid(2)]).unwrap_or(0);
        let b = state.doc.create_visgroup("Lights", Some(a), &[ObjRef::Entity(5)]).unwrap_or(0);
        state.doc.set_visgroup_visible(b, false).ok();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        run_ui(&mut state, |ui, state| panel.ui(ui, state));
        assert!(!state.doc.is_visible(ObjRef::Entity(5)));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        run_ui(&mut state, |ui, state| panel.ui(ui, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        crate::autovis::set_builtin_visible(&mut state.doc, crate::autovis::AutoKind::Lights, false);
        run_ui(&mut state, |ui, state| panel.ui(ui, state));
    }

    #[test]
    fn smoke_outliner() {
        let _lang = lang_lock();
        register_translations();
        let mut state = test_state();
        let mut outliner = Outliner::new();
        run_ui(&mut state, |ui, state| outliner.ui(ui, state));
        state.doc.group(&[ObjRef::Solid(2), ObjRef::Entity(4)]).ok();
        state.doc.set_hidden(&[ObjRef::Entity(5)], true);
        state.doc.set_selection(&[ObjRef::Solid(7)]);
        run_ui(&mut state, |ui, state| outliner.ui(ui, state));
        outliner.set_search("main");
        run_ui(&mut state, |ui, state| outliner.ui(ui, state));
        assert!(outliner.visible_keys().contains(&outliner::NodeKey::Entity(5)));
        outliner.set_search("no such thing");
        run_ui(&mut state, |ui, state| outliner.ui(ui, state));
        assert!(outliner.visible_keys().is_empty());
        outliner.set_search("");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        run_ui(&mut state, |ui, state| outliner.ui(ui, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }

    #[test]
    fn real_maps_when_available() {
        register_translations();
        let dir = Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples");
        let mut maps: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmf")))
            .collect();
        let extra = Path::new(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf");
        if extra.is_file() {
            maps.push(extra.to_path_buf());
        }
        if maps.is_empty() {
            eprintln!("no sample maps found, skipping");
            return;
        }
        maps.sort();
        for path in maps.iter().take(3) {
            let Ok(doc) = hp_editor::Document::open(path) else { continue };
            let mut state = test_state();
            state.doc = doc;
            let mut outliner = Outliner::new();
            let mut panel = VisgroupPanel::new();
            let t0 = std::time::Instant::now();
            run_ui(&mut state, |ui, state| {
                outliner.ui(ui, state);
                panel.ui(ui, state);
            });
            let problems = dialogs_problem_count(&state);
            eprintln!("{}: panels in {:?}, {problems} problems", path.display(), t0.elapsed());
        }
    }

    fn big_map(solids: usize, entities: usize) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let _ = writeln!(s, r#"world {{ "id" "1" "classname" "worldspawn""#);
        let mut side = 1;
        for i in 0..solids {
            let (x0, y0) = ((i % 200) as f64 * 64.0, (i / 200) as f64 * 64.0);
            let (x1, y1) = (x0 + 32.0, y0 + 32.0);
            let planes = [
                format!("({x0} {y1} 32) ({x1} {y1} 32) ({x1} {y0} 32)"),
                format!("({x0} {y0} 0) ({x1} {y0} 0) ({x1} {y1} 0)"),
                format!("({x0} {y1} 32) ({x0} {y0} 32) ({x0} {y0} 0)"),
                format!("({x1} {y1} 0) ({x1} {y0} 0) ({x1} {y0} 32)"),
                format!("({x1} {y1} 32) ({x0} {y1} 32) ({x0} {y1} 0)"),
                format!("({x1} {y0} 0) ({x0} {y0} 0) ({x0} {y0} 32)"),
            ];
            let _ = writeln!(s, r#"solid {{ "id" "{}""#, i + 2);
            for p in planes {
                let _ = writeln!(
                    s,
                    r#"side {{ "id" "{side}" "plane" "{p}" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }}"#
                );
                side += 1;
            }
            let group = if i % 2 == 0 { format!(r#""groupid" "{}""#, 1_000_000 + i / 100) } else { String::new() };
            let _ = writeln!(
                s,
                r#"editor {{ "color" "0 180 255" {group} "visgroupshown" "1" "visgroupautoshown" "1" }} }}"#
            );
        }
        for g in 0..solids.div_ceil(200) {
            let _ = writeln!(s, r#"group {{ "id" "{}" editor {{ "color" "0 100 0" }} }}"#, 1_000_000 + g);
        }
        let _ = writeln!(s, "}}");
        let classes = ["light", "prop_static", "info_player_start", "func_button_marker", "env_sprite"];
        for e in 0..entities {
            let _ = writeln!(
                s,
                r#"entity {{ "id" "{}" "classname" "{}" "targetname" "ent_{e}" "origin" "{} 0 64" }}"#,
                2_000_000 + e,
                classes[e % classes.len()],
                e * 8
            );
        }
        s
    }

    #[test]
    #[ignore = "benchmark"]
    fn bench_large_map() {
        register_translations();
        let text = big_map(40_000, 8_000);
        let map = hp_vmf::parse(&text).expect("parse");
        let mut state = test_state();
        state.doc = hp_editor::Document::from_map(map);
        let stats = state.doc.stats();
        eprintln!("map: {} solids, {} entities, {} groups", stats.solids, stats.entities, stats.groups);
        let mut outliner = Outliner::new();
        let mut panel = VisgroupPanel::new();
        let ctx = egui::Context::default();
        for round in 0..3 {
            state.doc.set_selection(&[ObjRef::Solid(2 + round)]);
            let t0 = std::time::Instant::now();
            frame(&ctx, |ui| outliner.ui(ui, &mut state));
            let t1 = std::time::Instant::now();
            frame(&ctx, |ui| panel.ui(ui, &mut state));
            let t2 = std::time::Instant::now();
            frame(&ctx, |ui| {
                outliner.ui(ui, &mut state);
                panel.ui(ui, &mut state);
            });
            let t3 = std::time::Instant::now();
            eprintln!(
                "round {round}: outliner rebuild {:?}, visgroups rebuild {:?}, steady frame {:?}",
                t1 - t0,
                t2 - t1,
                t3 - t2
            );
        }
        outliner.set_search("ent_7");
        let t0 = std::time::Instant::now();
        frame(&ctx, |ui| outliner.ui(ui, &mut state));
        eprintln!("search: {:?}", t0.elapsed());
        let t0 = std::time::Instant::now();
        let rows = visgroups::flatten_visgroups(&state.doc, &HashSet::new());
        let t1 = std::time::Instant::now();
        let mut auto = autovis::AutoVisData::new(&[]);
        auto.recount(&state.doc);
        let t2 = std::time::Instant::now();
        let mut classes = outliner::ClassNames::default();
        let tree = outliner::build_tree(&state.doc, &mut classes);
        let t3 = std::time::Instant::now();
        eprintln!(
            "flatten visgroups {:?} ({} rows), auto recount {:?}, outliner tree {:?} ({} roots)",
            t1 - t0,
            rows.len(),
            t2 - t1,
            t3 - t2,
            tree.len()
        );
    }

    fn dialogs_problem_count(state: &hp_app::AppState) -> usize {
        state.doc.check_problems_with(&hp_editor::ProblemContext::default()).len()
    }
}
