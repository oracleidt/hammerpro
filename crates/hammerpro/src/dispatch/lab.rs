use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::i18n::TrTable;
use hp_app::sidecar::Sidecar;
use hp_app::{Action, AppState, Severity, t, tf};
use hp_compile::{CompileEvent, CompileHandle};
use hp_ui_lab::bsp::{BspJob, BspResult};
use hp_ui_lab::nav::{NavHandle, NavJob};
use hp_ui_lab::particles::ParticleHandle;

use super::Package;
use crate::app::HammerProApp;
use crate::files;
use crate::prompts::{UnsavedAnswer, unsaved_modal};

pub(crate) struct Lab;

struct Decompiled {
    serial: u64,
    bsp: PathBuf,
    sources: Vec<u32>,
    dir: Option<PathBuf>,
}

type NavGenAnswer = Result<(PathBuf, hp_nav::GenReport, usize, u128), String>;

type PcfAnswer = Result<hp_ui_lab::particles::LoadedFile, String>;

#[derive(Default)]
pub(crate) struct State {
    logic: hp_ui_lab::logic::LogicalView,
    nav: NavHandle,
    nav_job: Option<NavJob>,
    nav_check: Option<Instant>,
    nav_map: Option<u64>,
    nav_failed: Option<(PathBuf, Option<std::time::SystemTime>)>,
    nav_announce: bool,
    nav_serial: u64,
    nav_pick: Option<Receiver<Option<PathBuf>>>,
    bsp_pick: Option<Receiver<Option<PathBuf>>>,
    bsp_job: Option<BspJob>,
    bsp_held: Option<BspResult>,
    decompiled: Vec<Decompiled>,
    env_checked: bool,
    particles: ParticleHandle,
    pcf_jobs: Vec<(String, Receiver<PcfAnswer>)>,
    pcf_queue: Vec<String>,
    pcf_done: std::collections::HashSet<String>,
    pcf_generation: u64,
    pcf_map: Option<((u64, u64), Vec<String>)>,
    pcf_report_key: Option<(u64, usize)>,
    particles_shown: bool,
    nav_gen: Option<Receiver<NavGenAnswer>>,
    prop_job: Option<PropJob>,
    compare_pick: Option<Receiver<Option<PathBuf>>>,
    compare_job: Option<hp_ui_lab::compare::CompareJob>,
    compare: hp_ui_lab::compare::CompareWindow,
    sniffer: hp_ui_lab::sniffer::SnifferWindow,
    sound_editor: hp_ui_lab::sound_edit::SoundEditorWindow,
    color_editor: hp_ui_lab::color_edit::ColorEditorWindow,
    color_zone: hp_ui_lab::color_zone::ZoneDriver,
    preview: Option<hp_ui_lab::particle_preview::ParticlePreviewWindow>,
    pub(crate) particle_editor: hp_ui_lab::particle_editor::EditorHandle,
    editor_window: Option<hp_ui_lab::particle_editor::ParticleEditorWindow>,
    pub(crate) model_import: hp_ui_entities::model_import::ModelImportDialog,
}

const HANDLED: &[Action] = &[
    Action::OpenBsp,
    Action::OpenBspSniffer,
    Action::OpenSoundEditor,
    Action::OpenColorCorrectionEditor,
    Action::ToggleColorCorrectionPreview,
    Action::ShowLogicalView,
    Action::ToggleNavMesh,
    Action::LoadNavMesh,
    Action::GenerateNavMesh,
    Action::PhysicsPlacement,
    Action::BrushesToProp,
    Action::PropToBrushes,
    Action::PropperOptions,
    Action::ToggleParticles,
    Action::ToggleParticlesSelected,
    Action::ToggleEntityFx,
    Action::ParticlePreview,
    Action::ParticleEditor,
    Action::FlyApplyTexture,
    Action::FlyApplyTextureBrush,
    Action::FlyPickTexture,
    Action::FlyAlignTexture,
    Action::CompareMaps,
    Action::EntityGallery,
    Action::ImportModel,
    Action::TogglePlayerView,
];

const MAX_PCF_JOBS: usize = 4;

const NAV_CHECK_EVERY: Duration = Duration::from_millis(1500);

fn pick_file(
    app: &HammerProApp,
    filter: String,
    ext: &'static [&'static str],
    ctx: &egui::Context,
) -> Receiver<Option<PathBuf>> {
    let start =
        app.state.doc.path().and_then(|p| p.parent()).map(PathBuf::from).or_else(|| {
            app.state.assets.fs().and_then(|fs| fs.game_dir().map(|g| g.join("maps"))).filter(|p| p.is_dir())
        });
    let (tx, rx) = crossbeam_channel::bounded(1);
    let mut dialog = rfd::FileDialog::new().add_filter(filter, ext);
    if let Some(s) = start {
        dialog = dialog.set_directory(s);
    }
    if cfg!(target_os = "macos") {
        let _ = tx.send(dialog.pick_file());
    } else {
        let spawned = std::thread::Builder::new().name("hp-lab-pick".into()).spawn(move || {
            let _ = tx.send(dialog.pick_file());
        });
        if spawned.is_err() {
            log::warn!("cannot start the file picker thread");
        }
    }
    ctx.request_repaint_after(Duration::from_millis(100));
    rx
}

fn poll_pick(slot: &mut Option<Receiver<Option<PathBuf>>>, ctx: &egui::Context) -> Option<PathBuf> {
    let rx = slot.as_ref()?;
    match rx.try_recv() {
        Ok(picked) => {
            *slot = None;
            picked
        }
        Err(crossbeam_channel::TryRecvError::Empty) => {
            ctx.request_repaint_after(Duration::from_millis(100));
            None
        }
        Err(crossbeam_channel::TryRecvError::Disconnected) => {
            *slot = None;
            None
        }
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn open_particle_editor(app: &mut HammerProApp) {
    let selected = app.state.runtime.lab.particle_preview.selected.clone().or_else(|| {
        hp_ui_lab::particle_preview::particle_entities(&app.state).into_iter().find_map(|id| {
            let node = app.state.doc.scene().entity(id)?;
            let name = node.entity.get("effect_name")?.trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
    });
    let source = selected.as_ref().and_then(|name| {
        app.state.assets.particles()?.systems.iter().find(|s| s.name.eq_ignore_ascii_case(name)).map(|s| s.file.clone())
    });
    let mut editor = app.pkg.lab.particle_editor.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    editor.open = true;
    let Some(name) = selected else { return };
    if let Some(id) = editor.doc.find_system(&name) {
        editor.select(id);
        return;
    }
    if editor.dirty() {
        return;
    }
    let Some(source) = source else { return };
    let Some(bytes) = app.state.assets.fs().and_then(|fs| fs.read(&source)) else { return };
    if bytes.len() > 64 * 1024 * 1024 {
        return;
    }
    match editor.load_bytes(&source, &bytes) {
        Ok(()) => {
            if let Some(id) = editor.doc.find_system(&name) {
                editor.select(id);
            }
        }
        Err(e) => app.state.notify(Severity::Warning, tf!("pe.operation_failed", e)),
    }
}

impl HammerProApp {
    pub(crate) fn flush_particle_edits(&mut self) {
        if let Some(window) = &mut self.pkg.lab.editor_window {
            window.flush_pending_edits();
        }
    }

    pub(crate) fn particle_document_dirty(&self) -> bool {
        self.pkg.lab.particle_editor.lock().unwrap_or_else(std::sync::PoisonError::into_inner).dirty()
    }
}

impl Package for Lab {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::ShowLogicalView => {
                app.state.runtime.lab.logic_open = !app.state.runtime.lab.logic_open;
            }
            Action::ToggleNavMesh => {
                let s = &mut app.state.settings.lab;
                s.show_nav_mesh = !s.show_nav_mesh;
                app.state.settings_dirty = true;
                if s.show_nav_mesh {
                    app.pkg.lab.nav_check = None;
                    app.pkg.lab.nav_announce = true;
                }
            }
            Action::ToggleParticles => {
                let s = &mut app.state.settings.lab;
                s.show_particles = !s.show_particles;
                app.state.settings_dirty = true;
            }
            Action::ToggleParticlesSelected => {
                let s = &mut app.state.settings.lab;
                s.particles_selected_only = !s.particles_selected_only;
                app.state.settings_dirty = true;
            }
            Action::ToggleEntityFx => {
                let s = &mut app.state.settings.lab;
                s.show_entity_fx = !s.show_entity_fx;
                app.state.settings_dirty = true;
            }
            Action::ParticlePreview => {
                let opening = !app.state.runtime.lab.particle_preview.open;
                if opening && app.state.runtime.lab.particle_preview.selected.is_none() {
                    let name = hp_ui_lab::particle_preview::particle_entities(&app.state).into_iter().find_map(|id| {
                        let node = app.state.doc.scene().entity(id)?;
                        let n = node.entity.get("effect_name")?.trim();
                        (!n.is_empty()).then(|| n.to_owned())
                    });
                    app.state.runtime.lab.particle_preview.selected = name;
                }
                app.state.runtime.lab.particle_preview.open = opening;
            }
            Action::ParticleEditor => {
                open_particle_editor(app);
            }
            Action::FlyApplyTexture
            | Action::FlyApplyTextureBrush
            | Action::FlyPickTexture
            | Action::FlyAlignTexture => {
                if let Some(op) = hp_ui_viewports::fly_texture::op_of(action) {
                    hp_ui_viewports::fly_texture::run(&mut app.state, op);
                }
            }
            Action::CompareMaps => {
                if app.pkg.lab.compare_pick.is_none() && app.pkg.lab.compare_job.is_none() {
                    app.pkg.lab.compare_pick =
                        Some(pick_file(app, t!("lab.compare.files").to_owned(), &["vmf", "vmx"], ctx));
                }
            }
            Action::EntityGallery => insert_gallery(app),
            Action::GenerateNavMesh => start_nav_gen(app, ctx),
            Action::BrushesToProp => {
                if app.pkg.lab.prop_job.is_none() {
                    app.state.runtime.lab.propper_create_confirmed = false;
                    app.state.runtime.lab.propper_create_open = true;
                }
            }
            Action::PropToBrushes => app.state.runtime.lab.unprop_open = true,
            Action::PropperOptions => {
                let open = &mut app.state.runtime.lab.propper_options_open;
                *open = !*open;
            }
            Action::ImportModel => app.pkg.lab.model_import.open(ctx),
            Action::TogglePlayerView => {
                let p = &mut app.state.runtime.lab.player;
                if p.active {
                    p.active = false;
                } else {
                    p.start_request = true;
                }
            }
            Action::PhysicsPlacement => {
                if app.require_selection() {
                    let objs = app.selected();
                    let state = &mut app.state;
                    let assets = &state.assets;
                    let result = state.doc.physics_place(&objs, true, &|e| assets.entity_bounds(e));
                    if let Some(0) = app.report(Action::PhysicsPlacement, result) {
                        app.state.notify(Severity::Info, t!("lab.physplace.nothing"));
                    }
                }
            }
            Action::LoadNavMesh => {
                if app.pkg.lab.nav_pick.is_none() {
                    app.pkg.lab.nav_pick = Some(pick_file(app, t!("lab.nav.files").to_owned(), &["nav"], ctx));
                }
            }
            Action::OpenSoundEditor => {
                let rt = &mut app.state.runtime.lab.sound_color;
                rt.sound_open = !rt.sound_open;
            }
            Action::OpenColorCorrectionEditor => {
                let rt = &mut app.state.runtime.lab.sound_color;
                rt.color_open = !rt.color_open;
            }
            Action::ToggleColorCorrectionPreview => {
                let s = &mut app.state.settings.lab;
                s.color_correction_preview = !s.color_correction_preview;
                app.state.settings_dirty = true;
            }
            Action::OpenBspSniffer => {
                let rt = &mut app.state.runtime.lab.sniffer;
                rt.open = !rt.open;
            }
            Action::OpenBsp => {
                if app.pkg.lab.bsp_pick.is_none() {
                    app.pkg.lab.bsp_pick = Some(pick_file(app, t!("lab.bsp.files").to_owned(), &["bsp"], ctx));
                }
            }
            _ => return false,
        }
        true
    }

    fn init(&self, app: &mut HammerProApp) {
        hp_app::i18n::register(hp_ui_lab::SNIFFER_TR);
        hp_app::i18n::register(hp_ui_lab::PARTICLE_PREVIEW_TR);
        hp_app::i18n::register(hp_ui_lab::particle_editor::TR);
        hp_app::i18n::register(hp_ui_lab::SOUND_TR);
        hp_app::i18n::register(hp_ui_lab::COLOR_TR);
        let old = content_cache(app).join("decompiled");
        if old.is_dir() {
            let _ = std::thread::Builder::new().name("hp-lab-clean".into()).spawn(move || {
                let week = Duration::from_secs(7 * 24 * 3600);
                let Ok(entries) = std::fs::read_dir(&old) else { return };
                for e in entries.flatten() {
                    let stale = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
                    if stale.is_some_and(|age| age > week) && e.path().is_dir() {
                        let _ = std::fs::remove_dir_all(e.path());
                    }
                }
            });
        }
        let handle = app.pkg.lab.nav.clone();
        app.viewports.add_layer(Box::new(hp_ui_lab::nav::NavLayer::new(handle)));
        let particles = app.pkg.lab.particles.clone();
        app.viewports.add_layer(Box::new(hp_ui_lab::particles::ParticleLayer::new(particles.clone())));
        app.viewports.add_layer(Box::new(hp_ui_lab::entity_fx::EntityFxLayer::new(particles)));
        app.viewports.set_logical_content(Box::new(hp_ui_lab::logic::LogicalQuadrants::default()));
        app.shell.add_status_segment(Box::new(NavStatus));
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        if !app.pkg.lab.env_checked {
            app.pkg.lab.env_checked = true;
            if !app.persist_settings
                && let Some(p) = std::env::var_os("HAMMERPRO_TEST_OPEN_BSP")
            {
                app.state.runtime.lab.open_bsp = Some(PathBuf::from(p));
            }
        }
        if let Some(path) = poll_pick(&mut app.pkg.lab.bsp_pick, ctx) {
            app.state.runtime.lab.open_bsp = Some(path);
        }
        if let Some(path) = poll_pick(&mut app.pkg.lab.nav_pick, ctx) {
            app.state.runtime.lab.nav_file = Some(path);
            if !app.state.settings.lab.show_nav_mesh {
                app.state.settings.lab.show_nav_mesh = true;
                app.state.settings_dirty = true;
            }
            app.pkg.lab.nav_check = None;
            app.pkg.lab.nav_failed = None;
            app.pkg.lab.nav_announce = true;
        }
        if let Some(path) = app.state.runtime.lab.open_bsp.take() {
            start_bsp(app, ctx, path);
        }
        hp_ui_lab::sniffer::pump(&mut app.state, ctx);
        if let Some(dir) = app.state.runtime.lab.sniffer.open_content.take()
            && let Some(fs) = app.state.assets.fs()
            && let Err(e) = fs.add_game_dir(&dir)
        {
            log::warn!("cannot mount {}: {e}", dir.display());
        }
        if let Some(path) = app.state.runtime.lab.sniffer.open_vmf.take() {
            app.request_open(ctx, path);
        }
        poll_nav_gen(app);
        if std::mem::take(&mut app.state.runtime.lab.propper_create_confirmed) {
            start_brushes_to_prop(app, ctx);
        }
        poll_prop_job(app, ctx);
        if let Some(path) = poll_pick(&mut app.pkg.lab.compare_pick, ctx) {
            app.state.log(Severity::Info, tf!("lab.compare.working", file_name(&path)));
            app.pkg.lab.compare_job =
                hp_ui_lab::compare::CompareJob::start(app.state.doc.to_map(), path, Some(ctx.clone()));
        }
        if let Some(job) = &app.pkg.lab.compare_job {
            match job.poll() {
                Some(Ok(diff)) => {
                    let path = job.path.clone();
                    app.pkg.lab.compare_job = None;
                    app.pkg.lab.compare.set_result(path, diff);
                }
                Some(Err(e)) => {
                    let name = file_name(&job.path);
                    app.pkg.lab.compare_job = None;
                    app.state.notify(Severity::Error, tf!("lab.compare.failed", name, e));
                }
                None => ctx.request_repaint_after(Duration::from_millis(100)),
            }
        }
        poll_bsp(app, ctx);
        nav_tick(app, ctx);
        particles_tick(app, ctx);
        app.pkg.lab.color_zone.update(&mut app.state, ctx);
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        let serial = app.state.sidecar.serial();
        app.state.runtime.lab.decompiled_from =
            app.pkg.lab.decompiled.iter().find(|d| d.serial == serial).map(|d| d.bsp.clone());
        let open: Vec<u64> =
            std::iter::once(serial).chain(app.tabs.parked().map(|(_, p)| p.sidecar.serial())).collect();
        let (keep, gone): (Vec<Decompiled>, Vec<Decompiled>) =
            std::mem::take(&mut app.pkg.lab.decompiled).into_iter().partition(|d| open.contains(&d.serial));
        app.pkg.lab.decompiled = keep;
        for d in gone {
            unmount(app, d);
        }
        if app.pkg.lab.nav_map != Some(serial) {
            app.pkg.lab.nav_map = Some(serial);
            app.state.runtime.lab.nav_file = None;
            app.state.runtime.lab.nav_summary = None;
            hp_ui_lab::nav::set_current(&app.pkg.lab.nav, None);
            app.pkg.lab.nav_job = None;
            app.pkg.lab.nav_check = None;
            app.pkg.lab.nav_failed = None;
        }
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.pkg.lab.logic.show(ctx, &mut app.state);
        app.pkg.lab.compare.show(ctx, &mut app.state);
        app.pkg.lab.sniffer.show(ctx, &mut app.state);
        app.pkg.lab.sound_editor.show(ctx, &mut app.state);
        app.pkg.lab.color_editor.show(ctx, &mut app.state);
        let editor = app.pkg.lab.particle_editor.clone();
        let particles = app.pkg.lab.particles.clone();
        app.pkg
            .lab
            .editor_window
            .get_or_insert_with(|| hp_ui_lab::particle_editor::ParticleEditorWindow::new(particles, editor))
            .show(ctx, &mut app.state);
        if app.state.runtime.lab.particle_preview.open || app.pkg.lab.preview.is_some() {
            let handle = app.pkg.lab.particles.clone();
            let window = app
                .pkg
                .lab
                .preview
                .get_or_insert_with(|| hp_ui_lab::particle_preview::ParticlePreviewWindow::new(handle));
            window.show(ctx, &mut app.state);
        }
        app.pkg.lab.model_import.show(ctx, &mut app.state);
        if let Some(imported) = app.pkg.lab.model_import.take_imported()
            && imported.place
        {
            place_imported(app, &imported);
        }
        if let Some(held) = &app.pkg.lab.bsp_held {
            let question = tf!("dialog.unsaved_body_open", files::map_display_name(&app.state), file_name(&held.path));
            match unsaved_modal(ctx, &question) {
                Some(UnsavedAnswer::Save) => {
                    if files::save_map(&mut app.state)
                        && let Some(result) = app.pkg.lab.bsp_held.take()
                    {
                        replace_with(app, result);
                    }
                }
                Some(UnsavedAnswer::DontSave) => {
                    if let Some(result) = app.pkg.lab.bsp_held.take() {
                        replace_with(app, result);
                    }
                }
                Some(UnsavedAnswer::Cancel) => app.pkg.lab.bsp_held = None,
                None => {}
            }
        }
    }

    fn translations(&self) -> TrTable {
        hp_ui_lab::TR
    }
}

fn insert_gallery(app: &mut HammerProApp) {
    let Some(fgd) = app.state.assets.fgd() else {
        app.state.notify(Severity::Info, t!("lab.gallery.none"));
        return;
    };
    let mut names: Vec<&str> = fgd.point_classes().iter().map(String::as_str).collect();
    names.sort_unstable_by_key(|n| n.to_ascii_lowercase());
    let mut largest = 32.0f64;
    let entries: Vec<hp_editor::gallery::GalleryEntry> = names
        .iter()
        .filter_map(|n| fgd.class(n))
        .filter(|c| !c.is_base() && c.is_point())
        .map(|c| {
            let (lo, hi) = c.bounds_or_default();
            for k in 0..3 {
                largest = largest.max((hi[k] - lo[k]).abs());
            }
            hp_editor::gallery::GalleryEntry { classname: c.name.clone(), keyvalues: c.default_keyvalues() }
        })
        .collect();
    let spacing = (largest * 1.5).clamp(128.0, 512.0);
    let top = app.state.cameras.top.center;
    let center = app.state.snap_vec(glam::DVec3::new(top.x, top.y, 0.0));
    let result = app.state.doc.insert_entity_gallery(&entries, center, spacing);
    if let Some(ids) = app.report(Action::EntityGallery, result) {
        app.state.notify(Severity::Success, tf!("lab.gallery.placed", ids.len()));
    }
}

fn content_cache(app: &HammerProApp) -> PathBuf {
    if app.persist_settings {
        hp_app::settings::Settings::cache_dir()
    } else {
        std::env::temp_dir().join("hammerpro_lab")
    }
}

fn start_bsp(app: &mut HammerProApp, ctx: &egui::Context, path: PathBuf) {
    if app.pkg.lab.bsp_job.is_some() || app.pkg.lab.bsp_held.is_some() {
        app.state.notify(Severity::Info, t!("lab.bsp.busy"));
        return;
    }
    let cache = content_cache(app);
    let outputs = hp_ui_lab::bsp::known_outputs(app.state.assets.fgd());
    app.state.log(Severity::Info, tf!("lab.bsp.working", file_name(&path)));
    app.state.runtime.lab.decompiling = Some(file_name(&path));
    match BspJob::start(path.clone(), Some(cache), outputs, Some(ctx.clone())) {
        Some(job) => app.pkg.lab.bsp_job = Some(job),
        None => app.state.notify(Severity::Error, tf!("lab.bsp.failed", file_name(&path), "no worker thread")),
    }
}

fn poll_bsp(app: &mut HammerProApp, ctx: &egui::Context) {
    let Some(job) = &app.pkg.lab.bsp_job else { return };
    let Some(result) = job.poll() else {
        ctx.request_repaint_after(Duration::from_millis(100));
        return;
    };
    let path = job.path.clone();
    app.pkg.lab.bsp_job = None;
    app.state.runtime.lab.decompiling = None;
    match result {
        Err(e) => app.state.notify(Severity::Error, tf!("lab.bsp.failed", file_name(&path), e)),
        Ok(result) => {
            if app.state.settings.workflow.open_in_new_tab && !crate::tabs::is_pristine(&app.state.doc) {
                app.new_tab();
                replace_with(app, result);
            } else if app.state.doc.is_dirty() {
                app.pkg.lab.bsp_held = Some(result);
            } else {
                replace_with(app, result);
            }
        }
    }
}

fn replace_with(app: &mut HammerProApp, result: BspResult) {
    let BspResult { path, doc, report, content_dir, extracted, elapsed } = result;
    app.deactivate_tool();
    files::flush_sidecar(&mut app.state);
    app.state.sidecar = Sidecar::open(app.state.sidecar.dir(), None);
    let old = std::mem::replace(&mut app.state.doc, doc);
    let _ = std::thread::Builder::new().name("hp-drop-map".into()).spawn(move || drop(old));
    files::apply_doc_options(&mut app.state);
    app.state.doc.mark_everything_changed();
    app.state.pointfile = None;
    files::frame_map(&mut app.state, app.central_size);
    app.activate_tool();
    app.state.runtime.lab.decompiled_from = Some(path.clone());
    let mut sources = Vec::new();
    if let Some(dir) = &content_dir
        && let Some(fs) = app.state.assets.fs()
    {
        match fs.add_game_dir(dir) {
            Ok(ids) => sources = ids,
            Err(e) => log::warn!("cannot mount {}: {e}", dir.display()),
        }
    }
    app.pkg.lab.decompiled.push(Decompiled {
        serial: app.state.sidecar.serial(),
        bsp: path.clone(),
        sources,
        dir: content_dir,
    });
    announce(&mut app.state, &path, &report, &extracted, elapsed);
}

fn announce(
    state: &mut AppState,
    path: &std::path::Path,
    r: &hp_decompile::Report,
    extracted: &hp_decompile::pak::Extracted,
    elapsed: Duration,
) {
    let solids = r.world_solids + r.detail_solids + r.entity_solids + r.areaportals + r.occluders;
    let entities = state.doc.scene().entities().len();
    state.notify(Severity::Success, tf!("lab.bsp.opened", file_name(path), elapsed.as_millis(), solids, entities));
    if r.failed_brushes > 0 {
        state.log(Severity::Warning, tf!("lab.bsp.failed_brushes", r.failed_brushes));
    }
    if r.empty_brushes > 0 {
        state.notify(Severity::Warning, tf!("lab.bsp.empty_brushes", r.empty_brushes));
    }
    if r.failed_displacements > 0 {
        state.log(Severity::Warning, tf!("lab.bsp.failed_disps", r.failed_displacements));
    }
    if r.failed_occluders > 0 {
        state.log(Severity::Warning, tf!("lab.bsp.failed_occluders", r.failed_occluders));
    }
    if r.overlays_without_sides > 0 {
        state.log(Severity::Warning, tf!("lab.bsp.lost_overlays", r.overlays_without_sides));
    }
    if extracted.files > 0 {
        state.log(Severity::Info, tf!("lab.bsp.content", extracted.files));
    }
    for w in &r.warnings {
        state.log(Severity::Warning, w.clone());
    }
    state.log(Severity::Info, t!("lab.bsp.limits"));
}

fn unmount(app: &mut HammerProApp, d: Decompiled) {
    if let Some(fs) = app.state.assets.fs() {
        for id in &d.sources {
            fs.remove_source(*id);
        }
    }
    if let Some(dir) = d.dir
        && dir.components().any(|c| c.as_os_str() == "decompiled")
    {
        let _ = std::thread::Builder::new().name("hp-lab-clean".into()).spawn(move || {
            let _ = std::fs::remove_dir_all(&dir);
        });
    }
}

fn nav_tick(app: &mut HammerProApp, ctx: &egui::Context) {
    let lab = &mut app.pkg.lab;
    if let Some(job) = &lab.nav_job
        && let Some(result) = job.poll()
    {
        let path = job.path.clone();
        lab.nav_job = None;
        match result {
            Ok(display) => {
                let reload = hp_ui_lab::nav::current(&lab.nav).is_some_and(|d| d.path == display.path);
                let s = &display.stats;
                let name = file_name(&display.path);
                let state = &mut app.state;
                let loaded = tf!("lab.nav.loaded", name, s.areas, s.ladders);
                if lab.nav_announce {
                    state.notify(Severity::Success, loaded);
                } else if reload {
                    state.log(Severity::Info, tf!("lab.nav.reloaded", name));
                } else {
                    state.log(Severity::Info, loaded);
                }
                if s.isolated_areas > 0 {
                    state.log(Severity::Warning, tf!("lab.nav.isolated", s.isolated_areas));
                }
                if s.one_way > 0 || s.dangling > 0 {
                    state.log(Severity::Info, tf!("lab.nav.one_way", s.one_way, s.dangling));
                }
                state.runtime.lab.nav_summary = Some((display.path.clone(), s.areas, s.isolated_areas));
                lab.nav_announce = false;
                lab.nav_failed = None;
                hp_ui_lab::nav::set_current(&lab.nav, Some(Arc::new(display)));
                ctx.request_repaint();
            }
            Err(e) => {
                lab.nav_failed = Some((path.clone(), hp_ui_lab::nav::modified(&path)));
                app.state.notify(Severity::Warning, tf!("lab.nav.failed", file_name(&path), e));
                lab.nav_announce = false;
            }
        }
    }
    let lab = &mut app.pkg.lab;
    if !app.state.settings.lab.show_nav_mesh {
        return;
    }
    if lab.nav_job.is_some() {
        ctx.request_repaint_after(Duration::from_millis(100));
        return;
    }
    let now = Instant::now();
    if lab.nav_check.is_some_and(|t| now < t) {
        ctx.request_repaint_after(NAV_CHECK_EVERY);
        return;
    }
    lab.nav_check = Some(now + NAV_CHECK_EVERY);
    ctx.request_repaint_after(NAV_CHECK_EVERY);
    let Some(path) = hp_ui_lab::nav::find_nav_file(&app.state) else {
        if hp_ui_lab::nav::current(&lab.nav).is_some() {
            hp_ui_lab::nav::set_current(&lab.nav, None);
            app.state.runtime.lab.nav_summary = None;
        }
        if lab.nav_announce {
            lab.nav_announce = false;
            app.state.notify(Severity::Info, t!("lab.nav.none"));
        }
        return;
    };
    let mtime = hp_ui_lab::nav::modified(&path);
    if lab.nav_failed.as_ref().is_some_and(|(p, t)| *p == path && *t == mtime) {
        return;
    }
    let current = hp_ui_lab::nav::current(&lab.nav);
    if current.is_some_and(|d| d.path == path && d.modified == mtime) {
        return;
    }
    lab.nav_serial += 1;
    let spawns = hp_ui_lab::nav::spawn_points(&app.state.doc);
    lab.nav_job = NavJob::start(path, lab.nav_serial, spawns, Some(ctx.clone()));
}

pub(crate) fn import_model_file(
    app: &mut HammerProApp,
    ctx: &egui::Context,
    path: PathBuf,
    confirm: bool,
    scale: Option<f64>,
) {
    app.pkg.lab.model_import.open_file(ctx, path);
    if let Some(s) = scale {
        app.pkg.lab.model_import.set_scale(s);
    }
    if confirm {
        app.pkg.lab.model_import.confirm();
    }
}

fn place_imported(app: &mut HammerProApp, imported: &hp_ui_entities::model_import::Imported) {
    let state = &mut app.state;
    let cam = state.cameras.cam3d;
    let dir = cam.forward();
    let (lo, hi) = imported.bounds;
    let reach = ((hi - lo).length() * 1.5).clamp(64.0, 1024.0);
    let opts = hp_editor::PickOptions { include_entities: false, max_distance: 8192.0, ..Default::default() };
    let hit = state.doc.pick_ray(&hp_geom::Ray::new(cam.pos, dir), &opts, &|e| state.entity_bounds(e));
    let target = match hit {
        Some(h) => hp_ui_viewports::placement::Target {
            point: h.point,
            normal: h.normal,
            face: None,
            on_surface: true,
            is_2d: false,
        },
        None => hp_ui_viewports::placement::Target {
            point: cam.pos + dir * reach,
            normal: glam::DVec3::Z,
            face: None,
            on_surface: false,
            is_2d: false,
        },
    };
    let model = imported.model.clone();
    if let Err(e) =
        hp_ui_viewports::placement::create_entity(state, imported.class, &target, imported.bounds, &[("model", &model)])
    {
        state.notify(Severity::Error, tf!("entities.mi_failed", e.to_string()));
    }
}

enum PropJob {
    Preparing(Receiver<Result<hp_ui_mapping::propper::PreparedProp, String>>, Vec<hp_editor::Id>),
    Compiling(CompileHandle, Vec<hp_editor::Id>, Box<hp_ui_mapping::propper::PreparedProp>),
}

fn start_brushes_to_prop(app: &mut HammerProApp, ctx: &egui::Context) {
    if app.pkg.lab.prop_job.is_some() {
        return;
    }
    if hp_compile::sandbox::is_sandboxed() {
        app.state.notify(Severity::Warning, t!("lab.prop.sandboxed"));
        return;
    }
    let solids: Vec<hp_editor::Id> = app
        .selected()
        .into_iter()
        .filter_map(|o| match o {
            hp_editor::ObjRef::Solid(id) => Some(id),
            hp_editor::ObjRef::Entity(_) => None,
        })
        .collect();
    if solids.is_empty() {
        app.state.notify(Severity::Warning, t!("lab.prop.select"));
        return;
    }
    let Some(root) = app.state.settings.game.gmod_root.clone().or_else(hp_fs::detect_gmod_root) else {
        app.state.notify(Severity::Warning, t!("lab.prop.no_game"));
        return;
    };
    if hp_ui_entities::model_tools::studiomdl_exe(&root).is_none() {
        app.state.notify(Severity::Warning, t!("lab.prop.no_studiomdl"));
        return;
    }
    let game_dir = app
        .state
        .assets
        .fs()
        .and_then(|f| f.game_dir().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| root.join("garrysmod"));
    let stem = app
        .state
        .doc
        .path()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| "map".to_owned());
    let mut sorted = solids.clone();
    sorted.sort_unstable();
    let mut h: u32 = 0x811c_9dc5;
    for id in &sorted {
        for b in id.to_le_bytes() {
            h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
        }
    }
    let clean: String = stem.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(40).collect();
    let options = app.state.settings.lab.propper_options();
    let suffix = if options.physics {
        "_physics"
    } else if !options.collision {
        "_visual"
    } else {
        ""
    };
    let model = format!("hammerpro/{clean}_{h:08x}{suffix}");
    let mut sizes: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
    let materials: Vec<String> = {
        let scene = app.state.doc.scene();
        solids
            .iter()
            .filter_map(|id| scene.solid(*id))
            .flat_map(|n| n.solid.sides.iter().map(|s| s.material.clone()))
            .collect()
    };
    for m in materials {
        let key = m.replace('\\', "/").to_ascii_lowercase();
        if sizes.contains_key(&key) {
            continue;
        }
        let size = app
            .state
            .assets
            .material_info(&m)
            .filter(|i| i.width > 0 && i.height > 0)
            .map(|i| (i.width, i.height))
            .unwrap_or((512, 512));
        sizes.insert(key, size);
    }
    let Some(geometry) = hp_editor::prop_export::collect_prop(&app.state.doc, &solids, &|m| {
        sizes.get(&m.replace('\\', "/").to_ascii_lowercase()).copied().unwrap_or((512, 512))
    }) else {
        app.state.notify(Severity::Warning, t!("lab.prop.empty"));
        return;
    };
    let dir = hp_app::settings::Settings::cache_dir().join("propsrc");
    let fs = app.state.assets.fs().cloned();
    let lod = app.state.settings.lab.model_lod;
    let (tx, rx) = crossbeam_channel::bounded(1);
    let repaint = ctx.clone();
    app.state.console.info(tf!("lab.prop.preparing", geometry.tris.len(), format!("models/{model}.mdl")));
    std::thread::spawn(move || {
        let _ =
            tx.send(hp_ui_mapping::propper::prepare_prop(fs.as_ref(), geometry, &model, lod, options, &dir, &game_dir));
        repaint.request_repaint();
    });
    app.pkg.lab.prop_job = Some(PropJob::Preparing(rx, solids));
}

fn poll_prop_job(app: &mut HammerProApp, ctx: &egui::Context) {
    match app.pkg.lab.prop_job.take() {
        None => {}
        Some(PropJob::Preparing(rx, solids)) => match rx.try_recv() {
            Err(crossbeam_channel::TryRecvError::Empty) => {
                app.pkg.lab.prop_job = Some(PropJob::Preparing(rx, solids));
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                app.state.notify(Severity::Error, t!("lab.prop.failed"));
            }
            Ok(Err(e)) => app.state.notify(Severity::Error, e),
            Ok(Ok(prepared)) => {
                let root = app.state.settings.game.gmod_root.clone().or_else(hp_fs::detect_gmod_root);
                let Some(exe) = root.as_deref().and_then(hp_ui_entities::model_tools::studiomdl_exe) else {
                    app.state.notify(Severity::Warning, t!("lab.prop.no_studiomdl"));
                    return;
                };
                let game_dir = prepared.game_dir.clone();
                let dir = prepared.qc.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
                let plan = hp_ui_entities::model_tools::studiomdl_plan(&exe, &game_dir, &prepared.qc);
                let repaint = ctx.clone();
                let handle = hp_compile::start_with_waker(plan, &dir, move || repaint.request_repaint());
                let triangles = prepared.lod_triangles.first().copied().unwrap_or(0);
                app.state.console.info(tf!("lab.prop.compiling", triangles, prepared.model.clone()));
                app.state.console.info(prepared.details());
                app.pkg.lab.prop_job = Some(PropJob::Compiling(handle, solids, Box::new(prepared)));
            }
        },
        Some(PropJob::Compiling(handle, solids, prepared)) => {
            let mut finished = None;
            loop {
                match handle.events.try_recv() {
                    Ok(CompileEvent::StepStarted { command_line, .. }) => app.state.console.info(command_line),
                    Ok(CompileEvent::Log { line, severity: sev, .. }) => {
                        app.state.console.push(compile_severity(sev), line);
                    }
                    Ok(CompileEvent::Finished { success, cancelled, .. }) => finished = Some((success, cancelled)),
                    Ok(_) => {}
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        finished.get_or_insert((false, false));
                        break;
                    }
                }
            }
            let Some((success, cancelled)) = finished else {
                app.pkg.lab.prop_job = Some(PropJob::Compiling(handle, solids, prepared));
                return;
            };
            if cancelled {
                return;
            }
            if !success {
                app.state.notify(Severity::Error, t!("lab.prop.compile_failed"));
                return;
            }
            if prepared.pieces == 0 {
                let path = prepared.game_dir.join(format!("models/{}.phy", prepared.model));
                if let Err(error) = std::fs::remove_file(&path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    app.state.notify(Severity::Error, tf!("propper.write_failed", error));
                    return;
                }
            }
            app.state.assets.remount(&app.state.settings);
            match app.state.doc.replace_with_prop_options(&solids, &prepared.model, prepared.origin, prepared.options) {
                Ok(_) => {
                    let done = tf!("lab.prop.done", format!("models/{}.mdl", prepared.model));
                    app.state.notify(Severity::Success, format!("{done} {}", prepared.details()));
                }
                Err(e) => app.state.notify(Severity::Error, tf!("lab.prop.replace_failed", e.to_string())),
            }
        }
    }
}

fn compile_severity(s: hp_compile::Severity) -> Severity {
    match s {
        hp_compile::Severity::Info => Severity::Info,
        hp_compile::Severity::Success => Severity::Success,
        hp_compile::Severity::Warning => Severity::Warning,
        hp_compile::Severity::Error => Severity::Error,
    }
}

fn start_nav_gen(app: &mut HammerProApp, ctx: &egui::Context) {
    if app.pkg.lab.nav_gen.is_some() {
        return;
    }
    let Some(path) = app.state.doc.path().map(|p| p.with_extension("nav")) else {
        app.state.notify(Severity::Warning, t!("lab.navgen.save_first"));
        return;
    };
    let input = hp_ui_lab::nav_gen::input_from_doc(&mut app.state);
    if input.seeds.is_empty() {
        app.state.notify(Severity::Warning, t!("lab.navgen.no_spawn"));
        return;
    }
    let (tx, rx) = crossbeam_channel::bounded(1);
    let repaint = ctx.clone();
    let spawned = std::thread::Builder::new().name("hp-nav-gen".into()).spawn(move || {
        let start = Instant::now();
        let (mesh, report) = hp_editor::background::run(|| hp_nav::generate(&input));
        let areas = mesh.areas.len();
        let answer = if areas == 0 {
            Err(t!("lab.navgen.nothing").to_owned())
        } else {
            let tmp = path.with_extension("nav.tmp");
            std::fs::write(&tmp, hp_nav::write(&mesh))
                .and_then(|()| std::fs::rename(&tmp, &path))
                .map(|()| (path, report, areas, start.elapsed().as_millis()))
                .map_err(|e| {
                    let _ = std::fs::remove_file(&tmp);
                    e.to_string()
                })
        };
        let _ = tx.send(answer);
        repaint.request_repaint();
    });
    match spawned {
        Ok(_) => {
            app.pkg.lab.nav_gen = Some(rx);
            app.state.log(Severity::Info, t!("lab.navgen.working"));
        }
        Err(e) => app.state.notify(Severity::Error, e.to_string()),
    }
}

fn poll_nav_gen(app: &mut HammerProApp) {
    let Some(rx) = &app.pkg.lab.nav_gen else { return };
    let answer = match rx.try_recv() {
        Ok(a) => a,
        Err(crossbeam_channel::TryRecvError::Empty) => return,
        Err(crossbeam_channel::TryRecvError::Disconnected) => Err(t!("lab.navgen.crashed").to_owned()),
    };
    app.pkg.lab.nav_gen = None;
    match answer {
        Ok((path, report, areas, ms)) => {
            app.state.runtime.lab.nav_file = Some(path);
            if !app.state.settings.lab.show_nav_mesh {
                app.state.settings.lab.show_nav_mesh = true;
                app.state.settings_dirty = true;
            }
            app.pkg.lab.nav_check = None;
            app.pkg.lab.nav_failed = None;
            app.state.notify(Severity::Success, tf!("lab.navgen.done", areas, ms));
            if report.capped {
                app.state.log(Severity::Warning, t!("lab.navgen.capped"));
            }
            if report.bad_seeds > 0 {
                app.state.log(Severity::Warning, tf!("lab.navgen.bad_seeds", report.bad_seeds));
            }
        }
        Err(e) => app.state.notify(Severity::Error, tf!("lab.navgen.failed", e)),
    }
}

fn particles_tick(app: &mut HammerProApp, ctx: &egui::Context) {
    if app.state.settings.lab.show_entity_fx && app.pkg.lab.particles.lock().is_ok_and(|s| s.fx_active) {
        ctx.request_repaint();
    }
    if !app.state.settings.lab.show_particles && !app.state.runtime.lab.particle_preview.open {
        let fx_on = app.state.settings.lab.show_entity_fx
            && app.pkg.lab.particles.lock().is_ok_and(|s| !s.fx_sprites.is_empty());
        if fx_on {
            particles_draw(app);
        } else if std::mem::take(&mut app.pkg.lab.particles_shown)
            && let Some(gfx) = app.state.gfx.as_mut()
        {
            gfx.renderer.set_particles(&[]);
        }
        let report = &mut app.state.runtime.lab.particles;
        report.playing.clear();
        report.particles = 0;
        return;
    }
    particles_draw(app);
    let generation = app.state.assets.generation();
    let lab = &mut app.pkg.lab;
    if generation != lab.pcf_generation {
        lab.pcf_generation = generation;
        lab.pcf_done.clear();
        lab.pcf_queue.clear();
        lab.pcf_map = None;
        if let Ok(mut s) = lab.particles.lock() {
            s.missing.clear();
            s.library.clear();
            s.files.clear();
            s.looks.clear();
            s.library_gen += 1;
        }
    }
    let map_key = (app.state.sidecar.serial(), generation);
    if app.pkg.lab.pcf_map.as_ref().map(|(k, _)| *k) != Some(map_key) {
        let files = map_particle_files(app);
        let lab = &mut app.pkg.lab;
        for f in &files {
            if !lab.pcf_done.contains(f) && !lab.pcf_queue.contains(f) {
                lab.pcf_queue.push(f.clone());
            }
        }
        lab.pcf_map = Some((map_key, files));
    }
    let lab = &mut app.pkg.lab;
    let mut finished = Vec::new();
    lab.pcf_jobs.retain(|(file, rx)| match rx.try_recv() {
        Ok(r) => {
            finished.push((file.clone(), r));
            false
        }
        Err(crossbeam_channel::TryRecvError::Empty) => true,
        Err(crossbeam_channel::TryRecvError::Disconnected) => {
            finished.push((file.clone(), Err("the reader stopped".to_owned())));
            false
        }
    });
    let catalog = app.state.assets.particles();
    let map_files: &[String] = lab.pcf_map.as_ref().map_or(&[], |(_, f)| f);
    let Ok(mut s) = lab.particles.lock() else { return };
    for (file, result) in finished {
        match result {
            Ok(loaded) => {
                let from_map = map_files.contains(&file);
                for plan in loaded.plans {
                    let key = plan.name.to_ascii_lowercase();
                    let taken_by_map = s.files.get(&key).is_some_and(|f| map_files.contains(f));
                    let owner = catalog.as_ref().and_then(|c| {
                        c.systems.iter().find(|i| i.name.eq_ignore_ascii_case(&key)).map(|i| i.file.clone())
                    });
                    let take = if from_map {
                        true
                    } else {
                        !taken_by_map
                            && !s.library.contains_key(&key)
                            && owner.is_none_or(|f| f.eq_ignore_ascii_case(&file))
                    };
                    if take {
                        s.files.insert(key.clone(), file.clone());
                        s.library.insert(key.clone(), plan);
                        s.wanted.remove(&key);
                        s.missing.remove(&key);
                    }
                }
                for (k, look) in loaded.looks {
                    s.looks.insert(k, look);
                }
                s.library_gen += 1;
            }
            Err(e) => log::warn!("particle file {file}: {e}"),
        }
    }
    let report = &mut app.state.runtime.lab.particles;
    let report_key = (s.library_gen, s.missing.len());
    if lab.pcf_report_key != Some(report_key) {
        lab.pcf_report_key = Some(report_key);
        report.loaded = s.files.iter().map(|(k, f)| (k.clone(), f.clone())).collect();
        report.missing = s.missing.iter().cloned().collect();
        report.unsupported = s
            .library
            .iter()
            .filter(|(_, p)| !p.unsupported.is_empty())
            .map(|(k, p)| (k.clone(), p.unsupported.clone()))
            .collect();
    }
    if report.playing != s.playing {
        report.playing.clone_from(&s.playing);
    }
    report.particles = s.particles;
    let map_pending = map_files.iter().any(|f| !lab.pcf_done.contains(f) || lab.pcf_jobs.iter().any(|(j, _)| j == f));
    if !map_pending && let Some(catalog) = &catalog {
        let wanted: Vec<String> = s.wanted.iter().cloned().collect();
        for name in wanted {
            if s.library.contains_key(&name) {
                s.wanted.remove(&name);
                continue;
            }
            let Some(info) = catalog.systems.iter().find(|i| i.name.eq_ignore_ascii_case(&name)) else {
                s.wanted.remove(&name);
                s.missing.insert(name);
                continue;
            };
            if lab.pcf_done.contains(&info.file) {
                if lab.pcf_jobs.iter().all(|(f, _)| *f != info.file) {
                    s.wanted.remove(&name);
                    s.missing.insert(name);
                }
                continue;
            }
            if !lab.pcf_queue.contains(&info.file) {
                lab.pcf_queue.push(info.file.clone());
            }
        }
    }
    let active = s.active;
    drop(s);
    while lab.pcf_jobs.len() < MAX_PCF_JOBS && !lab.pcf_queue.is_empty() {
        let file = lab.pcf_queue.remove(0);
        if lab.pcf_done.contains(&file) {
            continue;
        }
        let Some(fs) = app.state.assets.fs().cloned() else { break };
        lab.pcf_done.insert(file.clone());
        let (tx, rx) = crossbeam_channel::bounded(1);
        let worker_file = file.clone();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-pcf-load".into()).spawn(move || {
            let _ = tx.send(hp_ui_lab::particles::load_file(&fs, &worker_file));
            repaint.request_repaint();
        });
        if spawned.is_ok() {
            lab.pcf_jobs.push((file, rx));
        }
    }
    if active {
        ctx.request_repaint();
    }
    if !lab.pcf_jobs.is_empty() || !lab.pcf_queue.is_empty() {
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn map_particle_files(app: &HammerProApp) -> Vec<String> {
    let Some(path) = app.state.doc.path() else { return Vec::new() };
    let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else { return Vec::new() };
    let name = format!("{stem}_particles.txt");
    let text = app
        .state
        .assets
        .fs()
        .and_then(|fs| fs.read_text(&format!("maps/{name}")))
        .or_else(|| path.parent().and_then(|dir| std::fs::read_to_string(dir.join(&name)).ok()));
    text.map(|t| hp_ui_lab::particles::manifest_files(&t)).unwrap_or_default()
}

fn particles_draw(app: &mut HammerProApp) {
    let lab = &mut app.pkg.lab;
    let eye = app.state.cameras.cam3d.pos.as_vec3();
    let forward = app.state.cameras.cam3d.forward().as_vec3();
    let AppState { assets, gfx, .. } = &mut app.state;
    let Ok(mut shared) = lab.particles.lock() else { return };
    shared.sprite_layer = gfx.is_some();
    let Some(gfx) = gfx.as_mut() else { return };
    if shared.sprites.is_empty() && shared.fx_sprites.is_empty() && !lab.particles_shown {
        return;
    }
    let mut instances = hp_ui_lab::particles::instances_of(&shared.sprites, &shared.materials, &mut **assets);
    if !shared.fx_sprites.is_empty() {
        instances.extend(hp_ui_lab::particles::instances_of(&shared.fx_sprites, &shared.fx_materials, &mut **assets));
    }
    instances.sort_by(|a, b| {
        let depth = |p: &hp_render::ParticleInstance| (glam::Vec3::from(p.pos) - eye).dot(forward);
        depth(b).total_cmp(&depth(a))
    });
    lab.particles_shown = !instances.is_empty();
    gfx.renderer.set_particles(&instances);
}

pub(crate) fn particle_stats(app: &HammerProApp) -> String {
    let lab = &app.pkg.lab;
    let Ok(s) = lab.particles.lock() else { return String::new() };
    let drawn = app.state.gfx.as_ref().map_or(0, |g| g.renderer.particle_count());
    format!(
        "on={} effects={} particles={} drawn={} systems_loaded={} missing={} files_read={} reading={}",
        app.state.settings.lab.show_particles,
        s.playing.len(),
        s.particles,
        drawn,
        s.library.len(),
        s.missing.len(),
        lab.pcf_done.len(),
        lab.pcf_jobs.len()
    )
}

struct NavStatus;

impl StatusSegment for NavStatus {
    fn id(&self) -> &'static str {
        "lab_nav"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::CONTENT + 40
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, _cx: &mut StatusContext) {
        if let Some(name) = &state.runtime.lab.decompiling {
            ui.separator();
            ui.label(tf!("lab.bsp.working", name));
            ui.add(egui::Spinner::new());
        }
        if !state.settings.lab.show_nav_mesh {
            return;
        }
        let Some((path, areas, isolated)) = state.runtime.lab.nav_summary.clone() else { return };
        ui.separator();
        let text = if isolated > 0 {
            egui::RichText::new(tf!("lab.nav.status_isolated", areas, isolated)).color(hp_app::theme::warning())
        } else {
            egui::RichText::new(tf!("lab.nav.status", areas))
        };
        ui.label(text).on_hover_text(tf!("lab.nav.status_tip", path.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handled_actions_are_ready() {
        for a in HANDLED {
            assert!(a.is_ready(), "{} must be ready", a.id());
        }
    }
}
