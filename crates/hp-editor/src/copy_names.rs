use std::collections::HashSet;

use crate::ops::Fragment;
use crate::problems::TargetKeyFn;

pub(crate) const COMMON_TARGET_KEYS: &[&str] = &["target", "parentname", "lasertarget", "filtername", "damagefilter"];

const MAX_DIGITS: usize = 9;
const MAX_DELTA: u64 = 1_000_000;

pub(crate) fn apply_renames(frag: &mut Fragment, renames: &[(String, String)], is_target_key: Option<TargetKeyFn<'_>>) {
    if renames.is_empty() {
        return;
    }
    let renamed = |name: &str| renames.iter().find(|(old, _)| old.eq_ignore_ascii_case(name.trim())).map(|(_, n)| n);
    let names_entity = |class: &str, key: &str| {
        if key.eq_ignore_ascii_case("targetname") || key.eq_ignore_ascii_case("classname") {
            return false;
        }
        is_target_key
            .and_then(|f| f(class, key))
            .unwrap_or_else(|| COMMON_TARGET_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key)))
    };
    for e in &mut frag.entities {
        for c in &mut e.connections {
            if let Some(new) = renamed(&c.target) {
                c.target.clone_from(new);
            }
        }
        let class = e.classname.clone();
        for (key, value) in &mut e.props {
            if !names_entity(&class, key) {
                continue;
            }
            let (name, rest) = match value.split_once(',') {
                Some((n, r)) if key.eq_ignore_ascii_case("parentname") => (n.to_owned(), Some(r.to_owned())),
                _ => (value.clone(), None),
            };
            if let Some(new) = renamed(&name) {
                *value = match rest {
                    Some(r) => format!("{new},{r}"),
                    None => new.clone(),
                };
            }
        }
    }
}

struct Numbered<'a> {
    prefix: &'a str,
    value: u64,
    width: usize,
}

fn split_number(name: &str) -> Option<Numbered<'_>> {
    let digits = name.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > MAX_DIGITS || digits == name.len() {
        return None;
    }
    let (prefix, tail) = name.split_at(name.len() - digits);
    let value = tail.parse().ok()?;
    let width = if tail.starts_with('0') && digits > 1 { digits } else { 0 };
    Some(Numbered { prefix, value, width })
}

fn numbered_name(n: &Numbered<'_>, delta: u64) -> String {
    format!("{}{:0width$}", n.prefix, n.value + delta, width = n.width)
}

pub(crate) fn increment_names(frag: &mut Fragment, used: &mut HashSet<String>, is_target_key: Option<TargetKeyFn<'_>>) {
    let mut names: Vec<String> = Vec::new();
    for e in &frag.entities {
        let Some(name) = e.get("targetname").map(str::trim).filter(|n| !n.is_empty()) else { continue };
        if !names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            names.push(name.to_owned());
        }
    }
    if names.is_empty() {
        return;
    }
    let taken = |used: &HashSet<String>, n: &str| used.contains(&n.to_ascii_lowercase());
    let numbered: Vec<(usize, Numbered<'_>)> =
        names.iter().enumerate().filter_map(|(i, n)| split_number(n).map(|s| (i, s))).collect();
    let numbered_collides = numbered.iter().any(|(i, _)| taken(used, &names[*i]));
    let mut result: Vec<String> = names.clone();
    let mut fresh: HashSet<String> = HashSet::new();
    if numbered_collides {
        let mut delta = 1u64;
        let chosen = loop {
            let proposal: Vec<String> = numbered.iter().map(|(_, n)| numbered_name(n, delta)).collect();
            let free = proposal.iter().all(|p| !taken(used, p));
            let distinct = {
                let mut seen = HashSet::new();
                proposal.iter().all(|p| seen.insert(p.to_ascii_lowercase()))
            };
            if free && distinct {
                break Some(proposal);
            }
            delta += 1;
            if delta > MAX_DELTA {
                break None;
            }
        };
        if let Some(proposal) = chosen {
            for ((i, _), name) in numbered.iter().zip(proposal) {
                fresh.insert(name.to_ascii_lowercase());
                result[*i] = name;
            }
        }
    }
    for (i, name) in names.iter().enumerate() {
        let in_numbered = numbered.iter().any(|(j, _)| *j == i);
        if in_numbered && result[i] != *name {
            continue;
        }
        if !taken(used, name) && !fresh.contains(&name.to_ascii_lowercase()) {
            fresh.insert(name.to_ascii_lowercase());
            continue;
        }
        let mut k = 2u64;
        let new = loop {
            let candidate = match split_number(name) {
                Some(n) => numbered_name(&n, k),
                None => format!("{name}{k}"),
            };
            if !taken(used, &candidate) && !fresh.contains(&candidate.to_ascii_lowercase()) {
                break candidate;
            }
            k += 1;
            if k > MAX_DELTA {
                break name.clone();
            }
        };
        fresh.insert(new.to_ascii_lowercase());
        result[i] = new;
    }
    let renames: Vec<(String, String)> = names.into_iter().zip(result).filter(|(old, new)| old != new).collect();
    for e in &mut frag.entities {
        let Some(name) = e.get("targetname").map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned) else {
            continue;
        };
        if let Some((_, new)) = renames.iter().find(|(old, _)| old.eq_ignore_ascii_case(&name)) {
            e.set("targetname", new.clone());
        }
    }
    apply_renames(frag, &renames, is_target_key);
    used.extend(fresh);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(class: &str, name: &str, extra: &[(&str, &str)]) -> hp_vmf::Entity {
        let mut e = hp_vmf::Entity { classname: class.to_owned(), ..hp_vmf::Entity::default() };
        e.set("targetname", name);
        for (k, v) in extra {
            e.set(k, *v);
        }
        e
    }

    fn frag(entities: Vec<hp_vmf::Entity>) -> Fragment {
        Fragment { entities, ..Fragment::default() }
    }

    fn names(f: &Fragment) -> Vec<String> {
        f.entities.iter().filter_map(|e| e.get("targetname").map(str::to_owned)).collect()
    }

    fn used(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_ascii_lowercase()).collect()
    }

    #[test]
    fn linked_set_increments_together() {
        let mut f =
            frag(vec![entity("func_door", "porte1", &[]), entity("func_button", "btn1", &[("target", "porte1")])]);
        let mut u = used(&["porte1", "btn1"]);
        increment_names(&mut f, &mut u, None);
        assert_eq!(names(&f), vec!["porte2", "btn2"]);
        assert_eq!(f.entities[1].get("target"), Some("porte2"));
        let mut g = frag(vec![entity("func_door", "porte1", &[]), entity("func_button", "btn1", &[])]);
        increment_names(&mut g, &mut u, None);
        assert_eq!(names(&g), vec!["porte3", "btn3"]);
    }

    #[test]
    fn skips_numbers_in_use_with_a_common_delta() {
        let mut f = frag(vec![entity("func_door", "porte1", &[]), entity("func_button", "btn1", &[])]);
        let mut u = used(&["porte1", "btn1", "porte2", "btn4"]);
        increment_names(&mut f, &mut u, None);
        assert_eq!(names(&f), vec!["porte3", "btn3"]);
    }

    #[test]
    fn plain_names_change_only_on_collision() {
        let mut f = frag(vec![entity("info_target", "lamp", &[]), entity("info_target", "free", &[])]);
        let mut u = used(&["lamp"]);
        increment_names(&mut f, &mut u, None);
        assert_eq!(names(&f), vec!["lamp2", "free"]);
        let mut cut = frag(vec![entity("info_target", "lamp", &[])]);
        increment_names(&mut cut, &mut used(&[]), None);
        assert_eq!(names(&cut), vec!["lamp"]);
    }

    #[test]
    fn padding_connections_and_parentname() {
        let mut door = entity("func_door", "door01", &[]);
        door.connections.push(hp_vmf::Connection {
            output: "OnOpen".into(),
            target: "door01".into(),
            ..hp_vmf::Connection::default()
        });
        let child = entity("prop_dynamic", "", &[("parentname", "door01,attach")]);
        let mut f = frag(vec![door, child]);
        increment_names(&mut f, &mut used(&["door01"]), None);
        assert_eq!(f.entities[0].get("targetname"), Some("door02"));
        assert_eq!(f.entities[0].connections[0].target, "door02");
        assert_eq!(f.entities[1].get("parentname"), Some("door02,attach"));
    }
}
