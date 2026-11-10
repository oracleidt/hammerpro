use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use hp_app::settings::{CompilePresetSetting, CompileSettings};
use hp_app::{t, tf};
use hp_compile::history::{self, HistoryEntry};
use hp_compile::incremental::{self, AutoKind, BuildRecord, Choice, Fingerprint};
use hp_compile::{
    BspMode, CompilePlan, DeferredFiles, DeferredSender, RadMode, Severity as LogSeverity, StepKind, StepModes,
    ToolPaths, VisMode,
};
use hp_editor::compile_export::CompileSnapshot;
use hp_vmf::Map;
use serde::{Deserialize, Serialize};

use super::plan::{self, PlanExtras, effective_modes, packs_content, preset_of};
use crate::util::{normalize_material, plural};

pub(crate) const BUILD_DIR: &str = ".hammerpro_build";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CompileSidecar {
    pub builds: BTreeMap<String, String>,
}

impl hp_app::sidecar::SidecarSection for CompileSidecar {
    const NAME: &'static str = "compile";
}

impl CompileSidecar {
    pub(crate) fn record(&self, name: &str) -> Option<BuildRecord> {
        self.builds.get(name).and_then(|t| BuildRecord::decode(t))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub name: String,
    pub build_dir: PathBuf,
    pub source: Option<PathBuf>,
}

impl Target {
    pub(crate) fn vmf(&self) -> PathBuf {
        self.build_dir.join(format!("{}.vmf", self.name))
    }
}

pub(crate) fn target_for(
    doc_path: Option<&Path>,
    cordoned: bool,
    untitled_dir: &Path,
    beside: bool,
) -> Result<Target, String> {
    let suffix = if cordoned { "_cordon" } else { "" };
    match doc_path {
        Some(p) => {
            let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
            let name = hp_compile::validated_map_name(&abs).map_err(|e| super::notices::plan_error_text(&e))?;
            let dir = abs.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
            let build_dir = if beside {
                hp_app::settings::Settings::cache_dir().join("compile").join(hp_app::sidecar::map_key(Some(&abs)))
            } else {
                dir.join(BUILD_DIR)
            };
            Ok(Target { name: format!("{name}{suffix}"), build_dir, source: Some(abs) })
        }
        None => Ok(Target { name: format!("untitled{suffix}"), build_dir: untitled_dir.to_path_buf(), source: None }),
    }
}

pub(crate) struct SnapshotInput {
    pub snapshot: CompileSnapshot,
    pub target: Target,
    pub settings: CompileSettings,
    pub gmod_root: Option<PathBuf>,
    pub fs: Option<hp_fs::GameFs>,
    pub mounting: bool,
    pub previous: Option<BuildRecord>,
    pub cache_dir: PathBuf,
    pub content_dir: PathBuf,
    pub builds_dir: Option<PathBuf>,
    pub dry_run: bool,
    pub history_file: Option<PathBuf>,
    pub release: bool,
    pub background: bool,
    pub snapshot_ms: u64,
    pub launch_flags: Vec<&'static str>,
}

pub(crate) enum PrepError {
    NeedsMount(Box<SnapshotInput>),
    Failed(String),
}

pub(crate) struct PackJob {
    pub sender: DeferredSender,
    pub materials: Vec<String>,
    pub models: Vec<String>,
    pub roots: Vec<hp_fs::deps::DepRoot>,
    pub map: String,
    pub fs: hp_fs::GameFs,
    pub cache_dir: PathBuf,
}

pub(crate) struct Prepared {
    pub plan: CompilePlan,
    pub workdir: PathBuf,
    pub target: Target,
    pub record: BuildRecord,
    pub choice: Option<Choice>,
    pub pack: Option<PackJob>,
    pub release: bool,
}

pub(crate) struct SnapshotOutput {
    pub result: Result<Prepared, PrepError>,
    pub dry: Option<DryRun>,
    pub messages: Vec<(LogSeverity, String)>,
}

pub(crate) fn used_assets(map: &Map) -> (Vec<String>, Vec<String>) {
    let mut materials = Vec::new();
    let mut models = Vec::new();
    let solids = map
        .world
        .solids
        .iter()
        .filter(|s| !s.hidden)
        .chain(map.entities.iter().filter(|e| !e.hidden).flat_map(|e| e.solids.iter().filter(|s| !s.hidden)));
    for s in solids {
        for side in &s.sides {
            materials.push(normalize_material(&side.material));
        }
    }
    for e in map.entities.iter().filter(|e| !e.hidden) {
        for (key, value) in &e.props {
            if matches!(
                hp_editor::deps_roots::fallback_kind(&e.classname, key, value),
                Some(hp_editor::deps_roots::RootKind::Material | hp_editor::deps_roots::RootKind::Sprite)
            ) {
                materials.push(normalize_material(value));
            }
        }
        if let Some(m) = e.get("model") {
            let m = m.trim().replace('\\', "/").to_ascii_lowercase();
            if m.ends_with(".mdl") {
                models.push(m);
            }
        }
    }
    materials.retain(|m| !m.is_empty());
    materials.sort_unstable();
    materials.dedup();
    models.sort_unstable();
    models.dedup();
    (materials, models)
}

pub(crate) fn pack_roots(map: &Map) -> Vec<hp_fs::deps::DepRoot> {
    use hp_editor::deps_roots::RootKind as E;
    let classify = |_: &str, _: &str| None;
    let entities = std::iter::once(&map.world).chain(map.entities.iter().filter(|e| !e.hidden));
    hp_editor::deps_roots::entity_asset_roots(entities, &classify)
        .into_iter()
        .filter(|r| !matches!(r.kind, E::Material | E::Sprite))
        .filter_map(|r| {
            let kind = crate::release::root_kind(r.kind)?;
            Some(hp_fs::deps::DepRoot { kind, name: r.name, why: String::new() })
        })
        .collect()
}

pub(crate) fn mode_name(kind: StepKind, modes: &StepModes) -> &'static str {
    match kind {
        StepKind::Bsp => match (modes.bsp, modes.vis) {
            (BspMode::Full, VisMode::Full) => "full+vis",
            (BspMode::Full, VisMode::Fast) => "full+fastvis",
            (BspMode::Full, VisMode::Off) => "full",
            (BspMode::OnlyEnts, _) => "onlyents",
            (BspMode::OnlyProps, _) => "onlyprops",
            (BspMode::EntsAndProps, _) => "onlyents+onlyprops",
            (BspMode::Off, _) => "",
        },
        StepKind::Rad => match modes.rad {
            RadMode::Draft => "draft",
            RadMode::Normal => "normal",
            RadMode::Final => "final",
            RadMode::Off => "",
        },
        _ => "",
    }
}

pub(crate) fn estimate(entries: &[HistoryEntry], plan: &CompilePlan) -> Option<u64> {
    let steps: Vec<(StepKind, &str)> = plan
        .steps
        .iter()
        .filter(|s| matches!(s.kind, StepKind::Bsp | StepKind::Rad))
        .map(|s| (s.kind, mode_name(s.kind, &plan.modes)))
        .collect();
    if steps.is_empty() {
        return Some(0);
    }
    history::estimate_ms(entries, &steps)
}

fn stamp_text(p: &Path) -> String {
    match incremental::file_stamp(p) {
        Some((size, mtime)) => format!("{}|{size}|{mtime}", p.display()),
        None => format!("{}|missing", p.display()),
    }
}

fn context_hash(
    s: &CompileSettings,
    base: StepModes,
    tools: &ToolPaths,
    staged: &[PathBuf],
    game_dir: &Path,
    content: &[String],
    compiler: Option<&str>,
) -> u64 {
    let mut parts: Vec<String> = vec![
        "hammerpro-context-3".to_owned(),
        content.join(" "),
        format!("{base:?}"),
        format!("{:?}", s.lighting),
        s.extra_bsp_args.clone(),
        s.extra_rad_args.clone(),
        format!("{:?}", s.bsp_flag_choices),
        format!("{:?}", s.rad_flag_choices),
        format!("props {} {}", s.static_prop_lighting, s.static_prop_shadows),
        stamp_text(&tools.compiler),
        stamp_text(&game_dir.join("gameinfo.txt")),
    ];
    let mut staged: Vec<String> = staged.iter().map(|p| stamp_text(p)).collect();
    staged.sort_unstable();
    parts.extend(staged);
    if let Some(c) = compiler {
        parts.push(c.to_owned());
    }
    incremental::context_hash(parts.iter().map(String::as_str))
}

fn with_lights_rad(mut fp: Fingerprint, game_dir: &Path) -> Fingerprint {
    let mut h = hp_compile::stable_hash::StableHasher::new();
    std::hash::Hasher::write_u64(&mut h, fp.bake);
    h.str(&stamp_text(&game_dir.join("lights.rad")));
    fp.bake = h.finish_u64();
    fp
}

fn to_compile_fp(f: hp_editor::fingerprint::Fingerprint) -> Fingerprint {
    Fingerprint {
        structural: f.structural,
        detail: f.detail,
        bake: f.bake,
        static_props: f.static_props,
        detail_props: f.detail_props,
        logic: f.logic,
        brush_models: f.brush_models,
    }
}

pub(crate) fn snapshot_input_key(vmf: &Path, context: u64, bake: u64) -> Option<u64> {
    let bytes = std::fs::read(vmf).ok()?;
    let mut h = hp_compile::stable_hash::StableHasher::new();
    std::hash::Hasher::write_u64(&mut h, context);
    std::hash::Hasher::write_u64(&mut h, bake);
    std::hash::Hasher::write(&mut h, &bytes);
    Some(h.finish_u64())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DryRun {
    pub choice: Option<Choice>,
    pub estimate_ms: Option<u64>,
}

pub(crate) fn prepare_snapshot(mut input: SnapshotInput) -> SnapshotOutput {
    let mut messages: Vec<(LogSeverity, String)> = Vec::new();
    let fail = |messages: Vec<(LogSeverity, String)>, e: String| SnapshotOutput {
        result: Err(PrepError::Failed(e)),
        dry: None,
        messages,
    };
    let t0 = Instant::now();
    let (materials, models) = used_assets(&input.snapshot.map);

    if input.mounting {
        let unresolved = match &input.fs {
            None => !materials.is_empty(),
            Some(fs) => materials.iter().any(|m| fs.material_source(m).is_none()),
        };
        if unresolved {
            return SnapshotOutput { result: Err(PrepError::NeedsMount(Box::new(input))), dry: None, messages };
        }
    }

    let s = &input.settings;
    let Some(root) = input.gmod_root.clone().or_else(hp_fs::detect_gmod_root) else {
        return fail(messages, t!("compile.err_no_game").to_owned());
    };
    let t_tools = Instant::now();
    let resolved = match super::tools::resolve(&root, s) {
        Ok(r) => r,
        Err(e) => return fail(messages, e),
    };
    let tools_ms = t_tools.elapsed().as_millis();
    messages.extend(resolved.messages.iter().cloned());
    let tools = resolved.tools.clone();
    let missing = tools.missing_files();
    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(|p| p.display().to_string()).collect();
        return fail(messages, tf!("compile.err_missing_files", list.join(", ")));
    }
    let external = if input.background || input.release {
        None
    } else {
        match super::compilers::choice(s, &tools) {
            super::compilers::Choice::Builtin => None,
            super::compilers::Choice::External(p) => Some(p),
            super::compilers::Choice::Missing(id) => {
                messages.push((LogSeverity::Warning, tf!("compilers.missing_choice", id)));
                None
            }
        }
    };
    let signature = external.as_ref().map(super::compilers::signature);

    let t_fp = Instant::now();
    let fingerprint =
        with_lights_rad(to_compile_fp(hp_editor::fingerprint::fingerprint_map(&input.snapshot.map)), &tools.game_dir);
    let fp_ms = t_fp.elapsed().as_millis();

    let t_mat = Instant::now();
    let vmf = input.target.vmf();
    let (compile_game_dir, staged) = plan::stage_for_compilers(
        s,
        input.fs.as_ref(),
        &tools,
        (&materials, &models),
        &input.content_dir,
        &mut messages,
    );
    let base = if input.release { preset_of(CompilePresetSetting::Final).modes() } else { effective_modes(s) };
    let content_args = plan::content_args(input.fs.as_ref());
    let context = context_hash(s, base, &tools, &staged, &tools.game_dir, &content_args, signature.as_deref());
    let smart = s.smart_recompile
        && external.is_none()
        && !input.release
        && !input.background
        && matches!(s.preset, CompilePresetSetting::Draft | CompilePresetSetting::Normal)
        && base.bsp == BspMode::Full;
    let choice = smart.then(|| {
        let bsp = vmf.with_extension("bsp");
        incremental::choose(input.previous.as_ref(), &fingerprint, base, context, incremental::file_stamp(&bsp))
    });
    let entries = input.history_file.as_deref().map(history::load).unwrap_or_default();

    if input.dry_run {
        let estimate_ms = choice.and_then(|c| estimate_modes(&entries, &c.modes));
        return SnapshotOutput {
            result: Err(PrepError::Failed(String::new())),
            dry: Some(DryRun { choice, estimate_ms }),
            messages,
        };
    }

    if let Some(fs) = &input.fs {
        messages.extend(plan::resized_image_messages(fs, &materials));
    }
    let packs = packs_content(s, input.release) && !input.background;
    let gmod = s.target.is_none();
    if !packs
        && gmod
        && s.dev_addon
        && !input.background
        && let Some(fs) = &input.fs
        && super::devaddon::has_custom_content(fs)
    {
        {
            let roots = pack_roots(&input.snapshot.map);
            let content = super::devaddon::DevContent {
                materials: if s.export_custom_materials { &[] } else { &materials },
                models: &models,
                roots: &roots,
                map: &input.target.name,
            };
            match super::devaddon::export(fs, &tools.game_dir, &content) {
                Ok(r) => {
                    if r.written > 0 {
                        messages
                            .push((LogSeverity::Info, tf!("compile.dev_addon", r.written, super::devaddon::DEV_ADDON)));
                    }
                    for (path, error) in r.failed {
                        messages.push((LogSeverity::Warning, tf!("compile.export_failed", path, error)));
                    }
                }
                Err(e) => messages.push((LogSeverity::Warning, tf!("compile.dev_addon_failed", e))),
            }
        }
    }
    if gmod && !input.background && !input.release && hp_editor::sky::map_uses_switches(&input.snapshot.map) {
        if !s.dev_addon {
            messages.push((LogSeverity::Warning, t!("compile.skybox_switch_no_dev_addon").to_owned()));
        } else if !hp_compile::sandbox::is_sandboxed() {
            match super::devaddon::export_switch_lua(&tools.game_dir) {
                Ok(true) => {
                    messages.push((LogSeverity::Info, tf!("compile.skybox_switch_lua", super::devaddon::DEV_ADDON)))
                }
                Ok(false) => {}
                Err(e) => messages.push((LogSeverity::Warning, tf!("compile.dev_addon_failed", e))),
            }
        }
    }
    let mat_ms = t_mat.elapsed().as_millis();

    if (external.is_some() || s.bsp_beside_vmf)
        && let Some(dir) = input.target.source.as_deref().and_then(Path::parent)
    {
        let n = super::compilers::point_instances_at_the_map_folder(&mut input.snapshot.map, dir);
        if n > 0 {
            messages.push((LogSeverity::Info, tf!("compilers.instances_fixed", n)));
        }
    }
    let t_write = Instant::now();
    if let Err(e) = input.snapshot.write(&input.target.build_dir, &input.target.name) {
        return fail(messages, tf!("compile.err_snapshot", input.target.build_dir.display(), e));
    }
    for ext in ["lin", "pts"] {
        let _ = std::fs::remove_file(input.target.vmf().with_extension(ext));
    }
    let write_ms = t_write.elapsed().as_millis();
    if input.snapshot.is_cordoned() && !input.background {
        messages.push((
            LogSeverity::Info,
            tf!(
                "compile.cordon_compiled",
                input.snapshot.cordon_boxes.len(),
                input.snapshot.cordoned_out,
                input.target.name
            ),
        ));
        if input.snapshot.added_player_start {
            messages.push((LogSeverity::Info, t!("compile.cordon_player_start").to_owned()));
        }
    }
    if input.snapshot.hidden > 0 && !input.background {
        let n = input.snapshot.hidden;
        messages.push((LogSeverity::Info, tf!(plural(n, "compile.hidden_skipped_one", "compile.hidden_skipped"), n)));
    }
    if input.snapshot.hidden_lights > 0 && !input.background {
        let n = input.snapshot.hidden_lights;
        messages.push((LogSeverity::Warning, tf!(plural(n, "compile.hidden_lights_one", "compile.hidden_lights"), n)));
    }

    if let Some(profile) =
        super::expert::active_profile(s).filter(|_| !input.background && !input.release && external.is_none())
    {
        let mut plan = match super::expert::plan_of(profile, &input.target, &tools) {
            Ok(p) => p,
            Err(e) => return fail(messages, e),
        };
        messages.push((LogSeverity::Info, tf!("compile.expert.running", profile.name)));
        publish_bsp_step(&mut plan, &input.target, s);
        let workdir = plan::work_dir(&plan.vmf_path);
        return SnapshotOutput {
            result: Ok(Prepared {
                plan,
                workdir,
                target: input.target,
                record: BuildRecord { fingerprint, context, bsp_size: 0, bsp_mtime_ns: 0 },
                choice: None,
                pack: None,
                release: false,
            }),
            dry: None,
            messages,
        };
    }

    let mut settings = s.clone();
    let mut modes = choice.map_or(base, |c| c.modes);
    if modes.bsp == BspMode::Off && modes.vis == VisMode::Off && modes.rad == RadMode::Off {
        modes = base;
    }
    if input.background {
        modes = StepModes { bsp: BspMode::Full, vis: base.vis, rad: RadMode::Off };
        settings.low_priority = true;
        settings.copy_to_maps = false;
        settings.launch_game = false;
        settings.stop_on_leak = true;
    }
    let (deferred, pack) = if packs {
        match &input.fs {
            Some(fs) => {
                let (files, sender) = DeferredFiles::new();
                let job = PackJob {
                    sender,
                    materials: materials.clone(),
                    models: models.clone(),
                    roots: pack_roots(&input.snapshot.map),
                    map: input.target.name.clone(),
                    fs: fs.clone(),
                    cache_dir: input.cache_dir.clone(),
                };
                (Some(files), Some(job))
            }
            None => {
                messages.push((LogSeverity::Warning, t!("compile.no_content").to_owned()));
                (None, None)
            }
        }
    } else {
        (None, None)
    };
    if input.release {
        settings.preset = CompilePresetSetting::Final;
    }
    if modes.rad == RadMode::Draft && settings.lighting == hp_app::settings::LightingSetting::Both {
        settings.lighting = match hdr_level(&tools.game_dir) {
            Some(level) if level >= 2 => hp_app::settings::LightingSetting::Hdr,
            _ => hp_app::settings::LightingSetting::Ldr,
        };
    }
    let extras = PlanExtras {
        modes: Some(modes),
        deferred_pack: deferred,
        launch_flags: input.launch_flags.clone(),
        content_args,
    };
    let mut plan = match plan::build_plan_with(&settings, &tools, &vmf, Vec::new(), compile_game_dir, extras) {
        Ok(p) => p,
        Err(e) => return fail(messages, super::notices::plan_error_text(&e)),
    };
    if plan.enabled_steps().next().is_none() {
        return fail(messages, t!("compile.err_no_steps").to_owned());
    }
    if let Some(profile) = &external {
        plan = match super::compilers::external_plan(
            plan,
            profile,
            &settings,
            &tools,
            plan::effective_lighting(&settings),
        ) {
            Ok(p) => p,
            Err(e) => return fail(messages, e),
        };
        messages.push((LogSeverity::Info, tf!("compilers.running", super::compilers::display_name(profile))));
    }
    if external.is_none()
        && let Some(dir) = &input.builds_dir
        && let Some(key) = snapshot_input_key(&vmf, context, fingerprint.bake)
    {
        let map_key = hp_app::sidecar::map_key(input.target.source.as_deref());
        plan = plan.with_cache(key, dir.clone(), &map_key, 0);
    }
    if !input.background {
        publish_bsp_step(&mut plan, &input.target, s);
    }
    let estimate_ms = estimate(&entries, &plan);
    if let Some(c) = &choice {
        messages.push((LogSeverity::Info, auto_text(c.kind, estimate_ms, s.copy_to_maps || s.launch_game)));
    }
    messages.push((
        LogSeverity::Info,
        tf!(
            "compile.prepared_in",
            input.snapshot_ms + u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX),
            input.snapshot_ms,
            write_ms,
            fp_ms,
            tools_ms,
            mat_ms
        ),
    ));
    let workdir = plan::work_dir(&plan.vmf_path);
    SnapshotOutput {
        result: Ok(Prepared {
            plan,
            workdir,
            target: input.target,
            record: BuildRecord { fingerprint, context, bsp_size: 0, bsp_mtime_ns: 0 },
            choice,
            pack,
            release: input.release,
        }),
        dry: None,
        messages,
    }
}

pub(super) fn publish_bsp_step(plan: &mut CompilePlan, target: &Target, settings: &CompileSettings) {
    if !settings.bsp_beside_vmf {
        return;
    }
    let Some(dir) = target.source.as_deref().and_then(Path::parent) else { return };
    let step = hp_compile::Step::copy(StepKind::Custom, plan.bsp_path(), dir.join(format!("{}.bsp", target.name)));
    let at = plan.steps.iter().position(|s| s.kind == StepKind::LaunchGame).unwrap_or(plan.steps.len());
    plan.steps.insert(at, step);
}

fn hdr_level(game_dir: &Path) -> Option<u32> {
    use std::io::Read as _;
    let f = std::fs::File::open(game_dir.join("cfg").join("config.cfg")).ok()?;
    let mut bytes = Vec::new();
    f.take(512 * 1024).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    text.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix("mat_hdr_level")?;
        rest.trim().trim_matches('"').trim().parse().ok()
    })
}

fn estimate_modes(entries: &[HistoryEntry], modes: &StepModes) -> Option<u64> {
    let mut steps: Vec<(StepKind, &str)> = Vec::new();
    for kind in [StepKind::Bsp, StepKind::Rad] {
        let name = mode_name(kind, modes);
        if !name.is_empty() {
            steps.push((kind, name));
        }
    }
    if steps.is_empty() {
        return Some(0);
    }
    history::estimate_ms(entries, &steps)
}

pub(crate) fn run_pack_job(job: PackJob) -> usize {
    let files = job.fs.pack_list_with(&job.materials, &job.models, &job.roots, &job.map, &job.cache_dir);
    let files: Vec<(String, PathBuf)> = files
        .into_iter()
        .filter(|(_, disk)| !disk.components().any(|c| c.as_os_str().eq_ignore_ascii_case(super::devaddon::DEV_ADDON)))
        .collect();
    let n = files.len();
    job.sender.send(files);
    n
}

pub(crate) fn auto_text(kind: AutoKind, estimate_ms: Option<u64>, after: bool) -> String {
    use hp_compile::incremental::FullReason;
    let what = match kind {
        AutoKind::UpToDate if !after => t!("compile.auto.nothing"),
        AutoKind::UpToDate => t!("compile.auto.up_to_date"),
        AutoKind::EntitiesOnly => t!("compile.auto.entities"),
        AutoKind::PropsOnly => t!("compile.auto.props"),
        AutoKind::LightingOnly => t!("compile.auto.lighting"),
        AutoKind::DetailProps => t!("compile.auto.detail_props"),
        AutoKind::Full(FullReason::NoPreviousBuild) => t!("compile.auto.full_first"),
        AutoKind::Full(FullReason::SettingsChanged) => t!("compile.auto.full_settings"),
        AutoKind::Full(FullReason::BspChanged) => t!("compile.auto.full_bsp"),
        AutoKind::Full(FullReason::GeometryChanged) => t!("compile.auto.full_geometry"),
    };
    match estimate_ms {
        Some(ms) if ms > 0 => tf!("compile.auto_about", what, crate::util::fmt_duration_ms(ms)),
        _ => tf!("compile.auto", what),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_editor::compile_export::CompileExport;

    #[test]
    fn targets() {
        crate::register_translations();
        let dir = std::path::absolute(crate::test_scratch("compile_targets")).expect("absolute");
        let t = target_for(Some(&dir.join("gm_room.vmf")), false, &dir.join("untitled"), false).expect("target");
        assert_eq!(t.name, "gm_room");
        assert_eq!(t.build_dir, dir.join(BUILD_DIR));
        assert_eq!(t.vmf(), dir.join(BUILD_DIR).join("gm_room.vmf"));
        let c = target_for(Some(&dir.join("gm_room.vmf")), true, &dir, false).expect("target");
        assert_eq!(c.name, "gm_room_cordon");
        let u = target_for(None, false, &dir.join("untitled"), false).expect("untitled");
        assert_eq!((u.name.as_str(), u.source.is_none()), ("untitled", true));
        assert!(target_for(Some(&dir.join("my map.vmf")), false, &dir, false).is_err());
        assert!(target_for(None, false, Path::new("C:/Users/J\u{e9}r\u{f4}me/cache"), false).is_ok());
    }

    #[test]
    fn beside_vmf_keeps_the_source_safe_and_publishes_before_launch() {
        let dir = std::path::absolute(crate::test_scratch("compile_beside_vmf")).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("gm_room.vmf");
        let original = b"world { \"id\" \"1\" \"classname\" \"worldspawn\" }";
        std::fs::write(&source, original).unwrap();
        let target = target_for(Some(&source), false, &dir, true).unwrap();
        assert_ne!(target.vmf(), source);
        assert!(!target.build_dir.starts_with(&dir));
        let other = target_for(Some(&dir.join("other/gm_room.vmf")), false, &dir, true).unwrap();
        assert_ne!(target.build_dir, other.build_dir);
        let mut plan = CompilePlan::new(target.vmf());
        plan.steps.push(hp_compile::Step::new(StepKind::LaunchGame, "game.exe", Vec::new()));
        let settings = CompileSettings { bsp_beside_vmf: true, ..CompileSettings::default() };
        publish_bsp_step(&mut plan, &target, &settings);
        assert!(
            matches!(&plan.steps[0].action,hp_compile::StepAction::Copy { from, to } if *from == target.vmf().with_extension("bsp") && *to == source.with_extension("bsp"))
        );
        assert_eq!(plan.steps[1].kind, StepKind::LaunchGame);
        let snapshot = hp_app::testing::test_state().doc.compile_snapshot(&CompileExport::default());
        let temporary = crate::test_scratch("beside_snapshot");
        snapshot.write(&temporary, &target.name).unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }

    #[test]
    fn used_assets_skip_hidden_objects() {
        let text = r#"
world { "id" "1" "classname" "worldspawn"
  solid { "id" "2" side { "id" "1" "plane" "(0 0 0) (1 0 0) (0 1 0)" "material" "Custom\Brick" } }
  hidden { solid { "id" "3" side { "id" "2" "plane" "(0 0 0) (1 0 0) (0 1 0)" "material" "HIDDEN/MAT" } } }
}
entity { "id" "4" "classname" "info_overlay" "material" "decals/Graffiti" }
entity { "id" "5" "classname" "prop_static" "model" "Models\Props\Box.mdl" }
hidden { entity { "id" "6" "classname" "prop_static" "model" "models/hidden.mdl" } }
"#;
        let map = hp_vmf::parse(text).expect("parse");
        let (mats, models) = used_assets(&map);
        assert_eq!(mats, vec!["custom/brick".to_owned(), "decals/graffiti".to_owned()]);
        assert_eq!(models, vec!["models/props/box.mdl".to_owned()]);
    }

    #[test]
    fn pack_roots_cover_every_file_kind_of_the_shown_entities() {
        use hp_fs::deps::RootKind as K;
        let map = hp_vmf::parse(
            r#"world { "id" "1" "classname" "worldspawn" "skyname" "sky_w10" }
entity { "id" "2" "classname" "env_projectedtexture" "texturename" "effects/w10_flash" }
entity { "id" "3" "classname" "info_particle_system" "effect_name" "w10_fire" }
entity { "id" "4" "classname" "env_soundscape" "soundscape" "W10.Cave" }
entity { "id" "5" "classname" "ambient_generic" "message" "W10.Beep" }
entity { "id" "6" "classname" "color_correction" "filename" "materials/correction/w10.raw" }
entity { "id" "7" "classname" "prop_physics" "model" "models/w10/cube.mdl" "gibmodel" "models/w10/gib.mdl" }
entity { "id" "8" "classname" "env_sprite" "model" "sprites/glow01.vmt" }
hidden { entity { "id" "9" "classname" "ambient_generic" "message" "W10.Hidden" } }
"#,
        )
        .unwrap();
        let mut roots: Vec<(K, String)> = pack_roots(&map).into_iter().map(|r| (r.kind, r.name)).collect();
        roots.sort();
        let want = [
            (K::Texture, "effects/w10_flash"),
            (K::Model, "models/w10/cube.mdl"),
            (K::Model, "models/w10/gib.mdl"),
            (K::Sound, "W10.Beep"),
            (K::Particle, "w10_fire"),
            (K::Soundscape, "W10.Cave"),
            (K::Skybox, "sky_w10"),
            (K::File, "materials/correction/w10.raw"),
        ];
        let want: Vec<(K, String)> = want.iter().map(|(k, n)| (*k, (*n).to_owned())).collect();
        assert_eq!(roots, want);
    }

    #[test]
    fn effect_materials_are_included_in_compile_staging_and_packing_inputs() {
        let map = hp_vmf::parse(
            r#"world { "id" "1" "classname" "worldspawn" }
entity { "id" "2" "classname" "func_smokevolume" "Material" "CSGO/Particle/particle_smokegrenade" }
entity { "id" "3" "classname" "env_smokestack" "SmokeMaterial" "custom/embers.vmt" }
entity { "id" "4" "classname" "env_sprite" "model" "sprites/glow01.vmt" }
hidden { entity { "id" "5" "classname" "func_smokevolume" "material" "hidden/smoke" } }
"#,
        )
        .unwrap();
        let (materials, _) = used_assets(&map);
        assert_eq!(materials, ["csgo/particle/particle_smokegrenade", "custom/embers", "sprites/glow01"]);
    }

    #[test]
    fn hdr_level_from_the_game_config() {
        let dir = crate::test_scratch("snapshot_hdr");
        assert_eq!(hdr_level(&dir), None);
        std::fs::create_dir_all(dir.join("cfg")).expect("cfg");
        std::fs::write(dir.join("cfg/config.cfg"), "bind \"w\" \"+forward\"\nmat_hdr_level \"2\"\n").expect("cfg");
        assert_eq!(hdr_level(&dir), Some(2));
        std::fs::write(dir.join("cfg/config.cfg"), "mat_hdr_level 0").expect("cfg");
        assert_eq!(hdr_level(&dir), Some(0));
    }

    #[test]
    fn sidecar_records() {
        let mut sc = CompileSidecar::default();
        assert!(sc.record("gm_x").is_none());
        let r = BuildRecord { context: 5, bsp_size: 7, ..BuildRecord::default() };
        sc.builds.insert("gm_x".into(), r.encode());
        sc.builds.insert("gm_y".into(), "garbage".into());
        assert_eq!(sc.record("gm_x"), Some(r));
        assert!(sc.record("gm_y").is_none());
        let text = toml::to_string(&sc).expect("toml");
        let back: CompileSidecar = toml::from_str(&text).expect("toml back");
        assert_eq!(back, sc);
    }

    #[test]
    fn preparation_without_game_fails_cleanly() {
        crate::register_translations();
        let dir = crate::test_scratch("snapshot_no_game");
        let doc = hp_editor::Document::new_empty();
        let input = SnapshotInput {
            snapshot: doc.compile_snapshot(&CompileExport::default()),
            target: target_for(Some(&dir.join("gm_x.vmf")), false, &dir, false).expect("target"),
            settings: CompileSettings::default(),
            gmod_root: Some(dir.join("no_such_install")),
            fs: None,
            mounting: false,
            previous: None,
            cache_dir: dir.join("cache"),
            content_dir: dir.join("content"),
            builds_dir: None,
            dry_run: false,
            history_file: None,
            release: false,
            background: false,
            snapshot_ms: 0,
            launch_flags: Vec::new(),
        };
        let out = prepare_snapshot(input);
        assert!(matches!(out.result, Err(PrepError::Failed(_))));
        assert!(!dir.join("gm_x.vmf").exists(), "the user's map is never written");
    }

    #[test]
    fn stale_pointfiles_of_the_target_are_removed() {
        crate::register_translations();
        let dir = crate::test_scratch("snapshot_stale_lin");
        let root = dir.join("gmod");
        let game = root.join("garrysmod");
        std::fs::create_dir_all(&game).expect("game");
        std::fs::write(game.join("gameinfo.txt"), "\"GameInfo\" {}").expect("gameinfo");
        std::fs::write(root.join("hl2.exe"), b"MZ").expect("exe");
        let untitled = dir.join("untitled");
        std::fs::create_dir_all(&untitled).expect("untitled");
        for ext in ["lin", "pts"] {
            std::fs::write(
                untitled.join(format!("untitled.{ext}")),
                "0 0 0
64 0 0
",
            )
            .expect("stale");
        }
        std::fs::write(
            untitled.join("other.lin"),
            "0 0 0
",
        )
        .expect("other");
        let doc = hp_editor::Document::new_empty();
        let input = SnapshotInput {
            snapshot: doc.compile_snapshot(&CompileExport::default()),
            target: target_for(None, false, &untitled, false).expect("target"),
            settings: CompileSettings::default(),
            gmod_root: Some(root),
            fs: None,
            mounting: false,
            previous: None,
            cache_dir: dir.join("cache"),
            content_dir: dir.join("content"),
            builds_dir: None,
            dry_run: false,
            history_file: None,
            release: false,
            background: false,
            snapshot_ms: 0,
            launch_flags: Vec::new(),
        };
        let _ = prepare_snapshot(input);
        assert!(untitled.join("untitled.vmf").is_file(), "the snapshot is written");
        assert!(!untitled.join("untitled.lin").exists() && !untitled.join("untitled.pts").exists());
        assert!(untitled.join("other.lin").is_file(), "only the pointfiles of this target");
    }

    #[test]
    fn mounting_waits_only_when_needed() {
        let dir = crate::test_scratch("snapshot_mounting");
        let doc = hp_editor::Document::new_empty();
        let make = |fs: Option<hp_fs::GameFs>| SnapshotInput {
            snapshot: doc.compile_snapshot(&CompileExport::default()),
            target: target_for(Some(&dir.join("gm_x.vmf")), false, &dir, false).expect("target"),
            settings: CompileSettings::default(),
            gmod_root: Some(dir.join("no_such_install")),
            fs,
            mounting: true,
            previous: None,
            cache_dir: dir.join("cache"),
            content_dir: dir.join("content"),
            builds_dir: None,
            dry_run: false,
            history_file: None,
            release: false,
            background: false,
            snapshot_ms: 0,
            launch_flags: Vec::new(),
        };
        assert!(matches!(prepare_snapshot(make(None)).result, Err(PrepError::Failed(_))));
        let mut d2 = hp_editor::Document::new_empty();
        d2.create_primitive(&hp_geom::PrimitiveParams::default(), hp_editor::Owner::World).expect("brush");
        let mut input = make(None);
        input.snapshot = d2.compile_snapshot(&CompileExport::default());
        assert!(matches!(prepare_snapshot(input).result, Err(PrepError::NeedsMount(_))));
    }

    #[test]
    fn estimates_and_labels() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let mut plan = CompilePlan::new("C:/maps/gm_x.vmf");
        plan.modes = StepModes::new(BspMode::OnlyEnts, VisMode::Off, RadMode::Off);
        plan.steps.push(hp_compile::Step::new(StepKind::Bsp, "hammerpro.exe", Vec::new()));
        let entry = HistoryEntry {
            steps: vec![history::StepTime { kind: StepKind::Bsp, mode: "onlyents".into(), ms: 900, ok: true }],
            ..HistoryEntry::default()
        };
        assert_eq!(estimate(&[entry], &plan), Some(900));
        assert_eq!(estimate(&[], &plan), None);
        assert_eq!(auto_text(AutoKind::EntitiesOnly, Some(1200), true), "Auto: entities only (about 1.2 s)");
        assert_eq!(auto_text(AutoKind::EntitiesOnly, None, true), "Auto: entities only");
        assert_eq!(auto_text(AutoKind::UpToDate, Some(0), false), "Auto: up to date, nothing to do");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        assert!(auto_text(AutoKind::LightingOnly, Some(61_000), true).starts_with("Auto : "));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }
}
