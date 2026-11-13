use std::collections::{BTreeMap, BTreeSet};

use crate::matching::{Matches, compiled_away};
use crate::protocol::{LightPreview, Op, StaticGhost, Target, class_allowed};
use crate::snapshot::{EntSnap, Snapshot, parse3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    Brushes,
    Lighting,
    StaticProps,
    BrushEntities,
    RemovedOutputs,
    ClassChanged,
    NotInGame,
}

impl Reason {
    pub fn id(self) -> &'static str {
        match self {
            Reason::Brushes => "brushes",
            Reason::Lighting => "lighting",
            Reason::StaticProps => "static_props",
            Reason::BrushEntities => "brush_entities",
            Reason::RemovedOutputs => "removed_outputs",
            Reason::ClassChanged => "class_changed",
            Reason::NotInGame => "not_in_game",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Live,
    Light,
    Static,
    CompileOnly,
}

pub fn kind_of(snap: &EntSnap) -> Kind {
    match snap.class.as_str() {
        "light" | "light_spot" => Kind::Light,
        "prop_static" | "prop_detail" => Kind::Static,
        "light_environment" | "env_cubemap" | "info_overlay" | "info_lighting" | "func_detail" | "func_instance"
        | "func_viscluster" | "info_null" | "hammer_notes" => Kind::CompileOnly,
        _ => Kind::Live,
    }
}

pub fn light_preview(snap: &EntSnap) -> LightPreview {
    let nums = |k: &str| -> Vec<f64> {
        snap.get(k)
            .map(|v| v.split_whitespace().filter_map(|p| p.parse::<f64>().ok()).filter(|f| f.is_finite()).collect())
            .unwrap_or_default()
    };
    let one = |k: &str, d: f64| nums(k).first().copied().unwrap_or(d);
    let l = nums("_light");
    let color = [
        l.first().copied().unwrap_or(255.0).clamp(0.0, 255.0),
        l.get(1).copied().unwrap_or(255.0).clamp(0.0, 255.0),
        l.get(2).copied().unwrap_or(255.0).clamp(0.0, 255.0),
    ];
    let brightness = l.get(3).copied().unwrap_or(200.0).clamp(0.0, 100_000.0);
    let spot = snap.class == "light_spot";
    let mut angles = snap.angles();
    if spot && let Some(p) = nums("pitch").first() {
        angles[0] = -p;
    }
    let outer = one("_cone", 45.0).clamp(1.0, 89.0);
    let inner = one("_inner_cone", 30.0).clamp(0.0, outer);
    let distance = [one("_distance", 0.0), one("_zero_percent_distance", 0.0)]
        .into_iter()
        .find(|d| *d > 0.0)
        .unwrap_or_else(|| (brightness.sqrt() * 24.0).clamp(128.0, 1024.0))
        .clamp(16.0, 8192.0);
    LightPreview {
        kind: if spot { "spot" } else { "point" }.to_owned(),
        origin: snap.origin().unwrap_or([0.0; 3]),
        angles,
        color,
        brightness,
        inner,
        outer,
        distance,
    }
}

pub fn static_ghost(snap: &EntSnap) -> StaticGhost {
    let color = snap.get("rendercolor").and_then(parse3).unwrap_or([255.0; 3]);
    let alpha = snap.get("renderamt").and_then(|a| a.trim().parse::<f64>().ok()).unwrap_or(255.0);
    let c = |v: f64| v.clamp(0.0, 255.0) as u8;
    StaticGhost {
        model: snap.get("model").unwrap_or("").replace('\\', "/"),
        origin: snap.origin().unwrap_or([0.0; 3]),
        angles: snap.angles(),
        skin: snap.get("skin").and_then(|s| s.trim().parse().ok()).unwrap_or(0),
        color: [c(color[0]), c(color[1]), c(color[2]), c(alpha)],
        scale: snap.get("modelscale").and_then(|s| s.trim().parse().ok()).filter(|s: &f64| *s > 0.0).unwrap_or(1.0),
    }
}

fn kv_diff(last: &EntSnap, want: &EntSnap) -> BTreeMap<String, String> {
    want.kv.iter().filter(|(k, v)| last.kv.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn added_outputs(last: &EntSnap, want: &EntSnap) -> Vec<(String, String)> {
    let mut left: Vec<&(String, String)> = last.io.iter().collect();
    let mut out = Vec::new();
    for c in &want.io {
        if let Some(pos) = left.iter().position(|l| *l == c) {
            left.remove(pos);
        } else {
            out.push(c.clone());
        }
    }
    out
}

fn removes_outputs(last: &EntSnap, want: &EntSnap) -> bool {
    let mut left: Vec<&(String, String)> = want.io.iter().collect();
    for c in &last.io {
        match left.iter().position(|l| *l == c) {
            Some(pos) => {
                left.remove(pos);
            }
            None => return true,
        }
    }
    false
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Planner {
    applied: BTreeMap<u32, Option<EntSnap>>,
    owned: BTreeSet<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub ops: Vec<Op>,
    pub skipped: Vec<(u32, String)>,
}

impl Planner {
    pub fn reset(&mut self) {
        self.applied.clear();
        self.owned.clear();
    }

    pub fn is_clean(&self) -> bool {
        self.applied.is_empty()
    }

    fn last<'a>(&'a self, baseline: &'a Snapshot, id: u32) -> Option<&'a EntSnap> {
        match self.applied.get(&id) {
            Some(state) => state.as_ref(),
            None => baseline.ents.get(&id),
        }
    }

    pub fn target_of(&self, baseline: &Snapshot, matches: &Matches, id: u32) -> Option<Target> {
        self.target(baseline, matches, id)
    }

    fn target(&self, baseline: &Snapshot, matches: &Matches, id: u32) -> Option<Target> {
        if self.owned.contains(&id) {
            return Some(Target::Own(id));
        }
        if !baseline.ents.contains_key(&id) {
            return None;
        }
        matches.get(id).map(Target::Map)
    }

    pub fn plan(&mut self, baseline: &Snapshot, current: &Snapshot, matches: &Matches) -> Plan {
        let mut ids: BTreeSet<u32> = baseline.ents.keys().copied().collect();
        ids.extend(current.ents.keys().copied());
        ids.extend(self.applied.keys().copied());
        let mut plan = Plan::default();
        for id in ids {
            let last = self.last(baseline, id).cloned();
            let want = current.ents.get(&id);
            if last.as_ref() == want {
                continue;
            }
            self.transition(baseline, matches, id, last.as_ref(), want, &mut plan);
            if baseline.ents.get(&id) == want && !self.owned.contains(&id) {
                self.applied.remove(&id);
            } else {
                self.applied.insert(id, want.cloned());
            }
        }
        plan
    }

    fn transition(
        &mut self,
        baseline: &Snapshot,
        matches: &Matches,
        id: u32,
        last: Option<&EntSnap>,
        want: Option<&EntSnap>,
        plan: &mut Plan,
    ) {
        let kind = want.or(last).map(kind_of).unwrap_or(Kind::Live);
        match kind {
            Kind::CompileOnly => {}
            Kind::Light => {
                let light = want.filter(|w| baseline.ents.get(&id) != Some(*w)).map(light_preview);
                plan.ops.push(Op::Light { key: id, light });
            }
            Kind::Static => {
                let base = baseline.ents.get(&id);
                let ghost = want.filter(|w| base != Some(*w)).map(static_ghost);
                let removed_at = if want.is_none() { base.and_then(EntSnap::origin) } else { None };
                plan.ops.push(Op::Ghost { key: id, ghost, removed_at });
            }
            Kind::Live => self.live(baseline, matches, id, last, want, plan),
        }
    }

    fn live(
        &mut self,
        baseline: &Snapshot,
        matches: &Matches,
        id: u32,
        last: Option<&EntSnap>,
        want: Option<&EntSnap>,
        plan: &mut Plan,
    ) {
        match (last, want) {
            (None, Some(w)) => {
                if w.brush {
                    plan.skipped.push((id, Reason::BrushEntities.id().to_owned()));
                    return;
                }
                if !class_allowed(&w.class) {
                    plan.skipped.push((id, format!("class {} is not allowed", w.class)));
                    return;
                }
                self.owned.insert(id);
                plan.ops.push(Op::Create { key: id, class: w.class.clone(), kv: w.kv.clone(), io: w.io.clone() });
            }
            (Some(l), None) => match self.target(baseline, matches, id) {
                Some(target) => {
                    self.owned.remove(&id);
                    plan.ops.push(Op::Remove { key: id, target });
                }
                None if !compiled_away(l) => plan.skipped.push((id, Reason::NotInGame.id().to_owned())),
                None => {}
            },
            (Some(l), Some(w)) => {
                if l.class != w.class {
                    plan.skipped.push((id, Reason::ClassChanged.id().to_owned()));
                    return;
                }
                let Some(target) = self.target(baseline, matches, id) else {
                    if !compiled_away(w) {
                        plan.skipped.push((id, Reason::NotInGame.id().to_owned()));
                    }
                    return;
                };
                let kv = kv_diff(l, w);
                if !kv.is_empty() {
                    plan.ops.push(Op::Set { key: id, target, kv });
                }
                let add = added_outputs(l, w);
                if !add.is_empty() {
                    plan.ops.push(Op::Io { key: id, target, add });
                }
            }
            (None, None) => {}
        }
    }
}

pub fn needs_compile(baseline: &Snapshot, current: &Snapshot, matches: Option<&Matches>) -> BTreeSet<Reason> {
    let mut out = BTreeSet::new();
    if baseline.brush_sig != current.brush_sig {
        out.insert(Reason::Brushes);
    }
    let mut ids: BTreeSet<u32> = baseline.ents.keys().copied().collect();
    ids.extend(current.ents.keys().copied());
    for id in ids {
        let (b, c) = (baseline.ents.get(&id), current.ents.get(&id));
        if b == c {
            continue;
        }
        let snap = c.or(b).expect("one side exists");
        match kind_of(snap) {
            Kind::Light | Kind::CompileOnly => {
                out.insert(Reason::Lighting);
            }
            Kind::Static => {
                out.insert(Reason::StaticProps);
            }
            Kind::Live => {
                if snap.brush && (b.is_none() || c.is_none()) {
                    out.insert(Reason::BrushEntities);
                }
                if let (Some(b), Some(c)) = (b, c) {
                    if b.class != c.class {
                        out.insert(Reason::ClassChanged);
                    }
                    if removes_outputs(b, c) {
                        out.insert(Reason::RemovedOutputs);
                    }
                }
                if let (Some(m), Some(b)) = (matches, b)
                    && m.get(id).is_none()
                    && !compiled_away(b)
                {
                    out.insert(Reason::NotInGame);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(class: &str, kv: &[(&str, &str)]) -> EntSnap {
        EntSnap {
            class: class.into(),
            kv: kv.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
            io: Vec::new(),
            brush: false,
        }
    }

    fn snapshot(items: Vec<(u32, EntSnap)>) -> Snapshot {
        Snapshot { ents: items.into_iter().collect(), brush_sig: 1 }
    }

    fn matches(pairs: &[(u32, u32)]) -> Matches {
        Matches { by_id: pairs.iter().copied().collect(), ..Matches::default() }
    }

    #[test]
    fn moves_become_set_ops_once() {
        let base = snapshot(vec![(1, snap("prop_physics", &[("origin", "0 0 0"), ("model", "models/a.mdl")]))]);
        let mut cur = base.clone();
        cur.ents.get_mut(&1).unwrap().kv.insert("origin".into(), "64 0 0".into());
        let m = matches(&[(1, 77)]);
        let mut p = Planner::default();
        let plan = p.plan(&base, &cur, &m);
        let mut kv = BTreeMap::new();
        kv.insert("origin".to_owned(), "64 0 0".to_owned());
        assert_eq!(plan.ops, vec![Op::Set { key: 1, target: Target::Map(77), kv }]);
        assert!(p.plan(&base, &cur, &m).ops.is_empty(), "nothing new to send");
        let back = p.plan(&base, &base, &m);
        assert_eq!(back.ops.len(), 1, "undo sends the old origin back");
        assert!(p.is_clean());
    }

    #[test]
    fn new_entities_are_created_then_owned() {
        let base = snapshot(vec![]);
        let mut cur = snapshot(vec![(5, snap("prop_physics", &[("origin", "1 2 3"), ("model", "models/a.mdl")]))]);
        cur.ents.get_mut(&5).unwrap().io.push(("OnBreak".into(), "d,Open,,0,-1".into()));
        let m = Matches::default();
        let mut p = Planner::default();
        let plan = p.plan(&base, &cur, &m);
        assert!(
            matches!(&plan.ops[0], Op::Create { key: 5, class, io, .. } if class == "prop_physics" && io.len() == 1)
        );
        cur.ents.get_mut(&5).unwrap().kv.insert("skin".into(), "2".into());
        let plan = p.plan(&base, &cur, &m);
        assert!(matches!(&plan.ops[0], Op::Set { target: Target::Own(5), kv, .. } if kv.len() == 1));
        assert_eq!(p.target_of(&base, &m, 5), Some(Target::Own(5)));
        assert_eq!(p.target_of(&base, &m, 6), None, "an id that was never created has no target");
        let plan = p.plan(&base, &base, &m);
        assert_eq!(plan.ops, vec![Op::Remove { key: 5, target: Target::Own(5) }]);
        assert!(p.is_clean());
        assert_eq!(p.target_of(&base, &m, 5), None);
    }

    #[test]
    fn deleting_and_undeleting_map_entities() {
        let base = snapshot(vec![(2, snap("prop_dynamic", &[("origin", "0 0 0"), ("model", "models/a.mdl")]))]);
        let empty = snapshot(vec![]);
        let m = matches(&[(2, 40)]);
        let mut p = Planner::default();
        assert_eq!(p.plan(&base, &empty, &m).ops, vec![Op::Remove { key: 2, target: Target::Map(40) }]);
        let plan = p.plan(&base, &base, &m);
        assert!(matches!(&plan.ops[0], Op::Create { key: 2, .. }), "the removed entity comes back as a copy");
        let mut moved = base.clone();
        moved.ents.get_mut(&2).unwrap().kv.insert("origin".into(), "9 9 9".into());
        let plan = p.plan(&base, &moved, &m);
        assert!(matches!(&plan.ops[0], Op::Set { target: Target::Own(2), .. }));
    }

    #[test]
    fn lights_and_static_props_are_previews() {
        let base = snapshot(vec![
            (3, snap("light", &[("origin", "0 0 64"), ("_light", "255 255 255 200")])),
            (4, snap("prop_static", &[("origin", "0 0 0"), ("model", "models/a.mdl")])),
        ]);
        let mut cur = base.clone();
        cur.ents.get_mut(&3).unwrap().kv.insert("_light".into(), "255 0 0 400".into());
        cur.ents.remove(&4);
        let mut p = Planner::default();
        let plan = p.plan(&base, &cur, &Matches::default());
        match &plan.ops[0] {
            Op::Light { key: 3, light: Some(l) } => {
                assert_eq!(l.color, [255.0, 0.0, 0.0]);
                assert_eq!(l.brightness, 400.0);
                assert_eq!(l.kind, "point");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(plan.ops[1], Op::Ghost { key: 4, ghost: None, removed_at: Some([0.0; 3]) });
        let plan = p.plan(&base, &base, &Matches::default());
        assert_eq!(
            plan.ops,
            vec![Op::Light { key: 3, light: None }, Op::Ghost { key: 4, ghost: None, removed_at: None }]
        );
        let reasons = needs_compile(&base, &cur, None);
        assert!(reasons.contains(&Reason::Lighting) && reasons.contains(&Reason::StaticProps));
    }

    #[test]
    fn spot_lights_read_cone_and_pitch() {
        let s = snap(
            "light_spot",
            &[
                ("origin", "0 0 0"),
                ("angles", "0 90 0"),
                ("pitch", "-60"),
                ("_cone", "50"),
                ("_inner_cone", "70"),
                ("_light", "10 20 30"),
            ],
        );
        let l = light_preview(&s);
        assert_eq!(l.kind, "spot");
        assert_eq!(l.angles, [60.0, 90.0, 0.0]);
        assert_eq!((l.outer, l.inner), (50.0, 50.0));
        assert_eq!(l.color, [10.0, 20.0, 30.0]);
        assert_eq!(l.brightness, 200.0);
        assert!((l.distance - 339.41).abs() < 0.01, "{}", l.distance);
        let far = snap("light", &[("_distance", "300")]);
        assert_eq!(light_preview(&far).distance, 300.0);
    }

    #[test]
    fn outputs_are_added_and_removals_need_compile() {
        let mut a = snap("func_button", &[("targetname", "b")]);
        a.io.push(("OnPressed".into(), "door,Open,,0,-1".into()));
        let mut b = a.clone();
        b.io.push(("OnPressed".into(), "lamp,TurnOn,,0,-1".into()));
        let base = snapshot(vec![(1, a.clone())]);
        let cur = snapshot(vec![(1, b.clone())]);
        let mut p = Planner::default();
        let plan = p.plan(&base, &cur, &matches(&[(1, 9)]));
        assert_eq!(plan.ops, vec![Op::Io { key: 1, target: Target::Map(9), add: vec![b.io[1].clone()] }]);
        let fewer = snapshot(vec![(1, snap("func_button", &[("targetname", "b")]))]);
        assert!(needs_compile(&base, &fewer, None).contains(&Reason::RemovedOutputs));
    }

    #[test]
    fn unmatched_and_forbidden_entities_are_skipped() {
        let base = snapshot(vec![(1, snap("info_target", &[("origin", "0 0 0")]))]);
        let mut cur = base.clone();
        cur.ents.get_mut(&1).unwrap().kv.insert("origin".into(), "1 0 0".into());
        cur.ents.insert(2, snap("point_servercommand", &[]));
        let mut p = Planner::default();
        let plan = p.plan(&base, &cur, &Matches::default());
        assert!(plan.ops.is_empty());
        assert_eq!(plan.skipped.len(), 2);
        assert!(needs_compile(&base, &cur, Some(&Matches::default())).contains(&Reason::NotInGame));
        let mut brushy = cur.clone();
        brushy.brush_sig = 2;
        assert!(needs_compile(&base, &brushy, None).contains(&Reason::Brushes));
    }

    #[test]
    fn static_ghosts_carry_model_skin_and_color() {
        let g = static_ghost(&snap(
            "prop_static",
            &[
                ("model", "models\\x.mdl"),
                ("origin", "1 2 3"),
                ("angles", "0 45 0"),
                ("skin", "2"),
                ("rendercolor", "255 0 0"),
                ("modelscale", "2"),
            ],
        ));
        assert_eq!(g.model, "models/x.mdl");
        assert_eq!(g.skin, 2);
        assert_eq!(g.color, [255, 0, 0, 255]);
        assert_eq!(g.angles, [0.0, 45.0, 0.0]);
        assert_eq!(g.scale, 2.0);
    }
}
