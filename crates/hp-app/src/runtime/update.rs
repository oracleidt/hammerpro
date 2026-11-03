use crate::i18n::TrTable;

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
    pub manual: bool,
    pub api_overridden: bool,
}

impl UpdateRuntime {
    pub fn progress(&self) -> Option<f32> {
        (self.total_bytes > 0).then(|| (self.done_bytes as f64 / self.total_bytes as f64).clamp(0.0, 1.0) as f32)
    }

    pub fn can_install(&self) -> bool {
        matches!(self.status, UpdateStatus::Available | UpdateStatus::Failed)
            && self.latest.as_ref().is_some_and(|l| l.asset_name.is_some())
    }
}

pub fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
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
