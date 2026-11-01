use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use hp_app::runtime::helpers::NameIndex;
use hp_editor::{Document, Id};
use hp_fgd::ResolvedFgd;

pub const MAX_TARGETS_PER_OUTPUT: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NodeFilter {
    #[default]
    All,
    NamedAndConnected,
    ConnectedOnly,
}

impl NodeFilter {
    pub const ALL: [NodeFilter; 3] = [NodeFilter::All, NodeFilter::NamedAndConnected, NodeFilter::ConnectedOnly];

    pub fn label_key(self) -> &'static str {
        match self {
            NodeFilter::All => "lab.logic.filter_all",
            NodeFilter::NamedAndConnected => "lab.logic.filter_named",
            NodeFilter::ConnectedOnly => "lab.logic.filter_connected",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            NodeFilter::All => "all",
            NodeFilter::NamedAndConnected => "named",
            NodeFilter::ConnectedOnly => "connected",
        }
    }

    pub fn from_name(name: &str) -> Option<NodeFilter> {
        NodeFilter::ALL.into_iter().find(|f| f.name().eq_ignore_ascii_case(name.trim()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EdgeState {
    Valid,
    MissingTarget,
    UnknownOutput,
    UnknownInput,
}

impl EdgeState {
    pub fn is_problem(self) -> bool {
        self != EdgeState::Valid
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: Id,
    pub class: String,
    pub name: String,
    pub icon: Option<String>,
    pub stored: Option<[f32; 2]>,
    pub outputs: u32,
    pub inputs: u32,
    pub problems: u32,
    pub special: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: usize,
    pub to: Option<usize>,
    pub connection: usize,
    pub output: String,
    pub target: String,
    pub input: String,
    pub param: String,
    pub delay: f32,
    pub times: i32,
    pub state: EdgeState,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub index: HashMap<Id, usize>,
}

pub fn parse_logical_pos(text: &str) -> Option<[f32; 2]> {
    let inner = text.trim().trim_start_matches('[').trim_end_matches(']');
    let mut it = inner.split_ascii_whitespace().map(|s| s.parse::<f32>().ok());
    let x = it.next()??;
    let y = it.next()??;
    (x.is_finite() && y.is_finite() && x.abs() < 1.0e7 && y.abs() < 1.0e7).then_some([x, -y])
}

pub fn format_logical_pos(p: [f32; 2]) -> String {
    format!("[{} {}]", p[0].round() as i64, (-p[1]).round() as i64)
}

pub fn icon_material(fgd: Option<&ResolvedFgd>, e: &hp_vmf::Entity) -> Option<String> {
    let class = fgd?.class(&e.classname)?;
    let path = if class.has_helper("iconsprite") {
        class.iconsprite().map(str::to_owned).or_else(|| e.get("model").map(str::to_owned))
    } else if class.has_helper("sprite") {
        class.sprite().map(str::to_owned).or_else(|| e.get("model").map(str::to_owned))
    } else {
        None
    }?;
    let key = path.trim().replace('\\', "/").to_ascii_lowercase();
    if key.ends_with(".mdl") {
        return None;
    }
    let key = key.strip_prefix("materials/").unwrap_or(&key);
    let key = key.strip_suffix(".vmt").or_else(|| key.strip_suffix(".spr")).unwrap_or(key);
    let key = key.trim_matches('/');
    (!key.is_empty()).then(|| key.to_owned())
}

pub fn fingerprint(doc: &Document, with_positions: bool, fgd_generation: u64, filter: NodeFilter) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    fgd_generation.hash(&mut h);
    filter.hash(&mut h);
    for (id, node) in doc.scene().entities() {
        let e = &node.entity;
        id.hash(&mut h);
        e.classname.hash(&mut h);
        e.get("targetname").unwrap_or("").hash(&mut h);
        e.get("model").unwrap_or("").hash(&mut h);
        e.connections.len().hash(&mut h);
        for c in &e.connections {
            c.output.hash(&mut h);
            c.target.hash(&mut h);
            c.input.hash(&mut h);
            c.param.hash(&mut h);
            c.delay.to_bits().hash(&mut h);
            c.times.hash(&mut h);
        }
        if with_positions {
            e.editor.logical_pos.hash(&mut h);
        }
    }
    h.finish()
}

impl Graph {
    pub fn build(doc: &Document, names: &NameIndex, fgd: Option<&ResolvedFgd>, filter: NodeFilter) -> Graph {
        let scene = doc.scene();
        let mut resolved: Vec<(Id, usize, Vec<Id>)> = Vec::new();
        let mut targeted: std::collections::HashSet<Id> = std::collections::HashSet::new();
        let mut found = Vec::new();
        for (id, node) in scene.entities() {
            for (i, c) in node.entity.connections.iter().enumerate() {
                found.clear();
                if !NameIndex::is_special(&c.target) {
                    names.resolve(&c.target, &mut found);
                }
                found.truncate(MAX_TARGETS_PER_OUTPUT);
                targeted.extend(found.iter().copied());
                resolved.push((*id, i, found.clone()));
            }
        }
        let mut g = Graph::default();
        for (id, node) in scene.entities() {
            let e = &node.entity;
            let named = e.get("targetname").is_some_and(|n| !n.trim().is_empty());
            let wanted = filter == NodeFilter::All
                || !e.connections.is_empty()
                || targeted.contains(id)
                || (filter == NodeFilter::NamedAndConnected && named);
            if !wanted {
                continue;
            }
            g.index.insert(*id, g.nodes.len());
            g.nodes.push(Node {
                id: *id,
                class: e.classname.clone(),
                name: e.get("targetname").map(str::trim).unwrap_or("").to_owned(),
                icon: icon_material(fgd, e),
                stored: e.editor.logical_pos.as_deref().and_then(parse_logical_pos),
                outputs: e.connections.len() as u32,
                inputs: 0,
                problems: 0,
                special: 0,
            });
        }
        for (src, i, targets) in resolved {
            let Some(&from) = g.index.get(&src) else { continue };
            let Some(node) = scene.entity(src) else { continue };
            let Some(c) = node.entity.connections.get(i) else { continue };
            if NameIndex::is_special(&c.target) {
                g.nodes[from].special += 1;
                continue;
            }
            let class = fgd.and_then(|f| f.class(&node.entity.classname));
            let output_known = class.is_none_or(|cl| cl.output(c.output.trim()).is_some());
            let base = Edge {
                from,
                to: None,
                connection: i,
                output: c.output.clone(),
                target: c.target.clone(),
                input: c.input.clone(),
                param: c.param.clone(),
                delay: c.delay,
                times: c.times,
                state: EdgeState::MissingTarget,
            };
            if targets.is_empty() {
                g.nodes[from].problems += 1;
                g.edges.push(base);
                continue;
            }
            let mut problem = false;
            for t in targets {
                let Some(&to) = g.index.get(&t) else { continue };
                let target_class = fgd.and_then(|f| scene.entity(t).and_then(|n| f.class(&n.entity.classname)));
                let input_known = target_class.is_none_or(|cl| cl.input(c.input.trim()).is_some());
                let state = if !output_known {
                    EdgeState::UnknownOutput
                } else if !input_known {
                    EdgeState::UnknownInput
                } else {
                    EdgeState::Valid
                };
                problem |= state.is_problem();
                g.nodes[to].inputs += 1;
                g.edges.push(Edge { to: Some(to), state, ..base.clone() });
            }
            if problem {
                g.nodes[from].problems += 1;
            }
        }
        g
    }

    pub fn problem_count(&self) -> usize {
        self.nodes.iter().map(|n| n.problems as usize).sum()
    }

    pub fn links(&self) -> Vec<(usize, usize)> {
        self.edges.iter().filter_map(|e| e.to.map(|t| (e.from, t))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_positions_round_trip() {
        assert_eq!(parse_logical_pos("[0 1000]"), Some([0.0, -1000.0]));
        assert_eq!(parse_logical_pos(" [ -5.5 3 ] "), Some([-5.5, -3.0]));
        assert_eq!(parse_logical_pos("[1]"), None);
        assert_eq!(parse_logical_pos("[nan 2]"), None);
        assert_eq!(parse_logical_pos("[1e30 2]"), None);
        assert_eq!(format_logical_pos([12.4, -1000.0]), "[12 1000]");
        assert_eq!(parse_logical_pos(&format_logical_pos([12.0, -40.0])), Some([12.0, -40.0]));
    }

    #[test]
    fn the_test_map_has_its_trigger_and_light() {
        let state = hp_app::testing::test_state();
        let names = NameIndex::build(&state.doc);
        let g = Graph::build(&state.doc, &names, None, NodeFilter::NamedAndConnected);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        let e = &g.edges[0];
        assert_eq!(e.state, EdgeState::Valid);
        assert_eq!(g.nodes[e.from].class, "trigger_multiple");
        assert_eq!(g.nodes[e.to.expect("target")].name, "main_light");
        assert_eq!(g.nodes[e.to.expect("target")].inputs, 1);
        assert_eq!(g.problem_count(), 0);
    }

    #[test]
    fn unnamed_lights_and_point_entities_are_nodes_with_their_icons() {
        let text = r#"world { "id" "1" "classname" "worldspawn" }
entity { "id" "2" "classname" "light_environment" "origin" "0 0 0" }
entity { "id" "3" "classname" "light" "origin" "0 0 64" }
entity { "id" "4" "classname" "light_spot" "origin" "0 0 128" "angles" "-90 0 0" }
entity { "id" "5" "classname" "info_target" "origin" "0 0 32" }
entity { "id" "6" "classname" "env_sprite" "model" "sprites/glow01.spr" "origin" "8 0 0" }
entity { "id" "7" "classname" "prop_static" "model" "models/props_c17/oildrum001.mdl" "origin" "16 0 0" }
entity { "id" "8" "classname" "logic_relay" "targetname" "relay" "origin" "0 32 0" connections { "OnTrigger" "lamp,TurnOn,,0,-1" } }
entity { "id" "9" "classname" "light" "targetname" "lamp" "origin" "0 64 0" }
"#;
        let doc = Document::from_map(hp_vmf::parse(text).expect("map"));
        let fgd = hp_fgd::Fgd::parse(
            r#"@PointClass iconsprite("editor/light_env.vmt") = light_environment : "" []
@PointClass iconsprite("editor/light.vmt") = light : "" [ input TurnOn(void) : "" ]
@PointClass iconsprite("materials/editor/light_spot.vmt") = light_spot : "" []
@PointClass iconsprite("editor\info_target.vmt") = info_target : "" []
@PointClass iconsprite() sprite() = env_sprite : "" []
@PointClass studioprop() = prop_static : "" []
@PointClass iconsprite("editor/logic_relay.vmt") = logic_relay : "" [ output OnTrigger(void) : "" ]"#,
            None,
        )
        .resolve();
        let names = NameIndex::build(&doc);
        let g = Graph::build(&doc, &names, Some(&fgd), NodeFilter::All);
        assert_eq!(g.nodes.len(), 8, "every entity is a node");
        let icon = |id: Id| g.nodes[g.index[&id]].icon.as_deref();
        assert_eq!(icon(2), Some("editor/light_env"));
        assert_eq!(icon(3), Some("editor/light"));
        assert_eq!(icon(4), Some("editor/light_spot"));
        assert_eq!(icon(5), Some("editor/info_target"));
        assert_eq!(icon(6), Some("sprites/glow01"));
        assert_eq!(icon(7), None, "models have no icon");
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].state, EdgeState::Valid);
        assert_eq!(g.nodes[g.edges[0].to.expect("lamp")].id, 9);
        let named = Graph::build(&doc, &names, Some(&fgd), NodeFilter::NamedAndConnected);
        assert_eq!(named.nodes.len(), 2);
        assert!(!named.index.contains_key(&2), "the named filter leaves unnamed lights out");
        assert_eq!(NodeFilter::from_name("ALL"), Some(NodeFilter::All));
        assert_eq!(NodeFilter::from_name("connected"), Some(NodeFilter::ConnectedOnly));
        assert_eq!(NodeFilter::from_name("x"), None);
        assert_eq!(NodeFilter::default(), NodeFilter::All);
    }

    #[test]
    fn missing_targets_and_special_targets() {
        let mut state = hp_app::testing::test_state();
        let trigger = 6;
        let add = |doc: &mut Document, target: &str| {
            let c = hp_vmf::Connection {
                output: "OnTrigger".into(),
                target: target.into(),
                input: "Kill".into(),
                ..Default::default()
            };
            doc.add_connection(&[trigger], &c).expect("add");
        };
        add(&mut state.doc, "nobody");
        add(&mut state.doc, "!activator");
        let names = NameIndex::build(&state.doc);
        let g = Graph::build(&state.doc, &names, None, NodeFilter::ConnectedOnly);
        let t = g.index[&trigger];
        assert_eq!(g.nodes[t].problems, 1);
        assert_eq!(g.nodes[t].special, 1);
        assert!(g.edges.iter().any(|e| e.state == EdgeState::MissingTarget && e.to.is_none()));
        assert_eq!(g.problem_count(), 1);
    }

    #[test]
    fn fingerprints_follow_what_matters() {
        let mut state = hp_app::testing::test_state();
        let a = fingerprint(&state.doc, true, 1, NodeFilter::NamedAndConnected);
        let a_struct = fingerprint(&state.doc, false, 1, NodeFilter::NamedAndConnected);
        state.doc.edit_entity(5, |e| e.editor.logical_pos = Some("[10 20]".into())).expect("edit");
        assert_ne!(fingerprint(&state.doc, true, 1, NodeFilter::NamedAndConnected), a);
        assert_eq!(fingerprint(&state.doc, false, 1, NodeFilter::NamedAndConnected), a_struct);
        assert_ne!(fingerprint(&state.doc, false, 2, NodeFilter::NamedAndConnected), a_struct);
    }
}
