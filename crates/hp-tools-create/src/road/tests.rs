#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glam::{DVec2, DVec3};
use hp_geom::texel_uv;

use super::apply::{self, SplitAt, split_meta};
use super::build::{LateralSide, PieceRole, Road, RoadEnv, RoadError, build};
use super::model::{Attach, Junction, JunctionEnd, LineKind, RoadMeta, RoadPoint};
use super::spline::dense;
use crate::curve::apply::Target;
use crate::tests_support::settle;

fn p(x: f64, y: f64, z: f64) -> RoadPoint {
    RoadPoint::new(x, y, z)
}

fn meta(points: &[RoadPoint]) -> RoadMeta {
    RoadMeta { points: points.to_vec(), ..RoadMeta::default() }
}

fn built(m: &RoadMeta) -> Road {
    build(m, &RoadEnv::default()).unwrap()
}

fn solids(road: &Road) -> Vec<Vec<hp_vmf::Side>> {
    apply::road_solids(road, 16, 1000).unwrap()
}

fn assert_compiled_caps(road: &Road, brushes: &[Vec<hp_vmf::Side>], direction: DVec3, offset: f64, angle: f64) {
    let mut planes = hp_bspc::planes::PlaneSet::new();
    for (i, piece) in road.pieces.iter().enumerate() {
        let Some((j, next)) = road.pieces.iter().enumerate().find(|(_, next)| {
            next.segment == piece.segment + 1 && next.role == piece.role && next.lateral == piece.lateral
        }) else {
            continue;
        };
        let joint = (piece.corners[2] + piece.corners[3] + piece.corners[6] + piece.corners[7]) * 0.25;
        let boundary = |sides: &[hp_vmf::Side], sign: f64| {
            sides
                .iter()
                .find(|side| {
                    let n = side.normal();
                    n.dot(direction) * sign > 0.99 && (n.dot(joint) - side.plane_distance()).abs() < hp_geom::CLIP_EPS
                })
                .expect("shared road cross section")
                .plane
        };
        assert!(piece.corners[2].distance(next.corners[0]) < 1e-8);
        let a = boundary(&brushes[i], 1.0);
        let b = boundary(&brushes[j], -1.0);
        let ai = planes.from_points(a[0], a[1], a[2]).unwrap();
        let bi = planes.from_points(b[0], b[1], b[2]).unwrap();
        assert!(
            planes.get(ai).distance(joint).abs()
                < hp_geom::PLANE_EPS + joint.abs().max_element() * f64::from(f32::EPSILON) * 4.0,
            "compiled joint moved away from its road: {joint}, distance {}, plane {:?}, points {a:?}, offset {offset}, angle {angle}",
            planes.get(ai).distance(joint),
            planes.get(ai)
        );
        assert_eq!(
            ai ^ 1,
            bi,
            "compiled road crack at offset {offset}, angle {angle}, segment {}, role {:?}, A {:?} from {a:?}, B {:?} from {b:?}, joint {joint}",
            piece.segment,
            piece.role,
            planes.get(ai),
            planes.get(bi)
        );
    }
}

#[test]
fn source_compilation_keeps_adjacent_road_boundary_planes_identical() {
    for offset in [-12000.0, 0.0, 12000.0] {
        for angle in [0.0001f64, 0.001, 0.003, 0.005, 0.01, 0.31] {
            let start = DVec3::new(offset, offset, 64.0);
            let direction = DVec3::new(angle.cos(), angle.sin(), 0.0);
            let end = start + direction * 1024.0;
            let mut m = meta(&[p(start.x, start.y, start.z), p(end.x, end.y, end.z)]);
            m.line_width = 1.0;
            let road = built(&m);
            let brushes = solids(&road);
            assert_compiled_caps(&road, &brushes, direction, offset, angle);
        }
    }
}

#[test]
fn optimized_roads_preserve_geometry_markings_and_texture_coordinates() {
    for points in [
        vec![p(0.0, 0.0, 32.0), p(4096.0, 0.0, 32.0)],
        vec![p(0.0, 0.0, 32.0), p(512.0, 128.0, 32.0), p(1024.0, -256.0, 32.0)],
        vec![p(0.0, 0.0, 32.0), p(2048.0, 512.0, 160.0)],
        vec![p(0.0, 0.0, 32.0), p(512.0, 128.0, 96.0), p(1024.0, -256.0, 32.0)],
    ] {
        let m = RoadMeta { points, ..RoadMeta::default() };
        let original = built(&m);
        let reduced = built(&RoadMeta { optimize: true, ..m.clone() });
        assert_eq!(original.samples, reduced.samples);
        assert!(reduced.pieces.len() <= original.pieces.len());
        if m.points.len() == 2 {
            assert!(reduced.pieces.len() < original.pieces.len() / 4);
        }
        if m.points.len() == 3 && m.points.iter().all(|p| p.z == m.points[0].z) {
            assert!(reduced.pieces.len() < original.pieces.len());
        }
        let geometries: Vec<_> = solids(&reduced).iter().map(|s| hp_geom::validate_solid(s).unwrap()).collect();
        let volume: f64 = solids(&original).iter().map(|s| hp_geom::validate_solid(s).unwrap().volume()).sum();
        let new_volume: f64 = geometries.iter().map(|g| g.volume()).sum();
        assert!((volume - new_volume).abs() < volume * 1e-8);
        for piece in &original.pieces {
            let center = piece.corners.iter().sum::<DVec3>() / piece.corners.len() as f64;
            let (new, _) = reduced
                .pieces
                .iter()
                .zip(&geometries)
                .find(|(p, g)| p.material == piece.material && g.faces.iter().all(|f| f.plane.distance(center) <= 1e-5))
                .expect("same material covers the original road point");
            for uv in [piece.uv, new.uv] {
                let u = center.dot(uv.u_axis) / m.tex_scale + uv.u_shift;
                let v = center.dot(uv.v_axis) / m.tex_scale + uv.v_shift;
                let expected_u = center.dot(piece.uv.u_axis) / m.tex_scale + piece.uv.u_shift;
                let expected_v = center.dot(piece.uv.v_axis) / m.tex_scale + piece.uv.v_shift;
                assert!((u - expected_u).abs() < 1e-6 && (v - expected_v).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn source_compilation_preserves_shallow_slopes_at_large_coordinates() {
    let m = meta(&[p(12000.0, 12000.0, 64.0), p(13024.0, 12001.024, 66.048)]);
    let road = built(&m);
    let mut planes = hp_bspc::planes::PlaneSet::new();
    for (piece, sides) in road.pieces.iter().zip(solids(&road)) {
        for side in sides.iter().filter(|s| s.normal().z.abs() > 0.99) {
            let center = if side.normal().z > 0.0 { &piece.corners[4..] } else { &piece.corners[..4] };
            let center = center.iter().sum::<DVec3>() / 4.0;
            let p = side.plane;
            let index = planes.from_points(p[0], p[1], p[2]).unwrap();
            assert!(
                planes.get(index).distance(center).abs() < hp_geom::PLANE_EPS,
                "shallow road surface moved at {center}"
            );
        }
    }
}

#[test]
fn exporting_existing_world_and_entity_roads_stabilizes_compiler_planes() {
    let angle = 0.001f64;
    let direction = DVec3::new(angle.cos(), angle.sin(), 0.0);
    let start = DVec3::new(12000.0, 12000.0, 64.0);
    let end = start + direction * 1024.0;
    let mut m = meta(&[p(start.x, start.y, start.z), p(end.x, end.y, end.z)]);
    m.line_width = 1.0;
    let road = built(&m);
    for target in [Target::World, Target::NewEntity { class: "func_brush", keyvalues: &[] }] {
        let mut doc = hp_editor::Document::new_empty();
        let created = apply::create_road(&mut doc, &m, 16, target).unwrap();
        for (id, piece) in created.ids.iter().zip(&road.pieces) {
            let mut next = || 0;
            let original = hp_geom::hull_solid(&piece.corners, &piece.material, m.tex_scale, 16, &mut next).unwrap();
            doc.edit_solid(*id, |solid| {
                for (side, raw) in solid.sides.iter_mut().zip(original) {
                    side.plane = raw.plane;
                }
            })
            .unwrap();
        }
        let before = hp_vmf::write(&doc.scene().to_map());
        let exported = hp_vmf::parse(&doc.to_vmf_string()).unwrap();
        let exported_solids: Vec<_> =
            exported.world.solids.iter().chain(exported.entities.iter().flat_map(|e| &e.solids)).collect();
        let brushes: Vec<_> =
            created.ids.iter().map(|id| exported_solids.iter().find(|s| s.id == *id).unwrap().sides.clone()).collect();
        assert_compiled_caps(&road, &brushes, direction, 12000.0, angle);
        assert_eq!(hp_vmf::write(&doc.scene().to_map()), before);
    }
}

#[test]
fn optimized_straight_roads_are_limited_by_the_final_brush_count() {
    let mut m = meta(&[p(-14000.0, 0.0, 32.0), p(14000.0, 0.0, 32.0)]);
    m.step = 16.0;
    assert!(matches!(build(&m, &RoadEnv::default()), Err(RoadError::TooManyPieces(_))));
    m.optimize = true;
    let reduced = built(&m);
    assert!(reduced.pieces.len() < 1000);
    assert_eq!(solids(&reduced).len(), reduced.pieces.len());
}

#[test]
fn optimization_is_saved_and_existing_roads_can_be_rebuilt_with_fewer_brushes() {
    let m = meta(&[p(0.0, 0.0, 32.0), p(4096.0, 0.0, 32.0)]);
    let mut doc = hp_editor::Document::new_empty();
    let made = apply::create_road(&mut doc, &m, 16, Target::World).unwrap();
    let (record, mut loaded) = apply::read_road(&doc, made.ids[0]).unwrap();
    assert!(!loaded.optimize);
    loaded.optimize = true;
    assert_eq!(RoadMeta::decode(&loaded.encode()), Some(loaded.clone()));
    assert!(!RoadMeta::decode("r1;pts=0,0,32 4096,0,32").unwrap().optimize);
    let changed = apply::replace_road(&mut doc, &record, &loaded, 16).unwrap();
    assert!(changed.ids.len() < made.ids.len() / 4);
    assert!(apply::read_road(&doc, changed.ids[0]).unwrap().1.optimize);
    assert!(doc.undo());
    assert_eq!(doc.curve_record(made.ids[0]).unwrap(), record);
}

#[test]
fn world_roads_compile_as_detail_without_extra_brushes_or_changing_entity_roads() {
    let m = meta(&[p(0.0, 0.0, 16.0), p(256.0, 128.0, 64.0), p(768.0, -128.0, 32.0)]);
    let expected = built(&m).pieces.len();
    for target in [Target::World, Target::NewEntity { class: "func_brush", keyvalues: &[] }] {
        let mut doc = hp_editor::Document::new_empty();
        let made = apply::create_road(&mut doc, &m, 16, target).unwrap();
        assert_eq!(made.ids.len(), expected);
        assert_eq!(doc.stats().solids, expected);
        for id in &made.ids {
            let node = doc.scene().solid(*id).unwrap();
            let contents = if made.entity.is_none() { hp_vmf::CONTENTS_DETAIL } else { 0 };
            assert!(node.solid.sides.iter().all(|s| s.compile_contents() == contents));
            assert_eq!(node.owner, made.entity.map_or(hp_editor::Owner::World, hp_editor::Owner::Entity));
        }
        assert_eq!(doc.to_map().entities.len(), usize::from(made.entity.is_some()));
        assert!(doc.undo());
        assert_eq!(doc.stats().solids, 0);
    }
}

#[test]
fn junction_presets_produce_closed_nonoverlapping_brushes_and_survive_editing() {
    use super::model::RoadLayout;
    for layout in [RoadLayout::Tee, RoadLayout::Cross, RoadLayout::Wye, RoadLayout::CulDeSac] {
        for edge in LineKind::ALL {
            let m = RoadMeta {
                layout,
                edge,
                points: vec![p(128.0, -64.0, 16.0), p(1152.0, 192.0, 64.0)],
                ..RoadMeta::default()
            };
            let road = built(&m);
            let geometries: Vec<_> = solids(&road).iter().map(|s| hp_geom::validate_solid(s).unwrap()).collect();
            assert!(geometries.iter().all(|g| g.volume() > 0.0));
            for x in (-1100..1600).step_by(91) {
                for y in (-1200..1300).step_by(87) {
                    let xy = DVec2::new(x as f64 + 0.371, y as f64 + 0.217);
                    let dir = (m.points[1].xy() - m.points[0].xy()).normalize();
                    let z = m.points[0].z
                        + (xy - m.points[0].xy()).dot(dir) * 48.0 / m.points[0].xy().distance(m.points[1].xy())
                        - m.thickness * 0.5;
                    let point = xy.extend(z);
                    let count =
                        geometries.iter().filter(|g| g.faces.iter().all(|f| f.plane.distance(point) < -1e-5)).count();
                    assert!(count <= 1, "overlapping brushes for {layout:?}/{edge:?} at {point}");
                }
            }
            let preview = super::preview::mesh(&road, |_| 0);
            assert!(!preview.batches.is_empty());
            assert_eq!(RoadMeta::decode(&m.encode()), Some(m.clone()));
        }
        let mut state = settle();
        let m = RoadMeta { layout, points: vec![p(0.0, 0.0, 0.0), p(1024.0, 0.0, 0.0)], ..RoadMeta::default() };
        let old_count = state.doc.stats().solids;
        let made = apply::create_road(&mut state.doc, &m, 16, Target::World).unwrap();
        let (record, mut loaded) = apply::read_road(&state.doc, made.ids[0]).unwrap();
        assert_eq!(loaded.layout, layout);
        loaded.radius = 192.0;
        loaded.points[1].z = 128.0;
        let changed = apply::replace_road(&mut state.doc, &record, &loaded, 16).unwrap();
        assert_eq!(apply::read_road(&state.doc, changed.ids[0]).unwrap().1.radius, 192.0);
        assert!(state.doc.undo());
        assert!(state.doc.undo());
        assert_eq!(state.doc.stats().solids, old_count);
    }
}

#[test]
fn road_presets_reject_ambiguous_points_invalid_dimensions_and_out_of_map_geometry() {
    use super::model::RoadLayout;
    let mut m = RoadMeta {
        layout: RoadLayout::Cross,
        points: vec![p(0.0, 0.0, 0.0), p(1024.0, 0.0, 0.0)],
        ..RoadMeta::default()
    };
    m.points.push(p(1500.0, 0.0, 0.0));
    assert_eq!(build(&m, &RoadEnv::default()).unwrap_err(), RoadError::LayoutPoints);
    m.points.pop();
    m.radius = 2048.0;
    assert_eq!(build(&m, &RoadEnv::default()).unwrap_err(), RoadError::LayoutSize);
    m.radius = 256.0;
    m.follow_terrain = true;
    assert_eq!(build(&m, &RoadEnv::default()).unwrap_err(), RoadError::LayoutOptions);
    m.follow_terrain = false;
    m.points[0].x += 16000.0;
    m.points[1].x += 16000.0;
    assert_eq!(build(&m, &RoadEnv::default()).unwrap_err(), RoadError::OutOfRange);
    assert_eq!(RoadMeta::decode("r1;pts=0,0,0 512,0,0").unwrap().layout, RoadLayout::Path);
}

#[test]
fn straight_road_has_the_expected_cross_section() {
    let road = built(&meta(&[p(0.0, 0.0, 0.0), p(512.0, 0.0, 0.0)]));
    assert_eq!(road.samples.len(), 9);
    assert_eq!(road.pieces.len(), 8 * 7);
    assert!((road.half_width - 144.0).abs() < 1e-9);
    assert!((road.lane_half_width - 128.0).abs() < 1e-9);
    assert!((road.length - 512.0).abs() < 1e-6);
    let first: Vec<_> = road.pieces.iter().filter(|p| p.segment == 0).collect();
    let (lo, hi) =
        first.iter().flat_map(|p| p.corners.iter()).fold((f64::MAX, f64::MIN), |(a, b), c| (a.min(c.y), b.max(c.y)));
    assert!((hi - lo - 288.0).abs() < 1e-9);
    for piece in &first {
        let top: Vec<f64> = piece.corners[4..].iter().map(|c| c.z).collect();
        assert!(top.iter().all(|z| z.abs() < 1e-9));
        assert!(piece.corners[..4].iter().all(|c| (c.z + 8.0).abs() < 1e-9));
    }
    let roles: Vec<PieceRole> = first.iter().map(|p| p.role).collect();
    assert!(roles.contains(&PieceRole::Shoulder(LateralSide::Left)));
    assert!(roles.contains(&PieceRole::Shoulder(LateralSide::Right)));
    assert_eq!(roles.iter().filter(|r| matches!(r, PieceRole::Lane(_))).count(), 2);
    assert_eq!(roles.iter().filter(|r| matches!(r, PieceRole::Line(_))).count(), 3);
}

#[test]
fn line_kinds_change_the_piece_count() {
    let mut m = meta(&[p(0.0, 0.0, 0.0), p(256.0, 0.0, 0.0)]);
    m.center = LineKind::None;
    m.edge = LineKind::None;
    m.shoulder = 0.0;
    let road = built(&m);
    assert_eq!(road.pieces.iter().filter(|p| p.segment == 0).count(), 1);
    m.center = LineKind::Double;
    m.lanes = 4;
    m.divider = LineKind::Solid;
    let road = built(&m);
    let line_pieces = road.pieces.iter().filter(|p| p.segment == 0 && matches!(p.role, PieceRole::Line(_))).count();
    assert_eq!(line_pieces, 2 + 1 + 1);
    m.line_width = 32.0;
    m.lane_width = 16.0;
    assert_eq!(build(&m, &RoadEnv::default()).unwrap_err(), RoadError::LinesTooWide);
}

#[test]
fn dashed_lines_alternate_materials() {
    let mut m = meta(&[p(0.0, 0.0, 0.0), p(1024.0, 0.0, 0.0)]);
    m.center = LineKind::Dashed;
    m.dash = 64.0;
    m.gap = 64.0;
    m.center_material = "paint/yellow".to_owned();
    let road = built(&m);
    let mats: Vec<&str> = road
        .pieces
        .iter()
        .filter(|p| p.role == PieceRole::Line(super::build::LineRole::Center))
        .map(|p| p.material.as_str())
        .collect();
    assert_eq!(mats.len(), 16);
    assert!(mats.contains(&"paint/yellow"));
    assert!(mats.iter().any(|m| *m == road.meta.material));
    for pair in mats.chunks(2) {
        assert_ne!(pair[0], pair[1]);
    }
    m.center = LineKind::Solid;
    let road = built(&m);
    assert!(
        road.pieces
            .iter()
            .filter(|p| p.role == PieceRole::Line(super::build::LineRole::Center))
            .all(|p| p.material == "paint/yellow")
    );
}

#[test]
fn elevation_is_smooth_and_follows_the_points() {
    let m = meta(&[p(0.0, 0.0, 0.0), p(400.0, 0.0, 100.0), p(800.0, 0.0, 0.0)]);
    let road = built(&m);
    let zs: Vec<f64> = road.samples.iter().map(|s| s.center.z).collect();
    assert!(zs.iter().all(|z| (-1e-6..=100.0 + 1e-6).contains(z)), "{zs:?}");
    let peak = road.sample_at(road.length * 0.5).unwrap();
    assert!((peak.center.z - 100.0).abs() < 2.0, "{}", peak.center.z);
    let max_step = zs.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max);
    assert!(max_step < 30.0);
    assert!(road.length > 800.0, "3D length counts the slope");
}

#[test]
fn textures_run_along_the_road_without_seams() {
    let m = meta(&[p(0.0, 0.0, 0.0), p(300.0, 100.0, 40.0), p(600.0, -50.0, 10.0), p(900.0, 0.0, 0.0)]);
    let road = built(&m);
    let per = road.pieces.iter().filter(|p| p.segment == 0).count();
    let scale = road.meta.tex_scale;
    for i in 0..road.samples.len() - 2 {
        let shared = road.samples[i + 1].center;
        let (a, b) = (&road.pieces[i * per].uv, &road.pieces[(i + 1) * per].uv);
        let ua = shared.dot(a.u_axis) / scale + a.u_shift;
        let ub = shared.dot(b.u_axis) / scale + b.u_shift;
        assert!((ua - ub).abs() < 1e-6, "seam at station {}: {ua} {ub}", i + 1);
        assert!((ub - road.samples[i + 1].s / scale).abs() < 1e-6, "u follows the arc length");
        let va = shared.dot(a.v_axis) / scale + a.v_shift;
        assert!(va.abs() < 1e-6, "v is centered on the road: {va}");
    }
    for piece in road.pieces.iter().take(per * 3) {
        let sides = apply::piece_solid(piece, scale, 16, &mut || 1).unwrap();
        let top = sides.iter().filter(|s| s.normal().z > 0.5).count();
        assert!(top >= 1);
        for s in sides.iter().filter(|s| s.normal().z > 0.5) {
            let along = (s.uaxis.axis.dot(piece.uv.u_axis) - 1.0).abs() < 1e-9;
            assert!(along);
            let c = piece.corners[4];
            let uv = texel_uv(s, c);
            assert!(uv.is_finite());
        }
    }
}

#[test]
fn road_brushes_are_valid_and_convex() {
    let m = meta(&[p(0.0, 0.0, 0.0), p(400.0, 200.0, 60.0), p(800.0, 0.0, 20.0)]);
    let mut m = m;
    m.bank.roll_start = 6.0;
    m.bank.roll_end = -4.0;
    m.bank.auto_gain = 6.0;
    let road = built(&m);
    for sides in solids(&road) {
        hp_geom::validate_solid(&sides).unwrap();
    }
}

#[test]
fn tight_bends_are_refused() {
    let m = meta(&[p(0.0, 0.0, 0.0), p(60.0, 10.0, 0.0), p(0.0, 20.0, 0.0), p(60.0, 30.0, 0.0)]);
    assert!(matches!(build(&m, &RoadEnv::default()), Err(RoadError::Folded(_))));
    assert_eq!(build(&meta(&[p(0.0, 0.0, 0.0)]), &RoadEnv::default()).unwrap_err(), RoadError::TooFewPoints);
    let mut wide = meta(&[p(0.0, 0.0, 0.0), p(900.0, 0.0, 0.0)]);
    wide.lanes = 8;
    wide.lane_width = super::model::MAX_TOTAL_WIDTH / f64::from(wide.lanes);
    wide.shoulder = 16.0;
    assert_eq!(build(&wide, &RoadEnv::default()).unwrap_err(), RoadError::TooWide);
}

#[test]
fn a_single_wide_lane_keeps_one_brush_per_segment_and_roundtrips() {
    let mut m = meta(&[p(0.0, 0.0, 128.0), p(1024.0, 0.0, 256.0)]);
    m.lanes = 1;
    m.shoulder = 0.0;
    m.center = LineKind::None;
    m.edge = LineKind::None;
    m.divider = LineKind::None;
    m.lane_width = 256.0;
    let narrow = built(&m);
    m.lane_width = 4096.0;
    let wide = built(&m);
    assert_eq!(wide.pieces.len(), narrow.pieces.len());
    assert_eq!(wide.pieces.len(), wide.samples.len() - 1);
    assert_eq!(wide.lane_half_width, 2048.0);
    assert_eq!(RoadMeta::decode(&m.encode()).unwrap().lane_width, 4096.0);
    assert_eq!(solids(&wide).len(), wide.pieces.len());
    m.points.iter_mut().for_each(|point| point.y = super::model::MAX_COORD - 8.0);
    assert_eq!(build(&m, &RoadEnv::default()).unwrap_err(), RoadError::OutOfRange);
}

#[test]
fn terrain_following_uses_the_ground_function() {
    let mut m = meta(&[p(0.0, 0.0, 0.0), p(512.0, 0.0, 0.0)]);
    m.follow_terrain = true;
    m.lift = 2.0;
    let ground = |xy: DVec2, _z: f64| Some(xy.x * 0.1);
    let road = build(&m, &RoadEnv { ground: Some(&ground), target: None, ..RoadEnv::default() }).unwrap();
    for s in &road.samples {
        assert!((s.center.z - (s.center.x * 0.1 + 2.0)).abs() < 3.5, "{} {}", s.center.x, s.center.z);
    }
    let rise = road.samples.last().unwrap().center.z - road.samples[0].center.z;
    assert!((rise - 51.2).abs() < 8.0);
}

#[test]
fn junction_cuts_the_end_on_the_side_of_the_other_road() {
    let a = built(&RoadMeta { uid: 1, ..meta(&[p(-1000.0, 0.0, 0.0), p(1000.0, 0.0, 0.0)]) });
    let mut b = meta(&[p(0.0, -800.0, 0.0), p(0.0, -100.0, 0.0)]);
    b.uid = 2;
    b.junction = Some(Junction { end: JunctionEnd::End, target: 1, flare: 48.0, attach: Attach::Outer });
    let road = build(&b, &RoadEnv { ground: None, target: Some(&a), ..RoadEnv::default() }).unwrap();
    let last = road.samples.last().unwrap();
    assert!((last.center.y + 144.0).abs() < 1e-6, "{:?}", last.center);
    assert!((last.left_edge.y + 144.0).abs() < 1e-6 && (last.right_edge.y + 144.0).abs() < 1e-6);
    assert!(!road.flare_pieces().is_empty());
    for side in [LateralSide::Left, LateralSide::Right] {
        let area: f64 = road
            .flare_pieces()
            .into_iter()
            .filter(|p| p.role == PieceRole::Flare(side))
            .map(|p| {
                let t = [p.corners[3].truncate(), p.corners[4].truncate(), p.corners[5].truncate()];
                ((t[1] - t[0]).perp_dot(t[2] - t[0]) * 0.5).abs()
            })
            .sum();
        assert!((area - 48.0 * 48.0 / 6.0).abs() < 25.0, "a flare of 48 fills a sixth of the square: {area}");
    }
    for piece in road.flare_pieces() {
        assert_eq!(piece.corners.len(), 6);
        let sides = apply::piece_solid(piece, 0.25, 16, &mut || 1).unwrap();
        hp_geom::validate_solid(&sides).unwrap();
        for c in &piece.corners[3..] {
            assert!(c.y <= -144.0 + 1e-6, "outside the other road: {c:?}");
            assert!(c.x.abs() >= 144.0 - 1e-6, "outside the branch: {c:?}");
        }
        let min_y = piece.corners.iter().map(|c| c.y).fold(f64::MAX, f64::min);
        assert!(min_y > -144.0 - 200.0);
    }
    let mut oblique = meta(&[p(-600.0, -700.0, 0.0), p(-200.0, -200.0, 0.0)]);
    oblique.uid = 3;
    oblique.junction = Some(Junction { end: JunctionEnd::End, target: 1, flare: 40.0, attach: Attach::Outer });
    let road = build(&oblique, &RoadEnv { ground: None, target: Some(&a), ..RoadEnv::default() }).unwrap();
    let last = road.samples.last().unwrap();
    assert!((last.left_edge.y + 144.0).abs() < 1e-6 && (last.right_edge.y + 144.0).abs() < 1e-6);
    assert!((last.left_edge.x - last.right_edge.x).abs() > 1.0, "the end is cut at an angle");
    assert!(last.center.y < -144.0, "the centerline stops short of the side when the end is cut at an angle");
    let mut selfish = b.clone();
    selfish.uid = 1;
    assert!(matches!(
        build(&selfish, &RoadEnv { ground: None, target: Some(&a), ..RoadEnv::default() }),
        Err(RoadError::Junction(_))
    ));
    b.junction = Some(Junction { end: JunctionEnd::End, target: 1, flare: 0.0, attach: Attach::Outer });
    assert!(
        build(&b, &RoadEnv { ground: None, target: Some(&a), ..RoadEnv::default() }).unwrap().flare_pieces().is_empty()
    );
}

#[test]
fn split_keeps_the_halves_joined() {
    let mut m = meta(&[p(0.0, 0.0, 0.0), p(400.0, 0.0, 40.0), p(800.0, 200.0, 0.0)]);
    m.bank.roll_start = 10.0;
    m.bank.roll_end = -10.0;
    let (a, b) = split_meta(&m, SplitAt::Point(1), 9).unwrap();
    assert_eq!(a.points.len(), 2);
    assert_eq!(b.points.len(), 2);
    assert_eq!(b.uid, 9);
    assert_eq!(a.end_dir, b.start_dir);
    assert_eq!(a.end_slope, b.start_slope);
    assert_eq!(a.bank.roll_end, b.bank.roll_start);
    assert!(b.u0 > 400.0);
    let (ra, rb) = (built(&a), built(&b));
    let end = ra.samples.last().unwrap();
    let start = rb.samples.first().unwrap();
    for (x, y) in [(end.left_edge, start.left_edge), (end.right_edge, start.right_edge), (end.center, start.center)] {
        assert!(x.distance(y) < 1e-6, "{x:?} {y:?}");
    }
    let whole = built(&m);
    assert!((ra.length + rb.length - whole.length).abs() < 40.0);
    assert!(split_meta(&m, SplitAt::Arc(5.0), 9).is_err());
    assert!(split_meta(&m, SplitAt::Point(0), 9).is_err());
    let (c, d) = split_meta(&m, SplitAt::Near(DVec2::new(200.0, 0.0)), 9).unwrap();
    assert_eq!(c.points.len(), 2);
    assert_eq!(d.points.len(), 3);
    let straight = dense(&[DVec2::ZERO, DVec2::new(100.0, 0.0)], None, None);
    assert_eq!(straight.span_of_arc(50.0), 0);
}

#[test]
fn roads_are_created_edited_split_and_deleted_in_single_undo_steps() {
    let mut state = settle();
    let base = state.doc.stats().solids;
    let m = meta(&[p(-200.0, 0.0, 0.0), p(200.0, 0.0, 0.0)]);
    let made = apply::create_road(&mut state.doc, &m, 16, Target::World).unwrap();
    assert_eq!(made.ids.len(), 7 * 7);
    assert_eq!(state.doc.stats().solids, base + made.ids.len());
    assert!(state.doc.undo());
    assert_eq!(state.doc.stats().solids, base);
    assert!(state.doc.redo());
    let roads = apply::list_roads(&state.doc);
    assert_eq!(roads.len(), 1);
    let entry = roads.into_iter().next().unwrap();
    assert_eq!(entry.record.ids.len(), made.ids.len());
    assert_eq!(entry.meta.uid, made.uid);
    let piece = made.ids[3];
    assert!(apply::road_piece_of(&state.doc, piece).is_some());
    assert!(crate::road::apply::read_road(&state.doc, piece).is_some());

    let mut changed = entry.meta.clone();
    changed.lanes = 4;
    changed.points.push(p(600.0, 20.0, 0.0));
    let edited = apply::replace_road(&mut state.doc, &entry.record, &changed, 16).unwrap();
    assert_eq!(apply::list_roads(&state.doc).len(), 1);
    assert_ne!(edited.ids, made.ids);
    assert!(state.doc.undo());
    assert_eq!(apply::list_roads(&state.doc)[0].meta.lanes, 2);
    assert_eq!(state.doc.stats().solids, base + made.ids.len());
    assert!(state.doc.redo());

    let entry = apply::list_roads(&state.doc).into_iter().next().unwrap();
    let (first, second) = apply::split_road(&mut state.doc, &entry.record, &entry.meta, SplitAt::Point(1), 16).unwrap();
    let roads = apply::list_roads(&state.doc);
    assert_eq!(roads.len(), 2);
    assert_ne!(first.uid, second.uid);
    assert!(state.doc.undo());
    assert_eq!(apply::list_roads(&state.doc).len(), 1);
    assert!(state.doc.redo());

    let second = apply::list_roads(&state.doc).into_iter().find(|e| e.meta.uid == second.uid).unwrap();
    apply::delete_road(&mut state.doc, &second.record, 16).unwrap();
    assert_eq!(apply::list_roads(&state.doc).len(), 1);
    assert!(state.doc.undo());
    assert_eq!(apply::list_roads(&state.doc).len(), 2);
}

#[test]
fn roads_follow_the_ground_of_the_map() {
    let mut state = settle();
    let mut m = meta(&[p(-200.0, -120.0, 500.0), p(200.0, -120.0, 500.0)]);
    m.follow_terrain = true;
    m.lift = 3.0;
    let road = apply::build_in_doc(&state.doc, &m, &[]).unwrap();
    for s in &road.samples {
        assert!((s.center.z - 3.0).abs() < 1e-6, "{}", s.center.z);
    }
    let made = apply::create_road(&mut state.doc, &m, 16, Target::World).unwrap();
    let again = apply::build_in_doc(&state.doc, &m, &made.ids).unwrap();
    assert!((again.samples[0].center.z - 3.0).abs() < 1e-6, "the road ignores its own brushes");
}

#[test]
fn junctions_follow_the_other_road_when_it_changes() {
    let mut state = settle();
    let a = apply::create_road(&mut state.doc, &meta(&[p(-250.0, 0.0, 0.0), p(250.0, 0.0, 0.0)]), 16, Target::World)
        .unwrap();
    let mut b = meta(&[p(0.0, -240.0, 0.0), p(0.0, -170.0, 0.0)]);
    b.junction = Some(Junction { end: JunctionEnd::End, target: a.uid, flare: 32.0, attach: Attach::Outer });
    let b_made = apply::create_road(&mut state.doc, &b, 16, Target::World).unwrap();
    let b_entry = apply::find_road(&state.doc, b_made.uid).unwrap();
    let road_b = apply::build_in_doc(&state.doc, &b_entry.meta, &b_entry.record.ids).unwrap();
    assert!((road_b.samples.last().unwrap().center.y + 144.0).abs() < 1e-6);
    let a_entry = apply::find_road(&state.doc, a.uid).unwrap();
    let mut wider = a_entry.meta.clone();
    wider.shoulder = 48.0;
    let made = apply::replace_road(&mut state.doc, &a_entry.record, &wider, 16).unwrap();
    assert_eq!(made.refreshed, vec![b_made.uid]);
    assert!(made.stale.is_empty());
    let b_entry = apply::find_road(&state.doc, b_made.uid).unwrap();
    let road_b = apply::build_in_doc(&state.doc, &b_entry.meta, &b_entry.record.ids).unwrap();
    assert!((road_b.samples.last().unwrap().center.y + 176.0).abs() < 1e-6);
    assert!(state.doc.undo());
    let b_entry = apply::find_road(&state.doc, b_made.uid).unwrap();
    let road_b = apply::build_in_doc(&state.doc, &b_entry.meta, &b_entry.record.ids).unwrap();
    assert!((road_b.samples.last().unwrap().center.y + 144.0).abs() < 1e-6);
    let again = apply::set_junction(&mut state.doc, &b_entry.record, &b_entry.meta, None, 16).unwrap();
    assert!(apply::find_road(&state.doc, again.uid).unwrap().meta.junction.is_none());
}

#[test]
fn api_lists_the_pieces_of_a_road() {
    let road = built(&meta(&[p(0.0, 0.0, 0.0), p(256.0, 0.0, 0.0)]));
    assert_eq!(road.centerline().len(), 5);
    assert!((road.total_width() - 288.0).abs() < 1e-9);
    assert!((road.shoulder_width() - 16.0).abs() < 1e-9);
    assert_eq!(road.shoulder_pieces(LateralSide::Left).len(), 4);
    assert_eq!(road.shoulder_pieces(LateralSide::Right).len(), 4);
    assert_eq!(road.lane_pieces().len(), 8);
    assert_eq!(road.line_pieces().len(), 12);
    let e = road.edge(LateralSide::Left, false);
    assert!(e.iter().all(|p| (p.y - 144.0).abs() < 1e-9));
    let mid = road.sample_at(128.0).unwrap();
    assert!((mid.center.x - 128.0).abs() < 1e-6);
    assert_eq!(mid.tangent, DVec3::X);
}

#[test]
fn tool_options_round_trip_and_validate() {
    use hp_app::Tool;
    let mut state = settle();
    let mut tool = super::RoadTool::new();
    assert!(tool.set_option(&mut state, "lanes", "4"));
    assert!(!tool.set_option(&mut state, "lanes", "9"));
    assert!(!tool.set_option(&mut state, "lanes", "2.5"));
    assert!(tool.set_option(&mut state, "center_line", "double"));
    assert!(!tool.set_option(&mut state, "center_line", "wavy"));
    assert!(tool.set_option(&mut state, "follow_terrain", "on"));
    assert!(tool.set_option(&mut state, "create_as", "func_detail"));
    assert!(tool.set_option(&mut state, "material", "road/asphalt"));
    let values = tool.option_values();
    let get = |k: &str| values.iter().find(|o| o.key == k).unwrap().value.clone();
    assert_eq!(get("lanes"), "4");
    assert_eq!(get("center_line"), "double");
    assert_eq!(get("follow_terrain"), "on");
    assert_eq!(get("material"), "road/asphalt");
    assert_eq!(get("create_as"), "func_detail");
}

#[test]
fn tool_builds_and_applies_a_road() {
    let mut state = settle();
    let base = state.doc.stats().solids;
    let mut tool = super::RoadTool::new();
    assert!(tool.add_point(p(-200.0, 0.0, 0.0)));
    assert!(tool.create(&mut state).is_none());
    assert!(tool.add_point(p(200.0, 0.0, 0.0)));
    assert!(tool.insert_point_near(DVec2::new(0.0, 5.0)));
    assert_eq!(tool.points().len(), 3);
    assert!(tool.set_point_height(1, 10.0));
    assert!(tool.preview(&state).is_ok());
    let made = tool.create(&mut state).unwrap();
    assert!(state.doc.stats().solids > base);
    assert_eq!(made.ids.len(), state.doc.stats().solids - base);
    assert!(tool.points().is_empty());
    assert!(state.doc.undo());
    assert_eq!(state.doc.stats().solids, base);
}

#[test]
fn translations_are_clean() {
    let problems = hp_app::i18n::lint_table(super::ROAD_TR);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn width_fit_resolves_a_tight_bend_without_moving_the_path() {
    use hp_app::Tool;
    let mut state = settle();
    let mut tool = super::RoadTool::new();
    let points = [p(0.0, 0.0, 0.0), p(240.0, 0.0, 0.0), p(240.0, 240.0, 0.0)];
    for point in points {
        tool.add_point(point);
    }
    if tool.preview(&state).is_ok() {
        tool.set_option(&mut state, "lane_width", "256");
        tool.set_option(&mut state, "shoulder", "128");
    }
    assert!(matches!(&*tool.preview(&state), Err(RoadError::Folded(_))));
    assert!(tool.fit_width_to_bends(&state));
    assert_eq!(tool.points(), points);
    assert!(tool.preview(&state).is_ok());
    assert!(tool.options.meta.lane_width >= 16.0);
    assert!(tool.option_values().iter().any(|o| o.key == "fit_width"));
}

#[test]
fn roads_ignore_every_road_as_ground() {
    let mut state = settle();
    let mut a = meta(&[p(-200.0, -120.0, 0.0), p(200.0, -120.0, 0.0)]);
    a.follow_terrain = true;
    a.lift = 3.0;
    apply::create_road(&mut state.doc, &a, 16, Target::World).unwrap();
    let mut b = meta(&[p(0.0, -220.0, 0.0), p(0.0, -20.0, 0.0)]);
    b.follow_terrain = true;
    b.lift = 3.0;
    let road = apply::build_in_doc(&state.doc, &b, &[]).unwrap();
    for s in &road.samples {
        assert!((s.center.z - 3.0).abs() < 1e-6, "{}", s.center.z);
    }
}

#[test]
fn lane_attach_cuts_the_shoulder_away_under_the_joined_road() {
    let mut state = settle();
    let a = apply::create_road(&mut state.doc, &meta(&[p(-400.0, 0.0, 0.0), p(400.0, 0.0, 0.0)]), 16, Target::World)
        .unwrap();
    let (undo, _) = state.doc.history_len();
    let mut b = meta(&[p(0.0, -400.0, 0.0), p(0.0, -170.0, 0.0)]);
    b.junction = Some(Junction { end: JunctionEnd::End, target: a.uid, flare: 0.0, attach: Attach::Lane });
    let made = apply::create_road(&mut state.doc, &b, 16, Target::World).unwrap();
    assert_eq!(state.doc.history_len().0, undo + 1);
    let entry = apply::find_road(&state.doc, a.uid).unwrap();
    let road = apply::build_in_doc(&state.doc, &entry.meta, &entry.record.ids).unwrap();
    let shoulder = road.shoulder_pieces(LateralSide::Right);
    assert!(!shoulder.is_empty());
    for piece in &shoulder {
        let (lo, hi) = piece.corners.iter().fold((f64::MAX, f64::MIN), |(l, h), c| (l.min(c.x), h.max(c.x)));
        assert!(hi <= -144.0 + 1e-6 || lo >= 144.0 - 1e-6, "shoulder under the branch: {lo} {hi}");
    }
    let spans: f64 = shoulder
        .iter()
        .map(|p| {
            p.corners.iter().map(|c| c.x).fold(f64::MIN, f64::max)
                - p.corners.iter().map(|c| c.x).fold(f64::MAX, f64::min)
        })
        .sum();
    assert!(spans > 400.0, "the shoulder stays elsewhere: {spans}");
    let outer = apply::find_road(&state.doc, made.uid).unwrap();
    apply::delete_road(&mut state.doc, &outer.record, 16).unwrap();
    let entry = apply::find_road(&state.doc, a.uid).unwrap();
    let road = apply::build_in_doc(&state.doc, &entry.meta, &entry.record.ids).unwrap();
    assert_eq!(road.shoulder_pieces(LateralSide::Right).len(), 13);
}

#[test]
fn split_halves_keep_settings_heights_texture_run_and_junctions() {
    let mut state = settle();
    let mut m = meta(&[p(-240.0, -150.0, 0.0), p(0.0, -150.0, 30.0), p(240.0, -120.0, 10.0)]);
    m.lanes = 3;
    m.shoulder = 10.0;
    m.material = "concrete/concretefloor001a".to_owned();
    m.center = LineKind::Double;
    m.edge = LineKind::Dashed;
    m.bank.roll_start = 4.0;
    m.bank.roll_end = -4.0;
    let a = apply::create_road(&mut state.doc, &m, 16, Target::World).unwrap();
    let mut branch = meta(&[p(100.0, 400.0, 0.0), p(100.0, 200.0, 0.0)]);
    branch.lanes = 1;
    branch.lane_width = 64.0;
    branch.shoulder = 0.0;
    branch.junction = Some(Junction { end: JunctionEnd::End, target: a.uid, flare: 24.0, attach: Attach::Outer });
    let branch_made = apply::create_road(&mut state.doc, &branch, 16, Target::World).unwrap();
    let (undo, _) = state.doc.history_len();
    let entry = apply::find_road(&state.doc, a.uid).unwrap();
    let (first, second) = apply::split_road(&mut state.doc, &entry.record, &entry.meta, SplitAt::Point(1), 16).unwrap();
    assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
    let ra = apply::find_road(&state.doc, first.uid).unwrap();
    let rb = apply::find_road(&state.doc, second.uid).unwrap();
    for h in [&ra.meta, &rb.meta] {
        assert_eq!((h.lanes, h.shoulder, h.center, h.edge), (3, 10.0, LineKind::Double, LineKind::Dashed));
        assert_eq!(h.material, "concrete/concretefloor001a");
        assert_eq!(h.line_material, m.line_material);
    }
    let (ba, bb) = (
        apply::build_in_doc(&state.doc, &ra.meta, &ra.record.ids).unwrap(),
        apply::build_in_doc(&state.doc, &rb.meta, &rb.record.ids).unwrap(),
    );
    let (end, start) = (ba.samples.last().unwrap(), bb.samples.first().unwrap());
    assert!(end.center.distance(start.center) < 1e-6 && (end.roll - start.roll).abs() < 1e-6);
    assert!(end.left_edge.distance(start.left_edge) < 1e-6 && end.right_edge.distance(start.right_edge) < 1e-6);
    assert!((rb.meta.u0 - (ra.meta.u0 + ba.length)).abs() < 1e-6, "the texture run goes on");
    assert!((bb.samples.first().unwrap().s - 0.0).abs() < 1e-9 && (start.center.z - 30.0).abs() < 1e-6);
    let joined = apply::find_road(&state.doc, branch_made.uid).unwrap();
    let target = joined.meta.junction.unwrap().target;
    assert_eq!(target, second.uid, "the branch follows the half it touches");
    apply::build_in_doc(&state.doc, &joined.meta, &joined.record.ids).unwrap();
    assert!(state.doc.undo());
    assert_eq!(apply::list_roads(&state.doc).len(), 2);
    assert_eq!(apply::find_road(&state.doc, branch_made.uid).unwrap().meta.junction.unwrap().target, a.uid);
}
