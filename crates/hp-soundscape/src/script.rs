use hp_kv::Block;

use crate::soundlevel;

pub const MAX_POSITIONS: u8 = 8;
pub const RULE_LOOPING: &str = "playlooping";
pub const RULE_RANDOM: &str = "playrandom";
pub const RULE_NESTED: &str = "playsoundscape";
pub const RULES: [&str; 3] = [RULE_LOOPING, RULE_RANDOM, RULE_NESTED];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub min: f32,
    pub max: f32,
}

impl Interval {
    pub const fn fixed(v: f32) -> Self {
        Self { min: v, max: v }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split(',').map(str::trim);
        let a: f32 = parts.next()?.parse().ok()?;
        let b: f32 = match parts.next() {
            Some(p) if !p.is_empty() => p.parse().ok()?,
            _ => a,
        };
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        Some(Self { min: a.min(b), max: a.max(b) })
    }

    pub fn is_fixed(&self) -> bool {
        (self.max - self.min).abs() < 1e-6
    }

    pub fn sample(&self, t: f32) -> f32 {
        self.min + (self.max - self.min) * t.clamp(0.0, 1.0)
    }

    pub fn mid(&self) -> f32 {
        (self.min + self.max) * 0.5
    }

    pub fn to_text(&self) -> String {
        if self.is_fixed() { num(self.min) } else { format!("{},{}", num(self.min), num(self.max)) }
    }
}

pub fn num(v: f32) -> String {
    let r = (f64::from(v) * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r}") }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    None,
    Index(u8),
    Random,
}

impl Position {
    fn parse(text: Option<&str>) -> Self {
        match text.map(str::trim) {
            None | Some("") => Position::None,
            Some(t) if t.eq_ignore_ascii_case("random") => Position::Random,
            Some(t) => t
                .parse::<i32>()
                .ok()
                .filter(|i| (0..32).contains(i))
                .map_or(Position::None, |i| Position::Index(i as u8)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Looping {
    pub volume: Interval,
    pub pitch: Interval,
    pub wave: String,
    pub soundlevel: f32,
    pub position: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Random {
    pub volume: Interval,
    pub pitch: Interval,
    pub time: Interval,
    pub waves: Vec<String>,
    pub soundlevel: f32,
    pub position: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Nested {
    pub name: String,
    pub volume: Interval,
    pub position: i32,
    pub position_override: Option<u8>,
    pub ambient_position_override: Option<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Rule {
    Looping(Looping),
    Random(Random),
    Nested(Nested),
    Other(String),
}

impl Rule {
    pub fn kind(&self) -> &str {
        match self {
            Rule::Looping(_) => RULE_LOOPING,
            Rule::Random(_) => RULE_RANDOM,
            Rule::Nested(_) => RULE_NESTED,
            Rule::Other(name) => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Soundscape {
    pub name: String,
    pub dsp: Option<i32>,
    pub dsp_volume: Option<f32>,
    pub dsp_player: Option<i32>,
    pub soundmixer: Option<String>,
    pub rules: Vec<Rule>,
}

fn interval(block: &Block, key: &str, default: Interval) -> Interval {
    block.get(key).and_then(Interval::parse).unwrap_or(default)
}

fn level(block: &Block, default: f32) -> f32 {
    if let Some(v) = block.get("soundlevel") {
        return soundlevel::parse(v).unwrap_or(default);
    }
    if let Some(a) = block.get("attenuation").and_then(Interval::parse) {
        return soundlevel::from_attenuation(a.mid());
    }
    default
}

fn override_index(block: &Block, key: &str) -> Option<u8> {
    block.get(key).and_then(|v| v.trim().parse::<i32>().ok()).filter(|i| (0..32).contains(i)).map(|i| i as u8)
}

pub fn clean_wave(wave: &str) -> String {
    wave.trim()
        .trim_start_matches(['*', '#', '@', '>', '<', '^', ')', '(', '}', '$', '?', '&', '~', '+', '%', '`', '!'])
        .replace('\\', "/")
}

pub fn wave_path(wave: &str) -> Option<String> {
    let w = clean_wave(wave);
    let w = w.trim_start_matches('/');
    if w.is_empty() {
        return None;
    }
    Some(if w.to_ascii_lowercase().starts_with("sound/") { w.to_owned() } else { format!("sound/{w}") })
}

impl Rule {
    pub fn from_block(b: &Block) -> Rule {
        let kind = b.name.trim().to_ascii_lowercase();
        match kind.as_str() {
            RULE_LOOPING => Rule::Looping(Looping {
                volume: interval(b, "volume", Interval::fixed(0.0)),
                pitch: interval(b, "pitch", Interval::fixed(100.0)),
                wave: b.get("wave").unwrap_or("").trim().to_owned(),
                soundlevel: level(b, soundlevel::NORM),
                position: Position::parse(b.get("position")),
            }),
            RULE_RANDOM => {
                let mut waves: Vec<String> = Vec::new();
                for child in b.children_named("rndwave") {
                    waves.extend(child.get_all("wave").map(|w| w.trim().to_owned()).filter(|w| !w.is_empty()));
                }
                waves.extend(b.get_all("wave").map(|w| w.trim().to_owned()).filter(|w| !w.is_empty()));
                Rule::Random(Random {
                    volume: interval(b, "volume", Interval::fixed(0.0)),
                    pitch: interval(b, "pitch", Interval::fixed(0.0)),
                    time: interval(b, "time", Interval::fixed(0.0)),
                    waves,
                    soundlevel: level(b, soundlevel::NONE),
                    position: Position::parse(b.get("position")),
                })
            }
            RULE_NESTED => Rule::Nested(Nested {
                name: b.get("name").unwrap_or("").trim().to_owned(),
                volume: interval(b, "volume", Interval::fixed(1.0)),
                position: b.get("position").and_then(|v| v.trim().parse::<i32>().ok()).unwrap_or(0),
                position_override: override_index(b, "positionoverride"),
                ambient_position_override: override_index(b, "ambientpositionoverride"),
            }),
            _ => Rule::Other(b.name.clone()),
        }
    }

    pub fn to_block(&self) -> Block {
        match self {
            Rule::Looping(l) => {
                let mut b = Block::new(RULE_LOOPING);
                b.push_prop("volume", l.volume.to_text());
                b.push_prop("pitch", l.pitch.to_text());
                if (l.soundlevel - soundlevel::NORM).abs() > 0.01 {
                    b.push_prop("soundlevel", soundlevel::to_text(l.soundlevel));
                }
                push_position(&mut b, l.position);
                b.push_prop("wave", l.wave.clone());
                b
            }
            Rule::Random(r) => {
                let mut b = Block::new(RULE_RANDOM);
                b.push_prop("time", r.time.to_text());
                b.push_prop("volume", r.volume.to_text());
                b.push_prop("pitch", r.pitch.to_text());
                if r.soundlevel > 0.0 {
                    b.push_prop("soundlevel", soundlevel::to_text(r.soundlevel));
                }
                push_position(&mut b, r.position);
                let mut waves = Block::new("rndwave");
                for w in &r.waves {
                    waves.push_prop("wave", w.clone());
                }
                b.push_child(waves);
                b
            }
            Rule::Nested(n) => {
                let mut b = Block::new(RULE_NESTED);
                b.push_prop("volume", n.volume.to_text());
                b.push_prop("name", n.name.clone());
                if n.position != 0 {
                    b.push_prop("position", n.position.to_string());
                }
                if let Some(i) = n.position_override {
                    b.push_prop("positionoverride", i.to_string());
                }
                if let Some(i) = n.ambient_position_override {
                    b.push_prop("ambientpositionoverride", i.to_string());
                }
                b
            }
            Rule::Other(name) => Block::new(name.clone()),
        }
    }
}

fn push_position(b: &mut Block, p: Position) {
    match p {
        Position::None => {}
        Position::Index(i) => b.push_prop("position", i.to_string()),
        Position::Random => b.push_prop("position", "random"),
    }
}

impl Soundscape {
    pub fn from_block(b: &Block) -> Self {
        Self {
            name: b.name.trim().to_owned(),
            dsp: b.get("dsp").and_then(|v| v.trim().parse::<f32>().ok()).map(|v| v as i32),
            dsp_volume: b.get("dsp_volume").and_then(|v| v.trim().parse().ok()),
            dsp_player: b.get("dsp_player").and_then(|v| v.trim().parse::<f32>().ok()).map(|v| v as i32),
            soundmixer: b.get("soundmixer").map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()),
            rules: b.children.iter().map(Rule::from_block).collect(),
        }
    }

    pub fn waves(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for r in &self.rules {
            match r {
                Rule::Looping(l) if !l.wave.is_empty() => out.push(l.wave.as_str()),
                Rule::Random(r) => out.extend(r.waves.iter().map(String::as_str)),
                _ => {}
            }
        }
        out
    }

    pub fn nested(&self) -> impl Iterator<Item = &Nested> {
        self.rules.iter().filter_map(|r| match r {
            Rule::Nested(n) => Some(n),
            _ => None,
        })
    }
}

pub fn valid_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n.len() <= 128 && !n.contains(['"', '{', '}']) && !n.contains(char::is_whitespace)
}

pub fn parse_file(text: &str) -> Vec<Block> {
    match hp_kv::parse(text) {
        Ok(root) => root.children.into_iter().filter(|b| !b.name.trim().is_empty()).collect(),
        Err(_) => Vec::new(),
    }
}

pub fn write_file(blocks: &[Block]) -> String {
    let mut root = Block::new("");
    root.children = blocks.iter().map(without_empty_values).collect();
    let mut out = String::new();
    for b in &root.children {
        out.push('"');
        out.push_str(&b.name);
        out.push_str("\"\n{\n");
        hp_kv::write_contents(b, 1, &mut out);
        out.push_str("}\n\n");
    }
    out
}

fn without_empty_values(block: &Block) -> Block {
    let mut out = Block::new(block.name.clone());
    out.props = block.props.iter().filter(|(_, v)| !v.trim().is_empty()).cloned().collect();
    out.children = block.children.iter().map(without_empty_values).collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
"gm_x.outside"
{
	"dsp"	"1"
	"custom"	"kept"
	"playlooping"
	{
		"volume"	"0.5"
		"pitch"	"95,105"
		"wave"	"ambient/wind/wind1.wav"
	}
	"playrandom"
	{
		"time"	"5,15"
		"volume"	"0.3,0.6"
		"pitch"	"90,110"
		"soundlevel"	"SNDLVL_70dB"
		"position"	"random"
		"rndwave"
		{
			"wave"	"ambient/birds/bird1.wav"
			"wave"	")ambient/birds/bird2.wav"
		}
	}
	"playsoundscape"
	{
		"name"	"gm_x.base"
		"volume"	"0.7"
		"positionoverride"	"2"
	}
	"playlooping"
	{
		"wave"	"ambient/machines/hum.wav"
		"position"	"1"
		"attenuation"	"1"
	}
}
"bad name"
{
}
"#;

    #[test]
    fn intervals_parse_and_print() {
        assert_eq!(Interval::parse("0.5"), Some(Interval::fixed(0.5)));
        assert_eq!(Interval::parse(" 15 , 5 "), Some(Interval { min: 5.0, max: 15.0 }));
        assert_eq!(Interval::parse("4,"), Some(Interval::fixed(4.0)));
        assert_eq!(Interval::parse("x"), None);
        assert_eq!(Interval::parse("nan"), None);
        assert_eq!(Interval { min: 0.3, max: 0.6 }.to_text(), "0.3,0.6");
        assert_eq!(Interval::fixed(100.0).to_text(), "100");
        assert!((Interval { min: 2.0, max: 4.0 }.sample(0.5) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn a_file_parses_into_typed_rules() {
        let blocks = parse_file(SAMPLE);
        assert_eq!(blocks.len(), 2, "every named block is kept");
        let s = Soundscape::from_block(&blocks[0]);
        assert_eq!(s.name, "gm_x.outside");
        assert_eq!(s.dsp, Some(1));
        assert_eq!(s.rules.len(), 4);
        let Rule::Looping(l) = &s.rules[0] else { panic!("looping") };
        assert_eq!(l.volume, Interval::fixed(0.5));
        assert_eq!(l.pitch, Interval { min: 95.0, max: 105.0 });
        assert_eq!(l.position, Position::None);
        assert!((l.soundlevel - soundlevel::NORM).abs() < 1e-3);
        let Rule::Random(r) = &s.rules[1] else { panic!("random") };
        assert_eq!(r.time, Interval { min: 5.0, max: 15.0 });
        assert_eq!(r.waves.len(), 2);
        assert_eq!(r.position, Position::Random);
        assert!((r.soundlevel - 70.0).abs() < 1e-3);
        let Rule::Nested(n) = &s.rules[2] else { panic!("nested") };
        assert_eq!(n.name, "gm_x.base");
        assert_eq!(n.volume, Interval::fixed(0.7));
        assert_eq!(n.position_override, Some(2));
        let Rule::Looping(hum) = &s.rules[3] else { panic!("looping") };
        assert_eq!(hum.volume, Interval::fixed(0.0), "the engine plays nothing without a volume");
        assert_eq!(hum.position, Position::Index(1));
        assert!((hum.soundlevel - 70.0).abs() < 1e-3, "attenuation 1 is 70 dB");
        assert_eq!(s.waves().len(), 4);
        assert_eq!(s.nested().count(), 1);
    }

    #[test]
    fn rules_round_trip_through_blocks_and_files() {
        let blocks = parse_file(SAMPLE);
        let s = Soundscape::from_block(&blocks[0]);
        for rule in &s.rules[..3] {
            assert_eq!(&Rule::from_block(&rule.to_block()), rule);
        }
        let text = write_file(&blocks);
        assert!(text.starts_with("\"gm_x.outside\"\n{\n"));
        let again = parse_file(&text);
        assert_eq!(again, blocks, "unknown keys and every rule survive");
        let mut with_empty = blocks.clone();
        with_empty[0].push_prop("wave", "  ");
        assert!(!write_file(&with_empty).contains("\"  \""));
    }

    #[test]
    fn waves_are_cleaned_into_game_paths() {
        assert_eq!(wave_path(")ambient\\wind.wav").as_deref(), Some("sound/ambient/wind.wav"));
        assert_eq!(wave_path("sound/a.wav").as_deref(), Some("sound/a.wav"));
        assert_eq!(wave_path("  "), None);
        assert!(valid_name("gm_x.a"));
        assert!(!valid_name("a b"));
        assert!(!valid_name(""));
    }
}
