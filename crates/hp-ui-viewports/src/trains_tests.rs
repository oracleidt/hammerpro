use std::collections::HashSet;
use std::sync::Arc;

use glam::DVec3;
use hp_app::AppState;
use hp_editor::limits::world::BspWorld;
use hp_vmf::{Entity, Id, Side, Solid, TexAxis};

use crate::player;
use crate::trains::{Trains, accepts, tri_box};

const DT: f64 = 1.0 / 64.0;

fn ent(id: Id, class: &str, kv: &[(&str, &str)]) -> Entity {
    let mut e = Entity { id, classname: class.into(), ..Default::default() };
    for (k, v) in kv {
        e.set(k, *v);
    }
    e
}

fn block(id: Id, lo: DVec3, hi: DVec3) -> Solid {
    let p = DVec3::new;
    let planes = [
        [p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z)],
        [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)],
        [p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)],
        [p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z)],
        [p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z), p(lo.x, hi.y, lo.z)],
        [p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z)],
    ];
    let sides = planes
        .iter()
        .enumerate()
        .map(|(k, plane)| Side {
            id: id * 10 + k as Id,
            plane: *plane,
            material: "dev/dev_measuregeneric01".to_owned(),
            uaxis: TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
            vaxis: TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
            ..Default::default()
        })
        .collect();
    Solid { id, sides, ..Default::default() }
}

fn track(id: Id, name: &str, at: &str, next: &str, extra: &[(&str, &str)]) -> Entity {
    let mut e = ent(id, "path_track", &[("targetname", name), ("origin", at), ("target", next)]);
    for (k, v) in extra {
        e.set(k, *v);
    }
    e
}

fn train(id: Id, class: &str, kv: &[(&str, &str)]) -> Entity {
    let mut e = ent(id, class, kv);
    e.solids.push(block(id * 100, DVec3::new(-32.0, -32.0, -8.0), DVec3::new(32.0, 32.0, 8.0)));
    e
}

fn state_of(entities: Vec<Entity>) -> AppState {
    let mut state = hp_app::testing::test_state();
    state.doc = hp_editor::Document::from_map(hp_vmf::Map { entities, ..Default::default() });
    state
}

fn run(t: &mut Trains, seconds: f64) {
    let none = HashSet::new();
    for _ in 0..(seconds / DT).round() as usize {
        t.step(DT, &none);
    }
}

fn square(extra: &[(&str, &str)]) -> Vec<Entity> {
    vec![
        track(1, "p1", "0 0 0", "p2", &[]),
        track(2, "p2", "512 0 0", "p3", extra),
        track(3, "p3", "512 512 0", "p4", &[]),
        track(4, "p4", "0 512 0", "p1", &[]),
    ]
}

fn near(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < 0.01
}

#[test]
fn a_tracktrain_follows_its_loop_and_turns_with_its_wheels() {
    let mut ents = square(&[]);
    ents.push(train(
        10,
        "func_tracktrain",
        &[("target", "p1"), ("speed", "100"), ("startspeed", "200"), ("height", "8"), ("origin", "0 0 0")],
    ));
    let mut t = Trains::new(&state_of(ents));
    let r = &t.report()[0];
    assert!(r.active && r.moving && near(r.origin, DVec3::new(0.0, 0.0, 8.0)), "{r:?}");
    assert_eq!((r.speed, r.max_speed), (100.0, 200.0));
    run(&mut t, 1.0);
    let r = &t.report()[0];
    assert!(near(r.origin, DVec3::new(100.0, 0.0, 8.0)), "{:?}", r.origin);
    assert!(r.angles.y.abs() < 1e-6);
    run(&mut t, 3.5);
    let r = &t.report()[0];
    assert!(near(r.origin, DVec3::new(450.0, 0.0, 8.0)), "{:?}", r.origin);
    assert!((r.angles.y - 38f64.atan2(62.0).to_degrees()).abs() < 0.5, "the wheels see the corner: {:?}", r.angles);
    run(&mut t, 1.5);
    let r = &t.report()[0];
    assert!(
        r.origin.x > 500.0 && r.origin.x < 512.0 && (r.origin.y - 88.0).abs() < 12.0,
        "cut the corner: {:?}",
        r.origin
    );
    assert!((r.angles.y - 90.0).abs() < 2.0, "{:?}", r.angles);
    assert_eq!((r.node.as_str(), r.next.as_str()), ("p2", "p3"));
    run(&mut t, 2048.0 / 100.0 - 6.0 + 1.0);
    let r = &t.report()[0];
    assert!(r.origin.distance(DVec3::new(100.0, 0.0, 8.0)) < 40.0, "a full loop: {:?}", r.origin);
    assert!(r.moving && (r.angles.y - 360.0).abs() < 2.0, "{:?}", r.angles);
    let m = t.transforms().into_iter().find(|(id, _)| *id == 10).map(|(_, m)| m).unwrap_or_default();
    assert!(near(m.transform_point3(DVec3::ZERO), r.origin));
    let ahead = m.transform_point3(DVec3::X) - r.origin;
    assert!((ahead.y.atan2(ahead.x).to_degrees().rem_euclid(360.0) - r.angles.y.rem_euclid(360.0)).abs() < 1e-6);
}

#[test]
fn pitch_flags_and_fixed_orientation() {
    let path = || vec![track(1, "a", "0 0 0", "b", &[]), track(2, "b", "100 0 100", "", &[])];
    let angles = |kv: &[(&str, &str)]| {
        let mut ents = path();
        let mut all = vec![("target", "a"), ("speed", "0"), ("origin", "0 0 0"), ("wheels", "10")];
        all.extend_from_slice(kv);
        ents.push(train(10, "func_tracktrain", &all));
        Trains::new(&state_of(ents)).report()[0].angles
    };
    assert!((angles(&[]) - DVec3::new(-45.0, 0.0, 0.0)).length() < 1e-6, "nose up the slope");
    assert!(angles(&[("spawnflags", "1")]).length() < 1e-6, "No Pitch");
    assert!(angles(&[("spawnflags", "16"), ("angles", "0 30 0")]) == DVec3::new(0.0, 30.0, 0.0), "Fixed");
    assert!(angles(&[("orientationtype", "0")]).length() < 1e-6, "Never");
    let gold = angles(&[("spawnflags", "32768")]);
    assert!((gold - DVec3::new(45.0, 180.0, 0.0)).length() < 1e-6, "GoldSrc trains face -X: {gold:?}");
}

#[test]
fn blended_orientation_follows_the_path_track_angles() {
    let ents = vec![
        track(1, "a", "0 0 0", "b", &[]),
        track(2, "b", "200 0 0", "", &[("orientationtype", "2"), ("angles", "0 90 0")]),
        train(
            10,
            "func_tracktrain",
            &[("target", "a"), ("speed", "100"), ("orientationtype", "2"), ("origin", "0 0 0")],
        ),
    ];
    let mut t = Trains::new(&state_of(ents));
    run(&mut t, 1.0);
    let r = &t.report()[0];
    assert!((r.angles.y - 45.0).abs() < 0.1, "half way between 0 and 90: {:?}", r.angles);
}

#[test]
fn path_speeds_apply_to_trains_without_user_control() {
    let speed_after = |flags: &str| {
        let mut ents = square(&[("speed", "50")]);
        ents.push(train(
            10,
            "func_tracktrain",
            &[("target", "p1"), ("speed", "100"), ("startspeed", "300"), ("spawnflags", flags), ("origin", "0 0 0")],
        ));
        let mut t = Trains::new(&state_of(ents));
        run(&mut t, 6.0);
        t.report()[0].clone()
    };
    let driven = speed_after("2");
    assert_eq!(driven.speed, 50.0);
    let o = driven.origin;
    assert!(o.x > 495.0 && o.x < 512.0 && (o.y - 44.0).abs() < 8.0, "cut the corner, then slower: {o:?}");
    assert!(!driven.user_control);
    assert_eq!(speed_after("0").speed, 100.0, "the driver keeps the speed");
    let mut ents = square(&[]);
    ents.push(train(10, "func_tracktrain", &[("target", "p1"), ("startspeed", "300"), ("origin", "0 0 0")]));
    let mut t = Trains::new(&state_of(ents));
    assert!(!t.report()[0].moving, "speed 0: waits");
    t.input(10, "SetSpeed", "0.5");
    assert_eq!(t.report()[0].speed, 150.0);
    t.input(10, "SetSpeedReal", "1000");
    assert_eq!(t.report()[0].speed, 300.0, "clipped to the max speed");
    t.input(10, "Stop", "");
    assert!(!t.report()[0].moving);
    t.input(10, "Resume", "");
    assert_eq!(t.report()[0].speed, 300.0);
    t.input(10, "SetMaxSpeed", "120");
    assert_eq!(t.report()[0].speed, 120.0);
}

#[test]
fn a_dead_end_or_a_disabled_track_stops_the_train() {
    let ents = vec![
        track(1, "a", "0 0 0", "b", &[]),
        track(2, "b", "100 0 0", "c", &[]),
        track(3, "c", "200 0 0", "", &[("spawnflags", "1")]),
        train(10, "func_tracktrain", &[("target", "a"), ("speed", "100"), ("origin", "0 0 0")]),
    ];
    let mut t = Trains::new(&state_of(ents));
    t.take_fired();
    run(&mut t, 3.0);
    let r = &t.report()[0];
    assert!(!r.moving && near(r.origin, DVec3::new(100.0, 0.0, 0.0)), "stops before the disabled track: {r:?}");
    assert!(t.take_fired().contains(&(2, "OnPass")));
    assert!(t.input(3, "EnablePath", ""));
    t.input(10, "StartForward", "");
    run(&mut t, 3.0);
    let r = &t.report()[0];
    assert!(!r.moving && near(r.origin, DVec3::new(200.0, 0.0, 0.0)), "then at the dead end: {r:?}");
    assert!(t.take_fired().contains(&(10, "OnStart")));
}

#[test]
fn forks_take_the_default_path_until_the_branch_is_enabled() {
    let ents = || {
        vec![
            track(1, "a", "0 0 0", "b", &[]),
            track(2, "b", "100 0 0", "c", &[("altpath", "d")]),
            track(3, "c", "200 0 0", "", &[]),
            track(4, "d", "100 100 0", "", &[]),
            train(10, "func_tracktrain", &[("target", "a"), ("speed", "100"), ("origin", "0 0 0")]),
        ]
    };
    let mut t = Trains::new(&state_of(ents()));
    run(&mut t, 3.0);
    assert!(near(t.report()[0].origin, DVec3::new(200.0, 0.0, 0.0)));
    let mut t = Trains::new(&state_of(ents()));
    t.input(2, "ToggleAlternatePath", "");
    run(&mut t, 3.0);
    assert!(near(t.report()[0].origin, DVec3::new(100.0, 100.0, 0.0)));
}

#[test]
fn reverse_backs_up_facing_forward_and_teleport_tracks_jump() {
    let ents = vec![
        track(1, "a", "0 0 0", "b", &[]),
        track(2, "b", "100 0 0", "c", &[]),
        track(3, "c", "500 0 0", "d", &[("spawnflags", "16")]),
        track(4, "d", "600 0 0", "", &[]),
        train(10, "func_tracktrain", &[("target", "a"), ("speed", "50"), ("origin", "0 0 0")]),
    ];
    let mut t = Trains::new(&state_of(ents));
    run(&mut t, 1.0);
    t.input(10, "Reverse", "");
    run(&mut t, 0.5);
    let r = &t.report()[0];
    assert!(near(r.origin, DVec3::new(25.0, 0.0, 0.0)) && r.speed < 0.0, "{r:?}");
    assert!(r.angles.y.abs() < 1e-6, "still facing +X: {:?}", r.angles);
    t.input(10, "StartForward", "");
    run(&mut t, 2.0);
    let r = &t.report()[0];
    assert!(r.origin.distance(DVec3::new(525.0, 0.0, 0.0)) < 6.0, "jumped to c when the look-ahead passed b: {r:?}");
    assert!(t.take_fired().contains(&(3, "OnTeleport")));
}

#[test]
fn velocity_blend_between_path_speeds() {
    let ents = vec![
        track(1, "a", "0 0 0", "b", &[("speed", "100")]),
        track(2, "b", "1000 0 0", "", &[("speed", "300")]),
        train(
            10,
            "func_tracktrain",
            &[("target", "a"), ("speed", "100"), ("startspeed", "400"), ("velocitytype", "1"), ("origin", "0 0 0")],
        ),
    ];
    let mut t = Trains::new(&state_of(ents));
    for _ in 0..512 {
        t.step(DT, &HashSet::new());
        if t.report()[0].origin.x >= 500.0 {
            break;
        }
    }
    let r = &t.report()[0];
    let want = 100.0 + 200.0 * r.origin.x / 1000.0;
    assert!((r.speed - want).abs() < 2.0, "{} at x {}", r.speed, r.origin.x);
}

#[test]
fn func_train_waits_at_its_corners() {
    let corner = |id: Id, name: &str, at: &str, next: &str, kv: &[(&str, &str)]| {
        let mut e = ent(id, "path_corner", &[("targetname", name), ("origin", at), ("target", next)]);
        for (k, v) in kv {
            e.set(k, *v);
        }
        e
    };
    let ents = |named: bool| {
        let mut kv = vec![("target", "c1"), ("speed", "100"), ("origin", "0 0 0")];
        if named {
            kv.push(("targetname", "lift"));
        }
        vec![
            corner(1, "c1", "0 0 0", "c2", &[]),
            corner(2, "c2", "100 0 0", "c3", &[("wait", "2")]),
            corner(3, "c3", "100 100 0", "c1", &[("speed", "200")]),
            train(10, "func_train", &kv),
        ]
    };
    let mut t = Trains::new(&state_of(ents(false)));
    run(&mut t, 0.0625);
    assert!(near(t.report()[0].origin, DVec3::ZERO), "leaves after a moment");
    run(&mut t, 1.0625);
    assert!(near(t.report()[0].origin, DVec3::new(100.0, 0.0, 0.0)), "{:?}", t.report()[0].origin);
    run(&mut t, 1.5);
    assert!(near(t.report()[0].origin, DVec3::new(100.0, 0.0, 0.0)), "waits 2 s at c2");
    assert!(t.report()[0].moving, "waiting is part of the run");
    run(&mut t, 1.0);
    assert!(near(t.report()[0].origin, DVec3::new(100.0, 52.5, 0.0)), "{:?}", t.report()[0].origin);
    run(&mut t, 0.828125);
    let o = t.report()[0].origin;
    assert!(o.distance(DVec3::new(50.0, 50.0, 0.0)) < 0.5, "c3 speeds it up: {o:?}");
    let mut t = Trains::new(&state_of(ents(true)));
    run(&mut t, 2.0);
    assert!(near(t.report()[0].origin, DVec3::ZERO) && !t.report()[0].moving, "a named train waits for Start");
    t.input(10, "Start", "");
    run(&mut t, 0.5);
    assert!(near(t.report()[0].origin, DVec3::new(50.0, 0.0, 0.0)));
    t.input(10, "Toggle", "");
    run(&mut t, 0.5);
    assert!(near(t.report()[0].origin, DVec3::new(50.0, 0.0, 0.0)) && !t.report()[0].moving);
    assert_eq!(t.request(Some("LIFT"), None), 1);
    run(&mut t, 0.5);
    assert!(near(t.report()[0].origin, DVec3::new(100.0, 0.0, 0.0)));
}

#[test]
fn the_player_rides_the_train_and_gets_pushed_by_it() {
    let mut platform = ent(10, "func_tracktrain", &[("target", "a"), ("speed", "100"), ("origin", "0 0 0")]);
    platform.solids.push(block(1000, DVec3::new(-64.0, -64.0, 0.0), DVec3::new(64.0, 64.0, 16.0)));
    platform.set("targetname", "cart");
    let prop = ent(11, "prop_dynamic", &[("model", "models/x.mdl"), ("origin", "0 0 16"), ("parentname", "cart")]);
    let ents = vec![track(1, "a", "0 0 0", "b", &[]), track(2, "b", "2000 0 0", "", &[]), platform, prop];
    let state = state_of(ents);
    let mut t = Trains::new(&state);
    assert!(t.owns(10) && !t.owns(11));
    let mut p = hp_app::runtime::PlayerState {
        active: true,
        feet: DVec3::new(0.0, 0.0, 20.0),
        ..hp_app::runtime::PlayerState::default()
    };
    let mut yaw = 0.0;
    let none = HashSet::new();
    for _ in 0..64 {
        let riding = if p.on_ground { t.standing_on(p.feet, Some(&none), None) } else { None };
        t.step(DT, &none);
        t.move_player(riding, &mut p.feet, &mut yaw, player::HEIGHT, Some(&none));
        let c = player::DocCollider {
            state: &state,
            props: None,
            except: None,
            doors: None,
            hidden: None,
            trains: Some(&t),
        };
        let _ = player::step(&mut p, player::Move::default(), yaw, 0.0, DT, &c);
    }
    assert!(p.on_ground && (p.feet.z - 16.0).abs() < 1e-6, "{:?}", p.feet);
    let x = t.report()[0].origin.x;
    assert!(p.feet.x > 80.0 && p.feet.x <= x + 1e-6, "carried along: {:?}, train at {x}", p.feet);
    let moved = t.child_transform(11).map(|m| m.transform_point3(DVec3::new(0.0, 0.0, 16.0)));
    assert!(moved.is_some_and(|c| near(c, DVec3::new(100.0, 0.0, 16.0))), "{moved:?}");
    let mut feet = DVec3::new(200.0, 0.0, 0.0);
    for _ in 0..(3.0 / DT) as usize {
        t.step(DT, &none);
        t.move_player(None, &mut feet, &mut yaw, player::HEIGHT, Some(&none));
    }
    let front = t.report()[0].origin.x + 64.0;
    assert!(feet.x >= front + player::HALF_WIDTH - 1.0, "pushed ahead of the train: {feet:?}, front {front}");
}

#[test]
fn the_compiled_model_moves_from_where_the_map_placed_it() {
    let ents = vec![
        track(1, "a", "100 0 0", "b", &[]),
        track(2, "b", "100 500 0", "", &[]),
        train(10, "func_tracktrain", &[("target", "a"), ("speed", "0"), ("origin", "0 0 0")]),
    ];
    let mut t = Trains::new(&state_of(ents));
    let world = Arc::new(BspWorld {
        entity_models: vec![hp_editor::limits::world::EntityModel {
            id: 10,
            faces: 0..0,
            origin: [8.0, 0.0, 0.0],
            angles: [0.0, 90.0, 0.0],
        }],
        ..BspWorld::default()
    });
    let editor = t.transforms()[0].1;
    assert!(near(editor.transform_point3(DVec3::X), DVec3::new(100.0, 1.0, 0.0)), "faces +Y toward b");
    t.set_world(Some(&world));
    let compiled = t.transforms()[0].1;
    assert!(near(compiled.transform_point3(DVec3::new(8.0, 0.0, 0.0)), DVec3::new(100.0, 0.0, 0.0)));
    assert!(near(compiled.transform_point3(DVec3::new(8.0, 1.0, 0.0)), DVec3::new(100.0, 1.0, 0.0)));
}

#[test]
fn map_spawn_inputs_start_the_train_and_use_toggles_it() {
    let mut auto = ent(20, "logic_auto", &[]);
    auto.connections.push(hp_vmf::Connection {
        output: "OnMapSpawn".into(),
        target: "cart".into(),
        input: "StartForward".into(),
        param: String::new(),
        delay: 0.0,
        times: -1,
        separator: ',',
    });
    let ents = vec![
        track(1, "a", "0 0 0", "b", &[]),
        track(2, "b", "100 0 0", "", &[]),
        train(
            10,
            "func_tanktrain",
            &[("targetname", "cart"), ("target", "a"), ("startspeed", "50"), ("origin", "0 0 0")],
        ),
        auto,
    ];
    let state = state_of(ents);
    let mut t = Trains::new(&state);
    let mut sim = crate::io::IoSim::new(&state);
    let mut doors = player::Doors::default();
    sim.step(&state, &mut doors, DVec3::splat(9999.0), player::HEIGHT, 0.0);
    for (id, input, param) in sim.take_train_inputs() {
        t.input(id, &input, &param);
    }
    assert_eq!(t.report()[0].speed, 50.0);
    assert!(t.report()[0].user_control);
    assert_eq!(t.use_entity(10), Some(true));
    assert!(!t.report()[0].moving, "E stops it");
    assert!(t.toggle_all());
    assert!(t.report()[0].moving);
    assert!(accepts("path_track", "TogglePath"));
    assert!(!accepts("func_train", "StartForward"));
}

#[test]
fn hull_triangle_overlap() {
    let tri = [DVec3::new(-10.0, -10.0, 0.0), DVec3::new(10.0, -10.0, 0.0), DVec3::new(0.0, 10.0, 0.0)];
    assert!(tri_box(&tri, DVec3::ZERO, DVec3::splat(1.0)));
    assert!(!tri_box(&tri, DVec3::new(0.0, 0.0, 2.0), DVec3::splat(1.0)));
    assert!(!tri_box(&tri, DVec3::new(20.0, 0.0, 0.0), DVec3::splat(1.0)));
}

#[test]
fn triggers_parented_to_a_train_touch_where_the_train_is() {
    let cart =
        train(10, "func_tracktrain", &[("targetname", "cart"), ("target", "a"), ("speed", "100"), ("origin", "0 0 0")]);
    let mut trigger = ent(11, "trigger_multiple", &[("parentname", "cart")]);
    trigger.solids.push(block(1100, DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 64.0)));
    trigger.connections.push(hp_vmf::Connection {
        output: "OnStartTouch".into(),
        target: "cart".into(),
        input: "Stop".into(),
        param: String::new(),
        delay: 0.0,
        times: -1,
        separator: ',',
    });
    let ents = vec![track(1, "a", "0 0 0", "b", &[]), track(2, "b", "1000 0 0", "", &[]), cart, trigger];
    let state = state_of(ents);
    let mut t = Trains::new(&state);
    assert!(!t.owns(11), "a trigger does not collide");
    run(&mut t, 2.0);
    let mut sim = crate::io::IoSim::new(&state);
    let mut doors = player::Doors::default();
    sim.set_moved(&t.transforms());
    sim.step(&state, &mut doors, DVec3::ZERO, player::HEIGHT, 0.0);
    assert!(sim.take_train_inputs().is_empty(), "the trigger left the start");
    sim.step(&state, &mut doors, DVec3::new(200.0, 0.0, 0.0), player::HEIGHT, 0.0);
    let inputs = sim.take_train_inputs();
    assert_eq!(inputs, [(10, "stop".to_owned(), String::new())]);
    for (id, input, param) in inputs {
        t.input(id, &input, &param);
    }
    assert!(!t.report()[0].moving);
}

#[test]
fn the_cart_of_the_gmod_test_map_cuts_its_corners_like_the_engine() {
    let ents = vec![
        track(1, "c1", "-600 -600 0", "c2", &[("speed", "150")]),
        track(2, "c2", "600 -600 0", "c3", &[]),
        track(3, "c3", "600 600 0", "c4", &[("speed", "300")]),
        track(4, "c4", "-600 600 0", "c1", &[]),
        train(
            10,
            "func_tracktrain",
            &[
                ("target", "c1"),
                ("speed", "150"),
                ("startspeed", "300"),
                ("height", "12"),
                ("wheels", "64"),
                ("spawnflags", "2"),
                ("origin", "-600 -600 12"),
            ],
        ),
    ];
    let mut t = Trains::new(&state_of(ents));
    let none = HashSet::new();
    let until = |t: &mut Trains, done: &dyn Fn(DVec3) -> bool| {
        for _ in 0..100_000 {
            t.step(0.015, &none);
            if done(t.report()[0].origin) {
                break;
            }
        }
        t.report()[0].clone()
    };
    let r = until(&mut t, &|o| o.x >= 552.0);
    assert!((r.angles.y - 15.3).abs() < 0.5, "GMod: yaw 15.3 at x 552: {:?}", r.angles);
    let r = until(&mut t, &|o| o.y >= -597.6);
    assert!((r.origin.x - 588.0).abs() < 1.0, "GMod: x 588 after the corner: {:?}", r.origin);
    assert!((r.angles.y - 89.4).abs() < 0.2, "GMod: yaw 89.4: {:?}", r.angles);
    let r = until(&mut t, &|o| o.y >= -449.1);
    assert!((r.origin.x - 589.5).abs() < 1.0, "GMod: x 589.5 at y -449.1: {:?}", r.origin);
    let r = until(&mut t, &|o| o.y >= 450.8);
    assert!((r.origin.x - 598.5).abs() < 1.0, "GMod: x 598.5 at y 450.8: {:?}", r.origin);
    assert_eq!(r.speed, 150.0);
    let r = until(&mut t, &|o| o.x <= 266.9);
    assert!((r.origin.y - 589.7).abs() < 3.0, "GMod: y 589.7 at x 266.9: {:?}", r.origin);
    assert!((r.angles.y - 179.3).abs() < 0.5, "GMod: yaw 179.3: {:?}", r.angles);
    assert_eq!(r.speed, 300.0, "GMod: c3 sets 300");
}

#[test]
fn teleports_parented_to_a_train_send_the_player_where_the_train_is() {
    let cart =
        train(10, "func_tracktrain", &[("targetname", "cart"), ("target", "a"), ("speed", "100"), ("origin", "0 0 0")]);
    let mut gate = ent(11, "trigger_teleport", &[("parentname", "cart"), ("target", "seat")]);
    gate.solids.push(block(1100, DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 64.0)));
    let seat =
        ent(12, "info_teleport_destination", &[("targetname", "seat"), ("parentname", "cart"), ("origin", "0 0 100")]);
    let mut pt = ent(13, "point_teleport", &[("targetname", "jump"), ("parentname", "cart"), ("origin", "0 0 200")]);
    pt.set("angles", "0 90 0");
    let ents = vec![track(1, "a", "0 0 0", "b", &[]), track(2, "b", "1000 0 0", "", &[]), cart, gate, seat, pt];
    let state = state_of(ents);
    let mut t = Trains::new(&state);
    run(&mut t, 2.0);
    let x = t.report()[0].origin.x;
    let mut p = hp_app::runtime::PlayerState { active: true, feet: DVec3::ZERO, ..Default::default() };
    assert_eq!(player::teleport(&state, &mut p, Some(&t)), None, "the gate left the start");
    p.feet = DVec3::new(x, 0.0, 0.0);
    assert!(player::teleport(&state, &mut p, Some(&t)).is_some());
    assert!(near(p.feet, DVec3::new(x, 0.0, 100.0)), "to the seat on the train: {:?}", p.feet);
    let mut doors = player::Doors::default();
    let auto = hp_vmf::Connection {
        output: "OnTrigger".into(),
        target: "jump".into(),
        input: "Teleport".into(),
        param: String::new(),
        delay: 0.0,
        times: -1,
        separator: ',',
    };
    let mut relay = ent(20, "logic_relay", &[]);
    relay.connections.push(auto);
    let mut ents2: Vec<Entity> = state.doc.scene().entities().values().map(|n| n.entity.clone()).collect();
    ents2.push(relay);
    let state2 = state_of(ents2);
    let mut sim = crate::io::IoSim::new(&state2);
    sim.set_moved(&t.transforms());
    sim.fire(&state2, 20, "OnTrigger");
    sim.step(&state2, &mut doors, DVec3::splat(9999.0), player::HEIGHT, 0.0);
    let (to, yaw) = sim.teleport.expect("teleported");
    assert!(near(to, DVec3::new(x, 0.0, 200.0)), "{to:?}");
    assert!(yaw.is_some_and(|y| (y - 90.0).abs() < 1e-6), "{yaw:?}");
}
