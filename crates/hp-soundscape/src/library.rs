use std::collections::HashMap;

use hp_kv::Block;

use crate::script::{Interval, Position, Rule, Soundscape, parse_file};

pub const MAX_DEPTH: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Game,
    Map,
}

impl Origin {
    pub fn id(self) -> &'static str {
        match self {
            Origin::Game => "game",
            Origin::Map => "map",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub block: Block,
    pub file: String,
    pub origin: Origin,
    pub shadowed: bool,
}

impl Entry {
    pub fn soundscape(&self) -> Soundscape {
        Soundscape::from_block(&self.block)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Library {
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
    files: Vec<(String, Origin, usize)>,
}

pub fn manifest_files(text: &str) -> Vec<String> {
    let Ok(root) = hp_kv::parse(text) else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for block in &root.children {
        for (k, v) in &block.props {
            if k.eq_ignore_ascii_case("file") || k.eq_ignore_ascii_case("precache_file") {
                let f = v.trim().replace('\\', "/").to_ascii_lowercase();
                if !f.is_empty() && !out.contains(&f) {
                    out.push(f);
                }
            }
        }
    }
    out
}

pub fn map_file_name(map: &str) -> String {
    format!("scripts/soundscapes_{}.txt", map.trim().to_ascii_lowercase())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatKind {
    Looping,
    Random,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlatSound {
    pub kind: FlatKind,
    pub waves: Vec<String>,
    pub volume: Interval,
    pub masters: Vec<(u32, Interval)>,
    pub pitch: Interval,
    pub time: Interval,
    pub soundlevel: f32,
    pub position: Position,
    pub chain: Vec<String>,
    pub rule: usize,
    pub node: u32,
}

impl FlatSound {
    pub fn master(&self) -> f32 {
        self.masters.iter().map(|(_, i)| i.mid()).product()
    }

    pub fn key(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |s: &str| {
            for b in s.bytes() {
                h ^= u64::from(b.to_ascii_lowercase());
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            h ^= 0xff;
            h = h.wrapping_mul(0x0100_0000_01b3);
        };
        for c in &self.chain {
            eat(c);
        }
        eat(&self.rule.to_string());
        eat(&self.node.to_string());
        h
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flat {
    pub sounds: Vec<FlatSound>,
    pub dsp: Option<i32>,
    pub dsp_player: Option<i32>,
    pub dsp_volume: Option<f32>,
    pub missing: Vec<String>,
    pub cycles: Vec<String>,
}

#[derive(Clone)]
struct Params {
    masters: Vec<(u32, Interval)>,
    starting: i32,
    position_override: i32,
    ambient_override: i32,
    depth: usize,
}

fn index_position(i: i32) -> Position {
    if (0..32).contains(&i) { Position::Index(i as u8) } else { Position::None }
}

fn looping_position(p: Position, params: &Params) -> Position {
    let index = match p {
        Position::None => -1,
        Position::Index(i) => params.starting + i32::from(i),
        Position::Random => params.starting,
    };
    if index < 0 {
        if params.ambient_override >= 0 { index_position(params.ambient_override) } else { Position::None }
    } else if params.position_override >= 0 {
        index_position(params.position_override)
    } else {
        index_position(index)
    }
}

fn random_position(p: Position, params: &Params) -> Position {
    match p {
        Position::Random => Position::Random,
        Position::None if params.ambient_override >= 0 => index_position(params.ambient_override),
        Position::None => Position::None,
        Position::Index(_) if params.position_override >= 0 => index_position(params.position_override),
        Position::Index(i) => index_position(params.starting + i32::from(i)),
    }
}

impl Library {
    pub fn add_file(&mut self, file: &str, text: &str, origin: Origin, replace: bool) -> usize {
        let blocks = parse_file(text);
        self.add_blocks(file, blocks, origin, replace)
    }

    pub fn add_blocks(&mut self, file: &str, blocks: Vec<Block>, origin: Origin, replace: bool) -> usize {
        let count = blocks.len();
        for block in blocks {
            if block.props.is_empty() && block.children.is_empty() {
                continue;
            }
            let key = block.name.trim().to_ascii_lowercase();
            let at = self.entries.len();
            match self.index.get(&key).copied() {
                Some(old) if replace => {
                    self.entries[old].shadowed = true;
                    self.index.insert(key, at);
                    self.entries.push(Entry { block, file: file.to_owned(), origin, shadowed: false });
                }
                Some(_) => self.entries.push(Entry { block, file: file.to_owned(), origin, shadowed: true }),
                None => {
                    self.index.insert(key, at);
                    self.entries.push(Entry { block, file: file.to_owned(), origin, shadowed: false });
                }
            }
        }
        self.files.push((file.to_owned(), origin, count));
        count
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.index.get(&name.trim().to_ascii_lowercase()).and_then(|i| self.entries.get(*i))
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| !e.shadowed)
    }

    pub fn all_entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn files(&self) -> &[(String, Origin, usize)] {
        &self.files
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn game_has(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.origin == Origin::Game && e.block.name.trim().eq_ignore_ascii_case(name.trim()))
    }

    pub fn is_game_file(&self, file: &str) -> bool {
        self.files.iter().any(|(f, o, _)| *o == Origin::Game && f.eq_ignore_ascii_case(file))
    }

    pub fn flatten(&self, name: &str) -> Flat {
        let mut out = Flat::default();
        let params = Params { masters: Vec::new(), starting: 0, position_override: -1, ambient_override: -1, depth: 0 };
        let mut chain = Vec::new();
        let mut node = 0;
        self.flatten_into(name, &params, &mut chain, &mut out, &mut node);
        out
    }

    fn flatten_into(&self, name: &str, params: &Params, chain: &mut Vec<String>, out: &mut Flat, node: &mut u32) {
        let Some(entry) = self.get(name) else {
            if !out.missing.iter().any(|m| m.eq_ignore_ascii_case(name.trim())) {
                out.missing.push(name.trim().to_owned());
            }
            return;
        };
        if chain.iter().any(|c| c.eq_ignore_ascii_case(name.trim()))
            && !out.cycles.iter().any(|m| m.eq_ignore_ascii_case(name.trim()))
        {
            out.cycles.push(name.trim().to_owned());
        }
        let s = entry.soundscape();
        let top = params.depth == 0;
        if top {
            out.dsp = s.dsp;
            out.dsp_player = s.dsp_player;
            out.dsp_volume = s.dsp_volume;
        }
        let here = *node;
        chain.push(s.name.clone());
        for (rule, r) in s.rules.iter().enumerate() {
            match r {
                Rule::Looping(l) => out.sounds.push(FlatSound {
                    kind: FlatKind::Looping,
                    waves: vec![l.wave.clone()],
                    volume: l.volume,
                    masters: params.masters.clone(),
                    pitch: l.pitch,
                    time: Interval::fixed(0.0),
                    soundlevel: l.soundlevel,
                    position: looping_position(l.position, params),
                    chain: chain.clone(),
                    rule,
                    node: here,
                }),
                Rule::Random(x) => out.sounds.push(FlatSound {
                    kind: FlatKind::Random,
                    waves: x.waves.clone(),
                    volume: x.volume,
                    masters: params.masters.clone(),
                    pitch: x.pitch,
                    time: x.time,
                    soundlevel: x.soundlevel,
                    position: random_position(x.position, params),
                    chain: chain.clone(),
                    rule,
                    node: here,
                }),
                Rule::Nested(n) => {
                    if params.depth + 1 > MAX_DEPTH {
                        if !out.cycles.iter().any(|m| m.eq_ignore_ascii_case(n.name.trim())) {
                            out.cycles.push(n.name.trim().to_owned());
                        }
                        continue;
                    }
                    *node += 1;
                    let mut masters = params.masters.clone();
                    masters.push((*node, n.volume));
                    let mut sub = Params {
                        masters,
                        starting: params.starting + n.position,
                        position_override: params.position_override,
                        ambient_override: params.ambient_override,
                        depth: params.depth + 1,
                    };
                    if let Some(k) = n.position_override
                        && params.position_override < 0
                    {
                        sub.position_override = params.starting + i32::from(k);
                        sub.ambient_override = params.starting + i32::from(k);
                    }
                    if let Some(k) = n.ambient_position_override
                        && params.ambient_override < 0
                    {
                        sub.ambient_override = params.starting + i32::from(k);
                    }
                    if !n.name.is_empty() {
                        self.flatten_into(&n.name, &sub, chain, out, node);
                    }
                }
                Rule::Other(_) => {}
            }
        }
        chain.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: &str = r#"
"Generic.Wind" { "dsp" "1" "playlooping" { "volume" "0.4" "wave" "ambient/wind/wind1.wav" } }
"Generic.Birds" { "playrandom" { "time" "4,8" "volume" "0.5" "rndwave" { "wave" "a.wav" "wave" "b.wav" } } }
"Loop.A" { "playsoundscape" { "name" "Loop.B" } }
"Loop.B" { "playsoundscape" { "name" "Loop.A" } }
"#;

    const MAP: &str = r#"
"gm_x.forest"
{
	"dsp" "3"
	"playsoundscape" { "name" "Generic.Wind" "volume" "0.5" }
	"playsoundscape" { "name" "Generic.Birds" "volume" "0.8" "positionoverride" "2" }
	"playsoundscape" { "name" "Nowhere" }
	"playlooping" { "volume" "1" "wave" "x/hum.wav" "position" "0" }
}
"Generic.Wind" { "playlooping" { "volume" "0.9" "wave" "mine.wav" } }
"#;

    fn library() -> Library {
        let mut lib = Library::default();
        lib.add_file("scripts/soundscapes.txt", GAME, Origin::Game, false);
        lib.add_file("scripts/soundscapes_gm_x.txt", MAP, Origin::Map, true);
        lib
    }

    #[test]
    fn manifests_list_their_files() {
        let files = manifest_files(
            r#"soundscapes_manifest { "file" "scripts/SoundScapes.txt" "precache_file" "scripts/b.txt" "file" "scripts/soundscapes.txt" }"#,
        );
        assert_eq!(files, ["scripts/soundscapes.txt", "scripts/b.txt"]);
        assert_eq!(map_file_name("GM_X"), "scripts/soundscapes_gm_x.txt");
    }

    #[test]
    fn the_map_file_overrides_game_names() {
        let lib = library();
        assert_eq!(lib.len(), 5);
        let wind = lib.get("generic.wind").unwrap();
        assert_eq!(wind.origin, Origin::Map);
        assert!(lib.game_has("Generic.Wind"));
        assert!(lib.is_game_file("scripts/soundscapes.txt"));
        assert!(!lib.is_game_file("scripts/soundscapes_gm_x.txt"));
        assert_eq!(lib.entries().count(), 5);
        assert_eq!(lib.all_entries().len(), 6);
    }

    #[test]
    fn nesting_multiplies_volumes_and_overrides_positions() {
        let lib = library();
        let flat = lib.flatten("gm_x.forest");
        assert_eq!(flat.dsp, Some(3), "only the top soundscape sets the DSP");
        assert_eq!(flat.missing, ["Nowhere"]);
        assert_eq!(flat.sounds.len(), 3);
        let wind = &flat.sounds[0];
        assert_eq!(wind.waves, ["mine.wav"]);
        assert!((wind.master() - 0.5).abs() < 1e-6);
        assert_eq!(wind.chain, ["gm_x.forest", "Generic.Wind"]);
        let birds = &flat.sounds[1];
        assert_eq!(birds.kind, FlatKind::Random);
        assert_eq!(
            birds.position,
            Position::Index(2),
            "positionoverride also moves the ambient sounds, like the engine"
        );
        assert_eq!(birds.time, Interval { min: 4.0, max: 8.0 });
        let hum = &flat.sounds[2];
        assert_eq!(hum.position, Position::Index(0));
        assert_ne!(wind.key(), birds.key());
    }

    #[test]
    fn nested_positions_follow_the_client_code() {
        let mut lib = Library::default();
        lib.add_file(
            "x.txt",
            r#"
"top" { "playsoundscape" { "name" "mid" "position" "2" "volume" "0.5,0.7" } "playlooping" { "volume" "1" "wave" "a.wav" "position" "1" } }
"mid" { "playlooping" { "volume" "1" "wave" "b.wav" "position" "1" } "playsoundscape" { "name" "low" "ambientpositionoverride" "0" } "playrandom" { "volume" "1" "pitch" "100" "rndwave" { "wave" "r.wav" } "position" "random" } }
"low" { "playlooping" { "volume" "1" "wave" "c.wav" } "playrandom" { "volume" "1" "pitch" "100" "rndwave" { "wave" "d.wav" } "position" "random" } }
"empty" { }
"#,
            Origin::Game,
            false,
        );
        assert!(lib.get("empty").is_none(), "a soundscape without keys is skipped like the engine");
        let f = lib.flatten("top");
        let find = |w: &str| f.sounds.iter().find(|s| s.waves[0] == w).unwrap();
        assert_eq!(
            find("b.wav").position,
            Position::Index(3),
            "position shifts the starting position of the nested soundscape"
        );
        assert_eq!(
            find("c.wav").position,
            Position::Index(2),
            "ambientpositionoverride counts from the parent's start"
        );
        assert_eq!(find("d.wav").position, Position::Random, "random beats the ambient override");
        assert_eq!(find("r.wav").position, Position::Random);
        assert_eq!(find("a.wav").position, Position::Index(1));
        assert_eq!(find("b.wav").masters.len(), 1);
        assert_eq!(find("c.wav").masters.len(), 2);
        assert!((find("b.wav").master() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn random_rules_keep_the_engine_zero_defaults() {
        let mut lib = Library::default();
        lib.add_file("x.txt", r#""r" { "playrandom" { "rndwave" { "wave" "a.wav" } } }"#, Origin::Game, false);
        let f = lib.flatten("r");
        let s = &f.sounds[0];
        assert_eq!(s.volume, Interval::fixed(0.0));
        assert_eq!(s.pitch, Interval::fixed(0.0));
        assert_eq!(s.time, Interval::fixed(0.0));
        assert_eq!(s.soundlevel, 0.0);
    }

    #[test]
    fn cycles_stop() {
        let lib = library();
        let flat = lib.flatten("Loop.A");
        assert!(flat.sounds.is_empty());
        assert!(flat.cycles.iter().any(|c| c == "Loop.A"));
        assert_eq!(lib.flatten("nothing").missing, ["nothing"]);
    }
}
