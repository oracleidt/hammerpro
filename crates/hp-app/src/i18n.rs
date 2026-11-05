use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

pub type TrTable = &'static [(&'static str, &'static str, &'static str)];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    En,
    Fr,
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::En, Lang::Fr, Lang::Ru];

    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Français",
            Lang::Ru => "Русский",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::Ru => "ru",
        }
    }

    pub fn from_system() -> Lang {
        match sys_locale::get_locale() {
            Some(l) if l.to_ascii_lowercase().starts_with("fr") => Lang::Fr,
            Some(l) if l.to_ascii_lowercase().starts_with("ru") => Lang::Ru,
            _ => Lang::En,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Fr,
        2 => Lang::Ru,
        _ => Lang::En,
    }
}

struct Registry {
    map: HashMap<&'static str, (&'static str, &'static str)>,
    conflicts: Vec<&'static str>,
    missing: HashMap<String, &'static str>,
}

fn registry() -> &'static RwLock<Registry> {
    static REG: OnceLock<RwLock<Registry>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut map = HashMap::with_capacity(2048);
        for table in std::iter::once(&crate::strings::TR).chain(crate::actions::TABLES) {
            for &(k, en, fr) in table.iter() {
                map.insert(k, (en, fr));
            }
        }
        RwLock::new(Registry { map, conflicts: Vec::new(), missing: HashMap::new() })
    })
}

pub fn register(table: TrTable) {
    let mut reg = registry().write();
    for &(k, en, fr) in table {
        if let Some(old) = reg.map.insert(k, (en, fr))
            && old != (en, fr)
            && !reg.conflicts.contains(&k)
        {
            reg.conflicts.push(k);
        }
    }
}

pub fn conflicting_keys() -> Vec<&'static str> {
    let mut keys = registry().read().conflicts.clone();
    keys.sort_unstable();
    keys
}

pub fn entries() -> Vec<(&'static str, &'static str, &'static str)> {
    let reg = registry().read();
    let mut out: Vec<_> = reg.map.iter().map(|(k, (en, fr))| (*k, *en, *fr)).collect();
    out.sort_unstable_by_key(|e| e.0);
    out
}

pub fn doubled_word(text: &str) -> Option<String> {
    let word = |t: &str| !t.is_empty() && t.chars().all(char::is_alphanumeric);
    let mut prev: Option<&str> = None;
    for t in text.split(' ') {
        let is_word = word(t) && !t.chars().all(|c| c.is_ascii_digit());
        if let Some(p) = prev
            && is_word
            && p.to_lowercase() == t.to_lowercase()
            && !matches!(t.to_lowercase().as_str(), "nous" | "vous")
        {
            return Some(format!("{p} {t}"));
        }
        prev = if is_word { Some(t) } else { None };
    }
    None
}

pub fn tr(key: &str) -> &'static str {
    tr_in(key, lang())
}

pub fn tr_in(key: &str, lang: Lang) -> &'static str {
    if lang == Lang::Ru
        && let Some(text) = russian().get(key).filter(|s| !s.is_empty())
    {
        return text;
    }
    {
        let reg = registry().read();
        if let Some(&(en, fr)) = reg.map.get(key) {
            return match lang {
                Lang::En | Lang::Ru => en,
                Lang::Fr => {
                    if fr.is_empty() {
                        en
                    } else {
                        fr
                    }
                }
            };
        }
        if let Some(&s) = reg.missing.get(key) {
            return s;
        }
    }
    let mut reg = registry().write();
    if let Some(&s) = reg.missing.get(key) {
        return s;
    }
    log::debug!("missing translation key: {key}");
    let leaked: &'static str = Box::leak(key.to_owned().into_boxed_str());
    reg.missing.insert(key.to_owned(), leaked);
    leaked
}

fn russian() -> &'static HashMap<String, String> {
    static RU: OnceLock<HashMap<String, String>> = OnceLock::new();
    RU.get_or_init(|| {
        serde_json::from_str(include_str!("i18n/ru.json")).unwrap_or_else(|error| {
            log::error!("cannot read the Russian interface catalog: {error}");
            HashMap::new()
        })
    })
}

pub fn has_translation(key: &str, lang: Lang) -> bool {
    if lang == Lang::Ru { russian().get(key).is_some_and(|text| !text.is_empty()) } else { has_key(key) }
}

pub fn has_key(key: &str) -> bool {
    registry().read().map.contains_key(key)
}

pub fn tf(key: &str, args: &[&dyn Display]) -> String {
    format_placeholders(tr(key), args)
}

pub fn format_placeholders(template: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let inner = &after[..close];
                match inner.parse::<usize>().ok().and_then(|i| args.get(i)) {
                    Some(arg) => {
                        use std::fmt::Write as _;
                        let _ = write!(out, "{arg}");
                    }
                    None => {
                        out.push('{');
                        out.push_str(inner);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::tr($key)
    };
}

#[macro_export]
macro_rules! tf {
    ($key:expr $(, $arg:expr)* $(,)?) => {
        $crate::i18n::tf($key, &[$(&$arg as &dyn ::std::fmt::Display),*])
    };
}

pub fn lint_table(table: TrTable) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for &(k, en, fr) in table {
        if !seen.insert(k) {
            problems.push(format!("duplicate key {k}"));
        }
        if en.is_empty() {
            problems.push(format!("{k}: empty English text"));
        }
        if fr.is_empty() {
            problems.push(format!("{k}: empty French text"));
        }
        for s in [k, en, fr] {
            if s.contains('\u{2014}') || s.contains('\u{2013}') {
                problems.push(format!("{k}: contains an em/en dash"));
            }
        }
        for s in [en, fr] {
            if let Some(w) = doubled_word(s) {
                problems.push(format!("{k}: the word is doubled: \"{w}\""));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_table_is_clean() {
        let problems = lint_table(crate::strings::TR);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn doubled_words_are_found() {
        assert_eq!(doubled_word("Click the the button"), Some("the the".to_owned()));
        assert_eq!(doubled_word("Le le texte"), Some("Le le".to_owned()));
        assert_eq!(doubled_word("Click the button"), None);
        assert_eq!(doubled_word("kw:mot  mot-clé"), None, "a list, not a sentence");
        assert_eq!(doubled_word("1 1 0"), None);
        assert_eq!(doubled_word("Nous nous en occupons"), None);
        assert_eq!(doubled_word("{0} {0}"), None, "placeholders are not words");
        assert_eq!(doubled_word("a\nA"), None);
    }

    #[test]
    fn conflicting_registrations_are_listed() {
        register(&[("test.conflict", "One", "Un")]);
        register(&[("test.conflict", "One", "Un")]);
        assert!(!conflicting_keys().contains(&"test.conflict"), "same texts: no conflict");
        register(&[("test.conflict", "Two", "Deux")]);
        assert!(conflicting_keys().contains(&"test.conflict"));
        assert!(entries().iter().any(|e| e.0 == "test.conflict" && e.1 == "Two"));
    }

    #[test]
    fn placeholders() {
        assert_eq!(format_placeholders("{0} of {1}", &[&3, &"x"]), "3 of x");
        assert_eq!(format_placeholders("{9} {", &[&1]), "{9} {");
    }

    #[test]
    fn switching_language() {
        register(&[("test.hello", "Hello", "Bonjour")]);
        assert_eq!(tr_in("test.hello", Lang::En), "Hello");
        assert_eq!(tr_in("test.hello", Lang::Fr), "Bonjour");
        assert_eq!(tr_in("test.unknown.key", Lang::Fr), "test.unknown.key");
        assert_eq!(tr_in("test.hello", Lang::Ru), "Hello");
        assert_eq!(tr_in("common.cancel", Lang::Ru), "Отмена");
        assert_eq!(Lang::Ru.native_name(), "Русский");
        assert_eq!(Lang::Ru.code(), "ru");
        assert_eq!(serde_json::from_str::<Lang>("\"Ru\"").ok(), Some(Lang::Ru));
    }

    #[test]
    fn russian_catalog_parses_without_empty_or_damaged_strings() {
        let parsed = serde_json::from_str::<HashMap<String, String>>(include_str!("i18n/ru.json"));
        assert!(parsed.is_ok());
        let catalog = parsed.unwrap_or_default();
        assert!(catalog.len() > 8000);
        for (key, text) in catalog {
            assert!(!text.is_empty(), "{key}");
            assert!(!text.contains('\u{fffd}'), "{key}");
            assert!(!text.contains('⟦'), "{key}: unrestored technical token");
            assert!(!text.contains('\u{2013}') && !text.contains('\u{2014}'), "{key}");
        }
    }
}
