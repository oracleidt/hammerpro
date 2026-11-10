use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use anyhow::Result;
use hp_app::runtime::collaboration::Reconnecting;
use hp_app::{AppState, Severity, t, tf};
use hp_collab::document::{ValidatedSnapshot, digest};
use hp_collab::{Change, Snapshot, Unit};
use zeroize::Zeroizing;

use super::Controller;
use super::sync::{Origin, decode, touched_keys};

pub(super) const ATTEMPTS: u32 = 6;
const BACKOFF_SECONDS: [u64; ATTEMPTS as usize] = [1, 2, 4, 8, 15, 30];

const FINAL: &[&str] = &[
    "Host ended the session",
    "Host rotated access",
    "Removed by the host",
    "Replaced by a new connection",
    "Session full",
    "Nickname already in use",
    "access revoked",
    "Authentication",
    "Invitation or password rejected",
    "Invalid sequence",
    "Invalid session message",
    "Unsupported",
    "Banned",
];

const NETWORK: &[&str] = &[
    "timed out",
    "stalled",
    "host disconnected",
    "connection lost",
    "reset by peer",
    "slow participant",
    "revision mismatch",
    "stream closed",
    "stopped receiving",
    "could not connect",
    "closed by peer",
    "aborted by peer",
    "session ended",
];

pub(super) fn recoverable(error: &str) -> bool {
    let lower = error.to_lowercase();
    !FINAL.iter().any(|word| error.contains(word)) && NETWORK.iter().any(|word| lower.contains(word))
}

pub(super) struct Credentials {
    pub(super) invitation: Zeroizing<String>,
    pub(super) password: Zeroizing<String>,
    pub(super) address: String,
}

pub(super) struct Reconnect {
    pub(super) attempt: u32,
    pub(super) next: Option<Instant>,
    pub(super) error: String,
    pub(super) base: Option<ValidatedSnapshot>,
    pub(super) serial: u64,
    pub(super) keys: BTreeSet<String>,
}

pub(super) struct Offline {
    base: Option<ValidatedSnapshot>,
    units: Vec<(String, Option<Unit>)>,
}

impl Controller {
    pub(super) fn reconnectable(&self, error: &str) -> bool {
        !self.host
            && self.credentials.is_some()
            && self.denied.is_none_or(|refusal| refusal == hp_collab::protocol::Refusal::Busy)
            && (recoverable(error) || self.denied.is_some())
            && (self.canonical.is_some() || self.reconnect.is_some())
    }

    pub(super) fn connection_lost(&mut self, state: &mut AppState, error: String, immediate: bool) {
        let now = Instant::now();
        let mut keys = BTreeSet::new();
        if let Some(pending) = &self.pending {
            keys.extend(pending.changes.iter().map(|change| change.key.clone()));
        }
        if let Some(capturing) = &self.capturing {
            keys.extend(capturing.keys.iter().cloned());
        }
        if let Some(live) = &self.live {
            keys.extend(live.keys().cloned());
        }
        keys.extend(self.held.iter().cloned());
        let previous = self.reconnect.take();
        let (base, serial) = match (self.canonical.take(), previous) {
            (Some(map), _) => (Some(map), self.last_content_serial),
            (None, Some(previous)) => {
                keys.extend(previous.keys);
                (previous.base, previous.serial)
            }
            (None, None) => (None, self.last_content_serial),
        };
        self.halt(state);
        let wait = if immediate { Duration::ZERO } else { Duration::from_secs(BACKOFF_SECONDS[0]) };
        self.reconnect =
            Some(Reconnect { attempt: 0, next: Some(now + wait), error: error.clone(), base, serial, keys });
        self.status = tf!("team.connection_lost", error);
        state.notify(Severity::Warning, &self.status);
        self.publish_reconnect(state);
    }

    pub(super) fn reconnect_tick(&mut self, state: &mut AppState, ctx: &egui::Context) {
        let Some(reconnect) = &self.reconnect else { return };
        if self.session.is_some() {
            return;
        }
        let Some(next) = reconnect.next else {
            self.publish_reconnect(state);
            return;
        };
        let now = Instant::now();
        if next > now {
            ctx.request_repaint_after((next - now).min(Duration::from_millis(250)));
            self.publish_reconnect(state);
            return;
        }
        self.attempt_reconnect(state);
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn attempt_reconnect(&mut self, state: &mut AppState) {
        let Some(credentials) = &self.credentials else {
            self.give_up(state, t!("team.reconnect_no_credentials").to_owned());
            return;
        };
        self.invitation_input = credentials.invitation.clone();
        self.password = credentials.password.clone();
        self.direct_address = credentials.address.clone();
        let Some(reconnect) = self.reconnect.as_mut() else { return };
        reconnect.attempt += 1;
        reconnect.next = None;
        let attempt = reconnect.attempt;
        match self.open_session(false) {
            Ok(()) => {
                self.status = tf!("team.reconnecting", attempt, ATTEMPTS);
                self.publish_reconnect(state);
            }
            Err(error) => self.attempt_failed(state, format!("{error:#}")),
        }
    }

    pub(super) fn attempt_failed(&mut self, state: &mut AppState, error: String) {
        self.session = None;
        self.loading = None;
        self.waker = false;
        let Some(reconnect) = self.reconnect.as_mut() else { return };
        reconnect.error = error.clone();
        let busy = self.denied == Some(hp_collab::protocol::Refusal::Busy);
        if reconnect.attempt >= ATTEMPTS || !(recoverable(&error) || busy) {
            self.give_up(state, error);
            return;
        }
        let seconds = BACKOFF_SECONDS[reconnect.attempt as usize];
        reconnect.next = Some(Instant::now() + Duration::from_secs(seconds));
        self.status = tf!("team.reconnect_wait", error, seconds);
        self.publish_reconnect(state);
    }

    fn give_up(&mut self, state: &mut AppState, error: String) {
        if let Some(reconnect) = self.reconnect.as_mut() {
            reconnect.next = None;
            reconnect.error = error.clone();
        }
        self.status = tf!("team.reconnect_failed", error);
        state.notify(Severity::Warning, &self.status);
        self.publish_reconnect(state);
    }

    pub(super) fn retry_now(&mut self, state: &mut AppState) -> Result<()> {
        if self.reconnect.is_some() {
            anyhow::ensure!(self.session.is_none(), "A reconnection attempt is already running");
            if let Some(reconnect) = self.reconnect.as_mut() {
                if reconnect.next.is_none() {
                    reconnect.attempt = 0;
                }
                reconnect.next = Some(Instant::now());
            }
            self.publish_reconnect(state);
            return Ok(());
        }
        anyhow::ensure!(self.session.is_some() && !self.host, "Only a guest can reconnect to the session");
        anyhow::ensure!(self.credentials.is_some(), "Join the session first");
        anyhow::ensure!(self.canonical.is_some(), "The shared map is still arriving");
        self.connection_lost(state, t!("team.resync_reason").to_owned(), true);
        Ok(())
    }

    pub(super) fn publish_reconnect(&self, state: &mut AppState) {
        let value = self.reconnect.as_ref().map(|reconnect| Reconnecting {
            attempt: reconnect.attempt,
            attempts: ATTEMPTS,
            next: reconnect.next,
            error: reconnect.error.clone(),
            connecting: self.session.is_some(),
        });
        if state.runtime.collaboration.reconnecting != value {
            state.runtime.collaboration.reconnecting = value;
        }
    }

    pub(super) fn offline_changes(&mut self, state: &mut AppState, reconnect: Reconnect) -> Result<Offline> {
        let Reconnect { base, serial, mut keys, .. } = reconnect;
        match state.doc.shared_touched_since(serial) {
            Some(touched) => keys.extend(touched.keys()),
            None => {
                let mut map = state.doc.to_map();
                map.format = hp_vmf::TextFormat::default();
                let current = Snapshot::from_vmf(&hp_vmf::write(&map))?;
                let reference = base.as_deref().cloned().unwrap_or_default();
                keys.extend(reference.diff(&current).into_iter().map(|change| change.key));
            }
        }
        if keys.is_empty() {
            return Ok(Offline { base, units: Vec::new() });
        }
        let mut problems = Vec::new();
        let local = Self::export_units(state, &touched_keys(keys.iter()), &mut problems)?;
        self.report_local_only(state, problems);
        let units = local
            .into_iter()
            .filter(|(key, unit)| base.as_ref().is_none_or(|base| base.0.get(key) != unit.as_ref()))
            .collect();
        Ok(Offline { base, units })
    }

    pub(super) fn resend_offline(&mut self, state: &mut AppState, offline: Offline) -> Result<()> {
        let Offline { base, units } = offline;
        let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        let mut resend = Vec::new();
        let mut kept = Vec::new();
        for (key, unit) in units {
            let current = canonical.0.get(&key);
            if current == unit.as_ref() {
                continue;
            }
            let untouched = base.as_ref().is_some_and(|base| base.0.get(&key) == current);
            if untouched {
                resend.push(Change { key, before: current.map(digest), after: unit });
            } else {
                kept.push(key.clone());
                self.conflict.insert(key, unit);
            }
        }
        if !resend.is_empty() && canonical.check(&resend).is_err() {
            for change in resend.drain(..) {
                kept.push(change.key.clone());
                self.conflict.insert(change.key, change.after);
            }
        }
        self.stats.reconnects += 1;
        self.stats.offline_resent = resend.len();
        if !resend.is_empty() {
            let update = decode(&resend.iter().collect::<Vec<_>>(), state.doc.scene().world().id)?;
            state.doc.apply_shared_update(update)?;
            self.last_content_serial = state.doc.content_serial();
            let count = resend.len();
            self.submit(state, resend, Origin::Edit)?;
            state.notify(Severity::Info, tf!("team.reconnected_resent", count));
        } else {
            state.notify(Severity::Info, t!("team.reconnected"));
        }
        if !kept.is_empty() {
            let message = tf!("team.offline_kept", kept.len());
            self.message(state, message);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::recoverable;

    #[test]
    fn only_network_failures_reconnect() {
        for error in [
            "connection lost: timed out",
            "Session header receive stalled",
            "Host disconnected",
            "connection lost: closed by peer: Slow participant (code 1)",
            "Session revision mismatch",
            "Connecting to the session timed out",
            "Could not connect to the session: no route",
        ] {
            assert!(recoverable(error), "{error}");
        }
        for error in [
            "connection lost: closed by peer: Host ended the session (code 0)",
            "Removed by the host. You may reconnect with valid credentials.",
            "Nickname already in use",
            "Invitation or password rejected",
            "Banned by the host",
            "Session full or access revoked",
            "closed by peer: Host rotated access credentials (code 0)",
            "Invalid participant details",
        ] {
            assert!(!recoverable(error), "{error}");
        }
    }
}
