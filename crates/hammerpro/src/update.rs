use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use hp_app::runtime::update::{PREBUILT_PLATFORM, SOURCE_UPDATE_COMMANDS, megabytes, platform_name};
use hp_app::runtime::{UpdateFailure, UpdateInfo, UpdateStatus};
use hp_app::{AppState, Severity, t, tf};
use hp_update::{CheckResult, Download, Downloaded, Release, UpdateError};

const POLL: Duration = Duration::from_millis(200);

pub struct UpdateController {
    enabled: bool,
    startup_check: bool,
    started: bool,
    check: Option<Receiver<CheckResult>>,
    download: Option<Download>,
    release: Option<Release>,
    ready: Option<Downloaded>,
    armed: bool,
}

pub fn classify(error: &UpdateError) -> UpdateFailure {
    let (key, detail) = match error {
        UpdateError::Network(d) => ("update.err_network", d.clone()),
        UpdateError::Http(code) => ("update.err_http", code.to_string()),
        UpdateError::BadRelease(d) => ("update.err_release", d.clone()),
        UpdateError::BadZip(d) => ("update.err_zip", d.clone()),
        UpdateError::Io(d) => ("update.err_io", d.clone()),
        UpdateError::Cancelled => ("update.err_cancelled", String::new()),
        UpdateError::Apply(d) => ("update.err_other", d.clone()),
    };
    UpdateFailure { key, detail }
}

pub fn info_of(release: &Release) -> UpdateInfo {
    let asset = release.pick_asset();
    UpdateInfo {
        tag: release.tag.clone(),
        version: release.version.to_string(),
        title: release.title.clone(),
        notes: release.notes.clone(),
        page_url: release.page_url.clone(),
        asset_name: asset.map(|a| a.name.clone()),
        asset_size: asset.map_or(0, |a| a.size),
        asset_sha256: asset.and_then(|a| a.sha256.clone()),
    }
}

impl UpdateController {
    pub fn new(automated: bool) -> Self {
        let overridden = hp_update::api_url_overridden();
        Self {
            enabled: !automated || overridden,
            startup_check: !automated || overridden,
            started: false,
            check: None,
            download: None,
            release: None,
            ready: None,
            armed: false,
        }
    }

    pub fn tick(&mut self, state: &mut AppState, ctx: &egui::Context) {
        if state.runtime.update.current.is_empty() {
            state.runtime.update.current = hp_update::current_version().to_string();
            state.runtime.update.api_overridden = hp_update::api_url_overridden();
        }
        if !self.enabled {
            let rt = &mut state.runtime.update;
            if rt.install_requested {
                rt.install_requested = false;
                state.notify(Severity::Warning, t!("update.busy_automated"));
            }
            state.runtime.update.check_requested = None;
            return;
        }
        if !self.started {
            self.started = true;
            if self.startup_check && state.settings.update.check_on_startup {
                self.start_check(state, false);
            }
        }
        if let Some(manual) = state.runtime.update.check_requested.take()
            && self.check.is_none()
            && self.download.is_none()
        {
            self.start_check(state, manual);
        }
        self.poll_check(state, ctx);
        if std::mem::take(&mut state.runtime.update.cancel_requested)
            && let Some(download) = &self.download
        {
            download.progress.cancel.store(true, Ordering::Relaxed);
        }
        if std::mem::take(&mut state.runtime.update.install_requested) {
            self.start_download(state);
        }
        self.poll_download(state, ctx);
    }

    fn start_check(&mut self, state: &mut AppState, manual: bool) {
        let rt = &mut state.runtime.update;
        rt.status = UpdateStatus::Checking;
        rt.manual = manual;
        rt.failure = None;
        self.check = Some(hp_update::spawn_check());
    }

    fn poll_check(&mut self, state: &mut AppState, ctx: &egui::Context) {
        let Some(rx) = &self.check else { return };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(POLL);
                return;
            }
            Err(TryRecvError::Disconnected) => {
                CheckResult::Failed(UpdateError::Network("the check ended without an answer".into()))
            }
        };
        self.check = None;
        let manual = state.runtime.update.manual;
        match result {
            CheckResult::UpToDate { latest } => {
                let rt = &mut state.runtime.update;
                rt.status = UpdateStatus::UpToDate;
                rt.latest_seen = latest.to_string();
                rt.latest = None;
                if manual {
                    state.notify(Severity::Info, tf!("update.toast_up_to_date", rt_current(state)));
                }
            }
            CheckResult::Available(release) => {
                let info = info_of(&release);
                let announces = state.settings.update.announces(&release.tag);
                let rt = &mut state.runtime.update;
                rt.status = UpdateStatus::Available;
                rt.latest_seen = info.version.clone();
                rt.dont_show_again = false;
                rt.latest = Some(info);
                if manual || announces {
                    rt.dialog_open = true;
                }
                self.release = Some(release);
            }
            CheckResult::Failed(error) => {
                log::info!("update check failed: {error}");
                let failure = classify(&error);
                let text = tf!(failure.key, failure.detail);
                let rt = &mut state.runtime.update;
                rt.status = UpdateStatus::Failed;
                rt.failure = Some(failure);
                if manual {
                    state.notify(Severity::Warning, tf!("update.toast_failed", text));
                }
            }
        }
    }

    fn start_download(&mut self, state: &mut AppState) {
        let rt = &mut state.runtime.update;
        if !rt.can_install() || self.download.is_some() {
            return;
        }
        let Some(release) = self.release.clone() else { return };
        match hp_update::spawn_download(&release) {
            Ok(download) => {
                rt.status = UpdateStatus::Downloading;
                rt.failure = None;
                rt.done_bytes = 0;
                rt.total_bytes = rt.latest.as_ref().map_or(0, |l| l.asset_size);
                self.download = Some(download);
            }
            Err(error) => {
                rt.status = UpdateStatus::Failed;
                rt.failure = Some(classify(&error));
            }
        }
    }

    fn poll_download(&mut self, state: &mut AppState, ctx: &egui::Context) {
        let Some(download) = &self.download else { return };
        let rt = &mut state.runtime.update;
        rt.done_bytes = download.progress.done.load(Ordering::Relaxed);
        rt.total_bytes = download.progress.total.load(Ordering::Relaxed);
        let result = match download.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(POLL);
                return;
            }
            Err(TryRecvError::Disconnected) => Err(UpdateError::Network("the download ended without an answer".into())),
        };
        self.download = None;
        match result {
            Ok(done) => {
                self.ready = Some(done);
                rt.status = UpdateStatus::Ready;
                rt.restart_requested = true;
            }
            Err(UpdateError::Cancelled) => {
                rt.status = UpdateStatus::Available;
            }
            Err(error) => {
                log::warn!("update download failed: {error}");
                rt.status = UpdateStatus::Failed;
                rt.failure = Some(classify(&error));
            }
        }
    }

    pub fn arm(&mut self) -> bool {
        self.armed = self.ready.is_some();
        self.armed
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }

    pub fn spawn_on_exit(&mut self) {
        if !self.armed {
            return;
        }
        self.armed = false;
        let Some(done) = self.ready.take() else { return };
        match done.helper.spawn(&done.zip, Some(&done.sha256)) {
            Ok(pid) => log::info!("update helper started (process {pid})"),
            Err(e) => log::error!("cannot start the update helper: {e}"),
        }
    }

    pub fn dialog(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let rt = &state.runtime.update;
        if !rt.dialog_open {
            return;
        }
        let Some(info) = rt.latest.clone() else {
            state.runtime.update.dialog_open = false;
            return;
        };
        let status = rt.status;
        let mut ignore = false;
        let mut later = false;
        let mut update = false;
        let mut cancel = false;
        let mut restart = false;
        let mut dont_show = rt.dont_show_again;
        let modal = egui::Modal::new(egui::Id::unique("hp_update_dialog")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.heading(t!("update.title"));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tf!("update.heading", info.version)).strong());
            ui.label(tf!("update.current", state.runtime.update.current));
            ui.add_space(8.0);
            ui.label(egui::RichText::new(t!("update.notes")).strong());
            egui::ScrollArea::vertical().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
                notes_ui(ui, &info.notes);
            });
            ui.add_space(8.0);
            let rt = &state.runtime.update;
            match status {
                UpdateStatus::Downloading => {
                    let text = if rt.total_bytes > 0 {
                        tf!("update.downloading", megabytes(rt.done_bytes), megabytes(rt.total_bytes))
                    } else {
                        tf!("update.downloading_unknown", megabytes(rt.done_bytes))
                    };
                    ui.label(text);
                    let bar = egui::ProgressBar::new(rt.progress().unwrap_or(0.0)).show_percentage();
                    ui.add(if rt.total_bytes > 0 { bar } else { bar.animate(true) });
                    ui.add_space(6.0);
                    if ui.button(t!("update.btn_cancel")).clicked() {
                        cancel = true;
                    }
                }
                UpdateStatus::Ready => {
                    ui.label(t!("update.ready"));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("update.btn_restart")).clicked() {
                            restart = true;
                        }
                        if ui.button(t!("update.btn_later")).clicked() {
                            later = true;
                        }
                    });
                }
                _ if !PREBUILT_PLATFORM => {
                    source_update_ui(ui, ctx);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(t!("update.btn_page")).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(&info.page_url));
                        }
                        if ui.button(t!("update.btn_ignore")).clicked() {
                            ignore = true;
                        }
                    });
                    ui.add_space(4.0);
                    ui.checkbox(&mut dont_show, t!("update.dont_show")).on_hover_text(t!("update.dont_show_tip"));
                }
                _ => {
                    if let Some(failure) = &rt.failure {
                        let text = tf!("update.failed", tf!(failure.key, failure.detail));
                        ui.label(egui::RichText::new(text).color(ui.visuals().warn_fg_color));
                        ui.add_space(4.0);
                    }
                    let installable = info.asset_name.is_some();
                    if !installable {
                        ui.label(egui::RichText::new(t!("update.no_asset")).color(ui.visuals().warn_fg_color));
                        ui.add_space(4.0);
                    }
                    ui.horizontal(|ui| {
                        if installable {
                            let label = if status == UpdateStatus::Failed {
                                t!("update.btn_retry")
                            } else {
                                t!("update.btn_update")
                            };
                            if ui.button(label).clicked() {
                                update = true;
                            }
                        } else if ui.button(t!("update.btn_page")).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(&info.page_url));
                        }
                        if ui.button(t!("update.btn_ignore")).clicked() {
                            ignore = true;
                        }
                    });
                    ui.add_space(4.0);
                    ui.checkbox(&mut dont_show, t!("update.dont_show")).on_hover_text(t!("update.dont_show_tip"));
                }
            }
        });
        let rt = &mut state.runtime.update;
        rt.dont_show_again = dont_show;
        if cancel {
            rt.cancel_requested = true;
        }
        if update {
            rt.install_requested = true;
        }
        if restart {
            rt.restart_requested = true;
        }
        if modal.should_close() && !matches!(status, UpdateStatus::Downloading) {
            later = true;
        }
        if ignore {
            if dont_show {
                state.settings.update.skipped_version = info.tag.clone();
                state.settings_dirty = true;
            }
            state.runtime.update.dialog_open = false;
        } else if later {
            state.runtime.update.dialog_open = false;
        }
    }
}

fn source_update_ui(ui: &mut egui::Ui, ctx: &egui::Context) {
    let platform = platform_name();
    egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(tf!("update.source_title", platform)).strong());
        ui.add_space(4.0);
        ui.label(tf!("update.source_body", platform));
        ui.add_space(6.0);
        ui.label(t!("update.source_steps"));
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(SOURCE_UPDATE_COMMANDS).monospace());
            });
        ui.add_space(4.0);
        let copied_id = ui.make_persistent_id("hp_update_source_copied");
        let copied = ctx.data(|d| d.get_temp::<bool>(copied_id)).unwrap_or(false);
        let label = if copied { t!("update.copied") } else { t!("update.btn_copy") };
        if ui.button(label).clicked() {
            ctx.copy_text(SOURCE_UPDATE_COMMANDS.to_owned());
            ctx.data_mut(|d| d.insert_temp(copied_id, true));
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new(t!("update.source_after")).small().weak());
    });
}

pub fn run_helper(args: &[String]) -> i32 {
    let Some(report) = hp_update::apply::run_helper(args) else { return 2 };
    if let Some(error) = &report.error {
        hp_app::register_translations();
        let settings = hp_app::settings::Settings::load_or_default(&hp_app::settings::Settings::default_path());
        hp_app::i18n::set_lang(settings.language);
        let failure = classify(error);
        let text = tf!("update.failed", tf!(failure.key, failure.detail));
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title(t!("update.title"))
            .set_description(format!("{text}\n\n{}", report.log_path.display()))
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
    match hp_update::apply::relaunch(&report.target) {
        Ok(()) if report.error.is_none() => 0,
        Ok(()) => 1,
        Err(_) => 3,
    }
}

fn rt_current(state: &AppState) -> String {
    state.runtime.update.current.clone()
}

fn notes_ui(ui: &mut egui::Ui, notes: &str) {
    if notes.trim().is_empty() {
        ui.label(egui::RichText::new(t!("update.no_notes")).weak());
        return;
    }
    for raw in notes.lines() {
        let line = raw.trim_end().replace("**", "").replace('`', "");
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            ui.add_space(4.0);
        } else if trimmed.starts_with('#') {
            ui.label(egui::RichText::new(trimmed.trim_start_matches('#').trim()).strong());
        } else if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            ui.horizontal_wrapped(|ui| {
                ui.label("\u{2022}");
                ui.label(item);
            });
        } else {
            ui.label(trimmed);
        }
    }
}

#[cfg(test)]
mod tests {
    use hp_update::{Asset, Version};

    use super::*;

    fn release(with_asset: bool) -> Release {
        Release {
            tag: "v1.0.3".into(),
            version: Version::parse("1.0.3").unwrap(),
            title: "Hammer Pro 1.0.3".into(),
            notes: "- a".into(),
            page_url: "https://github.com/oracleidt/hammerpro/releases/tag/v1.0.3".into(),
            assets: if with_asset {
                vec![Asset {
                    name: "HammerPro-1.0.3-windows.zip".into(),
                    url: "https://github.com/x.zip".into(),
                    size: 99,
                    sha256: None,
                }]
            } else {
                Vec::new()
            },
        }
    }

    #[test]
    fn release_info_carries_notes_and_archive() {
        let info = info_of(&release(true));
        assert_eq!((info.tag.as_str(), info.version.as_str(), info.notes.as_str()), ("v1.0.3", "1.0.3", "- a"));
        assert_eq!((info.asset_name.as_deref(), info.asset_size), (Some("HammerPro-1.0.3-windows.zip"), 99));
        assert_eq!(info_of(&release(false)).asset_name, None);
    }

    #[test]
    fn errors_map_to_translated_messages() {
        hp_app::register_translations();
        let _lang = hp_app::testing::lang_lock();
        let cases = [
            UpdateError::Network("x".into()),
            UpdateError::Http(404),
            UpdateError::BadRelease("x".into()),
            UpdateError::BadZip("x".into()),
            UpdateError::Io("x".into()),
            UpdateError::Cancelled,
            UpdateError::Apply("x".into()),
        ];
        for error in cases {
            let f = classify(&error);
            assert!(hp_app::i18n::has_key(f.key), "{}", f.key);
        }
        assert_eq!(classify(&UpdateError::Http(404)).detail, "404");
    }

    #[test]
    fn automated_runs_never_check_or_install_without_the_test_address() {
        if hp_update::api_url_overridden() {
            return;
        }
        let mut controller = UpdateController::new(true);
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        state.runtime.update.check_requested = Some(true);
        state.runtime.update.install_requested = true;
        controller.tick(&mut state, &ctx);
        assert!(controller.check.is_none());
        assert_eq!(state.runtime.update.status, UpdateStatus::Idle);
        assert!(!state.runtime.update.install_requested && state.runtime.update.check_requested.is_none());
        assert!(!controller.arm(), "nothing downloaded: nothing to install");
    }

    fn with_download(result: Result<Downloaded, UpdateError>) -> (UpdateController, AppState) {
        let mut controller = UpdateController::new(false);
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(result).unwrap();
        controller.download = Some(Download { progress: Default::default(), rx });
        let mut state = hp_app::testing::test_state();
        state.runtime.update.status = UpdateStatus::Downloading;
        state.runtime.update.latest = Some(info_of(&release(true)));
        (controller, state)
    }

    #[test]
    fn failed_downloads_leave_the_app_usable_and_nothing_to_install() {
        let ctx = egui::Context::default();
        for error in [
            UpdateError::Network("cut".into()),
            UpdateError::Http(404),
            UpdateError::BadZip("truncated".into()),
            UpdateError::Io("disk full".into()),
        ] {
            let (mut controller, mut state) = with_download(Err(error));
            controller.poll_download(&mut state, &ctx);
            let rt = &state.runtime.update;
            assert_eq!(rt.status, UpdateStatus::Failed);
            assert!(rt.failure.is_some() && !rt.restart_requested);
            assert!(rt.can_install_archive(), "the user can try again");
            assert_eq!(rt.can_install(), PREBUILT_PLATFORM);
            assert!(controller.download.is_none() && controller.ready.is_none());
            assert!(!controller.arm(), "no helper without a verified download");
            controller.spawn_on_exit();
        }
    }

    #[test]
    fn cancelling_returns_to_available_without_an_error() {
        let ctx = egui::Context::default();
        let (mut controller, mut state) = with_download(Err(UpdateError::Cancelled));
        controller.poll_download(&mut state, &ctx);
        assert_eq!(state.runtime.update.status, UpdateStatus::Available);
        assert!(state.runtime.update.failure.is_none() && !state.runtime.update.restart_requested);
        assert!(!controller.arm());

        let (mut controller, mut state) = with_download(Err(UpdateError::Cancelled));
        state.runtime.update.cancel_requested = true;
        controller.tick(&mut state, &ctx);
        let flagged = controller.download.as_ref().is_none_or(|d| d.progress.cancel.load(Ordering::Relaxed));
        assert!(flagged);
        assert!(!state.runtime.update.cancel_requested);
    }

    #[test]
    fn a_release_without_an_archive_cannot_be_installed() {
        let mut controller = UpdateController::new(false);
        controller.release = Some(release(false));
        let mut state = hp_app::testing::test_state();
        state.runtime.update.status = UpdateStatus::Available;
        state.runtime.update.latest = Some(info_of(&release(false)));
        assert!(!state.runtime.update.can_install());
        controller.start_download(&mut state);
        assert!(controller.download.is_none());
        assert_eq!(state.runtime.update.status, UpdateStatus::Available);
    }

    #[test]
    fn exit_only_starts_the_helper_when_armed_with_a_download() {
        let mut controller = UpdateController::new(false);
        controller.spawn_on_exit();
        assert!(!controller.arm());
        controller.disarm();
        controller.spawn_on_exit();
    }
}
