use std::time::{Duration, Instant};

use hp_app::runtime::collaboration::{CameraMode, Collaborator};
use hp_app::{AppState, t, tf};
use hp_collab::Command;

use super::{Controller, size_text, tr};

fn repaint_soon(ui: &egui::Ui) {
    ui.ctx().request_repaint_after(Duration::from_millis(100));
}

fn swatch(ui: &mut egui::Ui, color: [u8; 3]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, hp_app::theme::rgb(color));
}

fn rate_text(bytes: u32) -> String {
    size_text(u64::from(bytes))
}

impl Controller {
    pub(crate) fn ui(&mut self, state: &mut AppState, ctx: &egui::Context, tab: u64, persistent: bool) -> bool {
        if tab == self.bound_tab && self.session.is_some() && self.canonical.is_some() {
            if let Err(error) = self.capture(state) {
                self.stop(state);
                self.message(state, format!("{error:#}"));
            }
            self.publish(state);
        }
        let requested = self.handle_requests(state, tab, persistent);
        if !state.runtime.collaboration.show {
            return requested;
        }
        state.runtime.collaboration.unread = 0;
        let mut opened = true;
        let mut join = requested;
        egui::Window::new("Team Mapping")
            .id(egui::Id::unique("collaboration"))
            .open(&mut opened)
            .default_size(egui::vec2(620.0, 640.0))
            .default_pos(ctx.content_rect().center() - egui::vec2(310.0, 320.0))
            .max_height((ctx.content_rect().height() - 80.0).clamp(200.0, 740.0))
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label(tr(
                    "Only the map is shared. Compilers, outputs and game launch stay on each PC.",
                    "Seule la map est partagée. Compilation, fichiers produits et jeu restent sur chaque PC.",
                    "Общая только карта. Компиляция, результаты и запуск игры остаются на каждом ПК.",
                ));
                ui.separator();
                if !self.active() {
                    self.setup_ui(ui, state, tab, persistent, &mut join);
                } else if self.reconnect.is_some() {
                    self.reconnect_ui(ui, state, tab);
                } else {
                    self.session_ui(ui, state, ctx, tab);
                }
                self.recovery_ui(ui, state);
                if !self.status.is_empty() {
                    ui.separator();
                    ui.label(&self.status);
                }
                let language = match hp_app::i18n::lang() {
                    hp_app::i18n::Lang::Fr => "fr",
                    hp_app::i18n::Lang::Ru => "ru",
                    _ => "en",
                };
                ui.hyperlink_to(
                    tr(
                        "Team Mapping documentation and limitations",
                        "Documentation et limites de Team Mapping",
                        "Документация и ограничения Team Mapping",
                    ),
                    format!(
                        "https://github.com/oracleidt/hammerpro/blob/master/docs/{language}/features/team-mapping.md"
                    ),
                );
            });
        state.runtime.collaboration.show = opened;
        join
    }

    fn setup_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, tab: u64, persistent: bool, join: &mut bool) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.joining, false, tr("Host", "Héberger", "Создать"));
            ui.selectable_value(&mut self.joining, true, tr("Join", "Rejoindre", "Подключиться"));
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.connection_mode,
                hp_collab::ConnectionMode::Relay,
                tr("Relay (recommended)", "Relais (recommandé)", "Ретрансляция (рекомендуется)"),
            );
            ui.selectable_value(
                &mut self.connection_mode,
                hp_collab::ConnectionMode::Direct,
                tr("Direct connection", "Connexion directe", "Прямое соединение"),
            );
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
                if self.port == 0 {
                    self.port = 41840;
                }
                ui.add(egui::DragValue::new(&mut self.port).range(1024..=65535));
            }
        }
        ui.label(tr("Session name (required)", "Nom de session (obligatoire)", "Название сеанса (обязательно)"));
        ui.add(egui::TextEdit::singleline(&mut self.session_name).char_limit(48));
        ui.label(tr(
            "Nickname: 1 to 12 ASCII letters, digits, _ or -",
            "Pseudo : 1 à 12 lettres ASCII, chiffres, _ ou -",
            "Имя: от 1 до 12 латинских букв, цифр, _ или -",
        ));
        ui.add(egui::TextEdit::singleline(&mut self.name).char_limit(12));
        self.name.retain(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
        ui.label(tr(
            "Password (required, 8 characters minimum)",
            "Mot de passe (obligatoire, 8 caractères minimum)",
            "Пароль (обязательно, минимум 8 символов)",
        ));
        ui.add(egui::TextEdit::singleline(&mut *self.password).password(!self.show_password).char_limit(256));
        ui.checkbox(&mut self.show_password, tr("Show password", "Afficher le mot de passe", "Показать пароль"));
        let valid = hp_collab::protocol::valid_name(&self.name)
            && self.password.len() >= 8
            && !self.session_name.trim().is_empty();
        if !self.joining {
            let maps = &state.runtime.workflow.tabs;
            if !maps.iter().any(|map| map.id == self.selected_tab) {
                self.selected_tab = tab;
            }
            ui.label(tr("Map to share", "Map à partager", "Карта для совместной работы"));
            let title = maps.iter().find(|map| map.id == self.selected_tab).map_or("Map", |map| map.title.as_str());
            egui::ComboBox::from_id_salt("shared-map").selected_text(title).show_ui(ui, |ui| {
                for map in maps {
                    ui.selectable_value(&mut self.selected_tab, map.id, &map.title);
                }
            });
            ui.small(tr(
                "Other open maps stay local. You can switch tabs without leaving the session.",
                "Les autres maps restent locales. Tu peux changer d'onglet sans quitter la session.",
                "Другие карты остаются локальными. Переключение вкладок не отключает сеанс.",
            ));
            self.policy_ui(ui, state);
            if ui
                .add_enabled(
                    valid,
                    egui::Button::new(tr(
                        "Host selected map",
                        "Héberger la map sélectionnée",
                        "Создать сеанс с выбранной картой",
                    )),
                )
                .clicked()
            {
                self.pending_host = Some(self.selected_tab);
            }
        } else {
            ui.separator();
            ui.label(tr("Invitation code", "Code d'invitation", "Код приглашения"));
            ui.add(egui::TextEdit::singleline(&mut *self.invitation_input).password(true).char_limit(2048));
            if ui
                .add_enabled(
                    valid,
                    egui::Button::new(tr(
                        "Join in a new local tab",
                        "Rejoindre dans un nouvel onglet local",
                        "Присоединиться в новой локальной вкладке",
                    )),
                )
                .clicked()
            {
                match self.join_session(persistent) {
                    Ok(()) => *join = true,
                    Err(error) => self.message(state, super::access::error_text(&error)),
                }
            }
        }
        ui.small(tr("Hammer Pro is free and open source. Public third-party relays avoid recurring hosting costs for the project. Service availability depends on their operator.", "Hammer Pro est gratuit et open source. Les relais publics tiers évitent des frais d'hébergement récurrents pour le projet. Leur disponibilité dépend de leur opérateur.", "Hammer Pro бесплатен и открыт. Публичные сторонние ретрансляторы позволяют избежать регулярных расходов проекта на серверы. Их доступность зависит от оператора."));
    }

    fn reconnect_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, tab: u64) {
        ui.heading(&self.session_name);
        if tab != self.bound_tab
            && ui.button(tr("Return to shared map", "Revenir à la map partagée", "Открыть общую карту")).clicked()
        {
            self.pending_switch = Some(self.bound_tab);
        }
        let Some(reconnect) = &self.reconnect else { return };
        let (attempt, next, error) = (reconnect.attempt, reconnect.next, reconnect.error.clone());
        ui.colored_label(hp_app::theme::warning(), tf!("team.connection_lost", error));
        let waiting = next.is_some();
        if self.session.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tf!("team.reconnecting", attempt, super::reconnect::ATTEMPTS));
            });
            self.join_progress_ui(ui);
            repaint_soon(ui);
        } else if let Some(next) = next {
            let seconds = next.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as u64;
            ui.label(tf!("team.reconnect_next", seconds, attempt + 1, super::reconnect::ATTEMPTS));
            repaint_soon(ui);
        }
        ui.small(t!("team.reconnect_info"));
        ui.horizontal(|ui| {
            if self.session.is_none() {
                let label = if waiting { t!("team.reconnect_now") } else { t!("team.try_again") };
                if ui.button(label).clicked()
                    && let Err(error) = self.retry_now(state)
                {
                    self.message(state, format!("{error:#}"));
                }
            }
            if ui.button(t!("team.stay_offline")).clicked() {
                state
                    .runtime
                    .collaboration
                    .requests
                    .push(hp_app::runtime::collaboration::CollaborationRequest::StopReconnecting);
            }
        });
        self.chat_log_ui(ui, state, false);
    }

    fn join_progress_ui(&self, ui: &mut egui::Ui) {
        let stats = &self.stats;
        if self.loading.is_some() || (self.session.is_some() && self.canonical.is_none()) {
            let total = stats.join_total.max(1);
            let fraction = if self.loading.is_some() { 1.0 } else { stats.join_units as f32 / total as f32 };
            let label = if self.loading.is_some() { t!("team.preparing") } else { t!("team.receiving") };
            ui.label(label);
            ui.add(egui::ProgressBar::new(fraction).show_percentage());
            ui.small(tf!("team.progress", stats.join_units, stats.join_total, size_text(stats.join_bytes)));
            repaint_soon(ui);
        }
    }

    fn session_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, ctx: &egui::Context, tab: u64) {
        ui.horizontal(|ui| {
            ui.heading(&self.session_name);
            ui.label(if self.connection_mode == hp_collab::ConnectionMode::Relay {
                tr("Encrypted relay", "Relais chiffré", "Зашифрованная ретрансляция")
            } else {
                tr("Direct encrypted connection", "Connexion directe chiffrée", "Прямое зашифрованное соединение")
            });
        });
        if tab != self.bound_tab
            && ui.button(tr("Return to shared map", "Revenir à la map partagée", "Открыть общую карту")).clicked()
        {
            self.pending_switch = Some(self.bound_tab);
        }
        self.join_progress_ui(ui);
        if self.host
            && ui
                .add_enabled(
                    !self.invitation.is_empty(),
                    egui::Button::new(tr("Copy invitation", "Copier l'invitation", "Копировать приглашение")),
                )
                .clicked()
        {
            ctx.copy_text(self.invitation.to_string());
        }
        ui.separator();
        self.participants_ui(ui, state);
        self.access_ui(ui, state);
        ui.separator();
        self.chat_ui(ui, state, tab);
        ui.separator();
        egui::CollapsingHeader::new(t!("team.sync_details"))
            .id_salt("team-sync")
            .show(ui, |ui| self.sync_ui(ui, state));
        if self.host {
            egui::CollapsingHeader::new(t!("team.host_tools"))
                .id_salt("team-host-tools")
                .show(ui, |ui| self.host_ui(ui, state));
        }
        if ui.button(tr("Leave session", "Quitter la session", "Покинуть сеанс")).clicked() {
            self.stop(state);
        }
    }

    fn participants_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading(tr("Participants", "Participants", "Участники"));
        let rows: Vec<Collaborator> = state.runtime.collaboration.participants.clone();
        let following = state.runtime.collaboration.following;
        let mut request = None;
        egui::Grid::new("team-participants").num_columns(5).spacing(egui::vec2(10.0, 4.0)).striped(true).show(
            ui,
            |ui| {
                for row in &rows {
                    let own = row.id == self.participant;
                    ui.horizontal(|ui| {
                        swatch(ui, row.color);
                        let role = self.access.role_label(row.id);
                        let name = if own { format!("{} ({})", row.name, t!("team.you")) } else { row.name.clone() };
                        ui.label(egui::RichText::new(name).strong().color(hp_app::theme::rgb(row.color)));
                        ui.weak(role);
                    });
                    let (upload, download, round_trip) = if own {
                        (
                            self.people.rates.upload,
                            self.people.rates.download,
                            self.stats.round_trip_average_ms.round() as u32,
                        )
                    } else {
                        (row.upload, row.download, row.round_trip_ms)
                    };
                    ui.label(format!("{} ms", row.latency_ms)).on_hover_text(t!("team.row_tip"));
                    ui.small(tf!("team.traffic_rate", rate_text(upload), rate_text(download)))
                        .on_hover_text(t!("team.row_tip"));
                    ui.horizontal(|ui| {
                        ui.small(tf!("team.edit_time", round_trip)).on_hover_text(t!("team.row_tip"));
                        if row.editing {
                            ui.colored_label(hp_app::theme::rgb(row.color), t!("team.editing"));
                        }
                    });
                    ui.horizontal(|ui| {
                        if own {
                            return;
                        }
                        if ui.small_button(tr("Teleport", "Téléporter", "Перейти")).clicked() {
                            request = Some((row.id, CameraMode::Teleport));
                        }
                        let active = following == Some(row.id);
                        if ui.selectable_label(active, t!("team.follow")).on_hover_text(t!("team.follow_tip")).clicked()
                        {
                            request = Some((row.id, if active { CameraMode::Stop } else { CameraMode::Follow }));
                        }
                        ui.menu_button(t!("team.more"), |ui| {
                            if ui.button(tr("Locate", "Repérer", "Найти")).clicked() {
                                request = Some((row.id, CameraMode::Look));
                                ui.close();
                            }
                            if ui
                                .add_enabled(!row.selected.is_empty(), egui::Button::new(t!("team.show_selection")))
                                .clicked()
                            {
                                request = Some((row.id, CameraMode::Selection));
                                ui.close();
                            }
                            if self.host && row.id != 1 {
                                ui.separator();
                                if ui.button(tr("Remove", "Exclure", "Исключить")).clicked() {
                                    if let Some(session) = &self.session {
                                        let _ = session.send(Command::Kick(row.id));
                                    }
                                    ui.close();
                                }
                                if ui.button(tr("Ban", "Bannir", "Заблокировать")).clicked() {
                                    self.ban_target = Some(row.id);
                                    ui.close();
                                }
                            }
                        });
                    });
                    ui.end_row();
                }
            },
        );
        if let Some((participant, mode)) = request
            && let Err(error) = self.camera(state, participant, mode)
        {
            self.message(state, format!("{error:#}"));
        }
        for lock in state.runtime.collaboration.locks.clone() {
            ui.colored_label(hp_app::theme::rgb(lock.color), tf!("team.locks", lock.name, lock.objects));
        }
        if self.host
            && let Some(target) = self.ban_target
        {
            ui.separator();
            ui.label(tr(
                "Ban duration in minutes (0 = until session ends)",
                "Durée en minutes (0 = jusqu'à la fin de session)",
                "Срок в минутах (0 = до конца сеанса)",
            ));
            ui.add(egui::DragValue::new(&mut self.ban_minutes).range(0..=10080));
            ui.label(tr("Reason", "Motif", "Причина"));
            ui.add(egui::TextEdit::singleline(&mut self.ban_reason).char_limit(256));
            ui.horizontal(|ui| {
                if ui.button(tr("Confirm ban", "Confirmer le bannissement", "Подтвердить блокировку")).clicked()
                {
                    let seconds = (self.ban_minutes > 0).then_some(self.ban_minutes * 60);
                    if let Some(session) = &self.session
                        && let Err(error) =
                            session.send(Command::Ban { participant: target, seconds, reason: self.ban_reason.clone() })
                    {
                        self.message(state, error);
                    }
                    self.ban_target = None;
                }
                if ui.button(tr("Cancel", "Annuler", "Отмена")).clicked() {
                    self.ban_target = None;
                }
            });
        }
    }

    fn chat_log_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, interactive: bool) {
        let lines = &state.runtime.collaboration.chat;
        if lines.is_empty() && !interactive {
            return;
        }
        let mut go = None;
        egui::ScrollArea::vertical()
            .id_salt("team-chat")
            .max_height(130.0)
            .stick_to_bottom(true)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if lines.is_empty() {
                    ui.weak(t!("team.chat_empty"));
                }
                for (index, line) in lines.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}:", line.name))
                                .strong()
                                .color(hp_app::theme::rgb(line.color)),
                        );
                        if !line.text.is_empty() {
                            ui.label(&line.text);
                        }
                        if let Some(point) = line.marker {
                            if line.text.is_empty() {
                                ui.weak(t!("team.ping_line"));
                            }
                            if interactive && ui.push_id(index, |ui| ui.small_button(t!("team.go_to"))).inner.clicked()
                            {
                                go = Some(point);
                            }
                        }
                    });
                }
            });
        if let Some(point) = go {
            self.go_to_point(point);
        }
    }

    fn chat_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, tab: u64) {
        ui.strong(t!("team.chat"));
        self.chat_log_ui(ui, state, true);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 150.0).max(120.0);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.chat_input)
                    .hint_text(t!("team.message_hint"))
                    .char_limit(super::people::MAX_CHAT_CHARS)
                    .desired_width(width),
            );
            let entered = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let ready = !self.chat_input.trim().is_empty() && self.canonical.is_some();
            if ui.add_enabled(ready, egui::Button::new(t!("team.send"))).clicked() || (entered && ready) {
                let text = self.chat_input.clone();
                match self.send_note(state, &text, None) {
                    Ok(()) => {
                        self.chat_input.clear();
                        if entered {
                            edit.request_focus();
                        }
                    }
                    Err(error) => self.message(state, format!("{error:#}")),
                }
            }
            let can_ping = tab == self.bound_tab && self.canonical.is_some();
            if ui
                .add_enabled(can_ping, egui::Button::new(t!("team.ping")))
                .on_hover_text(t!("team.ping_tip"))
                .on_disabled_hover_text(t!("team.ping_needs_shared_map"))
                .clicked()
            {
                let point = Self::ping_target(state);
                if let Err(error) = self.send_note(state, "", Some(point)) {
                    self.message(state, format!("{error:#}"));
                }
            }
        });
    }

    fn sync_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        if self.host && self.connection_mode == hp_collab::ConnectionMode::Direct {
            ui.collapsing(
                tr("Local connection addresses", "Adresses locales de connexion", "Локальные адреса подключения"),
                |ui| {
                    for address in &self.host_addresses {
                        ui.label(address.to_string());
                    }
                },
            );
        }
        ui.label(format!(
            "{} {} | {} {}",
            tr("Revision", "Révision", "Версия"),
            self.revision,
            tr("Pending", "En attente", "Ожидает"),
            usize::from(self.has_submitted())
        ));
        ui.label(format!(
            "{} {} | {} {}",
            tr("Sent", "Envoyé", "Отправлено"),
            size_text(self.sent),
            tr("Received", "Reçu", "Получено"),
            size_text(self.received)
        ));
        let stats = &self.stats;
        if self.canonical.is_some() {
            ui.small(tf!(
                "team.sync",
                format!("{:.0}", stats.round_trip_ms),
                format!("{:.0}", stats.round_trip_average_ms),
                format!("{:.1}", stats.apply_ms),
                format!("{:.1}", stats.apply_max_ms),
                format!("{:.1}", stats.capture_ms)
            ));
            ui.small(tf!("team.edits", stats.edits_sent, stats.edits_received, stats.live_edits));
        }
        if ui
            .checkbox(&mut state.settings.workflow.team_live_edits, t!("team.live_edits"))
            .on_hover_text(t!("team.live_edits_tip"))
            .changed()
        {
            state.settings_dirty = true;
        }
        if !self.host
            && ui
                .add_enabled(self.canonical.is_some(), egui::Button::new(t!("team.resync")))
                .on_hover_text(t!("team.resync_tip"))
                .clicked()
            && let Err(error) = self.retry_now(state)
        {
            self.message(state, format!("{error:#}"));
        }
    }

    fn host_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.label(tr("New password", "Nouveau mot de passe", "Новый пароль"));
        ui.add(egui::TextEdit::singleline(&mut *self.password).password(true).char_limit(256));
        if ui
            .add_enabled(
                self.password.len() >= 8,
                egui::Button::new(tr(
                    "Rotate access and disconnect guests",
                    "Renouveler les accès et déconnecter les invités",
                    "Обновить доступ и отключить гостей",
                )),
            )
            .clicked()
            && let Some(session) = &self.session
            && let Err(error) = session.send(Command::Rotate(self.password.clone()))
        {
            self.message(state, error);
        }
        ui.collapsing(
            tr("Session ban list", "Bannissements de la session", "Блокировки сеанса"),
            |ui| {
                for row in self.bans.clone() {
                    ui.horizontal(|ui| {
                        let remaining = row.remaining_seconds.map_or_else(
                            || tr("Until session ends", "Fin de session", "До конца сеанса").into(),
                            |seconds| format!("{seconds} s"),
                        );
                        ui.label(format!("{} | {} | {}", row.nickname, remaining, row.reason));
                        if ui.small_button(tr("Unban", "Débannir", "Разблокировать")).clicked()
                            && let Some(session) = &self.session
                        {
                            let _ = session.send(Command::Unban(row.id));
                        }
                    });
                }
            },
        );
        if self.snapshots.ui(ui, self.canonical.is_some()) {
            self.request_snapshot(state);
        }
    }

    fn recovery_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        if !self.local_only.is_empty() {
            ui.collapsing(tf!("team.local_only", self.local_only.len()), |ui| {
                for problem in &self.local_only {
                    ui.small(problem);
                }
            });
        }
        if !self.conflict.is_empty() {
            ui.small(tf!("team.conflicts", self.conflict.len()));
            let base = self.recovery_map().is_some();
            if ui
                .add_enabled(
                    self.recovery.is_none() && base,
                    egui::Button::new(tr(
                        "Recover local copy (Save As)",
                        "Récupérer la copie locale (Enregistrer sous)",
                        "Восстановить локальную копию (Сохранить как)",
                    )),
                )
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("VMF", &["vmf"])
                    .set_file_name("collaboration-local-recovery.vmf")
                    .save_file()
                && let Err(error) = self.save_recovery(path)
            {
                self.message(state, format!("{error:#}"));
            }
        }
    }
}
