use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use hp_vmf::{Connection, Entity, Solid};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntSnap {
    pub class: String,
    pub kv: BTreeMap<String, String>,
    pub io: Vec<(String, String)>,
    pub brush: bool,
}

impl EntSnap {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.kv.get(key).map(String::as_str)
    }

    pub fn name(&self) -> &str {
        self.get("targetname").unwrap_or("")
    }

    pub fn origin(&self) -> Option<[f64; 3]> {
        parse3(self.get("origin")?)
    }

    pub fn angles(&self) -> [f64; 3] {
        self.get("angles").and_then(parse3).unwrap_or([0.0; 3])
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub ents: BTreeMap<u32, EntSnap>,
    pub brush_sig: u64,
}

const SKIPPED_KEYS: &[&str] = &["id", "classname", "hammerid"];

pub fn parse3(text: &str) -> Option<[f64; 3]> {
    let mut it = text.split_whitespace().map(|p| p.parse::<f64>().ok().filter(|f| f.is_finite()));
    let out = [it.next()??, it.next()??, it.next()??];
    Some(out)
}

pub fn connection_value(c: &Connection) -> String {
    let sep = if c.param.contains(',') { '\u{1b}' } else { ',' };
    let delay = if c.delay.fract() == 0.0 { format!("{}", c.delay as i64) } else { format!("{}", c.delay) };
    [c.target.as_str(), c.input.as_str(), c.param.as_str(), delay.as_str(), &c.times.to_string()].join(&sep.to_string())
}

pub fn snap_entity(e: &Entity, brush: bool) -> EntSnap {
    let mut kv = BTreeMap::new();
    for (k, v) in &e.props {
        let key = k.trim().to_ascii_lowercase();
        if key.is_empty() || SKIPPED_KEYS.contains(&key.as_str()) {
            continue;
        }
        kv.insert(key, v.clone());
    }
    let io = e.connections.iter().map(|c| (c.output.clone(), connection_value(c))).collect();
    EntSnap { class: e.classname.trim().to_ascii_lowercase(), kv, io, brush: brush || !e.solids.is_empty() }
}

fn hash_f64(h: &mut impl Hasher, v: f64) {
    h.write_u64(if v == 0.0 { 0 } else { v.to_bits() });
}

fn hash_vec(h: &mut impl Hasher, v: &glam::DVec3) {
    hash_f64(h, v.x);
    hash_f64(h, v.y);
    hash_f64(h, v.z);
}

pub fn solid_hash(s: &Solid, owner: u32) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.id.hash(&mut h);
    owner.hash(&mut h);
    for side in &s.sides {
        for p in &side.plane {
            hash_vec(&mut h, p);
        }
        side.material.to_ascii_lowercase().hash(&mut h);
        if let Some(d) = &side.disp {
            d.power.hash(&mut h);
            hash_vec(&mut h, &d.start_position);
            hash_f64(&mut h, d.elevation);
            for n in d.normals.iter().chain(&d.offsets) {
                hash_vec(&mut h, n);
            }
            for f in &d.distances {
                hash_f64(&mut h, *f);
            }
        }
    }
    h.finish()
}

pub fn brush_signature<'a>(solids: impl Iterator<Item = (&'a Solid, u32)>) -> u64 {
    let mut count = 0u64;
    let mut sum = 0u64;
    let mut xor = 0u64;
    for (s, owner) in solids {
        let v = solid_hash(s, owner);
        count += 1;
        sum = sum.wrapping_add(v);
        xor ^= v.rotate_left(17);
    }
    sum ^ xor.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ count
}

impl Snapshot {
    pub fn from_entities<'a>(ents: impl Iterator<Item = (&'a Entity, bool)>, brush_sig: u64) -> Self {
        let ents = ents.map(|(e, brush)| (e.id, snap_entity(e, brush))).collect();
        Self { ents, brush_sig }
    }

    pub fn changed_entities(&self, other: &Snapshot) -> usize {
        let mut n = 0;
        for (id, a) in &self.ents {
            if other.ents.get(id) != Some(a) {
                n += 1;
            }
        }
        n + other.ents.keys().filter(|id| !self.ents.contains_key(id)).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(id: u32, class: &str, props: &[(&str, &str)]) -> Entity {
        Entity {
            id,
            classname: class.into(),
            props: props.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
            ..Entity::default()
        }
    }

    #[test]
    fn snapshots_normalize_keys_and_outputs() {
        let mut e = ent(3, "Prop_Physics", &[("Origin", "1 2 3"), ("id", "3"), ("model", "models/a.mdl")]);
        e.connections.push(Connection {
            output: "OnBreak".into(),
            target: "door".into(),
            input: "Open".into(),
            param: String::new(),
            delay: 0.5,
            times: -1,
            ..Connection::default()
        });
        e.connections.push(Connection {
            output: "OnHealthChanged".into(),
            target: "t".into(),
            input: "AddOutput".into(),
            param: "a,b".into(),
            delay: 2.0,
            times: 1,
            ..Connection::default()
        });
        let s = snap_entity(&e, false);
        assert_eq!(s.class, "prop_physics");
        assert_eq!(s.get("origin"), Some("1 2 3"));
        assert!(s.get("id").is_none());
        assert_eq!(s.origin(), Some([1.0, 2.0, 3.0]));
        assert_eq!(s.angles(), [0.0; 3]);
        assert_eq!(s.io[0], ("OnBreak".to_owned(), "door,Open,,0.5,-1".to_owned()));
        assert_eq!(s.io[1].1, "t\u{1b}AddOutput\u{1b}a,b\u{1b}2\u{1b}1");
        assert!(!s.brush);
    }

    #[test]
    fn brush_signature_follows_geometry_only() {
        let map = hp_vmf::parse(
            "world\n{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\nsolid\n{\n\"id\" \"2\"\nside\n{\n\"id\" \"1\"\n\"plane\" \"(0 0 0) (0 1 0) (1 1 0)\"\n\"material\" \"DEV/A\"\n\"uaxis\" \"[1 0 0 0] 0.25\"\n\"vaxis\" \"[0 -1 0 0] 0.25\"\n}\n}\n}\n",
        )
        .expect("vmf");
        let s = &map.world.solids[0];
        let a = brush_signature(std::iter::once((s, 0)));
        assert_eq!(a, brush_signature(std::iter::once((s, 0))));
        assert_ne!(a, brush_signature(std::iter::once((s, 9))));
        let mut moved = s.clone();
        moved.sides[0].plane[0].z += 8.0;
        assert_ne!(a, brush_signature(std::iter::once((&moved, 0))));
        let mut retextured = s.clone();
        retextured.sides[0].material = "dev/a".into();
        assert_eq!(a, brush_signature(std::iter::once((&retextured, 0))));
        retextured.sides[0].material = "dev/b".into();
        assert_ne!(a, brush_signature(std::iter::once((&retextured, 0))));
        assert_ne!(brush_signature(std::iter::empty()), a);
    }

    #[test]
    fn changed_entities_counts_both_sides() {
        let a = Snapshot::from_entities(
            [(&ent(1, "info_target", &[]), false), (&ent(2, "light", &[]), false)].into_iter(),
            0,
        );
        let mut b = a.clone();
        assert_eq!(a.changed_entities(&b), 0);
        b.ents.remove(&2);
        b.ents.insert(5, EntSnap::default());
        b.ents.get_mut(&1).unwrap().kv.insert("targetname".into(), "x".into());
        assert_eq!(a.changed_entities(&b), 3);
        assert_eq!(parse3("1 2"), None);
        assert_eq!(parse3("1 2 nan"), None);
    }
}
