use std::collections::{HashMap, HashSet};
use std::ops::{Deref, Range};

use anyhow::{Result, bail, ensure};
use hp_kv::Block;
use rayon::prelude::*;

use super::{Change, MAX_CHANGE_BYTES, MAX_MAP_BYTES, MAX_UNITS, Snapshot, Unit, digest, object_id, validate_block};

const MAX_DEPTH: usize = 128;
const GEOMETRY_LIMIT: u64 = 100_000_000;
const PARALLEL: usize = 16;
const NESTED: [&str; 4] = ["world", "entity", "solid", "hidden"];
const METADATA_FORBIDDEN: [&str; 9] =
    ["world", "entity", "solid", "hidden", "versioninfo", "cameras", "viewsettings", "cordon", "cordons"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutsideRange {
    pub side: bool,
    pub id: u32,
}

impl std::fmt::Display for OutsideRange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(if self.side {
            "Side identifier is outside the participant range"
        } else {
            "Object identifier is outside the participant range"
        })
    }
}

impl std::error::Error for OutsideRange {}

#[derive(Clone, Debug)]
pub struct ValidatedSnapshot {
    snapshot: Snapshot,
    index: Index,
}

impl PartialEq for ValidatedSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.snapshot == other.snapshot
    }
}

impl Eq for ValidatedSnapshot {}

impl Deref for ValidatedSnapshot {
    type Target = Snapshot;

    fn deref(&self) -> &Snapshot {
        &self.snapshot
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sizes {
    pub units: usize,
    pub unit_bytes: usize,
    pub encoded_bytes: usize,
    pub vmf_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Header {
    hidden: bool,
    len: usize,
    lines: usize,
    children: usize,
    depth: usize,
    encoded: usize,
}

impl Header {
    fn contribution(&self) -> usize {
        if self.hidden { 11 + self.len + self.lines } else { self.len }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SolidInfo {
    owner: u32,
    hidden: bool,
    len: usize,
    lines: usize,
    depth: usize,
    sides: Box<[u32]>,
    encoded: usize,
}

impl SolidInfo {
    fn base(&self) -> usize {
        self.len + if self.hidden { 11 + self.lines } else { 0 }
    }

    fn step(&self) -> usize {
        self.lines + if self.hidden { 3 } else { 0 }
    }

    fn geometry(&self) -> u64 {
        (self.sides.len() as u64).pow(3)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Parsed {
    Metadata { len: usize, children: usize, encoded: usize },
    World { id: u32, header: Header },
    Entity { id: u32, header: Header },
    Solid { id: u32, info: SolidInfo },
}

#[derive(Clone, Debug, Default)]
struct Owner {
    solids: HashSet<u32>,
    step: usize,
}

#[derive(Clone, Debug, Default)]
struct Index {
    unit_bytes: usize,
    encoded_entries: usize,
    metadata: Option<(usize, usize, usize)>,
    world: Option<(u32, Header)>,
    entities: HashMap<u32, Header>,
    entity_len: usize,
    solids: HashMap<u32, SolidInfo>,
    owners: HashMap<u32, Owner>,
    sides: HashMap<u32, u32>,
    geometry: u64,
    solid_base: usize,
    hidden_step: usize,
    units: usize,
}

struct Counter(usize);

impl std::io::Write for Counter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0 += buffer.len();
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn json_len<T: serde::Serialize + ?Sized>(value: &T) -> Result<usize> {
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.0)
}

pub(crate) fn changes_len(changes: &[Change]) -> Result<usize> {
    json_len(changes)
}

fn shape(block: &Block) -> (usize, usize, usize) {
    let mut len = block.name.len() + 5;
    let mut lines = 3 + block.props.len();
    let mut depth = 0;
    for (key, value) in &block.props {
        len += key.len() + value.len() + 7;
    }
    for child in &block.children {
        let (child_len, child_lines, child_depth) = shape(child);
        len += child_len + child_lines;
        lines += child_lines;
        depth = depth.max(child_depth + 1);
    }
    (len, lines, depth)
}

fn lowercase_in(name: &str, list: &[&str]) -> bool {
    list.iter().any(|item| name.eq_ignore_ascii_case(item))
}

fn kind_id(key: &str, prefix: &str) -> Option<u32> {
    let id: u32 = key.strip_prefix(prefix)?.parse().ok()?;
    (format!("{prefix}{id}") == key).then_some(id)
}

fn check_solid(block: Block) -> Result<Box<[u32]>> {
    let mut world = Block::new("world");
    world.props.push(("id".into(), "1".into()));
    world.children.push(block);
    let mut root = Block::default();
    root.children.push(world);
    let map = hp_vmf::from_kv(root)?;
    let solid = map.world.solids.into_iter().next().ok_or_else(|| anyhow::anyhow!("Invalid solid"))?;
    ensure!(solid.sides.len() <= 128, "Invalid solid");
    let mut sides = Vec::with_capacity(solid.sides.len());
    for side in &solid.sides {
        ensure!(!sides.contains(&side.id), "Duplicate side identifier");
        sides.push(side.id);
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
    Ok(sides.into_boxed_slice())
}

fn parse_unit(key: &str, unit: &Unit) -> Result<Parsed> {
    let encoded = json_len(key)? + 1 + json_len(unit)?;
    if key == "metadata" {
        ensure!(unit.owner == 0 && !unit.hidden, "Invalid map metadata owner");
        let root = hp_kv::parse_bounded(&unit.vmf, MAX_DEPTH)?;
        ensure!(
            root.props.is_empty() && root.children.iter().all(|block| !lowercase_in(&block.name, &METADATA_FORBIDDEN)),
            "Invalid map metadata"
        );
        let mut len = 0;
        for child in &root.children {
            validate_block(child, 1)?;
            len += shape(child).0;
        }
        return Ok(Parsed::Metadata { len, children: root.children.len(), encoded });
    }
    let mut parsed = hp_kv::parse_bounded(&unit.vmf, MAX_DEPTH)?;
    ensure!(parsed.children.len() == 1 && parsed.props.is_empty(), "Invalid map unit");
    let block = parsed.children.remove(0);
    let id = object_id(&block)?;
    if key == "world" || key.starts_with("entity:") {
        ensure!(
            block.children.iter().all(|child| !lowercase_in(&child.name, &NESTED)),
            "Nested map objects must use separate units"
        );
    }
    let (len, lines, depth) = shape(&block);
    if key == "world" {
        ensure!(block.name == "world" && unit.owner == 0 && !unit.hidden, "Invalid world");
        validate_block(&block, 1)?;
        let header = Header { hidden: false, len, lines, children: block.children.len(), depth, encoded };
        Ok(Parsed::World { id, header })
    } else if kind_id(key, "entity:") == Some(id) {
        ensure!(block.name == "entity" && unit.owner == 0, "Invalid entity");
        validate_block(&block, 1 + usize::from(unit.hidden))?;
        let header = Header { hidden: unit.hidden, len, lines, children: block.children.len(), depth, encoded };
        Ok(Parsed::Entity { id, header })
    } else if kind_id(key, "solid:") == Some(id) {
        ensure!(block.name == "solid", "Invalid solid");
        validate_block(&block, 2)?;
        let sides = check_solid(block)?;
        let info = SolidInfo { owner: unit.owner, hidden: unit.hidden, len, lines, depth, sides, encoded };
        Ok(Parsed::Solid { id, info })
    } else {
        bail!("Unknown map unit")
    }
}

fn located(key: &str, unit: &Unit) -> String {
    let place = key.replacen(':', " ", 1);
    let class = unit
        .vmf
        .find("\"classname\" \"")
        .map(|start| &unit.vmf[start + 13..])
        .and_then(|rest| rest.split('"').next())
        .filter(|class| !class.is_empty() && class.len() <= 64);
    match class {
        Some(class) => format!("{place} ({class})"),
        None => place,
    }
}

fn parse_located(key: &str, unit: &Unit) -> Result<Parsed> {
    parse_unit(key, unit).map_err(|error| anyhow::anyhow!("{}: {error:#}", located(key, unit)))
}

fn parse_all<'a>(items: Vec<(&'a str, &'a Unit)>) -> Result<Vec<Parsed>> {
    if items.len() >= PARALLEL {
        items.into_par_iter().map(|(key, unit)| parse_located(key, unit)).collect()
    } else {
        items.into_iter().map(|(key, unit)| parse_located(key, unit)).collect()
    }
}

impl Index {
    fn owner_hidden(&self, owner: u32) -> bool {
        self.entities.get(&owner).is_some_and(|header| header.hidden)
    }

    fn world_id(&self) -> Option<u32> {
        self.world.as_ref().map(|(id, _)| *id)
    }

    fn add(&mut self, key: &str, unit: &Unit, parsed: Parsed, checked: bool) -> Result<()> {
        match parsed {
            Parsed::Metadata { len, children, encoded } => {
                ensure!(!checked || self.metadata.is_none(), "Duplicate map metadata");
                self.metadata = Some((len, children, encoded));
                self.encoded_entries += encoded;
            }
            Parsed::World { id, header } => {
                ensure!(
                    !checked
                        || (self.world.is_none() && !self.entities.contains_key(&id) && !self.solids.contains_key(&id)),
                    "Duplicate object identifier"
                );
                self.encoded_entries += header.encoded;
                self.world = Some((id, header));
            }
            Parsed::Entity { id, header } => {
                ensure!(
                    !checked
                        || (self.world_id() != Some(id)
                            && !self.solids.contains_key(&id)
                            && !self.entities.contains_key(&id)),
                    "Duplicate object identifier"
                );
                self.encoded_entries += header.encoded;
                self.entity_len += header.contribution();
                if header.hidden {
                    self.hidden_step += self.owners.get(&id).map_or(0, |owner| owner.step);
                }
                self.entities.insert(id, header);
            }
            Parsed::Solid { id, info } => {
                if checked {
                    ensure!(
                        self.world_id() != Some(id)
                            && !self.entities.contains_key(&id)
                            && !self.solids.contains_key(&id),
                        "Duplicate object identifier"
                    );
                    ensure!(info.sides.iter().all(|side| !self.sides.contains_key(side)), "Duplicate side identifier");
                }
                for side in &info.sides {
                    self.sides.insert(*side, id);
                }
                self.geometry += info.geometry();
                self.encoded_entries += info.encoded;
                self.solid_base += info.base() + info.step();
                if self.owner_hidden(info.owner) {
                    self.hidden_step += info.step();
                }
                let owner = self.owners.entry(info.owner).or_default();
                owner.solids.insert(id);
                owner.step += info.step();
                self.solids.insert(id, info);
            }
        }
        self.units += 1;
        self.unit_bytes += key.len() + unit.vmf.len() + 4;
        Ok(())
    }

    fn sub(&mut self, key: &str, unit: &Unit) -> Option<Parsed> {
        let parsed = if key == "metadata" {
            let (len, children, encoded) = self.metadata.take()?;
            self.encoded_entries -= encoded;
            Parsed::Metadata { len, children, encoded }
        } else if key == "world" {
            let (id, header) = self.world.take()?;
            self.encoded_entries -= header.encoded;
            Parsed::World { id, header }
        } else if let Some(id) = kind_id(key, "entity:") {
            let header = self.entities.remove(&id)?;
            self.encoded_entries -= header.encoded;
            self.entity_len -= header.contribution();
            if header.hidden {
                self.hidden_step -= self.owners.get(&id).map_or(0, |owner| owner.step);
            }
            Parsed::Entity { id, header }
        } else {
            let id = kind_id(key, "solid:")?;
            let info = self.solids.remove(&id)?;
            for side in &info.sides {
                if self.sides.get(side) == Some(&id) {
                    self.sides.remove(side);
                }
            }
            self.geometry -= info.geometry();
            self.encoded_entries -= info.encoded;
            self.solid_base -= info.base() + info.step();
            if self.owner_hidden(info.owner) {
                self.hidden_step -= info.step();
            }
            if let Some(owner) = self.owners.get_mut(&info.owner) {
                owner.solids.remove(&id);
                owner.step -= info.step();
                if owner.solids.is_empty() {
                    self.owners.remove(&info.owner);
                }
            }
            Parsed::Solid { id, info }
        };
        self.units -= 1;
        self.unit_bytes -= key.len() + unit.vmf.len() + 4;
        Some(parsed)
    }

    fn encoded_bytes(&self) -> usize {
        2 + self.encoded_entries + self.units.saturating_sub(1)
    }

    fn vmf_bytes(&self) -> usize {
        self.metadata.map_or(0, |(len, _, _)| len)
            + self.world.as_ref().map_or(0, |(_, header)| header.len)
            + self.entity_len
            + self.solid_base
            + self.hidden_step
    }

    fn sizes(&self) -> Sizes {
        Sizes {
            units: self.units,
            unit_bytes: self.unit_bytes,
            encoded_bytes: self.encoded_bytes(),
            vmf_bytes: self.vmf_bytes(),
        }
    }

    fn check_totals(&self) -> Result<()> {
        ensure!(self.units <= MAX_UNITS, "Map object limit exceeded");
        ensure!(self.unit_bytes <= MAX_MAP_BYTES, "Map exceeds the session size limit");
        ensure!(self.encoded_bytes() <= MAX_MAP_BYTES, "Encoded map exceeds the session size limit");
        let (world, metadata) = (self.world.as_ref(), self.metadata.as_ref());
        let (Some((_, world)), Some(metadata)) = (world, metadata) else {
            bail!(if self.metadata.is_none() { "Missing map metadata" } else { "Missing world" })
        };
        ensure!(self.vmf_bytes() <= MAX_MAP_BYTES, "Map exceeds the session size limit");
        ensure!(metadata.1 + 1 + self.entities.len() <= MAX_UNITS, "Map block limit exceeded");
        ensure!(world.depth + 1 < MAX_DEPTH, "Map nesting limit exceeded");
        ensure!(self.geometry <= GEOMETRY_LIMIT, "Shared brush complexity limit exceeded");
        Ok(())
    }

    fn check_owner(&self, owner: u32) -> Result<()> {
        let Some(row) = self.owners.get(&owner) else { return Ok(()) };
        let header = if self.world_id() == Some(owner) {
            self.world.as_ref().map(|(_, header)| header)
        } else {
            self.entities.get(&owner)
        };
        let header = header.ok_or_else(|| anyhow::anyhow!("Missing brush entity owner"))?;
        ensure!(header.children + row.solids.len() <= MAX_UNITS, "Map block limit exceeded");
        if header.hidden {
            for id in &row.solids {
                self.check_depth(*id)?;
            }
        }
        Ok(())
    }

    fn check_depth(&self, solid: u32) -> Result<()> {
        let Some(info) = self.solids.get(&solid) else { return Ok(()) };
        let depth = 2 + usize::from(self.owner_hidden(info.owner)) + usize::from(info.hidden) + info.depth;
        ensure!(depth < MAX_DEPTH, "Map nesting limit exceeded");
        Ok(())
    }

    fn build(snapshot: &Snapshot) -> Result<Self> {
        ensure!(snapshot.0.len() <= MAX_UNITS, "Map object limit exceeded");
        let bytes: usize = snapshot.0.iter().map(|(key, unit)| key.len() + unit.vmf.len() + 4).sum();
        ensure!(bytes <= MAX_MAP_BYTES, "Map exceeds the session size limit");
        let items: Vec<_> = snapshot.0.iter().map(|(key, unit)| (key.as_str(), unit)).collect();
        let parsed = parse_all(items)?;
        let mut index = Self::default();
        let mut solids = Vec::new();
        for ((key, unit), parsed) in snapshot.0.iter().zip(parsed) {
            if matches!(parsed, Parsed::Solid { .. }) {
                solids.push((key, unit, parsed));
            } else {
                index.add(key, unit, parsed, true)?;
            }
        }
        for (key, unit, parsed) in solids {
            index.add(key, unit, parsed, true)?;
        }
        index.check_totals()?;
        let owners: Vec<u32> = index.owners.keys().copied().collect();
        for owner in owners {
            index.check_owner(owner)?;
        }
        for (id, info) in &index.solids {
            if !index.owner_hidden(info.owner) {
                index.check_depth(*id)?;
            }
        }
        for header in index.entities.values() {
            ensure!(header.depth + 1 + usize::from(header.hidden) < MAX_DEPTH, "Map nesting limit exceeded");
        }
        Ok(index)
    }
}

impl ValidatedSnapshot {
    pub fn new(snapshot: Snapshot) -> Result<Self> {
        let index = Index::build(&snapshot)?;
        Ok(Self { snapshot, index })
    }

    pub fn from_vmf(text: &str) -> Result<Self> {
        Self::new(Snapshot::split(text)?)
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn into_snapshot(self) -> Snapshot {
        self.snapshot
    }

    pub fn sizes(&self) -> Sizes {
        self.index.sizes()
    }

    pub fn highest_id(&self) -> u32 {
        let index = &self.index;
        let objects =
            index.world_id().into_iter().chain(index.entities.keys().copied()).chain(index.solids.keys().copied());
        objects.chain(index.sides.keys().copied()).max().unwrap_or(0)
    }

    pub fn side_owner(&self, side: u32) -> Option<u32> {
        self.index.sides.get(&side).copied()
    }

    pub fn apply(&mut self, changes: &[Change]) -> Result<()> {
        self.apply_checked(changes, None)
    }

    pub fn apply_from(&mut self, changes: &[Change], range: Range<u32>) -> Result<()> {
        self.apply_checked(changes, Some(range))
    }

    pub fn check(&self, changes: &[Change]) -> Result<()> {
        self.precheck(changes)?;
        check_units(changes)
    }

    fn precheck(&self, changes: &[Change]) -> Result<()> {
        ensure!(!changes.is_empty() && changes.len() <= 8192, "Invalid change count");
        ensure!(changes_len(changes)? <= MAX_CHANGE_BYTES, "Change exceeds the session size limit");
        let mut seen = HashSet::with_capacity(changes.len());
        for change in changes {
            ensure!(change.key.len() <= 64 && seen.insert(change.key.as_str()), "Duplicate or invalid change key");
            ensure!(
                self.snapshot.0.get(&change.key).map(digest) == change.before,
                "An object changed on another participant: {}",
                change.key
            );
        }
        Ok(())
    }

    fn apply_checked(&mut self, changes: &[Change], range: Option<Range<u32>>) -> Result<()> {
        self.precheck(changes)?;
        if let Some(range) = &range {
            for change in changes {
                if change.before.is_none()
                    && let Some(id) = change.key.split(':').nth(1)
                {
                    let id: u32 = id.parse()?;
                    if !range.contains(&id) {
                        bail!(OutsideRange { side: false, id });
                    }
                }
            }
        }
        let items: Vec<_> =
            changes.iter().filter_map(|change| change.after.as_ref().map(|unit| (change.key.as_str(), unit))).collect();
        let parsed = parse_all(items)?;
        if let Some(range) = &range {
            for parsed in &parsed {
                if let Parsed::Solid { info, .. } = parsed {
                    for side in &info.sides {
                        if !self.index.sides.contains_key(side) && !range.contains(side) {
                            bail!(OutsideRange { side: true, id: *side });
                        }
                    }
                }
            }
        }
        let mut order: Vec<usize> = (0..changes.len()).collect();
        let rank = |key: &str| usize::from(!key.starts_with("solid:"));
        order.sort_by_key(|index| rank(&changes[*index].key));
        let mut removed = Vec::new();
        for index in &order {
            let key = &changes[*index].key;
            if let Some(unit) = self.snapshot.0.remove(key) {
                let parsed = self.index.sub(key, &unit).ok_or_else(|| anyhow::anyhow!("Map index out of sync"))?;
                removed.push((key.clone(), unit, parsed));
            }
        }
        let mut afters: Vec<(usize, Parsed)> = Vec::with_capacity(parsed.len());
        let mut parsed = parsed.into_iter();
        for (index, change) in changes.iter().enumerate() {
            if change.after.is_some() {
                afters.push((index, parsed.next().unwrap()));
            }
        }
        afters.sort_by_key(|(_, parsed)| usize::from(matches!(parsed, Parsed::Solid { .. })));
        let mut inserted = Vec::new();
        let mut owners = Vec::new();
        let mut result = Ok(());
        for (index, parsed) in afters {
            let change = &changes[index];
            let unit = change.after.as_ref().unwrap();
            match &parsed {
                Parsed::Solid { info, .. } => owners.push(info.owner),
                Parsed::Entity { id, .. } | Parsed::World { id, .. } => owners.push(*id),
                Parsed::Metadata { .. } => {}
            }
            if let Err(error) = self.index.add(&change.key, unit, parsed, true) {
                result = Err(error);
                break;
            }
            self.snapshot.0.insert(change.key.clone(), unit.clone());
            inserted.push(change.key.clone());
        }
        for (_, _, parsed) in &removed {
            match parsed {
                Parsed::Solid { info, .. } => owners.push(info.owner),
                Parsed::Entity { id, .. } | Parsed::World { id, .. } => owners.push(*id),
                Parsed::Metadata { .. } => {}
            }
        }
        if result.is_ok() {
            result = self.post_check(&inserted, &owners);
        }
        if result.is_err() {
            inserted.sort_by_key(|key| rank(key));
            for key in &inserted {
                if let Some(unit) = self.snapshot.0.remove(key) {
                    self.index.sub(key, &unit);
                }
            }
            for (key, unit, parsed) in removed.into_iter().rev() {
                let _ = self.index.add(&key, &unit, parsed, false);
                self.snapshot.0.insert(key, unit);
            }
        }
        result
    }

    fn post_check(&self, inserted: &[String], owners: &[u32]) -> Result<()> {
        self.index.check_totals()?;
        let mut checked = HashSet::new();
        for owner in owners {
            if checked.insert(*owner) {
                self.index.check_owner(*owner)?;
            }
        }
        for key in inserted {
            if let Some(id) = kind_id(key, "solid:") {
                self.index.check_depth(id)?;
            } else if let Some(id) = kind_id(key, "entity:")
                && let Some(header) = self.index.entities.get(&id)
            {
                ensure!(header.depth + 1 + usize::from(header.hidden) < MAX_DEPTH, "Map nesting limit exceeded");
            }
        }
        Ok(())
    }
}

pub fn check_units(changes: &[Change]) -> Result<()> {
    ensure!(!changes.is_empty() && changes.len() <= 8192, "Invalid change count");
    ensure!(changes_len(changes)? <= MAX_CHANGE_BYTES, "Change exceeds the session size limit");
    let mut seen = HashSet::with_capacity(changes.len());
    for change in changes {
        ensure!(change.key.len() <= 64 && seen.insert(change.key.as_str()), "Duplicate or invalid change key");
    }
    let items: Vec<_> =
        changes.iter().filter_map(|change| change.after.as_ref().map(|unit| (change.key.as_str(), unit))).collect();
    parse_all(items).map(|_| ())
}

pub(super) fn check_snapshot(snapshot: &Snapshot) -> Result<()> {
    Index::build(snapshot).map(|_| ())
}

pub(super) fn apply_to(snapshot: &mut Snapshot, changes: &[Change]) -> Result<()> {
    let index = Index::build(snapshot)?;
    let mut validated = ValidatedSnapshot { snapshot: std::mem::take(snapshot), index };
    let result = validated.apply(changes);
    *snapshot = validated.snapshot;
    result
}

#[cfg(test)]
mod tests;
