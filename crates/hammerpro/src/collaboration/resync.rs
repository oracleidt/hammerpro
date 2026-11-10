use anyhow::Result;
use hp_app::{AppState, Severity};
use hp_collab::document::ValidatedSnapshot;
use hp_collab::protocol::Outcome;

use super::Controller;

impl Controller {
    pub(super) fn on_resync(
        &mut self,
        state: &mut AppState,
        revision: u64,
        outcomes: &[Outcome],
        map: ValidatedSnapshot,
    ) -> Result<()> {
        let resolved = self
            .pending
            .as_ref()
            .and_then(|pending| outcomes.iter().find(|outcome| outcome.sequence == pending.sequence))
            .cloned();
        if let Some(outcome) = resolved {
            match &outcome.rejection {
                None => self.acknowledge(state, self.revision, Vec::new())?,
                Some(rejection) => self.refused(state, outcome.sequence, rejection, self.revision)?,
            }
        }
        let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        let changes = canonical.diff(&map);
        drop(map);
        if changes.is_empty() {
            self.revision = revision;
        } else {
            self.apply_remote(state, 0, revision, changes)?;
        }
        self.stats.resyncs += 1;
        state.notify(Severity::Info, hp_app::i18n::tr("team.access.resynced"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hp_collab::document::ValidatedSnapshot;
    use hp_collab::protocol::{EditRefusal, Outcome, Rejection};
    use hp_collab::{Change, Event, Snapshot};

    use super::super::sync::{Origin, Pending};
    use super::super::*;

    fn guest() -> (Controller, AppState, Snapshot) {
        let mut state = hp_app::testing::test_state();
        state.doc = hp_editor::Document::from_map(hp_vmf::parse("world {\"id\" \"1\" \"classname\" \"worldspawn\"} entity {\"id\" \"2\" \"classname\" \"light\" \"_light\" \"255 255 255 200\" \"origin\" \"0 0 0\"} entity {\"id\" \"3\" \"classname\" \"light\" \"origin\" \"64 0 0\"}").unwrap());
        let mut map = state.doc.to_map();
        map.format = hp_vmf::TextFormat::default();
        let snapshot = Snapshot::from_vmf(&hp_vmf::write(&map)).unwrap();
        let controller = Controller {
            participant: 2,
            revision: 3,
            canonical: Some(ValidatedSnapshot::new(snapshot.clone()).unwrap()),
            last_content_serial: state.doc.content_serial(),
            ..Default::default()
        };
        (controller, state, snapshot)
    }

    fn with_key(map: &Snapshot, id: u32, key: &str, value: &str) -> Snapshot {
        let mut parsed = hp_vmf::parse(&map.to_vmf().unwrap()).unwrap();
        parsed.entities.iter_mut().find(|entity| entity.id == id).unwrap().set(key, value);
        Snapshot::from_vmf(&hp_vmf::write(&parsed)).unwrap()
    }

    fn submit(controller: &mut Controller, state: &mut AppState, before: &Snapshot, sequence: u64) -> Vec<Change> {
        state.doc.set_entity_key(&[2], "targetname", "mine").unwrap();
        controller.last_content_serial = state.doc.content_serial();
        let mine = with_key(before, 2, "targetname", "mine");
        let changes = before.diff(&mine);
        controller.pending = Some(Pending {
            sequence,
            changes: changes.clone(),
            undo: mine.diff(before),
            origin: Origin::Edit,
            sent: std::time::Instant::now(),
            detached: false,
        });
        changes
    }

    fn local(state: &AppState, id: u32, key: &str) -> Option<String> {
        state.doc.scene().entity(id).unwrap().entity.get(key).map(str::to_owned)
    }

    #[test]
    fn a_resync_acknowledges_the_pending_edit_and_applies_the_missed_changes() {
        let (mut controller, mut state, before) = guest();
        submit(&mut controller, &mut state, &before, 4);
        let remote = with_key(&with_key(&before, 2, "targetname", "mine"), 3, "targetname", "theirs");
        let outcomes = vec![Outcome { sequence: 4, rejection: None }];
        let map = ValidatedSnapshot::new(remote.clone()).unwrap();
        controller.process(&mut state, Event::Resync { revision: 9, outcomes, map }).unwrap();
        assert!(controller.pending.is_none() && controller.conflict.is_empty());
        assert_eq!(controller.revision, 9);
        assert_eq!(controller.undo.len(), 1, "the acknowledged edit can be undone");
        assert_eq!(controller.canonical.as_deref(), Some(&remote));
        assert_eq!(local(&state, 3, "targetname").as_deref(), Some("theirs"));
        assert_eq!(local(&state, 2, "targetname").as_deref(), Some("mine"));
        assert_eq!(controller.stats.resyncs, 1);
    }

    #[test]
    fn a_resync_reports_a_rejected_pending_edit_and_keeps_it_for_recovery() {
        let (mut controller, mut state, before) = guest();
        submit(&mut controller, &mut state, &before, 2);
        let remote = with_key(&before, 2, "_light", "0 0 255 200");
        let rejection = Rejection::new(EditRefusal::Selected, "entity:2 (Bob)");
        let outcomes = vec![Outcome { sequence: 2, rejection: Some(rejection) }];
        let map = ValidatedSnapshot::new(remote.clone()).unwrap();
        controller.process(&mut state, Event::Resync { revision: 3, outcomes, map }).unwrap();
        assert!(controller.pending.is_none());
        assert!(controller.conflict.contains_key("entity:2"), "the refused version is kept for recovery");
        assert_eq!(controller.canonical.as_deref(), Some(&remote));
        assert_eq!(local(&state, 2, "targetname"), None);
        assert_eq!(local(&state, 2, "_light").as_deref(), Some("0 0 255 200"));
        assert!(controller.status.contains("entity:2 (Bob)"), "{}", controller.status);
    }

    #[test]
    fn a_resync_keeps_a_pending_edit_the_host_has_not_processed_yet() {
        let (mut controller, mut state, before) = guest();
        let changes = submit(&mut controller, &mut state, &before, 6);
        let remote = with_key(&before, 3, "targetname", "theirs");
        let map = ValidatedSnapshot::new(remote.clone()).unwrap();
        controller.process(&mut state, Event::Resync { revision: 5, outcomes: Vec::new(), map }).unwrap();
        assert_eq!(controller.pending.as_ref().map(|pending| pending.sequence), Some(6));
        assert_eq!(local(&state, 2, "targetname").as_deref(), Some("mine"), "the local edit stays");
        assert_eq!(local(&state, 3, "targetname").as_deref(), Some("theirs"), "remote changes merge under it");
        controller
            .process(&mut state, Event::Applied { author: 2, sequence: 6, revision: 6, changes: Vec::new() })
            .unwrap();
        assert!(controller.pending.is_none() && controller.conflict.is_empty());
        let mut expected = remote.clone();
        expected.apply(&changes).unwrap();
        assert_eq!(controller.canonical.as_deref(), Some(&expected));
        assert_eq!(controller.revision, 6);
    }
}
