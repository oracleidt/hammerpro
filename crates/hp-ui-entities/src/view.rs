use std::collections::HashSet;

use hp_editor::{Document, Id, ObjRef, Owner};
use hp_fgd::{Property, PropertyType, ResolvedClass, ResolvedFgd};
use hp_vmf::{Connection, Entity};

use crate::names::MapNames;
use crate::values::{Tri, parse_flags, values_equal};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocStamp {
    revision: u64,
    entities: usize,
    solids: usize,
    next_id: Id,
    history: (usize, usize),
}

impl DocStamp {
    pub fn of(doc: &Document) -> DocStamp {
        DocStamp {
            revision: doc.revision(),
            entities: doc.scene().entities().len(),
            solids: doc.scene().solids().len(),
            next_id: doc.peek_next_object_id(),
            history: doc.history_len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    World,
    Entities(Vec<Id>),
}

impl Target {
    pub fn ids(&self) -> &[Id] {
        match self {
            Target::World => &[],
            Target::Entities(ids) => ids,
        }
    }

    pub fn is_world(&self) -> bool {
        matches!(self, Target::World)
    }

    pub fn merge_tag(&self, key: &str) -> String {
        match self {
            Target::World => format!("hp-ent:world:{}", key.to_ascii_lowercase()),
            Target::Entities(ids) => {
                let mut s = format!("hp-ent:{}:", key.to_ascii_lowercase());
                for id in ids.iter().take(64) {
                    s.push_str(&id.to_string());
                    s.push(',');
                }
                s.push_str(&ids.len().to_string());
                s
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionSummary {
    pub entities: Vec<Id>,
    pub world_solids: Vec<Id>,
    pub point_count: usize,
    pub brush_count: usize,
}

impl SelectionSummary {
    pub fn of(doc: &Document) -> SelectionSummary {
        let scene = doc.scene();
        let mut out = SelectionSummary::default();
        let mut seen: HashSet<Id> = HashSet::new();
        for obj in &doc.selection().objects {
            let entity = match *obj {
                ObjRef::Entity(id) => Some(id),
                ObjRef::Solid(id) => match scene.solid(id).map(|n| n.owner) {
                    Some(Owner::Entity(e)) => Some(e),
                    Some(Owner::World) => {
                        out.world_solids.push(id);
                        None
                    }
                    None => None,
                },
            };
            if let Some(e) = entity
                && scene.entity(e).is_some()
                && seen.insert(e)
            {
                out.entities.push(e);
                if scene.is_brush_entity(e) {
                    out.brush_count += 1;
                } else {
                    out.point_count += 1;
                }
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty() && self.world_solids.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Agg {
    Absent,
    Value(String),
    Mixed,
}

impl Agg {
    pub fn of<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Agg {
        let mut first: Option<Option<&str>> = None;
        for v in values {
            match first {
                None => first = Some(v),
                Some(f) if f == v => {}
                Some(_) => return Agg::Mixed,
            }
        }
        match first.flatten() {
            Some(v) => Agg::Value(v.to_owned()),
            None => Agg::Absent,
        }
    }

    pub fn value(&self) -> Option<&str> {
        match self {
            Agg::Value(v) => Some(v),
            _ => None,
        }
    }

    pub fn is_mixed(&self) -> bool {
        matches!(self, Agg::Mixed)
    }

    pub fn text(&self) -> &str {
        self.value().unwrap_or("")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropRow {
    pub key: String,
    pub prop: Option<Property>,
    pub agg: Agg,
    pub modified: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlagBit {
    pub bit: u32,
    pub label: String,
    pub state: Tri,
    pub default_on: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlagGroup {
    pub key: String,
    pub label: String,
    pub agg: Agg,
    pub values: Vec<u32>,
    pub bits: Vec<FlagBit>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RawRow {
    pub key: String,
    pub agg: Agg,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnIssues {
    pub unknown_output: bool,
    pub missing_target: bool,
    pub unknown_input: bool,
}

impl ConnIssues {
    pub fn any(&self) -> bool {
        self.unknown_output || self.missing_target || self.unknown_input
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnRow {
    pub conn: Connection,
    pub refs: Vec<(Id, usize)>,
    pub issues: ConnIssues,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputRow {
    pub source: Id,
    pub source_is_world: bool,
    pub source_class: String,
    pub source_name: String,
    pub conn: Connection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    World,
    Point,
    Brush,
    Mixed,
}

#[derive(Clone, Debug)]
pub struct EntityView {
    pub target: Target,
    pub kind: EditKind,
    pub classes: Vec<String>,
    pub description: String,
    pub classes_known: bool,
    pub props: Vec<PropRow>,
    pub flags: Vec<FlagGroup>,
    pub raw: Vec<RawRow>,
    pub conns: Vec<ConnRow>,
    pub conns_hidden: usize,
    pub names: Vec<String>,
    pub inputs: Vec<InputRow>,
    pub comments: Agg,
}

impl EntityView {
    pub fn empty() -> EntityView {
        EntityView {
            target: Target::Entities(Vec::new()),
            kind: EditKind::Point,
            classes: Vec::new(),
            description: String::new(),
            classes_known: false,
            props: Vec::new(),
            flags: Vec::new(),
            raw: Vec::new(),
            conns: Vec::new(),
            conns_hidden: 0,
            names: Vec::new(),
            inputs: Vec::new(),
            comments: Agg::Absent,
        }
    }

    pub fn count(&self) -> usize {
        match &self.target {
            Target::World => 1,
            Target::Entities(ids) => ids.len(),
        }
    }

    pub fn single_class(&self) -> Option<&str> {
        match self.classes.as_slice() {
            [c] => Some(c),
            _ => None,
        }
    }

    pub fn build(doc: &Document, target: Target, fgd: Option<&ResolvedFgd>, names: &MapNames) -> EntityView {
        let scene = doc.scene();
        let (target, entities): (Target, Vec<&Entity>) = match target {
            Target::World => (Target::World, vec![scene.world()]),
            Target::Entities(ids) => {
                let ids: Vec<Id> = ids.into_iter().filter(|id| scene.entity(*id).is_some()).collect();
                let ents = ids.iter().filter_map(|id| scene.entity(*id).map(|n| &n.entity)).collect();
                (Target::Entities(ids), ents)
            }
        };
        let mut view = EntityView::empty();
        view.target = target;
        if entities.is_empty() {
            return view;
        }

        for e in &entities {
            if !view.classes.iter().any(|c| c.eq_ignore_ascii_case(&e.classname)) {
                view.classes.push(e.classname.clone());
            }
        }
        view.kind = if view.target.is_world() {
            EditKind::World
        } else {
            let brush = view.target.ids().iter().filter(|id| scene.is_brush_entity(**id)).count();
            match brush {
                0 => EditKind::Point,
                n if n == entities.len() => EditKind::Brush,
                _ => EditKind::Mixed,
            }
        };

        let fgd_classes: Vec<&ResolvedClass> =
            view.classes.iter().filter_map(|c| fgd.and_then(|f| f.class(c))).collect();
        view.classes_known = !fgd_classes.is_empty() && fgd_classes.len() == view.classes.len();
        if let [only] = fgd_classes.as_slice()
            && view.classes.len() == 1
        {
            view.description = only.description.clone();
        }
        let mut schema: Vec<Property> = if view.classes_known {
            schema_props(&fgd_classes)
        } else if view.target.is_world() {
            worldspawn_fallback_props()
        } else {
            Vec::new()
        };
        if view.target.is_world() {
            for p in compiler_world_props() {
                if !schema.iter().any(|s| s.name.eq_ignore_ascii_case(&p.name)) {
                    schema.push(p);
                }
            }
        }

        let mut known: HashSet<String> = HashSet::new();
        for p in &schema {
            known.insert(p.name.to_ascii_lowercase());
            if p.kind == PropertyType::Flags {
                continue;
            }
            let agg = Agg::of(entities.iter().map(|e| e.get(&p.name)));
            let modified = match (&agg, p.default.as_deref()) {
                (Agg::Value(v), Some(d)) => !values_equal(v, d),
                (Agg::Value(v), None) => !v.trim().is_empty(),
                (Agg::Mixed, _) => true,
                (Agg::Absent, _) => false,
            };
            view.props.push(PropRow { key: p.name.clone(), prop: Some(p.clone()), agg, modified });
        }

        for p in schema.iter().filter(|p| p.kind == PropertyType::Flags) {
            view.flags.push(flag_group(&entities, &p.name, p.label(), Some(p)));
        }
        if view.flags.is_empty() && !view.target.is_world() && entities.iter().any(|e| e.get("spawnflags").is_some()) {
            known.insert("spawnflags".to_owned());
            view.flags.push(flag_group(&entities, "spawnflags", "spawnflags", None));
        }

        let mut seen: HashSet<String> = HashSet::new();
        for e in &entities {
            for (k, _) in &e.props {
                let lower = k.to_ascii_lowercase();
                if seen.insert(lower.clone()) {
                    let agg = Agg::of(entities.iter().map(|e| e.get(k)));
                    view.raw.push(RawRow { key: k.clone(), agg: agg.clone() });
                    if !known.contains(&lower) {
                        view.props.push(PropRow { key: k.clone(), prop: None, agg, modified: false });
                    }
                }
            }
        }

        view.comments = Agg::of(entities.iter().map(|e| e.editor.comments.as_deref()));

        if view.target.is_world() {
            return view;
        }

        let source_class = view.single_class().and_then(|c| fgd.and_then(|f| f.class(c)));
        let (rows, hidden) = common_connections(view.target.ids(), &entities);
        view.conns_hidden = hidden;
        view.conns = rows
            .into_iter()
            .map(|(conn, refs)| {
                let issues = check_connection(&conn, source_class, fgd, names);
                ConnRow { conn, refs, issues }
            })
            .collect();

        let mut seen_names: HashSet<String> = HashSet::new();
        for e in &entities {
            if let Some(n) = e.get("targetname").map(str::trim).filter(|n| !n.is_empty())
                && seen_names.insert(n.to_ascii_lowercase())
            {
                view.names.push(n.to_owned());
            }
        }
        view.inputs = inputs_targeting(doc, &view.names);
        view
    }
}

fn schema_props(classes: &[&ResolvedClass]) -> Vec<Property> {
    let Some((first, rest)) = classes.split_first() else {
        return Vec::new();
    };
    first.props.iter().filter(|p| rest.iter().all(|c| c.property(&p.name).is_some())).cloned().collect()
}

fn flag_group(entities: &[&Entity], key: &str, label: &str, prop: Option<&Property>) -> FlagGroup {
    let values: Vec<u32> = entities.iter().map(|e| e.get(key).map_or(0, parse_flags)).collect();
    let agg = Agg::of(entities.iter().map(|e| e.get(key)));
    let mut bits: Vec<FlagBit> = Vec::new();
    if let Some(p) = prop {
        for f in &p.flags {
            bits.push(FlagBit {
                bit: f.value,
                label: f.display.clone(),
                state: Tri::of_bit(&values, f.value),
                default_on: f.default_on,
            });
        }
    }
    let mut extra: u32 = values.iter().fold(0, |acc, v| acc | v);
    for b in &bits {
        extra &= !b.bit;
    }
    for bit in crate::values::bits_of(extra) {
        bits.push(FlagBit { bit, label: String::new(), state: Tri::of_bit(&values, bit), default_on: false });
    }
    FlagGroup { key: key.to_owned(), label: label.to_owned(), agg, values, bits }
}

pub type CommonConnection = (Connection, Vec<(Id, usize)>);

pub fn same_connection(a: &Connection, b: &Connection) -> bool {
    a.output.eq_ignore_ascii_case(&b.output)
        && a.target.eq_ignore_ascii_case(&b.target)
        && a.input.eq_ignore_ascii_case(&b.input)
        && a.param == b.param
        && (a.delay - b.delay).abs() < 1e-6
        && a.times == b.times
}

pub fn common_connections(ids: &[Id], entities: &[&Entity]) -> (Vec<CommonConnection>, usize) {
    let Some((first, rest)) = entities.split_first() else {
        return (Vec::new(), 0);
    };
    let Some((&first_id, rest_ids)) = ids.split_first() else {
        return (Vec::new(), 0);
    };
    let mut used: Vec<Vec<bool>> = rest.iter().map(|e| vec![false; e.connections.len()]).collect();
    let mut rows = Vec::new();
    let mut hidden = 0;
    'outer: for (i, conn) in first.connections.iter().enumerate() {
        let mut refs = Vec::with_capacity(entities.len());
        refs.push((first_id, i));
        let mut picks: Vec<(usize, usize)> = Vec::with_capacity(rest.len());
        for (k, e) in rest.iter().enumerate() {
            let flags = &used[k];
            let found = e.connections.iter().enumerate().position(|(j, c)| !flags[j] && same_connection(c, conn));
            match found {
                Some(j) => picks.push((k, j)),
                None => {
                    hidden += 1;
                    continue 'outer;
                }
            }
        }
        for (k, j) in picks {
            used[k][j] = true;
            refs.push((rest_ids[k], j));
        }
        rows.push((conn.clone(), refs));
    }
    if !rest.is_empty() {
        hidden += used.iter().map(|u| u.iter().filter(|x| !**x).count()).sum::<usize>();
    }
    (rows, hidden)
}

pub fn check_connection(
    conn: &Connection,
    source_class: Option<&ResolvedClass>,
    fgd: Option<&ResolvedFgd>,
    names: &MapNames,
) -> ConnIssues {
    let mut issues = ConnIssues {
        unknown_output: source_class.is_some_and(|c| c.output(conn.output.trim()).is_none()),
        missing_target: !names.target_exists(&conn.target),
        unknown_input: conn.input.trim().is_empty(),
    };
    if let Some(fgd) = fgd
        && !issues.unknown_input
    {
        let classes: Vec<&ResolvedClass> =
            names.target_classes(&conn.target).iter().filter_map(|c| fgd.class(c)).collect();
        if !classes.is_empty() {
            issues.unknown_input = !classes.iter().any(|c| c.input(conn.input.trim()).is_some());
        }
    }
    issues
}

pub fn inputs_targeting(doc: &Document, names: &[String]) -> Vec<InputRow> {
    if names.is_empty() {
        return Vec::new();
    }
    let scene = doc.scene();
    let world = scene.world();
    let row = |e: &Entity, conn: &Connection, is_world: bool| InputRow {
        source: e.id,
        source_is_world: is_world,
        source_class: e.classname.clone(),
        source_name: e.get("targetname").unwrap_or("").to_owned(),
        conn: conn.clone(),
    };
    if let [name] = names {
        return doc
            .outputs_targeting(name)
            .into_iter()
            .filter_map(|(id, index)| {
                if let Some(n) = scene.entity(id) {
                    n.entity.connections.get(index).map(|c| row(&n.entity, c, false))
                } else if id == world.id {
                    world.connections.get(index).map(|c| row(world, c, true))
                } else {
                    None
                }
            })
            .collect();
    }
    let wanted: HashSet<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    let mut out = Vec::new();
    for c in &world.connections {
        if wanted.contains(&c.target.to_ascii_lowercase()) {
            out.push(row(world, c, true));
        }
    }
    for n in scene.entities().values() {
        for c in &n.entity.connections {
            if wanted.contains(&c.target.to_ascii_lowercase()) {
                out.push(row(&n.entity, c, false));
            }
        }
    }
    out
}

pub fn worldspawn_fallback_props() -> Vec<Property> {
    fn prop(name: &str, kind: PropertyType, display: &str, default: Option<&str>) -> Property {
        let mut p = Property::new(name, kind);
        p.display_name = display.to_owned();
        p.default = default.map(str::to_owned);
        p
    }
    let yes_no = |name: &str, display: &str| {
        let mut p = prop(name, PropertyType::Choices, display, Some("0"));
        p.choices = vec![
            hp_fgd::Choice { value: "0".into(), display: "No".into() },
            hp_fgd::Choice { value: "1".into(), display: "Yes".into() },
        ];
        p
    };
    vec![
        prop("message", PropertyType::String, "Map Description / Title", None),
        prop("skyname", PropertyType::String, "SkyBox Texture Name", Some("sky_day01_01")),
        prop("chaptertitle", PropertyType::String, "Chapter Title Message", None),
        yes_no("startdark", "Level Fade In"),
        yes_no("gametitle", "Display Game Title"),
        yes_no("newunit", "New Level Unit"),
        prop("maxoccludeearea", PropertyType::Float, "Max occludee area", Some("0")),
        prop("minoccluderarea", PropertyType::Float, "Min occluder area", Some("0")),
        prop("maxpropscreenwidth", PropertyType::Float, "Start Fade Pixels", Some("-1")),
        prop("minpropscreenwidth", PropertyType::Float, "End Fade Pixels", Some("0")),
        prop("detailvbsp", PropertyType::String, "Detail.vbsp file", Some("detail.vbsp")),
        prop("detailmaterial", PropertyType::Material, "Detail material file", Some("detail/detailsprites")),
    ]
}

pub fn compiler_world_props() -> Vec<Property> {
    fn prop(name: &str, kind: PropertyType, display: &str, help: &str, default: Option<&str>) -> Property {
        let mut p = Property::new(name, kind);
        p.display_name = display.to_owned();
        p.description = help.to_owned();
        p.default = default.map(str::to_owned);
        p
    }
    vec![prop(
        "_lightsrad",
        PropertyType::String,
        hp_app::t!("entities.world_lightsrad"),
        hp_app::t!("entities.world_lightsrad_help"),
        None,
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fgd::test_fgd;

    #[test]
    fn map_properties_show_the_compiler_keys() {
        let state = hp_app::testing::test_state();
        let names = MapNames::build(&state.doc);
        let fgd = test_fgd();
        for f in [Some(&fgd), None] {
            let view = EntityView::build(&state.doc, Target::World, f, &names);
            let keys: Vec<&str> = view.props.iter().map(|r| r.key.as_str()).collect();
            assert_eq!(keys.iter().filter(|x| x.eq_ignore_ascii_case("_lightsrad")).count(), 1, "{keys:?}");
            assert!(!keys.iter().any(|x| x.eq_ignore_ascii_case("_ao")), "no ambient occlusion keys");
        }
        let view = EntityView::build(&state.doc, Target::Entities(vec![5]), Some(&fgd), &names);
        assert!(!view.props.iter().any(|r| r.key == "_lightsrad"));
    }

    fn ids_of(doc: &Document, class: &str) -> Vec<Id> {
        doc.scene().entities().values().filter(|n| n.entity.classname == class).map(|n| n.entity.id).collect()
    }

    #[test]
    fn agg_merges_values() {
        assert_eq!(Agg::of([Some("a"), Some("a")]), Agg::Value("a".into()));
        assert_eq!(Agg::of([Some("a"), Some("b")]), Agg::Mixed);
        assert_eq!(Agg::of([Some("a"), None]), Agg::Mixed);
        assert_eq!(Agg::of([None, None]), Agg::Absent);
        assert_eq!(Agg::of(std::iter::empty::<Option<&str>>()), Agg::Absent);
        assert_eq!(Agg::Mixed.text(), "");
    }

    #[test]
    fn selection_summary_counts_brush_entity_solids() {
        let mut state = hp_app::testing::test_state();
        let doc = &mut state.doc;
        assert!(SelectionSummary::of(doc).is_empty());
        doc.set_selection(&[ObjRef::Solid(7), ObjRef::Solid(2), ObjRef::Entity(5)]);
        let s = SelectionSummary::of(doc);
        assert_eq!(s.entities, [6, 5]);
        assert_eq!(s.world_solids, [2]);
        assert_eq!(s.brush_count, 1);
        assert_eq!(s.point_count, 1);
    }

    #[test]
    fn view_of_light_uses_fgd_order_and_defaults() {
        let state = hp_app::testing::test_state();
        let fgd = test_fgd();
        let names = MapNames::build(&state.doc);
        let view = EntityView::build(&state.doc, Target::Entities(vec![5]), Some(&fgd), &names);
        assert_eq!(view.kind, EditKind::Point);
        assert_eq!(view.single_class(), Some("light"));
        assert!(view.classes_known);
        assert!(!view.description.is_empty());
        let keys: Vec<&str> = view.props.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(&keys[..4], ["targetname", "angles", "_light", "style"]);
        assert!(keys.contains(&"origin"), "unknown keys come after the FGD keys: {keys:?}");
        let light = view.props.iter().find(|r| r.key == "_light").unwrap();
        assert_eq!(light.agg, Agg::Value("255 240 200 300".into()));
        assert!(light.modified, "differs from the default 255 255 255 200");
        let style = view.props.iter().find(|r| r.key == "style").unwrap();
        assert_eq!(style.agg, Agg::Absent);
        assert!(!style.modified);
        assert_eq!(view.flags.len(), 1);
        assert_eq!(view.names, ["main_light"]);
        assert_eq!(view.inputs.len(), 1, "the trigger targets the light");
        assert_eq!(view.inputs[0].source_class, "trigger_multiple");
        assert_eq!(view.raw.len(), 3);
    }

    #[test]
    fn view_of_trigger_validates_outputs() {
        let state = hp_app::testing::test_state();
        let fgd = test_fgd();
        let names = MapNames::build(&state.doc);
        let view = EntityView::build(&state.doc, Target::Entities(vec![6]), Some(&fgd), &names);
        assert_eq!(view.kind, EditKind::Brush);
        assert_eq!(view.conns.len(), 1);
        assert!(!view.conns[0].issues.any(), "{:?}", view.conns[0].issues);
        let flags = &view.flags[0];
        assert_eq!(flags.key, "spawnflags");
        assert_eq!(flags.bits[0].state, Tri::On);
        assert_eq!(flags.bits[1].state, Tri::Off);
    }

    #[test]
    fn multi_entity_aggregation() {
        let mut state = hp_app::testing::test_state();
        let fgd = test_fgd();
        let doc = &mut state.doc;
        let light = ids_of(doc, "light")[0];
        let start = ids_of(doc, "info_player_start")[0];
        let names = MapNames::build(doc);
        let view = EntityView::build(doc, Target::Entities(vec![light, start]), Some(&fgd), &names);
        assert_eq!(view.classes, ["light", "info_player_start"]);
        let typed: Vec<&str> = view.props.iter().filter(|r| r.prop.is_some()).map(|r| r.key.as_str()).collect();
        assert_eq!(typed, ["angles"]);
        let origin = view.props.iter().find(|r| r.key == "origin").unwrap();
        assert!(origin.agg.is_mixed());
        let angles = view.props.iter().find(|r| r.key == "angles").unwrap();
        assert!(angles.agg.is_mixed(), "only the player start has angles");
        doc.set_entity_key(&[light, start], "targetname", "shared").unwrap();
        let names = MapNames::build(doc);
        let view = EntityView::build(doc, Target::Entities(vec![light, start]), Some(&fgd), &names);
        let name = view.raw.iter().find(|r| r.key == "targetname").unwrap();
        assert_eq!(name.agg, Agg::Value("shared".into()));
        assert_eq!(view.names, ["shared"]);
    }

    #[test]
    fn flags_merge_across_entities() {
        let mut state = hp_app::testing::test_state();
        let fgd = test_fgd();
        let doc = &mut state.doc;
        let light = ids_of(doc, "light")[0];
        doc.set_entity_key(&[light], "spawnflags", "1").unwrap();
        let start = ids_of(doc, "info_player_start")[0];
        doc.set_entity_class(&[start], "light").unwrap();
        doc.set_entity_key(&[start], "spawnflags", "3").unwrap();
        let names = MapNames::build(doc);
        let view = EntityView::build(doc, Target::Entities(vec![light, start]), Some(&fgd), &names);
        let group = &view.flags[0];
        assert_eq!(group.values, [1, 3]);
        assert!(group.agg.is_mixed());
        assert_eq!(group.bits[0].state, Tri::On);
        let extra = group.bits.iter().find(|b| b.bit == 2).unwrap();
        assert_eq!(extra.state, Tri::Mixed);
        assert!(extra.label.is_empty());
    }

    #[test]
    fn common_connections_multiset() {
        let a = Connection::parse("OnTrigger", "x,Kill,,0,-1");
        let b = Connection::parse("OnTrigger", "y,Kill,,0,-1");
        let mut e1 = Entity { id: 1, ..Entity::default() };
        e1.connections = vec![a.clone(), a.clone(), b.clone()];
        let mut e2 = Entity { id: 2, ..Entity::default() };
        e2.connections = vec![b.clone(), a.clone()];
        let (rows, hidden) = common_connections(&[1, 2], &[&e1, &e2]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, [(1, 0), (2, 1)]);
        assert_eq!(rows[1].1, [(1, 2), (2, 0)]);
        assert_eq!(hidden, 1);
        let (single, hidden) = common_connections(&[1], &[&e1]);
        assert_eq!(single.len(), 3);
        assert_eq!(hidden, 0);
    }

    #[test]
    fn connection_validation() {
        let state = hp_app::testing::test_state();
        let fgd = test_fgd();
        let names = MapNames::build(&state.doc);
        let trigger = fgd.class("trigger_multiple");
        let ok = Connection::parse("OnTrigger", "main_light,TurnOff,,0,-1");
        assert!(!check_connection(&ok, trigger, Some(&fgd), &names).any());
        let bad_output = Connection::parse("OnNothing", "main_light,TurnOff,,0,-1");
        assert!(check_connection(&bad_output, trigger, Some(&fgd), &names).unknown_output);
        let missing = Connection::parse("OnTrigger", "nobody,TurnOff,,0,-1");
        let issues = check_connection(&missing, trigger, Some(&fgd), &names);
        assert!(issues.missing_target && !issues.unknown_input);
        let bad_input = Connection::parse("OnTrigger", "main_light,Explode,,0,-1");
        assert!(check_connection(&bad_input, trigger, Some(&fgd), &names).unknown_input);
        let special = Connection::parse("OnTrigger", "!activator,Anything,,0,-1");
        assert!(!check_connection(&special, trigger, Some(&fgd), &names).any());
        let empty_input = Connection::parse("OnTrigger", "main_light,,,0,-1");
        assert!(check_connection(&empty_input, trigger, None, &names).unknown_input);
        assert!(!check_connection(&bad_output, None, None, &names).any());
    }

    #[test]
    fn worldspawn_view_without_fgd_uses_fallback() {
        let state = hp_app::testing::test_state();
        let names = MapNames::build(&state.doc);
        let view = EntityView::build(&state.doc, Target::World, None, &names);
        assert_eq!(view.kind, EditKind::World);
        let sky = view.props.iter().find(|r| r.key == "skyname").unwrap();
        assert_eq!(sky.agg, Agg::Value("sky_day01_01".into()));
        assert!(!sky.modified);
        assert!(view.conns.is_empty());
        assert!(view.props.iter().any(|r| r.key == "detailmaterial"));
    }

    #[test]
    fn merge_tags_are_specific() {
        let a = Target::Entities(vec![1, 2]).merge_tag("_light");
        let b = Target::Entities(vec![1]).merge_tag("_light");
        let c = Target::Entities(vec![1, 2]).merge_tag("_LIGHT");
        assert_ne!(a, b);
        assert_eq!(a, c);
        assert_ne!(Target::World.merge_tag("skyname"), a);
    }
}
