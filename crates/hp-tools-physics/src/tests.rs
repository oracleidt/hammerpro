use super::*;
use glam::{DQuat, DVec3};
use hp_editor::{Document, Owner};
use hp_geom::{Aabb, PrimitiveParams};

fn setup() -> (AppState, u32) {
    register_translations();
    let mut state = hp_app::testing::test_state();
    state.doc = Document::new_empty();
    let params = |min, max| PrimitiveParams { bounds: Aabb::new(min, max), ..Default::default() };
    state
        .doc
        .create_primitive(&params(DVec3::new(-512.0, -512.0, -32.0), DVec3::new(512.0, 512.0, 0.0)), Owner::World)
        .unwrap();
    let id = state
        .doc
        .create_primitive(&params(DVec3::new(-16.0, -16.0, 184.0), DVec3::new(16.0, 16.0, 216.0)), Owner::World)
        .unwrap()[0];
    state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    (state, id)
}

fn tool(state: &AppState) -> PhysicsTool {
    let session = scene::Snapshot::capture(state).build(&AtomicBool::new(false)).unwrap();
    PhysicsTool { session: Some(session), key: Some(document_key(state)), ..Default::default() }
}

#[test]
fn simulation_is_temporary_and_keep_positions_is_one_undo_step() {
    let (mut state, id) = setup();
    let original = state.doc.scene().solid(id).unwrap().solid.clone();
    let revision = state.doc.revision();
    let mut tool = tool(&state);
    let session = tool.session.as_mut().unwrap();
    for _ in 0..600 {
        session.simulation.step();
    }
    assert_eq!(state.doc.revision(), revision);
    assert_eq!(state.doc.scene().solid(id).unwrap().solid, original);
    tool.save(&mut state);
    let result = state.doc.scene().solid(id).unwrap().bounds;
    assert!(result.min.z.abs() < 1.0 && (result.max.z - 32.0).abs() < 1.0, "{result:?}");
    state.doc.undo();
    assert_eq!(state.doc.scene().solid(id).unwrap().solid, original);
    state.doc.redo();
    assert!(state.doc.scene().solid(id).unwrap().bounds.min.z.abs() < 1.0);
}

#[test]
fn reset_and_undo_during_play_do_not_modify_the_document() {
    let (mut state, id) = setup();
    let original = state.doc.scene().solid(id).unwrap().solid.clone();
    let revision = state.doc.revision();
    let mut tool = tool(&state);
    for _ in 0..30 {
        tool.session.as_mut().unwrap().simulation.step();
    }
    assert!(tool.action(&mut state, Action::Undo));
    assert!(tool.session.is_none());
    assert_eq!(state.doc.revision(), revision);
    assert_eq!(state.doc.scene().solid(id).unwrap().solid, original);
}

#[test]
fn changed_documents_discard_stale_simulation_results() {
    let (mut state, id) = setup();
    let mut tool = tool(&state);
    state.doc.translate(&[ObjRef::Solid(id)], DVec3::X * 100.0).unwrap();
    let changed = state.doc.scene().solid(id).unwrap().solid.clone();
    tool.update(&mut state, &egui::Context::default());
    assert!(tool.session.is_none());
    tool.save(&mut state);
    assert_eq!(state.doc.scene().solid(id).unwrap().solid, changed);
}

#[test]
fn leaving_a_document_cancels_unconfirmed_poses() {
    let (mut state, id) = setup();
    let original = state.doc.scene().solid(id).unwrap().solid.clone();
    let revision = state.doc.revision();
    let mut tool = tool(&state);
    for _ in 0..600 {
        tool.session.as_mut().unwrap().simulation.step();
    }
    assert!(tool.cancel(&mut state));
    tool.deactivate(&mut state);
    assert_eq!(state.doc.revision(), revision);
    assert_eq!(state.doc.scene().solid(id).unwrap().solid, original);
}

#[test]
fn displacement_no_physics_does_not_create_a_solid_base_brush() {
    let mut state = hp_app::testing::test_state();
    state.doc = Document::new_empty();
    let id = state
        .doc
        .create_primitive(
            &PrimitiveParams {
                bounds: Aabb::new(DVec3::splat(-64.0), DVec3::new(64.0, 64.0, 0.0)),
                ..Default::default()
            },
            Owner::World,
        )
        .unwrap()[0];
    let face = state.doc.scene().solid(id).unwrap().geom.faces.iter().find(|face| face.plane.normal.z > 0.9).unwrap();
    let face = hp_editor::FaceRef { solid: id, side: face.side_index };
    state.doc.create_displacements(&[face], 2).unwrap();
    for disabled in [false, true] {
        state.doc.set_disp_flags(&[face], hp_geom::DISP_FLAG_NO_PHYSICS, disabled).unwrap();
        let mut session = scene::Snapshot::capture(&state).build(&AtomicBool::new(false)).unwrap();
        session.simulation.prepare();
        let hit = session.simulation.ray_hit(DVec3::Z * 128.0, DVec3::NEG_Z, 256.0);
        assert_eq!(hit.is_none(), disabled);
    }
}

#[test]
fn saved_model_rotation_is_written_when_angles_were_absent() {
    let mut state = hp_app::testing::test_state();
    let id = state.doc.create_point_entity("prop_physics", DVec3::Z * 100.0, &[]).unwrap();
    state.doc.remove_entity_key(&[id], "angles").unwrap();
    let original = state.doc.scene().entity(id).unwrap().entity.clone();
    let rotation = DQuat::from_rotation_x(0.7);
    let mut simulation = Simulation::default();
    simulation
        .add_body(&hp_physics::Body {
            id: 0,
            transform: Transform { position: DVec3::Z * 100.0, rotation },
            parts: vec![hp_physics::ConvexPart {
                points: (0..8)
                    .map(|i| {
                        DVec3::new(
                            if i & 1 == 0 { -1.0 } else { 1.0 },
                            if i & 2 == 0 { -1.0 } else { 1.0 },
                            if i & 4 == 0 { -1.0 } else { 1.0 },
                        )
                    })
                    .collect(),
                transform: Transform::default(),
            }],
            mass: 1.0,
            friction: 0.8,
            restitution: 0.0,
        })
        .unwrap();
    let mut tool = PhysicsTool {
        session: Some(Session {
            simulation,
            bindings: vec![Binding {
                id: 0,
                obj: ObjRef::Entity(id),
                initial: DMat4::from_translation(DVec3::Z * 100.0),
            }],
            skipped: vec![],
            ragdolls: vec![],
        }),
        key: Some(document_key(&state)),
        ..Default::default()
    };
    tool.save(&mut state);
    let angles = state.doc.scene().entity(id).unwrap().entity.angles().unwrap();
    assert!(hp_geom::angles_to_quat(angles).angle_between(rotation) < 1e-6);
    state.doc.undo();
    assert_eq!(state.doc.scene().entity(id).unwrap().entity, original);
}

#[test]
fn installed_ragdoll_simulates_and_keeps_a_source_pose_in_one_undo_step() {
    let game = std::env::var_os("HAMMERPRO_GMOD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "D:/SteamLibrary/steamapps/common/GarrysMod".into());
    let path = game.join("sourceengine/hl2_misc_dir.vpk");
    if !path.is_file() {
        eprintln!("skipping: installed Source models unavailable");
        return;
    }
    let archive = hp_vpk::VpkArchive::open(path).unwrap();
    let model = "models/humans/group01/male_07.mdl";
    let rig = Arc::new(hp_ragdoll::Rig::load(model, |path| archive.read(path).unwrap(), 0).unwrap());
    let mut state = hp_app::testing::test_state();
    state.doc = Document::new_empty();
    let id =
        state.doc.create_point_entity("prop_ragdoll", DVec3::Z * 128.0, &[("model".into(), model.into())]).unwrap();
    let original = state.doc.scene().entity(id).unwrap().entity.clone();
    let mut simulation = Simulation::default();
    simulation
        .add_static_mesh(
            &[
                DVec3::new(-1024.0, -1024.0, 0.0),
                DVec3::new(1024.0, -1024.0, 0.0),
                DVec3::new(1024.0, 1024.0, 0.0),
                DVec3::new(-1024.0, 1024.0, 0.0),
            ],
            &[[0, 1, 2], [0, 2, 3]],
        )
        .unwrap();
    let ragdoll =
        scene::add_ragdoll(&original, Arc::clone(&rig), true, &mut simulation, &mut Vec::new(), &mut Vec::new())
            .unwrap()
            .unwrap();
    simulation.prepare();
    for _ in 0..1200 {
        simulation.step();
    }
    let poses = ragdoll.poses(&simulation).unwrap();
    assert!(poses.iter().all(|pose| pose.is_finite()));
    for joint in &rig.joints {
        let (a, b) = rig.joint_frames(joint, 1.0);
        let a = (poses[joint.parent] * a).w_axis.truncate();
        let b = (poses[joint.child] * b).w_axis.truncate();
        assert!(a.distance(b) < 2.0, "joint {}->{}: {}", joint.parent, joint.child, a.distance(b));
    }
    let mut tool = PhysicsTool {
        session: Some(Session { simulation, bindings: vec![], ragdolls: vec![ragdoll], skipped: vec![] }),
        key: Some(document_key(&state)),
        ..Default::default()
    };
    assert_eq!(tool.session.as_ref().unwrap().bone_poses()[0].1.len(), rig.model.bones.len());
    tool.save(&mut state);
    let saved = &state.doc.scene().entity(id).unwrap().entity;
    let overrides = saved.get("angleOverride").unwrap();
    assert_eq!(overrides.split(',').count(), rig.bodies.len() * 2);
    let reconstructed = rig.body_poses(DMat4::from_translation(saved.origin().unwrap()), overrides);
    for (actual, saved) in poses.iter().zip(reconstructed) {
        assert!(actual.w_axis.truncate().distance(saved.w_axis.truncate()) < 2.0);
        assert!(actual.to_scale_rotation_translation().1.angle_between(saved.to_scale_rotation_translation().1) < 1e-5);
    }
    state.doc.undo();
    assert_eq!(state.doc.scene().entity(id).unwrap().entity, original);
}

#[test]
fn scripted_simulation_drops_the_selection_and_keeps_it_in_one_undo_step() {
    let (mut state, id) = setup();
    let original = state.doc.scene().solid(id).unwrap().solid.clone();
    let mut tool = PhysicsTool::default();
    assert!(tool.set_option(&mut state, "simulate", "0"), "0 does nothing");
    assert!(!tool.is_running());
    assert!(!tool.set_option(&mut state, "simulate", "-1"));
    assert!(!tool.set_option(&mut state, "simulate", "1e9"));
    assert!(!tool.set_option(&mut state, "simulate", "nan"));
    assert!(!tool.set_option(&mut state, "bodies", "3"), "read only");
    assert!(tool.set_option(&mut state, "bodies", "0"), "its own value is accepted");
    assert!(tool.set_option(&mut state, "simulate", "2.5"));
    assert!(tool.is_running());
    let values = tool.option_values();
    let value = |k: &str| values.iter().find(|o| o.key == k).map(|o| o.value.clone()).unwrap();
    assert_eq!((value("simulated"), value("paused"), value("bodies")), ("2.5".into(), "on".into(), "1".into()));
    for o in &values {
        assert!(tool.set_option(&mut state, o.key, &o.value), "{} takes back {}", o.key, o.value);
    }
    assert!(tool.is_running(), "reading the options back changes nothing");
    assert_eq!(state.doc.scene().solid(id).unwrap().solid, original, "the map waits for keep");
    assert!(tool.set_option(&mut state, "simulate", "2.5"));
    let (undo, _) = state.doc.history_len();
    assert!(tool.set_option(&mut state, "keep", "on"));
    assert!(!tool.is_running());
    assert_eq!(state.doc.history_len().0, undo + 1);
    let result = state.doc.scene().solid(id).unwrap().bounds;
    assert!(result.min.z.abs() < 1.0 && (result.max.z - 32.0).abs() < 1.0, "{result:?}");
    state.doc.undo();
    assert_eq!(state.doc.scene().solid(id).unwrap().solid, original);
    assert!(tool.set_option(&mut state, "simulate", "1"));
    assert!(tool.set_option(&mut state, "reset", "on"));
    assert!(!tool.is_running());
    assert_eq!(state.doc.scene().solid(id).unwrap().solid, original);
    state.doc.clear_selection();
    assert!(!tool.set_option(&mut state, "simulate", "1"), "nothing selected");
}
