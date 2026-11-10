mod patch;
mod units;
mod validated;
pub use patch::{apply_patch, encode_patch, expand_changes, is_patch, patch_changes};
pub use validated::{OutsideRange, Sizes, ValidatedSnapshot, check_units};

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail, ensure};
use hp_kv::Block;
use serde::{Deserialize, Serialize};

pub const MAX_MAP_BYTES: usize = 500 * 1024 * 1024;
pub const MAX_CHANGE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_UNITS: usize = 200_000;
pub const ID_STRIDE: u32 = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub owner: u32,
    #[serde(default)]
    pub hidden: bool,
    pub vmf: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Snapshot(pub BTreeMap<String, Unit>);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub key: String,
    pub before: Option<[u8; 32]>,
    pub after: Option<Unit>,
}

pub fn digest(unit: &Unit) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(&unit.owner.to_le_bytes());
    hash.update(&[u8::from(unit.hidden)]);
    hash.update(unit.vmf.as_bytes());
    *hash.finalize().as_bytes()
}

fn object_id(block: &Block) -> Result<u32> {
    let id = block.get_parsed::<u32>("id").unwrap_or(0);
    ensure!(id > 0 && id < 2_000_000_000, "Invalid map object identifier");
    Ok(id)
}

const PATH_KEYS: [&str; 8] =
    ["file", "model", "material", "texture", "detailmaterial", "skyname", "vscripts", "vscript_init_code"];
const SCRIPT_CLASSES: [&str; 4] = ["lua_run", "logic_script", "point_servercommand", "point_clientcommand"];
const SCRIPT_INPUTS: [&str; 4] = ["command", "runscriptcode", "runscriptfile", "runpassedcode"];

pub fn reference_problem(value: &str, instance: bool) -> Option<&'static str> {
    if value.contains('\0') {
        return Some("contains a null character");
    }
    let path = value.trim().replace('\\', "/");
    if path.starts_with("//") {
        return Some("network paths cannot be shared");
    }
    if instance && path.starts_with('/') {
        return Some("absolute paths cannot be shared");
    }
    let path = path.trim_start_matches('/');
    let parent_levels = if instance { 2 } else { 0 };
    if path.contains(':') {
        return Some("drive letters and other absolute locations cannot be shared");
    }
    let mut depth = 0isize;
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < -(parent_levels as isize) {
                    return Some("the path leaves its content folder");
                }
            }
            _ => depth += 1,
        }
    }
    None
}

pub fn script_class(classname: &str) -> bool {
    SCRIPT_CLASSES.iter().any(|class| classname.trim().eq_ignore_ascii_case(class))
}

pub fn script_input(output: &str, value: &str) -> bool {
    let input = hp_vmf::Connection::parse(output, value).input;
    SCRIPT_INPUTS.iter().any(|name| input.trim().eq_ignore_ascii_case(name))
}

pub fn script_problem(block: &str, key: &str, value: &str) -> Option<String> {
    if block.eq_ignore_ascii_case("connections") {
        return script_input(key, value)
            .then(|| format!("output \"{key}\" \"{value}\" refused: script and command inputs cannot be shared"));
    }
    (key.eq_ignore_ascii_case("classname") && script_class(value))
        .then(|| format!("class \"{value}\" refused: script and command entities cannot be shared"))
}

fn contains_ignore_case(text: &str, needle: &str) -> bool {
    text.as_bytes().windows(needle.len()).any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn scan_scripts(key: &str, unit: &Unit, mut found: impl FnMut(&str, &str, &str)) {
    if !key.starts_with("entity:")
        || !SCRIPT_CLASSES.iter().chain(&SCRIPT_INPUTS).any(|name| contains_ignore_case(&unit.vmf, name))
    {
        return;
    }
    let Ok(root) = hp_kv::parse_bounded(&unit.vmf, 128) else { return };
    for entity in &root.children {
        let classname = entity.get("classname").unwrap_or_default();
        found(classname, "classname", classname);
        for connections in entity.children.iter().filter(|child| child.name.eq_ignore_ascii_case("connections")) {
            for (output, value) in &connections.props {
                found(classname, output, value);
            }
        }
    }
}

pub fn unit_scripts(key: &str, unit: &Unit) -> Vec<String> {
    let mut uses = Vec::new();
    scan_scripts(key, unit, |classname, field, value| {
        if field == "classname" {
            if script_class(value) {
                uses.push(format!("{value} ({key})"));
            }
        } else if script_input(field, value) {
            let input = hp_vmf::Connection::parse(field, value).input;
            let owner = if classname.is_empty() { "entity" } else { classname };
            uses.push(format!("{field} > {input} ({owner}, {key})"));
        }
    });
    uses
}

pub fn script_refusal(changes: &[Change]) -> Option<String> {
    changes.iter().find_map(|change| {
        let unit = change.after.as_ref()?;
        let mut problem = None;
        scan_scripts(&change.key, unit, |_, field, value| {
            if problem.is_none() {
                let block = if field == "classname" { "entity" } else { "connections" };
                problem = script_problem(block, field, value);
            }
        });
        problem.map(|problem| format!("{}: {problem}", change.key))
    })
}

pub fn property_problem(classname: Option<&str>, key: &str, value: &str) -> Option<String> {
    let lower = key.to_ascii_lowercase();
    let class = |name: &str| classname.is_some_and(|class| class.trim().eq_ignore_ascii_case(name));
    let instance = class("func_instance") && lower == "file";
    if instance && !value.trim().to_ascii_lowercase().ends_with(".vmf") {
        return Some(format!("key \"{key}\" value \"{value}\" refused: instances must reference VMF files"));
    }
    let path = PATH_KEYS.contains(&lower.as_str()) || (class("ambient_generic") && lower == "message");
    if path && let Some(reason) = reference_problem(value, instance) {
        return Some(format!("key \"{key}\" value \"{value}\" refused: {reason}"));
    }
    None
}

pub fn sanitize_entity(entity: &mut hp_vmf::Entity, scripts: bool) -> std::result::Result<Vec<String>, String> {
    let label = if entity.classname.is_empty() { "entity".to_owned() } else { entity.classname.clone() };
    if !scripts && script_class(&entity.classname) {
        return Err(format!(
            "{label} {}: class \"{}\" refused: script and command entities cannot be shared, it stays on this PC",
            entity.id, entity.classname
        ));
    }
    let mut problems = Vec::new();
    let classname = entity.classname.clone();
    let id = entity.id;
    entity.props.retain(|(key, value)| match property_problem(Some(&classname), key, value) {
        Some(problem) => {
            problems.push(format!("{label} {id}: {problem}, it stays on this PC"));
            false
        }
        None => true,
    });
    entity.connections.retain(|connection| {
        let refused = !scripts && SCRIPT_INPUTS.iter().any(|name| connection.input.trim().eq_ignore_ascii_case(name));
        if refused {
            problems.push(format!(
                "{label} {id}: output \"{}\" to \"{}\" input \"{}\" refused: script and command inputs cannot be shared, it stays on this PC",
                connection.output, connection.target, connection.input
            ));
        }
        !refused
    });
    Ok(problems)
}

fn validate_block(block: &Block, depth: usize) -> Result<()> {
    ensure!(depth < 128, "Map nesting limit exceeded");
    ensure!(block.name.len() <= 128, "Map block name limit exceeded");
    ensure!(
        depth == 0
            || !block.name.is_empty()
                && !block.name.starts_with('[')
                && !block.name.contains("//")
                && !block.name.bytes().any(|byte| byte.is_ascii_whitespace() || matches!(byte, b'"' | b'{' | b'}')),
        "Invalid map block name"
    );
    ensure!(block.props.len() <= 8192 && block.children.len() <= MAX_UNITS, "Map block limit exceeded");
    let mut keys = BTreeSet::new();
    for (key, value) in &block.props {
        ensure!(key.len() <= 512 && value.len() <= 16_384 && !value.contains('\0'), "Map field limit exceeded");
        ensure!(!key.contains('"') && !value.contains('"'), "Invalid map field");
        let key = key.to_ascii_lowercase();
        let repeated = block.name.eq_ignore_ascii_case("connections")
            || (block.name.eq_ignore_ascii_case("editor") && key == "visgroupid")
            || (block.name.eq_ignore_ascii_case("vertices_plus") && key == "v");
        ensure!(repeated || keys.insert(key.clone()), "Duplicate map property '{key}' in '{}'", block.name);
        if block.name.eq_ignore_ascii_case("vertices_plus") && key == "v" {
            let mut components = 0;
            let valid = value
                .split(|character: char| character.is_ascii_whitespace() || matches!(character, '(' | ')' | '[' | ']'))
                .filter(|part| !part.is_empty())
                .all(|part| {
                    components += 1;
                    part.parse::<f64>()
                        .is_ok_and(|coordinate| coordinate.is_finite() && coordinate.abs() <= 1_000_000.0)
                });
            ensure!(valid && components == 3, "Invalid Hammer++ vertex coordinates");
        }
        let vector = matches!(key.as_str(), "origin" | "angles" | "mins" | "maxs")
            || (block.name.eq_ignore_ascii_case("dispinfo") && key == "startposition");
        if vector && !value.trim().is_empty() {
            ensure!(
                hp_vmf::parse_vec3(value)
                    .is_some_and(|vector| vector.is_finite() && vector.abs().max_element() <= 1_000_000.0),
                "Invalid vector property '{key}' in '{}'",
                block.name
            );
        }
        if !block.name.eq_ignore_ascii_case("connections")
            && let Some(problem) = property_problem(block.get("classname"), &key, value)
        {
            bail!("{problem}");
        }
    }
    for child in &block.children {
        validate_block(child, depth + 1)?;
    }
    Ok(())
}

fn unhide(blocks: Vec<Block>) -> Result<Vec<(Block, bool)>> {
    let mut result = Vec::new();
    for block in blocks {
        if block.name.eq_ignore_ascii_case("hidden") {
            ensure!(block.props.is_empty(), "Invalid hidden object container");
            result.extend(unhide(block.children)?.into_iter().map(|(block, _)| (block, true)));
        } else {
            result.push((block, false));
        }
    }
    Ok(result)
}

fn wrap_hidden(block: Block, hidden: bool) -> Block {
    if hidden { Block { name: "hidden".into(), children: vec![block], ..Default::default() } } else { block }
}

impl Snapshot {
    pub fn from_vmf(text: &str) -> Result<Self> {
        let snapshot = Self::split(text)?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    fn split(text: &str) -> Result<Self> {
        ensure!(text.len() <= MAX_MAP_BYTES, "Map exceeds the session size limit");
        let root = hp_kv::parse_bounded(text, 128)?;
        validate_block(&root, 0)?;
        let mut units = BTreeMap::new();
        let mut metadata = Block::default();
        let mut all_ids = BTreeSet::new();
        for (mut block, hidden) in unhide(root.children)? {
            match block.name.to_ascii_lowercase().as_str() {
                "world" | "entity" => {
                    block.name.make_ascii_lowercase();
                    ensure!(block.name != "world" || !hidden, "The world cannot be hidden");
                    let id = object_id(&block)?;
                    ensure!(all_ids.insert(id), "Duplicate object identifier");
                    let owner = id;
                    let mut header = Vec::new();
                    for (mut child, child_hidden) in unhide(std::mem::take(&mut block.children))? {
                        if child.name.eq_ignore_ascii_case("solid") {
                            child.name.make_ascii_lowercase();
                            let solid_id = object_id(&child)?;
                            ensure!(all_ids.insert(solid_id), "Duplicate object identifier");
                            let key = format!("solid:{solid_id}");
                            units.insert(key, Unit { owner, hidden: child_hidden, vmf: child.to_kv_string() });
                        } else {
                            ensure!(!child_hidden, "Only map objects can be hidden");
                            header.push(child);
                        }
                    }
                    block.children = header;
                    let key = if block.name == "world" { "world".to_owned() } else { format!("entity:{id}") };
                    ensure!(
                        units.insert(key, Unit { owner: 0, hidden, vmf: block.to_kv_string() }).is_none(),
                        "Duplicate world"
                    );
                }
                "versioninfo" | "cameras" | "viewsettings" | "cordon" | "cordons" => {}
                _ => {
                    ensure!(!hidden, "Only map objects can be hidden");
                    metadata.children.push(block);
                }
            }
        }
        units.insert("metadata".to_owned(), Unit { owner: 0, hidden: false, vmf: hp_kv::write(&metadata) });
        ensure!(units.contains_key("world") && units.len() <= MAX_UNITS, "Invalid shared map");
        Ok(Self(units))
    }

    pub fn to_vmf(&self) -> Result<String> {
        let mut world = None;
        let mut entities = BTreeMap::new();
        let mut solids = Vec::new();
        let metadata = self.0.get("metadata").ok_or_else(|| anyhow::anyhow!("Missing map metadata"))?;
        ensure!(metadata.owner == 0 && !metadata.hidden, "Invalid map metadata owner");
        let mut root = hp_kv::parse_bounded(&metadata.vmf, 128)?;
        ensure!(
            root.props.is_empty()
                && root.children.iter().all(|block| !matches!(
                    block.name.to_ascii_lowercase().as_str(),
                    "world"
                        | "entity"
                        | "solid"
                        | "hidden"
                        | "versioninfo"
                        | "cameras"
                        | "viewsettings"
                        | "cordon"
                        | "cordons"
                )),
            "Invalid map metadata"
        );
        for (key, unit) in &self.0 {
            if key == "metadata" {
                continue;
            }
            let mut parsed = hp_kv::parse_bounded(&unit.vmf, 128)?;
            ensure!(parsed.children.len() == 1 && parsed.props.is_empty(), "Invalid map unit");
            let block = parsed.children.remove(0);
            let id = object_id(&block)?;
            if key == "world" || key.starts_with("entity:") {
                ensure!(
                    block.children.iter().all(|child| !matches!(
                        child.name.to_ascii_lowercase().as_str(),
                        "world" | "entity" | "solid" | "hidden"
                    )),
                    "Nested map objects must use separate units"
                );
            }
            if key == "world" {
                ensure!(block.name == "world" && unit.owner == 0 && !unit.hidden, "Invalid world");
                world = Some(block);
            } else if key == &format!("entity:{id}") {
                ensure!(block.name == "entity" && unit.owner == 0, "Invalid entity");
                entities.insert(id, (block, unit.hidden));
            } else if key == &format!("solid:{id}") {
                ensure!(block.name == "solid", "Invalid solid");
                solids.push((unit.owner, block, unit.hidden));
            } else {
                bail!("Unknown map unit");
            }
        }
        let mut world = world.ok_or_else(|| anyhow::anyhow!("Missing world"))?;
        let world_id = object_id(&world)?;
        for (owner, solid, hidden) in solids {
            let target = if owner == world_id {
                &mut world
            } else {
                &mut entities.get_mut(&owner).ok_or_else(|| anyhow::anyhow!("Missing brush entity owner"))?.0
            };
            target.children.push(wrap_hidden(solid, hidden));
        }
        root.children.push(world);
        root.children.extend(entities.into_values().map(|(block, hidden)| wrap_hidden(block, hidden)));
        Ok(hp_kv::write(&root))
    }

    pub fn validate(&self) -> Result<()> {
        validated::check_snapshot(self)
    }

    #[cfg(test)]
    pub(crate) fn reference_sizes(&self) -> Result<(usize, usize, usize)> {
        ensure!(self.0.len() <= MAX_UNITS, "Map object limit exceeded");
        let bytes: usize = self.0.iter().map(|(key, unit)| key.len() + unit.vmf.len() + 4).sum();
        ensure!(bytes <= MAX_MAP_BYTES, "Map exceeds the session size limit");
        let encoded_bytes = serde_json::to_vec(self)?.len();
        ensure!(encoded_bytes <= MAX_MAP_BYTES, "Encoded map exceeds the session size limit");
        let text = self.to_vmf()?;
        ensure!(text.len() <= MAX_MAP_BYTES, "Map exceeds the session size limit");
        validate_block(&hp_kv::parse_bounded(&text, 128)?, 0)?;
        let map = hp_vmf::parse(&text)?;
        let mut objects = BTreeSet::new();
        let mut sides = BTreeSet::new();
        let mut geometry_budget = 0u64;
        for entity in std::iter::once(&map.world).chain(&map.entities) {
            ensure!(objects.insert(entity.id), "Duplicate object identifier");
            for solid in &entity.solids {
                ensure!(objects.insert(solid.id) && solid.sides.len() <= 128, "Invalid solid");
                geometry_budget = geometry_budget.saturating_add((solid.sides.len() as u64).pow(3));
                ensure!(geometry_budget <= 100_000_000, "Shared brush complexity limit exceeded");
                for side in &solid.sides {
                    ensure!(sides.insert(side.id), "Duplicate side identifier");
                    ensure!(
                        side.plane.iter().all(|point| point.is_finite() && point.abs().max_element() <= 1_000_000.0),
                        "Invalid map coordinates"
                    );
                    for axis in [&side.uaxis, &side.vaxis] {
                        ensure!(
                            axis.axis.is_finite() && axis.shift.is_finite() && axis.scale.is_finite(),
                            "Invalid texture alignment"
                        );
                    }
                    if let Some(displacement) = &side.disp {
                        ensure!(
                            displacement.power <= hp_vmf::MAX_DISP_POWER
                                && displacement.start_position.is_finite()
                                && displacement.elevation.is_finite(),
                            "Invalid displacement"
                        );
                        ensure!(
                            displacement
                                .normals
                                .iter()
                                .chain(&displacement.offsets)
                                .chain(&displacement.offset_normals)
                                .all(|vector| vector.is_finite()),
                            "Invalid displacement vectors"
                        );
                        ensure!(
                            displacement.distances.iter().chain(&displacement.alphas).all(|value| value.is_finite()),
                            "Invalid displacement values"
                        );
                    }
                }
            }
        }
        Ok((bytes, encoded_bytes, text.len()))
    }

    pub fn diff(&self, other: &Self) -> Vec<Change> {
        let keys: BTreeSet<_> = self.0.keys().chain(other.0.keys()).collect();
        keys.into_iter()
            .filter_map(|key| {
                let (before, after) = (self.0.get(key), other.0.get(key));
                (before != after).then(|| Change {
                    key: key.clone(),
                    before: before.map(digest),
                    after: after.cloned(),
                })
            })
            .collect()
    }

    pub fn apply(&mut self, changes: &[Change]) -> Result<()> {
        validated::apply_to(self, changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Snapshot {
        Snapshot::from_vmf("world {\"id\" \"1\" \"classname\" \"worldspawn\"}\nentity {\"id\" \"2\" \"classname\" \"light\" \"origin\" \"0 0 0\"}\nentity {\"id\" \"3\" \"classname\" \"light\"}").unwrap()
    }

    #[test]
    fn independent_edits_merge_and_conflicts_are_atomic() {
        let base = map();
        let mut a = base.clone();
        a.0.get_mut("entity:2").unwrap().vmf = a.0["entity:2"].vmf.replace("0 0 0", "8 0 0");
        let mut b = base.clone();
        b.0.remove("entity:3");
        let mut host = base.clone();
        host.apply(&base.diff(&a)).unwrap();
        host.apply(&base.diff(&b)).unwrap();
        assert_eq!(host.0.get("entity:2"), a.0.get("entity:2"));
        let saved = host.clone();
        assert!(host.apply(&base.diff(&a)).is_err());
        assert_eq!(host, saved);
        assert_eq!(Snapshot::from_vmf(&host.to_vmf().unwrap()).unwrap(), host);
    }

    #[test]
    fn untrusted_maps_cannot_request_local_paths_and_script_entities_are_shared() {
        for (class, key, value) in [
            ("func_instance", "file", "../../../private.vmf"),
            ("prop_dynamic", "model", "C:/secret.mdl"),
            ("logic_script", "vscripts", "C:/x.nut"),
        ] {
            let text = format!(
                "world {{\"id\" \"1\"}} entity {{\"id\" \"2\" \"classname\" \"{class}\" \"{key}\" \"{value}\"}}"
            );
            assert!(Snapshot::from_vmf(&text).is_err(), "{class}");
        }
        for (class, key, value) in [
            ("lua_run", "Code", "print(1)"),
            ("logic_script", "vscripts", "x.nut"),
            ("point_servercommand", "targetname", "commands"),
            ("point_clientcommand", "targetname", "client"),
        ] {
            let text = format!(
                "world {{\"id\" \"1\"}} entity {{\"id\" \"2\" \"classname\" \"{class}\" \"{key}\" \"{value}\"}}"
            );
            let snapshot = Snapshot::from_vmf(&text).unwrap();
            assert_eq!(unit_scripts("entity:2", &snapshot.0["entity:2"]), vec![format!("{class} (entity:2)")]);
        }
    }

    #[test]
    fn script_uses_are_found_and_refused_with_a_precise_message_when_the_host_asks() {
        let relay = Unit {
            owner: 0,
            hidden: false,
            vmf: "entity {\"id\" \"2\" \"classname\" \"logic_relay\" connections {\"OnTrigger\" \"!player,Command,say hi,0,-1\" \"OnTrigger\" \"door,Open,,0,-1\"}}".into(),
        };
        assert_eq!(unit_scripts("entity:2", &relay), vec!["OnTrigger > Command (logic_relay, entity:2)".to_owned()]);
        let lua = Unit {
            owner: 0,
            hidden: false,
            vmf: "entity {\"id\" \"3\" \"classname\" \"LUA_RUN\" \"Code\" \"print(1)\"}".into(),
        };
        let plain = Unit { owner: 0, hidden: false, vmf: "entity {\"id\" \"4\" \"classname\" \"light\"}".into() };
        assert!(unit_scripts("entity:4", &plain).is_empty());
        assert!(unit_scripts("solid:3", &lua).is_empty(), "only entity units carry scripts");
        let change = |key: &str, unit: &Unit| Change { key: key.into(), before: None, after: Some(unit.clone()) };
        assert_eq!(script_refusal(&[change("entity:4", &plain)]), None);
        assert_eq!(script_refusal(&[Change { key: "entity:3".into(), before: None, after: None }]), None);
        assert_eq!(
            script_refusal(&[change("entity:4", &plain), change("entity:3", &lua)]).as_deref(),
            Some("entity:3: class \"LUA_RUN\" refused: script and command entities cannot be shared")
        );
        assert_eq!(
            script_refusal(&[change("entity:2", &relay)]).as_deref(),
            Some(
                "entity:2: output \"OnTrigger\" \"!player,Command,say hi,0,-1\" refused: script and command inputs cannot be shared"
            )
        );
    }

    #[test]
    fn legitimate_references_are_shared_and_refusals_name_the_entity_key_and_value() {
        for (class, key, value) in [
            ("prop_static", "model", "models\\props_c17\\oildrum001.mdl"),
            ("func_instance", "file", "..\\instances\\door.vmf"),
            ("func_instance", "file", "../../instances/door.vmf"),
            ("infodecal", "texture", "decals/../decals/lambdaspray_2a"),
            ("ambient_generic", "message", "#music\\hl2_song1.mp3"),
            ("infodecal", "texture", "/OP_FORGE"),
            ("info_overlay", "material", "\\decals\\graffiti01"),
        ] {
            let text = format!(
                "world {{\"id\" \"1\"}} entity {{\"id\" \"2\" \"classname\" \"{class}\" \"{key}\" \"{value}\"}}"
            );
            assert!(Snapshot::from_vmf(&text).is_ok(), "{class} {key} {value}");
        }
        for (class, key, value, reason) in [
            ("prop_static", "model", "C:\\models\\x.mdl", "drive letters"),
            ("prop_static", "model", "\\\\server\\share\\x.mdl", "network paths"),
            ("func_instance", "file", "/instances/door.vmf", "absolute paths"),
            ("prop_static", "model", "../models/x.mdl", "leaves its content folder"),
            ("func_instance", "file", "../../../x.vmf", "leaves its content folder"),
            ("func_instance", "file", "door.txt", "VMF files"),
        ] {
            let mut host = map();
            let unit = Unit {
                owner: 0,
                hidden: false,
                vmf: format!("entity {{\"id\" \"2\" \"classname\" \"{class}\" \"{key}\" \"{value}\"}}"),
            };
            let change =
                Change { key: "entity:2".into(), before: host.0.get("entity:2").map(digest), after: Some(unit) };
            let error = host.apply(&[change]).unwrap_err().to_string();
            assert!(
                error.starts_with(&format!("entity 2 ({class}): key \"{key}\" value \"{value}\" refused")),
                "{error}"
            );
            assert!(error.contains(reason), "{error}");
        }
        let mut entity = hp_vmf::Entity { id: 7, classname: "prop_static".into(), ..Default::default() };
        entity.set("model", "C:/x.mdl");
        entity.set("skin", "1");
        entity.connections.push(hp_vmf::Connection::parse("OnTrigger", "!self,RunScriptCode,x,0,-1"));
        entity.connections.push(hp_vmf::Connection::parse("OnTrigger", "door,Open,,0,-1"));
        let mut kept = entity.clone();
        let problems = sanitize_entity(&mut entity, false).unwrap();
        assert_eq!(problems.len(), 2);
        assert!(problems[0].contains("prop_static 7") && problems[0].contains("C:/x.mdl"));
        assert_eq!(entity.get("model"), None);
        assert_eq!(entity.get("skin"), Some("1"));
        assert_eq!(entity.connections.len(), 1);
        assert_eq!(
            sanitize_entity(&mut kept, true).unwrap().len(),
            1,
            "only the path is refused when scripts are shared"
        );
        assert_eq!(kept.connections.len(), 2);
        let mut script = hp_vmf::Entity { id: 8, classname: "lua_run".into(), ..Default::default() };
        assert!(sanitize_entity(&mut script.clone(), false).unwrap_err().contains("lua_run"));
        assert_eq!(sanitize_entity(&mut script, true), Ok(Vec::new()));
    }

    #[test]
    fn malformed_or_privileged_changes_leave_the_host_document_unchanged() {
        let mut host = map();
        let original = host.clone();
        for (key, unit) in [
            ("metadata", Unit { owner: 0, hidden: false, vmf: "world {\"id\" \"100\"}".into() }),
            ("metadata", Unit { owner: 0, hidden: false, vmf: "WORLD {\"id\" \"100\"}".into() }),
            (
                "metadata",
                Unit {
                    owner: 0,
                    hidden: false,
                    vmf: "Hidden {entity {\"id\" \"100\" \"classname\" \"light\"}}".into(),
                },
            ),
            (
                "entity:2",
                Unit {
                    owner: 0,
                    hidden: false,
                    vmf: "entity {\"id\" \"2\" \"classname\" \"ambient_generic\" \"message\" \"C:/private.wav\"}"
                        .into(),
                },
            ),
        ] {
            let change = Change { key: key.into(), before: original.0.get(key).map(digest), after: Some(unit) };
            assert!(host.apply(&[change]).is_err());
            assert_eq!(host, original);
        }
        let duplicate = Change { key: "entity:2".into(), before: original.0.get("entity:2").map(digest), after: None };
        assert!(host.apply(&[duplicate.clone(), duplicate]).is_err());
        assert_eq!(host, original);
    }

    #[test]
    fn hidden_entities_and_brushes_are_individual_units_and_local_cordons_are_excluded() {
        let text = "world {\"id\" \"1\" \"classname\" \"worldspawn\" hidden {solid {\"id\" \"5\"}}} hidden {entity {\"id\" \"2\" \"classname\" \"light\"}} cordon {\"mins\" \"(-1024 -1024 -1024)\" \"maxs\" \"(1024 1024 1024)\" \"active\" \"0\"}";
        let snapshot = Snapshot::from_vmf(text).unwrap();
        assert!(snapshot.0["solid:5"].hidden);
        assert_eq!(snapshot.0["solid:5"].owner, 1);
        assert!(snapshot.0["entity:2"].hidden);
        assert!(!snapshot.0["metadata"].vmf.contains("cordon"));
        assert!(!snapshot.0["world"].vmf.contains("solid"));
        let restored = snapshot.to_vmf().unwrap();
        let map = hp_vmf::parse(&restored).unwrap();
        assert!(map.world.solids[0].hidden && map.entities[0].hidden);
        assert_eq!(Snapshot::from_vmf(&restored).unwrap(), snapshot);
    }

    #[test]
    fn property_and_nesting_limits_are_checked_before_typed_geometry() {
        let oversized = format!("world {{\"id\" \"1\" \"x\" \"{}\"}}", "x".repeat(16_385));
        assert!(Snapshot::from_vmf(&oversized).is_err());
        let nested = format!("world {{\"id\" \"1\" {} {} }}", "x {".repeat(130), "}".repeat(130));
        assert!(Snapshot::from_vmf(&nested).is_err());
    }

    #[test]
    fn multiple_visgroup_memberships_can_be_hosted_and_shared() {
        let text = "world {\"id\" \"1\" solid {\"id\" \"5\" editor {\"visgroupid\" \"10\" \"visgroupid\" \"20\"}}} entity {\"id\" \"2\" \"classname\" \"func_detail\" solid {\"id\" \"6\" editor {\"visgroupid\" \"10\" \"VISGROUPID\" \"20\"}}}";
        let hosted = Snapshot::from_vmf(text).unwrap();
        let parsed = hp_vmf::parse(&hosted.to_vmf().unwrap()).unwrap();
        assert_eq!(parsed.world.solids[0].editor.visgroup_ids, [10, 20]);
        assert_eq!(parsed.entities[0].solids[0].editor.visgroup_ids, [10, 20]);
        let mut remote = map();
        remote.apply(&remote.diff(&hosted)).unwrap();
        assert_eq!(remote, hosted);
    }

    #[test]
    fn ambiguous_single_value_properties_are_still_rejected() {
        for text in [
            "world {\"id\" \"1\" \"ID\" \"2\"}",
            "world {\"id\" \"1\" editor {\"color\" \"1 2 3\" \"COLOR\" \"4 5 6\"}}",
            "world {\"id\" \"1\"} entity {\"id\" \"2\" \"classname\" \"light\" \"CLASSNAME\" \"lua_run\"}",
        ] {
            assert!(Snapshot::from_vmf(text).unwrap_err().to_string().contains("Duplicate map property"));
        }
    }

    #[test]
    fn hammerplusplus_vertex_loops_can_be_hosted_and_shared() {
        let text = "world {\"id\" \"1\" solid {\"id\" \"5\" side {\"id\" \"6\" \"plane\" \"(0 0 64) (64 0 64) (64 64 64)\" vertices_plus {\"v\" \"0 0 64\" \"v\" \"64 0 64\" \"v\" \"64 64 64\" \"v\" \"0 64 64\"}}}}";
        let hosted = Snapshot::from_vmf(text).unwrap();
        let parsed = hp_vmf::parse(&hosted.to_vmf().unwrap()).unwrap();
        let vertices = parsed.world.solids[0].sides[0].vertices_plus.as_ref().unwrap();
        assert_eq!(vertices.len(), 4);
        assert_eq!(vertices[3].to_array(), [0.0, 64.0, 64.0]);
        let mut remote = map();
        remote.apply(&remote.diff(&hosted)).unwrap();
        assert_eq!(remote, hosted);
        for value in ["NaN 0 0", "inf 0 0", "1000001 0 0", "1 2", "1 2 3 4"] {
            assert!(Snapshot::from_vmf(&text.replace("0 64 64", value)).is_err());
        }
    }

    #[test]
    fn scalar_entity_start_positions_can_be_hosted_and_shared() {
        for class in ["func_movelinear", "func_water_analog", "momentary_door"] {
            for value in ["0", "0.5", "1"] {
                let text = format!(
                    "world {{\"id\" \"1\"}} entity {{\"id\" \"2\" \"classname\" \"{class}\" \"origin\" \"32 0 0\" \"startposition\" \"{value}\"}}"
                );
                let hosted = Snapshot::from_vmf(&text).unwrap();
                let parsed = hp_vmf::parse(&hosted.to_vmf().unwrap()).unwrap();
                assert_eq!(parsed.entities[0].get("startposition"), Some(value));
                let mut remote = map();
                remote.apply(&remote.diff(&hosted)).unwrap();
                assert_eq!(remote, hosted);
            }
        }
    }

    #[test]
    fn malformed_vectors_are_rejected_with_property_and_block_names() {
        for key in ["origin", "angles", "mins", "maxs"] {
            for value in ["1 2", "NaN 0 0", "1000001 0 0"] {
                let mut host = map();
                let original = host.clone();
                let unit = Unit {
                    owner: 0,
                    hidden: false,
                    vmf: format!("entity {{\"id\" \"2\" \"classname\" \"info_target\" \"{key}\" \"{value}\"}}"),
                };
                let change =
                    Change { key: "entity:2".into(), before: host.0.get("entity:2").map(digest), after: Some(unit) };
                assert_eq!(
                    host.apply(&[change]).unwrap_err().to_string(),
                    format!("entity 2 (info_target): Invalid vector property '{key}' in 'entity'")
                );
                assert_eq!(host, original);
            }
        }
        for value in ["0", "1 2", "NaN 0 0", "1000001 0 0"] {
            let text = format!("world {{\"id\" \"1\" dispinfo {{\"startposition\" \"{value}\"}}}}");
            assert_eq!(
                Snapshot::from_vmf(&text).unwrap_err().to_string(),
                "Invalid vector property 'startposition' in 'dispinfo'"
            );
        }
        assert!(Snapshot::from_vmf("world {\"id\" \"1\" dispinfo {\"startposition\" \"[0 0 64]\"}}").is_ok());
    }
}
