use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use egui::{Color32, CornerRadius, Stroke, Visuals};

use crate::settings::ThemeChoice;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg0: Color32,
    pub bg1: Color32,
    pub bg2: Color32,
    pub bg3: Color32,
    pub bg4: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub selection: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    pub text_faint: Color32,
    pub strong: Color32,
    pub inverse: Color32,
}

const fn g(v: u8) -> Color32 {
    Color32::from_gray(v)
}

pub const DARK: Palette = Palette {
    bg0: g(0x0A),
    bg1: g(0x11),
    bg2: g(0x18),
    bg3: g(0x22),
    bg4: g(0x2E),
    border: g(0x2A),
    border_strong: g(0x4A),
    selection: g(0x46),
    text: g(0xEC),
    text_weak: g(0x99),
    text_faint: g(0x66),
    strong: g(0xFF),
    inverse: g(0x0A),
};

pub const LIGHT: Palette = Palette {
    bg0: g(0xFF),
    bg1: g(0xF4),
    bg2: g(0xFB),
    bg3: g(0xE8),
    bg4: g(0xDC),
    border: g(0xD8),
    border_strong: g(0xB0),
    selection: g(0xC4),
    text: g(0x14),
    text_weak: g(0x66),
    text_faint: g(0x99),
    strong: g(0x00),
    inverse: g(0xFF),
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

pub fn is_dark() -> bool {
    IS_DARK.load(Ordering::Relaxed)
}

pub fn palette() -> &'static Palette {
    if is_dark() { &DARK } else { &LIGHT }
}

pub fn palette_of(theme: ThemeChoice) -> &'static Palette {
    match theme {
        ThemeChoice::Dark => &DARK,
        ThemeChoice::Light => &LIGHT,
    }
}

static ACCENT_RGB: AtomicU32 = AtomicU32::new(0);

pub fn set_accent(color: Option<[u8; 3]>) {
    let packed = color.map_or(0, |[r, g, b]| 0x0100_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b));
    ACCENT_RGB.store(packed, Ordering::Relaxed);
}

pub fn accent_color() -> Option<Color32> {
    let v = ACCENT_RGB.load(Ordering::Relaxed);
    (v & 0x0100_0000 != 0).then(|| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

fn readable_on(c: Color32) -> Color32 {
    let l = 0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b());
    if l > 150.0 { g(0x0A) } else { g(0xFF) }
}

fn tinted_selection(p: &Palette, accent: Color32) -> Color32 {
    mix(p.selection, accent, 0.45)
}

pub fn accent() -> Color32 {
    accent_color().unwrap_or(palette().strong)
}

pub fn accent_bg() -> Color32 {
    match accent_color() {
        Some(a) => tinted_selection(palette(), a),
        None => palette().selection,
    }
}

pub fn on_accent() -> Color32 {
    match accent_color() {
        Some(a) => readable_on(a),
        None => palette().inverse,
    }
}

pub fn text() -> Color32 {
    palette().text
}

pub fn text_weak() -> Color32 {
    palette().text_weak
}

pub fn border() -> Color32 {
    palette().border
}

pub fn info() -> Color32 {
    palette().text_weak
}

pub fn success() -> Color32 {
    palette().text
}

pub fn warning() -> Color32 {
    palette().strong
}

pub fn error() -> Color32 {
    palette().strong
}

pub fn bar_fill(severity: crate::Severity) -> Color32 {
    let dark = is_dark();
    match severity {
        crate::Severity::Info | crate::Severity::Success => palette().selection,
        crate::Severity::Warning => g(if dark { 0x5E } else { 0xA6 }),
        crate::Severity::Error => g(if dark { 0x76 } else { 0x8C }),
    }
}

pub fn fraction_severity(fraction: f64, warn_at: f64) -> crate::Severity {
    if fraction > 1.0 {
        crate::Severity::Error
    } else if fraction >= warn_at {
        crate::Severity::Warning
    } else {
        crate::Severity::Success
    }
}

pub const ACCENT: Color32 = DARK.strong;
pub const ACCENT_BG: Color32 = DARK.selection;
pub const COLOR_INFO: Color32 = DARK.text_weak;
pub const COLOR_SUCCESS: Color32 = DARK.text;
pub const COLOR_WARNING: Color32 = DARK.strong;
pub const COLOR_ERROR: Color32 = DARK.strong;
pub const AXIS_X: Color32 = Color32::from_rgb(235, 80, 80);
pub const AXIS_Y: Color32 = Color32::from_rgb(110, 210, 90);
pub const AXIS_Z: Color32 = Color32::from_rgb(80, 140, 255);

pub const VIEW_ACCENT: Color32 = Color32::from_rgb(255, 140, 40);
pub const VIEW_ACCENT_BG: Color32 = Color32::from_rgb(110, 62, 22);
pub const VIEW_SUCCESS: Color32 = Color32::from_rgb(110, 210, 120);
pub const VIEW_WARNING: Color32 = Color32::from_rgb(240, 200, 80);
pub const VIEW_ERROR: Color32 = Color32::from_rgb(255, 95, 90);

pub const RADIUS: u8 = 4;
pub const WINDOW_RADIUS: u8 = 8;
pub const MENU_RADIUS: u8 = 6;

pub fn visuals(theme: ThemeChoice) -> Visuals {
    visuals_with(theme, accent_color())
}

pub fn visuals_with(theme: ThemeChoice, accent: Option<Color32>) -> Visuals {
    let mut v = gray_visuals(theme);
    if let Some(a) = accent {
        let p = palette_of(theme);
        let sel = tinted_selection(p, a);
        v.selection.bg_fill = sel;
        v.selection.stroke = Stroke::new(1.0, a);
        v.hyperlink_color = a;
        v.text_cursor.stroke = Stroke::new(2.0, a);
        v.ime_composition.active_underline_stroke = Stroke::new(2.0, a);
        v.widgets.active.bg_fill = sel;
        v.widgets.active.weak_bg_fill = sel;
        v.widgets.active.bg_stroke = Stroke::new(1.0, a);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, mix(p.border_strong, a, 0.6));
        v.widgets.open.bg_stroke = Stroke::new(1.0, a);
    }
    v
}

fn gray_visuals(theme: ThemeChoice) -> Visuals {
    let p = palette_of(theme);
    let dark = theme == ThemeChoice::Dark;
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    v.dark_mode = dark;
    v.override_text_color = None;
    v.weak_text_color = Some(p.text_weak);
    v.panel_fill = p.bg1;
    v.window_fill = p.bg2;
    v.faint_bg_color = if dark { g(0x15) } else { g(0xF0) };
    v.extreme_bg_color = p.bg0;
    v.text_edit_bg_color = Some(p.bg0);
    v.code_bg_color = p.bg0;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(WINDOW_RADIUS);
    v.menu_corner_radius = CornerRadius::same(MENU_RADIUS);
    let shadow_alpha = if dark { 140 } else { 40 };
    v.window_shadow =
        egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(shadow_alpha) };
    v.popup_shadow =
        egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(shadow_alpha) };
    v.hyperlink_color = p.strong;
    v.warn_fg_color = p.strong;
    v.error_fg_color = p.strong;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.strong);
    v.text_cursor.stroke = Stroke::new(2.0, p.strong);
    v.ime_composition.active_underline_stroke = Stroke::new(2.0, p.strong);
    v.ime_composition.inactive_underline_stroke = Stroke::new(1.0, p.text_weak);
    v.striped = false;
    v.text_options.font_hinting = true;
    v.text_options.subpixel_binning = false;
    v.slider_trailing_fill = true;

    let r = CornerRadius::same(RADIUS);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.bg1;
    w.noninteractive.weak_bg_fill = p.bg1;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = p.bg3;
    w.inactive.weak_bg_fill = p.bg3;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = p.bg4;
    w.hovered.weak_bg_fill = p.bg4;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.5, p.strong);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.selection;
    w.active.weak_bg_fill = p.selection;
    w.active.bg_stroke = Stroke::new(1.0, p.strong);
    w.active.fg_stroke = Stroke::new(2.0, p.strong);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.bg4;
    w.open.weak_bg_fill = p.bg4;
    w.open.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.open.fg_stroke = Stroke::new(1.0, p.strong);
    w.open.corner_radius = r;
    v
}

pub fn apply(ctx: &egui::Context, theme: ThemeChoice, ui_scale: f32) {
    IS_DARK.store(theme == ThemeChoice::Dark, Ordering::Relaxed);
    let visuals = visuals(theme);
    ctx.set_theme(match theme {
        ThemeChoice::Dark => egui::Theme::Dark,
        ThemeChoice::Light => egui::Theme::Light,
    });
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(7.0, 3.0);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.interact_size.y = 22.0;
        style.spacing.scroll.floating = true;
        style.spacing.scroll.bar_width = 8.0;
        style.spacing.scroll.floating_allocated_width = 0.0;
        use egui::{FontFamily::Monospace, FontFamily::Proportional, FontId, TextStyle};
        style.text_styles = [
            (TextStyle::Small, FontId::new(12.0, Proportional)),
            (TextStyle::Body, FontId::new(14.0, Proportional)),
            (TextStyle::Button, FontId::new(14.0, Proportional)),
            (TextStyle::Heading, FontId::new(19.0, Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, Monospace)),
        ]
        .into();
    });
    ctx.set_zoom_factor(ui_scale.clamp(0.5, 3.0));
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn is_gray(c: Color32) -> bool {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    r == g && g == b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visuals_colors(v: &Visuals) -> Vec<(&'static str, Color32)> {
        let mut out = vec![
            ("panel_fill", v.panel_fill),
            ("window_fill", v.window_fill),
            ("faint_bg_color", v.faint_bg_color),
            ("extreme_bg_color", v.extreme_bg_color),
            ("code_bg_color", v.code_bg_color),
            ("window_stroke", v.window_stroke.color),
            ("hyperlink_color", v.hyperlink_color),
            ("warn_fg_color", v.warn_fg_color),
            ("error_fg_color", v.error_fg_color),
            ("selection.bg_fill", v.selection.bg_fill),
            ("selection.stroke", v.selection.stroke.color),
            ("text_cursor", v.text_cursor.stroke.color),
            ("ime_active", v.ime_composition.active_underline_stroke.color),
            ("ime_inactive", v.ime_composition.inactive_underline_stroke.color),
            ("window_shadow", v.window_shadow.color),
            ("popup_shadow", v.popup_shadow.color),
            ("text_edit_bg", v.text_edit_bg_color()),
            ("weak_text", v.weak_text_color()),
            ("strong_text", v.strong_text_color()),
        ];
        let w = &v.widgets;
        for (name, wv) in [
            ("noninteractive", &w.noninteractive),
            ("inactive", &w.inactive),
            ("hovered", &w.hovered),
            ("active", &w.active),
            ("open", &w.open),
        ] {
            out.push((name, wv.bg_fill));
            out.push((name, wv.weak_bg_fill));
            out.push((name, wv.bg_stroke.color));
            out.push((name, wv.fg_stroke.color));
        }
        out
    }

    #[test]
    fn both_themes_are_black_and_white_only() {
        for theme in [ThemeChoice::Dark, ThemeChoice::Light] {
            let v = visuals(theme);
            for (name, c) in visuals_colors(&v) {
                assert!(is_gray(c), "{theme:?} {name} has a hue: {c:?}");
            }
            let p = palette_of(theme);
            for c in [
                p.bg0,
                p.bg1,
                p.bg2,
                p.bg3,
                p.bg4,
                p.border,
                p.border_strong,
                p.selection,
                p.text,
                p.text_weak,
                p.text_faint,
                p.strong,
                p.inverse,
            ] {
                assert!(is_gray(c), "{theme:?} palette has a hue: {c:?}");
            }
        }
        for c in [ACCENT, ACCENT_BG, COLOR_INFO, COLOR_SUCCESS, COLOR_WARNING, COLOR_ERROR] {
            assert!(is_gray(c));
        }
    }

    fn luma(c: Color32) -> i32 {
        i32::from(c.r())
    }

    #[test]
    fn light_mirrors_dark() {
        assert!(luma(DARK.bg0) < luma(DARK.bg1) && luma(DARK.bg1) < luma(DARK.bg3) && luma(DARK.bg3) < luma(DARK.bg4));
        assert!(
            luma(LIGHT.bg0) > luma(LIGHT.bg1) && luma(LIGHT.bg1) > luma(LIGHT.bg3) && luma(LIGHT.bg3) > luma(LIGHT.bg4)
        );
        for p in [&DARK, &LIGHT] {
            assert!((luma(p.text) - luma(p.bg1)).abs() > 180, "primary text contrast");
            assert!((luma(p.text_weak) - luma(p.bg1)).abs() > 70, "secondary text contrast");
            assert!((luma(p.strong) - luma(p.inverse)).abs() > 240, "inverted fills");
            assert!((luma(p.selection) - luma(p.bg4)).abs() >= 20, "selected differs from hovered");
            assert!((luma(p.border) - luma(p.bg1)).abs() >= 20, "hairlines show on panels");
        }
    }

    const UI_CRATES: [&str; 11] = [
        "hp-app",
        "hp-ui-shell",
        "hp-ui-textures",
        "hp-ui-entities",
        "hp-ui-mapping",
        "hp-ui-lab",
        "hp-ui-viewports",
        "hp-tools-create",
        "hp-tools-mesh",
        "hp-tools-select",
        "hammerpro",
    ];

    const CONTENT_FILES: [&str; 10] = [
        "hp-app/src/theme.rs",
        "hp-tools-create/src/common.rs",
        "hp-tools-create/src/cordon.rs",
        "hp-tools-create/src/overlay_tool.rs",
        "hp-tools-create/src/sprinkle.rs",
        "hp-tools-mesh/src/disp/",
        "hp-tools-mesh/src/vertex/",
        "hp-tools-select/src/select/",
        "hp-ui-lab/src/logic/",
        "hp-ui-viewports/src/layers/",
    ];

    const CONTENT_LINES: [(&str, &str); 8] = [
        ("hp-app/src/chrome.rs", "Color32::from_rgb(0xE8, 0x11, 0x23)"),
        ("hp-app/src/chrome.rs", "Color32::from_rgb(0xF1, 0x70, 0x7A)"),
        ("hp-ui-textures/src/browser.rs", "Color32::from_rgb(200, 0, 200)"),
        ("hp-ui-textures/src/browser.rs", "Color32::from_rgba_unmultiplied(120, 60, 170, 225)"),
        ("hp-ui-textures/src/uv_editor.rs", "Color32::from_rgb(80, 200, 255)"),
        ("hp-ui-textures/src/uv_editor.rs", "Color32::from_rgb(255, 210, 60)"),
        ("hp-ui-viewports/src/lib.rs", "Color32::from_rgb(190, 230, 26)"),
        ("hp-ui-viewports/src/lib.rs", "Color32::from_rgb(255, 107, 10)"),
    ];

    const VIEW_CRATES: [&str; 5] = ["hp-app", "hp-ui-viewports", "hp-tools-create", "hp-tools-mesh", "hp-tools-select"];

    fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    fn hued_literals(line: &str) -> Vec<String> {
        let mut found = Vec::new();
        for ctor in ["from_rgb(", "from_rgba_unmultiplied(", "from_rgba_premultiplied(", "from_rgb_additive("] {
            let needle = format!("Color32::{ctor}");
            let mut rest = line;
            while let Some(i) = rest.find(&needle) {
                rest = &rest[i + needle.len()..];
                let args: Vec<Option<u32>> =
                    rest.split([',', ')']).take(3).map(|a| a.trim().trim_end_matches("u8").parse().ok()).collect();
                if let [Some(r), Some(g), Some(b)] = args[..]
                    && !(r == g && g == b)
                {
                    found.push(format!("{needle}{r}, {g}, {b}"));
                }
            }
        }
        for name in [
            "RED",
            "GREEN",
            "BLUE",
            "YELLOW",
            "ORANGE",
            "GOLD",
            "KHAKI",
            "BROWN",
            "LIGHT_RED",
            "LIGHT_GREEN",
            "LIGHT_BLUE",
            "LIGHT_YELLOW",
            "DARK_RED",
            "DARK_GREEN",
            "DARK_BLUE",
            "MAGENTA",
            "CYAN",
            "PURPLE",
        ] {
            let needle = format!("Color32::{name}");
            if let Some(i) = line.find(&needle) {
                let after = line[i + needle.len()..].chars().next();
                if !after.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    found.push(needle);
                }
            }
        }
        found
    }

    #[test]
    fn hued_literals_are_found() {
        assert_eq!(hued_literals("let c = Color32::from_rgb(255, 140, 40);").len(), 1);
        assert!(
            hued_literals("Color32::from_rgb(30, 30, 30), Color32::from_rgba_premultiplied(10, 10, 10, 190)")
                .is_empty()
        );
        assert!(hued_literals("Color32::from_rgb(c[0], c[1], c[2])").is_empty(), "dynamic content colors");
        assert_eq!(hued_literals("x(Color32::RED)").len(), 1);
        assert!(hued_literals("Color32::REDDISH_NAME").is_empty());
    }

    #[test]
    fn ui_crates_hold_no_hued_chrome_colors() {
        let crates_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut problems = Vec::new();
        for krate in UI_CRATES {
            let mut files = Vec::new();
            rs_files(&crates_dir.join(krate).join("src"), &mut files);
            assert!(!files.is_empty(), "{krate} has sources");
            for file in files {
                let rel = file.strip_prefix(&crates_dir).unwrap_or(&file).to_string_lossy().replace('\\', "/");
                let content = rel.ends_with("/data_colors.rs")
                    || CONTENT_FILES.iter().any(|c| if c.ends_with('/') { rel.starts_with(c) } else { rel == *c });
                let Ok(text) = std::fs::read_to_string(&file) else { continue };
                for (n, line) in text.lines().enumerate() {
                    if line.trim_start().starts_with("#[cfg(test)]") {
                        break;
                    }
                    if line.trim_start().starts_with("//") {
                        continue;
                    }
                    if !content {
                        for lit in hued_literals(line) {
                            if !CONTENT_LINES.iter().any(|(f, l)| *f == rel && line.contains(l)) {
                                problems.push(format!("{rel}:{}: hued color {lit} (use hp_app::theme tokens)", n + 1));
                            }
                        }
                    }
                    if !VIEW_CRATES.contains(&krate) {
                        for token in ["theme::VIEW_", "theme::AXIS_"] {
                            if line.contains(token) {
                                problems.push(format!("{rel}:{}: {token}* is for drawing inside the views", n + 1));
                            }
                        }
                    }
                    if krate != "hp-ui-viewports" && krate != "hp-app" && !content {
                        for c in ["ACCENT", "ACCENT_BG", "COLOR_INFO", "COLOR_SUCCESS", "COLOR_WARNING", "COLOR_ERROR"]
                        {
                            let imported = line.contains("theme::{")
                                && line.split(|ch: char| !(ch.is_alphanumeric() || ch == '_')).any(|w| w == c);
                            if imported {
                                problems.push(format!("{rel}:{}: imports theme::{c}, use the token function", n + 1));
                            }
                            let needle = format!("theme::{c}");
                            if let Some(i) = line.find(&needle) {
                                let after = line[i + needle.len()..].chars().next();
                                if !after.is_some_and(|ch| ch.is_alphanumeric() || ch == '_') {
                                    problems.push(format!(
                                        "{rel}:{}: {needle} is the dark theme value, use the token function",
                                        n + 1
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(problems.is_empty(), "UI chrome must be black and white:\n{}", problems.join("\n"));
    }

    #[test]
    fn data_colors_live_in_one_file() {
        let crates_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut found = Vec::new();
        for krate in UI_CRATES {
            let mut files = Vec::new();
            rs_files(&crates_dir.join(krate).join("src"), &mut files);
            for file in files {
                let rel = file.strip_prefix(&crates_dir).unwrap_or(&file).to_string_lossy().replace('\\', "/");
                if rel.ends_with("/data_colors.rs") {
                    found.push(rel);
                }
            }
        }
        assert_eq!(found, vec!["hp-app/src/data_colors.rs".to_owned()]);
    }

    #[test]
    fn data_colors_are_hued_where_they_carry_meaning() {
        use crate::data_colors as d;
        for c in [d::PIN_COLOR, d::PIN_FLOAT, d::PIN_UV, d::CHANNEL_RED, d::CHANNEL_GREEN, d::CHANNEL_BLUE] {
            assert!(!is_gray(c));
        }
        assert!(is_gray(d::CHANNEL_MASTER) && is_gray(d::NODE_OTHER));
    }

    #[test]
    fn tokens_follow_the_applied_theme() {
        let _guard = crate::testing::lang_lock();
        let ctx = egui::Context::default();
        apply(&ctx, ThemeChoice::Light, 1.0);
        assert_eq!(accent(), Color32::BLACK);
        assert_eq!(on_accent(), Color32::WHITE);
        assert!(!ctx.global_style().visuals.dark_mode);
        apply(&ctx, ThemeChoice::Dark, 1.0);
        assert_eq!(accent(), Color32::WHITE);
        assert_eq!(error(), Color32::WHITE);
        assert_eq!(text_weak(), DARK.text_weak);
        assert!(ctx.global_style().visuals.dark_mode);
    }

    #[test]
    fn explicit_theme_survives_opposite_system_theme_on_first_frame() {
        let _guard = crate::testing::lang_lock();
        for (choice, system, dark) in
            [(ThemeChoice::Dark, egui::Theme::Light, true), (ThemeChoice::Light, egui::Theme::Dark, false)]
        {
            let ctx = egui::Context::default();
            ctx.set_theme(egui::ThemePreference::System);
            apply(&ctx, choice, 1.0);
            for _ in 0..2 {
                let _ = ctx.run_ui(egui::RawInput { system_theme: Some(system), ..Default::default() }, |ui| {
                    assert_eq!(ui.visuals().dark_mode, dark);
                    assert_eq!(ui.visuals().panel_fill, palette_of(choice).bg1);
                });
            }
        }
    }
}
