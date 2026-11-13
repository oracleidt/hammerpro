use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: u32 = 1;
pub const DATA_DIR: &str = "hammerpro_livelink";
pub const SESSION_FILE: &str = "session.json";
pub const COMMAND_FILE: &str = "cmd.json";
pub const GAME_FILE: &str = "game.json";
pub const ENTITIES_FILE: &str = "ents.json";
pub const INJECT_FILE: &str = "addon.txt";
pub const MAX_FILE: u64 = 8 << 20;
pub const MAX_ENTITIES: usize = 16_384;
pub const MAX_COORD: f64 = 65_536.0;
pub const SESSION_STALE_SECS: u64 = 10;

pub const BLOCKED_CLASSES: &[&str] =
    &["point_servercommand", "point_clientcommand", "point_broadcastclientcommand", "lua_run"];
pub const BLOCKED_INPUTS: &[&str] = &["command", "runcode", "runpassedcode"];

pub fn class_allowed(class: &str) -> bool {
    !BLOCKED_CLASSES.iter().any(|c| c.eq_ignore_ascii_case(class.trim()))
}

pub fn input_allowed(input: &str) -> bool {
    let input = input.trim();
    !input.is_empty()
        && input.len() <= 128
        && input.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !BLOCKED_INPUTS.iter().any(|c| c.eq_ignore_ascii_case(input))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    #[serde(rename = "i")]
    Map(u32),
    #[serde(rename = "own")]
    Own(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LightPreview {
    pub kind: String,
    pub origin: [f64; 3],
    pub angles: [f64; 3],
    pub color: [f64; 3],
    pub brightness: f64,
    pub inner: f64,
    pub outer: f64,
    pub distance: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StaticGhost {
    pub model: String,
    pub origin: [f64; 3],
    pub angles: [f64; 3],
    pub skin: u32,
    pub color: [u8; 4],
    pub scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HighlightItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghost: Option<u32>,
    pub mins: [f64; 3],
    pub maxs: [f64; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Set {
        key: u32,
        target: Target,
        kv: BTreeMap<String, String>,
    },
    Io {
        key: u32,
        target: Target,
        add: Vec<(String, String)>,
    },
    Create {
        key: u32,
        class: String,
        kv: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        io: Vec<(String, String)>,
    },
    Remove {
        key: u32,
        target: Target,
    },
    Ghost {
        key: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ghost: Option<StaticGhost>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        removed_at: Option<[f64; 3]>,
    },
    Light {
        key: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        light: Option<LightPreview>,
    },
    Fire {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<Target>,
        input: String,
        param: String,
        delay: f64,
    },
    Teleport {
        origin: [f64; 3],
        angles: [f64; 3],
        noclip: bool,
    },
    Back,
    Highlight {
        items: Vec<HighlightItem>,
    },
    Reset,
    Entities,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Entity(u32),
    Ghost(u32),
    Light(u32),
    Camera,
    Highlight,
}

impl Op {
    pub fn slot(&self) -> Option<Slot> {
        match self {
            Op::Set { key, .. } => Some(Slot::Entity(*key)),
            Op::Ghost { key, .. } => Some(Slot::Ghost(*key)),
            Op::Light { key, .. } => Some(Slot::Light(*key)),
            Op::Teleport { .. } => Some(Slot::Camera),
            Op::Highlight { .. } => Some(Slot::Highlight),
            _ => None,
        }
    }

    pub fn needs_map(&self) -> bool {
        let map = |t: &Target| matches!(t, Target::Map(_));
        match self {
            Op::Set { target, .. } | Op::Io { target, .. } | Op::Remove { target, .. } => map(target),
            Op::Fire { target: Some(t), .. } => map(t),
            Op::Highlight { items } => items.iter().any(|i| i.target.as_ref().is_some_and(map)),
            _ => false,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Op::Set { .. } => "set",
            Op::Io { .. } => "io",
            Op::Create { .. } => "create",
            Op::Remove { .. } => "remove",
            Op::Ghost { .. } => "ghost",
            Op::Light { .. } => "light",
            Op::Fire { .. } => "fire",
            Op::Teleport { .. } => "teleport",
            Op::Back => "back",
            Op::Highlight { .. } => "highlight",
            Op::Reset => "reset",
            Op::Entities => "entities",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Queued {
    pub seq: u64,
    #[serde(flatten)]
    pub op: Op,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CommandFile<'a> {
    pub v: u32,
    pub token: &'a str,
    pub load: &'a str,
    pub ops: &'a [Queued],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SessionFile<'a> {
    pub v: u32,
    pub token: &'a str,
    pub beat: u64,
    pub follow: bool,
    pub labels: &'a BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpResult {
    pub seq: u64,
    pub ok: bool,
    pub msg: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameReport {
    pub ack: u64,
    pub load: String,
    pub map: String,
    pub beat: u64,
    pub addon: String,
    pub eye: Option<([f64; 3], [f64; 3])>,
    pub results: Vec<OpResult>,
    pub ents_rev: u64,
    pub created: u32,
    pub ghosts: u32,
    pub lights: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameEntity {
    pub index: u32,
    pub class: String,
    pub name: String,
    pub origin: [f64; 3],
    pub model: String,
    pub hammerid: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameEntities {
    pub load: String,
    pub rev: u64,
    pub ents: Vec<GameEntity>,
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|f| f.is_finite())
}

fn uint(v: Option<&Value>) -> u64 {
    v.and_then(num).filter(|f| *f >= 0.0 && *f <= 9.0e15).map(|f| f as u64).unwrap_or(0)
}

fn text(v: Option<&Value>, max: usize) -> String {
    let s = match v {
        Some(Value::String(s)) => s.as_str(),
        _ => "",
    };
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

pub fn vec3(v: Option<&Value>, limit: f64) -> Option<[f64; 3]> {
    let a = v?.as_array()?;
    if a.len() != 3 {
        return None;
    }
    let out = [num(&a[0])?, num(&a[1])?, num(&a[2])?];
    out.iter().all(|c| c.abs() <= limit).then_some(out)
}

fn token_ok(v: &Value, token: &str) -> bool {
    v.get("token").and_then(Value::as_str) == Some(token)
}

pub fn parse_report(bytes: &[u8], token: &str) -> Option<GameReport> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    if !token_ok(&v, token) {
        return None;
    }
    let eye = vec3(v.get("eye"), MAX_COORD).zip(vec3(v.get("ang"), 360_000.0));
    let results = v
        .get("results")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(64)
                .map(|r| OpResult {
                    seq: uint(r.get("seq")),
                    ok: r.get("ok").and_then(Value::as_bool).unwrap_or(false),
                    msg: text(r.get("msg"), 300),
                })
                .collect()
        })
        .unwrap_or_default();
    let stats = v.get("stats");
    let stat = |k: &str| u32::try_from(uint(stats.and_then(|s| s.get(k)))).unwrap_or(u32::MAX);
    Some(GameReport {
        ack: uint(v.get("ack")),
        load: text(v.get("load"), 96),
        map: text(v.get("map"), 96),
        beat: uint(v.get("beat")),
        addon: text(v.get("addon"), 16),
        eye,
        results,
        ents_rev: uint(v.get("ents_rev")),
        created: stat("created"),
        ghosts: stat("ghosts"),
        lights: stat("lights"),
    })
}

pub fn parse_entities(bytes: &[u8], token: &str) -> Option<GameEntities> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    if !token_ok(&v, token) {
        return None;
    }
    let mut ents = Vec::new();
    for e in v.get("ents").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default().iter().take(MAX_ENTITIES) {
        let index = uint(e.get("i"));
        let class = text(e.get("c"), 96).to_ascii_lowercase();
        let Some(origin) = vec3(e.get("o"), MAX_COORD * 2.0) else { continue };
        if index == 0 || index > u64::from(u32::MAX) || class.is_empty() {
            continue;
        }
        let hammerid = u32::try_from(uint(e.get("h"))).ok().filter(|h| *h > 0);
        ents.push(GameEntity {
            index: index as u32,
            class,
            name: text(e.get("n"), 128),
            origin,
            model: text(e.get("m"), 256).replace('\\', "/").to_ascii_lowercase(),
            hammerid,
        });
    }
    Some(GameEntities { load: text(v.get("load"), 96), rev: uint(v.get("rev")), ents })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ops_encode_as_flat_tagged_objects() {
        let mut kv = BTreeMap::new();
        kv.insert("origin".to_owned(), "1 2 3".to_owned());
        let q = Queued { seq: 7, op: Op::Set { key: 12, target: Target::Map(40), kv } };
        let v = serde_json::to_value(&q).unwrap();
        assert_eq!(v, json!({ "seq": 7, "op": "set", "key": 12, "target": { "i": 40 }, "kv": { "origin": "1 2 3" } }));
        let own = serde_json::to_value(Op::Remove { key: 3, target: Target::Own(3) }).unwrap();
        assert_eq!(own, json!({ "op": "remove", "key": 3, "target": { "own": 3 } }));
        assert_eq!(serde_json::to_value(Op::Back).unwrap(), json!({ "op": "back" }));
        let io = serde_json::to_value(Op::Io {
            key: 1,
            target: Target::Own(1),
            add: vec![("OnTrigger".into(), "a,B,,0,-1".into())],
        })
        .unwrap();
        assert_eq!(io["add"], json!([["OnTrigger", "a,B,,0,-1"]]));
        let light = Op::Light { key: 4, light: None };
        assert_eq!(serde_json::to_value(&light).unwrap(), json!({ "op": "light", "key": 4 }));
        let back: Op = serde_json::from_value(serde_json::to_value(&q.op).unwrap()).unwrap();
        assert_eq!(back, q.op);
    }

    #[test]
    fn command_files_carry_token_and_load() {
        let ops = vec![Queued { seq: 1, op: Op::Entities }];
        let file = CommandFile { v: VERSION, token: "abc", load: "gm_x:1", ops: &ops };
        let v = serde_json::to_value(file).unwrap();
        assert_eq!(v["token"], "abc");
        assert_eq!(v["load"], "gm_x:1");
        assert_eq!(v["ops"][0], json!({ "seq": 1, "op": "entities" }));
    }

    #[test]
    fn slots_and_map_needs() {
        let set = Op::Set { key: 5, target: Target::Map(1), kv: BTreeMap::new() };
        assert_eq!(set.slot(), Some(Slot::Entity(5)));
        assert!(set.needs_map());
        assert!(!Op::Set { key: 5, target: Target::Own(5), kv: BTreeMap::new() }.needs_map());
        assert_eq!(Op::Back.slot(), None);
        assert_eq!(Op::Teleport { origin: [0.0; 3], angles: [0.0; 3], noclip: true }.slot(), Some(Slot::Camera));
        let fire = Op::Fire {
            name: Some("door".into()),
            target: None,
            input: "Open".into(),
            param: String::new(),
            delay: 0.0,
        };
        assert!(!fire.needs_map());
        assert_eq!(fire.kind(), "fire");
    }

    #[test]
    fn reports_are_validated() {
        let raw = json!({
            "token": "t", "ack": 12, "load": "gm_x:99", "map": "gm_x", "beat": 1700000000, "addon": "mounted",
            "eye": [1, 2, 3], "ang": [10, 20, 0],
            "results": [{ "seq": 11, "ok": false, "msg": "no entity\u{7}" }],
            "ents_rev": 2, "stats": { "created": 1, "ghosts": 2, "lights": 3 }
        });
        let bytes = serde_json::to_vec(&raw).unwrap();
        let r = parse_report(&bytes, "t").unwrap();
        assert_eq!(r.ack, 12);
        assert_eq!(r.load, "gm_x:99");
        assert_eq!(r.eye, Some(([1.0, 2.0, 3.0], [10.0, 20.0, 0.0])));
        assert_eq!(r.results, vec![OpResult { seq: 11, ok: false, msg: "no entity".into() }]);
        assert_eq!((r.created, r.ghosts, r.lights), (1, 2, 3));
        assert!(parse_report(&bytes, "other").is_none());
        assert!(parse_report(b"not json", "t").is_none());
        let far = json!({ "token": "t", "eye": [1e9, 0, 0], "ang": [0, 0, 0], "ack": -4 });
        let r = parse_report(&serde_json::to_vec(&far).unwrap(), "t").unwrap();
        assert_eq!(r.eye, None);
        assert_eq!(r.ack, 0);
    }

    #[test]
    fn entity_lists_skip_bad_rows() {
        let raw = json!({ "token": "t", "load": "l", "rev": 3, "ents": [
            { "i": 5, "c": "Prop_Physics", "n": "crate", "o": [1, 2, 3], "m": "models\\Box.mdl", "h": 42 },
            { "i": 0, "c": "x", "o": [0, 0, 0] },
            { "i": 6, "c": "light", "o": [0, 0] },
            { "i": 7, "c": "func_door", "o": ["4", 5, 6], "m": "*1", "h": 0 }
        ]});
        let e = parse_entities(&serde_json::to_vec(&raw).unwrap(), "t").unwrap();
        assert_eq!(e.rev, 3);
        assert_eq!(e.ents.len(), 2);
        assert_eq!(e.ents[0].class, "prop_physics");
        assert_eq!(e.ents[0].model, "models/box.mdl");
        assert_eq!(e.ents[0].hammerid, Some(42));
        assert_eq!(e.ents[1].origin, [4.0, 5.0, 6.0]);
        assert_eq!(e.ents[1].hammerid, None);
        assert!(parse_entities(&serde_json::to_vec(&raw).unwrap(), "x").is_none());
    }

    #[test]
    fn dangerous_classes_and_inputs_are_refused() {
        assert!(!class_allowed("point_servercommand"));
        assert!(!class_allowed(" LUA_RUN "));
        assert!(class_allowed("prop_physics"));
        assert!(input_allowed("Open"));
        assert!(!input_allowed("Command"));
        assert!(!input_allowed("RunPassedCode"));
        assert!(!input_allowed("Open; quit"));
        assert!(!input_allowed(""));
    }
}
