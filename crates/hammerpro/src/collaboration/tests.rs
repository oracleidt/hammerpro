use std::time::{Duration, Instant};

use hp_app::Action;
use hp_app::runtime::collaboration::CameraMode;
use hp_collab::Snapshot;

use super::*;

const PASSWORD: &str = "editor-integration-password";

fn base_map() -> hp_vmf::Map {
    hp_vmf::parse("world {\"id\" \"1\" \"classname\" \"worldspawn\"} entity {\"id\" \"2\" \"classname\" \"light\" \"_light\" \"255 255 255 200\" \"origin\" \"0 0 0\"} entity {\"id\" \"3\" \"classname\" \"light\" \"origin\" \"64 0 0\"}").unwrap()
}

fn cube(min: glam::DVec3, size: f64) -> Vec<hp_vmf::Side> {
    let max = min + glam::DVec3::splat(size);
    let (x0, y0, z0, x1, y1, z1) = (min.x, min.y, min.z, max.x, max.y, max.z);
    let v = glam::DVec3::new;
    let floor = (glam::DVec3::X, glam::DVec3::NEG_Y);
    let wall_y = (glam::DVec3::X, glam::DVec3::NEG_Z);
    let wall_x = (glam::DVec3::Y, glam::DVec3::NEG_Z);
    [
        ([v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)], floor),
        ([v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)], floor),
        ([v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)], wall_y),
        ([v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)], wall_y),
        ([v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)], wall_x),
        ([v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)], wall_x),
    ]
    .iter()
    .map(|(plane, (u, w))| hp_vmf::Side {
        plane: *plane,
        material: "DEV/DEV_MEASUREGENERIC01".into(),
        uaxis: hp_vmf::TexAxis { axis: *u, shift: 0.0, scale: 0.25 },
        vaxis: hp_vmf::TexAxis { axis: *w, shift: 0.0, scale: 0.25 },
        ..Default::default()
    })
    .collect()
}

fn full(state: &AppState) -> Snapshot {
    let mut map = state.doc.to_map();
    map.format = hp_vmf::TextFormat::default();
    Snapshot::from_vmf(&hp_vmf::write(&map)).unwrap()
}

struct Pair {
    start_ui: Duration,
    join_ui: Duration,
    host: Controller,
    host_state: AppState,
    guest: Controller,
    guest_state: AppState,
    context: egui::Context,
}

impl Pair {
    fn new(map: hp_vmf::Map) -> Self {
        Self::with_host(map, |_| {})
    }

    fn with_host(map: hp_vmf::Map, configure: impl FnOnce(&mut AppState)) -> Self {
        let mut host_state = hp_app::testing::test_state();
        configure(&mut host_state);
        host_state.doc = hp_editor::Document::from_map(map);
        host_state.doc.set_path(Some("host-only.vmf".into()));
        let mut host = Controller {
            name: "Host".into(),
            session_name: "Editor QA".into(),
            connection_mode: hp_collab::ConnectionMode::Direct,
            password: Zeroizing::new(PASSWORD.into()),
            ..Default::default()
        };
        let started = Instant::now();
        host.start_host(&mut host_state, 1, false);
        let start_ui = started.elapsed();
        assert!(host.active(), "{}", host.status);
        let mut pair = Pair {
            start_ui,
            join_ui: Duration::ZERO,
            host,
            host_state,
            guest: Controller::default(),
            guest_state: hp_app::testing::test_state(),
            context: egui::Context::default(),
        };
        pair.run(|pair| !pair.host.invitation.is_empty() && pair.host.canonical.is_some());
        let port = pair.host.host_addresses.iter().find(|address| address.is_ipv4()).unwrap().port();
        pair.guest.name = "Guest".into();
        pair.guest.session_name = "Editor QA".into();
        pair.guest.password = Zeroizing::new(PASSWORD.into());
        pair.guest.connection_mode = hp_collab::ConnectionMode::Direct;
        pair.guest.direct_address = format!("127.0.0.1:{port}");
        pair.guest.invitation_input = Zeroizing::new(pair.host.invitation.to_string());
        pair.guest.join_session(false).unwrap();
        pair.guest.bound_tab = 2;
        let started = Instant::now();
        let mut join_ui = Duration::ZERO;
        loop {
            pair.host.tick(&mut pair.host_state, &pair.context, 1);
            let tick = Instant::now();
            pair.guest.tick(&mut pair.guest_state, &pair.context, 2);
            join_ui = join_ui.max(tick.elapsed());
            if pair.guest.canonical.is_some() && pair.guest.loading.is_none() && pair.converged() {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(600), "join timed out: {}", pair.guest.status);
            std::thread::sleep(Duration::from_millis(2));
        }
        pair.join_ui = join_ui;
        pair
    }

    fn tick(&mut self) {
        self.host.tick(&mut self.host_state, &self.context, 1);
        self.guest.tick(&mut self.guest_state, &self.context, 2);
    }

    fn run(&mut self, ready: impl Fn(&Pair) -> bool) {
        let started = Instant::now();
        loop {
            self.tick();
            if ready(self) {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "synchronization timed out: host {}, guest {}",
                self.host.status,
                self.guest.status
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn converged(&self) -> bool {
        self.host.canonical.is_some()
            && self.host.canonical == self.guest.canonical
            && self.host.pending.is_none()
            && self.guest.pending.is_none()
            && self.host.revision == self.guest.revision
    }

    fn settle(&mut self) {
        self.run(|pair| pair.converged() && pair.idle());
        for _ in 0..3 {
            self.tick();
        }
        self.run(|pair| pair.converged() && pair.idle());
    }

    fn wait_presence(&mut self) {
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(400) {
            self.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn idle(&self) -> bool {
        self.host.last_content_serial == self.host_state.doc.content_serial()
            && self.guest.last_content_serial == self.guest_state.doc.content_serial()
    }

    fn documents_match(&self) {
        let canonical = self.host.canonical.as_ref().unwrap().snapshot().clone();
        let host = full(&self.host_state);
        let guest = full(&self.guest_state);
        for (key, unit) in &canonical.0 {
            if key == "world" {
                continue;
            }
            assert_eq!(host.0.get(key), Some(unit), "host {key}");
            assert_eq!(guest.0.get(key), Some(unit), "guest {key}");
        }
        assert_eq!(host.0.len(), canonical.0.len());
        assert_eq!(guest.0.len(), canonical.0.len());
    }
}

#[test]
fn incremental_capture_matches_a_full_capture_for_every_edit_kind() {
    let mut state = hp_app::testing::test_state();
    state.doc = hp_editor::Document::from_map(base_map());
    let start = state.doc.content_serial();
    let doc = &mut state.doc;
    let solid = doc.create_solid(cube(glam::DVec3::ZERO, 64.0), hp_editor::Owner::World).unwrap();
    let other = doc.create_solid(cube(glam::DVec3::splat(128.0), 32.0), hp_editor::Owner::World).unwrap();
    doc.translate(&[hp_editor::ObjRef::Solid(solid)], glam::DVec3::new(16.0, 0.0, 0.0)).unwrap();
    doc.set_entity_key(&[2], "_light", "255 0 0 200").unwrap();
    let entity = doc.tie_to_entity(&[hp_editor::ObjRef::Solid(other)], "func_detail", &[]).unwrap();
    doc.group(&[hp_editor::ObjRef::Solid(solid), hp_editor::ObjRef::Entity(3)]).unwrap();
    doc.create_visgroup("Team", None, &[hp_editor::ObjRef::Solid(solid)]).unwrap();
    doc.set_world_key("skyname", "sky_day01_05").unwrap();
    doc.set_hidden(&[hp_editor::ObjRef::Entity(entity)], true);
    doc.delete(&[hp_editor::ObjRef::Entity(3)]).unwrap();
    let touched = state.doc.shared_touched_since(start).unwrap();
    let incremental = Controller::local_units(&state, &touched).unwrap();
    let current = full(&state);
    assert!(incremental.len() >= 6);
    for (key, unit) in &incremental {
        assert_eq!(current.0.get(key), unit.as_ref(), "{key}");
    }
    let keys: BTreeSet<&String> = incremental.iter().map(|(key, _)| key).collect();
    let original = Snapshot::from_vmf(&hp_vmf::write(&base_map())).unwrap();
    for change in original.diff(&current) {
        assert!(keys.contains(&change.key), "missed {}", change.key);
    }
}

#[test]
fn two_editors_converge_for_every_edit_kind_with_personal_undo() {
    let mut pair = Pair::new(base_map());
    pair.host_state.doc.set_entity_key(&[2], "_light", "255 0 0 200").unwrap();
    pair.guest_state.doc.set_entity_key(&[3], "targetname", "guest-light").unwrap();
    pair.settle();
    assert_eq!(pair.host.undo.len(), 1);
    assert_eq!(pair.guest.undo.len(), 1);
    let solid = pair.guest_state.doc.create_solid(cube(glam::DVec3::ZERO, 64.0), hp_editor::Owner::World).unwrap();
    pair.settle();
    assert!(pair.host_state.doc.scene().solid(solid).is_some());
    assert!((pair.guest.id_start..pair.guest.id_start + hp_collab::document::ID_STRIDE).contains(&solid));
    pair.guest_state.doc.translate(&[hp_editor::ObjRef::Solid(solid)], glam::DVec3::new(32.0, 0.0, 0.0)).unwrap();
    pair.settle();
    let host_solid = pair.host_state.doc.scene().solid(solid).unwrap();
    assert!(host_solid.bounds.min.x >= 31.9);
    let entity = pair.host_state.doc.tie_to_entity(&[hp_editor::ObjRef::Solid(solid)], "func_detail", &[]).unwrap();
    pair.settle();
    assert!(pair.guest_state.doc.scene().is_brush_entity(entity));
    pair.host_state.doc.clear_selection();
    pair.wait_presence();
    pair.guest_state.doc.group(&[hp_editor::ObjRef::Entity(entity), hp_editor::ObjRef::Entity(2)]).unwrap();
    pair.guest_state.doc.create_visgroup("Shared", None, &[hp_editor::ObjRef::Entity(2)]).unwrap();
    pair.guest_state.doc.set_world_key("skyname", "sky_day02_01").unwrap();
    pair.settle();
    assert_eq!(pair.host_state.doc.scene().groups().len(), 1);
    assert_eq!(pair.host_state.doc.scene().visgroups().len(), 1);
    assert_eq!(pair.host_state.doc.scene().world().get("skyname"), Some("sky_day02_01"));
    pair.host_state.doc.set_hidden(&[hp_editor::ObjRef::Entity(3)], true);
    pair.settle();
    assert!(pair.guest_state.doc.scene().entity(3).unwrap().entity.hidden);
    pair.documents_match();
    assert_eq!(pair.host_state.doc.path().unwrap().to_str(), Some("host-only.vmf"));
    assert!(pair.guest_state.doc.path().is_none());
    pair.host_state.doc.select(&[hp_editor::ObjRef::Entity(2)], hp_editor::SelectOp::Replace);
    pair.wait_presence();
    let guest_edits = pair.guest.undo.len();
    assert!(pair.guest.history(&mut pair.guest_state, Action::Undo));
    pair.settle();
    assert_eq!(pair.guest.undo.len(), guest_edits, "an undo refused by a selection lock can be tried again");
    assert_eq!(pair.host_state.doc.scene().world().get("skyname"), Some("sky_day02_01"));
    pair.host_state.doc.clear_selection();
    pair.wait_presence();
    assert!(pair.guest.history(&mut pair.guest_state, Action::Undo));
    pair.settle();
    assert_eq!(pair.guest.undo.len(), guest_edits - 1);
    assert_eq!(pair.host_state.doc.scene().world().get("skyname"), None);
    assert!(pair.guest.history(&mut pair.guest_state, Action::Redo));
    pair.settle();
    assert_eq!(pair.host_state.doc.scene().world().get("skyname"), Some("sky_day02_01"));
    pair.host_state.doc.clear_selection();
    pair.guest_state.doc.clear_selection();
    pair.wait_presence();
    pair.guest_state.doc.delete(&[hp_editor::ObjRef::Entity(entity)]).unwrap();
    pair.settle();
    assert!(pair.host_state.doc.scene().solid(solid).is_none());
    pair.documents_match();
    assert!(pair.host.conflict.is_empty() && pair.guest.conflict.is_empty());
}

#[test]
fn hiding_and_showing_is_shared_for_every_object_kind_with_author_undo() {
    let mut pair = Pair::new(base_map());
    let world_brush = pair.host_state.doc.create_solid(cube(glam::DVec3::ZERO, 64.0), hp_editor::Owner::World).unwrap();
    let detail =
        pair.host_state.doc.create_solid(cube(glam::DVec3::splat(128.0), 32.0), hp_editor::Owner::World).unwrap();
    let brush_entity =
        pair.host_state.doc.tie_to_entity(&[hp_editor::ObjRef::Solid(detail)], "func_detail", &[]).unwrap();
    let grouped =
        pair.host_state.doc.create_solid(cube(glam::DVec3::splat(-256.0), 32.0), hp_editor::Owner::World).unwrap();
    let group = pair.host_state.doc.group(&[hp_editor::ObjRef::Solid(grouped), hp_editor::ObjRef::Entity(3)]).unwrap();
    let visgroup = pair.host_state.doc.create_visgroup("Team", None, &[hp_editor::ObjRef::Solid(world_brush)]).unwrap();
    pair.settle();
    let objects = [
        hp_editor::ObjRef::Entity(2),
        hp_editor::ObjRef::Entity(brush_entity),
        hp_editor::ObjRef::Solid(world_brush),
        hp_editor::ObjRef::Solid(grouped),
        hp_editor::ObjRef::Entity(3),
    ];
    for object in objects {
        pair.guest_state.doc.take_changes();
        assert!(pair.guest_state.doc.is_visible(object));
        pair.host_state.doc.set_hidden(&[object], true);
        pair.settle();
        assert!(!pair.guest_state.doc.is_visible(object), "{object:?} hidden for the guest");
        assert!(pair.guest_state.doc.pending_changes().visibility_changed, "{object:?} redraws the guest views");
        pair.guest_state.doc.take_changes();
        assert!(pair.host.history(&mut pair.host_state, Action::Undo));
        pair.settle();
        assert!(pair.guest_state.doc.is_visible(object), "{object:?} shown again by the author undo");
        assert!(pair.guest_state.doc.pending_changes().visibility_changed);
        assert!(pair.host.history(&mut pair.host_state, Action::Redo));
        pair.settle();
        assert!(!pair.guest_state.doc.is_visible(object), "{object:?} hidden again by the author redo");
        pair.host_state.doc.set_hidden(&[object], false);
        pair.settle();
        assert!(pair.guest_state.doc.is_visible(object));
    }
    let _ = group;
    pair.guest_state.doc.take_changes();
    pair.host_state.doc.set_visgroup_visible(visgroup, false).unwrap();
    pair.settle();
    assert!(!pair.guest_state.doc.is_visible(hp_editor::ObjRef::Solid(world_brush)), "visgroup hide is shared");
    assert!(pair.guest_state.doc.pending_changes().visibility_changed);
    pair.host_state.doc.set_visgroup_visible(visgroup, true).unwrap();
    pair.settle();
    assert!(pair.guest_state.doc.is_visible(hp_editor::ObjRef::Solid(world_brush)));
    pair.documents_match();
}

#[test]
fn concurrent_edits_of_one_object_keep_one_version_and_the_other_in_recovery() {
    let mut pair = Pair::new(base_map());
    pair.host_state.doc.set_entity_key(&[2], "targetname", "host_version").unwrap();
    pair.guest_state.doc.set_entity_key(&[2], "targetname", "guest_version").unwrap();
    pair.guest_state.doc.set_entity_key(&[3], "targetname", "guest_only").unwrap();
    pair.settle();
    pair.documents_match();
    let name = pair.host_state.doc.scene().entity(2).unwrap().entity.get("targetname").map(str::to_owned);
    assert_eq!(pair.guest_state.doc.scene().entity(2).unwrap().entity.get("targetname"), name.as_deref());
    let recovered = pair.host.conflict.len() + pair.guest.conflict.len();
    assert_eq!(recovered, 1, "one side keeps its version in the recovery copy");
    assert_eq!(pair.host_state.doc.scene().entity(3).unwrap().entity.get("targetname"), Some("guest_only"));
}

#[test]
fn live_strokes_stream_coalesced_states_and_undo_as_one_step() {
    let mut pair = Pair::new(base_map());
    let solid = pair.guest_state.doc.create_solid(cube(glam::DVec3::ZERO, 64.0), hp_editor::Owner::World).unwrap();
    pair.settle();
    let edits = pair.guest.undo.len();
    let sent = pair.guest.stats.edits_sent;
    pair.guest_state.doc.begin("Sculpt stroke");
    let mut seen_live = false;
    for step in 0..40 {
        pair.guest_state.doc.translate(&[hp_editor::ObjRef::Solid(solid)], glam::DVec3::new(0.0, 0.0, 1.0)).unwrap();
        pair.tick();
        std::thread::sleep(Duration::from_millis(5));
        pair.tick();
        if step > 10 {
            seen_live |= pair.host_state.doc.scene().solid(solid).unwrap().bounds.min.z > 0.5;
        }
    }
    assert!(seen_live, "the host saw the stroke before it ended");
    pair.guest_state.doc.commit();
    pair.settle();
    assert_eq!(pair.host_state.doc.scene().solid(solid).unwrap().bounds.min.z, 40.0);
    let live = pair.guest.stats.live_edits;
    assert!(live > 0 && pair.guest.stats.edits_sent - sent < 40, "{live} live edits");
    assert_eq!(pair.guest.undo.len(), edits + 1, "the whole stroke is one personal undo step");
    assert!(pair.guest.history(&mut pair.guest_state, Action::Undo));
    pair.settle();
    assert_eq!(pair.host_state.doc.scene().solid(solid).unwrap().bounds.min.z, 0.0);
    pair.documents_match();
}

#[test]
fn rapid_edits_while_waiting_are_coalesced_without_losing_the_final_state() {
    let mut pair = Pair::new(base_map());
    let sent = pair.guest.stats.edits_sent;
    for step in 0..60 {
        pair.guest_state.doc.set_entity_key(&[3], "targetname", &format!("name_{step}")).unwrap();
        pair.guest.tick(&mut pair.guest_state, &pair.context, 2);
    }
    pair.settle();
    assert_eq!(pair.host_state.doc.scene().entity(3).unwrap().entity.get("targetname"), Some("name_59"));
    assert!(pair.guest.stats.edits_sent - sent < 60);
    pair.documents_match();
}

#[test]
fn remote_edits_apply_during_a_local_transaction_except_its_own_objects() {
    let mut pair = Pair::new(base_map());
    pair.guest_state.settings.workflow.team_live_edits = false;
    pair.guest_state.doc.begin("Long drag");
    pair.guest_state.doc.set_entity_key(&[2], "targetname", "guest_drag").unwrap();
    pair.host_state.doc.set_entity_key(&[3], "targetname", "host_unrelated").unwrap();
    pair.host_state.doc.set_entity_key(&[2], "_light", "1 2 3 4").unwrap();
    pair.run(|pair| pair.guest.revision >= 1);
    assert!(pair.guest_state.doc.in_transaction());
    assert_eq!(pair.guest_state.doc.scene().entity(3).unwrap().entity.get("targetname"), Some("host_unrelated"));
    assert!(pair.guest.held.contains("entity:2"));
    pair.guest_state.doc.commit();
    pair.settle();
    pair.documents_match();
    assert_eq!(pair.guest_state.doc.scene().entity(2).unwrap().entity.get("_light"), Some("1 2 3 4"));
    assert!(pair.guest.conflict.contains_key("entity:2"));
}

#[test]
fn large_remote_pastes_are_validated_and_decoded_in_the_background() {
    let mut pair = Pair::new(base_map());
    pair.guest_state.doc.begin("Paste");
    let mut ids = Vec::new();
    for index in 0..300 {
        let min = glam::DVec3::new((index % 20) as f64 * 80.0, (index / 20) as f64 * 80.0, 0.0);
        ids.push(pair.guest_state.doc.create_solid(cube(min, 64.0), hp_editor::Owner::World).unwrap());
    }
    pair.guest_state.doc.commit();
    let mut background = false;
    let mut captured = false;
    let started = Instant::now();
    loop {
        pair.tick();
        background |= pair.host.applying.is_some();
        captured |= pair.guest.capturing.is_some();
        if pair.converged() && pair.host_state.doc.scene().solids().len() == 300 {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(background, "the host applied the paste in the background");
    assert!(captured, "the guest captured the paste in the background");
    assert_eq!(pair.guest.undo.len(), 1, "the paste is one personal undo step");
    pair.settle();
    pair.documents_match();
    assert!(ids.iter().all(|id| pair.host_state.doc.scene().solid(*id).is_some()));
}

#[test]
fn refused_values_stay_local_with_a_precise_report_and_refused_edits_restore_the_shared_version() {
    let _english = english();
    let mut pair = Pair::new(base_map());
    let script = pair.guest_state.doc.create_point_entity("point_servercommand", glam::DVec3::ZERO, &[]).unwrap();
    pair.guest_state.doc.set_entity_key(&[2], "model", "C:\\private\\x.mdl").unwrap();
    pair.guest_state.doc.set_entity_key(&[2], "targetname", "shared_name").unwrap();
    pair.settle();
    assert!(pair.host_state.doc.scene().entity(script).is_some(), "script entities are shared by default");
    assert!(logged(
        &pair.host_state,
        &format!("Guest added or changed a script or command entity: point_servercommand (entity:{script})")
    ));
    assert_eq!(pair.host_state.doc.scene().entity(2).unwrap().entity.get("targetname"), Some("shared_name"));
    assert_eq!(pair.host_state.doc.scene().entity(2).unwrap().entity.get("model"), None);
    assert!(!pair.guest.local_only.iter().any(|problem| problem.contains("point_servercommand")));
    assert!(pair.guest.local_only.iter().any(|problem| problem.contains("light 2") && problem.contains("model")));
    pair.host_state.doc.select(&[hp_editor::ObjRef::Entity(3)], hp_editor::SelectOp::Replace);
    pair.wait_presence();
    pair.guest_state.doc.set_entity_key(&[3], "targetname", "blocked").unwrap();
    pair.settle();
    assert_eq!(pair.guest_state.doc.scene().entity(3).unwrap().entity.get("targetname"), None);
    assert!(pair.guest.conflict.contains_key("entity:3"));
    pair.host_state.doc.clear_selection();
    pair.wait_presence();
    pair.guest_state.doc.set_entity_key(&[3], "targetname", "after").unwrap();
    pair.settle();
    assert_eq!(pair.host_state.doc.scene().entity(3).unwrap().entity.get("targetname"), Some("after"));
}

#[test]
fn teleport_matches_the_contributor_camera_without_editing_the_map() {
    let mut state = hp_app::testing::test_state();
    state.doc = hp_editor::Document::from_map(base_map());
    let before = full(&state);
    state.ui.maximized_view = Some(1);
    let zoom = state.cameras.top.zoom;
    let position = [128.0, -256.0, 64.0];
    let direction = glam::DVec3::new(1.0, 2.0, 0.5).normalize();
    teleport_camera(&mut state, position, direction.to_array());
    assert_eq!(state.cameras.cam3d.pos.to_array(), position);
    assert!(state.cameras.cam3d.forward().abs_diff_eq(direction, 1e-9));
    assert_eq!(state.cameras.top.center.to_array(), [128.0, -256.0]);
    assert_eq!(state.cameras.front.center.to_array(), [-256.0, 64.0]);
    assert_eq!(state.cameras.side.center.to_array(), [128.0, 64.0]);
    assert_eq!(state.cameras.top.zoom, zoom);
    assert_eq!(state.ui.maximized_view, Some(0));
    assert_eq!(state.ui.focused_view, Some(0));
    assert_eq!(full(&state), before);
    let camera = state.cameras.cam3d;
    teleport_camera(&mut state, [f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0]);
    assert_eq!(state.cameras.cam3d, camera);
    teleport_camera(&mut state, position, [0.0; 3]);
    assert_eq!(state.cameras.cam3d, camera);
}

#[test]
fn mcp_requests_host_and_leave_through_the_runtime() {
    let mut state = hp_app::testing::test_state();
    state.doc = hp_editor::Document::from_map(base_map());
    let context = egui::Context::default();
    let mut controller = Controller::default();
    state.runtime.collaboration.requests.push(hp_app::runtime::collaboration::CollaborationRequest::Host {
        session: "MCP QA".into(),
        nickname: "Bot".into(),
        password: PASSWORD.into(),
        direct: true,
        port: 0,
    });
    assert!(!controller.ui(&mut state, &context, 7, false));
    assert_eq!(controller.take_host_request(), Some(7));
    controller.port = 0;
    controller.start_host(&mut state, 7, false);
    let started = Instant::now();
    while state.runtime.collaboration.stats.invitation.is_empty() {
        controller.tick(&mut state, &context, 7);
        assert!(started.elapsed() < Duration::from_secs(20), "{}", controller.status);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(state.runtime.collaboration.stats.host && state.runtime.collaboration.connected);
    assert_eq!(state.runtime.collaboration.stats.session, "MCP QA");
    state.runtime.collaboration.requests.push(hp_app::runtime::collaboration::CollaborationRequest::Leave);
    controller.ui(&mut state, &context, 7, false);
    assert!(!controller.active());
    assert!(state.runtime.collaboration.stats.session.is_empty());
}

fn english() -> std::sync::MutexGuard<'static, ()> {
    hp_app::i18n::register(TR);
    hp_app::i18n::register(access::TR);
    hp_app::testing::lang_lock()
}

fn logged(state: &AppState, text: &str) -> bool {
    state.console.lines().any(|line| line.text.contains(text))
}

impl Pair {
    fn wait(&mut self, description: &str, ready: impl Fn(&Pair) -> bool) {
        let started = Instant::now();
        loop {
            self.tick();
            if ready(self) {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "{description}: host {}, guest {}",
                self.host.status,
                self.guest.status
            );
            std::thread::sleep(Duration::from_millis(3));
        }
    }
}

#[test]
fn chat_lines_and_ping_markers_reach_the_others_with_the_author_colour() {
    let _english = english();
    let mut pair = Pair::new(base_map());
    pair.wait_presence();
    let host_color = pair.guest_state.runtime.collaboration.color_of(1);
    let guest_color = pair.host_state.runtime.collaboration.color_of(pair.guest.participant);
    assert_ne!(host_color, guest_color, "participants get distinct colours");
    assert_eq!(pair.host_state.runtime.collaboration.color_of(1), host_color, "everyone sees the same colours");
    pair.host.send_note(&mut pair.host_state, "  meet at the door\u{7}  ", None).unwrap();
    pair.guest.send_note(&mut pair.guest_state, "", Some([64.0, 32.0, 0.0])).unwrap();
    for index in 0..5 {
        pair.guest.send_note(&mut pair.guest_state, &format!("burst {index}"), None).unwrap();
    }
    pair.wait("chat delivered", |pair| {
        pair.guest_state.runtime.collaboration.chat.iter().any(|line| line.text == "meet at the door")
            && pair.host_state.runtime.collaboration.chat.iter().filter(|line| !line.own).count() == 6
    });
    let guest = &pair.guest_state.runtime.collaboration;
    let line = guest.chat.iter().find(|line| !line.own).unwrap();
    assert_eq!((line.name.as_str(), line.color, line.author), ("Host", host_color, 1));
    assert_eq!(guest.unread, 1);
    assert!(logged(&pair.guest_state, "Host: meet at the door"));
    let host = &pair.host_state.runtime.collaboration;
    let texts: Vec<&str> = host.chat.iter().filter(|line| !line.own).map(|line| line.text.as_str()).collect();
    assert_eq!(
        texts,
        ["", "burst 0", "burst 1", "burst 2", "burst 3", "burst 4"],
        "every queued line arrives once, in order"
    );
    assert_eq!(host.markers.len(), 1);
    assert_eq!(host.markers[0].position, [64.0, 32.0, 0.0]);
    assert_eq!(host.markers[0].color, guest_color);
    assert!(logged(&pair.host_state, "Guest pinged a point"));
    for _ in 0..20 {
        pair.tick();
    }
    assert_eq!(pair.host_state.runtime.collaboration.chat.iter().filter(|line| !line.own).count(), 6, "no duplicates");
    assert!(pair.guest.send_note(&mut pair.guest_state, "   ", None).is_err());
    pair.documents_match();
}

#[test]
fn remote_edits_mark_the_author_as_editing_and_flash_the_objects_in_their_colour() {
    let mut pair = Pair::new(base_map());
    pair.wait_presence();
    pair.host_state.doc.set_entity_key(&[2], "targetname", "flash").unwrap();
    pair.settle();
    pair.tick();
    let runtime = &pair.guest_state.runtime.collaboration;
    let host = runtime.participant(1).unwrap();
    assert!(host.editing, "the host is shown as editing");
    let highlight = runtime.highlights.iter().find(|highlight| highlight.author == 1).unwrap();
    assert_eq!(highlight.keys, ["entity:2"]);
    assert_eq!(highlight.color, host.color);
    assert!(!highlight.conflict);
    let mut overlay = hp_render::Overlay::default();
    overlay::build(&pair.guest_state, &mut overlay);
    assert!(overlay.lines_on_top.len() >= 12, "the edited entity is outlined");
}

#[test]
fn shared_selections_report_locks_and_refusals_name_the_participant_holding_the_object() {
    let _english = english();
    let mut pair = Pair::new(base_map());
    pair.host_state.doc.select(&[hp_editor::ObjRef::Entity(3)], hp_editor::SelectOp::Replace);
    pair.guest_state
        .doc
        .select(&[hp_editor::ObjRef::Entity(3), hp_editor::ObjRef::Entity(2)], hp_editor::SelectOp::Replace);
    pair.wait("locks reported", |pair| !pair.guest_state.runtime.collaboration.locks.is_empty());
    let lock = pair.guest_state.runtime.collaboration.locks[0].clone();
    assert_eq!((lock.participant, lock.name.as_str(), lock.objects), (1, "Host", 1));
    let host = pair.guest_state.runtime.collaboration.participant(1).unwrap();
    assert!(host.selection_center.is_some(), "a label marks the selection of the host");
    pair.guest_state.doc.set_entity_key(&[3], "targetname", "blocked").unwrap();
    pair.settle();
    assert!(pair.guest.status.contains("Host"), "{}", pair.guest.status);
    assert!(pair.guest.status.contains("light 3"), "{}", pair.guest.status);
    assert!(pair.guest.conflict.contains_key("entity:3"));
    assert!(
        pair.guest_state
            .runtime
            .collaboration
            .highlights
            .iter()
            .any(|highlight| highlight.conflict && highlight.keys == ["entity:3"])
    );
    pair.host_state.doc.clear_selection();
    pair.wait("lock released", |pair| pair.guest_state.runtime.collaboration.locks.is_empty());
}

#[test]
fn concurrent_changes_name_the_participant_who_changed_the_object_first() {
    let _english = english();
    let mut pair = Pair::new(base_map());
    pair.wait("host known", |pair| pair.guest_state.runtime.collaboration.participant(1).is_some());
    pair.guest_state.settings.workflow.team_live_edits = false;
    pair.guest_state.doc.begin("Long drag");
    pair.guest_state.doc.set_entity_key(&[2], "targetname", "guest_drag").unwrap();
    pair.host_state.doc.set_entity_key(&[2], "_light", "1 2 3 4").unwrap();
    pair.run(|pair| pair.guest.revision >= 1);
    pair.guest_state.doc.commit();
    pair.settle();
    assert!(pair.guest.conflict.contains_key("entity:2"));
    assert!(pair.guest.status.contains("Host") && pair.guest.status.contains("light 2"), "{}", pair.guest.status);
    assert!(pair.guest_state.runtime.collaboration.highlights.iter().any(|highlight| highlight.conflict));
}

#[test]
fn following_tracks_the_other_camera_and_stops_when_the_user_moves() {
    let _english = english();
    let mut pair = Pair::new(base_map());
    let target = glam::DVec3::new(512.0, -256.0, 128.0);
    pair.host_state.cameras.cam3d.pos = target;
    pair.host_state.cameras.cam3d.look_at(target + glam::DVec3::new(0.0, 1.0, 0.0));
    pair.wait_presence();
    pair.guest.camera(&mut pair.guest_state, 1, CameraMode::Follow).unwrap();
    assert_eq!(pair.guest_state.runtime.collaboration.following, Some(1));
    assert_eq!(pair.guest.take_switch_request(), Some(2));
    pair.wait("camera reached", |pair| pair.guest_state.cameras.cam3d.pos.distance(target) < 0.01);
    assert!(pair.guest_state.cameras.cam3d.forward().distance(glam::DVec3::Y) < 1e-3);
    assert_eq!(pair.guest_state.cameras.top.center, target.truncate());
    let moved = target + glam::DVec3::new(256.0, 0.0, 0.0);
    pair.host_state.cameras.cam3d.pos = moved;
    pair.wait("camera follows", |pair| pair.guest_state.cameras.cam3d.pos.distance(moved) < 0.01);
    pair.guest_state.cameras.cam3d.pos.z += 64.0;
    pair.tick();
    assert_eq!(pair.guest_state.runtime.collaboration.following, None, "moving the camera stops following");
    assert!(logged(&pair.guest_state, "Stopped following Host"));
    let before = full(&pair.guest_state);
    pair.guest.camera(&mut pair.guest_state, 1, CameraMode::Selection).unwrap_err();
    pair.host_state.doc.select(&[hp_editor::ObjRef::Entity(3)], hp_editor::SelectOp::Replace);
    pair.wait_presence();
    pair.guest.camera(&mut pair.guest_state, 1, CameraMode::Selection).unwrap();
    pair.tick();
    let forward = pair.guest_state.cameras.cam3d.forward();
    let to_entity = (glam::DVec3::new(64.0, 0.0, 0.0) - pair.guest_state.cameras.cam3d.pos).normalize();
    assert!(forward.dot(to_entity) > 0.99, "the selection of the host is framed");
    assert_eq!(full(&pair.guest_state), before, "camera moves never edit the map");
    assert!(pair.guest.camera(&mut pair.guest_state, pair.guest.participant, CameraMode::Teleport).is_err());
}

#[test]
fn a_lost_connection_reconnects_in_the_same_tab_and_sends_the_offline_edits_again() {
    let mut pair = Pair::new(base_map());
    let id_start = pair.guest.id_start;
    let solid = pair.guest_state.doc.create_solid(cube(glam::DVec3::ZERO, 64.0), hp_editor::Owner::World).unwrap();
    pair.settle();
    pair.guest.process(&mut pair.guest_state, Event::Error("connection lost: timed out".into())).unwrap();
    pair.guest.process(&mut pair.guest_state, Event::Closed).unwrap();
    assert!(pair.guest.active() && pair.guest.session.is_none() && pair.guest.reconnect.is_some());
    assert!(!pair.guest_state.runtime.collaboration.connected);
    assert!(pair.guest_state.runtime.collaboration.reconnecting.is_some());
    assert!(!pair.guest.history(&mut pair.guest_state, Action::Undo), "local undo while offline");
    pair.guest_state.doc.set_entity_key(&[3], "targetname", "offline_name").unwrap();
    pair.guest_state.doc.translate(&[hp_editor::ObjRef::Solid(solid)], glam::DVec3::new(0.0, 0.0, 32.0)).unwrap();
    let created =
        pair.guest_state.doc.create_solid(cube(glam::DVec3::splat(256.0), 32.0), hp_editor::Owner::World).unwrap();
    pair.guest_state.doc.set_entity_key(&[2], "targetname", "guest_offline").unwrap();
    pair.host_state.doc.set_entity_key(&[2], "targetname", "host_meanwhile").unwrap();
    pair.host.tick(&mut pair.host_state, &pair.context, 1);
    pair.wait("reconnected", |pair| pair.guest.reconnect.is_none() && pair.guest.canonical.is_some());
    pair.settle();
    assert_eq!(pair.guest.id_start, id_start, "the same identifier range after reconnecting");
    assert_eq!(pair.guest.bound_tab, 2, "the same tab");
    assert_eq!(pair.host_state.doc.scene().entity(3).unwrap().entity.get("targetname"), Some("offline_name"));
    assert!(pair.host_state.doc.scene().solid(solid).unwrap().bounds.min.z >= 31.9);
    assert!(pair.host_state.doc.scene().solid(created).is_some(), "objects created offline are shared");
    assert_eq!(pair.host_state.doc.scene().entity(2).unwrap().entity.get("targetname"), Some("host_meanwhile"));
    assert_eq!(pair.guest_state.doc.scene().entity(2).unwrap().entity.get("targetname"), Some("host_meanwhile"));
    assert!(pair.guest.conflict.contains_key("entity:2"), "the conflicting offline edit is kept for recovery");
    assert_eq!(pair.guest.stats.reconnects, 1);
    assert!(pair.guest.stats.offline_resent >= 3);
    assert!(pair.guest_state.runtime.collaboration.reconnecting.is_none());
    pair.documents_match();
    assert!(pair.guest.history(&mut pair.guest_state, Action::Undo), "the offline edits are one personal undo step");
    pair.settle();
    assert!(pair.host_state.doc.scene().solid(created).is_none());
    assert_eq!(pair.host_state.doc.scene().entity(3).unwrap().entity.get("targetname"), None);
    pair.documents_match();
}

#[test]
fn resync_reconnects_a_working_guest_and_a_removed_guest_does_not_come_back() {
    let _english = english();
    let mut pair = Pair::new(base_map());
    pair.guest_state.doc.set_entity_key(&[3], "targetname", "before_resync").unwrap();
    pair.guest.retry_now(&mut pair.guest_state).unwrap();
    pair.wait("resynchronized", |pair| pair.guest.reconnect.is_none() && pair.guest.canonical.is_some());
    pair.settle();
    assert_eq!(pair.host_state.doc.scene().entity(3).unwrap().entity.get("targetname"), Some("before_resync"));
    pair.documents_match();
    assert!(pair.host.retry_now(&mut pair.host_state).is_err(), "the host is the reference copy");
    pair.wait_presence();
    let guest = pair.guest.participant;
    pair.host.session.as_ref().unwrap().send(Command::Kick(guest)).unwrap();
    pair.wait("removed", |pair| !pair.guest.active());
    assert!(pair.guest.reconnect.is_none());
    assert!(pair.guest.status.contains("Removed by the host"), "{}", pair.guest.status);
}

#[test]
fn reconnection_gives_up_after_its_attempts_and_can_be_retried_or_stopped() {
    let mut state = hp_app::testing::test_state();
    state.doc = hp_editor::Document::from_map(base_map());
    let context = egui::Context::default();
    let mut controller = Controller { name: "Guest".into(), session_name: "Gone".into(), ..Default::default() };
    controller.credentials = Some(reconnect::Credentials {
        invitation: Zeroizing::new("hpjoin1:invalid".into()),
        password: Zeroizing::new(PASSWORD.into()),
        address: String::new(),
    });
    controller.reconnect = Some(reconnect::Reconnect {
        attempt: 0,
        next: Some(Instant::now()),
        error: "connection lost: timed out".into(),
        base: None,
        serial: state.doc.content_serial(),
        keys: BTreeSet::new(),
    });
    controller.bound_tab = 4;
    controller.tick(&mut state, &context, 4);
    let reconnecting = state.runtime.collaboration.reconnecting.clone().unwrap();
    assert!(reconnecting.next.is_none() && !reconnecting.connecting, "an invalid invitation stops at once");
    assert!(controller.active(), "the panel keeps the session to try again");
    controller.retry_now(&mut state).unwrap();
    assert!(state.runtime.collaboration.reconnecting.as_ref().unwrap().next.is_some());
    state.runtime.collaboration.requests.push(CollaborationRequest::StopReconnecting);
    controller.ui(&mut state, &context, 4, false);
    assert!(!controller.active());
    assert!(state.runtime.collaboration.reconnecting.is_none());
}

fn grid_map(brushes: u32) -> hp_vmf::Map {
    let mut parsed = hp_vmf::parse(hp_app::testing::TEST_VMF).unwrap();
    let cube = parsed.world.solids[0].clone();
    parsed.world.solids.clear();
    parsed.entities.clear();
    for index in 0..brushes {
        let mut solid = cube.clone();
        solid.id = index + 100;
        for (offset, side) in solid.sides.iter_mut().enumerate() {
            side.id = 100_000 + index * 16 + offset as u32;
        }
        parsed.world.solids.push(solid);
    }
    parsed.entities.push(hp_vmf::Entity {
        id: 20,
        classname: "prop_dynamic".into(),
        props: vec![("origin".into(), "0 0 0".into()), ("model".into(), "models/props_c17/oildrum001.mdl".into())],
        ..Default::default()
    });
    parsed
}

fn activate_features(pair: &mut Pair) {
    let solids: Vec<u32> = pair.host_state.doc.scene().solids().keys().copied().collect();
    let pick = |skip: usize| -> Vec<hp_editor::ObjRef> {
        solids.iter().skip(skip).take(2048).map(|id| hp_editor::ObjRef::Solid(*id)).collect()
    };
    let half = solids.len() / 2;
    pair.host_state.doc.select(&pick(1), hp_editor::SelectOp::Replace);
    pair.guest_state.doc.select(&pick(half.max(2050)), hp_editor::SelectOp::Replace);
    pair.wait_presence();
    pair.guest.camera(&mut pair.guest_state, 1, CameraMode::Follow).unwrap();
}

fn profile(map: hp_vmf::Map, label: &str, features: bool) {
    let label =
        if features { format!("{label} (selections, follow, chat and pings active)") } else { label.to_owned() };
    let label = label.as_str();
    let started = Instant::now();
    let mut pair = Pair::new(map);
    let join = started.elapsed();
    if features {
        activate_features(&mut pair);
    }
    let mut worst_tick = Duration::ZERO;
    let mut latencies = Vec::new();
    let mut details = Vec::new();
    let solid = *pair.host_state.doc.scene().solids().keys().next().unwrap();
    let prop = pair.host_state.doc.scene().entities().keys().copied().next();
    for step in 0..6 {
        if features {
            pair.host.send_note(&mut pair.host_state, &format!("step {step}"), Some([0.0, 0.0, step as f64])).unwrap();
            pair.host_state.cameras.cam3d.pos.x += 16.0;
        }
        let moved = if let Some(prop) = prop.filter(|_| step % 2 == 0) {
            pair.host_state
                .doc
                .translate(&[hp_editor::ObjRef::Entity(prop)], glam::DVec3::new(16.0, 0.0, 0.0))
                .unwrap();
            hp_editor::ObjRef::Entity(prop)
        } else {
            pair.host_state
                .doc
                .translate(&[hp_editor::ObjRef::Solid(solid)], glam::DVec3::new(16.0, 0.0, 0.0))
                .unwrap();
            hp_editor::ObjRef::Solid(solid)
        };
        let edited = Instant::now();
        let target = pair.host.revision + 1;
        loop {
            let tick = Instant::now();
            pair.host.tick(&mut pair.host_state, &pair.context, 1);
            worst_tick = worst_tick.max(tick.elapsed());
            let tick = Instant::now();
            pair.guest.tick(&mut pair.guest_state, &pair.context, 2);
            worst_tick = worst_tick.max(tick.elapsed());
            if pair.guest.revision >= target {
                break;
            }
            assert!(edited.elapsed() < Duration::from_secs(60), "{label}: edit did not arrive");
        }
        latencies.push(edited.elapsed().as_secs_f64() * 1000.0);
        let _ = moved;
        pair.settle();
        details.push(format!(
            "{:.1}/{:.1}/{:.1}",
            pair.host.stats.round_trip_ms, pair.host.stats.capture_ms, pair.guest.stats.apply_ms
        ));
    }
    pair.documents_match();
    if features {
        pair.wait("chat lines delivered", |pair| pair.guest_state.runtime.collaboration.chat.len() >= 6);
        assert_eq!(
            pair.guest_state.runtime.collaboration.following,
            Some(1),
            "{label}: the guest still follows the host"
        );
        let overlay = Instant::now();
        let mut lines = 0;
        for _ in 0..10 {
            let mut out = hp_render::Overlay::default();
            overlay::build(&pair.guest_state, &mut out);
            lines = out.lines_on_top.len();
        }
        eprintln!(
            "{label}: guest overlay of the host selection rebuilt in {:.2} ms ({lines} lines), total traffic including the map transfer: host sent {} bytes, guest sent {} bytes",
            overlay.elapsed().as_secs_f64() * 100.0,
            pair.host.sent,
            pair.guest.sent
        );
    }
    eprintln!("{label}: host round trip, host capture and guest apply per edit in ms: {details:?}");
    eprintln!(
        "{label}: host start on the UI thread {:.1} ms, longest guest frame while joining {:.1} ms, join {:.0} ms, edit to remote screen {:?} ms, worst tick {:.1} ms, host capture max {:.2} ms, guest apply max {:.2} ms",
        pair.start_ui.as_secs_f64() * 1000.0,
        pair.join_ui.as_secs_f64() * 1000.0,
        join.as_secs_f64() * 1000.0,
        latencies.iter().map(|value| (value * 10.0).round() / 10.0).collect::<Vec<_>>(),
        worst_tick.as_secs_f64() * 1000.0,
        pair.host.stats.capture_max_ms,
        pair.guest.stats.apply_max_ms,
    );
}

#[test]
#[ignore = "profiles synchronization on a generated large map"]
fn large_map_prop_edit_synchronization_profile() {
    profile(grid_map(12_000), "12000 brushes", false);
    profile(grid_map(12_000), "12000 brushes", true);
}

#[test]
#[ignore = "profiles synchronization on the benchmark maps named by HP_TEAM_MAPS"]
fn benchmark_maps_synchronization_profile() {
    let paths = std::env::var("HP_TEAM_MAPS").unwrap_or_default();
    for path in paths.split(';').filter(|path| !path.is_empty()) {
        let map = hp_vmf::load(std::path::Path::new(path)).unwrap();
        profile(map.clone(), path, false);
        profile(map, path, true);
    }
}

fn overlay_views(state: &AppState) -> Vec<hp_app::view::ViewInfo> {
    let bounds = |entity: &hp_vmf::Entity| state.entity_bounds(entity);
    let all: Vec<hp_editor::ObjRef> =
        state.doc.scene().solids().keys().map(|id| hp_editor::ObjRef::Solid(*id)).collect();
    let aabb = state.doc.scene().objects_bounds(&all, &bounds).unwrap();
    let mut cameras = hp_app::state::Cameras::default();
    cameras.cam3d.frame_box(aabb.min, aabb.max);
    cameras.frame_2d(aabb.min, aabb.max, egui::vec2(800.0, 600.0));
    [hp_app::ViewKind::Perspective, hp_app::ViewKind::Top, hp_app::ViewKind::Front, hp_app::ViewKind::Side]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| hp_app::view::ViewInfo {
            index,
            kind,
            rect: egui::Rect::from_min_size(
                egui::pos2((index % 2) as f32 * 800.0, (index / 2) as f32 * 600.0),
                egui::vec2(800.0, 600.0),
            ),
            cam3d: cameras.cam3d,
            cam2d: cameras.cam2d(kind),
            pixels_per_point: 1.0,
            grid: 16.0,
        })
        .collect()
}

fn overlay_case(state: &mut AppState, peers: u32, per_peer: usize, label: &str) {
    use hp_app::runtime::collaboration::{Collaborator, Highlight, Lock, Marker};
    use hp_ui_viewports::layers::OverlayLayer;
    let solids: Vec<u32> = state.doc.scene().solids().keys().copied().collect();
    let runtime = &mut state.runtime.collaboration;
    runtime.connected = true;
    runtime.active_map = true;
    runtime.local_id = 1;
    runtime.participants = (1..=peers + 1)
        .map(|id| Collaborator {
            id,
            name: format!("P{id}"),
            color: hp_app::runtime::collaboration::PALETTE[(id as usize - 1) % 8],
            position: [id as f64 * 64.0, 0.0, 256.0],
            direction: [1.0, 0.0, 0.0],
            selected: if id == 1 {
                Vec::new()
            } else {
                (0..per_peer)
                    .map(|index| format!("solid:{}", solids[((id as usize - 2) * per_peer + index) % solids.len()]))
                    .collect()
            },
            selection_center: Some([0.0, 0.0, 0.0]),
            ..Collaborator::default()
        })
        .collect();
    runtime.highlights = (2..=peers.min(4) + 1)
        .map(|author| Highlight {
            author,
            label: format!("P{author}"),
            color: [255, 255, 255],
            keys: solids
                .iter()
                .skip(author as usize * 600)
                .take(per_peer.min(512))
                .map(|id| format!("solid:{id}"))
                .collect(),
            until: Instant::now() + Duration::from_secs(60),
            conflict: author == 2,
        })
        .collect();
    runtime.markers = (2..=peers.min(3) + 1)
        .map(|author| Marker {
            author,
            name: format!("P{author}"),
            color: [255, 0, 0],
            position: [0.0, 0.0, 0.0],
            at: Instant::now(),
        })
        .collect();
    runtime.locks = vec![Lock { participant: 2, name: "P2".into(), color: [0, 255, 0], objects: 3 }];
    let views = overlay_views(state);
    let layer = TeamLayer;
    let mut builds = Vec::new();
    let mut lines = 0;
    for _ in 0..12 {
        let mut out = hp_render::Overlay::default();
        let started = Instant::now();
        layer.retained(state, &views[0], &mut out);
        builds.push(started.elapsed().as_secs_f64() * 1000.0);
        lines = out.lines_on_top.len();
    }
    let context = egui::Context::default();
    let mut frames = Vec::new();
    let mut primitives = 0;
    for _ in 0..40 {
        let started = Instant::now();
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1200.0))),
                ..Default::default()
            },
            |ui| {
                let painter = ui.painter().clone();
                for view in &views {
                    let mut frame = hp_render::Overlay::default();
                    layer.add(state, view, &mut frame);
                    layer.paint(state, view, &painter);
                }
            },
        );
        primitives = context.tessellate(output.shapes, 1.0).len();
        frames.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let median = |values: &mut Vec<f64>| {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    };
    eprintln!(
        "{label}: {peers} participants x {per_peer} selected objects: retained overlay rebuilt in {:.2} ms (max {:.2} ms, {lines} GPU lines, done only when a selection, an edit highlight or the map changes); per frame for 4 views (cameras, labels, markers, egui tessellation) {:.3} ms median, {:.3} ms max, {primitives} egui primitives",
        median(&mut builds.clone()),
        builds.iter().copied().fold(0.0, f64::max),
        median(&mut frames.clone()),
        frames.iter().copied().fold(0.0, f64::max),
    );
}

#[test]
#[ignore = "profiles the Team Mapping overlay on a generated map and the benchmark maps named by HP_TEAM_MAPS"]
fn team_overlay_profile() {
    let mut maps = vec![("12000 brushes".to_owned(), grid_map(12_000))];
    for path in std::env::var("HP_TEAM_MAPS").unwrap_or_default().split(';').filter(|path| !path.is_empty()) {
        maps.push((path.to_owned(), hp_vmf::load(std::path::Path::new(path)).unwrap()));
    }
    if let Ok(path) = std::env::var("HP_OVERLAY_EXPORT") {
        std::fs::write(path, hp_vmf::write(&maps[0].1)).unwrap();
    }
    for (label, map) in maps {
        let mut state = hp_app::testing::test_state();
        state.doc = hp_editor::Document::from_map(map);
        overlay_case(&mut state, 1, 20, &label);
        overlay_case(&mut state, 1, 2048, &label);
        overlay_case(&mut state, 7, 2048, &label);
    }
}

fn scripted_map() -> hp_vmf::Map {
    hp_vmf::parse("world {\"id\" \"1\" \"classname\" \"worldspawn\"} entity {\"id\" \"2\" \"classname\" \"light\" \"origin\" \"0 0 0\"} entity {\"id\" \"4\" \"classname\" \"lua_run\" \"Code\" \"print(1)\" \"origin\" \"16 0 0\"} entity {\"id\" \"5\" \"classname\" \"logic_relay\" \"origin\" \"32 0 0\" connections {\"OnTrigger\" \"!self,RunScriptCode,x,0,-1\"}}").unwrap()
}

#[test]
fn maps_with_script_entities_are_hosted_and_their_changes_raise_a_notice() {
    let _english = english();
    let mut pair = Pair::new(scripted_map());
    assert!(pair.host.local_only.is_empty(), "{:?}", pair.host.local_only);
    assert_eq!(pair.guest_state.doc.scene().entity(4).unwrap().entity.get("Code"), Some("print(1)"));
    assert_eq!(pair.guest_state.doc.scene().entity(5).unwrap().entity.connections.len(), 1);
    pair.documents_match();
    pair.guest_state.doc.set_entity_key(&[4], "Code", "print(2)").unwrap();
    pair.settle();
    assert_eq!(pair.host_state.doc.scene().entity(4).unwrap().entity.get("Code"), Some("print(2)"));
    assert!(logged(&pair.host_state, "Guest added or changed a script or command entity: lua_run (entity:4)"));
    assert_eq!(pair.host.stats.script_notices.len(), 1);
    pair.host_state.doc.set_entity_key(&[5], "targetname", "relay").unwrap();
    pair.settle();
    assert!(logged(
        &pair.guest_state,
        "Host added or changed a script or command entity: OnTrigger > RunScriptCode (logic_relay, entity:5)"
    ));
    pair.guest_state.doc.set_entity_key(&[2], "targetname", "plain").unwrap();
    pair.settle();
    assert_eq!(pair.host.stats.script_notices.len(), 1, "other edits raise no notice");
}

#[test]
fn a_host_that_turns_script_sharing_off_keeps_its_own_and_refuses_the_others() {
    let _english = english();
    let mut pair = Pair::with_host(scripted_map(), |state| state.settings.workflow.team_share_scripts = false);
    assert!(pair.guest_state.doc.scene().entity(4).is_none(), "the host lua_run stays on the host PC");
    assert!(pair.host.local_only.iter().any(|problem| problem.contains("lua_run 4")));
    assert_eq!(pair.guest_state.doc.scene().entity(5).unwrap().entity.connections.len(), 0);
    let script = pair.guest_state.doc.create_point_entity("lua_run", glam::DVec3::ZERO, &[]).unwrap();
    pair.settle();
    assert!(pair.host_state.doc.scene().entity(script).is_none());
    assert!(pair.guest.conflict.contains_key(&format!("entity:{script}")), "the refused entity is kept for recovery");
    let refusal = format!("entity:{script}: class \"lua_run\" refused: script and command entities cannot be shared");
    assert!(pair.guest.status.contains(&refusal), "{}", pair.guest.status);
    assert!(pair.guest_state.doc.scene().entity(script).is_none(), "the shared version is shown");
    pair.host_state.settings.workflow.team_share_scripts = true;
    pair.settle();
    pair.wait("host scripts shared", |pair| pair.guest_state.doc.scene().entity(4).is_some());
    pair.settle();
    assert_eq!(pair.guest_state.doc.scene().entity(5).unwrap().entity.connections.len(), 1);
    assert!(!pair.host.local_only.iter().any(|problem| problem.contains("script and command")));
    pair.documents_match();
}
