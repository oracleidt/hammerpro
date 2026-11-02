use std::collections::{BTreeMap, HashMap};

use egui::{Key, KeyboardShortcut, Modifiers};
use serde::{Deserialize, Serialize};

use super::Action;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeymapPreset {
    #[default]
    HammerPro,
    Hammer,
    HammerPlusPlus,
}

impl KeymapPreset {
    pub const ALL: [KeymapPreset; 3] = [KeymapPreset::HammerPro, KeymapPreset::Hammer, KeymapPreset::HammerPlusPlus];

    pub fn id(self) -> &'static str {
        match self {
            KeymapPreset::HammerPro => "hammerpro",
            KeymapPreset::Hammer => "hammer",
            KeymapPreset::HammerPlusPlus => "hammerpp",
        }
    }

    pub fn tables(self) -> &'static [&'static [(&'static str, &'static str)]] {
        match self {
            KeymapPreset::HammerPro => &[],
            KeymapPreset::Hammer => &[HAMMER_KEYS],
            KeymapPreset::HammerPlusPlus => &[HAMMER_KEYS, HAMMER_PLUS_PLUS_KEYS],
        }
    }

    pub fn alias_table(self) -> &'static [(&'static str, &'static str)] {
        match self {
            KeymapPreset::HammerPro => HAMMERPRO_ALIASES,
            KeymapPreset::Hammer | KeymapPreset::HammerPlusPlus => &[],
        }
    }
}

const HAMMER_KEYS: &[(&str, &str)] = &[
    ("toggle_grid", "Shift+R"),
    ("select_all", "Ctrl+Shift+A"),
    ("select_none", "Shift+Q"),
    ("reset_view_splits", "Ctrl+A"),
    ("flip_vertical", "Ctrl+I"),
    ("invert_selection", "Ctrl+Shift+I"),
    ("find_replace_text", "Ctrl+Shift+R"),
    ("replace_textures", ""),
    ("run_map", "F9"),
    ("compile_now", "Ctrl+F9"),
];

const HAMMER_PLUS_PLUS_KEYS: &[(&str, &str)] = &[
    ("show_model_browser", "Shift+F10"),
    ("show_particle_browser", "Shift+F11"),
    ("show_sound_browser", "Shift+F12"),
    ("toggle_tool_textures", "Ctrl+Shift+F2"),
    ("toggle_editor_objects", "Alt+O"),
];

const HAMMERPRO_ALIASES: &[(&str, &str)] = &[("select_none", "Shift+Q"), ("toggle_grid", "Shift+R")];

#[derive(Clone, Debug)]
pub struct KeyBindings {
    preset: KeymapPreset,
    preset_keys: HashMap<Action, Option<KeyboardShortcut>>,
    aliases: Vec<(Action, KeyboardShortcut)>,
    overrides: HashMap<Action, Option<KeyboardShortcut>>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::from_preset(KeymapPreset::HammerPro, &BTreeMap::new())
    }
}

impl KeyBindings {
    pub fn from_settings(map: &BTreeMap<String, String>) -> Self {
        Self::from_preset(KeymapPreset::HammerPro, map)
    }

    pub fn from_preset(preset: KeymapPreset, map: &BTreeMap<String, String>) -> Self {
        let mut overrides = HashMap::new();
        for (id, text) in map {
            if let Some(action) = Action::from_id(id) {
                let parsed = parse_shortcut(text);
                if parsed.is_none() && !text.trim().is_empty() {
                    continue;
                }
                overrides.insert(action, parsed);
            }
        }
        let mut out = Self { preset, preset_keys: HashMap::new(), aliases: Vec::new(), overrides };
        out.set_preset(preset);
        out
    }

    pub fn preset(&self) -> KeymapPreset {
        self.preset
    }

    pub fn set_preset(&mut self, preset: KeymapPreset) {
        self.preset = preset;
        self.preset_keys.clear();
        for table in preset.tables() {
            for (id, text) in *table {
                if let Some(action) = Action::from_id(id) {
                    self.preset_keys.insert(action, parse_shortcut(text));
                }
            }
        }
        self.aliases = preset
            .alias_table()
            .iter()
            .filter_map(|(id, text)| Some((Action::from_id(id)?, parse_shortcut(text)?)))
            .collect();
    }

    pub fn preset_shortcut(&self, action: Action) -> Option<KeyboardShortcut> {
        match self.preset_keys.get(&action) {
            Some(sc) => *sc,
            None => action.default_shortcut(),
        }
    }

    pub fn shortcut(&self, action: Action) -> Option<KeyboardShortcut> {
        match self.overrides.get(&action) {
            Some(o) => *o,
            None => self.preset_shortcut(action),
        }
    }

    pub fn set(&mut self, action: Action, shortcut: Option<KeyboardShortcut>) {
        if same_shortcut(shortcut, self.preset_shortcut(action)) {
            self.overrides.remove(&action);
        } else {
            self.overrides.insert(action, shortcut);
        }
    }

    pub fn reset(&mut self, action: Action) {
        self.overrides.remove(&action);
    }

    pub fn reset_all(&mut self) {
        self.overrides.clear();
    }

    pub fn is_customized(&self, action: Action) -> bool {
        !same_shortcut(self.shortcut(action), self.preset_shortcut(action))
    }

    pub fn aliases(&self, action: Action) -> Vec<KeyboardShortcut> {
        if !action.is_ready() {
            return Vec::new();
        }
        self.aliases
            .iter()
            .filter(|(a, _)| *a == action)
            .map(|(_, sc)| *sc)
            .filter(|sc| self.primary_for(sc).is_none())
            .collect()
    }

    pub fn to_settings(&self) -> BTreeMap<String, String> {
        self.overrides
            .iter()
            .map(|(a, s)| (a.id().to_owned(), s.map(|s| format_shortcut(&s)).unwrap_or_default()))
            .collect()
    }

    fn primary_for(&self, sc: &KeyboardShortcut) -> Option<Action> {
        let wanted = normalize_shortcut(sc);
        Action::ALL.iter().copied().find(|&a| self.shortcut(a).is_some_and(|s| normalize_shortcut(&s) == wanted))
    }

    pub fn action_for(&self, sc: &KeyboardShortcut) -> Option<Action> {
        if let Some(action) = self.primary_for(sc) {
            return Some(action);
        }
        let wanted = normalize_shortcut(sc);
        self.aliases.iter().find(|(a, s)| a.is_ready() && normalize_shortcut(s) == wanted).map(|(a, _)| *a)
    }

    pub fn take_triggered(&self, events: &mut Vec<egui::Event>) -> Vec<Action> {
        let mut out = Vec::new();
        events.retain(|event| {
            let (candidates, repeat) = match event {
                egui::Event::Key { key, physical_key, pressed: true, repeat, modifiers } if !is_modifier_key(*key) => {
                    (shortcut_candidates(*key, *physical_key, *modifiers), *repeat)
                }
                egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) | egui::Event::PasteImage(_) => {
                    (clipboard_event_shortcut(event).into_iter().collect(), false)
                }
                _ => return true,
            };
            let Some(action) = candidates.iter().find_map(|c| self.action_for(c)) else {
                return true;
            };
            if !repeat || action.repeats() {
                out.push(action);
            }
            false
        });
        out
    }

    pub fn take_action(&self, events: &mut Vec<egui::Event>, action: Action) -> bool {
        let found = events.iter().position(|event| match event {
            egui::Event::Key { key, physical_key, pressed: true, repeat: false, modifiers }
                if !is_modifier_key(*key) =>
            {
                shortcut_candidates(*key, *physical_key, *modifiers).iter().find_map(|c| self.action_for(c))
                    == Some(action)
            }
            _ => false,
        });
        match found {
            Some(i) => {
                events.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn poll(&self, ctx: &egui::Context, text_focus: bool) -> Vec<Action> {
        if text_focus {
            return Vec::new();
        }
        ctx.input_mut(|i| self.take_triggered(&mut i.events))
    }
}

pub fn normalize_modifiers(m: Modifiers) -> Modifiers {
    Modifiers { alt: m.alt, ctrl: false, shift: m.shift, mac_cmd: false, command: m.command || m.ctrl || m.mac_cmd }
}

fn normalize_shortcut(sc: &KeyboardShortcut) -> KeyboardShortcut {
    KeyboardShortcut::new(normalize_modifiers(sc.modifiers), sc.logical_key)
}

fn same_shortcut(a: Option<KeyboardShortcut>, b: Option<KeyboardShortcut>) -> bool {
    a.map(|s| normalize_shortcut(&s)) == b.map(|s| normalize_shortcut(&s))
}

fn is_digit(key: Key) -> bool {
    matches!(
        key,
        Key::Num0
            | Key::Num1
            | Key::Num2
            | Key::Num3
            | Key::Num4
            | Key::Num5
            | Key::Num6
            | Key::Num7
            | Key::Num8
            | Key::Num9
    )
}

fn is_symbol(key: Key) -> bool {
    matches!(
        key,
        Key::Colon
            | Key::Comma
            | Key::Backslash
            | Key::Slash
            | Key::Pipe
            | Key::Questionmark
            | Key::Exclamationmark
            | Key::OpenBracket
            | Key::CloseBracket
            | Key::OpenCurlyBracket
            | Key::CloseCurlyBracket
            | Key::Backtick
            | Key::Minus
            | Key::Period
            | Key::Plus
            | Key::Equals
            | Key::Semicolon
            | Key::Quote
    )
}

pub fn shortcut_from_key(key: Key, physical_key: Option<Key>, modifiers: Modifiers) -> KeyboardShortcut {
    let key = physical_key.filter(|k| is_digit(*k)).unwrap_or(key);
    KeyboardShortcut::new(normalize_modifiers(modifiers), key)
}

pub fn shortcut_from_event(event: &egui::Event) -> Option<KeyboardShortcut> {
    match event {
        egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } if !is_modifier_key(*key) => {
            Some(shortcut_from_key(*key, *physical_key, *modifiers))
        }
        _ => None,
    }
}

pub fn clipboard_event_shortcut(event: &egui::Event) -> Option<KeyboardShortcut> {
    let key = match event {
        egui::Event::Copy => Key::C,
        egui::Event::Cut => Key::X,
        egui::Event::Paste(_) | egui::Event::PasteImage(_) => Key::V,
        _ => return None,
    };
    Some(KeyboardShortcut::new(Modifiers::COMMAND, key))
}

fn shortcut_candidates(key: Key, physical_key: Option<Key>, modifiers: Modifiers) -> Vec<KeyboardShortcut> {
    let primary = shortcut_from_key(key, physical_key, modifiers);
    let mut out = vec![primary];
    if primary.logical_key != key {
        out.push(KeyboardShortcut::new(primary.modifiers, key));
    }
    if modifiers.ctrl && modifiers.alt && is_symbol(key) {
        out.push(KeyboardShortcut::new(Modifiers { shift: modifiers.shift, ..Modifiers::NONE }, key));
    }
    out
}

pub fn format_shortcut(sc: &KeyboardShortcut) -> String {
    let mut s = String::new();
    if sc.modifiers.command || sc.modifiers.ctrl {
        s.push_str("Ctrl+");
    }
    if sc.modifiers.alt {
        s.push_str("Alt+");
    }
    if sc.modifiers.shift {
        s.push_str("Shift+");
    }
    s.push_str(sc.logical_key.name());
    s
}

pub fn key_label(key: Key) -> String {
    key_label_in(key, crate::i18n::lang())
}

pub fn key_label_in(key: Key, lang: crate::i18n::Lang) -> String {
    let named = match key {
        Key::Delete => "key.delete",
        Key::Enter => "key.enter",
        Key::Escape => "key.escape",
        Key::Space => "key.space",
        Key::Backspace => "key.backspace",
        Key::Tab => "key.tab",
        Key::Insert => "key.insert",
        Key::Home => "key.home",
        Key::End => "key.end",
        Key::PageUp => "key.page_up",
        Key::PageDown => "key.page_down",
        Key::ArrowUp => "key.up",
        Key::ArrowDown => "key.down",
        Key::ArrowLeft => "key.left",
        Key::ArrowRight => "key.right",
        Key::Minus => return "-".to_owned(),
        _ => return key.symbol_or_name().to_owned(),
    };
    crate::i18n::tr_in(named, lang).to_owned()
}

pub fn shortcut_label(sc: &KeyboardShortcut) -> String {
    shortcut_label_in(sc, crate::i18n::lang())
}

pub fn shortcut_label_in(sc: &KeyboardShortcut, lang: crate::i18n::Lang) -> String {
    let mut s = modifiers_label_in(sc.modifiers, lang);
    s.push_str(&key_label_in(sc.logical_key, lang));
    s
}

pub fn modifiers_label(modifiers: Modifiers) -> String {
    modifiers_label_in(modifiers, crate::i18n::lang())
}

fn modifiers_label_in(modifiers: Modifiers, lang: crate::i18n::Lang) -> String {
    use crate::i18n::tr_in;
    let mut s = String::new();
    if modifiers.command || modifiers.ctrl || modifiers.mac_cmd {
        s.push_str(tr_in("key.ctrl", lang));
        s.push('+');
    }
    if modifiers.alt {
        s.push_str(tr_in("key.alt", lang));
        s.push('+');
    }
    if modifiers.shift {
        s.push_str(tr_in("key.shift", lang));
        s.push('+');
    }
    s
}

pub fn parse_shortcut(text: &str) -> Option<KeyboardShortcut> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => mods.command = true,
            "shift" => mods.shift = true,
            "alt" => mods.alt = true,
            _ => key = Key::from_name(part),
        }
    }
    key.filter(|k| !is_modifier_key(*k)).map(|k| KeyboardShortcut::new(mods, k))
}

pub fn is_modifier_key(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
    )
}

#[cfg(test)]
mod preset_tests {
    use super::*;

    fn sc(text: &str) -> KeyboardShortcut {
        parse_shortcut(text).unwrap_or_else(|| panic!("bad shortcut {text}"))
    }

    fn press(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn preset_tables_resolve() {
        for preset in KeymapPreset::ALL {
            let entries = preset.tables().iter().flat_map(|t| t.iter()).chain(preset.alias_table());
            for (id, text) in entries {
                assert!(Action::from_id(id).is_some(), "{preset:?}: unknown action {id}");
                assert!(text.is_empty() || parse_shortcut(text).is_some(), "{preset:?}: bad shortcut {text}");
                if !text.is_empty() {
                    assert_eq!(format_shortcut(&sc(text)), *text, "{preset:?}: {text} is not in storage form");
                }
            }
        }
    }

    #[test]
    fn preset_shortcuts_unique() {
        for preset in KeymapPreset::ALL {
            let keys = KeyBindings::from_preset(preset, &BTreeMap::new());
            let mut seen: HashMap<String, &str> = HashMap::new();
            for &a in Action::DECLARED {
                if let Some(s) = keys.preset_shortcut(a)
                    && let Some(prev) = seen.insert(format_shortcut(&s), a.id())
                {
                    panic!("{preset:?}: {} used by {prev} and {}", format_shortcut(&s), a.id());
                }
            }
            for (id, text) in preset.alias_table() {
                if let Some(prev) = seen.insert((*text).to_owned(), id) {
                    panic!("{preset:?}: alias {text} of {id} collides with {prev}");
                }
            }
        }
    }

    #[test]
    fn hammer_preset_changes_the_defaults() {
        let keys = KeyBindings::from_preset(KeymapPreset::Hammer, &BTreeMap::new());
        assert_eq!(keys.preset(), KeymapPreset::Hammer);
        assert_eq!(keys.shortcut(Action::ToggleGrid), Some(sc("Shift+R")));
        assert_eq!(keys.shortcut(Action::SelectNone), Some(sc("Shift+Q")));
        assert_eq!(keys.shortcut(Action::SelectAll), Some(sc("Ctrl+Shift+A")));
        assert_eq!(keys.shortcut(Action::Undo), Action::Undo.default_shortcut());
        assert_eq!(keys.shortcut(Action::FindReplaceText), Some(sc("Ctrl+Shift+R")));
        assert_eq!(keys.shortcut(Action::ReplaceTextures), None);
        assert_eq!(keys.shortcut(Action::RunMap), Some(sc("F9")), "Run Map is F9 in Hammer");
        assert_eq!(keys.shortcut(Action::CompileNow), Some(sc("Ctrl+F9")));
        assert!(!keys.is_customized(Action::ToggleGrid));
        let mut events = vec![press(Key::R, Modifiers::SHIFT)];
        assert_eq!(keys.take_triggered(&mut events), vec![Action::ToggleGrid]);
        let mut events = vec![press(Key::G, Modifiers::SHIFT)];
        assert!(keys.take_triggered(&mut events).is_empty());
        let pp = KeyBindings::from_preset(KeymapPreset::HammerPlusPlus, &BTreeMap::new());
        assert_eq!(pp.shortcut(Action::ShowModelBrowser), Some(sc("Shift+F10")));
        assert_eq!(pp.shortcut(Action::ToggleGrid), Some(sc("Shift+R")));
    }

    #[test]
    fn user_overrides_stay_on_top_of_presets() {
        let mut map = BTreeMap::new();
        map.insert("toggle_grid".to_owned(), "Ctrl+Alt+G".to_owned());
        map.insert("undo".to_owned(), String::new());
        let mut keys = KeyBindings::from_preset(KeymapPreset::Hammer, &map);
        assert_eq!(keys.shortcut(Action::ToggleGrid), Some(sc("Ctrl+Alt+G")));
        assert_eq!(keys.shortcut(Action::Undo), None);
        assert!(keys.is_customized(Action::ToggleGrid) && keys.is_customized(Action::Undo));
        keys.set_preset(KeymapPreset::HammerPro);
        assert_eq!(keys.shortcut(Action::ToggleGrid), Some(sc("Ctrl+Alt+G")));
        assert_eq!(keys.preset_shortcut(Action::ToggleGrid), Action::ToggleGrid.default_shortcut());
        keys.set(Action::ToggleGrid, Action::ToggleGrid.default_shortcut());
        assert!(!keys.is_customized(Action::ToggleGrid));
        assert!(!keys.to_settings().contains_key("toggle_grid"));
        keys.set_preset(KeymapPreset::Hammer);
        assert_eq!(keys.shortcut(Action::ToggleGrid), Some(sc("Shift+R")), "follows the preset again");
        keys.reset(Action::Undo);
        assert_eq!(keys.shortcut(Action::Undo), Action::Undo.default_shortcut());
        keys.set(Action::Copy, None);
        keys.reset_all();
        assert!(keys.to_settings().is_empty());
    }

    #[test]
    fn aliases_come_after_every_binding() {
        let mut keys = KeyBindings::default();
        assert_eq!(keys.aliases(Action::SelectNone), vec![sc("Shift+Q")]);
        let mut events = vec![press(Key::Q, Modifiers::SHIFT)];
        assert_eq!(keys.take_triggered(&mut events), vec![Action::SelectNone]);
        let mut events = vec![press(Key::A, Modifiers::COMMAND | Modifiers::SHIFT)];
        assert_eq!(keys.take_triggered(&mut events), vec![Action::SelectNone]);
        keys.set(Action::Copy, Some(sc("Shift+Q")));
        let mut events = vec![press(Key::Q, Modifiers::SHIFT)];
        assert_eq!(keys.take_triggered(&mut events), vec![Action::Copy]);
        assert!(keys.aliases(Action::SelectNone).is_empty());
        let hammer = KeyBindings::from_preset(KeymapPreset::Hammer, &BTreeMap::new());
        assert!(hammer.aliases(Action::ToggleGrid).is_empty());
    }

    #[test]
    fn preset_ids_and_serde() {
        let ids: Vec<&str> = KeymapPreset::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(ids, ["hammerpro", "hammer", "hammerpp"]);
        assert_eq!(KeymapPreset::default(), KeymapPreset::HammerPro);
    }

    #[test]
    fn modifier_keys_alone_are_never_shortcuts() {
        let press = |key: Key, modifiers: Modifiers| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        };
        assert_eq!(shortcut_from_event(&press(Key::ShiftLeft, Modifiers::SHIFT)), None);
        assert_eq!(shortcut_from_event(&press(Key::ControlRight, Modifiers::CTRL)), None);
        assert!(shortcut_from_event(&press(Key::S, Modifiers::CTRL)).is_some());
        assert_eq!(parse_shortcut("Shift+ShiftLeft"), None);
        assert_eq!(parse_shortcut("Ctrl+ControlLeft"), None);
        assert!(parse_shortcut("Ctrl+Shift+S").is_some());

        let mut saved = BTreeMap::new();
        saved.insert("import_map".to_owned(), "Shift+ShiftLeft".to_owned());
        let keys = KeyBindings::from_settings(&saved);
        let defaults = KeyBindings::from_settings(&BTreeMap::new());
        assert_eq!(keys.shortcut(Action::ImportMap), defaults.shortcut(Action::ImportMap), "default restored");
        for key in [Key::ShiftLeft, Key::ShiftRight, Key::ControlLeft, Key::AltLeft] {
            let mut events = vec![press(key, Modifiers::SHIFT)];
            assert!(keys.take_triggered(&mut events).is_empty(), "{key:?} alone runs nothing");
            assert_eq!(events.len(), 1, "the press is left for the rest of the UI");
        }
    }
}
