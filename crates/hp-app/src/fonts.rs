use std::path::PathBuf;
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontTweak};

const UI_FONT: &str = "hp-ui";
const SYMBOL_FONT: &str = "hp-symbols";

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
    defs
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
    fn without_system_fonts_the_defaults_stay() {
        let defs = definitions_with(None, None);
        assert!(!defs.font_data.contains_key(UI_FONT));
        assert!(defs.families[&FontFamily::Proportional].iter().any(|n| n == "Ubuntu-Light"));
    }

    #[test]
    fn the_ui_font_leads_and_symbols_follow() {
        let defs = definitions_with(Some(vec![0]), Some(vec![0]));
        let list = &defs.families[&FontFamily::Proportional];
        assert_eq!(list[0], UI_FONT);
        assert_eq!(list[1], SYMBOL_FONT);
        let mono = &defs.families[&FontFamily::Monospace];
        assert_eq!(mono[0], "Hack");
        assert!(mono.iter().any(|n| n == SYMBOL_FONT));
    }
}
