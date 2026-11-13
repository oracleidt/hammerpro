use std::hash::{Hash, Hasher};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use hp_app::AppState;
use hp_app::runtime::render::DetailSplit;
use hp_bspc::materials::MaterialOptions;

const SETTLE: Duration = Duration::from_millis(400);
const THREADS: usize = 2;

struct Job {
    signature: u64,
    options: u64,
    rx: Receiver<Option<DetailSplit>>,
}

#[derive(Default)]
struct Worker {
    revision: Option<u64>,
    signature: u64,
    changed: Option<Instant>,
    job: Option<Job>,
    failed: Option<(u64, u64)>,
}

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

pub(crate) struct Setup {
    pub(crate) map: hp_vmf::Map,
    pub(crate) opts: hp_bspc::CompileOptions,
    pub(crate) materials: MaterialOptions,
    pub(crate) signature: u64,
    pub(crate) options: u64,
    pub(crate) external: bool,
}

fn map_name(state: &AppState) -> String {
    state.doc.path().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

pub(crate) fn options_key(state: &AppState) -> (Vec<String>, u64) {
    let args = hp_ui_mapping::bsp_compile_args(&state.settings.compile);
    let export = hp_ui_mapping::compile_export_of(&state.settings);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    args.hash(&mut h);
    format!("{:?}", export.visibility).hash(&mut h);
    export.cordons.hash(&mut h);
    export.seal_material.hash(&mut h);
    state.settings.compile.uses_builtin_compiler().hash(&mut h);
    map_name(state).hash(&mut h);
    (args, h.finish())
}

pub(crate) fn setup(state: &AppState) -> Setup {
    let (args, options) = options_key(state);
    let parsed = crate::bspc_cli::parse_bspc_args(&args).unwrap_or_default();
    let mut opts = parsed.opts.clone();
    opts.vis = hp_bspc::VisLevel::Off;
    opts.vis_cache = None;
    opts.threads = Some(THREADS);
    let name = map_name(state);
    opts.map_name = (!name.is_empty()).then(|| name.clone());
    if let Ok(cli) = crate::bspc_game::resolve(parsed.target.as_deref(), None) {
        opts.format = cli.format;
    }
    let export = hp_ui_mapping::compile_export_of(&state.settings);
    let mut map = state.doc.compile_snapshot(&export).map;
    if parsed.replace_materials
        && let Some(subs) = crate::bspc_cli::material_substitutions(state.assets.fs().and_then(|f| f.game_dir()), &name)
    {
        crate::bspc_cli::replace_materials(&mut map, &subs);
    }
    if let Some(level) = parsed.dx_level {
        crate::bspc_cli::apply_dx_level(&mut map, level);
    }
    Setup {
        map,
        opts,
        materials: parsed.materials,
        signature: hp_assets::detail_preview::split_signature(&state.doc),
        options,
        external: !state.settings.compile.uses_builtin_compiler(),
    }
}

pub(crate) fn run(setup: Setup, fs: hp_fs::GameFs) -> Result<DetailSplit, String> {
    let materials = crate::bspc_cli::GameMaterials { fs, opts: setup.materials };
    let mut split = hp_assets::detail_preview::split_of_map(&setup.map, setup.signature, &materials, &setup.opts)?;
    split.options = setup.options;
    split.external = setup.external;
    Ok(split)
}

fn needed(state: &AppState) -> bool {
    state.settings.render.detail_preview && state.runtime.render.detail_preview.brush_faces > 0
}

fn fresh(state: &AppState, signature: u64, options: u64) -> bool {
    state.runtime.render.detail_split.as_ref().is_some_and(|s| s.signature == signature && s.options == options)
}

pub(crate) fn split_now(state: &mut AppState) -> Option<Arc<DetailSplit>> {
    let signature = hp_assets::detail_preview::split_signature(&state.doc);
    let (_, options) = options_key(state);
    if fresh(state, signature, options) {
        return state.runtime.render.detail_split.clone();
    }
    let fs = state.assets.fs()?.clone();
    match run(setup(state), fs) {
        Ok(split) => {
            let split = Arc::new(split);
            state.runtime.render.detail_split = Some(Arc::clone(&split));
            Some(split)
        }
        Err(e) => {
            log::warn!("detail props preview: the faces could not be split like the compile: {e}");
            None
        }
    }
}

pub(crate) fn tick(state: &mut AppState, ctx: &egui::Context) {
    let mut guard = WORKER.lock().unwrap_or_else(PoisonError::into_inner);
    let w = guard.get_or_insert_with(Worker::default);
    if let Some(job) = &w.job {
        match job.rx.try_recv() {
            Ok(Some(split)) => {
                state.runtime.render.detail_split = Some(Arc::new(split));
                w.job = None;
            }
            Ok(None) => {
                w.failed = Some((job.signature, job.options));
                w.job = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => w.job = None,
        }
    }
    if !state.settings.render.detail_preview {
        state.runtime.render.detail_split = None;
        w.revision = None;
        return;
    }
    if !needed(state) {
        return;
    }
    let revision = state.doc.revision();
    if w.revision != Some(revision) {
        w.revision = Some(revision);
        w.signature = hp_assets::detail_preview::split_signature(&state.doc);
        w.changed = Some(Instant::now());
    }
    let (_, options) = options_key(state);
    if state.runtime.render.detail_split.as_ref().is_some_and(|s| s.options != options) {
        state.runtime.render.detail_split = None;
        w.changed = Some(Instant::now());
    }
    if fresh(state, w.signature, options) || w.failed == Some((w.signature, options)) {
        return;
    }
    if let Some(job) = &w.job {
        if job.signature == w.signature && job.options == options {
            return;
        }
        w.job = None;
    }
    if let Some(t) = w.changed {
        let waited = t.elapsed();
        if waited < SETTLE {
            ctx.request_repaint_after(SETTLE - waited);
            return;
        }
    }
    let Some(fs) = state.assets.fs().cloned() else { return };
    let setup = setup(state);
    let (signature, options) = (setup.signature, setup.options);
    let (tx, rx) = channel();
    let repaint = ctx.clone();
    let spawned = std::thread::Builder::new().name("hp-detail-split".to_owned()).spawn(move || {
        let result = run(setup, fs);
        if let Err(e) = &result {
            log::warn!("detail props preview: the faces could not be split like the compile: {e}");
        }
        let _ = tx.send(result.ok());
        repaint.request_repaint();
    });
    match spawned {
        Ok(_) => w.job = Some(Job { signature, options, rx }),
        Err(e) => log::warn!("detail props preview: cannot start the split thread: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_split_uses_the_compile_settings_and_follows_their_changes() {
        let mut state = hp_app::testing::test_state();
        let (args, key) = options_key(&state);
        assert!(!args.iter().any(|a| a == "-nosubdiv"));
        assert!(!setup(&state).opts.no_subdiv);
        state.settings.compile.extra_bsp_args = "-nosubdiv -nomerge".to_owned();
        let (args, changed) = options_key(&state);
        assert!(args.iter().any(|a| a == "-nosubdiv"), "{args:?}");
        assert_ne!(key, changed, "changing the compile options asks for a new split");
        let s = setup(&state);
        assert!(s.opts.no_subdiv && s.opts.no_merge);
        assert_eq!(s.opts.vis, hp_bspc::VisLevel::Off);
        assert!(!s.external);
        state.settings.compile.extra_bsp_args.clear();
        state.settings.compile.expert = true;
        let mut profile = hp_app::settings::CompileProfile::default();
        profile.steps.push(hp_app::settings::ProfileStep {
            enabled: true,
            action: "bsp".to_owned(),
            program: String::new(),
            args: "-nosubdiv -game $gamedir $path/$file".to_owned(),
            continue_on_error: false,
        });
        state.settings.compile.profiles = vec![profile];
        state.settings.compile.active_profile = 0;
        let s = setup(&state);
        assert!(s.opts.no_subdiv, "the active expert profile gives the options");
        let old = Arc::new(DetailSplit { signature: s.signature, options: key, ..DetailSplit::default() });
        state.runtime.render.detail_split = Some(old);
        assert!(!fresh(&state, s.signature, s.options), "a split made with other options is not fresh");
    }
}
