use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail, ensure};
use hp_kv::Block;
use serde::{Deserialize, Serialize};

pub const MAX_MAP_BYTES: usize = 64 * 1024 * 1024;
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

fn safe_reference(value: &str) -> bool {
    !value.contains([':', '\0', '\\']) && !value.starts_with('/') && !value.split('/').any(|part| part == "..")
}

fn validate_block(block: &Block, depth: usize) -> Result<()> {
    ensure!(depth < 128, "Map nesting limit exceeded");
    ensure!(block.name.len() <= 128, "Map block name limit exceeded");
    ensure!(block.props.len() <= 8192 && block.children.len() <= MAX_UNITS, "Map block limit exceeded");
    let mut keys = BTreeSet::new();
    for (key, value) in &block.props {
        ensure!(key.len() <= 512 && value.len() <= 16_384 && !value.contains('\0'), "Map field limit exceeded");
        let key = key.to_ascii_lowercase();
        ensure!(block.name.eq_ignore_ascii_case("connections") || keys.insert(key.clone()), "Duplicate map property");
        if matches!(key.as_str(), "origin" | "angles" | "mins" | "maxs" | "startposition") && !value.trim().is_empty() {
            ensure!(
                hp_vmf::parse_vec3(value)
                    .is_some_and(|vector| vector.is_finite() && vector.abs().max_element() <= 1_000_000.0),
                "Invalid vector property"
            );
        }
        if block.name.eq_ignore_ascii_case("connections") {
            let input = hp_vmf::Connection::parse(&key, value).input.to_ascii_lowercase();
            ensure!(
                !matches!(input.as_str(), "command" | "runscriptcode" | "runscriptfile" | "runpassedcode"),
                "Script and command inputs cannot be shared"
            );
        }
        if block.get("classname").is_some_and(|class| class.eq_ignore_ascii_case("ambient_generic")) && key == "message"
        {
            ensure!(safe_reference(value), "Unsafe sound resource path");
        }
        if block.get("classname").is_some_and(|class| class.eq_ignore_ascii_case("func_instance")) && key == "file" {
            ensure!(value.to_ascii_lowercase().ends_with(".vmf"), "Instances must reference VMF files");
        }
        if matches!(
            key.as_str(),
            "file" | "model" | "material" | "texture" | "detailmaterial" | "skyname" | "vscripts" | "vscript_init_code"
        ) {
            ensure!(safe_reference(value), "Unsafe resource path");
        }
        if key == "classname" {
            ensure!(
                !matches!(
                    value.to_ascii_lowercase().as_str(),
                    "lua_run" | "logic_script" | "point_servercommand" | "point_clientcommand"
                ),
                "Script and command entities cannot be shared"
            );
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
        let snapshot = Self(units);
        snapshot.validate()?;
        Ok(snapshot)
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
        ensure!(self.0.len() <= MAX_UNITS, "Map object limit exceeded");
        let bytes: usize = self.0.iter().map(|(key, unit)| key.len() + unit.vmf.len() + 4).sum();
        ensure!(bytes <= MAX_MAP_BYTES, "Map exceeds the session size limit");
        ensure!(serde_json::to_vec(self)?.len() <= MAX_MAP_BYTES, "Encoded map exceeds the session size limit");
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
        Ok(())
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
        ensure!(!changes.is_empty() && changes.len() <= 8192, "Invalid change count");
        ensure!(serde_json::to_vec(changes)?.len() <= MAX_CHANGE_BYTES, "Change exceeds the session size limit");
        let mut seen = BTreeSet::new();
        for change in changes {
            ensure!(change.key.len() <= 64 && seen.insert(&change.key), "Duplicate or invalid change key");
            ensure!(
                self.0.get(&change.key).map(digest) == change.before,
                "An object changed on another participant: {}",
                change.key
            );
        }
        let mut updated = self.clone();
        for change in changes {
            if let Some(unit) = &change.after {
                updated.0.insert(change.key.clone(), unit.clone());
            } else {
                updated.0.remove(&change.key);
            }
        }
        updated.validate()?;
        *self = updated;
        Ok(())
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
    fn untrusted_map_cannot_request_local_paths_or_script_execution() {
        for (class, key, value) in [
            ("func_instance", "file", "../private.vmf"),
            ("prop_dynamic", "model", "C:/secret.mdl"),
            ("lua_run", "code", "print(1)"),
            ("logic_script", "vscripts", "x.nut"),
        ] {
            let text = format!(
                "world {{\"id\" \"1\"}} entity {{\"id\" \"2\" \"classname\" \"{class}\" \"{key}\" \"{value}\"}}"
            );
            assert!(Snapshot::from_vmf(&text).is_err());
        }
    }

    #[test]
    fn malformed_or_privileged_changes_leave_the_host_document_unchanged() {
        let mut host = map();
        let original = host.clone();
        for (key, unit) in [
            ("metadata", Unit { owner: 0, hidden: false, vmf: "world {\"id\" \"100\"}".into() }),
            ("metadata", Unit { owner: 0, hidden: false, vmf: "WORLD {\"id\" \"100\"}".into() }),
            ("metadata", Unit { owner: 0, hidden: false, vmf: "Hidden {entity {\"id\" \"100\" \"classname\" \"light\"}}".into() }),
            ("entity:2", Unit { owner: 0, hidden: false, vmf: "entity {\"id\" \"2\" \"classname\" \"ambient_generic\" \"message\" \"C:/private.wav\"}".into() }),
            ("entity:2", Unit { owner: 0, hidden: false, vmf: "entity {\"id\" \"2\" \"classname\" \"logic_relay\" connections {\"OnTrigger\" \"!player,Command,quit,0,-1\"}}".into() }),
            ("entity:2", Unit { owner: 0, hidden: false, vmf: "entity {\"id\" \"2\" \"classname\" \"logic_relay\" Connections {\"OnTrigger\" \"light,RunScriptCode,x,0,-1\"}}".into() }),
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
}
