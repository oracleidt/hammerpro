use std::path::{Path, PathBuf};

use hp_bspc::materials::{MaterialFacts, MaterialOptions, facts_by_name, facts_from_vmt_with};

pub const BSPC_ARG: &str = "--bspc";

pub const RAD_ARG: &str = "--rad";

pub(crate) struct GameMaterials {
    pub(crate) fs: hp_fs::GameFs,
    pub(crate) opts: MaterialOptions,
}

impl hp_bspc::MaterialSource for GameMaterials {
    fn facts(&self, name: &str) -> MaterialFacts {
        let Some(mat) = self.fs.material(name) else {
            let mut f = facts_by_name(name);
            f.found = false;
            f.has_surfaceprop = true;
            let mounted = !self.fs.sources().is_empty();
            if mounted && f.surf == 0 && f.contents == hp_bspc::materials::contents::SOLID {
                f.surf = if self.opts.light_if_missing { 0 } else { hp_bspc::materials::surf::NOLIGHT };
                f.width = 0;
                f.height = 0;
                f.reflectivity = [0.0; 3];
            }
            return f;
        };
        let mat = mat.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
        let base = mat.base_texture();
        let texture = base
            .clone()
            .or_else(|| ["$envmapmask", "$bumpmap", "$dudvmap", "$normalmap"].iter().find_map(|k| mat.texture(k)));
        let info = texture.as_deref().and_then(|t| self.fs.texture_info(t));
        const ERROR_SIZE: (i32, i32) = (128, 128);
        let tool = mat.get("%tooltexture").map(str::trim).filter(|t| !t.is_empty());
        let size = match tool {
            Some(t) => Some(self.fs.texture_info(t).map_or(ERROR_SIZE, |i| (i.width as i32, i.height as i32))),
            None => match &info {
                Some(i) => Some((i.width as i32, i.height as i32)),
                None => texture.is_some().then_some(ERROR_SIZE),
            },
        };
        let base_info = if base.is_some() { info.as_ref() } else { None };
        let base_missing = base.is_some() && base_info.is_none();
        let explicit = mat.get("$reflectivity").and_then(|v| {
            let n: Vec<f32> = v
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .filter_map(|x| x.parse().ok())
                .collect();
            (n.len() == 3).then(|| [n[0], n[1], n[2]])
        });
        let reflectivity = explicit
            .or_else(|| base_info.map(|i| i.reflectivity))
            .or_else(|| base_missing.then_some([0.0; 3]))
            .filter(|r| r.iter().all(|v| v.is_finite()))
            .or(Some([0.2; 3]));
        let key = |k: &str| mat.get(k).map(str::to_owned);
        let mut f = facts_from_vmt_with(&mat.shader, &key, size, reflectivity, &self.opts);
        let by_name = facts_by_name(name);
        if by_name.surf != 0 && f.surf == 0 && f.contents == hp_bspc::materials::contents::SOLID {
            f.contents = by_name.contents;
            f.raw_contents = by_name.raw_contents;
            f.surf = by_name.surf;
        }
        f
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.fs.read(path).or_else(|| loose_file(&self.fs, path))
    }
}

fn loose_file(fs: &hp_fs::GameFs, path: &str) -> Option<Vec<u8>> {
    let rel = Path::new(path.trim_start_matches(['/', '\\']));
    if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return None;
    }
    let sources = fs.sources();
    let dirs = [hp_fs::SourceOrigin::Extra, hp_fs::SourceOrigin::GameLoose]
        .iter()
        .flat_map(|o| sources.iter().filter(move |s| s.origin == *o).map(|s| s.path.clone()))
        .chain(fs.game_dir().map(Path::to_path_buf));
    dirs.filter(|d| d.is_dir()).find_map(|d| std::fs::read(d.join(rel)).ok())
}

pub(crate) struct GameModels<'a> {
    pub(crate) fs: &'a hp_fs::GameFs,
    pub(crate) allow_dynamic: bool,
}

impl hp_bspc::props::ModelSource for GameModels<'_> {
    fn bounds(&self, path: &str) -> Option<(glam::DVec3, glam::DVec3)> {
        let bytes = self.fs.read(path)?;
        let info = hp_mdl::read_header(&bytes).ok()?;
        let (lo, hi) = info.bounds();
        let lo = glam::DVec3::new(f64::from(lo[0]), f64::from(lo[1]), f64::from(lo[2]));
        let hi = glam::DVec3::new(f64::from(hi[0]), f64::from(hi[1]), f64::from(hi[2]));
        (lo.cmplt(hi).all()).then_some((lo, hi))
    }

    fn loads(&self, path: &str) -> Option<bool> {
        (!self.fs.sources().is_empty()).then(|| model_header(self.fs, path).is_some())
    }

    fn is_static_prop(&self, path: &str) -> Option<bool> {
        Some(model_header(self.fs, path).is_some_and(|h| h.is_static_prop()))
    }

    fn static_prop_error(&self, path: &str) -> Option<String> {
        let h = model_header(self.fs, path)?;
        hp_bspc::props::static_prop_error(path, "prop_static", h.is_static_prop(), &h.keyvalues, self.allow_dynamic)
    }

    fn hull_center(&self, path: &str) -> Option<glam::DVec3> {
        model_header(self.fs, path).map(|h| hull_center(&h))
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.fs.read(path)
    }

    fn mesh_points(&self, path: &str) -> Option<Vec<Vec<glam::DVec3>>> {
        let mdl = self.fs.read(path)?;
        let vvd = self.fs.read(&hp_mdl::sibling_paths(path).0)?;
        let meshes = hp_mdl::raw_mesh_positions(&mdl, &vvd).ok()?;
        let out: Vec<Vec<glam::DVec3>> = meshes
            .into_iter()
            .filter(|m| !m.is_empty())
            .map(|m| {
                m.into_iter().map(|p| glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))).collect()
            })
            .collect();
        (!out.is_empty()).then_some(out)
    }

    fn model_check(&self, path: &str) -> Option<hp_bspc::props::ModelCheck> {
        if self.fs.sources().is_empty() {
            return None;
        }
        Some(model_check(self.fs, path))
    }
}

fn model_check(fs: &hp_fs::GameFs, path: &str) -> hp_bspc::props::ModelCheck {
    let mut c = hp_bspc::props::ModelCheck::default();
    let Some(mdl) = fs.read(path) else { return c };
    c.exists = true;
    let Ok(h) = hp_mdl::read_header(&mdl) else { return c };
    c.loads = true;
    c.skins = h.num_skin_families;
    c.bodies = body_combinations(&mdl, h.num_body_parts);
    let i32_at = |d: &[u8], o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (vvd_path, vtx_paths) = hp_mdl::sibling_paths(path);
    if let Some(vvd) = fs.read(&vvd_path) {
        c.has_vvd = true;
        c.vvd_matches = i32_at(&vvd, 8) == Some(h.checksum);
        c.vertices = i32_at(&vvd, 16).map_or(0, |v| v.max(0) as u32);
    }
    if let Some(vtx) = vtx_paths.iter().find_map(|p| fs.read(p)) {
        c.has_vtx = true;
        c.vtx_matches = i32_at(&vtx, 16) == Some(h.checksum);
    }
    if let Some(phy) = fs.read(&hp_mdl::phy_path(path)) {
        c.has_phy = true;
        c.phy_solids = i32_at(&phy, 8).map_or(0, |v| v.max(0) as u32);
        c.phy_matches = i32_at(&phy, 12) == Some(h.checksum);
    }
    c
}

fn body_combinations(mdl: &[u8], parts: u32) -> u32 {
    let i32_at = |o: usize| mdl.get(o..o + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let index = i32_at(236).max(0) as usize;
    (0..parts as usize).fold(1u32, |n, k| n.saturating_mul(i32_at(index + k * 16 + 4).max(1) as u32))
}

fn model_header(fs: &hp_fs::GameFs, path: &str) -> Option<hp_mdl::MdlHeaderInfo> {
    hp_mdl::read_header(&fs.read(path)?).ok()
}

fn hull_center(h: &hp_mdl::MdlHeaderInfo) -> glam::DVec3 {
    let (lo, hi) = (h.hull_min, h.hull_max);
    glam::DVec3::new(f64::from(lo[0] + hi[0]), f64::from(lo[1] + hi[1]), f64::from(lo[2] + hi[2])) * 0.5
}

pub(crate) struct LazyModels {
    gmod: Option<PathBuf>,
    game: Option<PathBuf>,
    profile: Option<hp_fs::GameProfile>,
    mount: hp_fs::MountOptions,
    search_paths: Vec<PathBuf>,
    fs: std::sync::OnceLock<Option<hp_fs::GameFs>>,
}

impl hp_bspc::props::ModelSource for LazyModels {
    fn bounds(&self, _path: &str) -> Option<(glam::DVec3, glam::DVec3)> {
        None
    }

    fn hull_center(&self, path: &str) -> Option<glam::DVec3> {
        model_header(self.game_fs()?, path).map(|h| hull_center(&h))
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.game_fs()?.read(path)
    }
}

impl LazyModels {
    fn game_fs(&self) -> Option<&hp_fs::GameFs> {
        self.fs
            .get_or_init(|| {
                let root = self
                    .gmod
                    .clone()
                    .or_else(|| self.game.as_deref().and_then(gmod_root_of))
                    .or_else(hp_fs::detect_gmod_root);
                let fs = mount_content(self.profile.as_ref(), root.as_deref(), &self.mount)?.ok()?;
                let own = self.profile.as_ref().map(|p| p.game_dir.clone());
                for g in self.game.iter().filter(|g| Some(*g) != own.as_ref()).chain(&self.search_paths) {
                    say(&format!("Mounting search path: {}", g.display()));
                    let _ = fs.add_game_dir(g);
                }
                hp_fs::flush_index();
                Some(fs)
            })
            .as_ref()
    }
}

fn mount_content(
    profile: Option<&hp_fs::GameProfile>,
    root: Option<&Path>,
    options: &hp_fs::MountOptions,
) -> Option<Result<hp_fs::GameFs, hp_fs::FsError>> {
    if profile.is_none() && root.is_none() {
        return None;
    }
    say("Mounting game content...");
    let options = hp_fs::MountOptions { lookup_only: true, ..options.clone() };
    let progress = hp_fs::MountProgress::new();
    let started = std::time::Instant::now();
    let result = std::thread::scope(|scope| {
        let (done, wait) = std::sync::mpsc::channel::<()>();
        let tracking = &progress;
        scope.spawn(move || {
            while matches!(
                wait.recv_timeout(std::time::Duration::from_secs(2)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                let status = tracking.snapshot();
                say(&format!(
                    "Game content: {:?}, {}/{} sources, {:.0} s: {}",
                    status.stage,
                    status.done,
                    status.total,
                    started.elapsed().as_secs_f64(),
                    status.current
                ));
            }
        });
        let staged = match profile {
            Some(profile) => hp_fs::GameFs::for_game_staged_with(profile, &options, &progress),
            None => hp_fs::GameFs::for_gmod_staged_with(root.unwrap_or_else(|| Path::new(".")), &options, &progress),
        };
        let result = staged.map(|(fs, rest)| {
            fs.finish_mount_with(rest, &progress);
            fs
        });
        let _ = done.send(());
        result
    });
    hp_fs::flush_index();
    match &result {
        Ok(fs) => say(&format!(
            "Game content ready: {} sources in {:.3} s",
            fs.sources().len(),
            started.elapsed().as_secs_f64()
        )),
        Err(_) => say(&format!("Game content mount failed after {:.3} s", started.elapsed().as_secs_f64())),
    }
    Some(result)
}

static LOG_FILE: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);

pub(crate) fn say(line: &str) {
    println!("{line}");
    if let Ok(mut log) = LOG_FILE.lock()
        && let Some(f) = log.as_mut()
    {
        use std::io::Write as _;
        let _ = write!(f, "{}\r\n", line.replace('\n', "\r\n"));
    }
}

pub(crate) fn open_log(map: &Path) {
    let path = map.with_extension("log");
    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => {
            if let Ok(mut log) = LOG_FILE.lock() {
                *log = Some(f);
            }
        }
        Err(e) => println!("Can't create LogFile:\"{}\" ({e})", path.display()),
    }
}

fn set_low_priority() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, IDLE_PRIORITY_CLASS, SetPriorityClass};
        let ok = unsafe { SetPriorityClass(GetCurrentProcess(), IDLE_PRIORITY_CLASS) };
        if ok == 0 {
            say("Warning: -low: the priority could not be lowered");
        }
    }
}

const UNSUPPORTED: &[&str] = &[
    "-glview",
    "-nocsg",
    "-noshare",
    "-noweld",
    "-noopt",
    "-nomergewater",
    "-snapaxial",
    "-dumpcollide",
    "-dumpstaticprop",
    "-dumpstaticprops",
    "-allowdetailcracks",
    "-notjuncsdetail",
    "-xbox",
    "-fullminidumps",
];

const UNSUPPORTED_WITH_VALUE: &[(&str, usize)] = &[("-block", 2), ("-blocks", 4), ("-mpi_pw", 1)];

const ACCEPTED: &[&str] = &["-novconfig", "-allowdebug", "-steam", "-verboseentities", "-tmpout", "-tmpin"];

#[derive(Debug, Default)]
pub(crate) struct BspcArgs {
    pub(crate) game: Option<PathBuf>,
    pub(crate) target: Option<String>,
    pub(crate) gmod: Option<PathBuf>,
    pub(crate) map: Option<PathBuf>,
    pub(crate) opts: hp_bspc::CompileOptions,
    pub(crate) only_ents: bool,
    pub(crate) only_props: bool,
    pub(crate) mount: hp_fs::MountOptions,
    pub(crate) materials: MaterialOptions,
    pub(crate) allow_dynamic: bool,
    pub(crate) low: bool,
    pub(crate) threads: Option<usize>,
    pub(crate) embed: Option<PathBuf>,
    pub(crate) keep_stale_zip: bool,
    pub(crate) replace_materials: bool,
    pub(crate) dx_level: Option<i32>,
    pub(crate) search_paths: Vec<PathBuf>,
    pub(crate) verbose: bool,
    pub(crate) fast: bool,
    pub(crate) notes: Vec<String>,
}

pub(crate) fn app_ids_arg(value: &str) -> Result<Vec<u32>, String> {
    let ids: Vec<u32> = value
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.parse::<u32>()
                .ok()
                .filter(|id| *id > 0 && *id <= hp_fs::MAX_APP_ID)
                .ok_or_else(|| format!("\"{v}\" is not a Steam AppID (-mount_appid 240 or -mount_appid 240,440)"))
        })
        .collect::<Result<_, _>>()?;
    if ids.is_empty() {
        return Err("expected an AppID after -mount_appid".to_owned());
    }
    Ok(ids)
}

fn exe_salt() -> u64 {
    let stamp = std::env::current_exe().and_then(std::fs::metadata).ok().map(|m| {
        let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
        (m.len(), mtime.map_or(0, |d| d.as_nanos() as u64))
    });
    let (size, mtime) = stamp.unwrap_or((0, 0));
    size.rotate_left(32) ^ mtime
}

pub(crate) fn parse_bspc_args(args: &[String]) -> Result<BspcArgs, String> {
    let mut a = BspcArgs {
        opts: hp_bspc::CompileOptions::default(),
        mount: hp_fs::MountOptions { mount_workshop: false, ..hp_fs::MountOptions::default() },
        ..BspcArgs::default()
    };
    let mut i = 0;
    let value = |i: usize, name: &str| args.get(i + 1).cloned().ok_or_else(|| format!("expected a value after {name}"));
    let number = |i: usize, name: &str| -> Result<f64, String> {
        let v = value(i, name)?;
        v.trim()
            .parse::<f64>()
            .ok()
            .filter(|x| x.is_finite())
            .ok_or_else(|| format!("expected a number after {name}, not \"{v}\""))
    };
    while i < args.len() {
        let arg = args[i].as_str();
        let lower = arg.to_ascii_lowercase();
        match lower.as_str() {
            "-game" | "-vproject" => {
                a.game = Some(PathBuf::from(value(i, arg)?));
                i += 1;
            }
            "-insert_search_path" => {
                a.search_paths.push(PathBuf::from(value(i, arg)?));
                i += 1;
            }
            "-target" => {
                a.target = Some(value(i, arg)?);
                i += 1;
            }
            "-gmod" => {
                a.gmod = Some(PathBuf::from(value(i, arg)?));
                i += 1;
            }
            "-threads" => {
                let n = number(i, arg)?;
                a.threads = (n >= 1.0).then_some(n as usize);
                a.opts.threads = a.threads;
                i += 1;
            }
            "-low" => a.low = true,
            "-v" | "-verbose" => a.verbose = true,
            "-leaktest" => a.opts.leak_test = true,
            "-forceskyvis" | "-skyvis" => a.opts.sky_vis = true,
            "-onlyents" => a.only_ents = true,
            "-onlyprops" => a.only_props = true,
            "-vis" => a.opts.vis = hp_bspc::VisLevel::Full,
            "-fastvis" => a.opts.vis = hp_bspc::VisLevel::Fast,
            "-fast" => a.fast = true,
            "-nodetail" => a.opts.no_detail = true,
            "-nowater" => a.opts.no_water = true,
            "-fulldetail" => a.opts.full_detail = true,
            "-nomerge" => a.opts.no_merge = true,
            "-nosubdiv" => a.opts.no_subdiv = true,
            "-notjunc" => a.opts.no_tjunc = true,
            "-noprune" => a.opts.no_prune = true,
            "-novirtualmesh" => a.opts.no_virtual_mesh = true,
            "-propdebug" => a.opts.prop_debug = true,
            "-blocksize" => {
                let v = number(i, arg)?;
                match hp_bspc::compile::block_size_of(v) {
                    Some(size) => {
                        a.opts.block_size = size;
                        a.notes.push(if size == 0 {
                            "blocksize = 0 (no blocks)".to_owned()
                        } else {
                            format!("blocksize = {size}")
                        });
                    }
                    None => a.notes.push(format!(
                        "Ignoring invalid value {v:.6} for -blocksize, must be 0 (no blocks) or in range of [{}-{}]",
                        hp_bspc::compile::MIN_BLOCK_SIZE,
                        hp_bspc::compile::MAX_BLOCK_SIZE
                    )),
                }
                i += 1;
            }
            "-nosort" => a.opts.vis_sort = false,
            "-micro" => {
                a.opts.micro_volume = number(i, arg)?;
                a.notes.push(format!("microvolume = {:.6}", a.opts.micro_volume));
                i += 1;
            }
            "-luxelscale" => {
                a.opts.luxel_scale = number(i, arg)?;
                i += 1;
            }
            "-minluxelscale" => {
                a.opts.min_luxel_scale = number(i, arg)?.max(1.0);
                i += 1;
            }
            "-defaultluxelsize" => {
                let v = number(i, arg)?;
                if v >= 1.0 {
                    a.opts.default_luxel_size = v;
                } else {
                    a.notes.push(format!("Invalid value {v:.6} for -defaultluxelsize"));
                }
                i += 1;
            }
            "-maxlightmapdim" => {
                let v = number(i, arg)?;
                if (1.0..=hp_bspc::compile::MAX_LIGHTMAP_DIM_LIMIT).contains(&v) {
                    a.opts.max_lightmap_dim = v.floor();
                } else {
                    a.notes.push(format!(
                        "Ignoring invalid value {v:.6} for -maxlightmapdim, must be in range of [1-{}]",
                        hp_bspc::compile::MAX_LIGHTMAP_DIM_LIMIT
                    ));
                }
                i += 1;
            }
            "-bumpall" => a.materials.bump_all = true,
            "-lightifmissing" => a.materials.light_if_missing = true,
            "-nodrawtriggers" => a.materials.nodraw_triggers = true,
            "-allowdynamicpropsasstatic" => a.allow_dynamic = true,
            "-replacematerials" => a.replace_materials = true,
            "-keepstalezip" => a.keep_stale_zip = true,
            "-dxlevel" => {
                a.dx_level = Some(number(i, arg)? as i32);
                i += 1;
            }
            "-embed" => {
                a.embed = Some(PathBuf::from(value(i, arg)?));
                i += 1;
            }
            "-radius_override" => {
                a.opts.radius_override = Some(number(i, arg)?);
                i += 1;
            }
            "-viscache" => {
                a.opts.vis_cache =
                    Some(hp_bspc::vis::VisCache { dir: PathBuf::from(value(i, arg)?), salt: exe_salt() });
                i += 1;
            }
            "-trace" => {
                let c1 = number(i, arg)?;
                let c2 = number(i + 1, arg)?;
                a.opts.vis_trace = Some((c1.max(0.0) as u32, c2.max(0.0) as u32));
                i += 2;
            }
            "-noaddons" => a.mount.mount_addons = false,
            "-nowsaddons" => {
                a.mount.mount_addons = false;
                a.mount.mount_workshop = false;
            }
            "-wsaddons" => a.mount.mount_workshop = true,
            "-serveraddons" => a.mount.mount_downloads = true,
            "-mount_appid" => {
                a.mount.app_ids.extend(app_ids_arg(&value(i, arg)?)?);
                i += 1;
            }
            _ if ACCEPTED.contains(&lower.as_str()) => {}
            _ if UNSUPPORTED.contains(&lower.as_str())
                || UNSUPPORTED_WITH_VALUE.iter().any(|(n, _)| *n == lower)
                || lower.starts_with("-mpi") =>
            {
                let skip = UNSUPPORTED_WITH_VALUE.iter().find(|(n, _)| *n == lower).map_or(0, |(_, k)| *k);
                a.notes.push(format!("Warning: {arg} is not supported by the Hammer Pro compiler (ignored)"));
                i += skip;
            }
            _ if arg.starts_with('-') => a.notes.push(format!("Warning: VBSP: Unknown option \"{arg}\" (ignored)")),
            _ => a.map = Some(PathBuf::from(arg)),
        }
        i += 1;
    }
    if a.fast && a.opts.vis == hp_bspc::VisLevel::Full {
        a.opts.vis = hp_bspc::VisLevel::Fast;
    }
    Ok(a)
}

fn material_substitutions(game: Option<&Path>, map_name: &str) -> Option<Vec<(String, String)>> {
    let path = game?.join("cfg").join("materialsub.cfg");
    say("Loading Replacement Keys");
    let Some(text) = std::fs::read(&path).ok().map(|b| String::from_utf8_lossy(&b).into_owned()) else {
        say(&format!("File path: {}Failed to load KeyValues file!", path.display()));
        return None;
    };
    let kv = hp_kv::parse(&text).ok()?;
    let root = kv.child("MaterialReplacements").unwrap_or(&kv);
    let norm = |s: &str| s.trim().replace('\\', "/").to_ascii_lowercase();
    let mut out = Vec::new();
    for block in [root.child("AllMaps"), root.child(map_name)].into_iter().flatten() {
        out.extend(block.props.iter().map(|(k, v)| (norm(k), v.trim().to_owned())));
    }
    Some(out)
}

fn replace_materials(map: &mut hp_vmf::Map, subs: &[(String, String)]) -> usize {
    let find = |name: &str| {
        let key = name.trim().replace('\\', "/").to_ascii_lowercase();
        subs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone())
    };
    let mut n = 0;
    for e in std::iter::once(&mut map.world).chain(map.entities.iter_mut()) {
        for s in e.solids.iter_mut().flat_map(|s| s.sides.iter_mut()) {
            if let Some(new) = find(&s.material) {
                s.material = new;
                n += 1;
            }
        }
        let overlay = e.classname.eq_ignore_ascii_case("info_overlay")
            || e.classname.eq_ignore_ascii_case("info_overlay_transition");
        if overlay && let Some(new) = e.get("material").and_then(find) {
            e.set("material", new);
            n += 1;
        }
    }
    n
}

fn apply_dx_level(map: &mut hp_vmf::Map, level: i32) -> usize {
    let before = map.entities.len();
    map.entities.retain(|e| {
        let read = |k: &str| e.get(k).map_or(0, c_atoi);
        let (min, max) = (read("mindxlevel"), read("maxdxlevel"));
        if min == 0 && max == 0 {
            return true;
        }
        let min = if min == 0 { level } else { min };
        let max = if max == 0 { level } else { max };
        level == 0 || (min..=max).contains(&level)
    });
    before - map.entities.len()
}

fn c_atoi(s: &str) -> i32 {
    let t = s.trim_start();
    let end = t
        .char_indices()
        .find(|&(k, c)| !(c.is_ascii_digit() || (k == 0 && (c == '-' || c == '+'))))
        .map_or(t.len(), |(k, _)| k);
    t[..end].parse().unwrap_or(0)
}

fn folder_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(dir) {
                out.push((rel.to_string_lossy().replace('\\', "/"), p));
            }
        }
    }
    out.sort();
    out
}

fn pack_extra(bsp: Vec<u8>, embed: &[(String, PathBuf)], stale: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let file = hp_bsp::Bsp::from_vec(bsp).map_err(|e| e.to_string())?;
    let mut writer = file.pakfile().and_then(|p| p.to_writer()).map_err(|e| e.to_string())?;
    let mut kept = 0usize;
    if let Some(old) = stale.and_then(|b| hp_bsp::Bsp::from_vec(b.to_vec()).ok()) {
        let pak = old.pakfile().map_err(|e| e.to_string())?;
        for entry in pak.entries() {
            if !writer.contains(&entry.key) {
                let data = pak.read(entry).map_err(|e| e.to_string())?.into_owned();
                writer.add(&entry.key, data).map_err(|e| e.to_string())?;
                kept += 1;
            }
        }
    }
    for (inner, path) in embed {
        writer.add_file(inner, path.clone()).map_err(|e| format!("{inner}: {e}"))?;
    }
    if kept > 0 {
        say(&format!("{kept} files kept from the previous pakfile (-keepstalezip)"));
    }
    if !embed.is_empty() {
        say(&format!("{} files embedded (-embed)", embed.len()));
    }
    let zip = writer.finish(hp_bsp::PakOptions::default()).map_err(|e| e.to_string())?;
    let mut edit = file.edit();
    edit.set_pakfile(zip).map_err(|e| e.to_string())?;
    edit.to_vec().map_err(|e| e.to_string())
}

pub fn run(args: &[String]) -> i32 {
    let a = match parse_bspc_args(args) {
        Ok(a) => a,
        Err(e) => {
            println!("Error: {e}");
            return 1;
        }
    };
    if let Some(map) = &a.map {
        open_log(&map.with_extension(""));
    }
    say(&format!("Hammer Pro map compiler (hp-bspc {})", env!("CARGO_PKG_VERSION")));
    say(&format!("Command line: {}", args.join(" ")));
    for n in &a.notes {
        say(n);
    }
    if a.verbose {
        say("verbose = true");
        say(&format!("{:#?}", a.opts));
    }
    if a.low {
        set_low_priority();
    }
    let Some(map) = a.map.clone() else {
        say("Error: no map given (hammerpro --bspc [-game dir] [-leaktest] <map.vmf>)");
        return 1;
    };
    if a.embed.is_some() && (a.only_ents || a.only_props) {
        say("Error: -embed only makes sense alongside full BSP compiles.");
        return 1;
    }
    match a.threads {
        Some(n) => match rayon::ThreadPoolBuilder::new().num_threads(n).build() {
            Ok(pool) => {
                say(&format!("{n} threads"));
                pool.install(|| run_compile(&a, &map))
            }
            Err(e) => {
                say(&format!("Warning: -threads {n}: {e}"));
                run_compile(&a, &map)
            }
        },
        None => run_compile(&a, &map),
    }
}

fn run_compile(a: &BspcArgs, map: &Path) -> i32 {
    let mut opts = a.opts.clone();
    let vmf = if map.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmf")) {
        map.to_path_buf()
    } else {
        map.with_extension("vmf")
    };
    let vmf = std::path::absolute(&vmf).unwrap_or(vmf);
    let text = match std::fs::read_to_string(&vmf) {
        Ok(t) => t,
        Err(e) => {
            say(&format!("Error: cannot read {}: {e}", vmf.display()));
            return 1;
        }
    };
    let mut parsed = match hp_vmf::parse(&text) {
        Ok(m) => m,
        Err(e) => {
            say(&format!("Error: {}: {e}", vmf.display()));
            return 1;
        }
    };
    say(&format!("Loading {}", vmf.display()));
    opts.map_name = vmf.file_stem().map(|s| s.to_string_lossy().into_owned());
    let cli = match crate::bspc_game::resolve(a.target.as_deref(), a.game.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            say(&format!("Error: {e}"));
            return 1;
        }
    };
    opts.format = cli.format;
    if a.target.is_some() || cli.profile.is_some() {
        say(&cli.describe());
    }
    let root = a.gmod.clone().or_else(|| a.game.as_deref().and_then(gmod_root_of)).or_else(hp_fs::detect_gmod_root);
    let mounted = mount_content(cli.profile.as_ref(), root.as_deref(), &a.mount);
    let fs = match mounted {
        Some(Ok(fs)) => fs,
        Some(Err(e)) => {
            say(&format!("Warning: cannot mount the game ({e}): materials are guessed from their names"));
            hp_fs::GameFs::empty()
        }
        None => {
            say("Warning: Garry's Mod not found: materials are guessed from their names");
            hp_fs::GameFs::empty()
        }
    };
    for g in
        a.game.iter().filter(|g| Some(g.as_path()) != cli.game_dir()).chain(a.search_paths.iter()).chain(a.embed.iter())
    {
        say(&format!("Mounting search path: {}", g.display()));
        if let Err(e) = fs.add_game_dir(g) {
            say(&format!("Warning: {}: {e}", g.display()));
        }
    }
    hp_fs::flush_index();
    if crate::bspc_instances::has_instances(&parsed) {
        let files = crate::bspc_instances::InstanceFiles { vmf: &vmf, game_dir: a.game.as_deref(), fs: Some(&fs) };
        if cli.profile.is_some() {
            let fgd = cli.fgd();
            if fgd.is_none() {
                say("Could not locate the GameData file of the game: instance keys are fixed up by their names");
            }
            crate::bspc_instances::collapse(&mut parsed, &files, fgd.as_ref(), &mut |l| say(l));
        } else {
            crate::bspc_instances::merge_instances(&mut parsed, &files, root.as_deref(), &mut |l| say(l));
        }
    }
    if a.replace_materials {
        let name = opts.map_name.clone().unwrap_or_default().to_ascii_lowercase();
        if let Some(subs) = material_substitutions(a.game.as_deref(), &name) {
            let n = replace_materials(&mut parsed, &subs);
            say(&format!("{n} materials replaced (-replacematerials)"));
        }
    }
    if let Some(level) = a.dx_level {
        say(&format!("DXLevel = {level}"));
        let n = apply_dx_level(&mut parsed, level);
        if n > 0 {
            say(&format!("{n} entities left out by their mindxlevel/maxdxlevel"));
        }
    }
    let models = GameModels { fs: &fs, allow_dynamic: a.allow_dynamic };
    let materials = GameMaterials { fs: fs.clone(), opts: a.materials };
    let mut log = |l: &str| say(l);
    let bsp = vmf.with_extension("bsp");
    let lin = vmf.with_extension("lin");
    if a.only_ents || a.only_props {
        let Ok(existing) = std::fs::read(&bsp) else {
            say(&format!("Error: {} does not exist: compile the whole map first", bsp.display()));
            return 1;
        };
        let mut data = existing;
        if a.only_ents {
            match hp_bspc::compile::entities_only_for(&parsed, &materials, &data, opts.format) {
                Ok(d) => {
                    data = d;
                    say("Entities updated");
                }
                Err(e) => {
                    say(&format!("Error: {e}"));
                    return 1;
                }
            }
        }
        if a.only_props {
            match hp_bspc::compile::props_only_for(
                &parsed,
                &materials,
                &models,
                &data,
                a.opts.prop_debug,
                opts.format,
                &mut log,
            ) {
                Ok(d) => data = d,
                Err(e) => {
                    say(&format!("Error: {e}"));
                    return 1;
                }
            }
        }
        let problems = hp_bspc::validate::validate(&data);
        if !problems.is_empty() {
            for p in &problems {
                say(&format!("Error! {p}"));
            }
            return 1;
        }
        if let Err(e) = std::fs::write(&bsp, &data) {
            say(&format!("Error: cannot write {}: {e}", bsp.display()));
            return 1;
        }
        say(&format!("Wrote {}", bsp.display()));
        return 0;
    }
    let stale = if a.keep_stale_zip { std::fs::read(&bsp).ok() } else { None };
    let mut lines: Vec<String> = Vec::new();
    let result = hp_bspc::compile(&parsed, &materials, &models, &opts, &mut |l: &str| {
        say(l);
        lines.push(l.to_owned());
    });
    say_hints(&lines);
    match result {
        Ok(out) => {
            if let Some(path) = &out.leak {
                write_lin(&lin, path);
                say(&format!("Leak pointfile written to {}", lin.display()));
            } else if let Some(trace) = &out.trace {
                let _ = std::fs::write(&lin, trace);
                say(&format!("Trace written to {}", lin.display()));
            } else {
                let _ = std::fs::remove_file(&lin);
            }
            if out.leak.is_some() {
                let _ = std::fs::remove_file(vmf.with_extension("prt"));
            } else {
                let _ = std::fs::write(vmf.with_extension("prt"), out.prt.replace('\n', "\r\n"));
            }
            let mut data = out.bsp;
            let embed = a.embed.as_deref().map(folder_files).unwrap_or_default();
            if !embed.is_empty() || stale.is_some() {
                match pack_extra(data.clone(), &embed, stale.as_deref()) {
                    Ok(d) => data = d,
                    Err(e) => {
                        say(&format!("Error: cannot add the files to the pakfile: {e}"));
                        return 1;
                    }
                }
            }
            if let Err(e) = std::fs::write(&bsp, &data) {
                say(&format!("Error: cannot write {}: {e}", bsp.display()));
                return 1;
            }
            say(&format!("Wrote {}", bsp.display()));
            0
        }
        Err(hp_bspc::CompileError::Leaked { path }) => {
            write_lin(&lin, &path);
            say(&format!("Leak pointfile written to {}", lin.display()));
            say("Error: the map leaked (-leaktest)");
            1
        }
        Err(e) => {
            say(&format!("Error: {e}"));
            1
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RadArgs {
    pub(crate) opts: hp_bspc::rad::RadOptions,
    pub(crate) map: Option<PathBuf>,
    pub(crate) gmod: Option<PathBuf>,
    pub(crate) game: Option<PathBuf>,
    pub(crate) target: Option<String>,
    pub(crate) designer: Option<String>,
    pub(crate) low: bool,
    pub(crate) mount: hp_fs::MountOptions,
    pub(crate) search_paths: Vec<PathBuf>,
    pub(crate) ignored: Vec<String>,
    pub(crate) notes: Vec<String>,
}

pub(crate) fn parse_rad_args(args: &[String]) -> Result<RadArgs, String> {
    let mut r = RadArgs {
        opts: hp_bspc::rad::RadOptions { hdr: false, ldr: true, ..Default::default() },
        ..RadArgs::default()
    };
    let opts = &mut r.opts;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let next = || args.get(i + 1).and_then(|v| v.parse::<u32>().ok());
        match a.to_ascii_lowercase().as_str() {
            "-ldr" => (opts.ldr, opts.hdr) = (true, false),
            "-hdr" => (opts.ldr, opts.hdr) = (false, true),
            "-both" => (opts.ldr, opts.hdr) = (true, true),
            "-final" => {
                opts.quality = hp_bspc::rad::RadQuality::Final;
                (opts.props.sky_sample_scale, opts.props.sample_scale) = (4.0, 4.0);
            }
            "-staticpropsamplescale" | "-extrasky" => {
                let v =
                    args.get(i + 1).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite() && *v > 0.0);
                match (a.eq_ignore_ascii_case("-extrasky"), v) {
                    (true, Some(v)) => {
                        opts.props.sky_sample_scale = v;
                        opts.extra_sky = Some(v);
                    }
                    (false, Some(v)) => opts.props.sample_scale = v,
                    (_, None) => return Err(format!("expected a scale factor after '{a}'")),
                }
                i += 1;
            }
            "-noextra" => opts.no_extra = true,
            "-fast" => {
                opts.quality = hp_bspc::rad::RadQuality::Draft;
                opts.props.fast = true;
                opts.texlights.fast = true;
            }
            "-smooth" | "-softsun" | "-luxeldensity" => {
                let Some(v) =
                    args.get(i + 1).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0)
                else {
                    return Err(format!("expected a value after '{a}'"));
                };
                match a.to_ascii_lowercase().as_str() {
                    "-smooth" => opts.smooth_angle = v,
                    "-softsun" => opts.soft_sun = v as f32,
                    _ if v < 1.0 && v > 0.0 => opts.luxel_density = v as f32,
                    _ => r.notes.push(format!("Warning: -luxeldensity {v} ignored (it must be below 1)")),
                }
                i += 1;
            }
            "-noskyboxrecurse" => opts.no_skybox_recurse = true,
            "-noaddons" => r.mount.mount_addons = false,
            "-nowsaddons" => (r.mount.mount_addons, r.mount.mount_workshop) = (false, false),
            "-wsaddons" => r.mount.mount_workshop = true,
            "-serveraddons" => r.mount.mount_downloads = true,
            "-mount_appid" => {
                let Some(v) = args.get(i + 1) else { return Err(format!("expected an AppID after '{a}'")) };
                r.mount.app_ids.extend(app_ids_arg(v)?);
                i += 1;
            }
            "-insert_search_path" => {
                let Some(v) = args.get(i + 1) else { return Err(format!("expected a folder after '{a}'")) };
                r.search_paths.push(PathBuf::from(v));
                i += 1;
            }
            "-fastambient" => opts.fast_ambient = true,
            "-centersamples" => {}
            "-low" => r.low = true,
            "-staticproplighting" => opts.props.lighting = true,
            "-staticproppolys" => opts.props.polys = true,
            "-textureshadows" => opts.props.texture_shadows = true,
            "-nossprops" => opts.props.no_self_shadowing = true,
            "-staticpropnormals" => opts.props.show_normals = true,
            "-nostaticpropshadows" => opts.props.shadows = false,
            "-nodispshadows" => opts.no_disp_shadows = true,
            "-nostylebounce" => opts.no_style_bounce = true,
            "-ao" | "-noao" => {}
            "-aoradius" | "-aostrength" | "-aosamples" => {
                if args.get(i + 1).is_some_and(|v| v.trim().parse::<f32>().is_ok()) {
                    i += 1;
                }
            }
            "-bounce" => {
                if let Some(n) = next() {
                    opts.bounces = n;
                }
                i += 1;
            }
            "-threads" => {
                opts.threads = next().filter(|n| *n > 0).map(|n| n as usize);
                i += 1;
            }
            "-game" => {
                r.game = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "-gmod" => {
                r.gmod = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "-target" => {
                let Some(v) = args.get(i + 1) else { return Err(format!("expected a game after '{a}'")) };
                r.target = Some(v.clone());
                i += 1;
            }
            "-lights" => {
                r.designer = args.get(i + 1).filter(|v| !v.is_empty()).cloned();
                i += 1;
            }
            "-chop" | "-maxchop" | "-dispchop" => {
                let v = args.get(i + 1).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite());
                let t = &mut opts.texlights;
                match (a.to_ascii_lowercase().as_str(), v) {
                    (_, Some(v)) if v < 1.0 => return Err(format!("expected positive value after '{a}'")),
                    ("-chop", Some(v)) => t.minchop = v.min(t.maxchop),
                    ("-maxchop", Some(v)) => t.maxchop = v,
                    (_, Some(v)) => t.dispchop = v,
                    (_, None) => return Err(format!("expected a value after '{a}'")),
                }
                i += 1;
            }
            "-radius_override" => i += 1,
            _ if a.starts_with('-') => {
                let low = a.to_ascii_lowercase();
                let takes_value = [
                    "-vproject",
                    "-aoscale",
                    "-aofacesamples",
                    "-aopropsamples",
                    "-ambientocclusion",
                    "-sunsamplesarealight",
                    "-leafambientsamplereduction",
                    "-maxdispsamplesize",
                    "-disppatchradius",
                    "-mpi_workercount",
                    "-dumptrace",
                ];
                if takes_value.contains(&low.as_str()) {
                    i += 1;
                }
                r.ignored.push(a.to_owned());
            }
            _ => r.map = Some(PathBuf::from(a)),
        }
        i += 1;
    }
    Ok(r)
}

pub fn run_rad(args: &[String]) -> i32 {
    let parsed = match parse_rad_args(args) {
        Ok(p) => p,
        Err(e) => {
            println!("Error: {e}");
            return 1;
        }
    };
    let RadArgs { mut opts, map, gmod, game, target, designer, low, mount, search_paths, ignored, notes } = parsed;
    if low {
        set_low_priority();
    }
    for n in &notes {
        println!("{n}");
    }
    let cli = match crate::bspc_game::resolve(target.as_deref(), game.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            println!("Error: {e}");
            return 1;
        }
    };
    opts.limits = cli.format.limits;
    let models =
        LazyModels { gmod, game, profile: cli.profile.clone(), mount, search_paths, fs: std::sync::OnceLock::new() };
    if let Some(m) = &map {
        open_log(&m.with_extension(""));
    }
    say(&format!("Hammer Pro lighting (hp-bspc {})", env!("CARGO_PKG_VERSION")));
    for a in &ignored {
        say(&format!("Warning: option {a} is not supported by this lighting compiler: ignored"));
    }
    let Some(map) = map else {
        say("Error: no map given (hammerpro --rad [-both] [-final] <map.bsp>)");
        return 1;
    };
    let map = std::path::absolute(&map).unwrap_or(map);
    let bsp = map.with_extension("bsp");
    let data = match std::fs::read(&bsp) {
        Ok(d) => d,
        Err(e) => {
            say(&format!("Error: cannot read {}: {e}", bsp.display()));
            return 1;
        }
    };
    let mut log = |l: &str| say(l);
    if target.is_some() || cli.profile.is_some() {
        say(&cli.describe());
    }
    let root = match cli.profile.as_ref() {
        Some(_) => cli.root(),
        None => models
            .gmod
            .clone()
            .or_else(|| models.game.as_deref().and_then(gmod_root_of))
            .or_else(hp_fs::detect_gmod_root),
    };
    let game = match cli.game_dir() {
        Some(g) => Some(g.to_path_buf()),
        None => models.game.clone().or_else(|| root.as_deref().map(|r| r.join("garrysmod"))),
    };
    let world_rad = world_lights_rad(&data);
    opts.texlights.files =
        light_files(game.as_deref(), root.as_deref(), designer.as_deref(), world_rad.as_deref(), Some(&map), &mut log);
    opts.texlights.map_name = map.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let lit = match hp_bspc::rad::light_bsp_report(&data, &opts, &models, &mut log) {
        Ok((d, report)) => {
            if let Ok(file) = hp_bsp::Bsp::from_vec(d.clone()) {
                let lines = hp_bspc::stats::lighting_lines(&file, &report);
                for l in &lines {
                    say(l);
                }
                say_hints(&lines);
            }
            d
        }
        Err(e) => {
            say(&format!("Error: {e}"));
            return 1;
        }
    };
    let before = hp_bspc::validate::validate(&data);
    let (old, new): (Vec<String>, Vec<String>) =
        hp_bspc::validate::validate(&lit).into_iter().partition(|p| before.contains(p));
    for p in &old {
        say(&format!("Warning: {p} (already in the map before the lighting)"));
    }
    if !new.is_empty() {
        for p in &new {
            say(&format!("Error! {p}"));
        }
        return 1;
    }
    if let Err(e) = std::fs::write(&bsp, &lit) {
        say(&format!("Error: cannot write {}: {e}", bsp.display()));
        return 1;
    }
    say(&format!("Wrote {}", bsp.display()));
    0
}

fn say_hints(lines: &[String]) {
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    for l in hp_compile::stats::hint_lines(&hp_compile::stats::parse_lines(&lines)) {
        say(&l);
    }
}

fn write_lin(path: &Path, points: &[glam::DVec3]) {
    let text: String = points.iter().map(|p| format!("{:.6} {:.6} {:.6}\r\n", p.x, p.y, p.z)).collect();
    let _ = std::fs::write(path, text);
}

pub(crate) fn light_files(
    game: Option<&Path>,
    root: Option<&Path>,
    designer: Option<&str>,
    world: Option<&str>,
    map: Option<&Path>,
    log: &mut dyn FnMut(&str),
) -> Vec<hp_bspc::rad::LightFile> {
    let read = |p: &Path| {
        std::fs::read(p).ok().map(|b| hp_bspc::rad::LightFile {
            path: p.display().to_string(),
            text: String::from_utf8_lossy(&b).into_owned(),
        })
    };
    let mut out = Vec::new();
    let mut global = Vec::new();
    if let Some(g) = game {
        global.push(g.join("lights.rad"));
    }
    if let Some(root) = root.or_else(|| game.and_then(Path::parent)) {
        for dir in ["garrysmod", "sourceengine", "garrysmod/download", "bin", "bin/win64"] {
            global.push(root.join(dir).join("lights.rad"));
        }
    }
    match global.iter().find_map(|p| read(p)) {
        Some(f) => out.push(f),
        None => log("Warning: Couldn't open texlight file lights.rad."),
    }
    if let Some(d) = designer {
        let p = Path::new(d);
        let found = read(p).or_else(|| game.filter(|_| p.is_relative()).and_then(|g| read(&g.join(p))));
        match found {
            Some(f) => out.push(f),
            None => log(&format!("Warning: Couldn't open texlight file {d}.")),
        }
    }
    if let Some(w) = world.map(|w| w.trim().replace('\\', "/")).filter(|w| !w.is_empty()) {
        let p = Path::new(&w);
        let mut places: Vec<PathBuf> = Vec::new();
        if p.is_absolute() {
            places.push(p.to_path_buf());
        } else {
            places.extend(game.map(|g| g.join(p)));
            places.extend(map.and_then(Path::parent).map(|d| d.join(p)));
            places.extend(root.or_else(|| game.and_then(Path::parent)).map(|r| r.join(p)));
            places.extend(root.or_else(|| game.and_then(Path::parent)).map(|r| r.join("garrysmod").join(p)));
        }
        match places.iter().find_map(|p| read(p)) {
            Some(f) if out.iter().any(|o| same_file(&o.path, &f.path)) => {
                log(&format!("_lightsrad {w}: the -lights file, read once"))
            }
            Some(f) => {
                log(&format!("Map light file (_lightsrad): {}", f.path));
                out.push(f);
            }
            None => log(&format!("Warning: Couldn't open texlight file {w} (worldspawn _lightsrad).")),
        }
    }
    if let Some(f) = map.and_then(|m| read(&m.with_extension("rad"))) {
        out.push(f);
    }
    out
}

fn same_file(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let p = std::path::absolute(Path::new(s)).unwrap_or_else(|_| PathBuf::from(s));
        p.to_string_lossy().replace('\\', "/").to_ascii_lowercase()
    };
    norm(a) == norm(b)
}

pub(crate) fn world_lights_rad(bsp: &[u8]) -> Option<String> {
    let file = hp_bsp::Bsp::from_vec(bsp.to_vec()).ok()?;
    let ents = file.entities().ok()?;
    ents.first()?.get("_lightsrad").map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned)
}

fn gmod_root_of(game: &Path) -> Option<PathBuf> {
    let parent = game.parent()?;
    if parent.join("garrysmod").join("gameinfo.txt").is_file() {
        return Some(parent.to_path_buf());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    static RUN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn vbsp_and_vvis_options_are_read() {
        let a = parse_bspc_args(&args(
            "-game g -threads 3 -low -nodetail -nowater -fulldetail -forceskyvis -micro 2.5 -luxelscale 2 -minluxelscale 0.5 \
             -defaultluxelsize 8 -maxlightmapdim 64 -nomerge -nosubdiv -notjunc -noprune -novirtualmesh -bumpall -lightifmissing \
             -nodrawtriggers -AllowDynamicPropsAsStatic -replacematerials -keepstalezip -dxlevel 90 -embed e -vis -fast \
             -radius_override 512 -nosort -trace 1 2 -noaddons -insert_search_path p -viscache vc map",
        ))
        .expect("parsed");
        let o = &a.opts;
        assert_eq!(a.game.as_deref(), Some(Path::new("g")));
        assert_eq!((a.threads, o.threads, a.low), (Some(3), Some(3), true));
        assert!(o.no_detail && o.no_water && o.full_detail && o.sky_vis);
        assert_eq!(
            (o.micro_volume, o.luxel_scale, o.min_luxel_scale, o.default_luxel_size, o.max_lightmap_dim),
            (2.5, 2.0, 1.0, 8.0, 64.0)
        );
        assert!(o.no_merge && o.no_subdiv && o.no_tjunc && o.no_prune && o.no_virtual_mesh);
        assert!(a.materials.bump_all && a.materials.light_if_missing && a.materials.nodraw_triggers);
        assert!(a.allow_dynamic && a.replace_materials && a.keep_stale_zip);
        assert_eq!((a.dx_level, a.embed.as_deref()), (Some(90), Some(Path::new("e"))));
        assert_eq!(o.vis, hp_bspc::VisLevel::Fast, "-vis -fast");
        assert_eq!((o.radius_override, o.vis_sort, o.vis_trace), (Some(512.0), false, Some((1, 2))));
        assert_eq!(o.vis_cache.as_ref().map(|c| c.dir.as_path()), Some(Path::new("vc")));
        assert!(!a.mount.mount_addons && !a.mount.mount_workshop);
        assert_eq!(a.search_paths, [PathBuf::from("p")]);
        assert_eq!(a.map.as_deref(), Some(Path::new("map")));
        assert!(a.notes.iter().all(|n| !n.contains("Unknown")), "{:?}", a.notes);
    }

    #[test]
    fn vrad_options_are_read() {
        let r = parse_rad_args(&args(
            "-both -final -bounce 8 -threads 3 -low -noextra -smooth 30 -softsun 2 -extrasky 2 -StaticPropLighting              -StaticPropPolys -textureshadows -nossprops -noskyboxrecurse -fastambient -chop 2 -lights l.rad -game g map",
        ))
        .expect("parsed");
        let o = &r.opts;
        assert!(o.ldr && o.hdr && r.low);
        assert_eq!((o.quality, o.bounces, o.threads), (hp_bspc::rad::RadQuality::Final, 8, Some(3)));
        assert!(o.no_extra && o.no_skybox_recurse && o.fast_ambient);
        assert_eq!((o.smooth_angle, o.soft_sun, o.extra_sky), (30.0, 2.0, Some(2.0)));
        assert!(o.props.lighting && o.props.polys && o.props.texture_shadows && o.props.no_self_shadowing);
        assert!(o.props.shadows, "props cast shadows by default");
        assert_eq!((o.texlights.minchop, r.designer.as_deref()), (2.0, Some("l.rad")));
        assert_eq!((r.game.as_deref(), r.map.as_deref()), (Some(Path::new("g")), Some(Path::new("map"))));
        assert!(r.ignored.is_empty() && r.notes.is_empty(), "{:?} {:?}", r.ignored, r.notes);
        for flag in ["-nostaticpropshadows", "-NoStaticPropShadows"] {
            let r = parse_rad_args(&args(&format!("-ldr {flag} map"))).expect("parsed");
            assert!(!r.opts.props.shadows && r.ignored.is_empty());
            assert!(!r.opts.props.lighting, "the vertex lighting is apart");
        }
        assert!(!parse_rad_args(&args("-ldr map")).expect("parsed").opts.no_disp_shadows);
        let r = parse_rad_args(&args("-ldr -NoDispShadows map")).expect("parsed");
        assert!(r.opts.no_disp_shadows && r.ignored.is_empty());
        assert!(!r.opts.no_style_bounce);
        let r = parse_rad_args(&args("-ldr -NoStyleBounce map")).expect("parsed");
        assert!(r.opts.no_style_bounce && r.ignored.is_empty());
        let r = parse_rad_args(&args("-fast -vproject x -foo -luxeldensity 2 map")).expect("parsed");
        assert_eq!(r.opts.quality, hp_bspc::rad::RadQuality::Draft);
        assert_eq!(r.ignored, ["-vproject", "-foo"]);
        assert_eq!(r.notes.len(), 1);
        assert_eq!(r.map.as_deref(), Some(Path::new("map")));
        assert!(parse_rad_args(&args("-chop 0 map")).is_err());
        assert!(parse_rad_args(&args("-extrasky")).is_err());
        let r = parse_rad_args(&args("-ldr map")).expect("parsed");
        assert!(r.mount.mount_addons && r.mount.mount_workshop && !r.mount.mount_downloads);
        assert!(r.search_paths.is_empty());
        let r = parse_rad_args(&args("-serveraddons -noaddons -insert_search_path e map")).expect("parsed");
        assert!(r.mount.mount_downloads && !r.mount.mount_addons && r.ignored.is_empty(), "{:?}", r.ignored);
        assert_eq!(r.search_paths, [PathBuf::from("e")]);
        assert_eq!(r.map.as_deref(), Some(Path::new("map")));
        let r = parse_rad_args(&args("-nowsaddons map")).expect("parsed");
        assert!(!r.mount.mount_addons && !r.mount.mount_workshop);
        assert!(parse_rad_args(&args("-insert_search_path")).is_err());
        for f in hp_compile::flags::FLAGS.iter().filter(|f| f.step == hp_compile::StepKind::Rad) {
            let line = if f.value { format!("{} 2 map", f.flag) } else { format!("{} map", f.flag) };
            let r = parse_rad_args(&args(&line)).expect("parsed");
            assert!(r.ignored.is_empty(), "{}", f.flag);
            assert_eq!(r.map.as_deref(), Some(Path::new("map")), "{}", f.flag);
        }
    }

    #[test]
    fn old_ambient_occlusion_options_are_accepted_and_ignored() {
        use hp_bspc::rad::RadQuality;
        let plain = parse_rad_args(&args("-both -final map")).expect("parsed");
        let r =
            parse_rad_args(&args("-both -final -ao -aoradius 128 -AOStrength 0.5 -aosamples 40 map")).expect("parsed");
        assert_eq!(r.opts, plain.opts);
        assert_eq!(r.opts.quality, RadQuality::Final);
        assert!(r.ignored.is_empty());
        assert_eq!(
            parse_rad_args(&args("-noao map")).expect("parsed").opts,
            parse_rad_args(&args("map")).expect("parsed").opts
        );
        assert!(parse_rad_args(&args("-aoradius map")).is_ok());
    }

    #[test]
    fn map_light_file_from_the_worldspawn() {
        let dir = std::env::temp_dir().join(format!("hp_lightsrad_{}", std::process::id()));
        let (game, maps) = (dir.join("game"), dir.join("maps"));
        std::fs::create_dir_all(game.join("cfg")).expect("dirs");
        std::fs::create_dir_all(&maps).expect("dirs");
        std::fs::write(game.join("lights.rad"), "lights/a 255 255 255 100\n").expect("write");
        std::fs::write(game.join("cfg/custom.rad"), "lights/b 255 0 0 50\n").expect("write");
        std::fs::write(maps.join("near.rad"), "lights/c 0 255 0 50\n").expect("write");
        let map = maps.join("m.bsp");
        let files = |designer: Option<&str>, world: Option<&str>| {
            let mut lines = Vec::new();
            let f = light_files(Some(&game), None, designer, world, Some(&map), &mut |l| lines.push(l.to_owned()));
            (f.iter().map(|f| f.text.clone()).collect::<Vec<_>>(), lines)
        };
        let (f, _) = files(None, Some("cfg\\custom.rad"));
        assert_eq!(f, ["lights/a 255 255 255 100\n", "lights/b 255 0 0 50\n"]);
        let (f, _) = files(None, Some("near.rad"));
        assert_eq!(f[1], "lights/c 0 255 0 50\n");
        let (f, _) = files(Some(&maps.join("near.rad").display().to_string()), Some("cfg/custom.rad"));
        assert_eq!(f.len(), 3);
        assert_eq!(f[2], "lights/b 255 0 0 50\n");
        let near = maps.join("near.rad").display().to_string();
        let (f, lines) = files(Some(&near), Some(&near));
        assert_eq!(f.len(), 2, "{lines:?}");
        let (f, lines) = files(None, Some("nope.rad"));
        assert_eq!(f.len(), 1);
        assert!(lines.iter().any(|l| l.contains("nope.rad") && l.contains("_lightsrad")), "{lines:?}");
        let _ = std::fs::remove_dir_all(&dir);
        let mut edit = hp_bsp::Bsp::from_vec(minimal_bsp()).expect("bsp").edit();
        let world = hp_bsp::Entity {
            pairs: vec![
                ("classname".to_owned(), "worldspawn".to_owned()),
                ("_lightsrad".to_owned(), " cfg/custom.rad ".to_owned()),
            ],
        };
        edit.set_entities(&[world]).expect("entities");
        let bytes = edit.to_vec().expect("bsp");
        assert_eq!(world_lights_rad(&bytes).as_deref(), Some("cfg/custom.rad"));
        assert_eq!(world_lights_rad(&minimal_bsp()), None);
        assert_eq!(world_lights_rad(b"nope"), None);
    }

    fn minimal_bsp() -> Vec<u8> {
        let mut b = vec![0u8; 1036];
        b[0..4].copy_from_slice(b"VBSP");
        b[4..8].copy_from_slice(&20i32.to_le_bytes());
        b
    }

    #[test]
    fn unknown_and_unsupported_options_are_reported() {
        let a =
            parse_bspc_args(&args("-foo -glview -block 1 2 -skyvis -maxlightmapdim 500 -tmpin map")).expect("parsed");
        assert!(a.notes.contains(&"Warning: VBSP: Unknown option \"-foo\" (ignored)".to_owned()), "{:?}", a.notes);
        assert!(a.notes.contains(&"Warning: -glview is not supported by the Hammer Pro compiler (ignored)".to_owned()));
        assert!(a.notes.contains(&"Warning: -block is not supported by the Hammer Pro compiler (ignored)".to_owned()));
        assert!(
            a.notes
                .iter()
                .any(|n| n == "Ignoring invalid value 500.000000 for -maxlightmapdim, must be in range of [1-125]")
        );
        assert_eq!(a.map.as_deref(), Some(Path::new("map")));
        assert!(a.opts.sky_vis);
        assert!(a.mount.mount_addons && !a.mount.mount_workshop && !a.mount.mount_downloads);
        assert!(parse_bspc_args(&args("-wsaddons map")).expect("parsed").mount.mount_workshop);
        let a = parse_bspc_args(&args("-mount_appid 240 -mount_appid 440,220 map")).expect("parsed");
        assert_eq!(a.mount.app_ids, [240, 440, 220]);
        assert!(parse_bspc_args(&args("-mount_appid tf2 map")).is_err());
        assert!(parse_bspc_args(&args("-mount_appid")).is_err());
        let r = parse_rad_args(&args("-mount_appid 4000 map")).expect("parsed");
        assert_eq!(r.mount.app_ids, [4000]);
        assert!(parse_rad_args(&args("-mount_appid 0 map")).is_err());
        let a = parse_bspc_args(&args("-serveraddons map")).expect("parsed");
        assert!(a.mount.mount_downloads && a.notes.is_empty(), "{:?}", a.notes);
        assert!(parse_bspc_args(&args("-micro")).is_err());
        assert!(parse_bspc_args(&args("-threads x map")).is_err());
    }

    #[test]
    fn block_size_and_prop_debug() {
        let a = parse_bspc_args(&args("map")).expect("parsed");
        assert_eq!(a.opts.block_size, 1024);
        assert!(!a.opts.prop_debug);
        let a = parse_bspc_args(&args("-blocksize 0 -propdebug map")).expect("parsed");
        assert_eq!(a.opts.block_size, 0);
        assert!(a.opts.prop_debug);
        assert!(a.notes.contains(&"blocksize = 0 (no blocks)".to_owned()), "{:?}", a.notes);
        assert_eq!(a.map.as_deref(), Some(Path::new("map")));
        assert_eq!(parse_bspc_args(&args("-BlockSize 4096 map")).expect("parsed").opts.block_size, 4096);
        let a = parse_bspc_args(&args("-blocksize 50 map")).expect("parsed");
        assert_eq!(a.opts.block_size, 1024, "an invalid size keeps VBSP's");
        assert!(
            a.notes.iter().any(|n| n.starts_with("Ignoring invalid value 50.000000 for -blocksize")),
            "{:?}",
            a.notes
        );
        assert!(parse_bspc_args(&args("-blocksize")).is_err());
    }

    #[test]
    fn materials_are_replaced_and_dx_levels_filtered() {
        let text = "world\n{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\nsolid\n{\n\"id\" \"2\"\nside\n{\n\"id\" \"3\"\n\"plane\" \"(0 0 0) (1 0 0) (0 1 0)\"\n\"material\" \"DEV/OLD\"\n}\n}\n}\n\
                    entity\n{\n\"id\" \"4\"\n\"classname\" \"info_overlay\"\n\"material\" \"dev\\old\"\n}\n\
                    entity\n{\n\"id\" \"5\"\n\"classname\" \"light\"\n\"maxdxlevel\" \"80\"\n}\n\
                    entity\n{\n\"id\" \"6\"\n\"classname\" \"light\"\n\"mindxlevel\" \"80\"\n}\n";
        let mut map = hp_vmf::parse(text).expect("vmf");
        let subs = vec![("dev/old".to_owned(), "dev/new".to_owned())];
        assert_eq!(replace_materials(&mut map, &subs), 2);
        assert_eq!(map.world.solids[0].sides[0].material, "dev/new");
        assert_eq!(map.entities[0].get("material"), Some("dev/new"));
        assert_eq!(apply_dx_level(&mut map, 90), 1);
        assert_eq!(map.entities.iter().map(|e| e.id).collect::<Vec<_>>(), [4, 6]);
        assert_eq!(c_atoi(" 12abc"), 12);
    }

    #[test]
    fn embedded_and_stale_files_join_the_pakfile() {
        let dir = std::env::temp_dir().join(format!("hp_bspc_cli_pak_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("embed").join("materials")).unwrap();
        std::fs::write(dir.join("embed").join("materials").join("a.vmt"), "x").unwrap();
        let mut old = hp_bsp::PakWriter::new();
        old.add("materials/old.vmt", b"o".to_vec()).unwrap();
        let old =
            hp_bsp::synthetic_bsp(20, &[(hp_bsp::lump::PAKFILE, old.finish(hp_bsp::PakOptions::default()).unwrap())]);
        let new = hp_bsp::synthetic_bsp(20, &[]);
        let files = folder_files(&dir.join("embed"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "materials/a.vmt");
        let out = pack_extra(new, &files, Some(&old)).expect("packed");
        let bsp = hp_bsp::Bsp::from_vec(out).unwrap();
        let pak = bsp.pakfile().unwrap();
        assert!(pak.contains("materials/a.vmt") && pak.contains("materials/old.vmt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn room_of(material: &str, inner: &str) -> String {
        let mut out = String::from("world\n{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n");
        let mut id = 2;
        let boxes = [
            ([-128, -128, -16], [128, 128, 0]),
            ([-128, -128, 256], [128, 128, 272]),
            ([-144, -128, 0], [-128, 128, 256]),
            ([128, -128, 0], [144, 128, 256]),
            ([-128, 128, 0], [128, 144, 256]),
            ([-128, -144, 0], [128, -128, 256]),
            ([-32, -32, 64], [32, 32, 128]),
        ];
        for (k, ([x0, y0, z0], [x1, y1, z1])) in boxes.into_iter().enumerate() {
            let material = if k == 6 { inner } else { material };
            let planes = [
                format!("({x0} {y1} {z1}) ({x1} {y1} {z1}) ({x1} {y0} {z1})"),
                format!("({x0} {y0} {z0}) ({x1} {y0} {z0}) ({x1} {y1} {z0})"),
                format!("({x0} {y1} {z1}) ({x0} {y0} {z1}) ({x0} {y0} {z0})"),
                format!("({x1} {y1} {z0}) ({x1} {y0} {z0}) ({x1} {y0} {z1})"),
                format!("({x1} {y1} {z1}) ({x0} {y1} {z1}) ({x0} {y1} {z0})"),
                format!("({x1} {y0} {z0}) ({x0} {y0} {z0}) ({x0} {y0} {z1})"),
            ];
            out.push_str(&format!("solid\n{{\n\"id\" \"{id}\"\n"));
            id += 1;
            for p in planes {
                out.push_str(&format!(
                    "side\n{{\n\"id\" \"{id}\"\n\"plane\" \"{p}\"\n\"material\" \"{material}\"\n\"uaxis\" \"[1 0 0 0] 0.25\"\n\"vaxis\" \"[0 -1 0 0] 0.25\"\n\"lightmapscale\" \"16\"\n}}\n"
                ));
                id += 1;
            }
            out.push_str("}\n");
        }
        out.push_str(&format!(
            "}}\nentity\n{{\n\"id\" \"{id}\"\n\"classname\" \"info_player_start\"\n\"origin\" \"0 0 16\"\n}}\n"
        ));
        out
    }

    fn texinfos_of(bsp: &[u8], material: &str) -> Vec<(u32, i32)> {
        let bsp = hp_bsp::Bsp::from_vec(bsp.to_vec()).expect("bsp");
        let lump = |i: usize| bsp.lump(i).expect("lump").into_owned();
        let (texinfo, texdata, table, strings) = (lump(6), lump(hp_bsp::lump::TEXDATA), lump(44), lump(43));
        let int = |b: &[u8], at: usize| i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        let name = |td: usize| {
            let start = usize::try_from(int(&table, 4 * usize::try_from(int(&texdata, td * 32 + 12)).expect("id")))
                .expect("ofs");
            let end = strings[start..].iter().position(|c| *c == 0).map_or(strings.len(), |n| start + n);
            String::from_utf8_lossy(&strings[start..end]).to_ascii_lowercase()
        };
        texinfo
            .chunks_exact(72)
            .filter_map(|t| {
                let td = usize::try_from(int(t, 68)).ok()?;
                (name(td) == material).then(|| (int(t, 64) as u32, int(&texdata, td * 32 + 16)))
            })
            .collect()
    }

    #[test]
    fn server_content_is_found_with_serveraddons() {
        let _run = RUN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = std::env::temp_dir().join(format!("hp_bspc_cli_server_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let gmod = dir.join("GarrysMod");
        let mats = gmod.join("garrysmod").join("download").join("materials").join("hp_server");
        std::fs::create_dir_all(&mats).unwrap();
        let wall = "\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"hp_server/wall\"\n}\n";
        std::fs::write(mats.join("wall.vmt"), wall).unwrap();
        std::fs::write(
            mats.join("glass.vmt"),
            "\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"hp_server/glass\"\n\t\"$translucent\" \"1\"\n}\n",
        )
        .unwrap();
        let map = dir.join("hp_server_room.vmf");
        std::fs::write(&map, room_of("HP_SERVER/WALL", "HP_SERVER/GLASS")).unwrap();
        let compile = |extra: &[&str]| {
            let mut a: Vec<String> = extra.iter().map(|s| (*s).to_owned()).collect();
            a.extend(["-gmod".to_owned(), gmod.display().to_string(), map.display().to_string()]);
            assert_eq!(run(&a), 0, "{a:?}");
            let bsp = std::fs::read(map.with_extension("bsp")).expect("bsp written");
            (texinfos_of(&bsp, "hp_server/wall"), texinfos_of(&bsp, "hp_server/glass"))
        };
        let (nolight, trans) = (hp_bspc::materials::surf::NOLIGHT, hp_bspc::materials::surf::TRANS);
        let (wall, glass) = compile(&[]);
        assert!(!wall.is_empty() && !glass.is_empty());
        assert!(wall.iter().all(|(f, w)| f & nolight != 0 && *w == 0), "{wall:?}");
        assert!(glass.iter().all(|(f, _)| f & trans == 0), "{glass:?}");
        let (wall, glass) = compile(&["-serveraddons"]);
        assert!(!wall.is_empty() && !glass.is_empty());
        assert!(wall.iter().all(|(f, w)| f & nolight == 0 && *w > 0), "{wall:?}");
        assert!(glass.iter().all(|(f, _)| f & trans != 0), "{glass:?}");
        if let Ok(mut log) = LOG_FILE.lock() {
            *log = None;
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_output_is_appended_to_the_map_log() {
        let _run = RUN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = std::env::temp_dir().join(format!("hp_bspc_cli_log_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let map = dir.join("m");
        let code = run(&args(&format!("-foo -gmod {} {}", dir.join("nogame").display(), map.display())));
        assert_eq!(code, 1);
        let first = std::fs::read_to_string(dir.join("m.log")).expect("log written");
        assert!(first.contains("Warning: VBSP: Unknown option \"-foo\" (ignored)\r\n"), "{first}");
        assert!(first.contains("Error: cannot read"), "{first}");
        let _ = run(&args(&format!("-gmod {} {}", dir.join("nogame").display(), map.display())));
        let second = std::fs::read_to_string(dir.join("m.log")).expect("log");
        assert!(second.starts_with(&first) && second.len() > first.len(), "appended");
        if let Ok(mut log) = LOG_FILE.lock() {
            *log = None;
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
