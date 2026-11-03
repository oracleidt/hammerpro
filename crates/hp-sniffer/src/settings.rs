use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_LAUNCH_OPTIONS: &str =
    "-windowed -w 800 -h 450 -novid -nojoy -nosplash -console +mat_picmip 4 +fps_max 30 +volume 0";

pub const DEFAULT_TEMPLATE: &str = "{map}_sniffed";

pub const GMOD_APP_ID: u32 = 4000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsecureMode {
    #[default]
    Auto,
    Always,
    Never,
}

impl InsecureMode {
    pub const ALL: [InsecureMode; 3] = [InsecureMode::Auto, InsecureMode::Always, InsecureMode::Never];

    pub fn id(self) -> &'static str {
        match self {
            InsecureMode::Auto => "auto",
            InsecureMode::Always => "always",
            InsecureMode::Never => "never",
        }
    }

    pub fn from_id(id: &str) -> Option<InsecureMode> {
        InsecureMode::ALL.into_iter().find(|m| m.id().eq_ignore_ascii_case(id.trim()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameBits {
    #[default]
    Auto,
    Bits64,
    Bits32,
}

impl GameBits {
    pub const ALL: [GameBits; 3] = [GameBits::Auto, GameBits::Bits64, GameBits::Bits32];

    pub fn id(self) -> &'static str {
        match self {
            GameBits::Auto => "auto",
            GameBits::Bits64 => "64",
            GameBits::Bits32 => "32",
        }
    }

    pub fn from_id(id: &str) -> Option<GameBits> {
        GameBits::ALL.into_iter().find(|m| m.id().eq_ignore_ascii_case(id.trim()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometrySource {
    #[default]
    Auto,
    Memory,
    File,
}

impl GeometrySource {
    pub const ALL: [GeometrySource; 3] = [GeometrySource::Auto, GeometrySource::Memory, GeometrySource::File];

    pub fn id(self) -> &'static str {
        match self {
            GeometrySource::Auto => "auto",
            GeometrySource::Memory => "memory",
            GeometrySource::File => "file",
        }
    }

    pub fn from_id(id: &str) -> Option<GeometrySource> {
        GeometrySource::ALL.into_iter().find(|m| m.id().eq_ignore_ascii_case(id.trim()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapFileMode {
    #[default]
    Compare,
    Never,
}

impl MapFileMode {
    pub const ALL: [MapFileMode; 2] = [MapFileMode::Compare, MapFileMode::Never];

    pub fn id(self) -> &'static str {
        match self {
            MapFileMode::Compare => "compare",
            MapFileMode::Never => "never",
        }
    }

    pub fn from_id(id: &str) -> Option<MapFileMode> {
        MapFileMode::ALL.into_iter().find(|m| m.id().eq_ignore_ascii_case(id.trim()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DecompileSettings {
    pub brushes: bool,
    pub displacements: bool,
    pub entities: bool,
    pub brush_entities: bool,
    pub static_props: bool,
    pub overlays: bool,
    pub cubemaps: bool,
    pub areaportals: bool,
    pub occluders: bool,
    pub group_details: bool,
}

impl Default for DecompileSettings {
    fn default() -> Self {
        let t = hp_decompile::Toggles::default();
        Self {
            brushes: t.brushes,
            displacements: t.displacements,
            entities: t.entities,
            brush_entities: t.brush_entities,
            static_props: t.static_props,
            overlays: t.overlays,
            cubemaps: t.cubemaps,
            areaportals: t.areaportals,
            occluders: t.occluders,
            group_details: t.group_details,
        }
    }
}

impl DecompileSettings {
    pub const KEYS: [&'static str; 10] = [
        "brushes",
        "displacements",
        "entities",
        "brush_entities",
        "static_props",
        "overlays",
        "cubemaps",
        "areaportals",
        "occluders",
        "group_details",
    ];

    pub fn toggles(&self) -> hp_decompile::Toggles {
        hp_decompile::Toggles {
            brushes: self.brushes,
            displacements: self.displacements,
            entities: self.entities,
            brush_entities: self.brush_entities,
            static_props: self.static_props,
            overlays: self.overlays,
            cubemaps: self.cubemaps,
            areaportals: self.areaportals,
            occluders: self.occluders,
            group_details: self.group_details,
        }
    }

    pub fn get(&self, key: &str) -> Option<bool> {
        Some(match key {
            "brushes" => self.brushes,
            "displacements" => self.displacements,
            "entities" => self.entities,
            "brush_entities" => self.brush_entities,
            "static_props" => self.static_props,
            "overlays" => self.overlays,
            "cubemaps" => self.cubemaps,
            "areaportals" => self.areaportals,
            "occluders" => self.occluders,
            "group_details" => self.group_details,
            _ => return None,
        })
    }

    pub fn set(&mut self, key: &str, value: bool) -> bool {
        let slot = match key {
            "brushes" => &mut self.brushes,
            "displacements" => &mut self.displacements,
            "entities" => &mut self.entities,
            "brush_entities" => &mut self.brush_entities,
            "static_props" => &mut self.static_props,
            "overlays" => &mut self.overlays,
            "cubemaps" => &mut self.cubemaps,
            "areaportals" => &mut self.areaportals,
            "occluders" => &mut self.occluders,
            "group_details" => &mut self.group_details,
            _ => return false,
        };
        *slot = value;
        true
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnifferSettings {
    pub address: String,
    pub authorized: bool,
    pub consent_seen: bool,
    pub output_dir: String,
    pub file_name_template: String,
    pub gmod_dir: String,
    pub launch_options: String,
    pub insecure: InsecureMode,
    pub game_bits: GameBits,
    #[serde(skip)]
    pub geometry: GeometrySource,
    #[serde(skip)]
    pub map_file: MapFileMode,
    pub use_map_file: bool,
    pub read_ldr_lighting: bool,
    pub attach_running: bool,
    pub process_timeout_secs: u32,
    pub load_timeout_secs: u32,
    pub settle_secs: u32,
    pub poll_secs: u32,
    pub save_models: bool,
    pub save_materials: bool,
    pub save_sounds: bool,
    pub save_other: bool,
    pub keep_bsp: bool,
    pub only_non_stock: bool,
    pub kill_game: bool,
    pub auto_disconnect: bool,
    pub open_when_done: bool,
    pub decompile: DecompileSettings,
}

impl Default for SnifferSettings {
    fn default() -> Self {
        Self {
            address: String::new(),
            authorized: false,
            consent_seen: false,
            output_dir: String::new(),
            file_name_template: DEFAULT_TEMPLATE.to_owned(),
            gmod_dir: String::new(),
            launch_options: DEFAULT_LAUNCH_OPTIONS.to_owned(),
            insecure: InsecureMode::Auto,
            game_bits: GameBits::Auto,
            geometry: GeometrySource::Auto,
            map_file: MapFileMode::Never,
            use_map_file: false,
            read_ldr_lighting: true,
            attach_running: false,
            process_timeout_secs: 180,
            load_timeout_secs: 900,
            settle_secs: 6,
            poll_secs: 3,
            save_models: true,
            save_materials: true,
            save_sounds: true,
            save_other: true,
            keep_bsp: true,
            only_non_stock: true,
            kill_game: true,
            auto_disconnect: true,
            open_when_done: true,
            decompile: DecompileSettings::default(),
        }
    }
}

pub const SETTING_KEYS: &[(&str, &str)] = &[
    ("address", "text"),
    ("authorized", "bool"),
    ("output_dir", "text"),
    ("file_name_template", "text"),
    ("gmod_dir", "text"),
    ("launch_options", "text"),
    ("insecure", "auto|always|never"),
    ("game_bits", "auto|64|32"),
    ("use_map_file", "bool"),
    ("read_ldr_lighting", "bool"),
    ("attach_running", "bool"),
    ("process_timeout_secs", "number"),
    ("load_timeout_secs", "number"),
    ("settle_secs", "number"),
    ("poll_secs", "number"),
    ("save_models", "bool"),
    ("save_materials", "bool"),
    ("save_sounds", "bool"),
    ("save_other", "bool"),
    ("keep_bsp", "bool"),
    ("only_non_stock", "bool"),
    ("kill_game", "bool"),
    ("auto_disconnect", "bool"),
    ("open_when_done", "bool"),
    ("decompile.<option>", "bool"),
];

impl SnifferSettings {
    pub fn clamped(mut self) -> Self {
        self.map_file = if self.use_map_file { MapFileMode::Compare } else { MapFileMode::Never };
        self.process_timeout_secs = self.process_timeout_secs.clamp(10, 3600);
        self.load_timeout_secs = self.load_timeout_secs.clamp(30, 7200);
        self.settle_secs = self.settle_secs.clamp(0, 300);
        self.poll_secs = self.poll_secs.clamp(1, 60);
        if self.file_name_template.trim().is_empty() {
            self.file_name_template = DEFAULT_TEMPLATE.to_owned();
        }
        self
    }

    pub fn get(&self, key: &str) -> Option<SettingValue> {
        use SettingValue as V;
        if let Some(rest) = key.strip_prefix("decompile.") {
            return self.decompile.get(rest).map(V::Bool);
        }
        Some(match key {
            "address" => V::Text(self.address.clone()),
            "authorized" => V::Bool(self.authorized),
            "output_dir" => V::Text(self.output_dir.clone()),
            "file_name_template" => V::Text(self.file_name_template.clone()),
            "gmod_dir" => V::Text(self.gmod_dir.clone()),
            "launch_options" => V::Text(self.launch_options.clone()),
            "insecure" => V::Text(self.insecure.id().to_owned()),
            "game_bits" => V::Text(self.game_bits.id().to_owned()),
            "use_map_file" => V::Bool(self.use_map_file),
            "read_ldr_lighting" => V::Bool(self.read_ldr_lighting),
            "attach_running" => V::Bool(self.attach_running),
            "process_timeout_secs" => V::Number(f64::from(self.process_timeout_secs)),
            "load_timeout_secs" => V::Number(f64::from(self.load_timeout_secs)),
            "settle_secs" => V::Number(f64::from(self.settle_secs)),
            "poll_secs" => V::Number(f64::from(self.poll_secs)),
            "save_models" => V::Bool(self.save_models),
            "save_materials" => V::Bool(self.save_materials),
            "save_sounds" => V::Bool(self.save_sounds),
            "save_other" => V::Bool(self.save_other),
            "keep_bsp" => V::Bool(self.keep_bsp),
            "only_non_stock" => V::Bool(self.only_non_stock),
            "kill_game" => V::Bool(self.kill_game),
            "auto_disconnect" => V::Bool(self.auto_disconnect),
            "open_when_done" => V::Bool(self.open_when_done),
            _ => return None,
        })
    }

    pub fn set(&mut self, key: &str, value: &SettingValue) -> Result<(), String> {
        use SettingValue as V;
        if let Some(rest) = key.strip_prefix("decompile.") {
            let V::Bool(b) = value else { return Err(format!("{key} needs true or false")) };
            return if self.decompile.set(rest, *b) { Ok(()) } else { Err(format!("unknown decompile option {rest}")) };
        }
        let bool_of = |v: &V| match v {
            V::Bool(b) => Ok(*b),
            V::Text(t) => match t.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Ok(true),
                "false" | "0" | "no" | "off" => Ok(false),
                _ => Err(format!("{key} needs true or false")),
            },
            V::Number(n) => Ok(*n != 0.0),
        };
        let num_of = |v: &V| match v {
            V::Number(n) if n.is_finite() && *n >= 0.0 => Ok(*n as u32),
            V::Text(t) => t.trim().parse::<u32>().map_err(|_| format!("{key} needs a whole number")),
            _ => Err(format!("{key} needs a whole number")),
        };
        let text_of = |v: &V| match v {
            V::Text(t) => Ok::<String, String>(t.clone()),
            V::Bool(b) => Ok(b.to_string()),
            V::Number(n) => Ok(n.to_string()),
        };
        match key {
            "address" => self.address = text_of(value)?.trim().to_owned(),
            "authorized" => self.authorized = bool_of(value)?,
            "output_dir" => self.output_dir = text_of(value)?,
            "file_name_template" => self.file_name_template = text_of(value)?,
            "gmod_dir" => self.gmod_dir = text_of(value)?,
            "launch_options" => self.launch_options = text_of(value)?,
            "insecure" => {
                let t = text_of(value)?;
                self.insecure =
                    InsecureMode::from_id(&t).ok_or_else(|| "insecure must be auto, always or never".to_owned())?;
            }
            "use_map_file" => {
                self.use_map_file = bool_of(value)?;
                self.map_file = if self.use_map_file { MapFileMode::Compare } else { MapFileMode::Never };
            }
            "read_ldr_lighting" => self.read_ldr_lighting = bool_of(value)?,
            "game_bits" => {
                let t = text_of(value)?;
                self.game_bits = GameBits::from_id(&t).ok_or_else(|| "game_bits must be auto, 64 or 32".to_owned())?;
            }
            "attach_running" => self.attach_running = bool_of(value)?,
            "process_timeout_secs" => self.process_timeout_secs = num_of(value)?,
            "load_timeout_secs" => self.load_timeout_secs = num_of(value)?,
            "settle_secs" => self.settle_secs = num_of(value)?,
            "poll_secs" => self.poll_secs = num_of(value)?,
            "save_models" => self.save_models = bool_of(value)?,
            "save_materials" => self.save_materials = bool_of(value)?,
            "save_sounds" => self.save_sounds = bool_of(value)?,
            "save_other" => self.save_other = bool_of(value)?,
            "keep_bsp" => self.keep_bsp = bool_of(value)?,
            "only_non_stock" => self.only_non_stock = bool_of(value)?,
            "kill_game" => self.kill_game = bool_of(value)?,
            "auto_disconnect" => self.auto_disconnect = bool_of(value)?,
            "open_when_done" => self.open_when_done = bool_of(value)?,
            _ => return Err(format!("unknown sniffer setting {key}")),
        }
        *self = self.clone().clamped();
        Ok(())
    }

    pub fn set_internal(&mut self, key: &str, value: &SettingValue) -> Result<(), String> {
        let text = match value {
            SettingValue::Text(t) => t.clone(),
            SettingValue::Bool(b) => b.to_string(),
            SettingValue::Number(n) => n.to_string(),
        };
        match key {
            "geometry" => {
                self.geometry =
                    GeometrySource::from_id(&text).ok_or_else(|| "geometry must be auto, memory or file".to_owned())?;
                Ok(())
            }
            "map_file" => {
                self.map_file =
                    MapFileMode::from_id(&text).ok_or_else(|| "map_file must be compare or never".to_owned())?;
                self.use_map_file = self.map_file == MapFileMode::Compare;
                Ok(())
            }
            _ => self.set(key, value),
        }
    }

    pub fn output_root(&self) -> PathBuf {
        let t = self.output_dir.trim();
        if t.is_empty() { default_output_dir() } else { PathBuf::from(t) }
    }

    pub fn validate(&self) -> Result<(), String> {
        Ok(())
    }

    pub fn file_allowed(&self) -> bool {
        self.use_map_file
    }

    pub fn map_file_mode(&self) -> MapFileMode {
        if self.use_map_file { MapFileMode::Compare } else { MapFileMode::Never }
    }

    pub fn parsed_address(&self) -> Result<(String, u16), String> {
        if let Some(name) = self.local_map() {
            if name.is_empty() && !self.attach_running {
                return Err(
                    "enter map:<name> to start a map in a local game, or tick the running game option".to_owned()
                );
            }
            return Ok(("127.0.0.1".to_owned(), 0));
        }
        parse_address(&self.address)
    }

    pub fn local_map(&self) -> Option<String> {
        local_map_of(&self.address)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

pub fn default_output_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("HammerPro").join("sniffer")
}

pub fn local_map_of(text: &str) -> Option<String> {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    let rest = lower.strip_prefix("map:").or_else(|| lower.strip_prefix("local:"))?;
    let name: String =
        rest.trim().chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')).collect();
    Some(name)
}

pub fn parse_address(text: &str) -> Result<(String, u16), String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("enter a server address as ip:port".to_owned());
    }
    let (host, port) = match t.rsplit_once(':') {
        Some((h, p)) => (h.trim(), p.trim().parse::<u16>().map_err(|_| format!("{p} is not a valid port"))?),
        None => (t, 27015),
    };
    if port == 0 {
        return Err("the port cannot be 0".to_owned());
    }
    let ok_host = !host.is_empty()
        && host.len() <= 253
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');
    if !ok_host {
        return Err(format!("{host} is not a valid host"));
    }
    Ok((host.to_owned(), port))
}

pub fn sanitize_file_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ' | '(' | ')') { c } else { '_' })
        .collect();
    out = out.trim().trim_matches('.').to_owned();
    while out.contains("..") {
        out = out.replace("..", ".");
    }
    let reserved = ["con", "prn", "aux", "nul", "com1", "com2", "com3", "lpt1", "lpt2", "lpt3"];
    if reserved.contains(&out.to_ascii_lowercase().as_str()) {
        out.push('_');
    }
    if out.is_empty() {
        out = "sniffed".to_owned();
    }
    out.truncate(120);
    out
}

pub struct NameContext<'a> {
    pub map: &'a str,
    pub host: &'a str,
    pub port: u16,
    pub unix_seconds: u64,
}

pub fn civil_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i64;
    let rem = (secs % 86400) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

pub fn output_name(template: &str, ctx: &NameContext<'_>) -> String {
    let (y, mo, d, h, mi, s) = civil_from_unix(ctx.unix_seconds);
    let date = format!("{y:04}-{mo:02}-{d:02}");
    let time = format!("{h:02}-{mi:02}-{s:02}");
    let t = if template.trim().is_empty() { DEFAULT_TEMPLATE } else { template };
    let filled = t
        .replace("{map}", ctx.map)
        .replace("{ip}", ctx.host)
        .replace("{host}", ctx.host)
        .replace("{port}", &ctx.port.to_string())
        .replace("{date}", &date)
        .replace("{time}", &time)
        .replace("{datetime}", &format!("{date}_{time}"));
    let stem = filled.strip_suffix(".vmf").or_else(|| filled.strip_suffix(".VMF")).unwrap_or(&filled);
    sanitize_file_name(stem)
}

pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..10_000 {
        let p = dir.join(format!("{stem}_{n}.{ext}"));
        if !p.exists() {
            return p;
        }
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_map_replaces_the_server_address() {
        assert_eq!(local_map_of("map:gm_construct").as_deref(), Some("gm_construct"));
        assert_eq!(local_map_of(" LOCAL: My-Map_2 ").as_deref(), Some("my-map_2"));
        assert_eq!(local_map_of("map:").as_deref(), Some(""));
        assert_eq!(local_map_of("1.2.3.4:27015"), None);
        assert_eq!(local_map_of("map:a;b/c").as_deref(), Some("abc"));
        let mut s = SnifferSettings { address: "map:gm_construct".into(), ..SnifferSettings::default() };
        assert_eq!(s.parsed_address(), Ok(("127.0.0.1".to_owned(), 0)));
        s.address = "map:".into();
        assert!(s.parsed_address().is_err(), "an empty map name needs the running game");
        s.attach_running = true;
        assert!(s.parsed_address().is_ok());
    }

    #[test]
    fn addresses_are_parsed() {
        assert_eq!(parse_address("141.94.238.112:27015"), Ok(("141.94.238.112".to_owned(), 27015)));
        assert_eq!(parse_address(" example.com "), Ok(("example.com".to_owned(), 27015)));
        assert!(parse_address("").is_err());
        assert!(parse_address("1.2.3.4:0").is_err());
        assert!(parse_address("1.2.3.4:99999").is_err());
        assert!(parse_address("bad host:27015").is_err());
        assert!(parse_address("a;b:1").is_err());
    }

    #[test]
    fn names_follow_the_template() {
        let ctx =
            NameContext { map: "rp_rockford_v2b", host: "141.94.238.112", port: 27015, unix_seconds: 1_793_000_000 };
        assert_eq!(output_name("{map}_sniffed", &ctx), "rp_rockford_v2b_sniffed");
        assert_eq!(output_name("{map}-{ip}-{port}.vmf", &ctx), "rp_rockford_v2b-141.94.238.112-27015");
        let named = output_name("{map}_{datetime}", &ctx);
        assert!(named.starts_with("rp_rockford_v2b_20"), "{named}");
        assert_eq!(output_name("", &ctx), "rp_rockford_v2b_sniffed");
        let evil = output_name("../../evil", &ctx);
        assert!(
            !evil.contains('/') && !evil.contains(char::from(92u8)) && !evil.starts_with('.') && !evil.contains(".."),
            "{evil}"
        );
        assert_eq!(output_name("con", &ctx), "con_");
        assert!(!output_name("a/b\\c:d", &ctx).contains(['/', '\\', ':']));
    }

    #[test]
    fn dates_are_civil() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil_from_unix(951_782_400), (2000, 2, 29, 0, 0, 0));
        assert_eq!(civil_from_unix(1_793_000_000).0, 2026);
    }

    #[test]
    fn unique_paths_do_not_overwrite() {
        let dir = std::env::temp_dir().join(format!("hp_sniffer_unique_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let a = unique_path(&dir, "map", "vmf");
        assert_eq!(a, dir.join("map.vmf"));
        std::fs::write(&a, b"x").expect("write");
        assert_eq!(unique_path(&dir, "map", "vmf"), dir.join("map_2.vmf"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_round_trip_and_clamp() {
        let mut s = SnifferSettings::default();
        assert!(s.set("address", &SettingValue::Text("1.2.3.4:27016".into())).is_ok());
        assert!(s.set("kill_game", &SettingValue::Bool(false)).is_ok());
        assert!(s.set("decompile.displacements", &SettingValue::Bool(false)).is_ok());
        assert!(s.set("load_timeout_secs", &SettingValue::Number(1.0)).is_ok());
        assert_eq!(s.load_timeout_secs, 30);
        assert!(s.set("insecure", &SettingValue::Text("always".into())).is_ok());
        assert!(s.set("insecure", &SettingValue::Text("maybe".into())).is_err());
        assert!(s.set("nope", &SettingValue::Bool(true)).is_err());
        assert!(s.set("decompile.nope", &SettingValue::Bool(true)).is_err());
        assert!(!s.decompile.toggles().displacements);
        let text = toml::to_string(&s).expect("toml");
        let back: SnifferSettings = toml::from_str(&text).expect("back");
        assert_eq!(back, s);
        let partial: SnifferSettings = toml::from_str("address = \"x:1\"").expect("partial");
        assert!(partial.save_models && partial.kill_game);
        for (key, _) in SETTING_KEYS {
            if !key.contains('<') {
                assert!(SnifferSettings::default().get(key).is_some(), "{key}");
            }
        }
    }

    #[test]
    fn the_map_file_is_off_by_default_and_the_new_settings_are_reachable() {
        let mut s = SnifferSettings::default();
        assert!(!s.use_map_file && !s.file_allowed());
        assert_eq!(s.map_file_mode(), MapFileMode::Never);
        assert!(s.read_ldr_lighting);
        assert_eq!(s.game_bits, GameBits::Auto);
        assert_eq!(s.get("use_map_file"), Some(SettingValue::Bool(false)));
        assert!(s.set("use_map_file", &SettingValue::Text("yes".into())).is_ok());
        assert!(s.file_allowed());
        assert_eq!(s.map_file_mode(), MapFileMode::Compare);
        assert!(s.set("read_ldr_lighting", &SettingValue::Bool(false)).is_ok());
        assert!(s.set("game_bits", &SettingValue::Text("32".into())).is_ok());
        assert_eq!(s.get("game_bits"), Some(SettingValue::Text("32".to_owned())));
        assert!(s.set("game_bits", &SettingValue::Text("16".into())).is_err());
        let text = toml::to_string(&s).expect("toml");
        let back: SnifferSettings = toml::from_str(&text).expect("back");
        assert!(back.file_allowed() && !back.read_ldr_lighting && back.game_bits == GameBits::Bits32);
        assert_eq!(back.map_file_mode(), MapFileMode::Compare, "the mode follows the saved switch, not a hidden field");
        let old: SnifferSettings = toml::from_str("address = \"x:1\"").expect("old file");
        assert!(!old.file_allowed(), "settings saved before the switch existed do not read map files");
    }
}
