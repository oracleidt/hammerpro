use egui::{Rect, vec2};
use glam::DVec3;
use hp_app::AppState;
use hp_app::extend::{StatusFit, status_priority};
use hp_app::i18n::Lang;

use crate::Shell;
use crate::status::{SEGMENT_GAP, fit_widths};
use crate::tests::lang_guard;

fn state() -> AppState {
    let s = hp_app::testing::test_state();
    crate::register_translations();
    s
}

struct Texts {
    texts: Vec<(String, Rect)>,
    width: f32,
}

fn frame(ctx: &egui::Context, size: egui::Vec2, state: &mut AppState, shell: &mut Shell) -> Texts {
    let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
    let mut width = 0.0;
    let mut out = ctx.run_ui(input, |ui| {
        width = ui.ctx().content_rect().width();
        egui::Panel::bottom("bottom").show(ui, |ui| shell.status_bar(ui, state, Some("Drag to create a block".into())));
    });
    out.textures_delta.clear();
    let mut texts = Vec::new();
    if let Some(update) = out.platform_output.accesskit_update {
        for (_, node) in update.nodes {
            let text = [node.label(), node.value()].into_iter().flatten().collect::<Vec<_>>().join(" ");
            if let Some(b) = node.bounds()
                && !text.trim().is_empty()
            {
                let r = Rect::from_min_max(egui::pos2(b.x0 as f32, b.y0 as f32), egui::pos2(b.x1 as f32, b.y1 as f32));
                texts.push((text.trim().to_owned(), r));
            }
        }
    }
    Texts { texts, width }
}

fn check_layout(shell: &mut Shell, texts: &Texts, width: f32, label: &str) {
    let placed = shell.status().placed().to_vec();
    for (k, (id, r)) in placed.iter().enumerate() {
        assert!(r.left() >= -0.5 && r.right() <= width + 0.5, "{label}: {id} {r:?} leaves the window");
        for (other, q) in &placed[k + 1..] {
            assert!(r.right() <= q.left() + 0.01, "{label}: {id} {r:?} overlaps {other} {q:?}");
        }
    }
    if let Some((_, cursor)) = placed.iter().find(|(id, _)| *id == "shell_cursor") {
        let text = texts.texts.iter().find(|(t, _)| t.starts_with('⌖')).expect("cursor text");
        assert!(
            text.1.left() >= cursor.left() - 0.5 && text.1.right() <= cursor.right() + 0.5,
            "{label}: cursor text {:?} spills out of {cursor:?}",
            text.1
        );
        for (other, q) in &placed {
            if *other != "shell_cursor" {
                assert!(text.1.right() <= q.left() + 0.5 || text.1.left() >= q.right() - 0.5, "{label}: over {other}");
            }
        }
    }
}

#[test]
fn widths_fit_the_space_and_keep_their_preference() {
    let hint = Some(StatusFit::flex(60.0).with_priority(status_priority::HINT));
    let sel = Some(StatusFit::shrinkable(120.0, 340.0).with_priority(status_priority::SELECTION));
    let cur = Some(StatusFit::shrinkable(190.0, 200.0).with_priority(status_priority::CURSOR));
    let w = fit_widths(1000.0, 10.0, &[hint, sel, cur]);
    assert_eq!(w, vec![Some(1000.0 - 20.0 - 340.0 - 200.0), Some(340.0), Some(200.0)]);
    let w = fit_widths(500.0, 10.0, &[hint, sel, cur]);
    let [h, s, c] = [w[0].unwrap(), w[1].unwrap(), w[2].unwrap()];
    assert_eq!(h, 60.0);
    assert!((h + s + c + 20.0 - 500.0).abs() < 1e-3, "{w:?}");
    assert!((120.0..340.0).contains(&s) && (190.0..200.0).contains(&c));
}

#[test]
fn low_priority_segments_drop_first() {
    let hint = Some(StatusFit::flex(60.0).with_priority(status_priority::HINT));
    let dims = Some(StatusFit::shrinkable(90.0, 150.0).with_priority(status_priority::REAL_DIMS));
    let sel = Some(StatusFit::shrinkable(120.0, 340.0).with_priority(status_priority::SELECTION));
    let cur = Some(StatusFit::shrinkable(190.0, 200.0).with_priority(status_priority::CURSOR));
    let all = [hint, sel, dims, cur];
    let w = fit_widths(400.0, 10.0, &all);
    assert!(w[2].is_none(), "real dimensions go first: {w:?}");
    assert!(w[0].is_some() && w[1].is_some() && w[3].is_some(), "{w:?}");
    let w = fit_widths(330.0, 10.0, &all);
    assert_eq!((w[0], w[2]), (None, None), "then the hint: {w:?}");
    assert!(w[1].is_some() && w[3].is_some());
    let w = fit_widths(250.0, 10.0, &all);
    assert_eq!(w, vec![None, None, None, Some(200.0)], "the cursor stays longest, at its reserve");
    assert_eq!(fit_widths(100.0, 10.0, &all), vec![None; 4]);
    assert_eq!(fit_widths(-5.0, 10.0, &all), vec![None; 4]);
    assert_eq!(fit_widths(f32::NAN, 10.0, &all), vec![None; 4]);
    let w = fit_widths(500.0, 10.0, &[None, cur, None]);
    assert_eq!(w, vec![None, Some(200.0), None], "hidden segments take no gap");
}

#[test]
fn widths_never_overflow() {
    let fits = [
        Some(StatusFit::flex(60.0).with_priority(status_priority::HINT)),
        Some(StatusFit::shrinkable(120.0, 340.0).with_priority(status_priority::SELECTION)),
        Some(StatusFit::shrinkable(90.0, 180.0).with_priority(status_priority::REAL_DIMS)),
        Some(StatusFit::shrinkable(230.0, 240.0).with_priority(status_priority::CURSOR)),
    ];
    let mut avail = 0.0;
    while avail < 2000.0 {
        let w = fit_widths(avail, SEGMENT_GAP, &fits);
        let shown: Vec<f32> = w.iter().flatten().copied().collect();
        let used = shown.iter().sum::<f32>() + SEGMENT_GAP * shown.len().saturating_sub(1) as f32;
        assert!(used <= avail + 1e-3, "{avail}: {w:?}");
        for (f, w) in fits.iter().zip(&w) {
            if let (Some(f), Some(w)) = (f, w) {
                assert!(*w >= f.min - 1e-3, "{avail}: below the minimum {w} < {}", f.min);
            }
        }
        avail += 7.0;
    }
}

#[test]
fn long_coordinates_never_overlap_at_any_width() {
    let _g = lang_guard();
    let mut state = state();
    state.doc.select_all();
    for lang in [Lang::En, Lang::Fr, Lang::Ru] {
        state.settings.language = lang;
        hp_app::i18n::set_lang(lang);
        for scale in [1.0, 1.25, 1.5] {
            for size in [vec2(1600.0, 900.0), vec2(1280.0, 720.0), vec2(1024.0, 700.0), vec2(700.0, 500.0)] {
                for p in [
                    DVec3::new(9536.06, -8797.23, 2547.48),
                    DVec3::new(-15536.5, -16384.25, -12000.75),
                    DVec3::new(1.0, 2.0, 3.0),
                ] {
                    let ctx = egui::Context::default();
                    ctx.enable_accesskit();
                    hp_app::theme::apply(&ctx, hp_app::settings::ThemeChoice::Dark, scale);
                    let mut shell = Shell::new();
                    state.hover_world = Some(p);
                    let mut texts = frame(&ctx, size, &mut state, &mut shell);
                    for _ in 0..2 {
                        texts = frame(&ctx, size, &mut state, &mut shell);
                    }
                    let label = format!("{lang:?} x{scale} {size:?} {p:?}");
                    check_layout(&mut shell, &texts, texts.width, &label);
                    let placed = shell.status().placed();
                    if size.x >= 1024.0 && scale <= 1.25 {
                        assert!(placed.iter().any(|(id, _)| *id == "shell_cursor"), "{label}: cursor dropped");
                    }
                }
            }
        }
    }
    hp_app::i18n::set_lang(Lang::En);
}

#[test]
fn the_cursor_keeps_its_widest_reserve() {
    let _g = lang_guard();
    let mut state = state();
    hp_app::i18n::set_lang(Lang::En);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut shell = Shell::new();
    let size = vec2(1600.0, 900.0);
    state.hover_world = Some(DVec3::new(1.0, 2.0, 3.0));
    frame(&ctx, size, &mut state, &mut shell);
    let width = |shell: &mut Shell| {
        shell.status().placed().iter().find(|(id, _)| *id == "shell_cursor").map(|(_, r)| r.width()).unwrap_or(0.0)
    };
    let small = width(&mut shell);
    assert!(small >= crate::status::CURSOR_WIDTH - 0.5, "{small}");
    state.hover_world = Some(DVec3::new(-155360000.55, -163840000.25, -120000000.75));
    frame(&ctx, size, &mut state, &mut shell);
    let wide = width(&mut shell);
    assert!(wide > small, "{wide} > {small}");
    state.hover_world = Some(DVec3::new(1.0, 2.0, 3.0));
    frame(&ctx, size, &mut state, &mut shell);
    assert_eq!(width(&mut shell), wide, "no jumping back while the mouse moves");
}
