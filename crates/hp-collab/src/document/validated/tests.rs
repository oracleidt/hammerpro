use super::*;
use crate::document::Snapshot;

struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit.max(1)
    }
}

fn solid_text(id: u32, first_side: u32, offset: i64, disp: bool) -> String {
    let planes = [
        "(0 64 64) (64 64 64) (64 0 64)",
        "(0 0 0) (64 0 0) (64 64 0)",
        "(64 64 64) (0 64 64) (0 64 0)",
        "(64 0 0) (0 0 0) (0 0 64)",
        "(64 64 0) (64 0 0) (64 0 64)",
        "(0 64 64) (0 0 64) (0 0 0)",
    ];
    let mut text = format!("solid {{\"id\" \"{id}\"");
    for (index, plane) in planes.iter().enumerate() {
        let plane = plane.replace("64)", &format!("{})", 64 + offset)).replace("(0 ", &format!("({offset} "));
        text.push_str(&format!(
            "side {{\"id\" \"{}\" \"plane\" \"{plane}\" \"material\" \"dev/dev_measuregeneric01b\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\"",
            first_side + index as u32
        ));
        if disp && index == 0 {
            text.push_str("dispinfo {\"power\" \"2\" \"startposition\" \"[0 0 64]\" \"elevation\" \"0\" \"subdiv\" \"0\" distances {\"row0\" \"0 1 2 3 4\"}}");
        }
        text.push('}');
    }
    text.push_str("editor {\"color\" \"0 100 200\" \"visgroupshown\" \"1\"}}");
    text
}

fn solid_unit(id: u32, first_side: u32, offset: i64, owner: u32, hidden: bool) -> Unit {
    let solid = hp_vmf::parse_fragment(&solid_text(id, first_side, offset, offset % 3 == 0)).unwrap().0.remove(0);
    Unit::from_solid(&solid, owner).map(|unit| Unit { hidden, ..unit }).unwrap()
}

fn base_map() -> Snapshot {
    let mut text = String::from(
        "versioninfo {\"mapversion\" \"7\"} visgroups {visgroup {\"name\" \"A\" \"visgroupid\" \"10\" \"color\" \"1 2 3\"}} world {\"id\" \"1\" \"classname\" \"worldspawn\" \"skyname\" \"sky_day01_01\"",
    );
    for index in 0..6u32 {
        text.push_str(&solid_text(100 + index, 1000 + index * 10, index as i64 * 8, index == 2));
    }
    text.push_str(" group {\"id\" \"50\" editor {\"color\" \"1 1 1\"}}}");
    text.push_str(" entity {\"id\" \"2\" \"classname\" \"light\" \"origin\" \"0 0 64\"}");
    text.push_str(" hidden {entity {\"id\" \"3\" \"classname\" \"func_detail\"");
    text.push_str(&solid_text(200, 2000, 32, false));
    text.push_str(" hidden {");
    text.push_str(&solid_text(201, 2010, 48, false));
    text.push_str("}}}");
    text.push_str(" entity {\"id\" \"4\" \"classname\" \"func_brush\" connections {\"OnTrigger\" \"x,Toggle,,0,-1\"}");
    text.push_str(&solid_text(202, 2020, 16, true));
    text.push('}');
    Snapshot::from_vmf(&text).unwrap()
}

fn reference(snapshot: &Snapshot) -> Result<Sizes> {
    let (unit_bytes, encoded_bytes, vmf_bytes) = snapshot.reference_sizes()?;
    Ok(Sizes { units: snapshot.0.len(), unit_bytes, encoded_bytes, vmf_bytes })
}

fn reference_apply(snapshot: &Snapshot, changes: &[Change]) -> Result<Snapshot> {
    ensure!(!changes.is_empty() && changes.len() <= 8192, "Invalid change count");
    let mut seen = HashSet::new();
    let mut updated = snapshot.clone();
    for change in changes {
        ensure!(seen.insert(change.key.clone()), "Duplicate or invalid change key");
        ensure!(snapshot.0.get(&change.key).map(digest) == change.before, "stale");
        match &change.after {
            Some(unit) => updated.0.insert(change.key.clone(), unit.clone()),
            None => updated.0.remove(&change.key),
        };
    }
    reference(&updated)?;
    Ok(updated)
}

fn change(map: &Snapshot, key: &str, after: Option<Unit>) -> Change {
    Change { key: key.into(), before: map.0.get(key).map(digest), after }
}

fn entity_unit(id: u32, class: &str, hidden: bool) -> Unit {
    Unit { owner: 0, hidden, vmf: format!("entity\n{{\n\t\"id\" \"{id}\"\n\t\"classname\" \"{class}\"\n}}\n") }
}

fn random_edit(map: &Snapshot, random: &mut Random, next_id: &mut u32) -> Vec<Change> {
    let solids: Vec<&String> = map.0.keys().filter(|key| key.starts_with("solid:")).collect();
    let entities: Vec<&String> = map.0.keys().filter(|key| key.starts_with("entity:")).collect();
    let pick = |random: &mut Random, list: &[&String]| list[random.below(list.len() as u64) as usize].clone();
    let owners: Vec<u32> =
        std::iter::once(1).chain(entities.iter().filter_map(|key| key.strip_prefix("entity:")?.parse().ok())).collect();
    match random.below(16) {
        0 | 1 if !solids.is_empty() => {
            let key = pick(random, &solids);
            let unit = &map.0[&key];
            let id: u32 = key[6..].parse().unwrap();
            let first = unit.to_solid().unwrap().sides[0].id;
            let offset = random.below(64) as i64 - 32;
            vec![change(map, &key, Some(solid_unit(id, first, offset, unit.owner, unit.hidden)))]
        }
        2 if !solids.is_empty() => {
            let key = pick(random, &solids);
            let mut unit = map.0[&key].clone();
            unit.hidden = !unit.hidden;
            vec![change(map, &key, Some(unit))]
        }
        3 if !entities.is_empty() => {
            let key = pick(random, &entities);
            let mut unit = map.0[&key].clone();
            unit.hidden = !unit.hidden;
            vec![change(map, &key, Some(unit))]
        }
        4 if !solids.is_empty() => {
            let key = pick(random, &solids);
            let mut unit = map.0[&key].clone();
            unit.owner = owners[random.below(owners.len() as u64) as usize];
            vec![change(map, &key, Some(unit))]
        }
        5 | 6 => {
            let id = *next_id;
            *next_id += 20;
            let owner = owners[random.below(owners.len() as u64) as usize];
            vec![change(
                map,
                &format!("solid:{id}"),
                Some(solid_unit(id, id + 1, random.below(40) as i64, owner, false)),
            )]
        }
        7 => {
            let entity = *next_id;
            let solid = entity + 1;
            *next_id += 20;
            vec![
                change(
                    map,
                    &format!("entity:{entity}"),
                    Some(entity_unit(entity, "func_detail", random.below(2) == 0)),
                ),
                change(map, &format!("solid:{solid}"), Some(solid_unit(solid, solid + 1, 4, entity, false))),
            ]
        }
        8 if !entities.is_empty() => {
            let key = pick(random, &entities);
            let id: u32 = key[7..].parse().unwrap();
            let mut changes = vec![change(map, &key, None)];
            for (other, unit) in &map.0 {
                if unit.owner == id && other.starts_with("solid:") {
                    changes.push(change(map, other, None));
                }
            }
            changes
        }
        9 if !solids.is_empty() => {
            let key = pick(random, &solids);
            vec![change(map, &key, None)]
        }
        10 => {
            let mut root = hp_kv::parse_bounded(&map.0["world"].vmf, 128).unwrap();
            let mut group = Block::new("group");
            group.props.push(("id".into(), next_id.to_string()));
            *next_id += 20;
            root.children[0].children.push(group);
            let world = root.children[0].to_kv_string();
            vec![change(map, "world", Some(Unit { owner: 0, hidden: false, vmf: world }))]
        }
        11 => {
            let metadata = format!(
                "visgroups\n{{\n\tvisgroup\n\t{{\n\t\t\"name\" \"V{}\"\n\t\t\"visgroupid\" \"{}\"\n\t}}\n}}\n",
                next_id, next_id
            );
            *next_id += 20;
            vec![change(map, "metadata", Some(Unit { owner: 0, hidden: false, vmf: metadata }))]
        }
        12 if solids.len() >= 2 => {
            let first = pick(random, &solids);
            let second = pick(random, &solids);
            let sides = map.0[&second].to_solid().unwrap().sides[0].id;
            let unit = &map.0[&first];
            let id: u32 = first[6..].parse().unwrap();
            vec![change(map, &first, Some(solid_unit(id, sides, 2, unit.owner, false)))]
        }
        13 if !solids.is_empty() => {
            let key = pick(random, &solids);
            let mut unit = map.0[&key].clone();
            unit.owner = 999_999;
            vec![change(map, &key, Some(unit))]
        }
        14 if !solids.is_empty() => {
            let key = pick(random, &solids);
            let id: u32 = key[6..].parse().unwrap();
            vec![change(map, &format!("entity:{id}"), Some(entity_unit(id, "info_target", false)))]
        }
        _ => {
            let id = *next_id;
            *next_id += 20;
            let class = if random.below(2) == 0 { "lua_run" } else { "info_target" };
            vec![change(map, &format!("entity:{id}"), Some(entity_unit(id, class, random.below(2) == 0)))]
        }
    }
}

#[test]
fn incremental_validation_matches_full_validation_and_exact_sizes() {
    let start = base_map();
    let mut fast = ValidatedSnapshot::new(start.clone()).unwrap();
    assert_eq!(fast.sizes(), reference(&start).unwrap());
    let mut full = start;
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    let mut next_id = 5000;
    let (mut accepted, mut rejected) = (0, 0);
    for step in 0..600 {
        let changes = random_edit(&full, &mut random, &mut next_id);
        let expected = reference_apply(&full, &changes);
        let before = fast.sizes();
        let result = fast.apply(&changes);
        assert_eq!(result.is_ok(), expected.is_ok(), "step {step}: {result:?} versus {:?}", expected.as_ref().err());
        match expected {
            Ok(updated) => {
                full = updated;
                accepted += 1;
            }
            Err(_) => {
                rejected += 1;
                assert_eq!(fast.sizes(), before, "step {step}: rollback");
            }
        }
        assert_eq!(fast.snapshot(), &full, "step {step}");
        assert_eq!(fast.sizes(), reference(&full).unwrap(), "step {step}");
    }
    assert!(accepted > 150 && rejected > 50, "{accepted} accepted, {rejected} rejected");
    let rebuilt = ValidatedSnapshot::new(full.clone()).unwrap();
    assert_eq!(rebuilt.sizes(), fast.sizes());
}

#[test]
fn participant_ranges_cover_new_objects_and_new_sides_only() {
    let map = base_map();
    let mut validated = ValidatedSnapshot::new(map.clone()).unwrap();
    let moved = change(&map, "solid:100", Some(solid_unit(100, 1000, 24, 1, false)));
    validated.apply_from(&[moved], 1_000_000..2_000_000).unwrap();
    let foreign_sides = change(&map, "solid:101", Some(solid_unit(101, 9000, 24, 1, false)));
    let error = validated.apply_from(&[foreign_sides], 1_000_000..2_000_000).unwrap_err();
    assert_eq!(error.to_string(), "Side identifier is outside the participant range");
    let foreign = change(&map, "solid:7000", Some(solid_unit(7000, 1_000_001, 0, 1, false)));
    let error = validated.apply_from(&[foreign], 1_000_000..2_000_000).unwrap_err();
    assert_eq!(error.to_string(), "Object identifier is outside the participant range");
    let owned = change(&map, "solid:1000500", Some(solid_unit(1_000_500, 1_000_501, 0, 1, false)));
    validated.apply_from(&[owned], 1_000_000..2_000_000).unwrap();
    assert_eq!(validated.side_owner(1_000_501), Some(1_000_500));
}

#[test]
fn block_names_and_quotes_that_would_change_the_written_map_are_rejected() {
    let map = base_map();
    for text in [
        "entity {\"id\" \"2\" \"classname\" \"light\" \"x}entity{\" {}}",
        "entity {\"id\" \"2\" \"classname\" \"light\" \"a b\" {}}",
        "entity {\"id\" \"2\" \"classname\" \"light\" \"a//b\" {}}",
        "entity {\"id\" \"2\" \"classname\" \"light\" \"key\" [a\"b]}",
    ] {
        let mut validated = ValidatedSnapshot::new(map.clone()).unwrap();
        let unit = Unit { owner: 0, hidden: false, vmf: text.into() };
        assert!(validated.apply(&[change(&map, "entity:2", Some(unit))]).is_err(), "{text}");
        assert_eq!(validated.snapshot(), &map);
    }
}

#[test]
fn hidden_owners_keep_the_full_map_nesting_limit() {
    let base = Snapshot::from_vmf("world {\"id\" \"1\"} entity {\"id\" \"2\"}").unwrap();
    let unit = Unit {
        owner: 0,
        hidden: true,
        vmf: format!("entity {{\"id\" \"2\" {} {} }}", "x {".repeat(126), "}".repeat(126)),
    };
    let entity = change(&base, "entity:2", Some(unit));
    let mut full = base.clone();
    assert!(full.apply(std::slice::from_ref(&entity)).is_err());
    let mut incremental = ValidatedSnapshot::new(base.clone()).unwrap();
    assert!(incremental.apply(&[entity]).is_err());
    assert_eq!(incremental.snapshot(), &base);
    let deep = Unit {
        owner: 2,
        hidden: false,
        vmf: format!("solid {{\"id\" \"5\" {} {} }}", "x {".repeat(125), "}".repeat(125)),
    };
    let mut validated = ValidatedSnapshot::new(base.clone()).unwrap();
    validated.apply(&[change(&base, "solid:5", Some(deep))]).unwrap();
    let hide = Unit { hidden: true, ..validated.0["entity:2"].clone() };
    let hide = change(validated.snapshot(), "entity:2", Some(hide));
    assert_eq!(validated.apply(&[hide]).unwrap_err().to_string(), "Map nesting limit exceeded");
    assert!(reference(validated.snapshot()).is_ok());
}

#[test]
fn unit_constructors_match_the_full_map_split() {
    let parsed = hp_vmf::parse(&base_map().to_vmf().unwrap()).unwrap();
    let map = Snapshot::from_vmf(&hp_vmf::write(&parsed)).unwrap();
    for solid in &parsed.world.solids {
        assert_eq!(Unit::from_solid(solid, 1).unwrap(), map.0[&format!("solid:{}", solid.id)]);
    }
    for entity in &parsed.entities {
        assert_eq!(
            Unit::from_entity(entity, !entity.solids.is_empty()).unwrap(),
            map.0[&format!("entity:{}", entity.id)]
        );
        for solid in &entity.solids {
            assert_eq!(Unit::from_solid(solid, entity.id).unwrap(), map.0[&format!("solid:{}", solid.id)]);
        }
    }
    let mut header = parsed.clone();
    header.world.solids.clear();
    header.entities.clear();
    let (world, metadata) = Unit::header_units(&header).unwrap();
    assert_eq!(world, map.0["world"]);
    assert_eq!(metadata, map.0["metadata"]);
    assert_eq!(map.0["world"].to_world().unwrap().groups.len(), 1);
    assert_eq!(map.0["metadata"].to_metadata().unwrap().0[0].name, "A");
    assert!(map.0["entity:3"].to_entity().unwrap().hidden);
    assert!(map.0["solid:201"].to_solid().unwrap().hidden);
}
