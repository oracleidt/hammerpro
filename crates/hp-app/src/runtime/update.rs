use std::time::{Duration, Instant};

use crate::i18n::TrTable;

pub const PREBUILT_PLATFORM: bool = cfg!(windows);

pub const SOURCE_UPDATE_COMMANDS: &str = "git pull
cargo build --release -p hammerpro";

pub fn platform_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Ready,
    Failed,
}

impl UpdateStatus {
    pub fn id(self) -> &'static str {
        match self {
            UpdateStatus::Idle => "idle",
            UpdateStatus::Checking => "checking",
            UpdateStatus::UpToDate => "up_to_date",
            UpdateStatus::Available => "available",
            UpdateStatus::Downloading => "downloading",
            UpdateStatus::Ready => "ready",
            UpdateStatus::Failed => "failed",
        }
    }

    pub fn is_busy(self) -> bool {
        matches!(self, UpdateStatus::Checking | UpdateStatus::Downloading)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateInfo {
    pub tag: String,
    pub version: String,
    pub title: String,
    pub notes: String,
    pub page_url: String,
    pub asset_name: Option<String>,
    pub asset_size: u64,
    pub asset_sha256: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateFailure {
    pub key: &'static str,
    pub detail: String,
}

#[derive(Debug, Default)]
pub struct UpdateRuntime {
    pub status: UpdateStatus,
    pub current: String,
    pub latest: Option<UpdateInfo>,
    pub latest_seen: String,
    pub failure: Option<UpdateFailure>,
    pub dialog_open: bool,
    pub dont_show_again: bool,
    pub check_requested: Option<bool>,
    pub install_requested: bool,
    pub restart_requested: bool,
    pub cancel_requested: bool,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub meter: RateMeter,
    pub manual: bool,
    pub api_overridden: bool,
}

impl UpdateRuntime {
    pub fn progress(&self) -> Option<f32> {
        (self.total_bytes > 0).then(|| (self.done_bytes as f64 / self.total_bytes as f64).clamp(0.0, 1.0) as f32)
    }

    pub fn can_install(&self) -> bool {
        PREBUILT_PLATFORM && self.can_install_archive()
    }

    pub fn can_install_archive(&self) -> bool {
        matches!(self.status, UpdateStatus::Available | UpdateStatus::Failed)
            && self.latest.as_ref().is_some_and(|l| l.asset_name.is_some())
    }

    pub fn speed(&self) -> Option<f64> {
        self.meter.rate()
    }

    pub fn seconds_left(&self) -> Option<f64> {
        let rate = self.meter.rate()?;
        (self.total_bytes > 0).then(|| self.total_bytes.saturating_sub(self.done_bytes) as f64 / rate)
    }
}

const RATE_STEP: Duration = Duration::from_millis(250);
const RATE_WARMUP: Duration = Duration::from_secs(1);
const RATE_SMOOTHING_SECONDS: f64 = 3.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct RateMeter {
    started: Option<Instant>,
    last: Option<(Instant, u64)>,
    rate: f64,
    measured: bool,
}

impl RateMeter {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn sample(&mut self, now: Instant, bytes: u64) {
        let Some((at, before)) = self.last else {
            self.started = Some(now);
            self.last = Some((now, bytes));
            return;
        };
        let dt = now.saturating_duration_since(at);
        if dt < RATE_STEP {
            return;
        }
        let seconds = dt.as_secs_f64();
        let instant = bytes.saturating_sub(before) as f64 / seconds;
        if self.measured {
            let weight = 1.0 - (-seconds / RATE_SMOOTHING_SECONDS).exp();
            self.rate += (instant - self.rate) * weight;
        } else {
            self.rate = instant;
            self.measured = true;
        }
        self.last = Some((now, bytes));
        if self.started.is_some_and(|s| now.saturating_duration_since(s) >= RATE_WARMUP) && self.rate > 0.0 {
            self.started = None;
        }
    }

    pub fn rate(&self) -> Option<f64> {
        (self.measured && self.started.is_none() && self.rate > 0.0).then_some(self.rate)
    }
}

pub fn megabytes(bytes: u64) -> String {
    decimal_megabytes(bytes as f64)
}

fn decimal_megabytes(bytes: f64) -> String {
    let value = bytes / 1_000_000.0;
    let text = format!("{value:.1}");
    match crate::i18n::lang() {
        crate::i18n::Lang::En => text,
        _ => text.replace('.', ","),
    }
}

pub fn speed_text(bytes_per_second: f64) -> String {
    decimal_megabytes(bytes_per_second)
}

pub fn time_left_text(seconds: f64) -> String {
    let total = seconds.max(1.0).round() as u64;
    if total < 60 {
        crate::tf!("update.seconds", total)
    } else {
        let minutes = total / 60;
        crate::tf!("update.minutes", minutes, format!("{:02}", total % 60))
    }
}

pub const TR: TrTable = &[
    ("update.title", "Update available", "Mise à jour disponible"),
    ("update.heading", "Hammer Pro {0} is available", "Hammer Pro {0} est disponible"),
    ("update.current", "You are using version {0}.", "Vous utilisez la version {0}."),
    ("update.notes", "What's new", "Nouveautés"),
    ("update.no_notes", "This release has no notes.", "Cette version n'a pas de notes."),
    ("update.btn_update", "Update", "Mettre à jour"),
    ("update.btn_ignore", "Ignore", "Ignorer"),
    ("update.btn_later", "Later", "Plus tard"),
    ("update.btn_cancel", "Cancel", "Annuler"),
    ("update.btn_retry", "Try again", "Réessayer"),
    ("update.btn_restart", "Restart and install", "Redémarrer et installer"),
    ("update.btn_page", "Open the release page", "Ouvrir la page de la version"),
    ("update.dont_show", "Don't show this again", "Ne plus afficher"),
    (
        "update.dont_show_tip",
        "Skips this version only: the dialog comes back for the next release. Turn the startup check off in Settings > Updates to stop all of them.",
        "Ne saute que cette version : la fenêtre revient pour la version suivante. Désactivez la vérification au démarrage dans Options > Mises à jour pour les arrêter toutes.",
    ),
    (
        "update.downloading",
        "Downloading the update: {0} MB of {1} MB",
        "Téléchargement de la mise à jour : {0} Mo sur {1} Mo",
    ),
    ("update.downloading_unknown", "Downloading the update: {0} MB", "Téléchargement de la mise à jour : {0} Mo"),
    ("update.speed", "{0} MB/s, about {1} left", "{0} Mo/s, environ {1} restantes"),
    ("update.speed_only", "{0} MB/s", "{0} Mo/s"),
    ("update.measuring", "Measuring the download speed...", "Mesure de la vitesse de téléchargement..."),
    ("update.seconds", "{0} s", "{0} s"),
    ("update.minutes", "{0} min {1} s", "{0} min {1} s"),
    (
        "update.ready",
        "The update is downloaded. Hammer Pro closes, installs it and starts again.",
        "La mise à jour est téléchargée. Hammer Pro se ferme, l'installe et redémarre.",
    ),
    (
        "update.restarting",
        "Closing Hammer Pro to install the update...",
        "Fermeture de Hammer Pro pour installer la mise à jour...",
    ),
    ("update.failed", "The update failed: {0}", "La mise à jour a échoué : {0}"),
    (
        "update.no_asset",
        "This release has no Windows archive to install. Download it from the release page.",
        "Cette version n'a pas d'archive Windows à installer. Téléchargez-la depuis la page de la version.",
    ),
    (
        "update.busy_compile",
        "A compile is running. Wait for it to end, then update.",
        "Une compilation est en cours. Attendez la fin, puis mettez à jour.",
    ),
    (
        "update.busy_automated",
        "Updates cannot be installed in an automated run.",
        "Les mises à jour ne peuvent pas être installées pendant une exécution automatisée.",
    ),
    (
        "update.err_network",
        "no connection to the update server ({0})",
        "pas de connexion au serveur de mises à jour ({0})",
    ),
    (
        "update.err_http",
        "the update server answered with error {0}",
        "le serveur de mises à jour a répondu par l'erreur {0}",
    ),
    ("update.err_release", "the release is not usable ({0})", "la version n'est pas utilisable ({0})"),
    (
        "update.err_zip",
        "the downloaded archive is not usable ({0})",
        "l'archive téléchargée n'est pas utilisable ({0})",
    ),
    ("update.err_io", "a file could not be written ({0})", "un fichier n'a pas pu être écrit ({0})"),
    ("update.source_title", "Update from the source on {0}", "Mise à jour depuis les sources sur {0}"),
    (
        "update.source_body",
        "Ready made builds of Hammer Pro are published for Windows only, so this version cannot install itself on {0}. Your copy was built from the source: get the new version by updating your clone and building it again for your system.",
        "Les versions prêtes à l'emploi de Hammer Pro ne sont publiées que pour Windows, cette version ne peut donc pas s'installer seule sur {0}. Votre copie a été compilée depuis les sources : récupérez la nouvelle version en mettant à jour votre clone puis en la compilant à nouveau pour votre système.",
    ),
    ("update.source_steps", "In the folder of your clone, run:", "Dans le dossier de votre clone, lancez :"),
    (
        "update.source_after",
        "Then start the new build from target/release. A fresh clone works too if your copy is old or changed.",
        "Puis lancez la nouvelle version depuis target/release. Un nouveau clone fonctionne aussi si votre copie est ancienne ou modifiée.",
    ),
    ("update.btn_copy", "Copy the commands", "Copier les commandes"),
    ("update.copied", "Copied", "Copié"),
    (
        "update.status_source",
        "Version {0} is available. Update your clone and build it again (Windows only gets ready made builds).",
        "La version {0} est disponible. Mettez à jour votre clone et compilez-le à nouveau (seul Windows a des versions prêtes à l'emploi).",
    ),
    (
        "update.err_source_only",
        "ready made builds are for Windows only: on {0}, update the source (git pull) and build again with cargo build --release -p hammerpro",
        "les versions prêtes à l'emploi sont pour Windows uniquement : sur {0}, mettez à jour les sources (git pull) et compilez à nouveau avec cargo build --release -p hammerpro",
    ),
    ("update.err_cancelled", "the download was cancelled", "le téléchargement a été annulé"),
    ("update.err_other", "{0}", "{0}"),
    ("update.toast_up_to_date", "Hammer Pro is up to date (version {0}).", "Hammer Pro est à jour (version {0})."),
    ("update.toast_failed", "Could not check for updates: {0}", "Impossible de rechercher les mises à jour : {0}"),
    ("update.toast_available", "Hammer Pro {0} is available.", "Hammer Pro {0} est disponible."),
    ("update.page", "Updates", "Mises à jour"),
    ("update.heading_settings", "Updates", "Mises à jour"),
    (
        "update.intro",
        "Hammer Pro looks for a newer release on GitHub in the background and offers to install it. Nothing is downloaded before you click Update.",
        "Hammer Pro cherche une version plus récente sur GitHub en arrière-plan et propose de l'installer. Rien n'est téléchargé avant que vous cliquiez sur Mettre à jour.",
    ),
    ("update.check_on_startup", "Check for updates on startup", "Rechercher les mises à jour au démarrage"),
    (
        "update.show_dialog",
        "Show the update window when a new version is out",
        "Afficher la fenêtre de mise à jour quand une nouvelle version sort",
    ),
    (
        "update.show_dialog_hint",
        "Turn this back on to undo Don't show this again, whichever version you hid.",
        "Réactivez cette option pour annuler Ne plus afficher, quelle que soit la version masquée.",
    ),
    ("update.check_now", "Check for updates now", "Rechercher maintenant"),
    ("update.show_update", "Show the update", "Afficher la mise à jour"),
    ("update.current_version", "Installed version: {0}", "Version installée : {0}"),
    ("update.status_idle", "No check done yet in this session.", "Aucune recherche faite dans cette session."),
    ("update.status_checking", "Checking...", "Recherche en cours..."),
    (
        "update.status_up_to_date",
        "Hammer Pro is up to date (latest release: {0}).",
        "Hammer Pro est à jour (dernière version : {0}).",
    ),
    ("update.status_available", "Version {0} is available.", "La version {0} est disponible."),
    ("update.status_downloading", "Downloading version {0}...", "Téléchargement de la version {0}..."),
    ("update.status_ready", "Version {0} is ready to install.", "La version {0} est prête à être installée."),
    ("update.status_failed", "The last check failed: {0}", "La dernière recherche a échoué : {0}"),
    ("update.skipped", "Skipped version: {0}", "Version ignorée : {0}"),
    ("update.clear_skipped", "Stop skipping", "Ne plus ignorer"),
    (
        "update.note",
        "The check is one request to the GitHub releases API of the Hammer Pro project, made in the background: the editor never waits for it and works offline.",
        "La recherche est une seule requête vers l'API des versions GitHub du projet Hammer Pro, faite en arrière-plan : l'éditeur ne l'attend jamais et fonctionne hors ligne.",
    ),
    (
        "update.override_note",
        "The update address is replaced by the HP_UPDATE_API_URL environment variable (test mode).",
        "L'adresse des mises à jour est remplacée par la variable d'environnement HP_UPDATE_API_URL (mode test).",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_meter_waits_then_smooths() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut m = RateMeter::default();
        m.sample(at(0), 0);
        m.sample(at(500), 1_000_000);
        assert_eq!(m.rate(), None);
        m.sample(at(1000), 2_000_000);
        let first = m.rate().expect("rate after one second");
        assert!((first - 2_000_000.0).abs() < 1.0, "{first}");
        m.sample(at(1100), 2_000_000);
        assert!((m.rate().unwrap() - first).abs() < 1.0, "samples closer than the step are ignored");
        m.sample(at(2000), 2_000_000);
        let slowed = m.rate().unwrap();
        assert!(slowed < first && slowed > first * 0.5, "a stall lowers the rate smoothly: {slowed}");
        m.reset();
        assert_eq!(m.rate(), None);
    }

    #[test]
    fn time_left_and_sizes_read_like_github() {
        crate::i18n::register(TR);
        crate::i18n::with_lang(crate::i18n::Lang::En, || {
            assert_eq!(megabytes(43_456_789), "43.5");
            let mut rt = UpdateRuntime { done_bytes: 3_000_000, total_bytes: 43_000_000, ..Default::default() };
            let t0 = Instant::now();
            rt.meter.sample(t0, 0);
            rt.meter.sample(t0 + Duration::from_secs(1), 2_000_000);
            assert_eq!(rt.seconds_left().map(|s| s.round()), Some(20.0));
            assert_eq!(time_left_text(20.0), "20 s");
            assert_eq!(time_left_text(125.4), "2 min 05 s");
            assert_eq!(speed_text(2_000_000.0), "2.0");
        });
        crate::i18n::with_lang(crate::i18n::Lang::Fr, || assert_eq!(megabytes(43_456_789), "43,5"));
    }
}
