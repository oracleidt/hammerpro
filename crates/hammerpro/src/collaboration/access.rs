use std::collections::{BTreeMap, BTreeSet};

use hp_app::i18n::tr;
use hp_app::{AppState, Severity};
use hp_collab::moderation::{AccessPolicy, AccessState, MAX_BAN_SECONDS, MAX_INVITATION_MINUTES};
use hp_collab::protocol::{Participant, Refused, Rejection, Role};
use hp_collab::{Command, ConnectionMode};
use serde_json::{Value, json};

use super::Controller;

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("team.access.heading", "Access control", "Contrôle d'accès"),
    ("team.access.security", "Session security", "Sécurité de la session"),
    ("team.access.lock", "Closed to new participants", "Fermée aux nouveaux participants"),
    ("team.access.share_scripts", "Share script and command entities", "Partager les entités de script et de commande"),
    (
        "team.access.share_scripts_hint",
        "lua_run, logic_script, point_servercommand, point_clientcommand and the Command, RunScriptCode, RunScriptFile and RunPassedCode inputs only run in the game, never in the editor. Off: the host keeps its own on this PC and refuses the ones the others add or change.",
        "lua_run, logic_script, point_servercommand, point_clientcommand et les entrées Command, RunScriptCode, RunScriptFile et RunPassedCode ne s'exécutent que dans le jeu, jamais dans l'éditeur. Désactivé : l'hôte garde les siennes sur ce PC et refuse celles que les autres ajoutent ou modifient.",
    ),
    (
        "team.access.script_notice",
        "{0} added or changed a script or command entity: {1}",
        "{0} a ajouté ou modifié une entité de script ou de commande : {1}",
    ),
    (
        "team.access.lock_hint",
        "Participants already admitted can still reconnect.",
        "Les participants déjà admis peuvent toujours se reconnecter.",
    ),
    (
        "team.access.approval",
        "Ask me before admitting a new participant",
        "Me demander avant d'admettre un nouveau participant",
    ),
    (
        "team.access.read_only_guests",
        "New participants are read only",
        "Les nouveaux participants sont en lecture seule",
    ),
    (
        "team.access.lifetime",
        "Invitation valid for new participants (minutes, 0 = whole session)",
        "Invitation valable pour les nouveaux participants (minutes, 0 = toute la session)",
    ),
    (
        "team.access.lifetime_left",
        "New participants can join for {0} more minutes",
        "Les nouveaux participants peuvent rejoindre encore {0} minutes",
    ),
    (
        "team.access.lifetime_over",
        "The invitation no longer admits new participants",
        "L'invitation n'admet plus de nouveaux participants",
    ),
    ("team.access.pending", "Waiting for your approval", "En attente de ton accord"),
    ("team.access.pending_row", "{0} is waiting ({1} s)", "{0} attend ({1} s)"),
    ("team.access.admit", "Admit", "Admettre"),
    ("team.access.decline", "Refuse", "Refuser"),
    ("team.access.ban", "Ban", "Bannir"),
    ("team.access.read_only", "Read only", "Lecture seule"),
    ("team.access.role_host", "Host", "Hôte"),
    ("team.access.role_editor", "Editor", "Éditeur"),
    ("team.access.role_viewer", "Viewer", "Lecteur"),
    ("team.access.failed_passwords", "Wrong passwords: {0}", "Mots de passe faux : {0}"),
    (
        "team.access.failed_warning",
        "Someone who has the invitation typed a wrong password. If it was not one of your guests, rotate access.",
        "Quelqu'un qui a l'invitation a tapé un mauvais mot de passe. Si ce n'était pas un de tes invités, renouvelle les accès.",
    ),
    ("team.access.lockout", "Password attempts paused for {0} s", "Essais de mot de passe suspendus pendant {0} s"),
    (
        "team.access.counters",
        "Invalid connections: {0} | Refused joins: {1} | Slowed down for flooding: {2} | Resynchronized: {3}",
        "Connexions invalides : {0} | Entrées refusées : {1} | Ralentis pour abus : {2} | Resynchronisés : {3}",
    ),
    (
        "team.access.resynced",
        "Your connection fell behind the session: the shared map was sent again and your edits were kept",
        "Ta connexion avait pris du retard sur la session : la map partagée a été renvoyée et tes modifications sont conservées",
    ),
    ("team.access.join_request", "{0} asks to join the session", "{0} demande à rejoindre la session"),
    ("team.access.waiting", "Waiting for the host to admit you...", "En attente de l'accord de l'hôte..."),
    (
        "team.access.you_are_viewer",
        "Read only: the host did not allow you to edit. Your changes are not shared.",
        "Lecture seule : l'hôte ne t'a pas autorisé à modifier. Tes modifications ne sont pas partagées.",
    ),
    ("team.access.until_end", "until the session ends", "jusqu'à la fin de la session"),
    ("team.refusal.authentication_failed", "Invitation or password rejected", "Invitation ou mot de passe refusé"),
    (
        "team.refusal.too_many_attempts",
        "Too many wrong passwords for this session. Try again in {0} seconds",
        "Trop de mauvais mots de passe pour cette session. Réessaie dans {0} secondes",
    ),
    ("team.refusal.banned", "Banned from this session ({0}): {1}", "Banni de cette session ({0}) : {1}"),
    ("team.refusal.removed", "Removed by the host", "Exclu par l'hôte"),
    ("team.refusal.full", "Session full", "Session complète"),
    (
        "team.refusal.revoked",
        "Access revoked: ask the host for the new invitation",
        "Accès révoqué : demande la nouvelle invitation à l'hôte",
    ),
    ("team.refusal.nickname_taken", "Nickname already in use", "Pseudo déjà utilisé"),
    (
        "team.refusal.locked",
        "The host closed this session to new participants",
        "L'hôte a fermé cette session aux nouveaux participants",
    ),
    ("team.refusal.declined", "The host declined your request to join", "L'hôte a refusé ta demande"),
    (
        "team.refusal.approval_timeout",
        "The host did not answer your request to join",
        "L'hôte n'a pas répondu à ta demande",
    ),
    ("team.refusal.rate_limited", "Disconnected: too many messages", "Déconnecté : trop de messages"),
    ("team.refusal.protocol", "Invalid session message", "Message de session invalide"),
    ("team.refusal.ended", "The host ended the session", "L'hôte a terminé la session"),
    (
        "team.refusal.host_outdated",
        "The host uses an older Hammer Pro (1.x). Both computers need Hammer Pro 2.0 or newer",
        "L'hôte utilise un Hammer Pro plus ancien (1.x). Les deux PC doivent avoir Hammer Pro 2.0 ou plus récent",
    ),
    (
        "team.refusal.host_newer",
        "The host uses a newer Hammer Pro. Update Hammer Pro to join",
        "L'hôte utilise un Hammer Pro plus récent. Mets à jour Hammer Pro pour rejoindre",
    ),
    (
        "team.refusal.invitation_newer",
        "This invitation needs a newer Hammer Pro. Update Hammer Pro to join",
        "Cette invitation demande un Hammer Pro plus récent. Mets à jour Hammer Pro pour rejoindre",
    ),
    (
        "team.refusal.replaced",
        "Disconnected: this installation joined the session again from another connection",
        "Déconnecté : cette installation a rejoint la session depuis une autre connexion",
    ),
    (
        "team.refusal.slow",
        "Disconnected: the connection could not keep up with the session",
        "Déconnecté : la connexion n'a pas suivi le rythme de la session",
    ),
    (
        "team.refusal.busy",
        "The host is busy with other connections from your network. Try again in a moment",
        "L'hôte est occupé par d'autres connexions de ton réseau. Réessaie dans un instant",
    ),
    ("team.edit_refusal.read_only", "You are read only in this session", "Tu es en lecture seule dans cette session"),
    ("team.edit_refusal.invalid_edit", "Invalid edit", "Modification invalide"),
    ("team.edit_refusal.invalid_key", "Invalid object key", "Clé d'objet invalide"),
    (
        "team.edit_refusal.selected",
        "Object selected by another participant",
        "Objet sélectionné par un autre participant",
    ),
    (
        "team.edit_refusal.outside_range",
        "Object identifier outside your identifier range",
        "Identifiant d'objet hors de ta plage d'identifiants",
    ),
    (
        "team.edit_refusal.side_outside_range",
        "Face identifier outside your identifier range",
        "Identifiant de face hors de ta plage d'identifiants",
    ),
    ("team.edit_refusal.too_large", "Edit too large to share", "Modification trop grande pour être partagée"),
    ("team.edit_refusal.document", "Map change refused", "Modification de la map refusée"),
];

#[derive(Default)]
pub(super) struct Access {
    pub(super) policy: AccessPolicy,
    pub(super) state: AccessState,
    pub(super) roles: BTreeMap<u32, Role>,
    announced: BTreeSet<u32>,
    warned_failures: u64,
}

impl Access {
    pub(super) fn clear(&mut self) {
        self.state = AccessState::default();
        self.roles.clear();
        self.announced.clear();
        self.warned_failures = 0;
    }

    pub(super) fn role(&self, id: u32) -> Role {
        if id == 1 { Role::Host } else { self.roles.get(&id).copied().unwrap_or_default() }
    }

    pub(super) fn role_label(&self, id: u32) -> &'static str {
        role_text(self.role(id))
    }
}

fn role_text(role: Role) -> &'static str {
    match role {
        Role::Host => tr("team.access.role_host"),
        Role::Editor => tr("team.access.role_editor"),
        Role::Viewer => tr("team.access.role_viewer"),
    }
}

pub(crate) fn refusal_text(refused: &Refused) -> String {
    refused.format(tr(&format!("team.refusal.{}", refused.refusal.id())), tr("team.access.until_end"))
}

pub(crate) fn rejection_text(rejection: &Rejection) -> String {
    let text = tr(&format!("team.edit_refusal.{}", rejection.code.id()));
    if rejection.detail.is_empty() { text.to_owned() } else { format!("{text}: {}", rejection.detail) }
}

pub(super) fn error_text(error: &anyhow::Error) -> String {
    error.downcast_ref::<Refused>().map_or_else(|| format!("{error:#}"), refusal_text)
}

#[derive(Default)]
pub(crate) struct PolicyChange {
    pub(crate) locked: Option<bool>,
    pub(crate) approval: Option<bool>,
    pub(crate) read_only_guests: Option<bool>,
    pub(crate) invitation_minutes: Option<u32>,
    pub(crate) share_scripts: Option<bool>,
}

pub(crate) enum Moderation {
    Kick(u32),
    Ban { participant: u32, minutes: u64, reason: String },
    Unban(u64),
    ReadOnly { participant: u32, read_only: bool },
    Admit(u32),
    Decline(u32),
}

fn script_control(ui: &mut egui::Ui, state: &mut AppState) {
    ui.checkbox(&mut state.settings.workflow.team_share_scripts, tr("team.access.share_scripts"))
        .on_hover_text(tr("team.access.share_scripts_hint"));
}

fn policy_controls(ui: &mut egui::Ui, policy: &mut AccessPolicy) {
    ui.checkbox(&mut policy.approval, tr("team.access.approval"));
    ui.checkbox(&mut policy.read_only_guests, tr("team.access.read_only_guests"));
    ui.checkbox(&mut policy.locked, tr("team.access.lock")).on_hover_text(tr("team.access.lock_hint"));
    ui.horizontal(|ui| {
        ui.label(tr("team.access.lifetime"));
        ui.add(egui::DragValue::new(&mut policy.invitation_minutes).range(0..=MAX_INVITATION_MINUTES));
    });
}

impl Controller {
    pub(super) fn on_access(&mut self, state: &mut AppState, access: AccessState) {
        for row in &access.pending {
            if self.access.announced.insert(row.id) {
                state.notify(Severity::Info, hp_app::tf!("team.access.join_request", row.name));
            }
        }
        if access.failed_passwords > self.access.warned_failures {
            self.access.warned_failures = access.failed_passwords;
            state.notify(Severity::Warning, tr("team.access.failed_warning"));
        }
        self.access.state = access;
    }

    pub(super) fn on_waiting(&mut self) {
        self.status = tr("team.access.waiting").into();
    }

    pub(super) fn on_denied(&mut self, state: &mut AppState, refused: &Refused) {
        self.had_error = true;
        self.denied = Some(refused.refusal);
        let text = refusal_text(refused);
        self.last_error = text.clone();
        self.message(state, text);
    }

    pub(super) fn remember_roles(&mut self, rows: &[Participant]) {
        self.access.roles = rows.iter().map(|row| (row.id, row.role)).collect();
    }

    pub(super) fn policy_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.collapsing(tr("team.access.security"), |ui| {
            policy_controls(ui, &mut self.access.policy);
            script_control(ui, state);
        });
    }

    pub(super) fn sync_script_policy(&mut self, state: &mut AppState) {
        let refuse = self.host && !state.settings.workflow.team_share_scripts;
        state.runtime.collaboration.keep_scripts_local = refuse;
        if !self.host || self.access.policy.refuse_scripts == refuse {
            return;
        }
        let mut policy = self.access.policy;
        policy.refuse_scripts = refuse;
        if let Some(session) = &self.session
            && let Err(error) = session.send(Command::Policy(policy))
        {
            self.message(state, error);
            return;
        }
        self.access.policy = policy;
        if !refuse {
            self.share_local_scripts();
        }
    }

    fn share_local_scripts(&mut self) {
        self.script_rescan = true;
        self.local_only.retain(|problem| !problem.contains("script and command"));
    }

    pub(super) fn note_scripts(&mut self, state: &mut AppState, author: u32, uses: Vec<String>) {
        if author == 0 || author == self.participant || uses.is_empty() {
            return;
        }
        let name = state
            .runtime
            .collaboration
            .participants
            .iter()
            .find(|row| row.id == author)
            .map_or_else(|| format!("#{author}"), |row| row.name.clone());
        let shown: Vec<&str> = uses.iter().take(3).map(String::as_str).collect();
        let more = if uses.len() > shown.len() { format!(" (+{})", uses.len() - shown.len()) } else { String::new() };
        let text = hp_app::tf!("team.access.script_notice", name, format!("{}{more}", shown.join(", ")));
        state.notify(Severity::Warning, &text);
        let notices = &mut self.stats.script_notices;
        notices.push(text);
        if notices.len() > 8 {
            notices.remove(0);
        }
    }

    pub(super) fn access_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        if !self.host {
            if self.access.role(self.participant) == Role::Viewer {
                ui.colored_label(hp_app::theme::warning(), tr("team.access.you_are_viewer"));
            }
            return;
        }
        ui.separator();
        ui.heading(tr("team.access.heading"));
        let before = self.access.policy;
        policy_controls(ui, &mut self.access.policy);
        script_control(ui, state);
        if self.access.policy != before
            && let Some(session) = &self.session
            && let Err(error) = session.send(Command::Policy(self.access.policy))
        {
            self.access.policy = before;
            self.message(state, error);
        }
        match self.access.state.new_devices_seconds {
            Some(0) => {
                ui.colored_label(hp_app::theme::warning(), tr("team.access.lifetime_over"));
            }
            Some(seconds) => {
                ui.small(hp_app::tf!("team.access.lifetime_left", seconds.div_ceil(60)));
            }
            None => {}
        }
        let mut requests = Vec::new();
        if !self.access.state.pending.is_empty() {
            ui.label(tr("team.access.pending"));
            for row in &self.access.state.pending {
                ui.horizontal(|ui| {
                    ui.label(hp_app::tf!("team.access.pending_row", row.name, row.waiting_seconds));
                    if ui.small_button(tr("team.access.admit")).clicked() {
                        requests.push(Moderation::Admit(row.id));
                    }
                    if ui.small_button(tr("team.access.decline")).clicked() {
                        requests.push(Moderation::Decline(row.id));
                    }
                    if ui.small_button(tr("team.access.ban")).clicked() {
                        requests.push(Moderation::Ban { participant: row.id, minutes: 0, reason: String::new() });
                    }
                });
            }
        }
        let guests: Vec<(u32, String)> = state
            .runtime
            .collaboration
            .participants
            .iter()
            .filter(|row| row.id != 1)
            .map(|row| (row.id, row.name.clone()))
            .collect();
        for (id, name) in guests {
            let mut read_only = self.access.role(id) == Role::Viewer;
            if ui.checkbox(&mut read_only, format!("{name}: {}", tr("team.access.read_only"))).changed() {
                requests.push(Moderation::ReadOnly { participant: id, read_only });
            }
        }
        let security = &self.access.state;
        if security.failed_passwords > 0 {
            ui.colored_label(
                hp_app::theme::warning(),
                hp_app::tf!("team.access.failed_passwords", security.failed_passwords),
            );
        }
        if let Some(seconds) = security.lockout_seconds {
            ui.small(hp_app::tf!("team.access.lockout", seconds));
        }
        ui.small(hp_app::tf!(
            "team.access.counters",
            security.invalid_attempts,
            security.refused_joins,
            security.rate_limited,
            security.resyncs
        ));
        for request in requests {
            if let Err(error) = self.moderate(state, request) {
                self.message(state, error);
            }
        }
    }

    pub(crate) fn team_status(&self, state: &AppState) -> Value {
        let collaboration = &state.runtime.collaboration;
        let security = &self.access.state;
        json!({
            "active": self.active(),
            "connected": collaboration.connected,
            "role": if !self.active() { "none" } else if self.host { "host" } else { self.access.role(self.participant).id() },
            "session_name": self.session_name,
            "mode": match self.connection_mode { ConnectionMode::Relay => "relay", ConnectionMode::Direct => "direct" },
            "participant": self.participant,
            "revision": self.revision,
            "status": self.status,
            "participants": collaboration.participants.iter().map(|row| json!({
                "id": row.id,
                "name": row.name,
                "role": self.access.role(row.id).id(),
                "latency_ms": row.latency_ms,
            })).collect::<Vec<_>>(),
            "policy": {
                "locked": self.access.policy.locked,
                "approval": self.access.policy.approval,
                "read_only_guests": self.access.policy.read_only_guests,
                "invitation_minutes": self.access.policy.invitation_minutes,
                "share_scripts": state.settings.workflow.team_share_scripts,
            },
            "pending": security.pending.iter().map(|row| json!({
                "id": row.id,
                "name": row.name,
                "waiting_seconds": row.waiting_seconds,
            })).collect::<Vec<_>>(),
            "bans": self.bans.iter().map(|row| json!({
                "id": row.id,
                "name": row.nickname,
                "reason": row.reason,
                "remaining_seconds": row.remaining_seconds,
            })).collect::<Vec<_>>(),
            "security": {
                "failed_passwords": security.failed_passwords,
                "invalid_attempts": security.invalid_attempts,
                "refused_joins": security.refused_joins,
                "rate_limited": security.rate_limited,
                "resyncs": security.resyncs,
                "password_lockout_seconds": security.lockout_seconds,
                "new_participants_seconds_left": security.new_devices_seconds,
                "admitted_devices": security.known_devices,
            },
        })
    }

    pub(crate) fn change_policy(&mut self, state: &mut AppState, change: PolicyChange) -> Result<String, String> {
        let mut policy = self.access.policy;
        if let Some(share) = change.share_scripts {
            policy.refuse_scripts = !share;
        }
        policy.locked = change.locked.unwrap_or(policy.locked);
        policy.approval = change.approval.unwrap_or(policy.approval);
        policy.read_only_guests = change.read_only_guests.unwrap_or(policy.read_only_guests);
        policy.invitation_minutes = change.invitation_minutes.unwrap_or(policy.invitation_minutes);
        policy.validate().map_err(|error| error.to_string())?;
        if self.active() && !self.host {
            return Err("only the host can change the access policy".into());
        }
        if let Some(session) = &self.session {
            session.send(Command::Policy(policy)).map_err(|error| error.to_string())?;
        }
        self.access.policy = policy;
        if let Some(share) = change.share_scripts {
            state.settings.workflow.team_share_scripts = share;
            if self.host && share {
                self.share_local_scripts();
            }
            state.runtime.collaboration.keep_scripts_local = self.host && !share;
        }
        Ok(if self.active() {
            "access policy applied to the session".into()
        } else {
            "access policy saved for the next hosted session".into()
        })
    }

    pub(crate) fn moderate(&mut self, state: &AppState, request: Moderation) -> Result<String, String> {
        let session = match &self.session {
            Some(session) if self.host => session,
            Some(_) => return Err("only the host can moderate the session".into()),
            None => return Err("no Team Mapping session is active".into()),
        };
        let guest = |id: u32| {
            if id != 1 && state.runtime.collaboration.participants.iter().any(|row| row.id == id) {
                Ok(id)
            } else {
                Err(format!("no guest with id {id} (see get_team_session)"))
            }
        };
        let pending = |id: u32| {
            if self.access.state.pending.iter().any(|row| row.id == id) {
                Ok(id)
            } else {
                Err(format!("nobody with id {id} is waiting for approval"))
            }
        };
        let (command, done) = match request {
            Moderation::Kick(id) => (Command::Kick(guest(id).or_else(|_| pending(id))?), "participant removed"),
            Moderation::Ban { participant, minutes, reason } => {
                let participant = guest(participant).or_else(|_| pending(participant))?;
                let seconds = (minutes > 0).then(|| minutes.saturating_mul(60));
                if seconds.is_some_and(|seconds| seconds > MAX_BAN_SECONDS) {
                    return Err(format!("minutes must be 0 to {}", MAX_BAN_SECONDS / 60));
                }
                (Command::Ban { participant, seconds, reason }, "participant banned")
            }
            Moderation::Unban(id) => {
                if !self.bans.iter().any(|row| row.id == id) {
                    return Err(format!("no ban with id {id}"));
                }
                (Command::Unban(id), "ban removed")
            }
            Moderation::ReadOnly { participant, read_only } => (
                Command::SetRole {
                    participant: guest(participant)?,
                    role: if read_only { Role::Viewer } else { Role::Editor },
                },
                if read_only { "participant is now read only" } else { "participant can edit again" },
            ),
            Moderation::Admit(id) => (Command::Admit(pending(id)?), "participant admitted"),
            Moderation::Decline(id) => (Command::Decline(pending(id)?), "join request refused"),
        };
        session.send(command).map_err(|error| error.to_string())?;
        Ok(done.into())
    }
}

#[cfg(test)]
mod tests {
    use hp_collab::protocol::{EditRefusal, Refusal, Refused, Rejection};

    #[test]
    fn every_refusal_has_a_clean_translated_text() {
        assert!(hp_app::i18n::lint_table(super::TR).is_empty());
        hp_app::i18n::register(super::TR);
        for refusal in Refusal::ALL {
            let key = format!("team.refusal.{}", refusal.id());
            assert!(super::TR.iter().any(|(entry, _, _)| *entry == key), "{key}");
            let refused = Refused::with(refusal, "griefing", Some(42));
            let text = super::refusal_text(&refused);
            assert!(!text.contains('{') && !text.is_empty(), "{text}");
        }
        let banned = super::refusal_text(&Refused::with(Refusal::Banned, "griefing", None));
        assert!(banned.contains("griefing") && banned.contains("until the session ends"), "{banned}");
        let error = anyhow::Error::new(Refused::new(Refusal::HostOutdated));
        assert!(super::error_text(&error).contains("1.x"));
        for code in EditRefusal::ALL {
            let key = format!("team.edit_refusal.{}", code.id());
            assert!(super::TR.iter().any(|(entry, _, _)| *entry == key), "{key}");
            let text = super::rejection_text(&Rejection::new(code, "solid:12 (Alice)"));
            assert!(text.ends_with(": solid:12 (Alice)") && !text.starts_with(':'), "{text}");
            assert!(!super::rejection_text(&Rejection::new(code, "")).contains(':'));
        }
    }
}
