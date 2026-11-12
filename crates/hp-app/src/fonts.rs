use std::path::PathBuf;
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontTweak};

const UI_FONT: &str = "hp-ui";
const SYMBOL_FONT: &str = "hp-symbols";
const ICON_FONT: &str = "hp-icons";
const ICON_FONT_MONO: &str = "hp-icons-mono";
const ICON_BYTES: &[u8] = include_bytes!("../../../assets/fonts/hammerpro-icons.ttf");

struct Candidate {
    files: &'static [&'static str],
}

#[cfg(windows)]
const UI_CANDIDATES: Candidate = Candidate { files: &["segoeui.ttf"] };
#[cfg(windows)]
const SYMBOL_CANDIDATES: Candidate = Candidate { files: &["seguisym.ttf"] };

#[cfg(target_os = "macos")]
const UI_CANDIDATES: Candidate = Candidate { files: &["SFNS.ttf", "Helvetica.ttc"] };
#[cfg(target_os = "macos")]
const SYMBOL_CANDIDATES: Candidate = Candidate { files: &["Apple Symbols.ttf"] };

#[cfg(not(any(windows, target_os = "macos")))]
const UI_CANDIDATES: Candidate = Candidate { files: &["truetype/dejavu/DejaVuSans.ttf", "TTF/DejaVuSans.ttf"] };
#[cfg(not(any(windows, target_os = "macos")))]
const SYMBOL_CANDIDATES: Candidate = Candidate { files: &["truetype/dejavu/DejaVuSans.ttf", "TTF/DejaVuSans.ttf"] };

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("windir"));
        dirs.push(root.map_or_else(|| PathBuf::from("C:\\Windows"), PathBuf::from).join("Fonts"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
    }
    dirs
}

fn load(candidate: &Candidate, dirs: &[PathBuf]) -> Option<Vec<u8>> {
    dirs.iter().flat_map(|d| candidate.files.iter().map(move |f| d.join(f))).find_map(|p| std::fs::read(p).ok())
}

fn crisp() -> FontTweak {
    FontTweak { subpixel_binning: Some(false), hinting: Some(true), ..FontTweak::default() }
}

pub fn definitions_with(ui: Option<Vec<u8>>, symbols: Option<Vec<u8>>) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    for font in defs.font_data.values_mut() {
        let tweak = FontTweak { subpixel_binning: Some(false), ..font.tweak.clone() };
        *font = Arc::new(FontData::clone(font).tweak(tweak));
    }
    if let Some(bytes) = symbols {
        defs.font_data.insert(SYMBOL_FONT.to_owned(), Arc::new(FontData::from_owned(bytes).tweak(crisp())));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            if let Some(list) = defs.families.get_mut(&family) {
                let at = list.iter().position(|n| n == "egui-icons").unwrap_or(list.len());
                list.insert(at, SYMBOL_FONT.to_owned());
            }
        }
    }
    if let Some(bytes) = ui {
        defs.font_data.insert(UI_FONT.to_owned(), Arc::new(FontData::from_owned(bytes).tweak(crisp())));
        if let Some(list) = defs.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, UI_FONT.to_owned());
            if let Some(at) = list.iter().position(|n| n == SYMBOL_FONT) {
                let symbols = list.remove(at);
                list.insert(1, symbols);
            }
        }
    }
    for (family, name) in [(FontFamily::Proportional, ICON_FONT), (FontFamily::Monospace, ICON_FONT_MONO)] {
        let text = defs.families.get(&family).and_then(|l| l.first()).and_then(|n| defs.font_data.get(n)).cloned();
        let icons = text
            .and_then(|t| row_metrics(t.font.as_ref().as_ref(), t.index).map(|m| m.scaled(t.tweak.scale)))
            .and_then(|m| icons_fitting(ICON_BYTES, m));
        let data = icons.map_or_else(|| FontData::from_static(ICON_BYTES), FontData::from_owned);
        defs.font_data.insert(name.to_owned(), Arc::new(data.tweak(crisp())));
        defs.families.entry(family).or_default().insert(0, name.to_owned());
    }
    defs
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RowMetrics {
    ascent: f32,
    descent: f32,
    gap: f32,
}

impl RowMetrics {
    fn height(self) -> f32 {
        self.ascent - self.descent + self.gap
    }

    fn scaled(self, s: f32) -> Self {
        Self { ascent: self.ascent * s, descent: self.descent * s, gap: self.gap * s }
    }
}

fn be_u16(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes([s[0], s[1]]))
}

fn be_i16(b: &[u8], at: usize) -> Option<i16> {
    be_u16(b, at).map(|v| v as i16)
}

fn be_u32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn table(b: &[u8], index: u32, tag: &[u8; 4]) -> Option<usize> {
    let dir = if b.get(0..4) == Some(b"ttcf") { be_u32(b, 12 + 4 * index as usize)? as usize } else { 0 };
    let count = be_u16(b, dir + 4)? as usize;
    (0..count)
        .map(|i| dir + 12 + 16 * i)
        .find(|&r| b.get(r..r + 4) == Some(tag))
        .and_then(|r| be_u32(b, r + 8))
        .map(|o| o as usize)
}

fn row_metrics(b: &[u8], index: u32) -> Option<RowMetrics> {
    let upm = f32::from(be_u16(b, table(b, index, b"head")? + 18)?.max(1));
    let hhea = table(b, index, b"hhea")?;
    let os2 = table(b, index, b"OS/2");
    let typo = os2.and_then(|o| Some((be_u16(b, o + 62)?, be_i16(b, o + 68)?, be_i16(b, o + 70)?, be_i16(b, o + 72)?)));
    let win = os2.and_then(|o| Some((be_u16(b, o + 74)?, be_u16(b, o + 76)?)));
    let hhea = (be_i16(b, hhea + 4)?, be_i16(b, hhea + 6)?, be_i16(b, hhea + 8)?);
    let (a, d, g) = match (typo, win) {
        (Some((sel, a, d, g)), _) if sel & 0x80 != 0 => (f32::from(a), f32::from(d), f32::from(g)),
        (Some((_, a, d, g)), _) if hhea.0 == 0 && hhea.1 == 0 && (a != 0 || d != 0) => {
            (f32::from(a), f32::from(d), f32::from(g))
        }
        (_, Some((wa, wd))) if hhea.0 == 0 && hhea.1 == 0 => (f32::from(wa), -f32::from(wd), 0.0),
        _ => (f32::from(hhea.0), f32::from(hhea.1), f32::from(hhea.2)),
    };
    Some(RowMetrics { ascent: a / upm, descent: d / upm, gap: g / upm })
}

fn icons_fitting(icons: &[u8], text: RowMetrics) -> Option<Vec<u8>> {
    let own = row_metrics(icons, 0)?;
    if text.height() <= own.height() {
        return None;
    }
    let upm = f32::from(be_u16(icons, table(icons, 0, b"head")? + 18)?.max(1));
    let pad = (text.height() - own.height()) * 0.5 + 0.004;
    let ascent = ((own.ascent + pad) * upm).ceil().clamp(0.0, f32::from(i16::MAX)) as i16;
    let descent = ((own.descent - pad) * upm).floor().clamp(f32::from(i16::MIN), 0.0) as i16;
    let gap = (own.gap * upm).round() as i16;
    let mut out = icons.to_vec();
    let hhea = table(icons, 0, b"hhea")?;
    for (at, v) in [(hhea + 4, ascent), (hhea + 6, descent), (hhea + 8, gap)] {
        out.get_mut(at..at + 2)?.copy_from_slice(&v.to_be_bytes());
    }
    if let Some(os2) = table(icons, 0, b"OS/2") {
        for (at, v) in [(os2 + 68, ascent), (os2 + 70, descent), (os2 + 72, gap)] {
            out.get_mut(at..at + 2)?.copy_from_slice(&v.to_be_bytes());
        }
        for (at, v) in [(os2 + 74, ascent.unsigned_abs()), (os2 + 76, descent.unsigned_abs())] {
            out.get_mut(at..at + 2)?.copy_from_slice(&v.to_be_bytes());
        }
    }
    Some(out)
}

pub fn install(ctx: &egui::Context) {
    let dirs = font_dirs();
    let ui = load(&UI_CANDIDATES, &dirs);
    let symbols = load(&SYMBOL_CANDIDATES, &dirs);
    ctx.set_fonts(definitions_with(ui, symbols));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_fonts_can_render_the_russian_interface() {
        let ctx = egui::Context::default();
        ctx.set_fonts(definitions_with(None, None));
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.ctx().fonts_mut(|fonts| {
                for font in [egui::FontId::proportional(14.0), egui::FontId::monospace(13.0)] {
                    assert!(fonts.has_glyphs(&font, "Русский Язык Ёё Выделение Текстуры"));
                }
            });
        });
    }

    #[test]
    fn without_system_fonts_the_defaults_stay() {
        let defs = definitions_with(None, None);
        assert!(!defs.font_data.contains_key(UI_FONT));
        assert!(defs.families[&FontFamily::Proportional].iter().any(|n| n == "Ubuntu-Light"));
    }

    #[test]
    fn the_ui_font_leads_and_symbols_follow() {
        let defs = definitions_with(Some(vec![0]), Some(vec![0]));
        let list = &defs.families[&FontFamily::Proportional];
        assert_eq!(list[0], ICON_FONT);
        assert_eq!(list[1], UI_FONT);
        assert_eq!(list[2], SYMBOL_FONT);
        let mono = &defs.families[&FontFamily::Monospace];
        assert_eq!(mono[0], ICON_FONT_MONO);
        assert_eq!(mono[1], "Hack");
        assert!(mono.iter().any(|n| n == SYMBOL_FONT));
    }

    fn font_sets() -> Vec<(&'static str, FontDefinitions)> {
        let dirs = font_dirs();
        let mut sets = vec![("bundled", definitions_with(None, None))];
        if let Some(ui) = load(&UI_CANDIDATES, &dirs) {
            sets.push(("system", definitions_with(Some(ui), load(&SYMBOL_CANDIDATES, &dirs))));
        }
        sets
    }

    const SAMPLES: [&str; 4] = ["gjpqy Backgrounds.png", "Выделение руды ф у д р", "Ég ÊÀ çjp", "💾 ⚙ 🔍 Qg"];

    #[test]
    fn rows_are_as_tall_as_the_text_font() {
        for (name, defs) in font_sets() {
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                let list = &defs.families[&family];
                let text = defs.font_data.get(&list[1]).expect("text font");
                let icons = defs.font_data.get(&list[0]).expect("icon font");
                let want = row_metrics(text.font.as_ref().as_ref(), text.index).unwrap().scaled(text.tweak.scale);
                let have = row_metrics(icons.font.as_ref().as_ref(), icons.index).unwrap();
                assert!(have.height() >= want.height(), "{name} {family:?}: {have:?} < {want:?}");
                let own = row_metrics(ICON_BYTES, 0).unwrap();
                let centre = |m: RowMetrics| (m.ascent + m.descent) * 0.5;
                assert!((centre(have) - centre(own)).abs() < 0.01, "{name} {family:?}: icons moved");
            }
        }
    }

    #[test]
    fn descenders_stay_inside_their_row_for_every_text_style_and_scale() {
        let styles = [
            egui::FontId::proportional(12.0),
            egui::FontId::proportional(14.0),
            egui::FontId::proportional(19.0),
            egui::FontId::monospace(13.0),
        ];
        for (name, defs) in font_sets() {
            let ctx = egui::Context::default();
            ctx.set_fonts(defs);
            for scale in [1.0, 1.25, 1.5] {
                ctx.set_pixels_per_point(scale);
                let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                    ui.ctx().fonts_mut(|fonts| {
                        for id in &styles {
                            let row = fonts.row_height(id);
                            for text in SAMPLES {
                                let g = fonts.layout_no_wrap(text.to_owned(), id.clone(), egui::Color32::WHITE);
                                let b = g.mesh_bounds;
                                let h = g.size().y;
                                let ctx = format!("{name} {id:?} x{scale} {text:?}: row {row} height {h} bounds {b:?}");
                                assert!((h - row).abs() <= 1.0 / scale, "{ctx}");
                                assert!(b.max.y <= h.max(row) + 0.01, "{ctx}");
                                assert!(b.min.y >= -1.01 / scale, "{ctx}");
                            }
                        }
                    });
                });
            }
        }
    }

    #[test]
    fn metrics_are_read_from_hhea_and_os2() {
        let m = row_metrics(ICON_BYTES, 0).unwrap();
        assert!(m.ascent > 0.5 && m.descent < 0.0 && m.height() > 0.9);
        let taller = RowMetrics { ascent: 1.1, descent: -0.3, gap: 0.0 };
        let fitted = row_metrics(&icons_fitting(ICON_BYTES, taller).unwrap(), 0).unwrap();
        assert!(fitted.height() >= taller.height());
        assert!(icons_fitting(ICON_BYTES, RowMetrics { ascent: 0.5, descent: -0.1, gap: 0.0 }).is_none());
        assert!(row_metrics(&[0, 1, 2], 0).is_none());
    }

    #[test]
    fn icons_have_consistent_advances_at_every_ui_scale() {
        let ctx = egui::Context::default();
        ctx.set_fonts(definitions_with(None, None));
        for scale in [1.0, 1.25, 1.5, 2.0] {
            ctx.set_pixels_per_point(scale);
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.ctx().fonts_mut(|fonts| {
                    let id = egui::FontId::proportional(20.0);
                    for glyph in ["🗋", "🗁", "💾", "📷", "⚙", "🌿", "🎨", "🔍", "🔒"] {
                        assert!(fonts.has_glyphs(&id, glyph), "{glyph}");
                        let text = fonts.layout_no_wrap(glyph.to_owned(), id.clone(), egui::Color32::WHITE);
                        assert!((text.size().x - 20.0).abs() <= 1.0 / scale, "{glyph}: {:?}", text.size());
                    }
                });
            });
        }
    }
}
