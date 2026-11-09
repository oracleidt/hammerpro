use std::collections::VecDeque;
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use hp_app::{Action, AppState, Severity};
use hp_collab::{Change, Command, Event, Invitation, Presence, Session, Snapshot};
use zeroize::Zeroizing;
mod snapshots;

#[cfg(test)]
mod tests;

pub(crate) fn tr(en: &'static str, fr: &'static str, ru: &'static str) -> &'static str {
    match hp_app::i18n::lang() {
        hp_app::i18n::Lang::Fr => fr,
        hp_app::i18n::Lang::Ru => ru,
        _ => en,
    }
}

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

struct Submitted {
    sequence: u64,
    before: Snapshot,
    after: Snapshot,
    history: u8,
}

struct PersonalEdit {
    undo: Vec<Change>,
    redo: Vec<Change>,
    bytes: usize,
}

#[derive(Default)]
pub(crate) struct Controller {
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
    canonical: Option<Snapshot>,
    displayed: Option<Snapshot>,
    submitted: Option<Submitted>,
    undo: VecDeque<PersonalEdit>,
    redo: Vec<PersonalEdit>,
    deferred: VecDeque<Event>,
    held_baseline: Option<Snapshot>,
    replace_after_transaction: bool,
    conflict: Option<Snapshot>,
    last_tick: Option<Instant>,
    last_presence: Option<Presence>,
    last_content_serial: u64,
    sent: u64,
    received: u64,
}

impl Controller {
    pub(crate) fn active(&self) -> bool {
        self.session.is_some()
    }

    pub(crate) fn stop(&mut self, state: &mut AppState) {
        self.session = None;
        self.canonical = None;
        self.displayed = None;
        self.submitted = None;
        self.deferred.clear();
        self.held_baseline = None;
        self.replace_after_transaction = false;
        self.undo.clear();
        self.redo.clear();
        self.bans.clear();
        self.last_presence = None;
        self.pending_look = None;
        self.pending_teleport = None;
        self.pending_switch = None;
        self.snapshots = snapshots::Tasks::default();
        self.invitation.clear();
        self.participant = 0;
        state.runtime.collaboration.connected = false;
        state.runtime.collaboration.active_map = false;
        state.runtime.collaboration.can_undo = false;
        state.runtime.collaboration.can_redo = false;
        state.runtime.collaboration.participants.clear();
        state.doc.mark_everything_changed();
    }

    fn message(&mut self, state: &mut AppState, text: impl ToString) {
        self.status = text.to_string();
        state.notify(Severity::Warning, &self.status);
    }

    fn current(state: &AppState) -> Result<Snapshot> {
        Snapshot::from_vmf(&state.doc.to_vmf_string())
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

    fn install(&mut self, state: &mut AppState, map: Snapshot) -> Result<()> {
        let parsed = hp_vmf::parse(&map.to_vmf()?)?;
        state.doc.apply_shared_map(parsed, self.id_start, self.id_start + hp_collab::document::ID_STRIDE)?;
        self.last_content_serial = state.doc.content_serial();
        self.displayed = Some(map);
        Ok(())
    }

    fn rebase(&mut self, state: &mut AppState, previous: &Snapshot, current: Snapshot) -> Result<()> {
        let local = Self::current(state)?;
        let mut merged = current.clone();
        let mut conflicted = false;
        for change in previous.diff(&local) {
            if current.0.get(&change.key) == change.after.as_ref() {
                continue;
            }
            if current.0.get(&change.key).map(hp_collab::document::digest) != change.before {
                conflicted = true;
                continue;
            }
            if let Some(after) = change.after {
                merged.0.insert(change.key, after);
            } else {
                merged.0.remove(&change.key);
            }
        }
        if conflicted || merged.validate().is_err() {
            self.conflict = Some(local);
            self.message(
                state,
                tr(
                    "Conflict: your local version is kept in Recover local copy.",
                    "Conflit : ta version locale est conservée dans Récupérer la copie locale.",
                    "Конфликт: локальная версия сохранена в разделе восстановления.",
                ),
            );
            merged = current;
        }
        self.install(state, merged)?;
        if self.displayed != self.canonical {
            self.last_content_serial = 0;
        }
        Ok(())
    }

    fn finish_transaction(&mut self, state: &mut AppState) -> Result<()> {
        if state.doc.in_transaction() {
            return Ok(());
        }
        if self.replace_after_transaction {
            self.replace_after_transaction = false;
            self.held_baseline = None;
            self.conflict = Self::current(state).ok();
            self.install(state, self.canonical.clone().unwrap_or_default())
        } else if let Some(previous) = self.held_baseline.take() {
            self.rebase(state, &previous, self.canonical.clone().unwrap_or_default())
        } else {
            Ok(())
        }
    }

    fn process(&mut self, state: &mut AppState, event: Event) -> Result<()> {
        match event {
            Event::Ready { invitation, participant, id_start, revision, map, direct_addresses } => {
                self.host_addresses = direct_addresses;
                self.participant = participant;
                self.id_start = id_start;
                self.revision = revision;
                self.invitation = Zeroizing::new(invitation.unwrap_or_default());
                if !self.host {
                    state.doc.set_path(None);
                }
                self.install(state, map.clone())?;
                self.canonical = Some(map);
                state.runtime.collaboration.connected = true;
                state.runtime.collaboration.local_id = participant;
                self.status = tr("Connected", "Connecté", "Подключено").into();
            }
            Event::Applied { author, sequence, revision, changes } => {
                let mut previous = self.displayed.clone().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
                let canonical = self.canonical.as_mut().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
                canonical.apply(&changes)?;
                self.revision = revision;
                if author == self.participant {
                    let submitted =
                        self.submitted.take().ok_or_else(|| anyhow::anyhow!("Unexpected session acknowledgement"))?;
                    ensure!(submitted.sequence == sequence, "Session acknowledgement mismatch");
                    previous = submitted.after.clone();
                    if submitted.history == 0 {
                        let undo = submitted.after.diff(&submitted.before);
                        let redo = submitted.before.diff(&submitted.after);
                        let bytes = undo
                            .iter()
                            .chain(&redo)
                            .map(|c| c.after.as_ref().map_or(0, |unit| unit.vmf.len()) + 96)
                            .sum();
                        self.undo.push_back(PersonalEdit { undo, redo, bytes });
                        self.redo.clear();
                        while self.undo.len() > 128
                            || self.undo.iter().map(|edit| edit.bytes).sum::<usize>() > 64 * 1024 * 1024
                        {
                            self.undo.pop_front();
                        }
                    }
                }
                let current = self.canonical.clone().unwrap_or_default();
                if state.doc.in_transaction() {
                    if self.held_baseline.is_none() || author == self.participant {
                        self.held_baseline = Some(previous);
                    }
                } else {
                    self.rebase(state, &previous, current)?;
                }
            }
            Event::Rejected { sequence, reason, revision, map } => {
                if self.submitted.as_ref().is_some_and(|pending| pending.sequence == sequence) {
                    if self.conflict.is_none() {
                        self.conflict = Some(Self::current(state)?);
                    }
                    self.submitted = None;
                }
                self.revision = revision;
                if state.doc.in_transaction() {
                    self.replace_after_transaction = true;
                } else {
                    self.install(state, map.clone())?;
                }
                self.canonical = Some(map);
                self.message(
                    state,
                    format!(
                        "{}: {reason}",
                        tr(
                            "Edit rejected; local copy retained",
                            "Modification refusée, copie locale conservée",
                            "Правка отклонена, локальная копия сохранена"
                        )
                    ),
                );
            }
            Event::Participants(rows) => {
                if self.host {
                    let previous: std::collections::BTreeSet<u32> =
                        state.runtime.collaboration.participants.iter().map(|peer| peer.id).collect();
                    for row in &rows {
                        if row.id != 1 && !previous.contains(&row.id) {
                            state.notify(
                                Severity::Info,
                                format!(
                                    "{} {}",
                                    row.name,
                                    tr("joined the session", "a rejoint la session", "присоединился к сеансу")
                                ),
                            );
                        }
                    }
                }
                state.runtime.collaboration.participants = rows
                    .into_iter()
                    .map(|row| hp_app::runtime::collaboration::Collaborator {
                        id: row.id,
                        name: row.name,
                        position: row.presence.position,
                        direction: row.presence.direction,
                        selected: row.presence.selected,
                        latency_ms: row.latency_ms,
                    })
                    .collect();
            }
            Event::Bans(rows) => {
                self.bans = rows;
            }
            Event::Reset { revision, map } => {
                if self.submitted.is_some() && self.conflict.is_none() {
                    self.conflict = Some(Self::current(state)?);
                }
                self.submitted = None;
                self.undo.clear();
                self.redo.clear();
                self.revision = revision;
                self.canonical = Some(map.clone());
                if state.doc.in_transaction() {
                    self.replace_after_transaction = true;
                } else {
                    self.install(state, map)?;
                }
                if self.host {
                    self.snapshots.refresh();
                }
                state.notify(
                    Severity::Info,
                    tr(
                        "The host restored a session snapshot",
                        "L'hôte a restauré un snapshot de la session",
                        "Хост восстановил снимок сеанса",
                    ),
                );
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
                self.message(state, error);
            }
            Event::Closed => {
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
        Ok(())
    }

    pub(crate) fn tick(&mut self, state: &mut AppState, ctx: &egui::Context, tab: u64) {
        if !self.active() {
            return;
        }
        state.runtime.collaboration.can_undo = !self.undo.is_empty() && self.submitted.is_none();
        state.runtime.collaboration.can_redo = !self.redo.is_empty() && self.submitted.is_none();
        debug_assert_eq!(self.bound_tab, tab);
        if let Some(position) = self.pending_look.take() {
            state.cameras.cam3d.look_at(glam::DVec3::from_array(position));
        }
        if let Some((position, direction)) = self.pending_teleport.take() {
            teleport_camera(state, position, direction);
        }
        if let Err(error) = self.finish_transaction(state) {
            self.stop(state);
            self.message(state, error);
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(50));
        if let Some(session) = &self.session {
            for event in session.events.try_iter().take(256) {
                self.deferred.push_back(event);
            }
        }
        let mut ready = None;
        while let Some(event) = self.deferred.pop_front() {
            if state.doc.in_transaction() && matches!(event, Event::Ready { .. }) {
                ready = Some(event);
            } else {
                if let Err(error) = self.process(state, event) {
                    self.stop(state);
                    self.message(state, error);
                    return;
                }
            }
        }
        if let Some(event) = ready {
            self.deferred.push_back(event);
        }
        if self.host
            && let Some(map) = self.snapshots.tick(self.canonical.as_ref(), &self.session_name, self.revision)
            && let (Some(session), Some(store)) = (&self.session, self.snapshots.store())
            && let Err(error) = session.send(Command::Restore { map, store })
        {
            self.message(state, error);
        }
        if self.last_tick.is_some_and(|last| last.elapsed() < Duration::from_millis(100)) || self.canonical.is_none() {
            return;
        }
        self.last_tick = Some(Instant::now());
        let selected = state
            .doc
            .selection()
            .objects
            .iter()
            .map(|object| match object {
                hp_editor::ObjRef::Solid(id) => format!("solid:{id}"),
                hp_editor::ObjRef::Entity(id) => format!("entity:{id}"),
            })
            .chain(state.doc.selection().faces.iter().map(|face| format!("solid:{}", face.solid)))
            .take(2048)
            .collect();
        let presence = Presence {
            position: state.cameras.cam3d.pos.to_array(),
            direction: state.cameras.cam3d.forward().to_array(),
            selected,
        };
        if self.last_presence.as_ref() != Some(&presence)
            && self.session.as_ref().is_some_and(|session| session.send(Command::Presence(presence.clone())).is_ok())
        {
            self.last_presence = Some(presence);
        }
        if state.doc.in_transaction()
            || self.submitted.is_some()
            || self.last_content_serial == state.doc.content_serial()
        {
            return;
        }
        self.last_content_serial = state.doc.content_serial();
        match Self::current(state) {
            Ok(after) => {
                let before = self.canonical.as_ref().cloned().unwrap_or_default();
                let changes = before.diff(&after);
                if changes.is_empty() {
                    return;
                }
                if let Err(error) = self.submit(before, after, changes, 0) {
                    self.message(state, error);
                }
            }
            Err(error) => self.message(state, error),
        }
    }

    fn submit(&mut self, before: Snapshot, after: Snapshot, changes: Vec<Change>, history: u8) -> Result<()> {
        self.sequence += 1;
        self.session
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Session disconnected"))?
            .send(Command::Edit { sequence: self.sequence, changes })?;
        self.submitted = Some(Submitted { sequence: self.sequence, before, after, history });
        Ok(())
    }

    pub(crate) fn history(&mut self, state: &mut AppState, action: Action) -> bool {
        if !self.active() || !matches!(action, Action::Undo | Action::Redo) {
            return false;
        }
        let result = (|| -> Result<()> {
            ensure!(
                self.submitted.is_none() && !state.doc.in_transaction(),
                "Wait for the current edit to synchronize"
            );
            let current = Self::current(state)?;
            let before = self.canonical.clone().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
            ensure!(before == current, "Wait for the current edit to synchronize");
            let edit = if action == Action::Undo { self.undo.pop_back() } else { self.redo.pop() };
            let Some(edit) = edit else {
                return Ok(());
            };
            let changes = if action == Action::Undo { edit.undo.clone() } else { edit.redo.clone() };
            let mut after = before.clone();
            if let Err(error) = after.apply(&changes) {
                if action == Action::Undo {
                    self.undo.push_back(edit);
                } else {
                    self.redo.push(edit);
                }
                return Err(error);
            }
            self.submit(before, after.clone(), changes, 1)?;
            self.install(state, after)?;
            if action == Action::Undo {
                self.redo.push(edit);
            } else {
                self.undo.push_back(edit);
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.message(state, error);
        }
        true
    }

    pub(crate) fn ui(&mut self, state: &mut AppState, ctx: &egui::Context, tab: u64, persistent: bool) -> bool {
        if !state.runtime.collaboration.show {
            return false;
        }
        let mut opened = true;
        let mut join = false;
        egui::Window::new("Team Mapping")
            .id(egui::Id::unique("collaboration"))
            .open(&mut opened)
            .default_size(egui::vec2(620.0, 640.0))
            .default_pos(ctx.content_rect().center() - egui::vec2(310.0, 320.0))
            .max_height((ctx.content_rect().height() - 80.0).clamp(200.0, 740.0))
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label(tr("Only the map is shared. Compilers, outputs and game launch stay on each PC.", "Seule la map est partagée. Compilation, fichiers produits et jeu restent sur chaque PC.", "Общая только карта. Компиляция, результаты и запуск игры остаются на каждом ПК."));
                ui.separator();
                if !self.active() {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.joining, false, tr("Host", "Héberger", "Создать"));
                        ui.selectable_value(&mut self.joining, true, tr("Join", "Rejoindre", "Подключиться"));
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.connection_mode, hp_collab::ConnectionMode::Relay, tr("Relay (recommended)", "Relais (recommandé)", "Ретрансляция (рекомендуется)"));
                        ui.selectable_value(&mut self.connection_mode, hp_collab::ConnectionMode::Direct, tr("Direct connection", "Connexion directe", "Прямое соединение"));
                    });
                    if self.connection_mode == hp_collab::ConnectionMode::Relay {
                        ui.label(tr("No network setup. Participant IPs stay private. A relay may add latency.", "Aucune configuration réseau. Les IP restent privées entre participants. Le relais peut ajouter de la latence.", "Без настройки сети. IP скрыты от участников. Ретранслятор может увеличить задержку."));
                    } else {
                        ui.colored_label(hp_app::theme::warning(), tr("May reduce latency. Host IP is required and visible to guests. UDP port forwarding may be needed.", "Peut réduire la latence. IP de l'hôte requise et visible aux invités. Une ouverture de port UDP peut être nécessaire.", "Может уменьшить задержку. Нужен IP хоста, видимый гостям. Может потребоваться открытие UDP-порта."));
                        if self.joining {
                            ui.label(tr("Host IP:port", "IP:port de l'hôte", "IP:порт хоста"));
                            ui.text_edit_singleline(&mut self.direct_address);
                        } else {
                            ui.label(tr("UDP listen port", "Port UDP d'écoute", "Порт UDP"));
                            if self.port == 0 { self.port = 41840; }
                            ui.add(egui::DragValue::new(&mut self.port).range(1024..=65535));
                        }
                    }
                    ui.label(tr("Session name (required)", "Nom de session (obligatoire)", "Название сеанса (обязательно)"));
                    ui.add(egui::TextEdit::singleline(&mut self.session_name).char_limit(48));
                    ui.label(tr("Nickname: 1 to 12 ASCII letters, digits, _ or -", "Pseudo : 1 à 12 lettres ASCII, chiffres, _ ou -", "Имя: от 1 до 12 латинских букв, цифр, _ или -"));
                    ui.add(egui::TextEdit::singleline(&mut self.name).char_limit(12));
                    self.name.retain(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
                    ui.label(tr("Password (required, 8 characters minimum)", "Mot de passe (obligatoire, 8 caractères minimum)", "Пароль (обязательно, минимум 8 символов)"));
                    ui.add(egui::TextEdit::singleline(&mut *self.password).password(!self.show_password).char_limit(256));
                    ui.checkbox(&mut self.show_password, tr("Show password", "Afficher le mot de passe", "Показать пароль"));
                    let valid = hp_collab::protocol::valid_name(&self.name) && self.password.len() >= 8 && !self.session_name.trim().is_empty();
                    if !self.joining {
                        let maps = &state.runtime.workflow.tabs;
                        if !maps.iter().any(|map| map.id == self.selected_tab) { self.selected_tab = tab; }
                        ui.label(tr("Map to share", "Map à partager", "Карта для совместной работы"));
                        let title = maps.iter().find(|map| map.id == self.selected_tab).map_or("Map", |map| map.title.as_str());
                        egui::ComboBox::from_id_salt("shared-map").selected_text(title).show_ui(ui, |ui| {
                            for map in maps { ui.selectable_value(&mut self.selected_tab, map.id, &map.title); }
                        });
                        ui.small(tr("Other open maps stay local. You can switch tabs without leaving the session.", "Les autres maps restent locales. Tu peux changer d'onglet sans quitter la session.", "Другие карты остаются локальными. Переключение вкладок не отключает сеанс."));
                        if ui.add_enabled(valid, egui::Button::new(tr("Host selected map", "Héberger la map sélectionnée", "Создать сеанс с выбранной картой"))).clicked() {
                            self.pending_host = Some(self.selected_tab);
                        }
                    }
                    if self.joining {
                        ui.separator();
                        ui.label(tr("Invitation code", "Code d'invitation", "Код приглашения"));
                        ui.add(egui::TextEdit::singleline(&mut *self.invitation_input).password(true).char_limit(2048));
                    }
                    if self.joining && ui.add_enabled(valid, egui::Button::new(tr("Join in a new local tab", "Rejoindre dans un nouvel onglet local", "Присоединиться в новой локальной вкладке"))).clicked() {
                        let result = self.identity(persistent).and_then(|identity| Invitation::decode(&self.invitation_input).and_then(|invitation| {
                            ensure!(invitation.mode == self.connection_mode, "Choose the connection mode used by this invitation");
                            let direct_address = if self.connection_mode == hp_collab::ConnectionMode::Direct { Some(self.direct_address.parse()?) } else { None };
                            Session::join(invitation, self.name.clone(), self.password.clone(), hp_collab::transport::JoinOptions { identity, session_name: self.session_name.clone(), direct_address })
                        }));
                        match result {
                            Ok(session) => { self.had_error = false; self.session = Some(session); self.host = false; self.password.clear(); self.invitation_input.clear(); join = true; self.status = tr("Connecting...", "Connexion...", "Подключение...").into(); }
                            Err(error) => self.message(state, error),
                        }
                    }
                    ui.small(tr("Hammer Pro is free and open source. Public third-party relays avoid recurring hosting costs for the project. Service availability depends on their operator.", "Hammer Pro est gratuit et open source. Les relais publics tiers évitent des frais d'hébergement récurrents pour le projet. Leur disponibilité dépend de leur opérateur.", "Hammer Pro бесплатен и открыт. Публичные сторонние ретрансляторы позволяют избежать регулярных расходов проекта на серверы. Их доступность зависит от оператора."));
                } else {
                    ui.heading(&self.session_name);
                    if tab != self.bound_tab && ui.button(tr("Return to shared map", "Revenir à la map partagée", "Открыть общую карту")).clicked() { self.pending_switch = Some(self.bound_tab); }
                    ui.label(if self.connection_mode == hp_collab::ConnectionMode::Relay { tr("Encrypted relay", "Relais chiffré", "Зашифрованная ретрансляция") } else { tr("Direct encrypted connection", "Connexion directe chiffrée", "Прямое зашифрованное соединение") });
                    if self.host && self.connection_mode == hp_collab::ConnectionMode::Direct {
                        ui.collapsing(tr("Local connection addresses", "Adresses locales de connexion", "Локальные адреса подключения"), |ui| { for address in &self.host_addresses { ui.label(address.to_string()); } });
                    }
                    ui.label(format!("{} {} | {} {}", tr("Revision", "Révision", "Версия"), self.revision, tr("Pending", "En attente", "Ожидает"), usize::from(self.submitted.is_some())));
                    ui.label(format!("{} {:.1} KiB | {} {:.1} KiB", tr("Sent", "Envoyé", "Отправлено"), self.sent as f64 / 1024.0, tr("Received", "Reçu", "Получено"), self.received as f64 / 1024.0));
                    if self.host {
                        if ui.add_enabled(!self.invitation.is_empty(), egui::Button::new(tr("Copy invitation", "Copier l'invitation", "Копировать приглашение"))).clicked() { ctx.copy_text(self.invitation.to_string()); }
                        ui.label(tr("New password", "Nouveau mot de passe", "Новый пароль"));
                        ui.add(egui::TextEdit::singleline(&mut *self.password).password(true).char_limit(256));
                        if ui.add_enabled(self.password.len() >= 8, egui::Button::new(tr("Rotate access and disconnect guests", "Renouveler les accès et déconnecter les invités", "Обновить доступ и отключить гостей"))).clicked()
                            && let Err(error) = self.session.as_ref().unwrap().send(Command::Rotate(self.password.clone())) { self.message(state, error); }
                    }
                    ui.separator();
                    ui.heading(tr("Participants", "Participants", "Участники"));
                    let rows = state.runtime.collaboration.participants.clone();
                    for row in rows {
                        ui.horizontal_wrapped(|ui| {
                            let role = if row.id == 1 { tr("Host", "Hôte", "Хост") } else { tr("Guest", "Invité", "Гость") };
                            ui.label(format!("{} | {} | {} ms", row.name, role, row.latency_ms));
                            if row.id != self.participant && ui.small_button(tr("Locate", "Repérer", "Найти")).clicked() { self.pending_switch = Some(self.bound_tab); self.pending_look = Some(row.position); }
                            if row.id != self.participant && ui.small_button(tr("Teleport", "Téléporter", "Перейти")).clicked() { self.pending_switch = Some(self.bound_tab); self.pending_teleport = Some((row.position, row.direction)); }
                            if self.host && row.id != 1 && ui.small_button(tr("Remove", "Exclure", "Исключить")).clicked() { let _ = self.session.as_ref().unwrap().send(Command::Kick(row.id)); }
                            if self.host && row.id != 1 && ui.small_button(tr("Ban", "Bannir", "Заблокировать")).clicked() { self.ban_target = Some(row.id); }
                        });
                    }
                    if self.host {
                        if let Some(target) = self.ban_target {
                            ui.separator();
                            ui.label(tr("Ban duration in minutes (0 = until session ends)", "Durée en minutes (0 = jusqu'à la fin de session)", "Срок в минутах (0 = до конца сеанса)"));
                            ui.add(egui::DragValue::new(&mut self.ban_minutes).range(0..=10080));
                            ui.label(tr("Reason", "Motif", "Причина"));
                            ui.add(egui::TextEdit::singleline(&mut self.ban_reason).char_limit(256));
                            if ui.button(tr("Confirm ban", "Confirmer le bannissement", "Подтвердить блокировку")).clicked() {
                                let seconds = (self.ban_minutes > 0).then_some(self.ban_minutes * 60);
                                if let Err(error) = self.session.as_ref().unwrap().send(Command::Ban { participant: target, seconds, reason: self.ban_reason.clone() }) { self.message(state, error); }
                                self.ban_target = None;
                            }
                        }
                        ui.collapsing(tr("Session ban list", "Bannissements de la session", "Блокировки сеанса"), |ui| {
                            for row in self.bans.clone() {
                                ui.horizontal(|ui| {
                                    let remaining = row.remaining_seconds.map_or_else(|| tr("Until session ends", "Fin de session", "До конца сеанса").into(), |seconds| format!("{seconds} s"));
                                    ui.label(format!("{} | {} | {}", row.nickname, remaining, row.reason));
                                    if ui.small_button(tr("Unban", "Débannir", "Разблокировать")).clicked() { let _ = self.session.as_ref().unwrap().send(Command::Unban(row.id)); }
                                });
                            }
                        });
                    }
                    if self.host { self.snapshots.ui(ui, self.canonical.as_ref(), &self.session_name, self.revision); }
                    if ui.button(tr("Leave session", "Quitter la session", "Покинуть сеанс")).clicked() { self.stop(state); }
                }
                if self.conflict.is_some() && ui.button(tr("Recover local copy (Save As)", "Récupérer la copie locale (Enregistrer sous)", "Восстановить локальную копию (Сохранить как)")).clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("VMF", &["vmf"]).set_file_name("collaboration-local-recovery.vmf").save_file()
                        && let Some(map) = &self.conflict {
                            match map.to_vmf().and_then(|text| { std::fs::write(path, text)?; Ok(()) }) { Ok(()) => self.conflict = None, Err(error) => self.message(state, error) }
                        }
                if !self.status.is_empty() { ui.separator(); ui.label(&self.status); }
                let language = match hp_app::i18n::lang() { hp_app::i18n::Lang::Fr => "fr", hp_app::i18n::Lang::Ru => "ru", _ => "en" };
                ui.hyperlink_to(tr("Team Mapping documentation and limitations", "Documentation et limites de Team Mapping", "Документация и ограничения Team Mapping"), format!("https://github.com/oracleidt/hammerpro/blob/master/docs/{language}/features/team-mapping.md"));
            });
        state.runtime.collaboration.show = opened;
        join
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
        let result = Self::current(state).and_then(|map| {
            Session::host(
                map,
                self.name.clone(),
                self.password.clone(),
                hp_collab::transport::HostOptions {
                    room: hp_collab::Room { name: self.session_name.clone() },
                    mode: self.connection_mode,
                    port: self.port,
                },
            )
        });
        match result {
            Ok(session) => {
                self.had_error = false;
                self.session = Some(session);
                self.host = true;
                self.bound_tab = tab;
                self.snapshots.start(persistent);
                self.password.clear();
                self.status = tr(
                    "Starting secure session...",
                    "Démarrage de la session sécurisée...",
                    "Запуск защищённого сеанса...",
                )
                .into();
            }
            Err(error) => self.message(state, error),
        }
    }
}
