use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use hp_app::runtime::collaboration::{CollaborationRequest, CollaborationStats};
use hp_app::{AppState, Severity, t};
use hp_collab::{Command, Event, Invitation, Presence, Session, Unit};
use zeroize::Zeroizing;
pub(crate) mod access;
mod install;
mod overlay;
mod panel;
mod people;
mod reconnect;
mod resync;
mod snapshots;
mod sync;
use hp_collab::document::ValidatedSnapshot;
pub(crate) use overlay::TeamLayer;
use sync::{Pending, PersonalEdit};

#[cfg(test)]
mod tests;

pub(crate) fn tr(en: &'static str, fr: &'static str, ru: &'static str) -> &'static str {
    match hp_app::i18n::lang() {
        hp_app::i18n::Lang::Fr => fr,
        hp_app::i18n::Lang::Ru => ru,
        _ => en,
    }
}

pub const TR: hp_app::i18n::TrTable = &[
    (
        "team.rejected",
        "Edit rejected, the shared version is shown and your version is kept in Recover local copy",
        "Modification refusée, la version partagée est affichée et ta version est conservée dans Récupérer la copie locale",
    ),
    (
        "team.conflict",
        "Conflict: another participant changed the same object first. Your version is kept in Recover local copy.",
        "Conflit : un autre participant a modifié le même objet avant toi. Ta version est conservée dans Récupérer la copie locale.",
    ),
    (
        "team.undo_refused",
        "Undo refused: another participant changed these objects since",
        "Annulation refusée : un autre participant a modifié ces objets depuis",
    ),
    ("team.receiving", "Receiving the shared map", "Réception de la map partagée"),
    ("team.preparing", "Preparing the shared map", "Préparation de la map partagée"),
    ("team.progress", "{0} of {1} objects, {2} received", "{0} objets sur {1}, {2} reçus"),
    (
        "team.sync",
        "Round trip {0} ms (average {1} ms) | Remote edit applied in {2} ms (max {3} ms) | Capture {4} ms",
        "Aller-retour {0} ms (moyenne {1} ms) | Modification distante appliquée en {2} ms (max {3} ms) | Capture {4} ms",
    ),
    ("team.edits", "Edits sent {0}, received {1}, live {2}", "Modifications envoyées {0}, reçues {1}, en direct {2}"),
    (
        "team.live_edits",
        "Show my strokes and drags to the others while I edit",
        "Montrer mes coups de pinceau et déplacements aux autres pendant l'édition",
    ),
    (
        "team.live_edits_tip",
        "Sculpting, painting and other edits that stay open while the mouse is held are sent several times per second, so the others see them as they happen. The final state is always sent when the edit ends. Turn off to send only the final state.",
        "Le sculpt, la peinture et les autres modifications qui restent ouvertes tant que la souris est enfoncée sont envoyés plusieurs fois par seconde, les autres les voient en direct. L'état final est toujours envoyé à la fin. Désactive pour n'envoyer que l'état final.",
    ),
    ("team.conflicts", "{0} objects kept in the local copy", "{0} objets conservés dans la copie locale"),
    ("team.recovery_saved", "Local copy saved", "Copie locale enregistrée"),
    (
        "team.local_only",
        "{0} values kept only on this PC (refused by Team Mapping)",
        "{0} valeurs gardées uniquement sur ce PC (refusées par Team Mapping)",
    ),
    ("team.joined", "{0} joined the session", "{0} a rejoint la session"),
    ("team.entity", "entity {0}", "entité {0}"),
    ("team.brush", "brush {0}", "brush {0}"),
    ("team.pinged", "{0} pinged a point", "{0} a signalé un point"),
    ("team.ping_line", "pinged a point", "a signalé un point"),
    ("team.conflict_label", "Conflict", "Conflit"),
    (
        "team.conflict_by",
        "Conflict: {0} changed {1} first. Your version is kept in Recover local copy.",
        "Conflit : {0} a modifié {1} avant toi. Ta version est conservée dans Récupérer la copie locale.",
    ),
    (
        "team.conflict_on",
        "Conflict on {0}: another participant changed it first. Your version is kept in Recover local copy.",
        "Conflit sur {0} : un autre participant l'a modifié avant toi. Ta version est conservée dans Récupérer la copie locale.",
    ),
    (
        "team.locked_by",
        "Refused: {1} is selected by {0}. Your version is kept in Recover local copy.",
        "Refusé : {1} est sélectionné par {0}. Ta version est conservée dans Récupérer la copie locale.",
    ),
    ("team.follow_stopped", "Stopped following {0}", "Tu ne suis plus {0}"),
    ("team.connection_lost", "Connection lost: {0}", "Connexion perdue : {0}"),
    ("team.reconnecting", "Reconnecting (attempt {0} of {1})", "Reconnexion (tentative {0} sur {1})"),
    ("team.reconnect_wait", "{0}. New attempt in {1} s", "{0}. Nouvel essai dans {1} s"),
    (
        "team.reconnect_failed",
        "Could not reconnect: {0}. The map stays open on this PC.",
        "Reconnexion impossible : {0}. La map reste ouverte sur ce PC.",
    ),
    (
        "team.reconnect_no_credentials",
        "The invitation of this session is no longer available",
        "L'invitation de cette session n'est plus disponible",
    ),
    ("team.resync_reason", "Resynchronizing with the host", "Resynchronisation avec l'hôte"),
    ("team.reconnected", "Reconnected to the session", "Reconnecté à la session"),
    (
        "team.reconnected_resent",
        "Reconnected. {0} objects changed while offline were sent again.",
        "Reconnecté. {0} objets modifiés hors ligne ont été renvoyés.",
    ),
    (
        "team.offline_kept",
        "{0} objects changed while offline were also changed by someone else: your versions are kept in Recover local copy.",
        "{0} objets modifiés hors ligne ont aussi été modifiés par quelqu'un d'autre : tes versions sont conservées dans Récupérer la copie locale.",
    ),
    ("team.you", "you", "toi"),
    ("team.follow", "Follow", "Suivre"),
    (
        "team.follow_tip",
        "Your 3D camera follows this participant and the 2D views stay centered on them. Move the camera to stop.",
        "Ta caméra 3D suit ce participant et les vues 2D restent centrées sur lui. Bouge la caméra pour arrêter.",
    ),
    ("team.more", "More", "Plus"),
    ("team.show_selection", "Show their selection", "Voir sa sélection"),
    ("team.editing", "editing", "modifie"),
    (
        "team.row_tip",
        "Ping: network round trip to the host. Traffic: data this participant sends and receives per second. Edit: average time for their edits to be confirmed by the host.",
        "Ping : aller-retour réseau jusqu'à l'hôte. Trafic : données envoyées et reçues par seconde par ce participant. Modif : temps moyen pour que l'hôte confirme ses modifications.",
    ),
    ("team.traffic_rate", "up {0}/s, down {1}/s", "envoi {0}/s, réception {1}/s"),
    ("team.edit_time", "edit {0} ms", "modif {0} ms"),
    ("team.chat", "Chat", "Discussion"),
    ("team.chat_empty", "No messages yet", "Aucun message pour l'instant"),
    ("team.message_hint", "Message to everyone", "Message à tous"),
    ("team.send", "Send", "Envoyer"),
    ("team.ping", "Ping", "Signaler"),
    (
        "team.ping_tip",
        "Shows a marker to everyone for 8 seconds at the point in the centre of your 3D view. Right click in a view and choose Ping here to mark another point.",
        "Affiche un repère à tous pendant 8 secondes au point situé au centre de ta vue 3D. Fais un clic droit dans une vue et choisis Signaler ici pour marquer un autre point.",
    ),
    ("team.go_to", "Go to", "Aller à"),
    (
        "team.ping_needs_shared_map",
        "Switch to the shared map to ping a point",
        "Reviens à la map partagée pour signaler un point",
    ),
    ("team.sync_details", "Synchronization", "Synchronisation"),
    ("team.host_tools", "Host tools", "Outils de l'hôte"),
    ("team.reconnect_now", "Reconnect now", "Se reconnecter maintenant"),
    ("team.try_again", "Try again", "Réessayer"),
    ("team.stay_offline", "Stay offline", "Rester hors ligne"),
    ("team.resync", "Resync", "Resynchroniser"),
    (
        "team.resync_tip",
        "Reconnects and downloads the shared map again. Your edits that were not confirmed yet are sent again, or kept in Recover local copy if someone changed the same objects.",
        "Se reconnecte et télécharge à nouveau la map partagée. Tes modifications pas encore confirmées sont renvoyées, ou conservées dans Récupérer la copie locale si quelqu'un a modifié les mêmes objets.",
    ),
    (
        "team.reconnect_info",
        "Your map stays open. Edits you make meanwhile are sent when the connection is back, or kept in Recover local copy if someone changed the same objects.",
        "Ta map reste ouverte. Les modifications faites entre-temps sont envoyées au retour de la connexion, ou conservées dans Récupérer la copie locale si quelqu'un a modifié les mêmes objets.",
    ),
    (
        "team.reconnect_next",
        "Next attempt in {0} s (attempt {1} of {2})",
        "Prochain essai dans {0} s (tentative {1} sur {2})",
    ),
    (
        "team.locks",
        "{0} also has {1} of your selected objects selected: changes to them are refused until one of you deselects them.",
        "{0} a aussi sélectionné {1} de tes objets sélectionnés : leurs modifications sont refusées tant que l'un de vous ne les désélectionne pas.",
    ),
    ("team.lock_pill", "Also selected by {0}: {1} objects", "Aussi sélectionnés par {0} : {1} objets"),
    ("team.following_pill", "Following {0}. Move the camera to stop.", "Tu suis {0}. Bouge la caméra pour arrêter."),
    ("team.not_connected", "Team Mapping is not connected", "Team Mapping n'est pas connecté"),
];

fn teleport_camera(state: &mut AppState, position: [f64; 3], direction: [f64; 3]) {
    let position = glam::DVec3::from_array(position);
    if !position.is_finite() || position.abs().max_element() > 1_000_000.0 {
        return;
    }
    state.cameras.cam3d.pos = position;
    if let Some(direction) = glam::DVec3::from_array(direction).try_normalize() {
        state.cameras.cam3d.look_at(position + direction);
    }
    state.cameras.center_2d(position);
    if let Some(view) = state.settings.ui.quadrants.iter().position(|kind| *kind == 0) {
        state.ui.focused_view = Some(view);
        if state.ui.maximized_view.is_some() {
            state.ui.maximized_view = Some(view);
        }
    }
}

fn size_text(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    }
}

#[derive(Default)]
pub(crate) struct Controller {
    access: access::Access,
    denied: Option<hp_collab::protocol::Refusal>,
    script_rescan: bool,
    snapshots: snapshots::Tasks,
    session: Option<Session>,
    pub(crate) name: String,
    session_name: String,
    connection_mode: hp_collab::ConnectionMode,
    joining: bool,
    direct_address: String,
    port: u16,
    host_addresses: Vec<std::net::SocketAddr>,
    identity: Option<Zeroizing<[u8; 32]>>,
    bans: Vec<hp_collab::moderation::BanRow>,
    ban_target: Option<u32>,
    ban_minutes: u64,
    ban_reason: String,
    had_error: bool,
    password: Zeroizing<String>,
    invitation_input: Zeroizing<String>,
    invitation: Zeroizing<String>,
    show_password: bool,
    status: String,
    host: bool,
    bound_tab: u64,
    selected_tab: u64,
    pending_host: Option<u64>,
    pending_switch: Option<u64>,
    pending_look: Option<[f64; 3]>,
    pending_teleport: Option<([f64; 3], [f64; 3])>,
    participant: u32,
    id_start: u32,
    revision: u64,
    sequence: u64,
    canonical: Option<ValidatedSnapshot>,
    pending: Option<Pending>,
    live: Option<BTreeMap<String, Option<Unit>>>,
    last_live: Option<Instant>,
    held: BTreeSet<String>,
    undo: VecDeque<PersonalEdit>,
    redo: Vec<PersonalEdit>,
    deferred: VecDeque<Event>,
    conflict: BTreeMap<String, Option<Unit>>,
    recovery_base: Option<hp_collab::Snapshot>,
    local_only: BTreeSet<String>,
    loading: Option<install::Loading>,
    applying: Option<sync::Applying>,
    capturing: Option<sync::Capturing>,
    starting: Option<install::Starting>,
    building: Option<install::Building>,
    recovery: Option<install::Job<()>>,
    people: people::People,
    reconnect: Option<reconnect::Reconnect>,
    credentials: Option<reconnect::Credentials>,
    last_error: String,
    chat_input: String,
    waker: bool,
    joined_at: Option<Instant>,
    stats: CollaborationStats,
    last_tick: Option<Instant>,
    last_presence: Option<Presence>,
    last_content_serial: u64,
    sent: u64,
    received: u64,
}

impl Controller {
    pub(crate) fn active(&self) -> bool {
        self.session.is_some() || self.starting.is_some() || self.building.is_some() || self.reconnect.is_some()
    }

    pub(crate) fn stop(&mut self, state: &mut AppState) {
        let canonical = self.canonical.take().or_else(|| self.reconnect.take().and_then(|reconnect| reconnect.base));
        if !self.conflict.is_empty()
            && let Some(map) = canonical
        {
            self.recovery_base = Some(map.into_snapshot());
        }
        self.reconnect = None;
        self.credentials = None;
        self.halt(state);
        let runtime = &mut state.runtime.collaboration;
        runtime.chat.clear();
        runtime.unread = 0;
        runtime.reconnecting = None;
        self.chat_input.clear();
    }

    fn halt(&mut self, state: &mut AppState) {
        self.session = None;
        self.starting = None;
        self.building = None;
        self.loading = None;
        self.applying = None;
        self.capturing = None;
        self.canonical = None;
        self.pending = None;
        self.live = None;
        self.last_live = None;
        self.held.clear();
        self.deferred.clear();
        self.undo.clear();
        self.redo.clear();
        self.bans.clear();
        self.access.clear();
        self.last_presence = None;
        self.pending_look = None;
        self.pending_teleport = None;
        self.pending_switch = None;
        self.snapshots = snapshots::Tasks::default();
        self.invitation.clear();
        self.participant = 0;
        self.waker = false;
        self.joined_at = None;
        self.people.clear_session();
        let runtime = &mut state.runtime.collaboration;
        runtime.connected = false;
        runtime.keep_scripts_local = false;
        runtime.active_map = false;
        runtime.can_undo = false;
        runtime.can_redo = false;
        runtime.participants.clear();
        runtime.following = None;
        runtime.markers.clear();
        runtime.highlights.clear();
        runtime.locks.clear();
        runtime.overlay_serial += 1;
        self.publish(state);
        self.publish_reconnect(state);
        state.doc.mark_everything_changed();
    }

    fn message(&mut self, state: &mut AppState, text: impl ToString) {
        self.status = text.to_string();
        state.notify(Severity::Warning, &self.status);
    }

    fn identity(&mut self, persistent: bool) -> Result<Zeroizing<[u8; 32]>> {
        if self.identity.is_none() {
            self.identity = Some(if persistent {
                hp_collab::identity::load_or_create(
                    &hp_app::settings::Settings::config_dir().join("team-mapping-identity.key"),
                )?
            } else {
                Zeroizing::new(hp_collab::protocol::random_secret()?)
            });
        }
        Ok(self.identity.as_ref().cloned().unwrap())
    }

    fn publish(&self, state: &mut AppState) {
        let mut stats = self.stats.clone();
        stats.host = self.host;
        stats.session = if self.active() { self.session_name.clone() } else { String::new() };
        stats.status = self.status.clone();
        stats.invitation = if self.host { self.invitation.to_string() } else { String::new() };
        stats.revision = self.revision;
        stats.pending = self.pending.is_some() || self.capturing.is_some();
        stats.loading = self.loading.is_some()
            || self.starting.is_some()
            || self.building.is_some()
            || (self.active() && self.canonical.is_none());
        stats.sent_bytes = self.sent;
        stats.received_bytes = self.received;
        stats.conflicts = self.conflict.len();
        stats.local_only = self.local_only.iter().cloned().collect();
        stats.held = self.held.len();
        if state.runtime.collaboration.stats != stats {
            state.runtime.collaboration.stats = stats;
        }
    }

    fn adopt(&mut self, state: &mut AppState, installed: install::Installed, revision: u64, reset: bool) -> Result<()> {
        let offline = match self.reconnect.take() {
            Some(reconnect) => Some(self.offline_changes(state, reconnect)?),
            None => None,
        };
        let install::Installed { map, document } = installed;
        state.doc.adopt_shared(document, self.id_start, self.id_start + hp_collab::document::ID_STRIDE)?;
        self.last_content_serial = state.doc.content_serial();
        self.canonical = Some(map);
        self.revision = revision;
        self.held.clear();
        state.runtime.collaboration.connected = true;
        state.runtime.collaboration.local_id = self.participant;
        if let Some(joined) = self.joined_at.take() {
            self.stats.join_ms = joined.elapsed().as_secs_f64() * 1000.0;
        }
        if reset {
            state.notify(
                Severity::Info,
                tr(
                    "The host restored a session snapshot",
                    "L'hôte a restauré un snapshot de la session",
                    "Хост восстановил снимок сеанса",
                ),
            );
        } else {
            self.status = tr("Connected", "Connecté", "Подключено").into();
        }
        if let Some(offline) = offline {
            self.resend_offline(state, offline)?;
            self.publish_reconnect(state);
        }
        Ok(())
    }

    fn process(&mut self, state: &mut AppState, event: Event) -> Result<()> {
        match event {
            Event::Ready { invitation, participant, id_start, revision, map, direct_addresses } => {
                self.host_addresses = direct_addresses;
                self.participant = participant;
                self.id_start = id_start;
                self.revision = revision;
                self.invitation = Zeroizing::new(invitation.unwrap_or_default());
                if self.host {
                    state.doc.configure_shared_ids(id_start, id_start + hp_collab::document::ID_STRIDE)?;
                    self.canonical = Some(map);
                    state.runtime.collaboration.connected = true;
                    state.runtime.collaboration.local_id = participant;
                    self.status = tr("Connected", "Connecté", "Подключено").into();
                } else {
                    state.doc.set_path(None);
                    self.loading = Some(install::install(map, revision, false)?);
                    self.status = t!("team.preparing").into();
                }
            }
            Event::Applied { author, sequence, revision, changes } => {
                if author == self.participant
                    && self.pending.as_ref().is_some_and(|pending| pending.sequence == sequence)
                {
                    self.acknowledge(state, revision, changes)?;
                } else {
                    self.apply_remote(state, author, revision, changes)?;
                }
            }
            Event::Rejected { sequence, rejection, revision } => {
                self.refused(state, sequence, &rejection, revision)?;
            }
            Event::Resync { revision, outcomes, map } => self.on_resync(state, revision, &outcomes, map)?,
            Event::Access(access) => self.on_access(state, access),
            Event::Waiting => self.on_waiting(),
            Event::Denied(refused) => self.on_denied(state, &refused),
            Event::Participants(rows) => {
                self.remember_roles(&rows);
                self.on_participants(state, rows);
            }
            Event::Bans(rows) => {
                self.bans = rows;
            }
            Event::Reset { revision, map } => {
                if let Some(pending) = self.pending.as_mut() {
                    pending.detached = true;
                }
                self.live = None;
                self.held.clear();
                self.undo.clear();
                self.redo.clear();
                self.canonical = None;
                self.loading = Some(install::install(map, revision, true)?);
                if self.host {
                    self.snapshots.refresh();
                }
            }
            Event::Invitation(value) => {
                self.invitation = Zeroizing::new(value);
                self.status = tr(
                    "Access rotated. Previous invitations revoked and guests disconnected.",
                    "Accès renouvelés. Anciens codes révoqués et invités déconnectés.",
                    "Доступ обновлён. Старые приглашения отозваны, гости отключены.",
                )
                .into();
            }
            Event::Traffic { sent, received } => {
                self.sent = sent;
                self.received = received;
            }
            Event::JoinProgress { units, total, bytes } => {
                self.stats.join_units = units;
                self.stats.join_total = total;
                self.stats.join_bytes = bytes;
                self.status = t!("team.receiving").into();
            }
            Event::NetworkPrivacy { relay_only, published_ip_count, direct_path_count } => {
                if relay_only {
                    ensure!(
                        published_ip_count == 0 && direct_path_count == 0,
                        "Direct address detected in a relay session"
                    );
                }
            }
            Event::Error(error) => {
                self.had_error = true;
                if !self.reconnectable(&error) {
                    self.message(state, &error);
                }
                self.last_error = error;
            }
            Event::Closed => {
                let error = std::mem::take(&mut self.last_error);
                if self.reconnect.is_some() && self.canonical.is_none() {
                    self.attempt_failed(state, error);
                } else if self.reconnectable(&error) {
                    self.connection_lost(state, error, false);
                } else {
                    self.stop(state);
                    if !self.had_error {
                        self.status = tr(
                            "Disconnected. Your local map is retained.",
                            "Déconnecté. Ta map locale est conservée.",
                            "Отключено. Локальная карта сохранена.",
                        )
                        .into();
                    }
                }
            }
        }
        Ok(())
    }

    fn poll_jobs(&mut self, state: &mut AppState) -> Result<()> {
        if let Some(building) = &mut self.building
            && let Some(starting) = building.step(state, Duration::from_millis(4))?
        {
            self.building = None;
            self.starting = Some(starting);
        }
        if let Some(starting) = &self.starting
            && let Some(result) = starting.job.poll()
        {
            let serial = starting.serial;
            self.starting = None;
            let (session, problems) = result.map_err(|error| anyhow::anyhow!(error))?;
            self.session = Some(session);
            self.report_local_only(state, problems);
            self.last_content_serial = serial;
            self.waker = false;
        }
        if let Some(loading) = &self.loading
            && !state.doc.in_transaction()
            && let Some(result) = loading.job.poll()
        {
            let (revision, reset) = (loading.revision, loading.reset);
            self.loading = None;
            let installed = result.map_err(|error| anyhow::anyhow!(error))?;
            self.adopt(state, installed, revision, reset)?;
        }
        if let Some(recovery) = &self.recovery
            && let Some(result) = recovery.poll()
        {
            self.recovery = None;
            match result {
                Ok(()) => {
                    self.conflict.clear();
                    self.recovery_base = None;
                    state.notify(Severity::Info, t!("team.recovery_saved"));
                }
                Err(error) => self.message(state, error),
            }
        }
        Ok(())
    }

    pub(crate) fn tick(&mut self, state: &mut AppState, ctx: &egui::Context, tab: u64) {
        if !self.active() {
            return;
        }
        debug_assert_eq!(self.bound_tab, tab);
        if self.session.is_none() && self.reconnect.is_some() {
            self.reconnect_tick(state, ctx);
            self.publish(state);
            if self.session.is_none() {
                return;
            }
        }
        if !self.waker
            && let Some(session) = &self.session
        {
            let context = ctx.clone();
            session.set_waker(std::sync::Arc::new(move || context.request_repaint()));
            self.waker = true;
        }
        self.sync_script_policy(state);
        let idle = self.pending.is_none() && self.capturing.is_none();
        state.runtime.collaboration.can_undo = !self.undo.is_empty() && idle;
        state.runtime.collaboration.can_redo = !self.redo.is_empty() && idle;
        if let Some(position) = self.pending_look.take() {
            state.cameras.cam3d.look_at(glam::DVec3::from_array(position));
        }
        if let Some((position, direction)) = self.pending_teleport.take() {
            self.stop_following(state, false);
            teleport_camera(state, position, direction);
        }
        self.apply_camera_requests(state, ctx);
        let revision = state.doc.revision();
        let result = (|| -> Result<()> {
            self.poll_jobs(state)?;
            self.finish_applying(state)?;
            self.resolve_held(state)?;
            if let Some(session) = &self.session {
                self.deferred.extend(session.events.try_iter().take(512));
            }
            while let Some(event) = self.deferred.pop_front() {
                let waits = matches!(
                    event,
                    Event::Applied { .. } | Event::Rejected { .. } | Event::Reset { .. } | Event::Resync { .. }
                );
                if waits && (self.loading.is_some() || self.applying.is_some() || self.canonical.is_none()) {
                    self.deferred.push_front(event);
                    break;
                }
                self.process(state, event)?;
                if !self.active() {
                    return Ok(());
                }
            }
            if self.host {
                let (restore, due) = self.snapshots.tick(self.canonical.is_some());
                if let Some(map) = restore
                    && let (Some(session), Some(store)) = (&self.session, self.snapshots.store())
                    && let Err(error) = session.send(Command::Restore { map, store })
                {
                    self.message(state, error);
                }
                if due {
                    self.request_snapshot(state);
                }
            }
            self.capture(state)
        })();
        if let Err(error) = result {
            self.stop(state);
            self.message(state, format!("{error:#}"));
            return;
        }
        if self.session.is_some() && self.canonical.is_some() {
            self.refresh_people(state, ctx);
        }
        self.publish(state);
        if state.doc.revision() != revision || self.building.is_some() {
            ctx.request_repaint();
        } else if self.starting.is_some()
            || self.loading.is_some()
            || self.applying.is_some()
            || self.capturing.is_some()
            || self.recovery.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(15));
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        if self.canonical.is_none() || self.session.is_none() {
            return;
        }
        let now = Instant::now();
        let released = self.people.release(now).is_some();
        if self.people.waiting() {
            ctx.request_repaint_after(people::NOTE_SPACING);
        }
        if !released && self.last_tick.is_some_and(|last| now.duration_since(last) < Duration::from_millis(100)) {
            return;
        }
        self.last_tick = Some(now);
        let selected: Vec<String> = state
            .doc
            .selection()
            .objects
            .iter()
            .map(people::object_key)
            .chain(state.doc.selection().faces.iter().map(|face| format!("solid:{}", face.solid)))
            .take(2048)
            .collect();
        self.people.set_selection(&selected);
        let presence = Presence {
            position: state.cameras.cam3d.pos.to_array(),
            direction: state.cameras.cam3d.forward().to_array(),
            selected,
            notes: self.people.live_notes(),
            activity: self.people.rates,
        };
        if self.last_presence.as_ref() != Some(&presence)
            && self.session.as_ref().is_some_and(|session| session.send(Command::Presence(presence.clone())).is_ok())
        {
            self.last_presence = Some(presence);
        }
    }

    fn request_snapshot(&mut self, state: &mut AppState) {
        let Some(session) = &self.session else { return };
        if let Some((store, done)) = self.snapshots.begin_save()
            && let Err(error) = session.send(Command::Snapshot { store, done })
        {
            self.message(state, error);
        }
    }

    fn handle_requests(&mut self, state: &mut AppState, tab: u64, persistent: bool) -> bool {
        let mut join = false;
        for request in std::mem::take(&mut state.runtime.collaboration.requests) {
            let result = match request {
                CollaborationRequest::Chat { text } => self.send_note(state, &text, None),
                CollaborationRequest::Ping { position } => match position {
                    Some(point) => self.send_note(state, "", Some(point)),
                    None if tab == self.bound_tab => {
                        let point = Self::ping_target(state);
                        self.send_note(state, "", Some(point))
                    }
                    None => Err(anyhow::anyhow!(hp_app::t!("team.ping_needs_shared_map"))),
                },
                CollaborationRequest::Camera { participant, mode } => self.camera(state, participant, mode),
                CollaborationRequest::Reconnect => self.retry_now(state),
                CollaborationRequest::StopReconnecting => {
                    if self.reconnect.is_some() {
                        self.stop(state);
                        self.status = tr(
                            "Disconnected. Your local map is retained.",
                            "Déconnecté. Ta map locale est conservée.",
                            "Отключено. Локальная карта сохранена.",
                        )
                        .into();
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("No reconnection in progress"))
                    }
                }
                CollaborationRequest::Leave => {
                    if self.active() {
                        self.stop(state);
                        self.status = tr(
                            "Disconnected. Your local map is retained.",
                            "Déconnecté. Ta map locale est conservée.",
                            "Отключено. Локальная карта сохранена.",
                        )
                        .into();
                    }
                    Ok(())
                }
                CollaborationRequest::SaveRecovery { path } => self.save_recovery(path),
                CollaborationRequest::Host { session, nickname, password, direct, port } => {
                    if self.active() {
                        Err(anyhow::anyhow!("A Team Mapping session is already active"))
                    } else {
                        self.session_name = session;
                        self.name = nickname;
                        self.password = Zeroizing::new(password);
                        self.connection_mode =
                            if direct { hp_collab::ConnectionMode::Direct } else { hp_collab::ConnectionMode::Relay };
                        self.port = if port == 0 { 41840 } else { port };
                        self.pending_host = Some(tab);
                        Ok(())
                    }
                }
                CollaborationRequest::Join { invitation, session, nickname, password, address } => {
                    if self.active() {
                        Err(anyhow::anyhow!("A Team Mapping session is already active"))
                    } else {
                        self.session_name = session;
                        self.name = nickname;
                        self.password = Zeroizing::new(password);
                        self.invitation_input = Zeroizing::new(invitation);
                        self.connection_mode = if address.is_some() {
                            hp_collab::ConnectionMode::Direct
                        } else {
                            hp_collab::ConnectionMode::Relay
                        };
                        self.direct_address = address.unwrap_or_default();
                        let joined = self.join_session(persistent);
                        join |= joined.is_ok();
                        joined
                    }
                }
            };
            if let Err(error) = result {
                self.message(state, format!("{error:#}"));
            }
        }
        join
    }

    fn recovery_map(&self) -> Option<&hp_collab::Snapshot> {
        self.canonical
            .as_deref()
            .or(self.recovery_base.as_ref())
            .or_else(|| self.reconnect.as_ref().and_then(|reconnect| reconnect.base.as_deref()))
    }

    fn save_recovery(&mut self, path: std::path::PathBuf) -> Result<()> {
        ensure!(!self.conflict.is_empty(), "No local copy to recover");
        ensure!(self.recovery.is_none(), "The local copy is already being saved");
        let base = self.recovery_map();
        let map = base.ok_or_else(|| anyhow::anyhow!("The shared map is not available yet"))?.clone();
        let overlay = self.conflict.iter().map(|(key, unit)| (key.clone(), unit.clone())).collect();
        self.recovery = Some(install::recovery(map, overlay, path)?);
        Ok(())
    }

    fn join_session(&mut self, persistent: bool) -> Result<()> {
        self.reconnect = None;
        self.open_session(persistent)?;
        self.conflict.clear();
        self.recovery_base = None;
        self.local_only.clear();
        self.stats = CollaborationStats::default();
        self.status = tr("Connecting...", "Connexion...", "Подключение...").into();
        Ok(())
    }

    fn open_session(&mut self, persistent: bool) -> Result<()> {
        let identity = self.identity(persistent)?;
        let invitation = Invitation::decode(&self.invitation_input)?;
        ensure!(invitation.mode == self.connection_mode, "Choose the connection mode used by this invitation");
        let direct_address = if self.connection_mode == hp_collab::ConnectionMode::Direct {
            Some(self.direct_address.trim().parse()?)
        } else {
            None
        };
        let session = Session::join(
            invitation,
            self.name.clone(),
            self.password.clone(),
            hp_collab::transport::JoinOptions { identity, session_name: self.session_name.clone(), direct_address },
        )?;
        self.credentials = Some(reconnect::Credentials {
            invitation: self.invitation_input.clone(),
            password: self.password.clone(),
            address: self.direct_address.clone(),
        });
        self.had_error = false;
        self.denied = None;
        self.last_error.clear();
        self.session = Some(session);
        self.host = false;
        self.password.clear();
        self.invitation_input.clear();
        self.joined_at = Some(Instant::now());
        Ok(())
    }

    pub(crate) fn bind_tab(&mut self, tab: u64) {
        self.bound_tab = tab;
    }

    pub(crate) fn bound_tab(&self) -> u64 {
        self.bound_tab
    }

    pub(crate) fn take_host_request(&mut self) -> Option<u64> {
        self.pending_host.take()
    }

    pub(crate) fn take_switch_request(&mut self) -> Option<u64> {
        self.pending_switch.take()
    }

    pub(crate) fn start_host(&mut self, state: &mut AppState, tab: u64, persistent: bool) {
        self.access.policy.refuse_scripts = !state.settings.workflow.team_share_scripts;
        state.runtime.collaboration.keep_scripts_local = self.access.policy.refuse_scripts;
        let options = hp_collab::transport::HostOptions {
            room: hp_collab::Room { name: self.session_name.clone() },
            mode: self.connection_mode,
            port: self.port,
            policy: self.access.policy,
        };
        let result = options.room.validate().and_then(|()| {
            ensure!(hp_collab::protocol::valid_name(&self.name), "Invalid participant name");
            ensure!((8..=256).contains(&self.password.len()), "Use at least 8 password characters");
            install::Building::new(&state.doc, self.name.clone(), self.password.clone(), options)
        });
        match result {
            Ok(building) => {
                self.had_error = false;
                self.denied = None;
                self.reconnect = None;
                self.credentials = None;
                self.building = Some(building);
                self.conflict.clear();
                self.recovery_base = None;
                self.local_only.clear();
                self.host = true;
                self.bound_tab = tab;
                self.stats = CollaborationStats::default();
                self.snapshots.start(persistent);
                self.password.clear();
                self.status = tr(
                    "Starting secure session...",
                    "Démarrage de la session sécurisée...",
                    "Запуск защищённого сеанса...",
                )
                .into();
            }
            Err(error) => self.message(state, format!("{error:#}")),
        }
    }
}
