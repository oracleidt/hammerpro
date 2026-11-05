use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::i18n::TrTable;
use hp_app::runtime::{CaptureState, CapturedImage};
use hp_app::state::RightTab;
use hp_app::{Action, AppState, Severity, ViewKind, t, tf};
use hp_ui_mapping::mapassets::{MapAssetsPanel, TAB as ASSETS_TAB};

use super::{DockTab, Package};
use crate::app::HammerProApp;

pub(crate) struct Content;

#[derive(Default)]
pub(crate) struct State {
    pick: Option<Receiver<Option<PathBuf>>>,
    capture: Arc<Mutex<Option<CapturedImage>>>,
    capture_frames: u32,
    new_version: Option<String>,
    copies: Option<Receiver<Vec<Result<String, String>>>>,
    assets: MapAssetsPanel,
}

const HANDLED: &[Action] = &[
    Action::BuildContent,
    Action::PublishWorkshop,
    Action::OpenArchive,
    Action::SaveAsNewVersion,
    Action::ShowMapAssets,
    Action::ShaderEditor,
    Action::VtfEditor,
];

impl Package for Content {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::BuildContent => {
                app.state.runtime.content.release_open = true;
                app.state.runtime.content.release_tab = Some("build");
            }
            Action::PublishWorkshop => {
                app.state.runtime.content.release_open = true;
                app.state.runtime.content.release_tab = Some("publish");
            }
            Action::ShowMapAssets => show_map_assets(app),
            Action::ShaderEditor => app.state.runtime.shader.show = true,
            Action::VtfEditor => app.state.runtime.vtf.open_window(),
            Action::SaveAsNewVersion => match app.state.doc.path().and_then(|p| p.file_stem()) {
                Some(stem) => app.pkg.content.new_version = Some(next_version(&stem.to_string_lossy())),
                None => app.state.request(Action::SaveMapAs),
            },
            Action::OpenArchive => {
                app.state.runtime.content.inspector_open = true;
                if app.pkg.content.pick.is_none() {
                    let filter = t!("inspector.archives").to_owned();
                    let start = app.state.doc.path().and_then(|p| p.parent()).map(Path::to_path_buf);
                    let (tx, rx) = crossbeam_channel::bounded(1);
                    let mut dialog = rfd::FileDialog::new().add_filter(filter, &["bsp", "gma", "vpk"]);
                    if let Some(s) = start {
                        dialog = dialog.set_directory(s);
                    }
                    if cfg!(target_os = "macos") {
                        let _ = tx.send(dialog.pick_file());
                    } else {
                        let _ = std::thread::Builder::new().name("hp-open-archive".into()).spawn(move || {
                            let _ = tx.send(dialog.pick_file());
                        });
                    }
                    app.pkg.content.pick = Some(rx);
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        if let Some(rx) = &app.pkg.content.pick {
            match rx.try_recv() {
                Ok(picked) => {
                    app.pkg.content.pick = None;
                    if let Some(path) = picked {
                        app.state.runtime.content.inspector_request = Some(path);
                    }
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => app.pkg.content.pick = None,
            }
        }
        if Action::OpenArchive.is_ready()
            && let Some(path) = app.state.runtime.workflow.take_dropped(&["bsp", "gma", "vpk"]).pop()
        {
            app.state.runtime.content.inspector_request = Some(path);
        }
        self.capture_tick(app, ctx);
        let visible =
            app.state.settings.content.show_map_assets && app.state.ui.right_tab == RightTab::Package(ASSETS_TAB);
        app.pkg.content.assets.tick(ctx, &mut app.state, visible);
        if let Some(rx) = &app.pkg.content.copies {
            match rx.try_recv() {
                Ok(results) => {
                    app.pkg.content.copies = None;
                    for r in results {
                        match r {
                            Ok(name) => app.state.log(Severity::Info, tf!("content.copied", name)),
                            Err(e) => app.state.notify(Severity::Warning, e),
                        }
                    }
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => app.pkg.content.copies = None,
            }
        }
    }

    fn init(&self, app: &mut HammerProApp) {
        app.shell.add_status_segment(Box::new(ReleaseProgress));
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        let Some(name) = app.pkg.content.new_version.as_mut() else { return };
        let valid = map_name_ok(name);
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("hp_new_version")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(t!("content.new_version_title"));
            ui.label(t!("content.new_version_text"));
            let field = ui.text_edit_singleline(name);
            if !field.has_focus() && !field.lost_focus() && ui.memory(|m| m.focused().is_none()) {
                field.request_focus();
            }
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) && valid {
                answer = Some(true);
            }
            if !valid {
                ui.colored_label(hp_app::theme::error(), t!("content.bad_name"));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(valid, egui::Button::new(t!("content.save_version"))).clicked() {
                    answer = Some(true);
                }
                if ui.button(t!("content.cancel")).clicked() {
                    answer = Some(false);
                }
            });
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(false);
        }
        match answer {
            Some(true) => {
                let name = app.pkg.content.new_version.take().unwrap_or_default();
                save_new_version(app, &name);
            }
            Some(false) => app.pkg.content.new_version = None,
            None => {}
        }
    }

    fn dock_tabs(&self, app: &HammerProApp, tabs: &mut Vec<DockTab>) {
        if app.state.settings.content.show_map_assets {
            tabs.push(DockTab { id: ASSETS_TAB, label: t!("action.show_map_assets").to_owned() });
        }
    }

    fn dock_ui(&self, app: &mut HammerProApp, ui: &mut egui::Ui, id: &'static str) -> bool {
        if id != ASSETS_TAB {
            return false;
        }
        app.pkg.content.assets.ui(ui, &mut app.state);
        true
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

fn show_map_assets(app: &mut HammerProApp) {
    let tab = RightTab::Package(ASSETS_TAB);
    let s = &mut app.state.settings.content;
    if s.show_map_assets && app.state.ui.right_tab == tab {
        s.show_map_assets = false;
    } else {
        s.show_map_assets = true;
        app.state.ui.right_tab = tab;
    }
    app.state.settings_dirty = true;
}

impl Content {
    fn capture_tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        match app.state.runtime.content.capture {
            CaptureState::Requested => {
                let rect = app
                    .viewports
                    .view_rects()
                    .into_iter()
                    .flatten()
                    .find(|(k, _)| *k == ViewKind::Perspective)
                    .map(|(_, r)| r);
                let Some(rect) = rect else {
                    app.state.runtime.content.capture = CaptureState::Failed;
                    return;
                };
                app.state.runtime.content.capture = CaptureState::Capturing;
                app.pkg.content.capture_frames = 0;
                let slot = app.pkg.content.capture.clone();
                let ppp = ctx.pixels_per_point();
                let repaint = ctx.clone();
                ctx.request_screenshot(move |image| {
                    let [w, h] = image.size;
                    let x0 = ((rect.left() * ppp).round().max(0.0) as usize).min(w);
                    let y0 = ((rect.top() * ppp).round().max(0.0) as usize).min(h);
                    let x1 = ((rect.right() * ppp).round().max(0.0) as usize).min(w);
                    let y1 = ((rect.bottom() * ppp).round().max(0.0) as usize).min(h);
                    if x1 > x0 && y1 > y0 && image.pixels.len() >= w * h {
                        let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
                        for y in y0..y1 {
                            for c in &image.pixels[y * w + x0..y * w + x1] {
                                rgba.extend_from_slice(&c.to_array());
                            }
                        }
                        let img = CapturedImage { width: (x1 - x0) as u32, height: (y1 - y0) as u32, rgba };
                        *slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(img);
                    }
                    repaint.request_repaint();
                });
            }
            CaptureState::Capturing => {
                let taken = app.pkg.content.capture.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
                if let Some(img) = taken {
                    app.state.runtime.content.capture = CaptureState::Ready(img);
                } else {
                    app.pkg.content.capture_frames += 1;
                    if app.pkg.content.capture_frames > 120 {
                        app.state.runtime.content.capture = CaptureState::Failed;
                    }
                    ctx.request_repaint();
                }
            }
            _ => {}
        }
    }
}

fn next_version(stem: &str) -> String {
    let lower = stem.to_ascii_lowercase();
    if let Some((base, n)) = lower.rsplit_once("_v")
        && let Ok(n) = n.parse::<u32>()
    {
        return format!("{base}_v{}", n.saturating_add(1));
    }
    format!("{lower}_v2")
}

fn map_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn save_new_version(app: &mut HammerProApp, name: &str) {
    let Some(old_vmf) = app.state.doc.path().map(Path::to_path_buf) else { return };
    let Some(old) = old_vmf.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()) else { return };
    let new_vmf = old_vmf.with_file_name(format!("{name}.vmf"));
    if new_vmf.exists() {
        app.state.notify(Severity::Warning, tf!("content.exists", new_vmf.display()));
        return;
    }
    if !crate::files::save_map_to(&mut app.state, &new_vmf) {
        return;
    }
    let mut copies: Vec<(PathBuf, PathBuf)> = Vec::new();
    if let Some(fs) = app.state.assets.fs() {
        for (from, to) in [
            (format!("maps/{old}_particles.txt"), format!("{name}_particles.txt")),
            (format!("maps/{old}_level_sounds.txt"), format!("{name}_level_sounds.txt")),
            (format!("scripts/soundscapes_{old}.txt"), format!("soundscapes_{name}.txt")),
        ] {
            if let Some(src) = fs.disk_path(&from) {
                let dest = src.with_file_name(to);
                copies.push((src, dest));
            }
        }
    }
    copies.push((old_vmf.with_extension("nav"), new_vmf.with_extension("nav")));
    let name = name.to_owned();
    let (tx, rx) = crossbeam_channel::bounded(1);
    let spawned = std::thread::Builder::new().name("hp-new-version".into()).spawn(move || {
        let mut results = Vec::new();
        let old_bsp = old_vmf.with_extension("bsp");
        let new_bsp = new_vmf.with_extension("bsp");
        if old_bsp.is_file() && !new_bsp.exists() {
            let r = hp_bsp::Bsp::read(&old_bsp)
                .and_then(|b| hp_bsp::rename::rename_map(b, &old, &name))
                .and_then(|e| e.save(&new_bsp));
            results.push(match r {
                Ok(()) => Ok(new_bsp.display().to_string()),
                Err(e) => Err(tf!("content.copy_failed", new_bsp.display(), e)),
            });
        }
        for (src, dest) in copies {
            if !src.is_file() || dest.exists() {
                continue;
            }
            results.push(match std::fs::copy(&src, &dest) {
                Ok(_) => Ok(dest.display().to_string()),
                Err(e) => Err(tf!("content.copy_failed", dest.display(), e)),
            });
        }
        let _ = tx.send(results);
    });
    if spawned.is_ok() {
        app.pkg.content.copies = Some(rx);
    }
}

struct ReleaseProgress;

impl StatusSegment for ReleaseProgress {
    fn id(&self) -> &'static str {
        "content_release"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::CONTENT + 20
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, _cx: &mut StatusContext) {
        let Some((stage, done)) = state.runtime.content.progress.clone() else { return };
        ui.separator();
        if ui.small_button("✖").on_hover_text(t!("release.cancel")).clicked() {
            state.runtime.content.cancel_requested = true;
        }
        let bar = ui.add(hp_app::widgets::progress_bar(done).text(stage).desired_width(160.0));
        if bar.clicked() {
            state.runtime.content.release_open = true;
        }
    }
}

const TR: TrTable = &[
    ("content.new_version_title", "Save as new version", "Enregistrer comme nouvelle version"),
    (
        "content.new_version_text",
        "New map name. The compiled map, its navigation mesh and its per map scripts are copied under the new name (built cubemaps keep working).",
        "Nouveau nom de carte. La carte compilée, son maillage de navigation et ses scripts propres sont copiés sous le nouveau nom (les cubemaps construites restent valides).",
    ),
    (
        "content.bad_name",
        "Use lowercase letters, digits, _ and - only.",
        "Utilisez uniquement des minuscules, des chiffres, _ et -.",
    ),
    ("content.save_version", "Save", "Enregistrer"),
    ("content.cancel", "Cancel", "Annuler"),
    ("content.exists", "This map already exists: {0}", "Cette carte existe déjà : {0}"),
    ("content.copied", "Copied for the new version: {0}", "Copié pour la nouvelle version : {0}"),
    ("content.copy_failed", "Cannot write {0}: {1}", "Impossible d'écrire {0} : {1}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_names() {
        assert_eq!(next_version("gm_test"), "gm_test_v2");
        assert_eq!(next_version("GM_Test_v9"), "gm_test_v10");
        assert_eq!(next_version("ttt_x_vfinal"), "ttt_x_vfinal_v2");
        assert!(map_name_ok("gm_a-b_2"));
        assert!(!map_name_ok("gm a"));
        assert!(!map_name_ok("Gm_a"));
        assert!(!map_name_ok(""));
        assert!(hp_app::i18n::lint_table(TR).is_empty());
        assert!(TR.iter().all(|(k, _, _)| k.starts_with("content.")));
    }
}
