use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use glam::{DVec2, DVec3};
use hp_app::runtime::collaboration::{
    CONFLICT_COLOR, CameraMode, ChatLine, Collaborator, Highlight, Lock, Marker, assign_colors,
};
use hp_app::{AppState, Severity, t, tf};
use hp_collab::{Activity, Note, Participant};
use hp_editor::ObjRef;

use super::Controller;

pub(super) const NOTE_LIFETIME: Duration = Duration::from_secs(20);
pub(super) const MARKER_LIFETIME: Duration = Duration::from_secs(8);
pub(super) const NOTE_SPACING: Duration = Duration::from_millis(300);
pub(super) const EDITING_WINDOW: Duration = Duration::from_secs(2);
const EDIT_HIGHLIGHT: Duration = Duration::from_millis(1500);
const CONFLICT_HIGHLIGHT: Duration = Duration::from_secs(5);
const HIGHLIGHT_KEYS: usize = 512;
const CHAT_LINES: usize = 200;
const AUTHORS: usize = 16_384;
const CENTER_SAMPLE: usize = 256;
const RATE_INTERVAL: Duration = Duration::from_secs(2);
const FOLLOW_SPEED: f64 = 10.0;
pub(super) const MAX_CHAT_CHARS: usize = 200;

pub(super) fn object_key(object: &ObjRef) -> String {
    match object {
        ObjRef::Solid(id) => format!("solid:{id}"),
        ObjRef::Entity(id) => format!("entity:{id}"),
    }
}

pub(super) fn object_of(key: &str) -> Option<ObjRef> {
    if let Some(id) = key.strip_prefix("solid:") {
        return id.parse().ok().map(ObjRef::Solid);
    }
    key.strip_prefix("entity:").and_then(|id| id.parse().ok()).map(ObjRef::Entity)
}

pub(super) fn quantize_rate(bytes_per_second: f64) -> u32 {
    if !bytes_per_second.is_finite() || bytes_per_second < 125.0 {
        return 0;
    }
    if bytes_per_second < 1000.0 {
        return ((bytes_per_second / 250.0).round() * 250.0) as u32;
    }
    let magnitude = 10f64.powi(bytes_per_second.log10().floor() as i32 - 1);
    ((bytes_per_second / magnitude).round() * magnitude).min(u32::MAX as f64) as u32
}

pub(super) fn clean_text(text: &str) -> String {
    let replaced: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut out = String::new();
    for character in replaced.trim().chars().take(MAX_CHAT_CHARS) {
        if out.len() + character.len_utf8() > hp_collab::protocol::MAX_NOTE_BYTES {
            break;
        }
        out.push(character);
    }
    out
}

#[derive(Clone, Copy)]
struct Follow {
    participant: u32,
    last: Option<(DVec3, f64, f64, [DVec2; 3])>,
    frame: Instant,
}

#[derive(Default)]
pub(super) struct People {
    notes: Vec<(Note, Instant)>,
    outbox: VecDeque<(String, Option<[f64; 3]>)>,
    serial: u32,
    last_note: Option<Instant>,
    seen: BTreeSet<(u32, u32)>,
    activity: BTreeMap<u32, Instant>,
    authors: HashMap<String, u32>,
    follow: Option<Follow>,
    traffic: Option<(u64, u64, Instant)>,
    pub(super) rates: Activity,
    centers: Option<(u64, u64)>,
    locks: Option<(u64, u64)>,
    mine: Vec<String>,
    mine_serial: u64,
    pending_frame: Option<(DVec3, DVec3)>,
}

impl People {
    pub(super) fn live_notes(&self) -> Vec<Note> {
        self.notes.iter().map(|(note, _)| note.clone()).collect()
    }

    pub(super) fn queue(&mut self, text: String, marker: Option<[f64; 3]>) {
        self.outbox.push_back((text, marker));
        while self.outbox.len() > 16 {
            self.outbox.pop_front();
        }
    }

    pub(super) fn release(&mut self, now: Instant) -> Option<Note> {
        let before = self.notes.len();
        self.notes.retain(|(_, at)| now.duration_since(*at) < NOTE_LIFETIME);
        let expired = self.notes.len() != before;
        if self.last_note.is_some_and(|last| now.duration_since(last) < NOTE_SPACING) {
            return expired.then(Note::default);
        }
        let Some((text, marker)) = self.outbox.pop_front() else { return expired.then(Note::default) };
        self.serial += 1;
        let note = Note { serial: self.serial, text, marker };
        self.notes.push((note.clone(), now));
        while self.notes.len() > hp_collab::protocol::MAX_NOTES {
            self.notes.remove(0);
        }
        self.last_note = Some(now);
        Some(note)
    }

    pub(super) fn set_selection(&mut self, selected: &[String]) {
        if self.mine != selected {
            self.mine = selected.to_vec();
            self.mine_serial += 1;
        }
    }

    pub(super) fn waiting(&self) -> bool {
        !self.outbox.is_empty()
    }

    pub(super) fn following(&self) -> Option<u32> {
        self.follow.map(|follow| follow.participant)
    }

    pub(super) fn author_of(&self, key: &str) -> Option<u32> {
        self.authors.get(key).copied()
    }

    pub(super) fn clear_session(&mut self) {
        let (serial, mine_serial) = (self.serial, self.mine_serial + 1);
        *self = Self { serial, mine_serial, ..Self::default() };
    }
}

fn describe(state: &AppState, key: &str) -> String {
    match object_of(key) {
        Some(ObjRef::Entity(id)) => match state.doc.scene().entity(id) {
            Some(node) => format!("{} {id}", node.entity.classname),
            None => tf!("team.entity", id),
        },
        Some(ObjRef::Solid(id)) => tf!("team.brush", id),
        None => key.to_owned(),
    }
}

pub(super) fn describe_keys<'a>(state: &AppState, keys: impl IntoIterator<Item = &'a String>) -> String {
    let keys: Vec<&String> = keys.into_iter().collect();
    let mut text = keys.iter().take(3).map(|key| describe(state, key)).collect::<Vec<_>>().join(", ");
    if keys.len() > 3 {
        text.push_str(&format!(" (+{})", keys.len() - 3));
    }
    text
}

impl Controller {
    pub(super) fn on_participants(&mut self, state: &mut AppState, rows: Vec<Participant>) {
        let now = Instant::now();
        if self.host {
            let previous: BTreeSet<u32> = state.runtime.collaboration.participants.iter().map(|peer| peer.id).collect();
            for row in &rows {
                if row.id != 1 && !previous.contains(&row.id) {
                    state.notify(Severity::Info, tf!("team.joined", row.name));
                }
            }
        }
        let people: Vec<(u32, &str)> = rows.iter().map(|row| (row.id, row.name.as_str())).collect();
        let colors = assign_colors(&people);
        let old = std::mem::take(&mut state.runtime.collaboration.participants);
        let mut changed = old.len() != rows.len();
        let mut fresh = Vec::new();
        let mut collaborators = Vec::with_capacity(rows.len());
        for (row, color) in rows.into_iter().zip(colors) {
            let previous = old.iter().find(|peer| peer.id == row.id);
            changed |= previous.is_none_or(|peer| peer.selected != row.presence.selected || peer.color != color);
            if row.id != self.participant {
                for note in &row.presence.notes {
                    if self.people.seen.insert((row.id, note.serial)) {
                        fresh.push((row.id, row.name.clone(), color, note.clone()));
                    }
                }
            }
            let editing = self.people.activity.get(&row.id).is_some_and(|at| now.duration_since(*at) < EDITING_WINDOW);
            collaborators.push(Collaborator {
                id: row.id,
                name: row.name,
                position: row.presence.position,
                direction: row.presence.direction,
                selected: row.presence.selected,
                latency_ms: row.latency_ms,
                color,
                upload: row.presence.activity.upload,
                download: row.presence.activity.download,
                round_trip_ms: row.presence.activity.round_trip_ms,
                editing,
                selection_center: previous.and_then(|peer| peer.selection_center),
            });
        }
        let present: BTreeSet<u32> = collaborators.iter().map(|peer| peer.id).collect();
        self.people.seen.retain(|(id, _)| present.contains(id));
        state.runtime.collaboration.participants = collaborators;
        if changed {
            state.runtime.collaboration.overlay_serial += 1;
            self.people.centers = None;
            self.people.locks = None;
        }
        for (author, name, color, note) in fresh {
            self.receive_note(state, author, name, color, note, now);
        }
        if let Some(following) = self.people.following()
            && !present.contains(&following)
        {
            self.stop_following(state, true);
        }
    }

    fn receive_note(
        &mut self,
        state: &mut AppState,
        author: u32,
        name: String,
        color: [u8; 3],
        note: Note,
        now: Instant,
    ) {
        let runtime = &mut state.runtime.collaboration;
        if let Some(position) = note.marker {
            runtime.markers.retain(|marker| marker.author != author);
            runtime.markers.push(Marker { author, name: name.clone(), color, position, at: now });
            runtime.overlay_serial += 1;
        }
        let text = note.text.clone();
        runtime.chat.push_back(ChatLine {
            author,
            name: name.clone(),
            color,
            text,
            marker: note.marker,
            at: now,
            own: false,
        });
        while runtime.chat.len() > CHAT_LINES {
            runtime.chat.pop_front();
        }
        if !runtime.show {
            runtime.unread += 1;
            let message =
                if note.text.is_empty() { tf!("team.pinged", name) } else { format!("{name}: {}", note.text) };
            state.notify(Severity::Info, message);
        }
    }

    pub(super) fn send_note(
        &mut self,
        state: &mut AppState,
        text: &str,
        marker: Option<[f64; 3]>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(self.session.is_some() && self.canonical.is_some(), "{}", t!("team.not_connected"));
        let text = clean_text(text);
        let marker = marker.filter(|point| point.iter().all(|n| n.is_finite() && n.abs() <= 1_000_000.0));
        anyhow::ensure!(!text.is_empty() || marker.is_some(), "Write a message");
        let now = Instant::now();
        let runtime = &mut state.runtime.collaboration;
        let color = runtime.color_of(self.participant);
        let name = self.name.clone();
        if let Some(position) = marker {
            runtime.markers.retain(|marker| marker.author != self.participant);
            runtime.markers.push(Marker { author: self.participant, name: name.clone(), color, position, at: now });
            runtime.overlay_serial += 1;
        }
        runtime.chat.push_back(ChatLine {
            author: self.participant,
            name,
            color,
            text: text.clone(),
            marker,
            at: now,
            own: true,
        });
        while runtime.chat.len() > CHAT_LINES {
            runtime.chat.pop_front();
        }
        self.people.queue(text, marker);
        Ok(())
    }

    pub(super) fn ping_target(state: &AppState) -> [f64; 3] {
        let camera = state.cameras.cam3d;
        let ray = hp_geom::Ray::new(camera.pos, camera.forward());
        let hit = state.view_pick(&ray, &hp_editor::PickOptions { max_distance: 65_536.0, ..Default::default() });
        hit.map_or(camera.pos + camera.forward() * 256.0, |hit| hit.point).to_array()
    }

    pub(super) fn note_remote_edit<'a>(
        &mut self,
        state: &mut AppState,
        author: u32,
        keys: impl IntoIterator<Item = &'a String>,
    ) {
        if author == 0 {
            return;
        }
        let now = Instant::now();
        self.people.activity.insert(author, now);
        if self.people.authors.len() > AUTHORS {
            self.people.authors.clear();
        }
        let mut shown = Vec::new();
        for key in keys {
            self.people.authors.insert(key.clone(), author);
            if shown.len() < HIGHLIGHT_KEYS && object_of(key).is_some() {
                shown.push(key.clone());
            }
        }
        let runtime = &mut state.runtime.collaboration;
        if let Some(peer) = runtime.participants.iter_mut().find(|peer| peer.id == author) {
            peer.editing = true;
        }
        if shown.is_empty() {
            return;
        }
        let color = runtime.color_of(author);
        let label = runtime.participant(author).map(|peer| peer.name.clone()).unwrap_or_default();
        let until = now + EDIT_HIGHLIGHT;
        match runtime.highlights.iter_mut().find(|highlight| highlight.author == author && !highlight.conflict) {
            Some(highlight) => {
                highlight.keys = shown;
                highlight.until = until;
            }
            None => runtime.highlights.push(Highlight { author, label, color, keys: shown, until, conflict: false }),
        }
        runtime.overlay_serial += 1;
    }

    pub(super) fn conflict_message<'a>(
        &self,
        state: &mut AppState,
        keys: impl IntoIterator<Item = &'a String>,
    ) -> String {
        let keys: Vec<&String> = keys.into_iter().collect();
        let mut by_author: BTreeMap<u32, Vec<&String>> = BTreeMap::new();
        for key in &keys {
            if let Some(author) = self.people.author_of(key).filter(|author| *author != self.participant) {
                by_author.entry(author).or_default().push(key);
            }
        }
        let shown: Vec<String> = keys.iter().filter_map(|key| object_of(key).map(|_| (*key).clone())).collect();
        if !shown.is_empty() {
            let runtime = &mut state.runtime.collaboration;
            runtime.highlights.push(Highlight {
                author: 0,
                label: t!("team.conflict_label").to_owned(),
                color: CONFLICT_COLOR,
                keys: shown.into_iter().take(HIGHLIGHT_KEYS).collect(),
                until: Instant::now() + CONFLICT_HIGHLIGHT,
                conflict: true,
            });
            runtime.overlay_serial += 1;
        }
        let named = by_author.into_iter().find_map(|(author, objects)| {
            state.runtime.collaboration.participant(author).map(|peer| (peer.name.clone(), objects))
        });
        match named {
            Some((name, objects)) if !name.trim().is_empty() => {
                tf!("team.conflict_by", name, describe_keys(state, objects))
            }
            _ if !keys.is_empty() => tf!("team.conflict_on", describe_keys(state, keys)),
            _ => t!("team.conflict").to_owned(),
        }
    }

    pub(super) fn lock_message(&self, state: &mut AppState, keys: &[String]) -> Option<String> {
        let runtime = &state.runtime.collaboration;
        let holder = runtime.participants.iter().filter(|peer| peer.id != self.participant).find_map(|peer| {
            let locked: Vec<&String> = keys.iter().filter(|key| peer.selected.contains(key)).collect();
            (!locked.is_empty()).then(|| (peer.name.clone(), locked))
        })?;
        let (name, locked) = holder;
        let objects = describe_keys(state, locked.iter().copied());
        let shown: Vec<String> = locked.iter().map(|key| (*key).clone()).collect();
        let runtime = &mut state.runtime.collaboration;
        runtime.highlights.push(Highlight {
            author: 0,
            label: t!("team.conflict_label").to_owned(),
            color: CONFLICT_COLOR,
            keys: shown,
            until: Instant::now() + CONFLICT_HIGHLIGHT,
            conflict: true,
        });
        runtime.overlay_serial += 1;
        Some(tf!("team.locked_by", name, objects))
    }

    pub(super) fn refresh_people(&mut self, state: &mut AppState, ctx: &egui::Context) {
        let now = Instant::now();
        let runtime = &mut state.runtime.collaboration;
        let before = (runtime.markers.len(), runtime.highlights.len());
        runtime.markers.retain(|marker| now.duration_since(marker.at) < MARKER_LIFETIME);
        runtime.highlights.retain(|highlight| highlight.until > now);
        let mut changed = before != (runtime.markers.len(), runtime.highlights.len());
        for peer in &mut runtime.participants {
            let editing = self.people.activity.get(&peer.id).is_some_and(|at| now.duration_since(*at) < EDITING_WINDOW);
            changed |= peer.editing != editing;
            peer.editing = editing;
        }
        if changed {
            runtime.overlay_serial += 1;
        }
        if !runtime.markers.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if let Some(next) = runtime
            .highlights
            .iter()
            .map(|highlight| highlight.until)
            .chain(self.people.activity.values().map(|at| *at + EDITING_WINDOW).filter(|at| *at > now))
            .min()
        {
            ctx.request_repaint_after(next.saturating_duration_since(now) + Duration::from_millis(5));
        }
        self.people.activity.retain(|_, at| now.duration_since(*at) < EDITING_WINDOW * 4);
        self.update_centers(state);
        self.update_locks(state);
        self.update_rates(now);
    }

    fn update_centers(&mut self, state: &mut AppState) {
        let key = (state.doc.revision(), state.runtime.collaboration.overlay_serial);
        if self.people.centers == Some(key) {
            return;
        }
        self.people.centers = Some(key);
        let bounds = |entity: &hp_vmf::Entity| state.entity_bounds(entity);
        let scene = state.doc.scene();
        let centers: Vec<Option<[f64; 3]>> = state
            .runtime
            .collaboration
            .participants
            .iter()
            .map(|peer| {
                let mut sum = DVec3::ZERO;
                let mut count = 0.0;
                for object in peer.selected.iter().filter_map(|key| object_of(key)).take(CENTER_SAMPLE) {
                    if let Some(aabb) = scene.object_bounds(object, &bounds).filter(|aabb| !aabb.is_empty()) {
                        sum += (aabb.min + aabb.max) * 0.5;
                        count += 1.0;
                    }
                }
                (count > 0.0).then(|| (sum / count).to_array())
            })
            .collect();
        for (peer, center) in state.runtime.collaboration.participants.iter_mut().zip(centers) {
            peer.selection_center = center;
        }
    }

    fn update_locks(&mut self, state: &mut AppState) {
        let signature = (self.people.mine_serial, state.runtime.collaboration.overlay_serial);
        if self.people.locks == Some(signature) {
            return;
        }
        self.people.locks = Some(signature);
        let mine: HashSet<&str> = self.people.mine.iter().map(String::as_str).collect();
        let runtime = &mut state.runtime.collaboration;
        runtime.locks = runtime
            .participants
            .iter()
            .filter(|peer| peer.id != self.participant)
            .filter_map(|peer| {
                let objects = peer.selected.iter().filter(|key| mine.contains(key.as_str())).count();
                (objects > 0).then(|| Lock {
                    participant: peer.id,
                    name: peer.name.clone(),
                    color: peer.color,
                    objects,
                })
            })
            .collect();
    }

    fn update_rates(&mut self, now: Instant) {
        let round_trip = self.stats.round_trip_average_ms.round().clamp(0.0, u32::MAX as f64) as u32;
        match self.people.traffic {
            Some((sent, received, at)) if now.duration_since(at) >= RATE_INTERVAL => {
                let seconds = now.duration_since(at).as_secs_f64().max(0.001);
                let rate = |bytes: u64| quantize_rate(bytes as f64 / seconds);
                self.people.rates = Activity {
                    upload: rate(self.sent.saturating_sub(sent)),
                    download: rate(self.received.saturating_sub(received)),
                    round_trip_ms: round_trip,
                };
                self.stats.upload = self.people.rates.upload;
                self.stats.download = self.people.rates.download;
                self.people.traffic = Some((self.sent, self.received, now));
            }
            Some(_) => self.people.rates.round_trip_ms = round_trip,
            None => self.people.traffic = Some((self.sent, self.received, now)),
        }
    }

    pub(super) fn camera(&mut self, state: &mut AppState, participant: u32, mode: CameraMode) -> anyhow::Result<()> {
        if mode == CameraMode::Stop {
            self.stop_following(state, false);
            return Ok(());
        }
        anyhow::ensure!(self.session.is_some(), "{}", t!("team.not_connected"));
        anyhow::ensure!(participant != self.participant, "Choose another participant");
        let peer = state
            .runtime
            .collaboration
            .participant(participant)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No participant {participant} in this session"))?;
        self.pending_switch = Some(self.bound_tab);
        match mode {
            CameraMode::Teleport => self.pending_teleport = Some((peer.position, peer.direction)),
            CameraMode::Look => self.pending_look = Some(peer.position),
            CameraMode::Follow => {
                self.people.follow = Some(Follow { participant, last: None, frame: Instant::now() });
                state.runtime.collaboration.following = Some(participant);
            }
            CameraMode::Selection => {
                let bounds = |entity: &hp_vmf::Entity| state.entity_bounds(entity);
                let objects: Vec<ObjRef> = peer.selected.iter().filter_map(|key| object_of(key)).collect();
                anyhow::ensure!(!objects.is_empty(), "{} has nothing selected", peer.name);
                let aabb = state
                    .doc
                    .scene()
                    .objects_bounds(&objects, &bounds)
                    .filter(|aabb| !aabb.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("The selection of {} has no bounds", peer.name))?;
                self.people.pending_frame = Some((aabb.min, aabb.max));
            }
            CameraMode::Stop => {}
        }
        Ok(())
    }

    pub(super) fn go_to_point(&mut self, point: [f64; 3]) {
        let point = DVec3::from_array(point);
        if !point.is_finite() {
            return;
        }
        self.pending_switch = Some(self.bound_tab);
        self.people.pending_frame = Some((point - DVec3::splat(96.0), point + DVec3::splat(96.0)));
    }

    pub(super) fn stop_following(&mut self, state: &mut AppState, notify: bool) {
        let Some(follow) = self.people.follow.take() else { return };
        state.runtime.collaboration.following = None;
        if notify {
            let name = state
                .runtime
                .collaboration
                .participant(follow.participant)
                .map_or_else(String::new, |peer| peer.name.clone());
            state.notify(Severity::Info, tf!("team.follow_stopped", name));
        }
    }

    pub(super) fn apply_camera_requests(&mut self, state: &mut AppState, ctx: &egui::Context) {
        if let Some((min, max)) = self.people.pending_frame.take() {
            self.stop_following(state, false);
            state.cameras.cam3d.frame_box(min, max);
            state.cameras.frame_2d(min, max, egui::vec2(800.0, 600.0));
        }
        let Some(mut follow) = self.people.follow else { return };
        let cameras = state.cameras;
        let centers = [cameras.top.center, cameras.front.center, cameras.side.center];
        if let Some((pos, yaw, pitch, last_centers)) = follow.last {
            let moved = cameras.cam3d.pos.distance(pos) > 1e-6
                || (cameras.cam3d.yaw_deg - yaw).abs() > 1e-6
                || (cameras.cam3d.pitch_deg - pitch).abs() > 1e-6
                || centers.iter().zip(last_centers).any(|(a, b)| a.distance(b) > 1e-6);
            if moved {
                self.stop_following(state, true);
                return;
            }
        }
        let Some(peer) = state.runtime.collaboration.participant(follow.participant) else { return };
        let target = DVec3::from_array(peer.position);
        let Some(direction) = DVec3::from_array(peer.direction).try_normalize() else { return };
        if !target.is_finite() {
            return;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(follow.frame).as_secs_f64().clamp(0.0, 0.1);
        follow.frame = now;
        let blend = 1.0 - (-elapsed * FOLLOW_SPEED).exp();
        let camera = &mut state.cameras.cam3d;
        let forward = camera.forward();
        let close = camera.pos.distance(target) < 0.5 && forward.dot(direction) > 0.99999;
        if close {
            camera.pos = target;
        } else {
            camera.pos += (target - camera.pos) * blend;
        }
        let blended = if close { direction } else { (forward + (direction - forward) * blend).normalize_or(direction) };
        let position = camera.pos;
        camera.look_at(position + blended);
        state.cameras.center_2d(position);
        let cameras = state.cameras;
        follow.last = Some((
            cameras.cam3d.pos,
            cameras.cam3d.yaw_deg,
            cameras.cam3d.pitch_deg,
            [cameras.top.center, cameras.front.center, cameras.side.center],
        ));
        self.people.follow = Some(follow);
        if !close {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_rounded_so_idle_presence_does_not_churn() {
        assert_eq!(quantize_rate(0.0), 0);
        assert_eq!(quantize_rate(90.0), 0);
        assert_eq!(quantize_rate(130.0), 250);
        assert_eq!(quantize_rate(610.0), 500);
        assert_eq!(quantize_rate(1234.0), 1200);
        assert_eq!(quantize_rate(98_765.0), 99_000);
        assert_eq!(quantize_rate(f64::NAN), 0);
    }

    #[test]
    fn chat_text_is_trimmed_bounded_and_without_control_characters() {
        assert_eq!(clean_text("  hi\tthere\n "), "hi there");
        assert_eq!(clean_text(&"é".repeat(400)).chars().count(), hp_collab::protocol::MAX_NOTE_BYTES / 2);
        assert!(clean_text(&"x".repeat(400)).len() <= MAX_CHAT_CHARS);
        assert_eq!(object_of("solid:12"), Some(ObjRef::Solid(12)));
        assert_eq!(object_of("entity:4"), Some(ObjRef::Entity(4)));
        assert_eq!(object_of("world"), None);
        assert_eq!(object_key(&ObjRef::Entity(9)), "entity:9");
    }

    #[test]
    fn notes_leave_one_by_one_and_expire() {
        let mut people = People::default();
        let start = Instant::now();
        people.queue("a".into(), None);
        people.queue(String::new(), Some([1.0, 2.0, 3.0]));
        assert_eq!(people.release(start).map(|note| note.serial), Some(1));
        assert!(people.release(start + Duration::from_millis(10)).is_none(), "spacing between notes");
        let second = people.release(start + NOTE_SPACING).unwrap();
        assert_eq!((second.serial, second.marker), (2, Some([1.0, 2.0, 3.0])));
        assert_eq!(people.live_notes().len(), 2);
        assert!(people.release(start + NOTE_SPACING + NOTE_LIFETIME).is_some(), "expiry changes the presence");
        assert!(people.live_notes().is_empty());
        for index in 0..10 {
            people.queue(format!("{index}"), None);
        }
        let mut now = start + NOTE_LIFETIME * 2;
        while people.waiting() {
            people.release(now);
            now += NOTE_SPACING;
        }
        assert_eq!(people.live_notes().len(), hp_collab::protocol::MAX_NOTES);
        assert!(people.live_notes().iter().all(|note| note.validate()));
    }
}
