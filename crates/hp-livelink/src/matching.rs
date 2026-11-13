use std::collections::{BTreeMap, BTreeSet};

use crate::protocol::GameEntity;
use crate::snapshot::{EntSnap, Snapshot};

pub const ORIGIN_TOLERANCE: f64 = 1.5;
pub const PHYSICS_TOLERANCE: f64 = 64.0;

const COMPILED_AWAY: &[&str] = &[
    "prop_static",
    "prop_detail",
    "func_detail",
    "func_instance",
    "func_instance_parms",
    "func_viscluster",
    "info_overlay",
    "info_overlay_transition",
    "info_lighting",
    "info_no_dynamic_shadow",
    "env_cubemap",
    "info_null",
    "light_environment",
    "func_ladderendpoint",
    "info_compile_parameters",
    "hammer_notes",
];

pub fn compiled_away(snap: &EntSnap) -> bool {
    let class = snap.class.as_str();
    if COMPILED_AWAY.contains(&class) {
        return true;
    }
    matches!(class, "light" | "light_spot") && snap.name().is_empty()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Matches {
    pub by_id: BTreeMap<u32, u32>,
    pub missing: Vec<u32>,
    pub by_hammerid: usize,
    pub by_name: usize,
    pub by_origin: usize,
    pub by_order: usize,
}

impl Matches {
    pub fn get(&self, id: u32) -> Option<u32> {
        self.by_id.get(&id).copied()
    }

    pub fn expected(&self) -> usize {
        self.by_id.len() + self.missing.len()
    }
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn model_of(snap: &EntSnap) -> Option<String> {
    snap.get("model").map(|m| m.trim().replace('\\', "/").to_ascii_lowercase()).filter(|m| m.ends_with(".mdl"))
}

fn is_physics(class: &str) -> bool {
    class.starts_with("prop_physics") || class == "prop_ragdoll"
}

fn brush_model_number(model: &str) -> u32 {
    model.strip_prefix('*').and_then(|n| n.parse().ok()).unwrap_or(u32::MAX)
}

pub fn match_entities(baseline: &Snapshot, game: &[GameEntity]) -> Matches {
    let mut out = Matches::default();
    let mut used: BTreeSet<usize> = BTreeSet::new();
    let mut by_hammer: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, g) in game.iter().enumerate() {
        if let Some(h) = g.hammerid {
            by_hammer.entry(h).or_insert(i);
        }
    }
    let mut pending: Vec<(u32, &EntSnap)> = Vec::new();
    for (id, snap) in &baseline.ents {
        match by_hammer.get(id) {
            Some(&i) if !used.contains(&i) => {
                used.insert(i);
                out.by_id.insert(*id, game[i].index);
                out.by_hammerid += 1;
            }
            _ => pending.push((*id, snap)),
        }
    }
    let mut rest = Vec::new();
    for (id, snap) in pending {
        let name = snap.name();
        if !name.is_empty() {
            let found: Vec<usize> = game
                .iter()
                .enumerate()
                .filter(|(i, g)| !used.contains(i) && g.class == snap.class && g.name.eq_ignore_ascii_case(name))
                .map(|(i, _)| i)
                .collect();
            if found.len() == 1 {
                used.insert(found[0]);
                out.by_id.insert(id, game[found[0]].index);
                out.by_name += 1;
                continue;
            }
        }
        rest.push((id, snap));
    }
    let mut brushes: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    let mut unmatched = Vec::new();
    for (id, snap) in rest {
        if snap.brush {
            brushes.entry(snap.class.clone()).or_default().push(id);
            continue;
        }
        let Some(origin) = snap.origin() else {
            unmatched.push((id, snap));
            continue;
        };
        let model = model_of(snap);
        let tolerance = if is_physics(&snap.class) { PHYSICS_TOLERANCE } else { ORIGIN_TOLERANCE };
        let best = game
            .iter()
            .enumerate()
            .filter(|(i, g)| !used.contains(i) && g.class == snap.class)
            .filter(|(_, g)| model.as_ref().is_none_or(|m| g.model.is_empty() || &g.model == m))
            .map(|(i, g)| (i, dist(g.origin, origin)))
            .filter(|(_, d)| *d <= tolerance)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, _)) => {
                used.insert(i);
                out.by_id.insert(id, game[i].index);
                out.by_origin += 1;
            }
            None => unmatched.push((id, snap)),
        }
    }
    for (class, ids) in brushes {
        let mut candidates: Vec<usize> = game
            .iter()
            .enumerate()
            .filter(|(i, g)| !used.contains(i) && g.class == class && g.model.starts_with('*'))
            .map(|(i, _)| i)
            .collect();
        candidates.sort_by_key(|i| brush_model_number(&game[*i].model));
        if candidates.len() == ids.len() {
            for (id, i) in ids.iter().zip(candidates) {
                used.insert(i);
                out.by_id.insert(*id, game[i].index);
                out.by_order += 1;
            }
        } else {
            for id in ids {
                if let Some(snap) = baseline.ents.get(&id) {
                    unmatched.push((id, snap));
                }
            }
        }
    }
    out.missing = unmatched.into_iter().filter(|(_, s)| !compiled_away(s)).map(|(id, _)| id).collect();
    out.missing.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(class: &str, kv: &[(&str, &str)], brush: bool) -> EntSnap {
        EntSnap {
            class: class.into(),
            kv: kv.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
            io: Vec::new(),
            brush,
        }
    }

    fn game(index: u32, class: &str, name: &str, origin: [f64; 3], model: &str, hammerid: Option<u32>) -> GameEntity {
        GameEntity { index, class: class.into(), name: name.into(), origin, model: model.into(), hammerid }
    }

    fn base(items: Vec<(u32, EntSnap)>) -> Snapshot {
        Snapshot { ents: items.into_iter().collect(), brush_sig: 0 }
    }

    #[test]
    fn hammer_ids_win_over_everything() {
        let b = base(vec![(10, snap("prop_physics", &[("origin", "0 0 0")], false))]);
        let g = vec![
            game(1, "prop_physics", "", [0.0, 0.0, 0.0], "", None),
            game(2, "prop_physics", "", [900.0, 0.0, 0.0], "", Some(10)),
        ];
        let m = match_entities(&b, &g);
        assert_eq!(m.get(10), Some(2));
        assert_eq!(m.by_hammerid, 1);
        assert!(m.missing.is_empty());
    }

    #[test]
    fn names_then_origins_then_brush_order() {
        let b = base(vec![
            (1, snap("func_door", &[("targetname", "door")], true)),
            (2, snap("prop_dynamic", &[("origin", "100 0 0"), ("model", "models/A.mdl")], false)),
            (3, snap("prop_dynamic", &[("origin", "100.5 0 0"), ("model", "models/b.mdl")], false)),
            (4, snap("func_button", &[], true)),
            (5, snap("func_button", &[], true)),
            (6, snap("info_player_start", &[("origin", "0 0 64")], false)),
            (7, snap("prop_static", &[("origin", "5 5 5")], false)),
            (8, snap("light", &[("origin", "1 1 1")], false)),
            (9, snap("light", &[("origin", "2 2 2"), ("targetname", "lamp")], false)),
        ]);
        let g = vec![
            game(20, "func_door", "door", [0.0; 3], "*3", None),
            game(21, "prop_dynamic", "", [100.0, 0.0, 0.0], "models/b.mdl", None),
            game(22, "prop_dynamic", "", [100.0, 0.0, 0.0], "models/a.mdl", None),
            game(23, "func_button", "", [0.0; 3], "*5", None),
            game(24, "func_button", "", [0.0; 3], "*4", None),
            game(25, "info_player_start", "", [0.0, 0.0, 64.5], "", None),
        ];
        let m = match_entities(&b, &g);
        assert_eq!(m.get(1), Some(20));
        assert_eq!(m.get(2), Some(22), "the model breaks the tie");
        assert_eq!(m.get(3), Some(21));
        assert_eq!(m.get(4), Some(24), "lower brush model first");
        assert_eq!(m.get(5), Some(23));
        assert_eq!(m.get(6), Some(25));
        assert_eq!(m.get(7), None);
        assert_eq!(m.missing, vec![9], "unnamed lights and static props are compiled away");
        assert_eq!((m.by_name, m.by_origin, m.by_order), (1, 3, 2));
        assert_eq!(m.expected(), 7);
    }

    #[test]
    fn physics_props_may_have_settled() {
        let b = base(vec![(1, snap("prop_physics", &[("origin", "0 0 100"), ("model", "models/c.mdl")], false))]);
        let g = vec![game(30, "prop_physics", "", [0.0, 3.0, 60.0], "models/c.mdl", None)];
        assert_eq!(match_entities(&b, &g).get(1), Some(30));
        let far = vec![game(30, "prop_physics", "", [0.0, 300.0, 60.0], "models/c.mdl", None)];
        assert_eq!(match_entities(&b, &far).missing, vec![1]);
    }

    #[test]
    fn ambiguous_names_fall_back_to_origin() {
        let b = base(vec![(1, snap("info_target", &[("targetname", "t"), ("origin", "8 0 0")], false))]);
        let g = vec![
            game(40, "info_target", "t", [0.0; 3], "", None),
            game(41, "info_target", "t", [8.0, 0.0, 0.0], "", None),
        ];
        let m = match_entities(&b, &g);
        assert_eq!(m.get(1), Some(41));
        assert_eq!(m.by_origin, 1);
    }

    #[test]
    fn brush_counts_must_agree() {
        let b = base(vec![(1, snap("func_brush", &[], true)), (2, snap("func_brush", &[], true))]);
        let g = vec![game(50, "func_brush", "", [0.0; 3], "*1", None)];
        let m = match_entities(&b, &g);
        assert!(m.by_id.is_empty());
        assert_eq!(m.missing, vec![1, 2]);
    }
}
