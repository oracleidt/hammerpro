use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use hp_app::settings::{ContentSettings, ReleaseMode};
use hp_app::{t, tf};
use hp_bsp::{Bsp, PakCompression, PakOptions, lump};
use hp_compile::{CompileEvent, Severity as LogSeverity, StepAction, StepKind};
use hp_fs::GameFs;
use hp_fs::deps::{DepKind, DepNode, DepRoot, DepWarning, DependencyGraph, DepsCache, PackPolicy, ResolveOptions};
use hp_gma::{AddonJson, GmaSource, GmaWriter};
use rayon::prelude::*;

use super::files::{copy_atomic, safe_join, stamp, write_atomic};
use super::manifest::{ReleaseManifest, Written, digest};
use super::placement::{Place, name_problem, place, skipped};
use super::session::{self, Pass, Verdict};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Stage {
    Dependencies,
    Compile,
    GameSession,
    Pack,
    Nav,
    Content,
    Gma,
    Done,
}

impl Stage {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Stage::Dependencies => t!("release.stage.dependencies"),
            Stage::Compile => t!("release.stage.compile"),
            Stage::GameSession => t!("release.stage.game"),
            Stage::Pack => t!("release.stage.pack"),
            Stage::Nav => t!("release.stage.nav"),
            Stage::Content => t!("release.stage.content"),
            Stage::Gma => t!("release.stage.gma"),
            Stage::Done => t!("release.stage.done"),
        }
    }
}

pub(crate) type KeyKinds = Arc<HashMap<String, Vec<(String, hp_editor::deps_roots::RootKind)>>>;

pub(crate) struct ReleaseInput {
    pub vmf: PathBuf,
    pub map: String,
    pub out_root: PathBuf,
    pub addon_folder: String,
    pub settings: ContentSettings,
    pub compile: Option<crate::compile::PrepInput>,
    pub fs: GameFs,
    pub roots: Vec<DepRoot>,
    pub instances: Vec<(String, String)>,
    pub key_kinds: KeyKinds,
    pub addon: AddonJson,
    pub thumb_png: Option<Vec<u8>>,
    pub icon_jpg: Option<Vec<u8>>,
    pub cache: Arc<DepsCache>,
    pub cache_dir: PathBuf,
    pub game_session: bool,
    pub gmod_root: Option<PathBuf>,
    pub nav_fingerprint: Option<u64>,
    pub protect_salt: String,
    pub workshop_id: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum NavOutcome {
    #[default]
    None,
    Shipped {
        fixed: bool,
    },
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SizeTip {
    pub vpath: String,
    pub bytes: u64,
    pub tip: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BspFacts {
    pub size: u64,
    pub has_vis: bool,
    pub ldr: bool,
    pub hdr: bool,
    pub cubemap_samples: usize,
    pub built_cubemaps: usize,
    pub compressed: bool,
    pub limits: Vec<hp_bsp::LimitUsage>,
    pub walkable: Option<u64>,
}

impl BspFacts {
    pub(crate) fn read(path: &Path, map: &str) -> Option<BspFacts> {
        let bsp = Bsp::open(path).ok()?;
        let nonempty = |l: usize| bsp.lump_info(l).is_ok_and(|i| i.len > 0);
        let pak = bsp.pakfile().ok();
        let samples = bsp.cubemaps().unwrap_or_default();
        let built_at = pak.as_ref().map(|p| hp_bsp::cubemaps::built_origins(p, map)).unwrap_or_default();
        let built = samples.iter().filter(|(o, _)| built_at.contains(o)).count();
        Some(BspFacts {
            size: bsp.len() as u64,
            has_vis: nonempty(lump::VISIBILITY),
            ldr: nonempty(lump::LIGHTING),
            hdr: nonempty(lump::LIGHTING_HDR),
            cubemap_samples: samples.len(),
            built_cubemaps: built,
            compressed: pak.as_ref().is_some_and(|p| p.any_compressed()),
            limits: bsp.limit_usage(),
            walkable: bsp.walkable_fingerprint().ok(),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ReleaseSummary {
    pub out_root: PathBuf,
    pub addon_dir: PathBuf,
    pub bsp: PathBuf,
    pub gma: Option<PathBuf>,
    pub icon: Option<PathBuf>,
    pub pak_files: usize,
    pub pak_bytes: u64,
    pub addon_files: usize,
    pub addon_bytes: u64,
    pub up_to_date: usize,
    pub cubemaps_carried: usize,
    pub nav: NavOutcome,
    pub tips: Vec<SizeTip>,
    pub facts: BspFacts,
    pub duration_ms: u64,
    pub gma_bytes: u64,
    pub fastdl: Option<PathBuf>,
    pub lint: Vec<hp_fs::lint::LintIssue>,
    pub protected: Option<Protected>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Protected {
    pub server_bsp: PathBuf,
    pub entities_removed: usize,
    pub connections_stripped: usize,
    pub original_faces_bytes: usize,
}

pub(crate) enum ReleaseEvent {
    Stage(Stage),
    Log(LogSeverity, String),
    Compile(CompileEvent),
    Progress(f32),
    Graph(Arc<DependencyGraph>),
    NavFingerprint(u64),
    Done(Box<Result<ReleaseSummary, String>>),
}

struct Ctx<'a> {
    input: &'a ReleaseInput,
    tx: &'a Sender<ReleaseEvent>,
    cancel: &'a AtomicBool,
    wake: &'a (dyn Fn() + Sync),
}

impl Ctx<'_> {
    fn send(&self, e: ReleaseEvent) {
        let _ = self.tx.send(e);
        (self.wake)();
    }

    fn log(&self, severity: LogSeverity, text: String) {
        self.send(ReleaseEvent::Log(severity, text));
    }

    fn stage(&self, s: Stage) {
        self.send(ReleaseEvent::Stage(s));
        self.send(ReleaseEvent::Progress(0.0));
    }

    fn check_cancel(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) { Err(t!("release.cancelled").to_owned()) } else { Ok(()) }
    }
}

pub(crate) fn run(
    input: ReleaseInput,
    tx: Sender<ReleaseEvent>,
    cancel: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let start = Instant::now();
    let ctx = Ctx { input: &input, tx: &tx, cancel: &cancel, wake: &*wake };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_stages(&ctx)))
        .unwrap_or_else(|_| Err(t!("release.err_crashed").to_owned()))
        .map(|mut s| {
            s.duration_ms = start.elapsed().as_millis() as u64;
            s
        });
    ctx.send(ReleaseEvent::Done(Box::new(result)));
}

pub(crate) fn analyze(
    input: ReleaseInput,
    tx: Sender<ReleaseEvent>,
    cancel: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let ctx = Ctx { input: &input, tx: &tx, cancel: &cancel, wake: &*wake };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.stage(Stage::Dependencies);
        let graph = resolve(&ctx)?;
        let policy = PackPolicy { workshop: input.settings.pack_workshop, mounted: input.settings.pack_mounted };
        let shipped: Vec<&DepNode> = graph.shipped(policy).filter(|n| !skipped(&n.vpath)).collect();
        let tips = size_tips(&input.fs, &shipped);
        let lint = input.fs.lint_paths(&graph, policy);
        ctx.send(ReleaseEvent::Graph(Arc::new(graph)));
        ctx.stage(Stage::Done);
        Ok(ReleaseSummary { tips, lint, ..ReleaseSummary::default() })
    }))
    .unwrap_or_else(|_| Err(t!("release.err_crashed").to_owned()));
    ctx.send(ReleaseEvent::Done(Box::new(result)));
}

fn run_stages(ctx: &Ctx<'_>) -> Result<ReleaseSummary, String> {
    let input = ctx.input;
    let s = &input.settings;

    ctx.stage(Stage::Dependencies);
    let graph = Arc::new(resolve(ctx)?);
    ctx.send(ReleaseEvent::Graph(graph.clone()));
    ctx.check_cancel()?;
    if graph.cancelled {
        return Err(t!("release.cancelled").to_owned());
    }
    if !graph.missing.is_empty() {
        return Err(tf!("release.err_missing", graph.missing.len()));
    }
    let policy = PackPolicy { workshop: s.pack_workshop, mounted: s.pack_mounted };
    let shipped: Vec<&DepNode> = graph.shipped(policy).filter(|n| !skipped(&n.vpath)).collect();
    if let Some(bad) = shipped.iter().find(|n| name_problem(&n.vpath).is_some()) {
        return Err(tf!("release.err_bad_name", bad.vpath));
    }
    let mut nowhere: Vec<&str> = Vec::new();
    for n in &shipped {
        if place(&n.vpath, s.mode).is_none() {
            if n.kind == DepKind::SoundScript {
                ctx.log(LogSeverity::Warning, tf!("release.not_loadable_script", n.vpath));
            } else {
                nowhere.push(&n.vpath);
            }
        }
    }
    if !nowhere.is_empty() {
        for v in nowhere.iter().take(20) {
            ctx.log(LogSeverity::Error, tf!("release.not_loadable", v));
        }
        let key = if s.mode == ReleaseMode::WorkshopAddon {
            "release.err_not_loadable"
        } else {
            "release.err_not_loadable_bsp"
        };
        return Err(tf!(key, nowhere.len()));
    }
    let tips = size_tips(&input.fs, &shipped);
    let lint = input.fs.lint_paths(&graph, policy);

    let mut compiled = input.vmf.with_extension("bsp");
    if let Some(prep) = &input.compile
        && s.compile_first
    {
        ctx.stage(Stage::Compile);
        compiled = compile(ctx, prep, &graph)?;
    }
    ctx.check_cancel()?;
    if !compiled.is_file() {
        return Err(tf!("release.err_no_bsp", compiled.display()));
    }

    let addon_dir = input.out_root.join(&input.addon_folder);
    let maps_dir = addon_dir.join("maps");
    std::fs::create_dir_all(&maps_dir).map_err(|e| tf!("release.err_write", maps_dir.display(), e))?;
    let out_bsp = maps_dir.join(format!("{}.bsp", input.map));
    let mut manifest = ReleaseManifest::load(&input.out_root);
    manifest.version = 1;
    let mut summary = ReleaseSummary {
        out_root: input.out_root.clone(),
        addon_dir: addon_dir.clone(),
        bsp: out_bsp.clone(),
        tips,
        lint,
        ..ReleaseSummary::default()
    };

    let pak_nodes: Vec<&DepNode> =
        shipped.iter().copied().filter(|n| place(&n.vpath, s.mode) == Some(Place::Pakfile)).collect();
    let addon_nodes: Vec<&DepNode> =
        shipped.iter().copied().filter(|n| place(&n.vpath, s.mode) == Some(Place::Addon)).collect();

    let game = input.gmod_root.as_deref().and_then(hp_compile::ToolPaths::detect);
    let mut cubemap_sources: Vec<PathBuf> = vec![out_bsp.clone()];
    if let Some(t) = &game {
        cubemap_sources.push(t.maps_dir.join(format!("{}.bsp", input.map)));
    }
    let mut session_nav: Option<PathBuf> = None;
    if input.game_session {
        let Some(tools) = &game else {
            return Err(t!("release.err_no_game").to_owned());
        };
        ctx.stage(Stage::Pack);
        let staged = tools.maps_dir.join(format!("{}.bsp", input.map));
        let carried = pack(ctx, &compiled, &cubemap_sources, &pak_nodes, PakCompression::Store, &staged, None)?;
        summary.cubemaps_carried = carried.1;
        ctx.stage(Stage::GameSession);
        session_nav = game_session(ctx, tools, &staged)?;
        cubemap_sources = vec![staged];
    }
    ctx.check_cancel()?;
    ctx.stage(Stage::Pack);
    let compression = if s.compress_pakfile { PakCompression::Lzma } else { PakCompression::Store };
    let (pak_stats, carried) =
        pack(ctx, &compiled, &cubemap_sources, &pak_nodes, compression, &out_bsp, Some(&mut manifest))?;
    summary.pak_files = pak_stats.0;
    summary.pak_bytes = pak_stats.1;
    summary.cubemaps_carried = summary.cubemaps_carried.max(carried);
    summary.facts = BspFacts::read(&out_bsp, &input.map).unwrap_or_default();
    ctx.check_cancel()?;

    ctx.stage(Stage::Nav);
    let out_nav = maps_dir.join(format!("{}.nav", input.map));
    summary.nav = ship_nav(ctx, &compiled, session_nav.as_deref(), game.as_ref(), &out_nav, summary.facts.size)?;

    if s.protect {
        protect_map(ctx, &out_bsp, &out_nav, &mut summary)?;
    }

    ctx.stage(Stage::Content);
    let mut produced: BTreeMap<String, PathBuf> = BTreeMap::new();
    produced.insert(format!("maps/{}.bsp", input.map), out_bsp.clone());
    if out_nav.is_file() {
        produced.insert(format!("maps/{}.nav", input.map), out_nav.clone());
    }
    if let Some(png) = &input.thumb_png {
        let vpath = format!("maps/thumb/{}.png", input.map);
        let path = addon_dir.join("maps").join("thumb").join(format!("{}.png", input.map));
        write_if_changed(&path, png).map_err(|e| tf!("release.err_write", path.display(), e))?;
        produced.insert(vpath, path);
    }
    if uses_skybox_switches(&out_bsp) {
        for (vpath, content) in hp_editor::sky::switch_lua_files() {
            let path = safe_join(&addon_dir, vpath).ok_or_else(|| tf!("release.err_bad_name", vpath))?;
            write_if_changed(&path, content.as_bytes()).map_err(|e| tf!("release.err_write", path.display(), e))?;
            produced.insert(vpath.to_owned(), path);
        }
        ctx.log(LogSeverity::Info, t!("release.skybox_switch_lua").to_owned());
    }
    if s.protect && s.protect_lock && !s.protect_lock_key.trim().is_empty() {
        let path = safe_join(&addon_dir, hp_bsp::LOCK_LUA_PATH)
            .ok_or_else(|| tf!("release.err_bad_name", hp_bsp::LOCK_LUA_PATH))?;
        write_if_changed(&path, hp_bsp::LOCK_LUA.as_bytes())
            .map_err(|e| tf!("release.err_write", path.display(), e))?;
        produced.insert(hp_bsp::LOCK_LUA_PATH.to_owned(), path);
        ctx.log(LogSeverity::Info, t!("release.protect_lock_lua").to_owned());
    }
    summary.up_to_date = copy_content(ctx, &addon_nodes, &addon_dir, &mut manifest, &mut produced)?;
    summary.addon_files = produced.len();
    summary.addon_bytes = produced.values().filter_map(|p| stamp(p).map(|s| s.0)).sum();
    let json_path = addon_dir.join("addon.json");
    write_if_changed(&json_path, input.addon.to_json().as_bytes())
        .map_err(|e| tf!("release.err_write", json_path.display(), e))?;
    remove_stale(ctx, &addon_dir, &mut manifest, &produced);
    for (vpath, path) in &produced {
        if let Some(st) = stamp(path) {
            manifest.files.entry(vpath.clone()).or_insert(Written { size: st.0, mtime: st.1, ..Written::default() });
        }
    }

    if s.write_gma {
        ctx.stage(Stage::Gma);
        let gma_path = input.out_root.join(format!("{}.gma", input.addon_folder));
        let d = digest(produced.iter().filter_map(|(v, p)| stamp(p).map(|st| (v.as_str(), st.0, st.1))));
        if manifest.gma_digest == Some(d) && gma_path.is_file() {
            ctx.log(LogSeverity::Info, t!("release.gma_up_to_date").to_owned());
        } else {
            let mut w = GmaWriter::new(input.addon.clone());
            for (vpath, path) in &produced {
                if let Err(reason) = w.add(vpath, GmaSource::File(path.clone())) {
                    ctx.log(LogSeverity::Warning, tf!("release.gma_refused", vpath, format!("{reason:?}")));
                }
            }
            let tx_progress = |done: u64, total: u64| {
                if total > 0 {
                    ctx.send(ReleaseEvent::Progress(done as f32 / total as f32));
                }
            };
            let res = w.write_with(&gma_path, ctx.cancel, &tx_progress).map_err(|e| tf!("release.err_gma", e))?;
            manifest.gma_digest = Some(d);
            ctx.log(LogSeverity::Info, tf!("release.gma_written", gma_path.display(), fmt_bytes(res.bytes)));
        }
        summary.gma_bytes = std::fs::metadata(&gma_path).map(|m| m.len()).unwrap_or(0);
        summary.gma = Some(gma_path);
    }

    if let Some(jpg) = &input.icon_jpg {
        let icon = input.out_root.join(format!("{}_icon.jpg", input.addon_folder));
        write_if_changed(&icon, jpg).map_err(|e| tf!("release.err_write", icon.display(), e))?;
        summary.icon = Some(icon);
    }
    if s.mode == ReleaseMode::ServerKit {
        summary.fastdl = Some(server_kit(ctx, &out_bsp, &out_nav)?);
    }
    manifest.save(&input.out_root).map_err(|e| tf!("release.err_write", input.out_root.display(), e))?;
    ctx.stage(Stage::Done);
    Ok(summary)
}

fn resolve(ctx: &Ctx<'_>) -> Result<DependencyGraph, String> {
    let input = ctx.input;
    let mut roots = input.roots.clone();
    let mut warn = |w: String| ctx.log(LogSeverity::Warning, w);
    let nested = super::instance_roots(&input.vmf, &input.instances, &input.key_kinds, ctx.cancel, &mut warn)
        .ok_or_else(|| t!("release.cancelled").to_owned())?;
    roots.extend(nested);
    let opts = ResolveOptions { map_name: &input.map, cancel: Some(ctx.cancel), ..ResolveOptions::default() };
    let graph = input.fs.resolve_dependencies(&roots, &opts, &input.cache);
    let shipped =
        graph.shipped(PackPolicy { workshop: input.settings.pack_workshop, mounted: input.settings.pack_mounted });
    let (n, bytes) = shipped.fold((0usize, 0u64), |(n, b), node| (n + 1, b + node.size));
    ctx.log(
        LogSeverity::Info,
        tf!("release.graph_summary", graph.nodes.len(), n, fmt_bytes(bytes), graph.missing.len()),
    );
    for m in graph.missing.iter().take(50) {
        ctx.log(LogSeverity::Error, tf!("release.missing_line", m.name));
    }
    let mut mounted = 0;
    for w in &graph.warnings {
        let text = match w {
            DepWarning::MountedContent { .. } => {
                mounted += 1;
                continue;
            }
            DepWarning::ScriptNotLoaded { name, file } => tf!("release.warn_script", name, file),
            DepWarning::SoundscapeNotLoaded { name, file } => tf!("release.warn_soundscape", name, file),
            DepWarning::Unreadable { vpath, error } => tf!("release.warn_unreadable", vpath, error),
        };
        ctx.log(LogSeverity::Warning, text);
    }
    if mounted > 0 {
        let key = if input.settings.pack_mounted { "release.warn_mounted_shipped" } else { "release.warn_mounted" };
        ctx.log(LogSeverity::Warning, tf!(key, mounted));
    }
    Ok(graph)
}

fn compile(ctx: &Ctx<'_>, prep: &crate::compile::PrepInput, graph: &DependencyGraph) -> Result<PathBuf, String> {
    let mut input = crate::compile::PrepInput {
        vmf: prep.vmf.clone(),
        settings: prep.settings.clone(),
        gmod_root: prep.gmod_root.clone(),
        fs: prep.fs.clone(),
        materials: prep.materials.clone(),
        models: Vec::new(),
        cache_dir: prep.cache_dir.clone(),
        content_dir: prep.content_dir.clone(),
    };
    for n in &graph.nodes {
        if n.kind == DepKind::Material
            && let Some(name) = n.vpath.strip_prefix("materials/").and_then(|p| p.strip_suffix(".vmt"))
        {
            input.materials.push(name.to_owned());
        }
    }
    input.materials.sort_unstable();
    input.materials.dedup();
    let out = crate::compile::prepare(input);
    for (sev, msg) in out.messages {
        ctx.log(sev, msg);
    }
    let (mut plan, workdir) = out.result?;
    let bsp = plan.bsp_path();
    for step in &mut plan.steps {
        if matches!(step.kind, StepKind::CopyToMaps | StepKind::LaunchGame | StepKind::Pack) {
            step.enabled = false;
        }
    }
    let handle = hp_compile::start(plan, &workdir);
    let mut result: Option<(bool, bool, bool)> = None;
    loop {
        if ctx.cancel.load(Ordering::Relaxed) {
            handle.cancel();
        }
        match handle.events.recv_timeout(Duration::from_millis(100)) {
            Ok(ev @ CompileEvent::Finished { .. }) => {
                if let CompileEvent::Finished { success, cancelled, leaked, .. } = &ev {
                    result = Some((*success, *cancelled, *leaked));
                }
                ctx.send(ReleaseEvent::Compile(ev));
                break;
            }
            Ok(ev) => ctx.send(ReleaseEvent::Compile(ev)),
            Err(crossbeam_timeout) if crossbeam_timeout.is_timeout() => {}
            Err(_) => break,
        }
    }
    handle.wait();
    match result {
        Some((_, true, _)) | None if ctx.cancel.load(Ordering::Relaxed) => Err(t!("release.cancelled").to_owned()),
        Some((_, _, true)) => Err(t!("release.err_leak").to_owned()),
        Some((true, _, _)) => Ok(bsp),
        _ => Err(t!("release.err_compile").to_owned()),
    }
}

fn pack(
    ctx: &Ctx<'_>,
    compiled: &Path,
    cubemap_sources: &[PathBuf],
    nodes: &[&DepNode],
    compression: PakCompression,
    dest: &Path,
    manifest: Option<&mut ReleaseManifest>,
) -> Result<((usize, u64), usize), String> {
    let input = ctx.input;
    let bsp = Bsp::read(compiled).map_err(|e| tf!("release.err_bsp", compiled.display(), e))?;
    let mut writer =
        bsp.pakfile().and_then(|p| p.to_writer()).map_err(|e| tf!("release.err_bsp", compiled.display(), e))?;
    let mut carried = 0;
    if input.settings.keep_cubemaps {
        let mut sources: Vec<&PathBuf> =
            cubemap_sources.iter().filter(|p| p.is_file() && p.as_path() != compiled).collect();
        sources.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
        let mut done = HashSet::new();
        for src in sources {
            let Ok(old) = Bsp::read(src) else { continue };
            if let Ok(old_pak) = old.pakfile()
                && let Ok(n) = hp_bsp::cubemaps::carry_over(&old_pak, &bsp, &input.map, &mut writer, &mut done)
            {
                carried += n;
            }
        }
        if carried > 0 {
            ctx.log(LogSeverity::Info, tf!("release.cubemaps_kept", carried));
        }
    }
    let materialized: Vec<Result<(String, PathBuf), String>> = nodes
        .par_iter()
        .map(|n| {
            input
                .fs
                .materialize_dependency(n, &input.cache_dir)
                .map(|p| (n.vpath.clone(), p))
                .map_err(|e| tf!("release.err_materialize", n.vpath, e))
        })
        .collect();
    let mut bytes = 0;
    let mut inputs: Vec<(String, u64, u64)> = Vec::with_capacity(nodes.len() + 4);
    for item in materialized {
        let (vpath, path) = item?;
        let st = stamp(&path).unwrap_or_default();
        bytes += st.0;
        inputs.push((vpath.clone(), st.0, st.1));
        writer.add_file(&vpath, path).map_err(|e| tf!("release.err_bad_name", e))?;
    }
    ctx.check_cancel()?;
    let files = writer.len();
    let options = format!("{compression:?} {} {}", input.settings.lzma_level, input.settings.keep_cubemaps);
    let mut extra = vec![("compiled bsp".to_owned(), stamp(compiled).unwrap_or_default()), (options, (0, 0))];
    for src in cubemap_sources {
        extra.push((src.display().to_string(), stamp(src).filter(|_| src != dest).unwrap_or_default()));
    }
    let d = digest(
        inputs.iter().map(|(v, a, b)| (v.as_str(), *a, *b)).chain(extra.iter().map(|(v, (a, b))| (v.as_str(), *a, *b))),
    );
    if let Some(m) = &manifest
        && m.bsp_digest == Some(d)
        && m.bsp_written.is_some()
        && m.bsp_written == stamp(dest)
    {
        ctx.log(LogSeverity::Info, t!("release.pak_up_to_date").to_owned());
        let pak_len = Bsp::open(dest).ok().and_then(|b| b.lump_info(lump::PAKFILE).ok().map(|i| u64::from(i.len)));
        return Ok(((files, pak_len.unwrap_or(bytes)), carried));
    }
    let opts = PakOptions { compression, level: input.settings.lzma_level.min(9) };
    let progress = |done: u64, total: u64| {
        if total > 0 {
            ctx.send(ReleaseEvent::Progress(done as f32 / total as f32));
        }
    };
    let zip = writer.finish_with(opts, ctx.cancel, &progress).map_err(|e| {
        if ctx.cancel.load(Ordering::Relaxed) { t!("release.cancelled").to_owned() } else { tf!("release.err_pack", e) }
    })?;
    let zip_len = zip.len();
    let mut edit = bsp.edit();
    edit.set_pakfile(zip).map_err(|e| tf!("release.err_pack", e))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| tf!("release.err_write", parent.display(), e))?;
    }
    edit.save(dest).map_err(|e| tf!("release.err_write", dest.display(), e))?;
    if let Some(m) = manifest {
        m.bsp_digest = Some(d);
        m.bsp_written = stamp(dest);
    }
    ctx.log(LogSeverity::Info, tf!("release.packed", files, fmt_bytes(zip_len as u64), dest.display()));
    Ok(((files, zip_len as u64), carried))
}

fn game_session(ctx: &Ctx<'_>, tools: &hp_compile::ToolPaths, staged: &Path) -> Result<Option<PathBuf>, String> {
    if hp_compile::sandbox::is_sandboxed() {
        return Err(hp_compile::sandbox::REFUSED.to_owned());
    }
    let input = ctx.input;
    let exe = session::game_exe(tools).ok_or_else(|| t!("release.err_no_game").to_owned())?;
    let game_folder =
        tools.game_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "garrysmod".into());
    let facts = BspFacts::read(staged, &input.map).unwrap_or_default();
    let passes = session::passes(facts.ldr, facts.hdr, facts.cubemap_samples > 0, true);
    let workdir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let log_path = tools.game_dir.join("console.log");
    let restore = RestoreOnDrop(super::game_settings::backup(&tools.game_dir));
    for (i, pass) in passes.iter().enumerate() {
        ctx.check_cancel()?;
        ctx.send(ReleaseEvent::Progress(i as f32 / passes.len() as f32));
        let label = match pass {
            Pass::Cubemaps { hdr: false } => t!("release.pass_cubemaps_ldr"),
            Pass::Cubemaps { hdr: true } => t!("release.pass_cubemaps_hdr"),
            Pass::Nav => t!("release.pass_nav"),
        };
        ctx.log(LogSeverity::Info, tf!("release.pass_start", label));
        let mut plan = hp_compile::CompilePlan::new(&input.vmf);
        plan.steps.push(hp_compile::Step {
            kind: StepKind::Custom,
            enabled: true,
            exe: exe.clone(),
            args: session::args(*pass, &input.map, &game_folder),
            continue_on_error: false,
            action: StepAction::Run,
            workdir: None,
        });
        let original = if *pass == Pass::Nav { Some(set_session_entities(staged, true)?) } else { None };
        let mut tail = session::LogTail::new(&log_path);
        let handle = hp_compile::start(plan, &workdir);
        let started = Instant::now();
        let mut verdict: Option<Verdict> = None;
        let mut exited_ok = false;
        loop {
            if ctx.cancel.load(Ordering::Relaxed) {
                handle.cancel();
            }
            match handle.events.recv_timeout(Duration::from_millis(500)) {
                Ok(CompileEvent::Finished { success, .. }) => {
                    exited_ok = success;
                    break;
                }
                Ok(_) => {}
                Err(e) if e.is_timeout() => {}
                Err(_) => break,
            }
            let text = tail.poll();
            if verdict.is_none()
                && let Some(v) = session::scan(*pass, &text)
            {
                verdict = Some(v);
                handle.cancel();
            }
            if started.elapsed() > session::timeout(*pass) {
                verdict.get_or_insert(Verdict::Failed(t!("release.pass_timeout").to_owned()));
                handle.cancel();
            }
        }
        handle.wait();
        restore.0.restore();
        if let Some(entities) = original {
            restore_entities(staged, &entities)?;
        }
        ctx.check_cancel()?;
        match (pass, verdict) {
            (_, Some(Verdict::Failed(line))) => return Err(tf!("release.pass_failed", label, line)),
            (Pass::Nav, Some(Verdict::Done)) => {}
            (Pass::Nav, None) => return Err(tf!("release.pass_failed", label, t!("release.pass_no_marker"))),
            (Pass::Cubemaps { .. }, _) if !exited_ok => {
                return Err(tf!("release.pass_failed", label, t!("release.pass_exit")));
            }
            _ => {}
        }
        ctx.log(LogSeverity::Info, tf!("release.pass_done", label));
    }
    let after = BspFacts::read(staged, &input.map).unwrap_or_default();
    ctx.log(LogSeverity::Info, tf!("release.cubemaps_built", after.built_cubemaps, after.cubemap_samples));
    let nav = tools.maps_dir.join(format!("{}.nav", input.map));
    if nav.is_file() {
        if let Some(fp) = after.walkable {
            ctx.send(ReleaseEvent::NavFingerprint(fp));
        }
        let _ = hp_bsp::nav::fixup_bsp_size(&nav, after.size);
        return Ok(Some(nav));
    }
    Ok(None)
}

struct RestoreOnDrop(super::game_settings::Backup);

impl Drop for RestoreOnDrop {
    fn drop(&mut self) {
        self.0.restore();
    }
}

fn set_session_entities(staged: &Path, add: bool) -> Result<Vec<hp_bsp::Entity>, String> {
    let bsp = Bsp::read(staged).map_err(|e| tf!("release.err_bsp", staged.display(), e))?;
    let original = bsp.entities().map_err(|e| tf!("release.err_bsp", staged.display(), e))?;
    if add {
        let mut entities = original.clone();
        entities.extend(session::nav_helpers());
        let mut edit = bsp.edit();
        edit.set_entities(&entities).map_err(|e| tf!("release.err_bsp", staged.display(), e))?;
        edit.save(staged).map_err(|e| tf!("release.err_write", staged.display(), e))?;
    }
    Ok(original)
}

fn restore_entities(staged: &Path, entities: &[hp_bsp::Entity]) -> Result<(), String> {
    let bsp = Bsp::read(staged).map_err(|e| tf!("release.err_bsp", staged.display(), e))?;
    let mut edit = bsp.edit();
    edit.set_entities(entities).map_err(|e| tf!("release.err_bsp", staged.display(), e))?;
    edit.save(staged).map_err(|e| tf!("release.err_write", staged.display(), e))
}

fn ship_nav(
    ctx: &Ctx<'_>,
    compiled: &Path,
    session_nav: Option<&Path>,
    game: Option<&hp_compile::ToolPaths>,
    out_nav: &Path,
    bsp_size: u64,
) -> Result<NavOutcome, String> {
    let input = ctx.input;
    let name = format!("{}.nav", input.map);
    let candidates: Vec<PathBuf> = match session_nav {
        Some(p) => vec![p.to_path_buf()],
        None => {
            let mut c = vec![input.vmf.with_extension("nav")];
            if let Some(t) = game {
                c.push(t.maps_dir.join(&name));
            }
            c.push(out_nav.to_path_buf());
            c
        }
    };
    let newest =
        candidates.iter().filter(|p| p.is_file()).max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    let Some(src) = newest.cloned() else {
        return Ok(NavOutcome::None);
    };
    let current = Bsp::open(compiled).ok().and_then(|b| b.walkable_fingerprint().ok());
    let mut known = input.nav_fingerprint;
    if session_nav.is_some() {
        known = current;
    }
    if known.is_none() {
        let header = std::fs::read(&src).ok().and_then(|b| hp_bsp::nav::read_nav_header(&b).ok());
        if let (Some(h), Some(t)) = (header, game) {
            let game_bsp = t.maps_dir.join(format!("{}.bsp", input.map));
            if let Ok(b) = Bsp::open(&game_bsp)
                && h.bsp_size == Some(b.len() as u32)
                && b.walkable_fingerprint().ok() == current
            {
                known = current;
            }
        }
    }
    if src != out_nav {
        copy_atomic(&src, out_nav).map_err(|e| tf!("release.err_write", out_nav.display(), e))?;
    }
    if known.is_some() && known == current {
        if let Some(fp) = current {
            ctx.send(ReleaseEvent::NavFingerprint(fp));
        }
        let fixed = hp_bsp::nav::fixup_bsp_size(out_nav, bsp_size).map_err(|e| tf!("release.err_nav", e))?;
        ctx.log(LogSeverity::Info, t!(if fixed { "release.nav_fixed" } else { "release.nav_current" }).to_owned());
        Ok(NavOutcome::Shipped { fixed })
    } else {
        ctx.log(LogSeverity::Warning, t!("release.nav_stale").to_owned());
        Ok(NavOutcome::Stale)
    }
}

type Copied = (String, PathBuf, Option<Written>);

fn protect_map(ctx: &Ctx<'_>, out_bsp: &Path, out_nav: &Path, summary: &mut ReleaseSummary) -> Result<(), String> {
    let input = ctx.input;
    let s = &input.settings;
    ctx.check_cancel()?;
    let full = hp_bsp::Bsp::read(out_bsp).map_err(|e| tf!("release.err_protect", e))?;
    let watermark: String = s.protect_watermark.chars().filter(|c| !c.is_control()).take(120).collect();
    let lock = (s.protect_lock && !s.protect_lock_key.trim().is_empty())
        .then(|| hp_bsp::Lock { hash: hp_bsp::lock_hash(s.protect_lock_key.trim()), block: s.protect_lock_block });
    let opts = hp_bsp::ProtectOptions {
        watermark,
        strip_original_faces: s.protect_strip_original_faces,
        randomize_materials: s.protect_randomize,
        salt: input.protect_salt.clone(),
        lock,
        layout: hp_bsp::LayoutOptions {
            shuffle_lumps: s.protect_shuffle_lumps,
            junk_gaps: s.protect_junk_gaps,
            decoys: s.protect_decoys,
            scramble_header: s.protect_scramble_header,
            shuffle_game_lumps: s.protect_shuffle_game_lumps,
            random_pak: s.protect_random_pak,
        },
        reorder: hp_bsp::ReorderOptions { tables: s.protect_reorder_tables, world: s.protect_reorder_world },
        layout_seed: s.protect_layout_seed.trim().to_owned(),
        ..Default::default()
    };
    let (protected, report) = hp_bsp::protect(&full, &opts).map_err(|e| tf!("release.err_protect", e))?;
    let server_bytes = hp_bsp::server_copy(&protected, &full, &opts).map_err(|e| tf!("release.err_protect", e))?;
    let server_maps = input.out_root.join("server").join("garrysmod").join("maps");
    std::fs::create_dir_all(&server_maps).map_err(|e| tf!("release.err_write", server_maps.display(), e))?;
    let server_bsp = server_maps.join(format!("{}.bsp", input.map));
    write_if_changed(&server_bsp, &server_bytes).map_err(|e| tf!("release.err_write", server_bsp.display(), e))?;
    let readme = input.out_root.join("server").join("README.txt");
    let lock_on = s.protect_lock && !s.protect_lock_key.trim().is_empty();
    write_if_changed(&readme, protect_readme(&input.map, lock_on, hp_app::i18n::lang()).as_bytes())
        .map_err(|e| tf!("release.err_write", readme.display(), e))?;
    for old in [format!("{}_server.bsp", input.map), format!("{}_server.nav", input.map), "SERVER_README.txt".into()] {
        let _ = std::fs::remove_file(input.out_root.join(old));
    }
    write_if_changed(out_bsp, &protected).map_err(|e| tf!("release.err_write", out_bsp.display(), e))?;
    summary.facts = BspFacts::read(out_bsp, &input.map).unwrap_or(summary.facts.clone());
    ctx.log(
        LogSeverity::Info,
        tf!(
            "release.protected",
            report.entities_removed,
            report.connections_stripped,
            fmt_bytes(report.original_faces_bytes as u64)
        ),
    );
    if report.materials_randomized > 0 {
        ctx.log(LogSeverity::Info, tf!("release.protect_randomized", report.materials_randomized));
    }
    if !report.layout_seed.is_empty() {
        ctx.log(
            LogSeverity::Info,
            tf!(
                "release.protect_layout",
                report.reordered.len(),
                report.decoy_blocks,
                fmt_bytes((report.decoy_bytes + report.gap_bytes) as u64),
                report.layout_seed
            ),
        );
    }
    if out_nav.is_file() {
        let kept = server_maps.join(format!("{}.nav", input.map));
        let _ = std::fs::remove_file(&kept);
        if std::fs::rename(out_nav, &kept).is_ok() {
            if let Err(e) = hp_bsp::nav::fixup_bsp_size(&kept, server_bytes.len() as u64) {
                ctx.log(LogSeverity::Warning, tf!("release.err_write", kept.display(), e));
            }
            ctx.log(LogSeverity::Info, tf!("release.protect_nav", kept.display()));
        }
    }
    ctx.log(LogSeverity::Warning, tf!("release.protect_server", server_bsp.display()));
    summary.protected = Some(Protected {
        server_bsp,
        entities_removed: report.entities_removed,
        connections_stripped: report.connections_stripped,
        original_faces_bytes: report.original_faces_bytes,
    });
    Ok(())
}

fn protect_readme(map: &str, lock: bool, lang: hp_app::i18n::Lang) -> String {
    if lang == hp_app::i18n::Lang::Fr {
        let lock_step = if lock {
            "\n3. Ta map est verrouillée : ajoute cette ligne dans le server.cfg de ton\n   \
             serveur, avec la clé que tu as donnée à Hammer Pro :\n\n       \
             hammerpro_key \"ta-cle-secrete\"\n\n   \
             Sans la bonne clé, la map affiche le filigrane (ou se bloque si tu as\n   \
             activé le blocage). La clé n'est jamais écrite dans la map, seulement\n   \
             son empreinte.\n"
        } else {
            ""
        };
        return format!(
            "Protect your BSP : fichiers pour TON serveur\n\
             ==============================================\n\n\
             Les joueurs téléchargent la copie PROTÉGÉE de {map} (dans l'addon ou le\n\
             GMA) : sans logique de jeu, avec filigrane, difficile à décompiler.\n\
             Ce dossier contient la copie avec toute la logique, réservée à ton\n\
             serveur. Ne la partage pas.\n\n\
             1. Copie le dossier \"garrysmod\" d'ici dans le dossier de ton serveur.\n   \
             Il ajoute garrysmod/maps/{map}.bsp (et {map}.nav si la map en a un).\n\
             2. Installe aussi l'addon sur le serveur (le GMA ou l'addon du Workshop) :\n   \
             il contient le contenu de la map et ses scripts.\n\
             {lock_step}\n\
             Les deux copies sont identiques sauf leurs entités, c'est-à-dire tout ce\n\
             que le jeu compare quand un joueur rejoint : les joueurs qui ont la\n\
             copie protégée peuvent donc rejoindre ton serveur.\n"
        );
    }
    let lock_step = if lock {
        "\n3. Your map is locked: add this line to your server's server.cfg, with the\n   \
         key you gave Hammer Pro:\n\n       \
         hammerpro_key \"your-secret-key\"\n\n   \
         Without the right key the map shows the watermark (or is blocked when you\n   \
         turned that on). The key is never written in the map, only its hash.\n"
    } else {
        ""
    };
    format!(
        "Protect your BSP: files for YOUR server\n\
         =========================================\n\n\
         Players download the PROTECTED copy of {map} (in the addon or the GMA):\n\
         no gameplay logic, watermarked, hard to decompile. This folder holds the\n\
         copy with all the logic, for your server only. Do not share it.\n\n\
         1. Copy the \"garrysmod\" folder from here into your server's folder.\n   \
         It adds garrysmod/maps/{map}.bsp (and {map}.nav when the map has one).\n\
         2. Install the addon on the server too (the GMA or the Workshop addon):\n   \
         it carries the map's content and scripts.\n\
         {lock_step}\n\
         Both copies are identical except for their entities, which is everything\n\
         the game compares when a player joins: players who have the protected\n\
         copy can join your server.\n"
    )
}

fn uses_skybox_switches(bsp: &Path) -> bool {
    hp_bsp::Bsp::open(bsp).and_then(|b| b.entities()).is_ok_and(|ents| {
        ents.iter()
            .any(|e| e.get("classname").is_some_and(|c| c.trim().eq_ignore_ascii_case(hp_editor::sky::SWITCH_CLASS)))
    })
}

fn copy_content(
    ctx: &Ctx<'_>,
    nodes: &[&DepNode],
    addon_dir: &Path,
    manifest: &mut ReleaseManifest,
    produced: &mut BTreeMap<String, PathBuf>,
) -> Result<usize, String> {
    let input = ctx.input;
    let current: &ReleaseManifest = manifest;
    let results: Vec<Result<Copied, String>> = nodes
        .par_iter()
        .map(|n| {
            let dest = safe_join(addon_dir, &n.vpath).ok_or_else(|| tf!("release.err_bad_name", n.vpath))?;
            let src = input
                .fs
                .materialize_dependency(n, &input.cache_dir)
                .map_err(|e| tf!("release.err_materialize", n.vpath, e))?;
            let src_stamp = stamp(&src).ok_or_else(|| tf!("release.err_materialize", n.vpath, src.display()))?;
            if current.up_to_date(&n.vpath, src_stamp, stamp(&dest)) {
                return Ok((n.vpath.clone(), dest, None));
            }
            copy_atomic(&src, &dest).map_err(|e| tf!("release.err_write", dest.display(), e))?;
            let d = stamp(&dest).unwrap_or_default();
            let w = Written { source_size: src_stamp.0, source_mtime: src_stamp.1, size: d.0, mtime: d.1 };
            Ok((n.vpath.clone(), dest, Some(w)))
        })
        .collect();
    let (mut copied, mut same) = (0, 0);
    for r in results {
        let (vpath, dest, written) = r?;
        match written {
            Some(w) => {
                manifest.files.insert(vpath.clone(), w);
                copied += 1;
            }
            None => same += 1,
        }
        produced.insert(vpath, dest);
    }
    ctx.log(LogSeverity::Info, tf!("release.content_copied", copied, same));
    Ok(same)
}

fn remove_stale(ctx: &Ctx<'_>, addon_dir: &Path, manifest: &mut ReleaseManifest, produced: &BTreeMap<String, PathBuf>) {
    let stale: Vec<String> = manifest.files.keys().filter(|k| !produced.contains_key(*k)).cloned().collect();
    for vpath in stale {
        manifest.files.remove(&vpath);
        if let Some(path) = safe_join(addon_dir, &vpath)
            && path.is_file()
            && std::fs::remove_file(&path).is_ok()
        {
            ctx.log(LogSeverity::Info, tf!("release.removed_stale", vpath));
        }
    }
}

fn server_kit(ctx: &Ctx<'_>, bsp: &Path, nav: &Path) -> Result<PathBuf, String> {
    let input = ctx.input;
    let root = input.out_root.join("fastdl");
    let maps = root.join("maps");
    copy_atomic(bsp, &maps.join(format!("{}.bsp", input.map)))
        .map_err(|e| tf!("release.err_write", maps.display(), e))?;
    if nav.is_file() {
        copy_atomic(nav, &maps.join(format!("{}.nav", input.map)))
            .map_err(|e| tf!("release.err_write", maps.display(), e))?;
    }
    let lua = match input.workshop_id {
        Some(id) => format!(
            "-- Generated by Hammer Pro: clients download {} from the Workshop.\nresource.AddWorkshop(\"{id}\")\n",
            input.map
        ),
        None => format!(
            "-- Generated by Hammer Pro: clients download the map through FastDL.\nresource.AddFile(\"maps/{}.bsp\")\n",
            input.map
        ),
    };
    let lua_path = root.join("lua").join("autorun").join("server").join(format!("{}_resources.lua", input.map));
    write_if_changed(&lua_path, lua.as_bytes()).map_err(|e| tf!("release.err_write", lua_path.display(), e))?;
    ctx.log(LogSeverity::Info, tf!("release.fastdl_written", root.display()));
    Ok(root)
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() == bytes.len() as u64)
        && std::fs::read(path).is_ok_and(|b| b == bytes)
    {
        return Ok(());
    }
    write_atomic(path, bytes)
}

fn size_tips(fs: &GameFs, nodes: &[&DepNode]) -> Vec<SizeTip> {
    let mut tips: Vec<SizeTip> = nodes
        .par_iter()
        .filter(|n| n.kind == DepKind::Texture && n.size > 64 << 10)
        .filter_map(|n| {
            let info = fs.read_with(&n.vpath, |b| hp_vtf::read_info(b).ok()).flatten()?;
            use hp_vtf::ImageFormat as F;
            let alpha = info.flags & (hp_vtf::flags::EIGHTBITALPHA | hp_vtf::flags::ONEBITALPHA) != 0;
            let tip = if info.width.max(info.height) > 1024 {
                "release.tip_large"
            } else if info.format == F::Dxt5 && !alpha {
                "release.tip_dxt1"
            } else if matches!(
                info.format,
                F::Rgba8888 | F::Bgra8888 | F::Rgb888 | F::Bgr888 | F::Abgr8888 | F::Argb8888
            ) {
                "release.tip_uncompressed"
            } else if info.mip_count <= 1 && info.width.max(info.height) > 64 {
                "release.tip_mipmaps"
            } else {
                return None;
            };
            Some(SizeTip { vpath: n.vpath.clone(), bytes: n.size, tip })
        })
        .collect();
    tips.sort_by_key(|t| std::cmp::Reverse(t.bytes));
    tips
}

pub(crate) fn fmt_bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{n} {}", UNITS[0]) } else { format!("{v:.1} {}", UNITS[u]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_formatting() {
        assert_eq!(fmt_bytes(12), "12 B");
        assert_eq!(fmt_bytes(1536), "1.5 KB");
        assert_eq!(fmt_bytes(5 << 20), "5.0 MB");
    }

    #[test]
    fn skybox_switches_are_found_in_the_compiled_map() {
        let dir = crate::test_scratch("release_switch");
        std::fs::create_dir_all(&dir).expect("dir");
        let write = |name: &str, ents: &str| {
            let path = dir.join(name);
            std::fs::write(&path, hp_bsp::synthetic_bsp(20, &[(0, ents.as_bytes().to_vec())])).expect("bsp");
            path
        };
        let with = write("with.bsp", r#"{ "classname" "worldspawn" } { "classname" "point_hammerpro_skybox" }"#);
        let without = write("without.bsp", r#"{ "classname" "worldspawn" }"#);
        assert!(uses_skybox_switches(&with));
        assert!(!uses_skybox_switches(&without));
        assert!(!uses_skybox_switches(&dir.join("missing.bsp")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
