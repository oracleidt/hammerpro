use super::*;

fn fixture() -> (Controller, AppState, Snapshot) {
    let mut state = hp_app::testing::test_state();
    state.doc = hp_editor::Document::from_map(hp_vmf::parse("world {\"id\" \"1\" \"classname\" \"worldspawn\"} entity {\"id\" \"2\" \"classname\" \"light\" \"_light\" \"255 255 255 200\"} entity {\"id\" \"3\" \"classname\" \"light\"}").unwrap());
    let map = Controller::current(&state).unwrap();
    let mut controller = Controller { host: true, ..Controller::default() };
    controller
        .process(
            &mut state,
            Event::Ready {
                invitation: None,
                participant: 1,
                id_start: 1_000_000,
                revision: 0,
                map: map.clone(),
                direct_addresses: Vec::new(),
            },
        )
        .unwrap();
    (controller, state, map)
}

#[test]
fn acknowledgement_keeps_newer_local_drag_and_schedules_its_next_delta() {
    let (mut controller, mut state, before) = fixture();
    state.doc.set_entity_key(&[2], "_light", "255 0 0 200").unwrap();
    let submitted = Controller::current(&state).unwrap();
    controller.submitted =
        Some(Submitted { sequence: 1, before: before.clone(), after: submitted.clone(), history: 0 });
    state.doc.set_entity_key(&[2], "_light", "0 255 0 200").unwrap();
    let newer = Controller::current(&state).unwrap();
    controller
        .process(&mut state, Event::Applied { author: 1, sequence: 1, revision: 1, changes: before.diff(&submitted) })
        .unwrap();
    assert_eq!(Controller::current(&state).unwrap(), newer);
    assert_eq!(controller.canonical.as_ref(), Some(&submitted));
    assert_eq!(controller.last_content_serial, 0);
    assert!(controller.conflict.is_none());
}

#[test]
fn foreign_independent_edit_is_retained_and_personal_undo_leaves_it_alone() {
    let (mut controller, mut state, before) = fixture();
    state.doc.set_entity_key(&[2], "_light", "255 0 0 200").unwrap();
    let submitted = Controller::current(&state).unwrap();
    controller.submitted =
        Some(Submitted { sequence: 1, before: before.clone(), after: submitted.clone(), history: 0 });
    let mut foreign_map = hp_vmf::parse(&before.to_vmf().unwrap()).unwrap();
    foreign_map.entities.iter_mut().find(|entity| entity.id == 3).unwrap().set("targetname", "remote");
    let foreign = Snapshot::from_vmf(&hp_vmf::write(&foreign_map)).unwrap();
    controller
        .process(&mut state, Event::Applied { author: 2, sequence: 1, revision: 1, changes: before.diff(&foreign) })
        .unwrap();
    controller
        .process(&mut state, Event::Applied { author: 1, sequence: 1, revision: 2, changes: before.diff(&submitted) })
        .unwrap();
    let mut undone = Controller::current(&state).unwrap();
    undone.apply(&controller.undo.back().unwrap().undo).unwrap();
    assert_eq!(undone, foreign);
    assert!(controller.conflict.is_none());
}

#[test]
fn rejection_does_not_replace_saved_conflict_with_the_remote_version() {
    let (mut controller, mut state, before) = fixture();
    state.doc.set_entity_key(&[2], "_light", "255 0 0 200").unwrap();
    let submitted = Controller::current(&state).unwrap();
    controller.submitted =
        Some(Submitted { sequence: 1, before: before.clone(), after: submitted.clone(), history: 0 });
    let mut remote_map = hp_vmf::parse(&before.to_vmf().unwrap()).unwrap();
    remote_map.entities.iter_mut().find(|entity| entity.id == 2).unwrap().set("_light", "0 0 255 200");
    let remote = Snapshot::from_vmf(&hp_vmf::write(&remote_map)).unwrap();
    controller
        .process(&mut state, Event::Applied { author: 2, sequence: 1, revision: 1, changes: before.diff(&remote) })
        .unwrap();
    assert_eq!(controller.conflict.as_ref(), Some(&submitted));
    controller
        .process(
            &mut state,
            Event::Rejected { sequence: 1, reason: "conflict".into(), revision: 1, map: remote.clone() },
        )
        .unwrap();
    assert_eq!(controller.conflict.as_ref(), Some(&submitted));
    assert_eq!(Controller::current(&state).unwrap(), remote);
}

#[test]
fn long_transaction_merges_many_foreign_updates_without_interrupting_local_edit() {
    let (mut controller, mut state, mut canonical) = fixture();
    state.doc.begin("local drag");
    state.doc.set_entity_key(&[2], "_light", "255 0 0 200").unwrap();
    for revision in 1..=200 {
        let mut parsed = hp_vmf::parse(&canonical.to_vmf().unwrap()).unwrap();
        parsed.entities.iter_mut().find(|entity| entity.id == 3).unwrap().set("targetname", revision.to_string());
        let next = Snapshot::from_vmf(&hp_vmf::write(&parsed)).unwrap();
        controller
            .process(
                &mut state,
                Event::Applied { author: 2, sequence: revision, revision, changes: canonical.diff(&next) },
            )
            .unwrap();
        canonical = next;
        assert!(state.doc.in_transaction());
        assert!(controller.deferred.is_empty());
    }
    state.doc.commit();
    controller.finish_transaction(&mut state).unwrap();
    let current = Controller::current(&state).unwrap();
    assert_eq!(current.0["entity:3"], canonical.0["entity:3"]);
    assert!(current.0["entity:2"].vmf.contains("255 0 0 200"));
    assert!(controller.conflict.is_none());
    assert_eq!(controller.last_content_serial, 0);
}

#[test]
fn acknowledgement_arriving_during_a_new_drag_uses_the_submitted_local_baseline() {
    let (mut controller, mut state, before) = fixture();
    state.doc.set_entity_key(&[2], "_light", "255 0 0 200").unwrap();
    let submitted = Controller::current(&state).unwrap();
    controller.submitted =
        Some(Submitted { sequence: 1, before: before.clone(), after: submitted.clone(), history: 0 });
    state.doc.begin("next drag");
    state.doc.set_entity_key(&[2], "_light", "0 255 0 200").unwrap();
    let newer = Controller::current(&state).unwrap();
    controller
        .process(&mut state, Event::Applied { author: 1, sequence: 1, revision: 1, changes: before.diff(&submitted) })
        .unwrap();
    assert!(state.doc.in_transaction());
    state.doc.commit();
    controller.finish_transaction(&mut state).unwrap();
    assert_eq!(Controller::current(&state).unwrap(), newer);
    assert!(controller.conflict.is_none());
    assert_eq!(controller.undo.len(), 1);
    assert_eq!(controller.last_content_serial, 0);
}

fn synchronize(
    host: &mut Controller,
    host_state: &mut AppState,
    guest: &mut Controller,
    guest_state: &mut AppState,
    ready: impl Fn(&Controller, &Controller) -> bool,
) {
    let context = egui::Context::default();
    let started = Instant::now();
    loop {
        host.tick(host_state, &context, 1);
        guest.tick(guest_state, &context, 2);
        if ready(host, guest) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "editor synchronization timed out: host {}, guest {}",
            host.status,
            guest.status
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn two_editor_documents_converge_over_real_transport_and_keep_personal_undo_and_paths() {
    let (_, mut host_state, map) = fixture();
    host_state.doc.set_path(Some("host-only.vmf".into()));
    let password = Zeroizing::new("editor-integration-password".to_owned());
    let session = Session::host(
        map,
        "Host".into(),
        password.clone(),
        hp_collab::transport::HostOptions {
            room: hp_collab::Room { name: "Editor QA".into() },
            mode: hp_collab::ConnectionMode::Direct,
            port: 0,
        },
    )
    .unwrap();
    let mut host = Controller { session: Some(session), host: true, bound_tab: 1, ..Default::default() };
    let mut guest = Controller::default();
    let mut guest_state = hp_app::testing::test_state();
    synchronize(&mut host, &mut host_state, &mut guest, &mut guest_state, |h, _| !h.invitation.is_empty());
    let port = host.host_addresses.iter().find(|address| address.is_ipv4()).unwrap().port();
    guest.session = Some(
        Session::join(
            Invitation::decode(&host.invitation).unwrap(),
            "Guest".into(),
            password,
            hp_collab::transport::JoinOptions {
                identity: Zeroizing::new(hp_collab::protocol::random_secret().unwrap()),
                session_name: "Editor QA".into(),
                direct_address: Some(([127, 0, 0, 1], port).into()),
            },
        )
        .unwrap(),
    );
    guest.bound_tab = 2;
    synchronize(&mut host, &mut host_state, &mut guest, &mut guest_state, |h, g| {
        g.canonical.is_some() && h.canonical == g.canonical
    });
    host_state.doc.set_entity_key(&[2], "_light", "255 0 0 200").unwrap();
    guest_state.doc.set_entity_key(&[3], "targetname", "guest-light").unwrap();
    synchronize(&mut host, &mut host_state, &mut guest, &mut guest_state, |h, g| {
        h.undo.len() == 1
            && g.undo.len() == 1
            && h.submitted.is_none()
            && g.submitted.is_none()
            && h.canonical == g.canonical
    });
    assert_eq!(Controller::current(&host_state).unwrap(), Controller::current(&guest_state).unwrap());
    assert_eq!(host_state.doc.path().unwrap().to_str(), Some("host-only.vmf"));
    assert!(guest_state.doc.path().is_none());
    assert!(guest.history(&mut guest_state, Action::Undo));
    synchronize(&mut host, &mut host_state, &mut guest, &mut guest_state, |h, g| {
        g.redo.len() == 1 && g.submitted.is_none() && h.canonical == g.canonical
    });
    assert_eq!(host_state.doc.scene().entity(2).unwrap().entity.get("_light"), Some("255 0 0 200"));
    assert!(host_state.doc.scene().entity(3).unwrap().entity.get("targetname").is_none());
    assert!(host.conflict.is_none() && guest.conflict.is_none());
}
