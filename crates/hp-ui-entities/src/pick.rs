use glam::DVec3;
use hp_app::runtime::{AssetKind, AssetPick};
use hp_app::state::{PickKind, PickRequest, PickResult, RightTab};
use hp_app::{AppState, Severity, tf};
use hp_editor::{Document, EntityQuery, Id};

use crate::edit::{Op, apply};
use crate::values::{fmt_num_rounded, format_angles, parse_angles};
use crate::view::Target;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickPurpose {
    Target,
    Faces { add: bool },
    Aim { negative_pitch: bool },
}

impl PickPurpose {
    fn kind(self) -> PickKind {
        match self {
            PickPurpose::Target => PickKind::Entity,
            PickPurpose::Faces { .. } => PickKind::Faces,
            PickPurpose::Aim { .. } => PickKind::Angles,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Pending {
    requester: String,
    target: Target,
    key: String,
    purpose: PickPurpose,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NamePrompt {
    pub entity: Id,
    pub class: String,
    pub target: Target,
    pub key: String,
    pub name: String,
    pub focus: bool,
}

#[derive(Debug)]
pub(crate) struct ViewPicks {
    salt: String,
    pending: Option<Pending>,
    pub prompt: Option<NamePrompt>,
}

impl ViewPicks {
    pub fn new(salt: &str) -> ViewPicks {
        ViewPicks { salt: salt.to_owned(), pending: None, prompt: None }
    }

    fn requester(&self, key: &str) -> String {
        format!("entities:{}:{}", self.salt, key.to_ascii_lowercase())
    }

    pub fn start(&mut self, pick: &mut Option<PickRequest>, target: Target, key: &str, purpose: PickPurpose) {
        let requester = self.requester(key);
        *pick = Some(PickRequest { kind: purpose.kind(), requester: requester.clone(), result: None });
        self.pending = Some(Pending { requester, target, key: key.to_owned(), purpose });
    }

    pub fn is_waiting(&self, pick: &Option<PickRequest>, key: &str) -> bool {
        let Some(p) = &self.pending else { return false };
        p.key.eq_ignore_ascii_case(key)
            && pick.as_ref().is_some_and(|r| r.requester == p.requester && r.result.is_none())
    }

    pub fn cancel(&mut self, pick: &mut Option<PickRequest>, key: &str) {
        if self.is_waiting(pick, key) {
            *pick = None;
            self.pending = None;
        }
    }

    pub fn poll(&mut self, doc: &Document, pick: &mut Option<PickRequest>, ops: &mut Vec<Op>) {
        let Some(p) = &self.pending else { return };
        let Some(req) = pick.as_ref().filter(|r| r.requester == p.requester) else {
            self.pending = None;
            return;
        };
        let Some(result) = req.result.clone() else { return };
        *pick = None;
        let Some(p) = self.pending.take() else { return };
        match result {
            PickResult::Cancelled => {}
            PickResult::Entity(id) => {
                let Some(e) = doc.scene().entity(id).map(|n| &n.entity) else { return };
                match e.get("targetname").map(str::trim).filter(|n| !n.is_empty()) {
                    Some(name) => {
                        ops.push(Op::SetKey { target: p.target, key: p.key, value: name.to_owned(), merge: false })
                    }
                    None => {
                        self.prompt = Some(NamePrompt {
                            entity: id,
                            class: e.classname.clone(),
                            target: p.target,
                            name: suggest_name(doc, &e.classname),
                            key: p.key,
                            focus: true,
                        });
                    }
                }
            }
            PickResult::Faces(faces) => {
                let PickPurpose::Faces { add } = p.purpose else { return };
                let picked: Vec<Id> = faces
                    .iter()
                    .filter_map(|(solid, side)| doc.scene().solid(*solid)?.solid.sides.get(*side).map(|s| s.id))
                    .collect();
                let current = if add { current_value(doc, &p.target, &p.key) } else { String::new() };
                let value = merge_sides(&current, &picked);
                ops.push(Op::SetKey { target: p.target, key: p.key, value, merge: false });
            }
            PickResult::Point(point) => {
                let PickPurpose::Aim { negative_pitch } = p.purpose else { return };
                let values = aim_values(doc, &p.target, &p.key, point, negative_pitch);
                if !values.is_empty() {
                    ops.push(Op::SetPerEntity { values });
                }
            }
        }
    }
}

fn current_value(doc: &Document, target: &Target, key: &str) -> String {
    let e = match target {
        Target::World => Some(doc.scene().world()),
        Target::Entities(ids) => ids.first().and_then(|id| doc.scene().entity(*id)).map(|n| &n.entity),
    };
    e.and_then(|e| e.get(key)).unwrap_or("").to_owned()
}

pub(crate) fn merge_sides(current: &str, picked: &[Id]) -> String {
    let mut ids = hp_editor::side_index::parse_side_list(current);
    for id in picked {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    hp_editor::side_index::format_side_list(&ids)
}

pub(crate) fn suggest_name(doc: &Document, class: &str) -> String {
    let base: String = class
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let base = if base.is_empty() { "entity".to_owned() } else { base };
    let used: std::collections::HashSet<String> = doc
        .scene()
        .entities()
        .values()
        .filter_map(|n| n.entity.get("targetname"))
        .map(|n| n.trim().to_ascii_lowercase())
        .collect();
    (1..=used.len() + 1).map(|i| format!("{base}_{i}")).find(|n| !used.contains(n)).unwrap_or(base)
}

fn aim_values(
    doc: &Document,
    target: &Target,
    key: &str,
    point: DVec3,
    negative_pitch: bool,
) -> Vec<(Id, Vec<(String, String)>)> {
    let Target::Entities(ids) = target else { return Vec::new() };
    ids.iter()
        .filter_map(|id| {
            let e = &doc.scene().entity(*id)?.entity;
            let keys = aim_keys(e, key, point, negative_pitch)?;
            Some((*id, keys))
        })
        .collect()
}

pub fn is_vrad_light(class: &str) -> bool {
    ["light_spot", "light_environment", "light_directional"].iter().any(|c| class.trim().eq_ignore_ascii_case(c))
}

pub fn aim_keys(e: &hp_vmf::Entity, key: &str, point: DVec3, negative_pitch: bool) -> Option<Vec<(String, String)>> {
    let (pitch, yaw) = aim_angles(e.origin()?, point)?;
    let up = -pitch + 0.0;
    let vrad = is_vrad_light(&e.classname);
    let angles_key = if negative_pitch { "angles" } else { key };
    let roll = e.get(angles_key).and_then(parse_angles).map_or(0.0, |a| a.z);
    let mut out = vec![(angles_key.to_owned(), format_angles(DVec3::new(pitch, yaw, roll)))];
    let has_pitch_key = e.get("pitch").is_some_and(|v| !v.trim().is_empty());
    if negative_pitch {
        out.push((key.to_owned(), fmt_num_rounded(up, 2)));
    } else if vrad || has_pitch_key {
        out.push(("pitch".to_owned(), fmt_num_rounded(up, 2)));
    }
    let angle_override = e.get("angle").and_then(|v| v.trim().parse::<f64>().ok()).is_some_and(|v| v != 0.0);
    if angle_override {
        out.push(("angle".to_owned(), fmt_num_rounded(yaw, 2)));
    }
    Some(out)
}

pub(crate) fn aim_angles(from: DVec3, to: DVec3) -> Option<(f64, f64)> {
    let d = to - from;
    if !d.is_finite() || d.length() < 1e-6 {
        return None;
    }
    let yaw = crate::values::normalize_yaw(d.y.atan2(d.x).to_degrees());
    let pitch = -d.z.atan2(d.truncate().length()).to_degrees();
    let round = |v: f64| (v * 100.0).round() / 100.0 + 0.0;
    Some((round(pitch), round(crate::values::normalize_yaw(round(yaw)))))
}

pub fn entities_matching_target(doc: &Document, target: &str) -> Vec<Id> {
    let t = target.trim();
    if t.is_empty() || t.starts_with('!') {
        return Vec::new();
    }
    let by_name = EntityQuery { targetname: Some(t.to_owned()), include_hidden: true, ..EntityQuery::default() };
    let by_class = EntityQuery { classname: Some(t.to_owned()), include_hidden: true, ..EntityQuery::default() };
    let mut found: std::collections::HashSet<Id> = doc.find_entities(&by_name).into_iter().collect();
    found.extend(doc.find_entities(&by_class));
    doc.scene().entities().keys().copied().filter(|id| found.contains(id)).collect()
}

pub(crate) fn mark(doc: &mut Document, requested: &mut Vec<hp_app::Action>, ids: &[Id]) {
    let objs: Vec<hp_editor::ObjRef> = ids.iter().map(|id| hp_editor::ObjRef::Entity(*id)).collect();
    doc.set_selection(&objs);
    for a in [hp_app::Action::Center2dOnSelection, hp_app::Action::Center3dOnSelection] {
        if !requested.contains(&a) {
            requested.push(a);
        }
    }
}

pub(crate) fn start_asset_pick(
    state_pick: &mut Option<AssetPick>,
    kind: AssetKind,
    target: &Target,
    key: &str,
    label: String,
    current: &str,
) {
    *state_pick = Some(AssetPick {
        kind,
        entities: target.ids().to_vec(),
        world: target.is_world(),
        key: key.to_owned(),
        label,
        current: current.to_owned(),
        result: None,
    });
}

pub fn asset_value(kind: AssetKind, chosen: &str) -> String {
    let v = chosen.trim();
    match kind {
        AssetKind::Sprite
            if !v.is_empty()
                && !v.to_ascii_lowercase().ends_with(".vmt")
                && !v.to_ascii_lowercase().ends_with(".spr") =>
        {
            format!("{v}.vmt")
        }
        _ => v.to_owned(),
    }
}

pub fn finish_asset_pick(state: &mut AppState) -> bool {
    let Some(pick) = state.runtime.entities.asset_pick.as_ref() else { return false };
    let Some(chosen) = pick.result.clone() else { return false };
    let Some(pick) = state.runtime.entities.asset_pick.take() else { return false };
    let target = if pick.world { Target::World } else { Target::Entities(pick.entities.clone()) };
    let target = match target {
        Target::Entities(ids) => {
            let alive: Vec<Id> = ids.into_iter().filter(|id| state.doc.scene().entity(*id).is_some()).collect();
            if alive.is_empty() {
                return false;
            }
            Target::Entities(alive)
        }
        t => t,
    };
    let value = asset_value(pick.kind, &chosen);
    let op = Op::SetKey { target, key: pick.key.clone(), value, merge: false };
    let written = match apply(&mut state.doc, None, op) {
        Ok(_) => true,
        Err(e) => {
            state.notify(Severity::Error, tf!("entities.edit_failed", e));
            false
        }
    };
    state.ui.right_tab = RightTab::Entity;
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        hp_app::testing::test_state()
    }

    #[test]
    fn entity_pick_writes_the_name_or_asks_for_one() {
        let mut s = state();
        let mut picks = ViewPicks::new("t");
        let target = Target::Entities(vec![6]);
        picks.start(&mut s.pick, target.clone(), "filtername", PickPurpose::Target);
        assert!(picks.is_waiting(&s.pick, "FilterName"));
        let mut ops = Vec::new();
        picks.poll(&s.doc, &mut s.pick, &mut ops);
        assert!(ops.is_empty(), "no result yet");
        if let Some(p) = s.pick.as_mut() {
            p.result = Some(PickResult::Entity(5));
        }
        picks.poll(&s.doc, &mut s.pick, &mut ops);
        assert_eq!(s.pick, None);
        assert_eq!(
            ops,
            [Op::SetKey { target: target.clone(), key: "filtername".into(), value: "main_light".into(), merge: false }]
        );
        picks.start(&mut s.pick, target, "filtername", PickPurpose::Target);
        if let Some(p) = s.pick.as_mut() {
            p.result = Some(PickResult::Entity(4));
        }
        let mut ops = Vec::new();
        picks.poll(&s.doc, &mut s.pick, &mut ops);
        assert!(ops.is_empty());
        let prompt = picks.prompt.clone().expect("prompt");
        assert_eq!(prompt.entity, 4);
        assert_eq!(prompt.name, "info_player_start_1");
    }

    #[test]
    fn naming_and_setting_is_one_step() {
        let mut s = state();
        let op = Op::NameAndSet {
            named: 4,
            name: "spawn".into(),
            target: Target::Entities(vec![6]),
            key: "filtername".into(),
        };
        apply(&mut s.doc, None, op).expect("apply");
        assert_eq!(s.doc.scene().entity(4).unwrap().entity.get("targetname"), Some("spawn"));
        assert_eq!(s.doc.scene().entity(6).unwrap().entity.get("filtername"), Some("spawn"));
        assert_eq!(s.doc.history_len().0, 1);
        let bad = Op::NameAndSet { named: 4, name: " ".into(), target: Target::Entities(vec![6]), key: "x".into() };
        assert!(apply(&mut s.doc, None, bad).is_err());
    }

    #[test]
    fn face_picks_replace_or_add() {
        let mut s = state();
        let mut picks = ViewPicks::new("t");
        let target = Target::Entities(vec![5]);
        s.doc.set_entity_key(&[5], "sides", "3 4").unwrap();
        picks.start(&mut s.pick, target.clone(), "sides", PickPurpose::Faces { add: true });
        if let Some(p) = s.pick.as_mut() {
            p.result = Some(PickResult::Faces(vec![(2, 0), (2, 3), (99, 0)]));
        }
        let mut ops = Vec::new();
        picks.poll(&s.doc, &mut s.pick, &mut ops);
        assert_eq!(
            ops,
            [Op::SetKey { target: target.clone(), key: "sides".into(), value: "3 4 1".into(), merge: false }]
        );
        picks.start(&mut s.pick, target.clone(), "sides", PickPurpose::Faces { add: false });
        if let Some(p) = s.pick.as_mut() {
            p.result = Some(PickResult::Faces(vec![(2, 1)]));
        }
        let mut ops = Vec::new();
        picks.poll(&s.doc, &mut s.pick, &mut ops);
        assert_eq!(ops, [Op::SetKey { target, key: "sides".into(), value: "2".into(), merge: false }]);
    }

    #[test]
    fn aiming_sets_angles_per_entity() {
        let mut s = state();
        let mut picks = ViewPicks::new("t");
        picks.start(&mut s.pick, Target::Entities(vec![5, 4]), "angles", PickPurpose::Aim { negative_pitch: false });
        if let Some(p) = s.pick.as_mut() {
            p.result = Some(PickResult::Point(DVec3::new(0.0, 0.0, 0.0)));
        }
        let mut ops = Vec::new();
        picks.poll(&s.doc, &mut s.pick, &mut ops);
        let Some(Op::SetPerEntity { values }) = ops.first() else { panic!("{ops:?}") };
        let angles_of = |i: usize| values[i].1.iter().find(|(k, _)| k == "angles").map(|(_, v)| v.clone());
        assert_eq!(angles_of(0).as_deref(), Some("90 0 0"), "straight down");
        assert_eq!(angles_of(1).as_deref(), Some("0 90 0"), "toward +Y");
        assert_eq!(aim_angles(DVec3::ZERO, DVec3::new(-1.0, 0.0, 1.0)), Some((-45.0, 180.0)));
        assert_eq!(aim_angles(DVec3::ZERO, DVec3::ZERO), None);
    }

    #[test]
    fn aiming_lights_uses_the_vrad_pitch() {
        let mut spot = hp_vmf::Entity { classname: "light_spot".into(), ..Default::default() };
        spot.set("origin", "0 0 0");
        spot.set("angles", "0 0 0");
        spot.set("pitch", "-30");
        let keys = |e: &hp_vmf::Entity, key: &str, neg: bool, p: DVec3| aim_keys(e, key, p, neg).unwrap();
        let down = keys(&spot, "angles", false, DVec3::new(0.0, 0.0, -100.0));
        assert!(down.contains(&("angles".into(), "90 0 0".into())), "{down:?}");
        assert!(down.contains(&("pitch".into(), "-90".into())), "{down:?}");
        let side = keys(&spot, "pitch", true, DVec3::new(0.0, 100.0, 100.0));
        assert!(side.contains(&("angles".into(), "-45 90 0".into())), "{side:?}");
        assert!(side.contains(&("pitch".into(), "45".into())), "{side:?}");
        let dir = |kv: &[(String, String)]| {
            let mut e = spot.clone();
            for (k, v) in kv {
                e.set(k, v);
            }
            let a = e.angles().unwrap_or_default();
            let pitch = e.get("pitch").and_then(|v| v.parse::<f64>().ok()).filter(|p| *p != 0.0).unwrap_or(a.x);
            let (sp, cp) = pitch.to_radians().sin_cos();
            let (sy, cy) = a.y.to_radians().sin_cos();
            DVec3::new(cp * cy, cp * sy, sp)
        };
        assert!((dir(&down) - DVec3::NEG_Z).length() < 1e-6);
        assert!((dir(&side) - DVec3::new(0.0, 1.0, 1.0).normalize()).length() < 1e-6);
        let mut prop = hp_vmf::Entity { classname: "prop_dynamic".into(), ..Default::default() };
        prop.set("origin", "0 0 0");
        prop.set("angles", "0 0 15");
        let up = keys(&prop, "angles", false, DVec3::new(-100.0, 0.0, 100.0));
        assert_eq!(up, vec![("angles".to_owned(), "-45 180 15".to_owned())]);
        let mut overridden = spot.clone();
        overridden.set("angle", "-2");
        let k = keys(&overridden, "angles", false, DVec3::new(100.0, 0.0, 0.0));
        assert!(k.contains(&("angle".into(), "0".into())), "{k:?}");
    }

    #[test]
    fn point_at_from_the_panel_turns_entities_toward_the_click() {
        use hp_app::runtime::helpers::geometry::light_direction;
        let mut s = state();
        s.doc.set_entity_class(&[5], "light_spot").unwrap();
        let cases = [
            ("angles", false, DVec3::new(0.0, 0.0, 0.0)),
            ("angles", false, DVec3::new(200.0, 200.0, 96.0)),
            ("pitch", true, DVec3::new(-100.0, 0.0, 196.0)),
            ("angles", false, DVec3::new(0.0, -300.0, -204.0)),
        ];
        for (key, neg, point) in cases {
            let mut picks = ViewPicks::new("t");
            picks.start(&mut s.pick, Target::Entities(vec![5, 4]), key, PickPurpose::Aim { negative_pitch: neg });
            if let Some(p) = s.pick.as_mut() {
                p.result = Some(PickResult::Point(point));
            }
            let mut ops = Vec::new();
            picks.poll(&s.doc, &mut s.pick, &mut ops);
            for op in ops {
                apply(&mut s.doc, None, op).unwrap();
            }
            let light = &s.doc.scene().entity(5).unwrap().entity;
            let want = (point - light.origin().unwrap()).normalize();
            let got = light_direction(light);
            assert!((got - want).length() < 1e-3, "{key} {point:?}: light points {got:?}, wanted {want:?}");
            let player = &s.doc.scene().entity(4).unwrap().entity;
            let want = (point - player.origin().unwrap()).normalize();
            let a = player.angles().unwrap();
            let (sp, cp) = a.x.to_radians().sin_cos();
            let (sy, cy) = a.y.to_radians().sin_cos();
            let got = DVec3::new(cp * cy, cp * sy, -sp);
            if key == "angles" {
                assert!((got - want).length() < 1e-3, "{point:?}: entity faces {got:?}, wanted {want:?}");
            }
        }
    }

    #[test]
    fn point_at_then_rotate_keeps_every_light_and_its_model_together() {
        use hp_app::runtime::helpers::geometry::light_direction;
        let model = |e: &hp_vmf::Entity| hp_geom::angle_matrix(e.angles().unwrap_or_default()).x_axis;
        let close = |a: DVec3, b: DVec3| (a - b).length() < 1e-3;
        for class in ["light_spot", "light_environment", "light_directional"] {
            for point in [DVec3::new(100.0, 50.0, -80.0), DVec3::new(-40.0, 0.0, 300.0), DVec3::new(0.0, 0.0, -500.0)] {
                let mut s = state();
                s.doc.set_entity_class(&[5], class).unwrap();
                let mut picks = ViewPicks::new("t");
                picks.start(
                    &mut s.pick,
                    Target::Entities(vec![5]),
                    "angles",
                    PickPurpose::Aim { negative_pitch: false },
                );
                if let Some(p) = s.pick.as_mut() {
                    p.result = Some(PickResult::Point(point));
                }
                let mut ops = Vec::new();
                picks.poll(&s.doc, &mut s.pick, &mut ops);
                for op in ops {
                    apply(&mut s.doc, None, op).unwrap();
                }
                let e = s.doc.scene().entity(5).unwrap().entity.clone();
                let o = e.origin().unwrap();
                let want = (point - o).normalize();
                assert!(close(light_direction(&e), want), "{class} {point:?}: {e:?}");
                assert!(close(model(&e), want), "{class} {point:?}: the model points at the click {e:?}");
                let pitch: f64 = e.get("pitch").unwrap().parse().unwrap();
                assert!((pitch - want.z.asin().to_degrees()).abs() < 0.01, "{class}: pitch is up positive");
                for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
                    let before = s.doc.scene().entity(5).unwrap().entity.clone();
                    s.doc.rotate_axis(&[hp_editor::ObjRef::Entity(5)], o, axis, 90.0).unwrap();
                    let after = s.doc.scene().entity(5).unwrap().entity.clone();
                    let q = glam::DQuat::from_axis_angle(axis, 90f64.to_radians());
                    assert!(close(light_direction(&after), q * light_direction(&before)), "{class} {axis}: {after:?}");
                    assert!(close(model(&after), light_direction(&after)), "{class} {axis}: model and beam {after:?}");
                }
            }
        }
    }

    #[test]
    fn requests_replaced_elsewhere_are_dropped() {
        let mut s = state();
        let mut picks = ViewPicks::new("t");
        picks.start(&mut s.pick, Target::Entities(vec![5]), "target", PickPurpose::Target);
        s.pick = Some(PickRequest { kind: PickKind::Point, requester: "other".into(), result: None });
        let mut ops = Vec::new();
        picks.poll(&s.doc, &mut s.pick, &mut ops);
        assert!(!picks.is_waiting(&s.pick, "target"));
        assert!(s.pick.is_some(), "the other request is kept");
        picks.start(&mut s.pick, Target::Entities(vec![5]), "target", PickPurpose::Target);
        picks.cancel(&mut s.pick, "target");
        assert_eq!(s.pick, None);
    }

    #[test]
    fn mark_finds_names_wildcards_and_classes() {
        let s = state();
        assert_eq!(entities_matching_target(&s.doc, "MAIN_LIGHT"), [5]);
        assert_eq!(entities_matching_target(&s.doc, "main_*"), [5]);
        assert_eq!(entities_matching_target(&s.doc, "trigger_multiple"), [6]);
        assert!(entities_matching_target(&s.doc, "!activator").is_empty());
        assert!(entities_matching_target(&s.doc, " ").is_empty());
    }

    #[test]
    fn names_are_suggested_free() {
        let mut s = state();
        assert_eq!(suggest_name(&s.doc, "prop_physics"), "prop_physics_1");
        s.doc.set_entity_key(&[4], "targetname", "light_1").unwrap();
        assert_eq!(suggest_name(&s.doc, "Light"), "light_2");
        assert_eq!(suggest_name(&s.doc, "  "), "entity_1");
    }

    #[test]
    fn asset_picks_write_once_and_return_to_the_panel() {
        let mut s = state();
        let target = Target::Entities(vec![5]);
        start_asset_pick(
            &mut s.runtime.entities.asset_pick,
            AssetKind::Sprite,
            &target,
            "model",
            "light (model)".into(),
            "",
        );
        assert!(!finish_asset_pick(&mut s), "nothing chosen yet");
        if let Some(p) = s.runtime.entities.asset_pick.as_mut() {
            p.result = Some("sprites/glow01".into());
        }
        assert!(finish_asset_pick(&mut s));
        assert_eq!(s.doc.scene().entity(5).unwrap().entity.get("model"), Some("sprites/glow01.vmt"));
        assert_eq!(s.runtime.entities.asset_pick, None);
        assert_eq!(s.ui.right_tab, RightTab::Entity);
        assert_eq!(s.doc.history_len().0, 1);
        assert_eq!(asset_value(AssetKind::Material, " a/b "), "a/b");
        assert_eq!(asset_value(AssetKind::Sprite, "sprites/x.spr"), "sprites/x.spr");
        start_asset_pick(
            &mut s.runtime.entities.asset_pick,
            AssetKind::Material,
            &Target::Entities(vec![999]),
            "m",
            String::new(),
            "",
        );
        if let Some(p) = s.runtime.entities.asset_pick.as_mut() {
            p.result = Some("x".into());
        }
        assert!(!finish_asset_pick(&mut s));
    }
}
