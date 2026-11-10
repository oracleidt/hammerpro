use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use hp_editor::{Document, Id};

use crate::view::DocStamp;

pub const SPECIAL_TARGETS: [&str; 7] =
    ["!self", "!activator", "!caller", "!player", "!pvsplayer", "!speechtarget", "!picker"];

pub const SLOW_REFRESH_MS: f64 = 1.0;
pub const REFRESH_INTERVAL_S: f64 = 0.25;

#[derive(Clone, Debug, Default)]
pub struct MapNames {
    stamp: Option<DocStamp>,
    fingerprint: u64,
    checked_at: Option<f64>,
    check_ms: f64,
    names: Vec<String>,
    by_name: HashMap<String, Vec<(Id, u32)>>,
    classes: HashMap<String, u32>,
    class_list: Vec<String>,
    filters: Vec<String>,
}

impl MapNames {
    pub fn build(doc: &Document) -> MapNames {
        let mut out = MapNames { stamp: Some(DocStamp::of(doc)), fingerprint: fingerprint(doc), ..MapNames::default() };
        let mut filters: HashSet<String> = HashSet::new();
        for node in doc.scene().entities().values() {
            let e = &node.entity;
            let class = lower(e.classname.trim());
            let class_index = match out.classes.get(class.as_ref()) {
                Some(i) => *i,
                None => {
                    let i = u32::try_from(out.class_list.len()).unwrap_or(u32::MAX);
                    out.classes.insert(class.clone().into_owned(), i);
                    out.class_list.push(class.clone().into_owned());
                    i
                }
            };
            let Some(name) = e.get("targetname").map(str::trim).filter(|n| !n.is_empty()) else {
                continue;
            };
            let key = lower(name);
            match out.by_name.get_mut(key.as_ref()) {
                Some(list) => list.push((e.id, class_index)),
                None => {
                    out.by_name.insert(key.clone().into_owned(), vec![(e.id, class_index)]);
                    out.names.push(name.to_owned());
                }
            }
            if class.starts_with("filter_") && !filters.contains(key.as_ref()) {
                filters.insert(key.into_owned());
                out.filters.push(name.to_owned());
            }
        }
        out.names.sort_unstable_by(|a, b| cmp_ignore_case(a, b));
        out.filters.sort_unstable_by(|a, b| cmp_ignore_case(a, b));
        out
    }

    pub fn refresh(&mut self, doc: &Document) {
        let stamp = DocStamp::of(doc);
        if self.stamp == Some(stamp) {
            return;
        }
        if self.stamp.is_some() && fingerprint(doc) == self.fingerprint {
            self.stamp = Some(stamp);
            return;
        }
        *self = MapNames::build(doc);
    }

    pub fn refresh_at(&mut self, doc: &Document, now: f64) -> bool {
        if self.stamp == Some(DocStamp::of(doc)) {
            return false;
        }
        if self.check_ms > SLOW_REFRESH_MS
            && let Some(at) = self.checked_at
            && (0.0..REFRESH_INTERVAL_S).contains(&(now - at))
        {
            return true;
        }
        let start = std::time::Instant::now();
        self.refresh(doc);
        self.check_ms = start.elapsed().as_secs_f64() * 1000.0;
        self.checked_at = Some(now);
        false
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn filters(&self) -> &[String] {
        &self.filters
    }

    pub fn entities_named(&self, name: &str) -> Vec<(Id, &str)> {
        self.by_name
            .get(lower(name.trim()).as_ref())
            .map_or_else(Vec::new, |list| list.iter().map(|(id, c)| (*id, self.class_name(*c))).collect())
    }

    fn class_name(&self, index: u32) -> &str {
        self.class_list.get(index as usize).map_or("", String::as_str)
    }

    pub fn has_class(&self, class: &str) -> bool {
        self.classes.contains_key(lower(class.trim()).as_ref())
    }

    pub fn target_exists(&self, target: &str) -> bool {
        let t = target.trim();
        if t.is_empty() {
            return false;
        }
        if t.starts_with('!') {
            return true;
        }
        let key = lower(t);
        if self.by_name.contains_key(key.as_ref()) || self.classes.contains_key(key.as_ref()) {
            return true;
        }
        if let Some(prefix) = key.strip_suffix('*') {
            return self.by_name.keys().any(|k| k.starts_with(prefix));
        }
        false
    }

    pub fn target_classes(&self, target: &str) -> Vec<String> {
        let t = target.trim();
        if t.is_empty() || t.starts_with('!') {
            return Vec::new();
        }
        let key = lower(t);
        let mut indices: Vec<u32> = Vec::new();
        let mut push = |c: u32| {
            if !indices.contains(&c) {
                indices.push(c);
            }
        };
        if let Some(prefix) = key.strip_suffix('*') {
            for (k, list) in &self.by_name {
                if k.starts_with(prefix) {
                    list.iter().for_each(|(_, c)| push(*c));
                }
            }
        } else if let Some(list) = self.by_name.get(key.as_ref()) {
            list.iter().for_each(|(_, c)| push(*c));
        }
        if let Some(c) = self.classes.get(key.as_ref()) {
            push(*c);
        }
        indices.into_iter().map(|c| self.class_name(c).to_owned()).collect()
    }
}

fn lower(s: &str) -> Cow<'_, str> {
    if s.bytes().any(|b| b.is_ascii_uppercase()) { Cow::Owned(s.to_ascii_lowercase()) } else { Cow::Borrowed(s) }
}

pub fn cmp_ignore_case(a: &str, b: &str) -> Ordering {
    a.bytes().map(|c| c.to_ascii_lowercase()).cmp(b.bytes().map(|c| c.to_ascii_lowercase())).then_with(|| a.cmp(b))
}

fn fingerprint(doc: &Document) -> u64 {
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(PRIME);
        }
        h ^= 0xff;
        h = h.wrapping_mul(PRIME);
    };
    for node in doc.scene().entities().values() {
        let e = &node.entity;
        eat(e.classname.as_bytes());
        eat(e.get("targetname").unwrap_or("").as_bytes());
    }
    h
}

pub fn match_score(query_lower: &str, candidate: &str) -> Option<u8> {
    if query_lower.is_empty() {
        return Some(5);
    }
    let cand = candidate.to_ascii_lowercase();
    if cand == query_lower {
        return Some(0);
    }
    if cand.starts_with(query_lower) {
        return Some(1);
    }
    if let Some(pos) = cand.find(query_lower) {
        let at_word = cand[..pos].ends_with(['_', '/', '.', '-', ' ']) || {
            cand.match_indices(query_lower).any(|(i, _)| i > 0 && cand[..i].ends_with(['_', '/', '.', '-', ' ']))
        };
        return Some(if at_word { 2 } else { 3 });
    }
    let mut it = cand.chars();
    if query_lower.chars().all(|q| it.any(|c| c == q)) {
        return Some(4);
    }
    None
}

pub fn rank<'a>(query: &str, candidates: impl IntoIterator<Item = &'a str>, limit: usize) -> Vec<usize> {
    rank_up_to(query, candidates, limit, u8::MAX)
}

pub fn suggest<'a>(query: &str, candidates: impl IntoIterator<Item = &'a str>, limit: usize) -> Vec<usize> {
    rank_up_to(query, candidates, limit, 3)
}

fn rank_up_to<'a>(
    query: &str,
    candidates: impl IntoIterator<Item = &'a str>,
    limit: usize,
    max_score: u8,
) -> Vec<usize> {
    let q = query.trim().to_ascii_lowercase();
    let mut scored: Vec<(u8, usize, &str, usize)> = candidates
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| match_score(&q, c).filter(|s| *s <= max_score || q.is_empty()).map(|s| (s, c.len(), c, i)))
        .collect();
    scored.sort_unstable_by(|a, b| {
        a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then_with(|| a.2.to_ascii_lowercase().cmp(&b.2.to_ascii_lowercase()))
    });
    scored.into_iter().take(limit).map(|(_, _, _, i)| i).collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WordSearch {
    pub terms: Vec<String>,
    pub excluded: Vec<String>,
}

pub fn contains_lower(haystack: &str, needle_lower: &str) -> bool {
    let (h, n) = (haystack.as_bytes(), needle_lower.as_bytes());
    let Some((&first, rest)) = n.split_first() else { return true };
    if n.len() > h.len() {
        return false;
    }
    let last = h.len() - n.len();
    let mut i = 0;
    while i <= last {
        if h[i].to_ascii_lowercase() == first
            && h[i + 1..i + n.len()].iter().zip(rest).all(|(a, b)| a.to_ascii_lowercase() == *b)
        {
            return true;
        }
        i += 1;
    }
    false
}

impl WordSearch {
    pub fn parse(text: &str) -> WordSearch {
        let mut out = WordSearch::default();
        for word in text.split_whitespace() {
            let w = word.to_ascii_lowercase();
            match w.strip_prefix('!') {
                Some(rest) if !rest.is_empty() => out.excluded.push(rest.to_owned()),
                Some(_) => {}
                None => out.terms.push(w),
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.excluded.is_empty()
    }

    pub fn matches(&self, text: &str) -> bool {
        if self.is_empty() {
            return true;
        }
        self.terms.iter().all(|t| contains_lower(text, t)) && !self.excluded.iter().any(|t| contains_lower(text, t))
    }

    pub fn matches2(&self, a: &str, b: &str) -> bool {
        if self.is_empty() {
            return true;
        }
        self.terms.iter().all(|t| contains_lower(a, t) || contains_lower(b, t))
            && !self.excluded.iter().any(|t| contains_lower(a, t) || contains_lower(b, t))
    }
}

pub fn filter_words<'a>(query: &str, candidates: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_ascii_lowercase).collect();
    candidates
        .into_iter()
        .enumerate()
        .filter(|(_, c)| {
            words.is_empty() || {
                let lower = c.to_ascii_lowercase();
                words.iter().all(|w| lower.contains(w.as_str()))
            }
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_order() {
        assert_eq!(match_score("door", "door"), Some(0));
        assert_eq!(match_score("door", "door_1"), Some(1));
        assert_eq!(match_score("door", "main_door"), Some(2));
        assert_eq!(match_score("door", "maindoor"), Some(3));
        assert_eq!(match_score("mdr", "main_door"), Some(4));
        assert_eq!(match_score("xyz", "main_door"), None);
        assert_eq!(match_score("", "anything"), Some(5));
        assert_eq!(match_score("DOOR", "Door"), None, "query must be lowercase");
        assert_eq!(match_score("door", "DOOR"), Some(0));
    }

    #[test]
    fn ranking_prefers_prefix_then_short() {
        let names = ["main_light", "light", "light_spot", "spotlight", "l_i_g_h_t"];
        let order: Vec<&str> = rank("light", names.iter().copied(), 10).into_iter().map(|i| names[i]).collect();
        assert_eq!(order, ["light", "light_spot", "main_light", "spotlight", "l_i_g_h_t"]);
        assert_eq!(rank("light", names.iter().copied(), 2).len(), 2);
        assert!(rank("zzz", names.iter().copied(), 10).is_empty());
    }

    #[test]
    fn suggestions_need_the_typed_text_in_one_piece() {
        let names = ["Triggerhurt_07", "relay_01", "timer_01", "main_light"];
        let found =
            |q: &str| -> Vec<&str> { suggest(q, names.iter().copied(), 10).into_iter().map(|i| names[i]).collect() };
        assert!(found("th7").is_empty(), "scattered letters are not a suggestion");
        assert_eq!(found("relay"), ["relay_01"]);
        assert_eq!(found("_01"), ["relay_01", "timer_01"]);
        assert_eq!(found("light"), ["main_light"]);
        assert_eq!(found("").len(), 4, "an empty field lists every name");
        assert_eq!(rank("th7", names.iter().copied(), 10), [0]);
    }

    #[test]
    fn word_search_with_negation() {
        let s = WordSearch::parse("Vent office !dynamic !");
        assert_eq!(s.terms, ["vent", "office"]);
        assert_eq!(s.excluded, ["dynamic"]);
        assert!(s.matches("models/props_office/vent_01.mdl"));
        assert!(!s.matches("models/props_office/vent_dynamic.mdl"));
        assert!(!s.matches("models/props/vent.mdl"));
        assert!(WordSearch::parse("  ").matches("anything"));
        assert!(s.matches2("vent", "props_office"));
        assert!(!s.matches2("vent office", "dynamic"));
    }

    #[test]
    fn word_filter_keeps_order() {
        let names = ["prop_physics", "prop_static", "func_physbox", "info_player_start"];
        assert_eq!(filter_words("prop", names.iter().copied()), [0, 1]);
        assert_eq!(filter_words("PHYS", names.iter().copied()), [0, 2]);
        assert_eq!(filter_words("prop phys", names.iter().copied()), [0]);
        assert_eq!(filter_words("  ", names.iter().copied()), [0, 1, 2, 3]);
    }

    #[test]
    fn map_names_from_test_map() {
        let state = hp_app::testing::test_state();
        let names = MapNames::build(&state.doc);
        assert_eq!(names.names(), ["main_light"]);
        assert!(names.target_exists("MAIN_LIGHT"));
        assert!(names.target_exists("main_*"));
        assert!(names.target_exists("!activator"));
        assert!(names.target_exists("light"), "class names are valid targets");
        assert!(!names.target_exists("nothing"));
        assert!(!names.target_exists(""));
        assert_eq!(names.target_classes("main_light"), ["light"]);
        assert_eq!(names.target_classes("main*"), ["light"]);
        assert!(names.target_classes("!self").is_empty());
        assert_eq!(names.entities_named("Main_Light"), [(5, "light")]);
        assert!(names.has_class("TRIGGER_MULTIPLE"));
        let mut cached = names.clone();
        cached.refresh(&state.doc);
        assert_eq!(cached.names().len(), 1);
    }
}
