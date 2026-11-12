use crossbeam_channel::Receiver;
use hp_app::runtime::CombineRequest;
use hp_app::{AppState, Severity, t, tf};
use hp_ui_mapping::propper::PreparedProp;

use crate::app::HammerProApp;

type Built = Result<(Box<CombineRequest>, PreparedProp), (Box<CombineRequest>, String)>;

pub(crate) struct CombineJob {
    rx: Receiver<(Vec<(Severity, String)>, Built)>,
}

fn severity(s: hp_compile::Severity) -> Severity {
    match s {
        hp_compile::Severity::Info => Severity::Info,
        hp_compile::Severity::Success => Severity::Success,
        hp_compile::Severity::Warning => Severity::Warning,
        hp_compile::Severity::Error => Severity::Error,
    }
}

pub(crate) fn build_blocking(
    fs: &hp_fs::GameFs,
    request: Box<CombineRequest>,
    exe: &std::path::Path,
    args: &str,
) -> (Vec<(Severity, String)>, Built) {
    let mut lines = Vec::new();
    let result = hp_ui_mapping::propper::combine::build(fs, request.creation.clone(), exe, args, &mut |s, line| {
        lines.push((severity(s), line))
    });
    let built = match result {
        Ok(prepared) => Ok((request, prepared)),
        Err(e) => Err((request, e)),
    };
    (lines, built)
}

pub(crate) fn apply(state: &mut AppState, request: &CombineRequest, prepared: &PreparedProp) -> Result<u32, String> {
    let game = state.assets.fs().and_then(|fs| fs.game_dir());
    if game != Some(prepared.game_dir.as_path()) && !state.settings.game.extra_game_dirs.contains(&prepared.game_dir) {
        state.settings.game.extra_game_dirs.push(prepared.game_dir.clone());
        state.settings_dirty = true;
    }
    state.assets.remount(&state.settings);
    state
        .doc
        .combine_props(&request.props, &prepared.model, prepared.origin, &request.keys, request.keep_originals)
        .map_err(|e| tf!("propper.combine_replace_failed", e.to_string()))
}

pub(crate) fn start(app: &mut HammerProApp, ctx: &egui::Context, job: &mut Option<CombineJob>) {
    if job.is_some() {
        return;
    }
    let Some(request) = app.state.runtime.lab.combine_request.take() else { return };
    if hp_compile::sandbox::is_sandboxed() {
        app.state.notify(Severity::Warning, t!("lab.prop.sandboxed"));
        return;
    }
    let exe = match hp_app::model_compiler::resolve(&app.state.settings) {
        Ok(exe) => exe,
        Err(e) => {
            app.state.notify(Severity::Warning, e);
            return;
        }
    };
    let Some(fs) = app.state.assets.fs().cloned() else {
        app.state.notify(Severity::Warning, t!("lab.prop.no_game"));
        return;
    };
    let args = app.state.settings.compile.model_compiler_args.clone();
    let (tx, rx) = crossbeam_channel::bounded(1);
    let repaint = ctx.clone();
    app.state.console.info(tf!("propper.combine_started", hp_app::model_compiler::name(&app.state.settings)));
    let spawned = std::thread::Builder::new().name("hp-combine-props".into()).spawn(move || {
        let _ = tx.send(build_blocking(&fs, request, &exe, &args));
        repaint.request_repaint();
    });
    match spawned {
        Ok(_) => {
            app.state.runtime.lab.combine_busy = true;
            *job = Some(CombineJob { rx });
        }
        Err(e) => app.state.notify(Severity::Error, e.to_string()),
    }
}

pub(crate) fn poll(app: &mut HammerProApp, job: &mut Option<CombineJob>) {
    let Some(j) = job.as_ref() else { return };
    let (lines, built) = match j.rx.try_recv() {
        Ok(v) => v,
        Err(crossbeam_channel::TryRecvError::Empty) => return,
        Err(crossbeam_channel::TryRecvError::Disconnected) => {
            *job = None;
            app.state.runtime.lab.combine_busy = false;
            app.state.notify(Severity::Error, t!("lab.prop.failed"));
            return;
        }
    };
    *job = None;
    app.state.runtime.lab.combine_busy = false;
    for (s, line) in lines {
        app.state.console.push(s, line);
    }
    match built {
        Ok((request, prepared)) => match apply(&mut app.state, &request, &prepared) {
            Ok(_) => hp_ui_mapping::propper::combine::report_done(&mut app.state, &request, &prepared.model),
            Err(e) => app.state.notify(Severity::Error, e),
        },
        Err((_, e)) => app.state.notify(Severity::Error, e),
    }
}
