use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const BUILTIN_COMPILER: &str = "builtin";
pub const BUILTIN_COMPILER_NAME: &str = "Hammer Pro (built-in)";
pub const DETECTED_PREFIX: &str = "detected:";
pub const USER_PREFIX: &str = "user:";
pub const MAX_COMPILERS: usize = 64;
pub const MAX_COMPILER_STEPS: usize = 32;
pub const MAX_COMPILER_NAME: usize = 64;
pub const MAX_COMPILER_TEXT: usize = 2048;
pub const COMPILE_ARGS: &str = r"-game $gamedir $path\$file";
pub const COPY_ARGS: &str = r"$path\$file.bsp $bspdir\$file.bsp";
pub const GAME_EXE: &str = "$gameexe";
pub const GAME_ARGS: &str = "-console -dev -hijack -game $gamedir +map $file";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilerStepKind {
    #[default]
    Bsp,
    Vis,
    Rad,
    Copy,
    Game,
    Custom,
}

impl CompilerStepKind {
    pub const ALL: [CompilerStepKind; 6] = [
        CompilerStepKind::Bsp,
        CompilerStepKind::Vis,
        CompilerStepKind::Rad,
        CompilerStepKind::Copy,
        CompilerStepKind::Game,
        CompilerStepKind::Custom,
    ];

    pub fn key(self) -> &'static str {
        match self {
            CompilerStepKind::Bsp => "bsp",
            CompilerStepKind::Vis => "vis",
            CompilerStepKind::Rad => "rad",
            CompilerStepKind::Copy => "copy",
            CompilerStepKind::Game => "game",
            CompilerStepKind::Custom => "custom",
        }
    }

    pub fn from_key(key: &str) -> Option<CompilerStepKind> {
        let key = key.trim();
        Self::ALL.into_iter().find(|k| k.key().eq_ignore_ascii_case(key))
    }

    pub fn runs_program(self) -> bool {
        self != CompilerStepKind::Copy
    }

    pub fn is_compile(self) -> bool {
        matches!(self, CompilerStepKind::Bsp | CompilerStepKind::Vis | CompilerStepKind::Rad)
    }

    pub fn default_args(self) -> &'static str {
        match self {
            CompilerStepKind::Bsp | CompilerStepKind::Vis | CompilerStepKind::Rad => COMPILE_ARGS,
            CompilerStepKind::Copy => COPY_ARGS,
            CompilerStepKind::Game => GAME_ARGS,
            CompilerStepKind::Custom => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompilerStep {
    pub kind: CompilerStepKind,
    pub enabled: bool,
    pub exe: String,
    pub args: String,
    pub extra: String,
    pub options: BTreeMap<String, String>,
    pub workdir: String,
    pub continue_on_error: bool,
}

impl Default for CompilerStep {
    fn default() -> Self {
        CompilerStep {
            kind: CompilerStepKind::Bsp,
            enabled: true,
            exe: String::new(),
            args: String::new(),
            extra: String::new(),
            options: BTreeMap::new(),
            workdir: String::new(),
            continue_on_error: false,
        }
    }
}

impl CompilerStep {
    pub fn new(kind: CompilerStepKind, exe: impl Into<String>, args: impl Into<String>) -> CompilerStep {
        CompilerStep { kind, exe: exe.into(), args: args.into(), ..CompilerStep::default() }
    }

    pub fn of_kind(kind: CompilerStepKind) -> CompilerStep {
        let exe = if kind == CompilerStepKind::Game { GAME_EXE } else { "" };
        CompilerStep::new(kind, exe, kind.default_args())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompilerProfile {
    pub id: String,
    pub name: String,
    pub steps: Vec<CompilerStep>,
    pub mode_flags: bool,
}

impl Default for CompilerProfile {
    fn default() -> Self {
        CompilerProfile { id: String::new(), name: String::new(), steps: Vec::new(), mode_flags: true }
    }
}

impl CompilerProfile {
    pub fn builtin() -> CompilerProfile {
        CompilerProfile {
            id: BUILTIN_COMPILER.to_owned(),
            name: BUILTIN_COMPILER_NAME.to_owned(),
            steps: Vec::new(),
            mode_flags: true,
        }
    }

    pub fn blank(name: &str) -> CompilerProfile {
        CompilerProfile {
            id: String::new(),
            name: name.to_owned(),
            steps: [CompilerStepKind::Bsp, CompilerStepKind::Vis, CompilerStepKind::Rad]
                .into_iter()
                .map(CompilerStep::of_kind)
                .collect(),
            mode_flags: true,
        }
    }

    pub fn is_builtin(&self) -> bool {
        is_builtin_id(&self.id)
    }

    pub fn is_detected(&self) -> bool {
        self.id.starts_with(DETECTED_PREFIX)
    }

    pub fn is_user(&self) -> bool {
        self.id.starts_with(USER_PREFIX)
    }

    pub fn step(&self, kind: CompilerStepKind) -> Option<&CompilerStep> {
        self.steps.iter().find(|s| s.kind == kind)
    }
}

pub fn is_builtin_id(id: &str) -> bool {
    let id = id.trim();
    id.is_empty() || id.eq_ignore_ascii_case(BUILTIN_COMPILER)
}

pub fn find_compiler<'a>(list: &'a [CompilerProfile], key: &str) -> Option<&'a CompilerProfile> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    list.iter()
        .find(|p| p.id.eq_ignore_ascii_case(key))
        .or_else(|| list.iter().find(|p| p.name.trim().eq_ignore_ascii_case(key)))
}

pub fn all_compilers(stored: &[CompilerProfile], detected: &[CompilerProfile]) -> Vec<CompilerProfile> {
    let mut out = Vec::with_capacity(1 + stored.len() + detected.len());
    out.push(CompilerProfile::builtin());
    for p in detected {
        let edited = stored.iter().find(|s| s.is_detected() && s.id.eq_ignore_ascii_case(&p.id));
        out.push(edited.map_or_else(|| p.clone(), |e| CompilerProfile { name: p.name.clone(), ..e.clone() }));
    }
    out.extend(stored.iter().filter(|p| p.is_user()).cloned());
    out
}

pub fn is_edited(stored: &[CompilerProfile], id: &str) -> bool {
    stored.iter().any(|s| s.is_detected() && s.id.eq_ignore_ascii_case(id))
}

pub fn new_user_id(list: &[CompilerProfile]) -> String {
    let next = list.iter().filter_map(|p| p.id.strip_prefix(USER_PREFIX)?.parse::<u64>().ok()).max().unwrap_or(0) + 1;
    format!("{USER_PREFIX}{next}")
}

pub fn unique_name(taken: &[CompilerProfile], wanted: &str) -> String {
    let base: String = wanted.trim().chars().take(MAX_COMPILER_NAME).collect();
    let base = if base.is_empty() { "Compiler".to_owned() } else { base };
    let used = |n: &str| taken.iter().any(|p| p.name.trim().eq_ignore_ascii_case(n));
    if !used(&base) {
        return base;
    }
    (2..).map(|n| format!("{base} ({n})")).find(|n| !used(n)).unwrap_or(base)
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() }
}

fn tool(dir: &Path, name: &str) -> Option<PathBuf> {
    let p = dir.join(exe_name(name));
    p.is_file().then_some(p)
}

fn rel_key(rel: &[&str]) -> String {
    rel.join("/")
}

fn valve_profile(
    id: String,
    name: String,
    bsp: PathBuf,
    vis: Option<PathBuf>,
    rad: Option<PathBuf>,
) -> CompilerProfile {
    let mut steps = vec![CompilerStep::new(CompilerStepKind::Bsp, bsp.to_string_lossy(), COMPILE_ARGS)];
    if let Some(v) = vis {
        steps.push(CompilerStep::new(CompilerStepKind::Vis, v.to_string_lossy(), COMPILE_ARGS));
    }
    if let Some(r) = rad {
        steps.push(CompilerStep::new(CompilerStepKind::Rad, r.to_string_lossy(), COMPILE_ARGS));
    }
    CompilerProfile { id, name, steps, mode_flags: true }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        let s = s.trim_end_matches('/').to_owned();
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(a) == norm(b)
}

fn hammer_configs(config: &Path, game_dir: &Path) -> Option<(PathBuf, Option<PathBuf>, Option<PathBuf>)> {
    let meta = std::fs::metadata(config).ok()?;
    if !meta.is_file() || meta.len() > 1024 * 1024 {
        return None;
    }
    let text = String::from_utf8_lossy(&std::fs::read(config).ok()?).into_owned();
    let root = hp_kv::parse(&text).ok()?;
    let configs = root.child("Configs").unwrap_or(&root);
    let games = configs.child("Games")?;
    let exists = |v: Option<&str>| v.map(|p| PathBuf::from(p.trim())).filter(|p| p.is_absolute() && p.is_file());
    games.children.iter().find_map(|g| {
        let dir = g.get("GameDir")?;
        if !same_dir(Path::new(dir.trim()), game_dir) {
            return None;
        }
        let h = g.child("Hammer")?;
        Some((exists(h.get("BSP"))?, exists(h.get("Vis")), exists(h.get("Light"))))
    })
}

pub fn detect_compilers(game_dir: &Path) -> Vec<CompilerProfile> {
    let Some(root) = game_dir.parent().filter(|p| !p.as_os_str().is_empty()) else { return Vec::new() };
    let mut out: Vec<CompilerProfile> = Vec::new();
    let push = |p: CompilerProfile, out: &mut Vec<CompilerProfile>| {
        let exes = |p: &CompilerProfile| -> Vec<String> {
            p.steps.iter().map(|s| s.exe.replace('\\', "/").to_lowercase()).collect()
        };
        if !out.iter().any(|q| exes(q) == exes(&p)) {
            out.push(p);
        }
    };
    let folders: [&[&str]; 3] = [&["bin", "win64"], &["bin", "x64"], &["bin"]];
    for rel in folders {
        let dir = rel.iter().fold(root.to_path_buf(), |d, c| d.join(c));
        let key = rel_key(rel);
        if let Some(bsp) = tool(&dir, "vbsp") {
            let p = valve_profile(
                format!("{DETECTED_PREFIX}valve:{key}"),
                format!("Valve ({key})"),
                bsp,
                tool(&dir, "vvis"),
                tool(&dir, "vrad"),
            );
            push(p, &mut out);
        }
    }
    for rel in folders {
        let dir = rel.iter().fold(root.to_path_buf(), |d, c| d.join(c));
        let key = rel_key(rel);
        for (file, kind, label) in [
            (dir.join("hammerplusplus").join("hammerplusplus_gameconfig.txt"), "hammerpp", "Hammer++"),
            (dir.join("GameConfig.txt"), "hammer", "Hammer"),
        ] {
            if let Some((bsp, vis, rad)) = hammer_configs(&file, game_dir) {
                let p =
                    valve_profile(format!("{DETECTED_PREFIX}{kind}:{key}"), format!("{label} ({key})"), bsp, vis, rad);
                push(p, &mut out);
            }
        }
    }
    if out.is_empty()
        && let Some(common) = root.parent()
    {
        for sdk in ["Source SDK Base 2013 Multiplayer", "Source SDK Base 2013 Singleplayer"] {
            let dir = common.join(sdk).join("bin");
            if let Some(bsp) = tool(&dir, "vbsp") {
                let p = valve_profile(
                    format!("{DETECTED_PREFIX}sdk:{sdk}"),
                    format!("Valve ({sdk})"),
                    bsp,
                    tool(&dir, "vvis"),
                    tool(&dir, "vrad"),
                );
                push(p, &mut out);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_app_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().expect("parent")).expect("dir");
        std::fs::write(p, b"MZ").expect("file");
    }

    #[test]
    fn profiles_round_trip_and_old_files_load() {
        let mut p = CompilerProfile::blank("My tools");
        p.id = "user:3".into();
        p.steps[0].exe = r"C:\Program Files\Tools\vbsp.exe".into();
        p.steps[1].enabled = false;
        p.steps.push(CompilerStep { workdir: "$path".into(), ..CompilerStep::of_kind(CompilerStepKind::Custom) });
        p.mode_flags = false;
        let text = toml::to_string(&p).expect("write");
        let back: CompilerProfile = toml::from_str(&text).expect("read");
        assert_eq!(back, p);
        let old: CompilerProfile = toml::from_str("name = \"x\"\n[[steps]]\nkind = \"rad\"\n").expect("partial");
        assert!(old.mode_flags && old.steps[0].enabled && old.steps[0].kind == CompilerStepKind::Rad);
        assert_eq!(CompilerStepKind::from_key("VIS"), Some(CompilerStepKind::Vis));
        assert_eq!(CompilerStepKind::from_key("nope"), None);
        for k in CompilerStepKind::ALL {
            assert_eq!(CompilerStepKind::from_key(k.key()), Some(k));
        }
    }

    #[test]
    fn names_ids_and_lookup() {
        let mut list = vec![CompilerProfile::builtin()];
        let id = new_user_id(&list);
        assert_eq!(id, "user:1");
        list.push(CompilerProfile { id, ..CompilerProfile::blank("Tools") });
        list.push(CompilerProfile { id: "user:7".into(), ..CompilerProfile::blank("Other") });
        assert_eq!(new_user_id(&list), "user:8");
        assert_eq!(unique_name(&list, "tools"), "tools (2)");
        assert_eq!(unique_name(&list, "  New  "), "New");
        assert_eq!(unique_name(&list, ""), "Compiler");
        assert_eq!(find_compiler(&list, "USER:7").map(|p| p.name.as_str()), Some("Other"));
        assert_eq!(find_compiler(&list, "other").map(|p| p.id.as_str()), Some("user:7"));
        assert!(find_compiler(&list, "builtin").is_some_and(CompilerProfile::is_builtin));
        assert!(find_compiler(&list, "").is_none());
        assert!(is_builtin_id("") && is_builtin_id("BuiltIn") && !is_builtin_id("user:1"));
        let all = all_compilers(&list[1..], &[]);
        assert!(all[0].is_builtin() && all[1].is_user());
        let found = CompilerProfile { id: "detected:valve:bin".into(), ..CompilerProfile::blank("Valve (bin)") };
        let mut edited = found.clone();
        edited.steps[2].options.insert("-bounce".into(), "8".into());
        edited.name = "old name".into();
        let stale = CompilerProfile { id: "detected:valve:gone".into(), ..CompilerProfile::blank("Gone") };
        let stored = vec![list[1].clone(), edited, stale];
        let all = all_compilers(&stored, std::slice::from_ref(&found));
        assert_eq!(all.len(), 3, "the stale edit of a tool that is gone is not listed");
        assert_eq!(all[1].name, "Valve (bin)", "detected names stay");
        assert_eq!(all[1].steps[2].options.get("-bounce").map(String::as_str), Some("8"));
        assert!(is_edited(&stored, "DETECTED:valve:bin") && !is_edited(&stored, "user:1"));
    }

    #[test]
    fn valve_tools_are_found_next_to_the_game() {
        let root = scratch("compilers_detect");
        let game = root.join("GarrysMod").join("garrysmod");
        std::fs::create_dir_all(&game).expect("game");
        assert!(detect_compilers(&game).is_empty());
        let bin = root.join("GarrysMod").join("bin");
        touch(&bin.join(exe_name("vbsp")));
        touch(&bin.join(exe_name("vvis")));
        touch(&bin.join(exe_name("vrad")));
        touch(&bin.join("win64").join(exe_name("vbsp")));
        touch(&bin.join("win64").join(exe_name("vrad")));
        let found = detect_compilers(&game);
        let names: Vec<&str> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Valve (bin/win64)", "Valve (bin)"]);
        assert!(found.iter().all(CompilerProfile::is_detected));
        let kinds: Vec<CompilerStepKind> = found[0].steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [CompilerStepKind::Bsp, CompilerStepKind::Rad], "no vvis in win64");
        assert_eq!(found[1].steps.len(), 3);
        assert_eq!(found[1].steps[2].args, COMPILE_ARGS);
        assert_eq!(PathBuf::from(&found[1].steps[0].exe), bin.join(exe_name("vbsp")));

        let custom = root.join("tools");
        touch(&custom.join("vbsp_pp.exe"));
        touch(&custom.join("vrad_pp.exe"));
        let config = format!(
            "\"Configs\"\n{{\n\"Games\"\n{{\n\"Garry's Mod\"\n{{\n\"GameDir\" \"{}\"\n\"Hammer\"\n{{\n\"BSP\" \"{}\"\n\"Vis\" \"{}\"\n\"Light\" \"{}\"\n}}\n}}\n}}\n}}\n",
            game.display(),
            custom.join("vbsp_pp.exe").display(),
            bin.join(exe_name("vvis")).display(),
            custom.join("vrad_pp.exe").display(),
        );
        let hpp = bin.join("win64").join("hammerplusplus").join("hammerplusplus_gameconfig.txt");
        std::fs::create_dir_all(hpp.parent().expect("parent")).expect("dir");
        std::fs::write(&hpp, config).expect("config");
        let same = format!(
            "\"Configs\"{{\"Games\"{{\"GMod\"{{\"GameDir\" \"{}\" \"Hammer\"{{\"BSP\" \"{}\" \"Vis\" \"{}\" \"Light\" \"{}\"}}}}}}}}",
            game.display(),
            bin.join(exe_name("vbsp")).display(),
            bin.join(exe_name("vvis")).display(),
            bin.join(exe_name("vrad")).display(),
        );
        std::fs::write(bin.join("GameConfig.txt"), same).expect("hammer config");
        let found = detect_compilers(&game);
        let names: Vec<&str> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Valve (bin/win64)", "Valve (bin)", "Hammer++ (bin/win64)"], "Hammer's own is the same");
        assert!(found[2].steps[0].exe.ends_with("vbsp_pp.exe"));
        assert_eq!(found[2].id, "detected:hammerpp:bin/win64");
        let other_game = root.join("GarrysMod").join("other");
        std::fs::create_dir_all(&other_game).expect("other");
        assert_eq!(detect_compilers(&other_game).len(), 2, "the Hammer++ config is for another game");
    }

    #[test]
    fn source_sdk_tools_for_other_games() {
        let root = scratch("compilers_sdk");
        let game = root.join("Half-Life 2").join("hl2");
        std::fs::create_dir_all(&game).expect("game");
        let sdk = root.join("Source SDK Base 2013 Singleplayer").join("bin");
        touch(&sdk.join(exe_name("vbsp")));
        let found = detect_compilers(&game);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Valve (Source SDK Base 2013 Singleplayer)");
        assert!(detect_compilers(Path::new("")).is_empty());
    }
}
