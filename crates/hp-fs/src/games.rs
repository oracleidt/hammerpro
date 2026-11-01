use std::path::{Path, PathBuf};

pub use hp_bsp::limits::LimitSet;
pub use hp_bsp::{Branch, BspFormat};

use crate::detect::{LibraryFolder, parse_library_folders, path_key};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LightingPasses {
    #[default]
    Both,
    HdrOnly,
    LdrOnly,
}

impl LightingPasses {
    pub const fn flag(self) -> &'static str {
        match self {
            LightingPasses::Both => "-both",
            LightingPasses::HdrOnly => "-hdr",
            LightingPasses::LdrOnly => "-ldr",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnownGame {
    pub id: &'static str,
    pub name: &'static str,
    pub app_id: u32,
    pub install_dirs: &'static [&'static str],
    pub game_folder: &'static str,
    pub exes: &'static [&'static str],
    pub fgds: &'static [&'static str],
    pub branch: Option<Branch>,
    pub unsupported: Option<&'static str>,
    pub lighting: LightingPasses,
    pub skybox: &'static str,
    pub map_prefix: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn game(
    id: &'static str,
    name: &'static str,
    app_id: u32,
    install_dirs: &'static [&'static str],
    game_folder: &'static str,
    exes: &'static [&'static str],
    fgds: &'static [&'static str],
    branch: Branch,
) -> KnownGame {
    KnownGame {
        id,
        name,
        app_id,
        install_dirs,
        game_folder,
        exes,
        fgds,
        branch: Some(branch),
        unsupported: None,
        lighting: LightingPasses::Both,
        skybox: "sky_day01_01",
        map_prefix: "",
    }
}

const fn unsupported(
    id: &'static str,
    name: &'static str,
    app_id: u32,
    install_dirs: &'static [&'static str],
    game_folder: &'static str,
    reason: &'static str,
) -> KnownGame {
    KnownGame {
        id,
        name,
        app_id,
        install_dirs,
        game_folder,
        exes: &[],
        fgds: &[],
        branch: None,
        unsupported: Some(reason),
        lighting: LightingPasses::Both,
        skybox: "",
        map_prefix: "",
    }
}

pub const GMOD_ID: &str = "gmod";

pub const SDK2013_SP_APP_ID: u32 = 243_730;
pub const SDK2013_MP_APP_ID: u32 = 243_750;
pub const SDK2007_APP_ID: u32 = 218;
pub const SDK2006_APP_ID: u32 = 215;

const HL2_EXES: &[&str] = &["hl2_win64.exe", "hl2.exe", "bin/win64/hl2.exe"];

pub static KNOWN_GAMES: &[KnownGame] = &[
    KnownGame {
        skybox: "painted",
        map_prefix: "gm_",
        ..game(
            GMOD_ID,
            "Garry's Mod",
            4000,
            &["GarrysMod"],
            "garrysmod",
            &["bin/win64/gmod.exe", "gmod.exe", "hl2.exe"],
            &["garrysmod.fgd"],
            Branch::GarrysMod,
        )
    },
    game("hl2", "Half-Life 2", 220, &["Half-Life 2"], "hl2", HL2_EXES, &["halflife2.fgd"], Branch::Source2013),
    game(
        "episodic",
        "Half-Life 2: Episode One",
        380,
        &["Half-Life 2", "Half-Life 2 Episode One"],
        "episodic",
        HL2_EXES,
        &["halflife2.fgd"],
        Branch::Source2013,
    ),
    game(
        "ep2",
        "Half-Life 2: Episode Two",
        420,
        &["Half-Life 2", "Half-Life 2 Episode Two"],
        "ep2",
        HL2_EXES,
        &["halflife2.fgd"],
        Branch::Source2013,
    ),
    game(
        "lostcoast",
        "Half-Life 2: Lost Coast",
        340,
        &["Half-Life 2", "Half-Life 2 Lost Coast"],
        "lostcoast",
        HL2_EXES,
        &["halflife2.fgd"],
        Branch::Source2013,
    ),
    game(
        "hl1",
        "Half-Life: Source",
        280,
        &["Half-Life 2", "Half-Life Source"],
        "hl1",
        HL2_EXES,
        &["halflife.fgd"],
        Branch::Source2013,
    ),
    KnownGame {
        map_prefix: "testchmb_",
        ..game(
            "portal",
            "Portal",
            400,
            &["Portal"],
            "portal",
            &["portal_win64.exe", "hl2.exe"],
            &["portal.fgd"],
            Branch::Source2013,
        )
    },
    game(
        "sdk2013sp",
        "Source SDK Base 2013 Singleplayer",
        SDK2013_SP_APP_ID,
        &["Source SDK Base 2013 Singleplayer"],
        "hl2",
        &["hl2_win64.exe", "hl2.exe"],
        &["halflife2.fgd"],
        Branch::Source2013,
    ),
    game(
        "synergy",
        "Synergy",
        17_520,
        &["Synergy"],
        "synergy",
        &["synergy_win64.exe", "synergy.exe", "hl2.exe"],
        &["synergy.fgd"],
        Branch::Source2013,
    ),
    KnownGame {
        map_prefix: "dm_",
        ..game(
            "hl2mp",
            "Half-Life 2: Deathmatch",
            320,
            &["Half-Life 2 Deathmatch"],
            "hl2mp",
            &["hl2mp_win64.exe", "hl2mp.exe", "hl2.exe"],
            &["hl2mp.fgd"],
            Branch::Source2013,
        )
    },
    KnownGame {
        map_prefix: "de_",
        ..game(
            "cstrike",
            "Counter-Strike: Source",
            240,
            &["Counter-Strike Source"],
            "cstrike",
            &["cstrike_win64.exe", "cstrike.exe", "hl2.exe"],
            &["cstrike.fgd"],
            Branch::Source2013,
        )
    },
    KnownGame {
        map_prefix: "dod_",
        ..game(
            "dod",
            "Day of Defeat: Source",
            300,
            &["Day of Defeat Source"],
            "dod",
            &["dod_win64.exe", "dod.exe", "hl2.exe"],
            &["dod.fgd"],
            Branch::Source2013,
        )
    },
    KnownGame {
        skybox: "sky_tf2_04",
        map_prefix: "ctf_",
        ..game(
            "tf",
            "Team Fortress 2",
            440,
            &["Team Fortress 2"],
            "tf",
            &["tf_win64.exe", "tf.exe", "hl2.exe"],
            &["tf.fgd"],
            Branch::Source2013,
        )
    },
    game(
        "sdk2013mp",
        "Source SDK Base 2013 Multiplayer",
        SDK2013_MP_APP_ID,
        &["Source SDK Base 2013 Multiplayer"],
        "hl2mp",
        &["hl2_win64.exe", "hl2.exe"],
        &["hl2mp.fgd"],
        Branch::Source2013,
    ),
    game(
        "nmrih",
        "No More Room in Hell",
        224_260,
        &["nmrih"],
        "nmrih",
        &["nmrih.exe", "hl2.exe"],
        &["nmrih.fgd"],
        Branch::Source2013,
    ),
    game(
        "zps",
        "Zombie Panic! Source",
        17_500,
        &["Zombie Panic Source"],
        "zps",
        &["zps.exe", "hl2.exe"],
        &["zps.fgd"],
        Branch::Source2013,
    ),
    game(
        "fof",
        "Fistful of Frags",
        265_630,
        &["Fistful of Frags"],
        "fof",
        &["fof.exe", "hl2.exe"],
        &["fof.fgd"],
        Branch::Source2013,
    ),
    game(
        "sdk2007",
        "Source SDK Base 2007",
        SDK2007_APP_ID,
        &["Source SDK Base 2007"],
        "hl2",
        &["hl2.exe"],
        &["halflife2.fgd"],
        Branch::Source2007,
    ),
    KnownGame {
        map_prefix: "bm_",
        ..game("bms", "Black Mesa", 362_890, &["Black Mesa"], "bms", &["bms.exe"], &["bms.fgd"], Branch::BlackMesa)
    },
    game("ship", "The Ship", 2400, &["The Ship"], "ship", &["ship.exe", "hl2.exe"], &["ship.fgd"], Branch::TheShip),
    game(
        "pm",
        "Bloody Good Time",
        2450,
        &["Bloody Good Time"],
        "pm",
        &["pm.exe", "hl2.exe"],
        &["pm.fgd"],
        Branch::BloodyGoodTime,
    ),
    game(
        "left4dead",
        "Left 4 Dead",
        500,
        &["left 4 dead", "Left 4 Dead"],
        "left4dead",
        &["left4dead.exe"],
        &["left4dead.fgd"],
        Branch::Left4Dead,
    ),
    KnownGame {
        map_prefix: "c1m1_",
        ..game(
            "left4dead2",
            "Left 4 Dead 2",
            550,
            &["Left 4 Dead 2"],
            "left4dead2",
            &["left4dead2.exe"],
            &["left4dead2.fgd"],
            Branch::Left4Dead2,
        )
    },
    KnownGame {
        lighting: LightingPasses::HdrOnly,
        ..game(
            "swarm",
            "Alien Swarm",
            630,
            &["alien swarm", "Alien Swarm"],
            "swarm",
            &["swarm.exe"],
            &["swarm.fgd"],
            Branch::AlienSwarm,
        )
    },
    KnownGame {
        lighting: LightingPasses::HdrOnly,
        ..game(
            "nucleardawn",
            "Nuclear Dawn",
            17_710,
            &["nucleardawn", "Nuclear Dawn"],
            "nucleardawn",
            &["nucleardawn.exe"],
            &["nucleardawn.fgd"],
            Branch::AlienSwarm,
        )
    },
    KnownGame {
        lighting: LightingPasses::HdrOnly,
        map_prefix: "sp_",
        ..game(
            "portal2",
            "Portal 2",
            620,
            &["Portal 2"],
            "portal2",
            &["portal2.exe"],
            &["portal2.fgd"],
            Branch::Portal2,
        )
    },
    KnownGame {
        lighting: LightingPasses::HdrOnly,
        map_prefix: "de_",
        ..game(
            "csgo",
            "Counter-Strike: Global Offensive (legacy)",
            4_465_480,
            &["Counter-Strike Global Offensive", "csgo legacy"],
            "csgo",
            &["csgo.exe"],
            &["csgo.fgd"],
            Branch::Csgo,
        )
    },
    KnownGame {
        lighting: LightingPasses::HdrOnly,
        ..game(
            "insurgency",
            "Insurgency",
            222_880,
            &["insurgency2", "Insurgency"],
            "insurgency",
            &["insurgency.exe"],
            &["insurgency.fgd"],
            Branch::Insurgency,
        )
    },
    KnownGame {
        lighting: LightingPasses::HdrOnly,
        ..game(
            "doi",
            "Day of Infamy",
            447_820,
            &["dayofinfamy", "Day of Infamy"],
            "doi",
            &["dayofinfamy.exe"],
            &["doi.fgd"],
            Branch::Insurgency,
        )
    },
    unsupported(
        "cs2",
        "Counter-Strike 2",
        730,
        &["Counter-Strike Global Offensive"],
        "game/csgo",
        "Source 2: its maps are VPK files built by its own tools, not BSP files",
    ),
    unsupported(
        "dota2",
        "Dota 2",
        570,
        &["dota 2 beta"],
        "game/dota",
        "Dota 2 now runs on Source 2; its Source 1 client (BSP versions 22 and 23) is gone",
    ),
    unsupported(
        "contagion",
        "Contagion",
        238_430,
        &["Contagion"],
        "contagion",
        "BSP version 27, a format of its own that is not documented",
    ),
    unsupported(
        "mm",
        "Dark Messiah of Might and Magic",
        2100,
        &["Dark Messiah Might and Magic Single Player", "Dark Messiah Might and Magic Multi-Player"],
        "mm",
        "its texture infos, brush models and static props have fields of its own that are not documented",
    ),
    unsupported(
        "vampire",
        "Vampire: The Masquerade - Bloodlines",
        2600,
        &["Vampire The Masquerade - Bloodlines"],
        "vampire",
        "BSP version 17 (2004), before every format this compiler writes",
    ),
    unsupported(
        "dearesther",
        "Dear Esther",
        203_810,
        &["Dear Esther"],
        "dearesther",
        "static prop records of its own (not documented)",
    ),
    unsupported(
        "zenoclash",
        "Zeno Clash",
        22_200,
        &["Zeno Clash"],
        "zenoclash",
        "a static prop field of its own (not documented)",
    ),
    unsupported(
        "vindictus",
        "Vindictus",
        212_160,
        &["Vindictus"],
        "vindictus",
        "an online game with a BSP format of its own; maps cannot be added",
    ),
    unsupported(
        "sin",
        "SiN Episodes: Emergence",
        1300,
        &["SiN Episodes Emergence"],
        "sin",
        "Source 2006: its lighting lumps predate the formats this compiler writes",
    ),
    unsupported(
        "sdk2006",
        "Source SDK Base 2006",
        SDK2006_APP_ID,
        &["Source SDK Base"],
        "hl2",
        "Source 2006: its lighting lumps predate the formats this compiler writes",
    ),
];

pub fn known_game(id: &str) -> Option<&'static KnownGame> {
    KNOWN_GAMES.iter().find(|g| g.id.eq_ignore_ascii_case(id.trim()))
}

pub fn known_game_of_app(app_id: u32) -> Option<&'static KnownGame> {
    KNOWN_GAMES.iter().find(|g| g.app_id == app_id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameProfile {
    pub id: String,
    pub name: String,
    pub app_id: u32,
    pub root: PathBuf,
    pub game_dir: PathBuf,
    pub branch: Option<Branch>,
    pub unsupported: Option<String>,
    pub exe: Option<PathBuf>,
    pub fgds: Vec<PathBuf>,
    pub fgd_dirs: Vec<PathBuf>,
    pub source_mod: bool,
    pub mapbase: bool,
    pub installed: bool,
    pub lighting: LightingPasses,
    pub skybox: String,
}

impl GameProfile {
    pub fn is_gmod(&self) -> bool {
        self.id == GMOD_ID
    }

    pub fn is_supported(&self) -> bool {
        self.branch.is_some()
    }

    pub fn format(&self) -> hp_bsp::BspFormat {
        self.branch.unwrap_or_default().format()
    }

    pub fn maps_dir(&self) -> PathBuf {
        self.game_dir.join("maps")
    }

    pub fn game_arg(&self) -> String {
        if self.source_mod || self.game_dir.parent().is_none_or(|p| path_key(p) != path_key(&self.root)) {
            self.game_dir.display().to_string()
        } else {
            self.game_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        }
    }

    pub fn launch_args(&self, map: Option<&str>) -> Vec<String> {
        let mut args = vec!["-steam".to_owned(), "-game".to_owned(), self.game_arg()];
        if let Some(m) = map.filter(|m| !m.is_empty()) {
            args.push("+map".to_owned());
            args.push(m.to_owned());
        }
        args
    }

    pub fn not_installed(g: &KnownGame) -> GameProfile {
        GameProfile {
            id: g.id.to_owned(),
            name: g.name.to_owned(),
            app_id: g.app_id,
            root: PathBuf::new(),
            game_dir: PathBuf::new(),
            branch: g.branch,
            unsupported: g.unsupported.map(str::to_owned),
            exe: None,
            fgds: Vec::new(),
            fgd_dirs: Vec::new(),
            source_mod: false,
            mapbase: false,
            installed: false,
            lighting: g.lighting,
            skybox: g.skybox.to_owned(),
        }
    }

    pub fn installed_in(g: &KnownGame, root: &Path) -> Option<GameProfile> {
        let game_dir = root.join(g.game_folder);
        let gameinfo = read_gameinfo(&game_dir)?;
        let mut p =
            GameProfile { root: root.to_path_buf(), game_dir, installed: true, ..GameProfile::not_installed(g) };
        p.exe = g.exes.iter().map(|e| root.join(e)).find(|e| e.is_file());
        p.mapbase = gameinfo.mapbase;
        let names: Vec<String> = match gameinfo.game_data {
            Some(d) => vec![d],
            None => g.fgds.iter().map(|s| (*s).to_owned()).collect(),
        };
        p.fill_fgds(&names, &[]);
        Some(p)
    }

    pub fn from_game_dir(game_dir: &Path, libraries: &[PathBuf]) -> Option<GameProfile> {
        let gameinfo = read_gameinfo(game_dir)?;
        let folder = game_dir.file_name()?.to_string_lossy().into_owned();
        let parent = game_dir.parent()?.to_path_buf();
        let in_sourcemods = parent.file_name().is_some_and(|n| n.eq_ignore_ascii_case("sourcemods"));
        if !in_sourcemods {
            let install = parent.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let known = KNOWN_GAMES.iter().filter(|g| g.game_folder.eq_ignore_ascii_case(&folder)).find(|g| {
                g.install_dirs.iter().any(|d| d.eq_ignore_ascii_case(&install)) || g.app_id == gameinfo.app_id
            });
            if let Some(g) = known {
                return GameProfile::installed_in(g, &parent);
            }
        }
        let base = known_game_of_app(gameinfo.app_id);
        let base_root = if in_sourcemods {
            base.and_then(|b| find_install(b, libraries)).unwrap_or_default()
        } else {
            parent.clone()
        };
        let branch = match base {
            Some(b) => b.branch,
            None => Some(Branch::Source2013),
        };
        let unsupported =
            base.and_then(|b| b.unsupported).map(|r| format!("runs on {}: {r}", base.map_or("", |b| b.name)));
        let id = if in_sourcemods { format!("mod:{folder}") } else { format!("dir:{}", game_dir.display()) };
        let name = gameinfo.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| folder.clone());
        let mut p = GameProfile {
            id,
            name,
            app_id: gameinfo.app_id,
            root: base_root.clone(),
            game_dir: game_dir.to_path_buf(),
            branch,
            unsupported,
            exe: None,
            fgds: Vec::new(),
            fgd_dirs: Vec::new(),
            source_mod: in_sourcemods,
            mapbase: gameinfo.mapbase,
            installed: true,
            lighting: base.map_or(LightingPasses::Both, |b| b.lighting),
            skybox: base.map_or("sky_day01_01", |b| b.skybox).to_owned(),
        };
        let exes: &[&str] = base.map_or(&["hl2_win64.exe", "hl2.exe"], |b| b.exes);
        p.exe = exes.iter().map(|e| base_root.join(e)).find(|e| !base_root.as_os_str().is_empty() && e.is_file());
        let names: Vec<String> = match gameinfo.game_data {
            Some(d) => vec![d],
            None => base.map(|b| b.fgds.iter().map(|s| (*s).to_owned()).collect()).unwrap_or_default(),
        };
        let extra: Vec<PathBuf> = if gameinfo.mapbase { vec![parent.join("mapbase_shared")] } else { Vec::new() };
        p.fill_fgds(&names, &extra);
        Some(p)
    }

    fn fill_fgds(&mut self, names: &[String], extra: &[PathBuf]) {
        let mut dirs = vec![self.game_dir.join("bin"), self.game_dir.clone()];
        if !self.root.as_os_str().is_empty() {
            dirs.push(self.root.join("bin"));
            dirs.push(self.root.join("bin").join("win64"));
        }
        dirs.extend(extra.iter().cloned());
        for name in names {
            if let Some(f) = dirs.iter().map(|d| d.join(name)).find(|f| f.is_file())
                && !self.fgds.contains(&f)
            {
                self.fgds.push(f);
            }
        }
        self.fgd_dirs = dirs.into_iter().filter(|d| d.is_dir()).collect();
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GameInfo {
    pub(crate) title: Option<String>,
    pub(crate) app_id: u32,
    pub(crate) game_data: Option<String>,
    pub(crate) mapbase: bool,
    pub(crate) text: String,
}

pub(crate) fn read_gameinfo(game_dir: &Path) -> Option<GameInfo> {
    let bytes = std::fs::read(game_dir.join("gameinfo.txt")).ok()?;
    Some(parse_gameinfo(&String::from_utf8_lossy(&bytes)))
}

pub(crate) fn parse_gameinfo(text: &str) -> GameInfo {
    let kv = hp_kv::parse(text).ok();
    let gi = kv.as_ref().and_then(|k| k.child("GameInfo"));
    let get = |k: &str| gi.and_then(|g| g.get(k)).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    let app_id = gi
        .and_then(|g| g.child("FileSystem"))
        .and_then(|f| f.get("SteamAppId"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    GameInfo {
        title: get("game"),
        app_id,
        game_data: get("GameData"),
        mapbase: text.to_ascii_lowercase().contains("mapbase"),
        text: text.to_owned(),
    }
}

fn find_install(g: &KnownGame, libraries: &[PathBuf]) -> Option<PathBuf> {
    for lib in libraries {
        let steamapps = lib.join("steamapps");
        let mut candidates = Vec::new();
        if let Ok(bytes) = std::fs::read(steamapps.join(format!("appmanifest_{}.acf", g.app_id)))
            && let Ok(kv) = hp_kv::parse(&String::from_utf8_lossy(&bytes))
            && let Some(dir) = kv.child("AppState").and_then(|s| s.get("installdir"))
            && !dir.trim().is_empty()
        {
            candidates.push(steamapps.join("common").join(dir.trim()));
        }
        candidates.extend(g.install_dirs.iter().map(|d| steamapps.join("common").join(d)));
        if let Some(root) = candidates.into_iter().find(|c| c.join(g.game_folder).join("gameinfo.txt").is_file()) {
            return Some(root);
        }
    }
    None
}

fn libraries() -> Vec<PathBuf> {
    crate::detect::detect_steam_libraries()
}

pub fn detect_games_in(libraries: &[PathBuf]) -> Vec<GameProfile> {
    let mut out: Vec<GameProfile> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for g in KNOWN_GAMES.iter().filter(|g| g.branch.is_some()) {
        let Some(root) = find_install(g, libraries) else { continue };
        let Some(p) = GameProfile::installed_in(g, &root) else { continue };
        let key = path_key(&p.game_dir);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(p);
    }
    for lib in libraries {
        let mods = lib.join("steamapps").join("sourcemods");
        let Ok(read) = std::fs::read_dir(&mods) else { continue };
        let mut dirs: Vec<PathBuf> =
            read.flatten().map(|e| e.path()).filter(|p| p.join("gameinfo.txt").is_file()).collect();
        dirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()));
        for d in dirs {
            if let Some(p) = GameProfile::from_game_dir(&d, libraries)
                && p.is_supported()
                && !seen.contains(&path_key(&p.game_dir))
            {
                seen.push(path_key(&p.game_dir));
                out.push(p);
            }
        }
    }
    out
}

pub fn detect_games() -> Vec<GameProfile> {
    detect_games_in(&libraries())
}

pub fn game_catalogue_in(libraries: &[PathBuf]) -> Vec<GameProfile> {
    let mut out = detect_games_in(libraries);
    let missing: Vec<GameProfile> = KNOWN_GAMES
        .iter()
        .filter(|g| g.branch.is_some() && !out.iter().any(|p| p.id == g.id))
        .map(GameProfile::not_installed)
        .collect();
    out.extend(missing);
    out
}

pub fn game_catalogue() -> Vec<GameProfile> {
    let mut cache = CATALOGUE.lock();
    if let Some(list) = cache.as_ref() {
        return list.clone();
    }
    let list = game_catalogue_in(&libraries());
    *cache = Some(list.clone());
    list
}

pub fn refresh_games() {
    *CATALOGUE.lock() = None;
    crate::apps::refresh_steam_apps();
}

static CATALOGUE: parking_lot::Mutex<Option<Vec<GameProfile>>> = parking_lot::Mutex::new(None);

pub fn resolve_game(id: &str, game_dir: Option<&Path>) -> Option<GameProfile> {
    if let Some(dir) = game_dir.filter(|d| !d.as_os_str().is_empty()) {
        let dir = game_folder_of(dir, id);
        if let Some(mut p) = GameProfile::from_game_dir(&dir, &libraries()) {
            if known_game(id).is_some() {
                p.id = id.to_owned();
            }
            return Some(p);
        }
    }
    let list = game_catalogue();
    list.into_iter()
        .find(|p| p.id.eq_ignore_ascii_case(id.trim()))
        .or_else(|| known_game(id).map(GameProfile::not_installed))
}

pub fn game_folder_of(dir: &Path, id: &str) -> PathBuf {
    if dir.join("gameinfo.txt").is_file() {
        return dir.to_path_buf();
    }
    if let Some(g) = known_game(id) {
        let sub = dir.join(g.game_folder);
        if sub.join("gameinfo.txt").is_file() {
            return sub;
        }
    }
    dir.to_path_buf()
}

pub fn libraries_of_steam_root(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    if let Ok(bytes) = std::fs::read(root.join("steamapps").join("libraryfolders.vdf")) {
        for LibraryFolder { path, .. } in parse_library_folders(&String::from_utf8_lossy(&bytes)) {
            if !out.iter().any(|p| path_key(p) == path_key(&path)) {
                out.push(path);
            }
        }
    }
    out.retain(|p| p.join("steamapps").is_dir());
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GamePath {
    Vpk(PathBuf),
    Dir(PathBuf),
    Wildcard(PathBuf),
    VpkGlob(PathBuf),
}

pub(crate) fn game_paths(
    gameinfo: &str,
    base: &Path,
    game_dir: &Path,
    app_root: &dyn Fn(u32) -> Option<PathBuf>,
) -> Vec<GamePath> {
    use crate::mount::strip_prefix_ci;
    let Ok(kv) = hp_kv::parse(gameinfo) else { return Vec::new() };
    let Some(paths) = kv.child("GameInfo").and_then(|g| g.child("FileSystem")).and_then(|f| f.child("SearchPaths"))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, value) in &paths.props {
        if !key.split('+').any(|t| t.trim().eq_ignore_ascii_case("game")) {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let (root, rest): (PathBuf, &str) = if let Some(r) = strip_prefix_ci(value, "|all_source_engine_paths|") {
            (base.to_path_buf(), r)
        } else if let Some(r) = strip_prefix_ci(value, "|gameinfo_path|") {
            (game_dir.to_path_buf(), r)
        } else if let Some(r) = strip_prefix_ci(value, "|appid_") {
            let Some((id, r)) = r.split_once('|') else { continue };
            let Some(root) = id.trim().parse().ok().and_then(app_root) else { continue };
            (root, r)
        } else if Path::new(value).is_absolute() {
            (PathBuf::new(), value)
        } else {
            (base.to_path_buf(), value)
        };
        let rest = rest.trim_start_matches(['/', '\\']);
        let join = |r: &str| {
            let r = r.trim_end_matches(['/', '\\']);
            if r.is_empty() || r == "." {
                root.clone()
            } else if root.as_os_str().is_empty() {
                PathBuf::from(r)
            } else {
                root.join(r)
            }
        };
        let lower = rest.to_ascii_lowercase();
        out.push(if lower.ends_with("*.vpk") {
            GamePath::VpkGlob(join(&rest[..rest.len() - 5]))
        } else if let Some(dir) = rest.strip_suffix('*') {
            GamePath::Wildcard(join(dir))
        } else if lower.ends_with(".vpk") {
            GamePath::Vpk(join(rest))
        } else {
            GamePath::Dir(join(rest))
        });
    }
    out
}

fn mounts_paks_and_dlc(branch: Option<Branch>) -> bool {
    matches!(
        branch,
        Some(
            Branch::Left4Dead
                | Branch::Left4Dead2
                | Branch::AlienSwarm
                | Branch::Portal2
                | Branch::Csgo
                | Branch::Insurgency
        )
    )
}

fn dlc_folders(game_dir: &Path) -> Vec<PathBuf> {
    let Some(name) = game_dir.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()) else { return Vec::new() };
    let Some(parent) = game_dir.parent() else { return Vec::new() };
    let prefix = format!("{name}_dlc");
    let mut found: Vec<(u32, PathBuf)> = crate::mount::list_dir(parent)
        .into_iter()
        .filter(|(_, _, is_dir)| *is_dir)
        .filter_map(|(n, p, _)| {
            n.to_ascii_lowercase().strip_prefix(&prefix).and_then(|k| k.parse().ok()).map(|k| (k, p))
        })
        .collect();
    found.sort_by_key(|(k, _)| std::cmp::Reverse(*k));
    found.into_iter().map(|(_, p)| p).collect()
}

pub(crate) fn plan_game(
    profile: &GameProfile,
    opts: &crate::MountOptions,
) -> Result<(Vec<crate::mount::PlanItem>, Vec<String>), crate::FsError> {
    use crate::mount::{PlanItem, PlanKind, folder_name, game_folder_items, has_ext, list_dir, resolve_vpk, vpk_label};
    use crate::source::SourceOrigin;
    let game_dir = &profile.game_dir;
    let Some(info) = read_gameinfo(game_dir) else {
        return Err(crate::FsError::NotAGameDir(game_dir.clone()));
    };
    let base = if profile.root.as_os_str().is_empty() {
        game_dir.parent().map(Path::to_path_buf).unwrap_or_default()
    } else {
        profile.root.clone()
    };
    let uses_appid = info.text.to_ascii_lowercase().contains("|appid_");
    let libs = if uses_appid { libraries() } else { Vec::new() };
    let app_root = |id: u32| known_game_of_app(id).and_then(|g| find_install(g, &libs));
    let mut paths = game_paths(&info.text, &base, game_dir, &app_root);
    if paths.is_empty() {
        paths.push(GamePath::Dir(game_dir.clone()));
    }
    let paks = mounts_paks_and_dlc(profile.branch);
    let game_key = path_key(game_dir);
    let mut warnings = Vec::new();
    let mut items: Vec<PlanItem> = opts.custom_dirs.iter().map(PlanItem::custom).collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut push = |item: PlanItem, items: &mut Vec<PlanItem>| {
        if seen.insert(path_key(&item.path)) {
            items.push(item);
        }
    };
    let mut dlc_done = false;
    for sp in paths {
        match sp {
            GamePath::Vpk(p) => match resolve_vpk(&p) {
                Some(dir) => {
                    let label = vpk_label(&dir);
                    push(PlanItem::new(PlanKind::Vpk, dir, label, SourceOrigin::BaseGame), &mut items);
                }
                None => warnings.push(format!("VPK listed in gameinfo.txt not found: {}", p.display())),
            },
            GamePath::Dir(p) => {
                if !p.is_dir() {
                    continue;
                }
                let is_game = path_key(&p) == game_key;
                if paks && !dlc_done && is_game {
                    dlc_done = true;
                    for dlc in dlc_folders(&p) {
                        let pak = dlc.join("pak01_dir.vpk");
                        if pak.is_file() {
                            let label = format!("{} ({})", folder_name(&dlc), vpk_label(&pak));
                            push(PlanItem::new(PlanKind::Vpk, pak, label, SourceOrigin::BaseGame), &mut items);
                        }
                        let label = folder_name(&dlc);
                        push(PlanItem::new(PlanKind::GameDir, dlc, label, SourceOrigin::BaseGame), &mut items);
                    }
                }
                if paks {
                    let pak = p.join("pak01_dir.vpk");
                    if pak.is_file() {
                        let label = format!("{} ({})", folder_name(&p), vpk_label(&pak));
                        push(PlanItem::new(PlanKind::Vpk, pak, label, SourceOrigin::BaseGame), &mut items);
                    }
                }
                let label = folder_name(&p);
                let origin = if is_game { SourceOrigin::GameLoose } else { SourceOrigin::BaseGame };
                push(PlanItem::new(PlanKind::GameDir, p, label, origin), &mut items);
            }
            GamePath::Wildcard(dir) => {
                for (name, p, is_dir) in list_dir(&dir) {
                    if is_dir {
                        let label = format!("{}: {name}", folder_name(&dir));
                        push(PlanItem::new(PlanKind::GameDir, p, label, SourceOrigin::Addon), &mut items);
                    } else if has_ext(&name, "vpk") && is_mountable_vpk(&name) {
                        let label = format!("{}: {}", folder_name(&dir), vpk_label(&p));
                        push(PlanItem::new(PlanKind::Vpk, p, label, SourceOrigin::Addon), &mut items);
                    }
                }
            }
            GamePath::VpkGlob(dir) => {
                for (name, p, is_dir) in list_dir(&dir) {
                    if !is_dir && has_ext(&name, "vpk") && is_mountable_vpk(&name) {
                        let label = vpk_label(&p);
                        push(PlanItem::new(PlanKind::Vpk, p, label, SourceOrigin::BaseGame), &mut items);
                    }
                }
            }
        }
    }
    for path in &opts.extra_game_dirs {
        let label = format!("Extra: {}", folder_name(path));
        for item in game_folder_items(path, SourceOrigin::Extra, &label, &mut warnings) {
            push(item, &mut items);
        }
    }
    for item in crate::apps::app_items(opts, &mut warnings) {
        push(item, &mut items);
    }
    Ok((items, warnings))
}

fn is_mountable_vpk(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".vpk").unwrap_or(&lower);
    if stem.ends_with("_dir") {
        return true;
    }
    !stem.rsplit_once('_').is_some_and(|(_, n)| n.len() == 3 && n.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    fn gameinfo(title: &str, app: u32, extra: &str) -> String {
        format!(
            "\"GameInfo\"\n{{\n\tgame \"{title}\"\n\t{extra}\n\tFileSystem\n\t{{\n\t\tSteamAppId {app}\n\t\tSearchPaths\n\t\t{{\n\t\t\tgame+mod |gameinfo_path|.\n\t\t}}\n\t}}\n}}\n"
        )
    }

    fn steam() -> TempDir {
        let t = TempDir::new("steam-games");
        let lib1 = "lib1/steamapps";
        t.write(
            &format!("{lib1}/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}",
                t.path().join("lib1").display().to_string().replace('\\', "\\\\"),
                t.path().join("lib2").display().to_string().replace('\\', "\\\\")
            )
            .as_bytes(),
        );
        t.write(
            &format!("{lib1}/appmanifest_440.acf"),
            b"\"AppState\" { \"appid\" \"440\" \"installdir\" \"Team Fortress 2\" }",
        );
        t.write(
            &format!("{lib1}/common/Team Fortress 2/tf/gameinfo.txt"),
            gameinfo("Team Fortress 2", 440, "GameData \"tf.fgd\"").as_bytes(),
        );
        t.write(&format!("{lib1}/common/Team Fortress 2/tf_win64.exe"), b"MZ");
        t.write(&format!("{lib1}/common/Team Fortress 2/bin/tf.fgd"), b"@include \"base.fgd\"");
        t.write(&format!("{lib1}/common/Team Fortress 2/bin/base.fgd"), b"");
        t.write(
            &format!("{lib1}/common/Portal 2/portal2/gameinfo.txt"),
            gameinfo("PORTAL 2", 620, "GameData \"portal2.fgd\"").as_bytes(),
        );
        t.write(&format!("{lib1}/common/Portal 2/portal2.exe"), b"MZ");
        t.write(&format!("{lib1}/common/Portal 2/bin/portal2.fgd"), b"");
        let lib2 = "lib2/steamapps";
        t.write(
            &format!("{lib2}/common/Counter-Strike Source/cstrike/gameinfo.txt"),
            gameinfo("Counter-Strike Source", 240, "").as_bytes(),
        );
        t.write(&format!("{lib2}/common/Counter-Strike Source/bin/cstrike.fgd"), b"");
        t.write(&format!("{lib2}/common/Half-Life 2/hl2/gameinfo.txt"), gameinfo("Half-Life 2", 220, "").as_bytes());
        t.write(
            &format!("{lib2}/common/Half-Life 2/ep2/gameinfo.txt"),
            gameinfo("Half-Life 2: Episode Two", 220, "").as_bytes(),
        );
        t.write(&format!("{lib2}/common/Half-Life 2/hl2.exe"), b"MZ");
        t.write(&format!("{lib2}/common/Counter-Strike Global Offensive/game/csgo/gameinfo.gi"), b"");
        t.write(
            &format!("{lib2}/common/Source SDK Base 2013 Singleplayer/hl2/gameinfo.txt"),
            gameinfo("Source SDK Base 2013", 243730, "").as_bytes(),
        );
        t.write(&format!("{lib2}/common/Source SDK Base 2013 Singleplayer/hl2.exe"), b"MZ");
        t.write(&format!("{lib2}/common/Source SDK Base 2013 Singleplayer/bin/halflife2.fgd"), b"");
        t.write(
            &format!("{lib1}/sourcemods/mymod/gameinfo.txt"),
            gameinfo("My Mapbase Mod", 243730, "// mapbase\nGameData \"mymod.fgd\"").as_bytes(),
        );
        t.write(&format!("{lib1}/sourcemods/mymod/mymod.fgd"), b"");
        t.write(&format!("{lib1}/sourcemods/notamod/readme.txt"), b"");
        t.write(&format!("{lib1}/sourcemods/oldmod/gameinfo.txt"), gameinfo("Old Mod", SDK2006_APP_ID, "").as_bytes());
        t
    }

    #[test]
    fn games_are_found_in_steam_libraries() {
        let t = steam();
        let libs = libraries_of_steam_root(&t.path().join("lib1"));
        assert_eq!(libs.len(), 2);
        let games = detect_games_in(&libs);
        let ids: Vec<&str> = games.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, ["hl2", "ep2", "sdk2013sp", "cstrike", "tf", "portal2", "mod:mymod"]);
        let tf = &games[4];
        assert_eq!(tf.branch, Some(Branch::Source2013));
        assert!(tf.exe.as_ref().is_some_and(|e| e.ends_with("tf_win64.exe")));
        assert_eq!(tf.fgds.len(), 1);
        assert!(tf.fgds[0].ends_with("bin/tf.fgd") || tf.fgds[0].ends_with("bin\\tf.fgd"));
        assert_eq!(tf.game_arg(), "tf");
        assert_eq!(tf.launch_args(Some("ctf_test")), ["-steam", "-game", "tf", "+map", "ctf_test"]);
        assert!(games[3].fgds[0].ends_with("cstrike.fgd"));
        let p2 = &games[5];
        assert_eq!((p2.branch, p2.lighting), (Some(Branch::Portal2), LightingPasses::HdrOnly));
        let m = &games[6];
        assert!(m.source_mod && m.mapbase && m.installed);
        assert_eq!(m.name, "My Mapbase Mod");
        assert_eq!(m.branch, Some(Branch::Source2013));
        assert!(m.root.ends_with("Source SDK Base 2013 Singleplayer"));
        assert!(m.exe.as_ref().is_some_and(|e| e.ends_with("hl2.exe")));
        assert!(m.fgds[0].ends_with("mymod.fgd"));
        assert_eq!(m.game_arg(), m.game_dir.display().to_string());
    }

    #[test]
    fn the_catalogue_lists_missing_games_after_the_detected_ones() {
        let t = steam();
        let libs = libraries_of_steam_root(&t.path().join("lib1"));
        let all = game_catalogue_in(&libs);
        let detected = detect_games_in(&libs).len();
        assert!(all[..detected].iter().all(|g| g.installed));
        assert!(all[detected..].iter().all(|g| !g.installed));
        let gmod = all.iter().find(|g| g.is_gmod()).expect("Garry's Mod is always offered");
        assert_eq!(gmod.branch, Some(Branch::GarrysMod));
        let l4d2 = all.iter().find(|g| g.id == "left4dead2").expect("listed");
        assert_eq!(l4d2.format().version, 21);
        assert!(all.iter().all(GameProfile::is_supported));
        assert!(!all.iter().any(|g| g.id == "cs2" || g.id == "contagion"));
    }

    #[test]
    fn known_games_are_consistent() {
        for (i, g) in KNOWN_GAMES.iter().enumerate() {
            assert!(KNOWN_GAMES[i + 1..].iter().all(|o| o.id != g.id), "{} twice", g.id);
            assert_eq!(g.branch.is_none(), g.unsupported.is_some(), "{}", g.id);
            assert!(!g.install_dirs.is_empty());
            if g.branch.is_some() {
                assert!(!g.exes.is_empty() && !g.fgds.is_empty(), "{}", g.id);
            }
        }
        assert_eq!(known_game("TF").map(|g| g.app_id), Some(440));
        assert_eq!(known_game_of_app(4000).map(|g| g.id), Some(GMOD_ID));
        assert_eq!(KNOWN_GAMES[0].id, GMOD_ID);
    }

    #[test]
    fn a_folder_picked_by_the_user_is_read_as_its_game() {
        let t = steam();
        let tf_root = t.path().join("lib1/steamapps/common/Team Fortress 2");
        assert_eq!(game_folder_of(&tf_root, "tf"), tf_root.join("tf"));
        let p = GameProfile::from_game_dir(&tf_root.join("tf"), &[]).expect("a game");
        assert_eq!(p.id, "tf");
        let other = t.path().join("lib1/steamapps/common/Other/othergame");
        t.write("lib1/steamapps/common/Other/othergame/gameinfo.txt", gameinfo("Other", 243750, "").as_bytes());
        let p = GameProfile::from_game_dir(&other, &[]).expect("a game");
        assert!(p.id.starts_with("dir:"));
        assert_eq!(p.branch, Some(Branch::Source2013));
        t.write("lib1/steamapps/sourcemods/old/gameinfo.txt", gameinfo("Old", 215, "").as_bytes());
        let p = GameProfile::from_game_dir(&t.path().join("lib1/steamapps/sourcemods/old"), &[]).expect("a mod");
        assert!(!p.is_supported());
        assert!(p.unsupported.as_deref().unwrap_or("").contains("Source 2006"));
    }

    #[test]
    fn gameinfo_keys() {
        let g = parse_gameinfo(&gameinfo("X", 440, "GameData \"tf.fgd\""));
        assert_eq!(
            (g.title.as_deref(), g.app_id, g.game_data.as_deref(), g.mapbase),
            (Some("X"), 440, Some("tf.fgd"), false)
        );
        assert_eq!(parse_gameinfo("garbage").app_id, 0);
    }
    #[test]
    fn search_paths_resolve_like_the_engine() {
        let base = Path::new("R");
        let game = base.join("tf");
        let gi = "GameInfo { FileSystem { SteamAppId 440 SearchPaths {
            game+mod tf/tf2_misc.vpk
            game+mod tf/custom/*
            game |all_source_engine_paths|hl2/hl2_textures.vpk
            game+mod+game_write |gameinfo_path|.
            game |appid_220|hl2
            game |appid_999|nothing
            game content/*.vpk
            platform |all_source_engine_paths|platform
            game_lv tf/tf_lv.vpk
        } } }";
        let app = |id: u32| (id == 220).then(|| PathBuf::from("HL2"));
        assert_eq!(
            game_paths(gi, base, &game, &app),
            vec![
                GamePath::Vpk(base.join("tf/tf2_misc.vpk")),
                GamePath::Wildcard(base.join("tf/custom")),
                GamePath::Vpk(base.join("hl2/hl2_textures.vpk")),
                GamePath::Dir(game.clone()),
                GamePath::Dir(PathBuf::from("HL2").join("hl2")),
                GamePath::VpkGlob(base.join("content")),
            ]
        );
        assert!(is_mountable_vpk("pak01_dir.vpk") && is_mountable_vpk("single.vpk") && is_mountable_vpk("my_mod.vpk"));
        assert!(!is_mountable_vpk("pak01_000.vpk"));
    }

    fn labels(fs: &crate::GameFs) -> Vec<String> {
        fs.sources().into_iter().map(|s| s.label).collect()
    }

    #[test]
    fn a_source_2013_game_mounts_its_gameinfo_in_order() {
        let t = TempDir::new("game-mount-tf");
        let root = t.path().join("Team Fortress 2");
        let gi = "\"GameInfo\" { game \"Team Fortress 2\" FileSystem { SteamAppId 440 SearchPaths {
            game+mod tf/tf2_misc.vpk
            game+mod tf/custom/*
            game+mod+game_write |gameinfo_path|.
            game |all_source_engine_paths|hl2
        } } }";
        t.write("Team Fortress 2/tf/gameinfo.txt", gi.as_bytes());
        t.write("Team Fortress 2/tf/materials/tf/a.vmt", b"LightmappedGeneric { }");
        t.write("Team Fortress 2/tf/custom/mymod/materials/tf/a.vmt", b"UnlitGeneric { }");
        t.write("Team Fortress 2/hl2/materials/hl2/b.vmt", b"LightmappedGeneric { }");
        t.write("Team Fortress 2/hl2/pak01_dir.vpk", b"not a vpk");
        let g = known_game("tf").unwrap();
        let p = GameProfile::installed_in(g, &root).unwrap();
        let fs = crate::GameFs::for_game(&p, &crate::MountOptions::default()).expect("mounted");
        assert_eq!(labels(&fs), ["custom: mymod", "tf", "hl2"]);
        assert!(fs.warnings().iter().any(|w| w.contains("tf2_misc.vpk")), "{:?}", fs.warnings());
        assert_eq!(fs.read_text("materials/tf/a.vmt").as_deref(), Some("UnlitGeneric { }"), "custom wins");
        assert!(fs.exists("materials/hl2/b.vmt"));
        assert_eq!(fs.root(), Some(root.as_path()));
        assert_eq!(fs.game_dir(), Some(root.join("tf").as_path()));
    }

    #[test]
    fn portal2_mounts_its_dlc_first() {
        let t = TempDir::new("game-mount-p2");
        let root = t.path().join("Portal 2");
        t.write(
            "Portal 2/portal2/gameinfo.txt",
            b"\"GameInfo\" { FileSystem { SteamAppId 620 SearchPaths { Game |gameinfo_path|. } } }",
        );
        t.write("Portal 2/portal2/materials/a.vmt", b"base");
        t.mkdir("Portal 2/portal2_dlc1/materials");
        t.write("Portal 2/portal2_dlc2/materials/a.vmt", b"dlc2");
        t.write("Portal 2/portal2_french/materials/a.vmt", b"not mounted");
        let p = GameProfile::installed_in(known_game("portal2").unwrap(), &root).unwrap();
        let fs = crate::GameFs::for_game(&p, &crate::MountOptions::default()).expect("mounted");
        assert_eq!(labels(&fs), ["portal2_dlc2", "portal2_dlc1", "portal2"]);
        assert_eq!(fs.read_text("materials/a.vmt").as_deref(), Some("dlc2"));
    }

    #[test]
    fn garrys_mod_keeps_its_own_mount() {
        let t = TempDir::new("game-mount-gmod");
        t.write(
            "GarrysMod/garrysmod/gameinfo.txt",
            b"\"GameInfo\" { FileSystem { SearchPaths { game+mod garrysmod/addons/* game |gameinfo_path|. } } }",
        );
        t.write("GarrysMod/garrysmod/addons/a/materials/x.vmt", b"x");
        let p = GameProfile::installed_in(known_game(GMOD_ID).unwrap(), &t.path().join("GarrysMod")).unwrap();
        let opts = crate::MountOptions { mount_workshop: false, ..crate::MountOptions::default() };
        let fs = crate::GameFs::for_game(&p, &opts).unwrap();
        let plain = crate::GameFs::for_gmod(&t.path().join("GarrysMod"), &opts).unwrap();
        assert_eq!(labels(&fs), labels(&plain));
        assert!(labels(&fs).contains(&"Addon: a".to_owned()));
    }
}
