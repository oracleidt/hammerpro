use std::collections::HashSet;

use super::*;

#[test]
fn translation_table_is_clean() {
    let problems = hp_app::i18n::lint_table(TR);
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(TR.iter().all(|(k, _, _)| k.starts_with("vtf.")));
}

#[test]
fn every_used_key_is_translated() {
    let keys: HashSet<&str> = TR.iter().map(|(k, _, _)| *k).collect();
    let core: HashSet<&str> = hp_app::strings::TR.iter().map(|(k, _, _)| *k).collect();
    let other: HashSet<&str> = crate::TR.iter().map(|(k, _, _)| *k).collect();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("vtfedit");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
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
                if !keys.contains(key) && !core.contains(key) && !other.contains(key) {
                    missing.push(format!("{}: {key}", path.display()));
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn dynamic_keys_exist() {
    let keys: HashSet<&str> = TR.iter().map(|(k, _, _)| *k).collect();
    let mut wanted: Vec<String> = Vec::new();
    for (_, id) in hp_vtf::flags::NAMED {
        wanted.push(format!("vtf.flag.{id}"));
        wanted.push(format!("vtf.flag.{id}.tip"));
    }
    for s in hp_vtf::ops::HeightSource::ALL {
        wanted.push(format!("vtf.height.{}", s.id()));
    }
    for s in hp_vtf::ops::NormalKernel::ALL {
        wanted.push(format!("vtf.kernel.{}", s.id()));
    }
    for s in hp_vtf::ops::NormalAlpha::ALL {
        wanted.push(format!("vtf.nalpha.{}", s.id()));
    }
    for s in hp_vtf::ops::ResizeFilter::ALL {
        wanted.push(format!("vtf.rfilter.{}", s.id()));
    }
    for m in ["opaque", "constant", "luminance", "red", "green", "blue", "invert", "image"] {
        wanted.push(format!("vtf.alpha.{m}"));
    }
    for f in 0..7 {
        wanted.push(format!("vtf.face.{}", face_suffix(f)));
    }
    let missing: Vec<&String> = wanted.iter().filter(|k| !keys.contains(k.as_str())).collect();
    assert!(missing.is_empty(), "{missing:?}");
}

#[test]
fn no_forbidden_dashes() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("vtfedit");
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'), "{}", entry.path().display());
    }
}

fn gradient(w: u32, h: u32) -> hp_vtf::RgbaImage {
    let px: Vec<u8> = (0..w * h)
        .flat_map(|i| [(i % w * 4) as u8, (i / w * 4) as u8, 80, if i % 7 == 0 { 40 } else { 255 }])
        .collect();
    hp_vtf::RgbaImage::new(w, h, px).expect("image")
}

fn state() -> AppState {
    let state = hp_app::testing::test_state();
    crate::register_translations();
    state
}

fn pump(editor: &mut VtfEditor, state: &mut AppState) {
    hp_app::testing::run_ctx(state, |ctx, s| editor.show(ctx, s));
}

#[test]
fn window_opens_files_and_saves() {
    let dir = crate::tests::scratch("vtfedit_window");
    let src = dir.join("wall.vtf");
    let opts = hp_vtf::EncodeOptions { format: hp_vtf::EncodeFormat::Bgra8888, ..hp_vtf::EncodeOptions::default() };
    std::fs::write(&src, hp_vtf::encode(&gradient(32, 32), &opts).unwrap()).unwrap();
    let mut state = state();
    let mut editor = VtfEditor::new();
    state.runtime.vtf.request = Some(VtfRequest { file: Some(src.clone()), ..VtfRequest::default() });
    pump(&mut editor, &mut state);
    assert!(state.runtime.vtf.show && editor.is_open());
    for tab in 0..4 {
        editor.view.tab = tab;
        pump(&mut editor, &mut state);
    }
    assert!(editor.view.tex.is_some(), "the picture is uploaded");

    let png = dir.join("seen.png");
    editor.export_current(&png);
    assert!(png.is_file(), "{:?}", editor.status);
    let back = hp_vtf::load_image_file(&std::fs::read(&png).unwrap(), "png").unwrap();
    assert_eq!((back.width, back.height), (32, 32));

    {
        let l = editor.loaded.as_mut().unwrap();
        l.doc.flags |= hp_vtf::flags::CLAMPS | hp_vtf::flags::ANISOTROPIC;
        l.doc.bumpmap_scale = 4.0;
    }
    let ctx = egui::Context::default();
    editor.start_save(&ctx, &mut state);
    for _ in 0..400 {
        if !editor.is_saving() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        editor.poll_job(&ctx, &mut state);
    }
    assert!(!editor.is_saving());
    let saved = std::fs::read(&src).unwrap();
    let info = hp_vtf::read_info(&saved).unwrap();
    assert!(info.flags & hp_vtf::flags::CLAMPS != 0 && info.bumpmap_scale == 4.0);
    assert!(crate::matedit::backup_path(&src).is_file(), "a backup is kept");
    assert!(!editor.loaded.as_ref().unwrap().doc.is_dirty());

    editor.import_into_current(&mut state, &png);
    assert!(editor.loaded.as_ref().unwrap().doc.is_dirty());
    editor.ops.new_format = hp_vtf::ImageFormat::Dxt5;
    {
        let l = editor.loaded.as_mut().unwrap();
        l.doc.set_format(hp_vtf::ImageFormat::Dxt5).unwrap();
        l.doc.add_frame(None, hp_vtf::ops::ResizeFilter::Color).unwrap();
        l.rev += 1;
    }
    let all = dir.join("all.png");
    editor.export_all(&all);
    assert!(dir.join("all_f000.png").is_file() && dir.join("all_f001.png").is_file(), "{:?}", editor.status);
    let to = dir.join("copy.vtf");
    editor.save_to_file(&ctx, &mut state, to.clone());
    for _ in 0..400 {
        if !editor.is_saving() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        editor.poll_job(&ctx, &mut state);
    }
    let info = hp_vtf::read_info(&std::fs::read(&to).unwrap()).unwrap();
    assert_eq!((info.format, info.frames), (hp_vtf::ImageFormat::Dxt5, 2));
    pump(&mut editor, &mut state);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_textures_come_from_images_with_a_material() {
    let dir = crate::tests::scratch("vtfedit_new");
    let png = dir.join("brick.png");
    let g = gradient(30, 20);
    std::fs::write(&png, hp_vtf::ops::encode_image_file(&g, hp_vtf::ops::ImageFileFormat::Png).unwrap()).unwrap();
    let custom = dir.join("custom");
    std::fs::create_dir_all(&custom).unwrap();
    let mut state = state();
    crate::folders::add_custom_dir(&mut state.settings, &custom);
    let mut editor = VtfEditor::new();
    editor.new_from_image(&mut state, &png);
    assert!(matches!(editor.loaded.as_ref().unwrap().origin, Origin::New));
    assert!(editor.save.open && editor.save.name == "brick");
    let ctx = egui::Context::default();
    editor.save.name = "mytex/brick".to_owned();
    editor.start_save(&ctx, &mut state);
    for _ in 0..400 {
        if !editor.is_saving() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        editor.poll_job(&ctx, &mut state);
    }
    let layout = crate::folders::layout_of(&state, &state.settings.game.custom_texture_dirs[0].clone());
    let (vtf, name) = crate::folders::asset_target(&custom, &layout, "mytex/brick", "vtf");
    assert!(vtf.is_file(), "{:?}", editor.status);
    let vmt = std::fs::read_to_string(vtf.with_extension("vmt")).unwrap();
    assert!(vmt.contains("$basetexture") && vmt.contains(&name) && vmt.contains("$translucent"), "{vmt}");
    let info = hp_vtf::read_info(&std::fs::read(&vtf).unwrap()).unwrap();
    assert_eq!((info.width, info.height), (32, 16), "rounded to powers of two");
    assert!(matches!(&editor.loaded.as_ref().unwrap().origin, Origin::Texture(n) if *n == name));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn external_sessions_copy_back_and_reload() {
    let dir = crate::tests::scratch("vtfedit_session");
    let work = dir.join("work").join("a.vtf");
    let dest = dir.join("custom").join("a.vtf");
    std::fs::create_dir_all(work.parent().unwrap()).unwrap();
    let opts = hp_vtf::EncodeOptions::default();
    std::fs::write(&work, hp_vtf::encode(&gradient(16, 16), &opts).unwrap()).unwrap();
    let mut state = state();
    let mut editor = VtfEditor::new();
    let ctx = egui::Context::default();
    let mut s = external::Session::new("a", work.clone(), dest.clone(), vec!["m".to_owned()], "test");
    s.link = Some(("$basetexture".to_owned(), "prefix/a".to_owned()));
    editor.add_session(&ctx, s);
    assert_eq!(editor.session_count(), 1);
    let edited = hp_vtf::encode(&gradient(32, 32), &opts).unwrap();
    std::fs::write(&work, &edited).unwrap();
    for _ in 0..8 {
        std::thread::sleep(std::time::Duration::from_millis(600));
        editor.poll_sessions(&ctx, &mut state);
        if dest.is_file() {
            break;
        }
    }
    assert_eq!(std::fs::read(&dest).unwrap(), edited, "the edit was copied into the custom folder");
    assert_eq!(editor.sessions[0].synced, 1);
    assert!(editor.sessions[0].link.is_none(), "the link was posted once");
    assert_eq!(crate::shared::take_texture_link(&ctx).map(|l| l.texture), Some("prefix/a".to_owned()));

    std::fs::write(&work, b"not a vtf").unwrap();
    for _ in 0..8 {
        std::thread::sleep(std::time::Duration::from_millis(600));
        editor.poll_sessions(&ctx, &mut state);
        if editor.sessions[0].error.is_some() {
            break;
        }
    }
    assert!(editor.sessions[0].error.is_some(), "a broken file is reported, not copied");
    assert_eq!(std::fs::read(&dest).unwrap(), edited);
    editor.stop_session(0);
    assert_eq!(editor.session_count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
