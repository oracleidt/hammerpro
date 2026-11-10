use glam::{DMat4, DQuat, DVec2, DVec3};
use hp_geom::{
    Aabb, BevelParams, ExtrudeMode, FaceExtrudeParams, InsetParams, Justify, Plane, Primitive, PrimitiveParams,
    RiseDirection, bevel_solid, brush_edges, build_solid, carve, clip_solid, face_extrude, face_inset, face_prism,
    fit_texture, hollow, intersect_traced, justify, make_primitive, merge_convex, rebuild_from_points, transform_sides,
    union_pieces, union_volume, unique_vertices, wrap_texture,
};
use hp_vmf::Side;

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
    fn vec(&mut self, half: f64) -> DVec3 {
        DVec3::new(self.range(-half, half), self.range(-half, half), self.range(-half, half))
    }
    fn quat(&mut self) -> DQuat {
        DQuat::from_axis_angle(self.vec(1.0).normalize_or(DVec3::Z), self.range(0.0, std::f64::consts::TAU))
    }
}

fn ids() -> impl FnMut() -> u32 {
    let mut n = 100_000;
    move || {
        n += 1;
        n
    }
}

fn block(min: DVec3, max: DVec3, next: &mut dyn FnMut() -> u32) -> Vec<Side> {
    let p = PrimitiveParams { kind: Primitive::Block, bounds: Aabb::new(min, max), ..Default::default() };
    make_primitive(&p, next).expect("block").remove(0)
}

fn assert_valid(sides: &[Side], what: &str) -> f64 {
    let g = build_solid(sides);
    assert!(g.valid, "{what}: invalid solid");
    assert!(g.vertices().all(|v| v.is_finite()), "{what}: non finite vertex");
    for s in sides {
        for p in s.plane {
            assert!(p.is_finite(), "{what}: non finite plane point");
        }
        assert!(s.uaxis.axis.is_finite() && s.vaxis.axis.is_finite(), "{what}: non finite texture axis");
        assert!(s.uaxis.scale.is_finite() && s.vaxis.scale.is_finite(), "{what}: non finite texture scale");
        assert!(s.uaxis.shift.is_finite() && s.vaxis.shift.is_finite(), "{what}: non finite texture shift");
    }
    g.volume()
}

fn unique_ids(pieces: &[Vec<Side>]) -> bool {
    let mut all: Vec<u32> = pieces.iter().flatten().map(|s| s.id).collect();
    let n = all.len();
    all.sort_unstable();
    all.dedup();
    all.len() == n
}

#[test]
fn primitives_at_every_size_orientation_and_count_are_valid_or_refused() {
    let mut next = ids();
    let sizes = [
        DVec3::new(0.02, 0.02, 0.02),
        DVec3::new(0.5, 0.5, 0.5),
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(3.0, 2.0, 1.0),
        DVec3::new(64.0, 64.0, 0.02),
        DVec3::new(4096.0, 8.0, 8.0),
        DVec3::new(32768.0, 32768.0, 32768.0),
        DVec3::new(65536.0, 65536.0, 16.0),
    ];
    let mut made = 0;
    for kind in Primitive::ALL {
        for size in sizes {
            for offset in [DVec3::ZERO, DVec3::new(16384.0, -16384.0, 0.33)] {
                for sides in [3, 4, 7, 64, 128] {
                    for (up, flip) in [(2, false), (0, false), (1, true)] {
                        let params = PrimitiveParams {
                            kind,
                            sides,
                            bounds: Aabb::new(offset, offset + size),
                            up_axis: up,
                            up_flip: flip,
                            stair_steps: sides.min(256),
                            door_arch: sides % 2 == 0,
                            door_arch_segments: sides.clamp(2, 64),
                            direction: RiseDirection::ALL[sides as usize % RiseDirection::ALL.len()],
                            ..Default::default()
                        };
                        let what = format!("{kind:?} {size} at {offset} sides {sides} up {up} flip {flip}");
                        let Ok(solids) = make_primitive(&params, &mut next) else { continue };
                        assert!(!solids.is_empty(), "{what}: no solid");
                        assert!(unique_ids(&solids), "{what}: duplicate side ids");
                        let bounds = params.bounds.expand(1.0);
                        for s in &solids {
                            assert_valid(s, &what);
                            assert!(bounds.contains_aabb(&build_solid(s).aabb), "{what}: outside its box");
                        }
                        made += 1;
                    }
                }
            }
        }
    }
    assert!(made > 1000, "{made}");
}

#[test]
fn primitives_with_hostile_parameters_never_panic() {
    let mut next = ids();
    let mut rng = Rng(0x9e1);
    let hostile =
        [0.0, -1.0, 1e-9, 0.5, 1.0, 45.0, 179.9, 180.0, 359.9, 360.0, 720.0, 1e6, -1e6, f64::NAN, f64::INFINITY];
    for round in 0..4000 {
        let mut h = || hostile[rng.below(hostile.len())];
        let params = PrimitiveParams {
            kind: Primitive::ALL[round % Primitive::ALL.len()],
            bounds: Aabb::new(DVec3::ZERO, DVec3::new(128.0, 96.0, 64.0)),
            sides: [0, 1, 2, 3, 5, 128, 129, u32::MAX][round % 8],
            arch_wall_width: h(),
            arch_arc_degrees: h(),
            arch_start_angle: h(),
            arch_add_height: h(),
            torus_ring_width: h(),
            torus_tube_sides: [0, 3, 200][round % 3],
            torus_rotation_arc: h(),
            torus_rotation_start: h(),
            torus_rotation_height: h(),
            stair_steps: [0, 1, 300, u32::MAX][round % 4],
            stair_step_height: h(),
            stair_open: round % 2 == 0,
            spiral_inner: h(),
            spiral_angle: h(),
            spiral_start: h(),
            ramp_angle: h(),
            pillar_base: h(),
            pillar_capital: h(),
            pillar_inset: h(),
            door_width: h(),
            door_height: h(),
            door_thickness: h(),
            door_arch: round % 3 == 0,
            door_arch_radius: h(),
            door_arch_segments: [0, 1, 2, 65, u32::MAX][round % 5],
            tex_scale: h(),
            ..Default::default()
        };
        let what = format!("round {round} {params:?}");
        if let Ok(solids) = make_primitive(&params, &mut next) {
            for s in &solids {
                assert_valid(s, &what);
            }
        }
    }
}

#[test]
fn clips_through_vertices_edges_and_faces_conserve_volume() {
    let mut next = ids();
    let mut rng = Rng(0xc11);
    for round in 0..400 {
        let mut sides = block(DVec3::ZERO, DVec3::new(64.0, 32.0, 16.0), &mut next);
        if round % 2 == 1 {
            let m = DMat4::from_rotation_translation(rng.quat(), rng.vec(1000.0));
            transform_sides(&mut sides, m, true, false, None).expect("transform");
        }
        let g = build_solid(&sides);
        let (verts, _) = unique_vertices(&g);
        let a = verts[rng.below(verts.len())];
        let b = verts[rng.below(verts.len())];
        let normal = match round % 4 {
            0 => (b - a).normalize_or(DVec3::X),
            1 => g.faces[rng.below(g.faces.len())].plane.normal,
            2 => rng.vec(1.0).normalize_or(DVec3::Z),
            _ => (b - a).cross(rng.vec(1.0)).normalize_or(DVec3::Y),
        };
        let Some(plane) = Plane::from_normal_point(normal, a) else { continue };
        let v = g.volume();
        let (front, back) = clip_solid(&sides, &plane, &mut next).expect("clip");
        let vf = front.as_deref().map_or(0.0, |s| assert_valid(s, "front"));
        let vb = back.as_deref().map_or(0.0, |s| assert_valid(s, "back"));
        assert!((vf + vb - v).abs() < 1e-6 * v.max(1.0), "round {round}: {vf} + {vb} != {v}");
    }
}

#[test]
fn degenerate_carves() {
    let mut next = ids();
    let target = block(DVec3::ZERO, DVec3::splat(64.0), &mut next);
    assert!(carve(&target, &target, &mut next).expect("self").is_empty(), "a solid carved by itself vanishes");
    let around = block(DVec3::splat(-8.0), DVec3::splat(72.0), &mut next);
    assert!(carve(&target, &around, &mut next).expect("inside").is_empty());
    let touching = block(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0), &mut next);
    assert_eq!(carve(&target, &touching, &mut next).expect("touching"), vec![target.clone()]);
    let tiny = block(DVec3::splat(31.99), DVec3::splat(32.01), &mut next);
    let pieces = carve(&target, &tiny, &mut next).expect("tiny");
    let left: f64 = pieces.iter().map(|p| assert_valid(p, "tiny carve")).sum();
    assert!((left - 64f64.powi(3)).abs() < 1.0, "{left}");
    let mut rng = Rng(0xca);
    for round in 0..300 {
        let mut carver = block(DVec3::splat(-1.0), DVec3::splat(1.0) * rng.range(0.01, 80.0), &mut next);
        let m = DMat4::from_rotation_translation(rng.quat(), DVec3::splat(32.0) + rng.vec(40.0));
        transform_sides(&mut carver, m, false, false, None).expect("transform");
        let Ok(pieces) = carve(&target, &carver, &mut next) else { continue };
        let mut all = pieces.clone();
        all.push(carver.clone());
        let parts: f64 = pieces.iter().map(|p| assert_valid(p, &format!("carve {round}"))).sum();
        let mut both = target.clone();
        both.extend(carver.iter().cloned());
        let inter = { if build_solid(&both).valid { build_solid(&both).volume() } else { 0.0 } };
        let slivers = hp_geom::PLANE_EPS * build_solid(&target).surface_area();
        assert!((parts + inter - 64f64.powi(3)).abs() < slivers, "round {round}: {parts} + {inter}");
        assert!(unique_ids(&pieces), "round {round}: ids");
    }
}

#[test]
fn hollow_any_shape_with_any_thickness() {
    let mut next = ids();
    let mut rng = Rng(0x401);
    for kind in [Primitive::Block, Primitive::Wedge, Primitive::Cylinder, Primitive::Spike, Primitive::Sphere] {
        for round in 0..40 {
            let size = DVec3::new(rng.range(1.0, 512.0), rng.range(1.0, 512.0), rng.range(1.0, 512.0));
            let params = PrimitiveParams {
                kind,
                sides: 3 + rng.below(20) as u32,
                bounds: Aabb::new(DVec3::ZERO, size),
                ..Default::default()
            };
            let Ok(solids) = make_primitive(&params, &mut next) else { continue };
            let mut sides = solids[0].clone();
            if round % 3 == 0 {
                let m = DMat4::from_rotation_translation(rng.quat(), rng.vec(2000.0));
                transform_sides(&mut sides, m, true, false, None).expect("transform");
            }
            let outer = assert_valid(&sides, "outer");
            for t in [rng.range(-64.0, 64.0), 1e-4, -1e-4, 0.5, 1e9, -1e9, f64::NAN, f64::INFINITY] {
                let what = format!("{kind:?} {size} round {round} thickness {t}");
                if let Ok(walls) = hollow(&sides, t, &mut next) {
                    let total: f64 = walls.iter().map(|w| assert_valid(w, &what)).sum();
                    assert!(unique_ids(&walls), "{what}: ids");
                    if t > 0.0 {
                        assert!(total <= outer + 1e-6 * outer.max(1.0), "{what}: {total} > {outer}");
                    } else {
                        assert!(total > 0.0, "{what}");
                    }
                }
            }
        }
    }
}

#[test]
fn union_and_intersect_of_random_sets_keep_the_volume() {
    let mut next = ids();
    let mut rng = Rng(0xb00);
    for round in 0..60 {
        let n = 2 + rng.below(6);
        let solids: Vec<Vec<Side>> = (0..n)
            .map(|_| {
                let mut b = block(DVec3::ZERO, DVec3::new(rng.range(8.0, 96.0), 48.0, rng.range(8.0, 64.0)), &mut next);
                let m = if round % 2 == 0 {
                    DMat4::from_translation(rng.vec(64.0).round())
                } else {
                    DMat4::from_rotation_translation(rng.quat(), rng.vec(64.0))
                };
                transform_sides(&mut b, m, false, false, None).expect("transform");
                b
            })
            .collect();
        let expected = union_volume(&solids).expect("volume");
        let parts = union_pieces(&solids, &mut next).expect("union");
        let total: f64 = parts.iter().map(|p| assert_valid(&p.sides, "union part")).sum();
        assert!((total - expected).abs() < 1e-4 * expected.max(1.0), "round {round}: {total} != {expected}");
        let lists: Vec<Vec<Side>> = parts.iter().map(|p| p.sides.clone()).collect();
        assert!(unique_ids(&lists), "round {round}: ids");
        for p in &parts {
            assert_eq!(p.sides.len(), p.sources.len());
            assert!(p.origin < n);
        }
        if let Ok(Some(piece)) = intersect_traced(&solids[0], &solids[1], &mut next) {
            let v = assert_valid(&piece.sides, "intersection");
            let mut both = solids[0].clone();
            both.extend(solids[1].iter().cloned());
            assert!((v - build_solid(&both).volume()).abs() < 1e-4 * v.max(1.0));
        }
    }
}

#[test]
fn merge_and_rebuild_degenerate_inputs() {
    let mut next = ids();
    let a = block(DVec3::ZERO, DVec3::splat(64.0), &mut next);
    let merged = merge_convex(&[a.clone(), a.clone()], &mut next).expect("same");
    assert!((assert_valid(&merged, "same") - 64f64.powi(3)).abs() < 1e-6);
    assert!(merge_convex(&[], &mut next).is_err());
    let flat: Vec<DVec3> = (0..8).map(|i| DVec3::new(f64::from(i % 4) * 16.0, f64::from(i / 4) * 16.0, 0.0)).collect();
    assert!(rebuild_from_points(&flat, &a, &mut next).is_err(), "coplanar points");
    let dup = vec![DVec3::ZERO; 12];
    assert!(rebuild_from_points(&dup, &a, &mut next).is_err(), "one point");
    let nan = vec![DVec3::NAN, DVec3::X, DVec3::Y, DVec3::Z, DVec3::ONE];
    assert!(rebuild_from_points(&nan, &a, &mut next).is_err(), "not finite");
}

#[test]
fn bevel_inset_and_extrude_with_hostile_parameters() {
    let mut next = ids();
    let mut rng = Rng(0xbe1);
    let shapes: Vec<Vec<Side>> = [Primitive::Block, Primitive::Wedge, Primitive::Cylinder, Primitive::Spike]
        .iter()
        .map(|kind| {
            let p = PrimitiveParams {
                kind: *kind,
                sides: 6,
                bounds: Aabb::new(DVec3::ZERO, DVec3::new(128.0, 96.0, 64.0)),
                ..Default::default()
            };
            make_primitive(&p, &mut next).expect("shape").remove(0)
        })
        .collect();
    let values = [0.0, 1e-6, 0.01, 0.5, 4.0, 16.0, 47.9, 48.0, 64.0, 1000.0, 1e9, -8.0, f64::NAN, f64::INFINITY];
    for round in 0..600 {
        let mut sides = shapes[round % shapes.len()].clone();
        if round % 5 == 0 {
            let m = DMat4::from_rotation_translation(rng.quat(), rng.vec(500.0));
            transform_sides(&mut sides, m, true, false, None).expect("transform");
        }
        let edges = brush_edges(&sides);
        let picked: Vec<[DVec3; 2]> = (0..1 + rng.below(3)).map(|_| edges[rng.below(edges.len())]).collect();
        let v = values[rng.below(values.len())];
        let what = format!("round {round} value {v}");
        let params = BevelParams { radius: v, segments: [0, 1, 3, 32, 1000][round % 5], ..Default::default() };
        if let Ok(r) = bevel_solid(&sides, &picked, &[], &params, &mut next) {
            for p in &r.pieces {
                assert_valid(&p.sides, &format!("bevel {what}"));
            }
            for a in &r.added {
                assert_valid(a, &format!("bevel added {what}"));
            }
        }
        let face = rng.below(sides.len());
        let inset = InsetParams { distance: v, depth: values[rng.below(values.len())], ..Default::default() };
        if let Ok(pieces) = face_inset(&sides, face, &inset, &mut next) {
            for p in &pieces {
                assert_valid(&p.sides, &format!("inset {what}"));
            }
        }
        let g = build_solid(&sides);
        if let Some(f) = g.face_for_side(face) {
            let c = f.center();
            let outline: Vec<DVec3> = f.vertices.iter().map(|p| c.lerp(*p, rng.range(0.0, 1.2))).collect();
            for mode in [ExtrudeMode::Carve, ExtrudeMode::Extrude] {
                let params = FaceExtrudeParams { mode, depth: v, through: round % 2 == 0, material: None };
                if let Ok(r) = face_extrude(&sides, face, &outline, &params, &mut next) {
                    for p in &r.pieces {
                        assert_valid(&p.sides, &format!("extrude {what}"));
                    }
                    for a in r.added.iter().chain(&r.prism) {
                        assert_valid(a, &format!("extrude added {what}"));
                    }
                }
            }
            if let Ok(prism) = face_prism(&sides, face, v, None, &mut next) {
                assert_valid(&prism, &format!("prism {what}"));
            }
        }
    }
}

#[test]
fn texture_tools_on_degenerate_faces_stay_finite() {
    let mut next = ids();
    let mut sides = block(DVec3::ZERO, DVec3::new(64.0, 0.5, 300.0), &mut next);
    sides[0].uaxis.axis = DVec3::ZERO;
    sides[0].uaxis.scale = 0.0;
    sides[1].vaxis.scale = f64::NAN;
    sides[2].uaxis.axis = sides[2].vaxis.axis;
    let g = build_solid(&sides);
    for face in &g.faces {
        let pts = &face.vertices;
        let src = sides[face.side_index].clone();
        for (w, h) in [(0, 0), (1, 1), (512, 512), (u32::MAX, 3)] {
            for mode in [Justify::Left, Justify::Right, Justify::Top, Justify::Bottom, Justify::Center, Justify::Fit] {
                let mut s = src.clone();
                justify(&mut s, pts, w, h, mode);
                assert!(s.uaxis.shift.is_finite() && s.vaxis.shift.is_finite(), "justify {mode:?} {w}x{h}");
                assert!(s.uaxis.scale.is_finite() && s.vaxis.scale.is_finite(), "justify scale {mode:?} {w}x{h}");
            }
            for (ru, rv) in [(1.0, 1.0), (0.0, 0.0), (-2.0, 3.0), (f64::NAN, 1.0), (1e12, 1e-12)] {
                let mut s = src.clone();
                fit_texture(&mut s, pts, w, h, ru, rv);
                assert!(
                    s.uaxis.shift.is_finite()
                        && s.vaxis.shift.is_finite()
                        && s.uaxis.scale.is_finite()
                        && s.vaxis.scale.is_finite(),
                    "fit {ru} {rv} {w}x{h}"
                );
            }
        }
        for other in &g.faces {
            let mut dst = sides[other.side_index].clone();
            wrap_texture(&src, face.plane.normal, &mut dst, other.plane.normal, face.vertices[0]);
            assert!(dst.uaxis.axis.is_finite() && dst.vaxis.axis.is_finite(), "wrap axes");
            assert!(dst.uaxis.shift.is_finite() && dst.vaxis.shift.is_finite(), "wrap shift");
        }
    }
}

#[test]
fn singular_and_mirroring_transforms() {
    let mut next = ids();
    let sides = block(DVec3::ZERO, DVec3::splat(64.0), &mut next);
    for m in [
        DMat4::from_scale(DVec3::new(1.0, 1.0, 0.0)),
        DMat4::from_scale(DVec3::NAN),
        DMat4::from_cols_array(&[0.0; 16]),
    ] {
        let mut s = sides.clone();
        assert!(transform_sides(&mut s, m, true, false, None).is_err(), "{m}");
    }
    for m in [
        DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)),
        DMat4::from_scale(DVec3::new(-1.0, -1.0, -1.0)),
        DMat4::from_scale(DVec3::new(1e-3, 1e3, 1.0)),
        DMat4::from_scale_rotation_translation(DVec3::new(-2.0, 1.0, 0.5), DQuat::from_rotation_z(0.3), DVec3::X),
    ] {
        let mut s = sides.clone();
        transform_sides(&mut s, m, true, false, None).expect("transform");
        assert_valid(&s, &format!("{m}"));
        let mut s = sides.clone();
        transform_sides(&mut s, m, true, true, None).expect("scale lock");
        assert_valid(&s, &format!("scale lock {m}"));
    }
}

#[test]
fn displacements_with_hostile_values_stay_finite() {
    let quad = [DVec3::ZERO, DVec3::new(0.0, 128.0, 0.0), DVec3::new(128.0, 128.0, 0.0), DVec3::new(128.0, 0.0, 0.0)];
    for power in 0..6 {
        let r = hp_geom::create_disp(&quad, power);
        if !(1..=4).contains(&power) {
            assert!(r.is_err(), "power {power}");
            continue;
        }
        let mut disp = r.expect("disp");
        for factor in [0.0, -1.0, 1e12, f64::NAN, f64::INFINITY] {
            let _ = hp_geom::scale_disp(&mut disp, &quad, factor);
            assert!(disp.distances.iter().all(|d| d.is_finite()), "scale {factor}");
        }
        let modes = [
            hp_geom::PaintMode::Raise,
            hp_geom::PaintMode::Lower,
            hp_geom::PaintMode::Smooth,
            hp_geom::PaintMode::Set,
            hp_geom::PaintMode::AlphaPaint,
        ];
        for mode in modes {
            for (radius, amount) in [(0.0, 1.0), (f64::NAN, 1.0), (64.0, f64::NAN), (64.0, 1e12), (1e12, 1.0)] {
                for center in [DVec3::new(64.0, 64.0, 0.0), DVec3::NAN, DVec3::splat(1e300)] {
                    let brush = hp_geom::PaintBrush {
                        center,
                        radius,
                        amount,
                        mode,
                        falloff: hp_geom::Falloff::Smooth,
                        spatial: radius > 1.0,
                    };
                    let _ = hp_geom::disp_paint(&mut disp, &quad, &brush);
                    assert!(
                        disp.distances.iter().chain(&disp.alphas).all(|d| d.is_finite()),
                        "paint {mode:?} {radius} {amount} {center}"
                    );
                }
            }
        }
        let mesh = hp_geom::build_disp(&quad, &disp).expect("mesh");
        assert!(mesh.positions.iter().all(|p| p.is_finite()));
    }
    let tri = [DVec3::ZERO, DVec3::X, DVec3::Y];
    assert!(hp_geom::create_disp(&tri, 2).is_err());
}

#[test]
fn terrain_fields_of_any_size() {
    for (w, h) in [(0, 0), (1, 1), (1, 7), (2, 2), (129, 3)] {
        for preset in hp_geom::TerrainPreset::ALL {
            let params = hp_geom::GenParams::preset(preset, 7);
            let f = hp_geom::generate_field(&params, w, h, (1024.0, 512.0));
            let (lo, hi) = f.min_max();
            assert!(lo.is_finite() || w * h == 0, "{preset:?} {w}x{h}");
            assert!(hi.is_finite() || w * h == 0, "{preset:?} {w}x{h}");
            let _ = f.sample(-5.0, 1e9);
            let _ = f.sample_linear(f64::NAN, 0.0);
            let _ = f.resampled(3, 0);
        }
    }
    assert!(hp_geom::HeightField::new(3, 3, vec![0.0; 8]).is_none());
    let mut f = hp_geom::HeightField::filled(17, 17, 1.0);
    f.smooth(0);
    f.smooth(50);
    assert!(f.min_max().0.is_finite());
    let _ = DVec2::ZERO;
}

#[test]
fn merging_never_fills_a_one_unit_step() {
    let mut next = ids();
    let a = block(DVec3::ZERO, DVec3::new(96.0, 48.0, 8.0), &mut next);
    let b = block(DVec3::new(0.0, 0.0, 8.0), DVec3::new(95.0, 48.0, 64.0), &mut next);
    assert_eq!(hp_geom::merge_exact(&[a.clone(), b.clone()], &mut next).expect("merge"), None);
    let parts = union_pieces(&[a.clone(), b.clone()], &mut next).expect("union");
    assert_eq!(parts.len(), 2, "the step stays");
    let total: f64 = parts.iter().map(|p| assert_valid(&p.sides, "part")).sum();
    assert!((total - (96.0 * 48.0 * 8.0 + 95.0 * 48.0 * 56.0)).abs() < 1e-6, "{total}");
    let flush = block(DVec3::new(0.0, 0.0, 8.0), DVec3::new(96.0, 48.0, 64.0), &mut next);
    assert!(hp_geom::merge_exact(&[a, flush], &mut next).expect("merge").is_some(), "a flush stack still merges");
}

#[test]
fn degenerate_primitives_are_refused_instead_of_invalid() {
    let mut next = ids();
    let at = DVec3::new(16384.0, -16384.0, 0.33);
    let tiny = PrimitiveParams {
        kind: Primitive::Cylinder,
        sides: 64,
        bounds: Aabb::new(at, at + DVec3::splat(0.5)),
        ..Default::default()
    };
    assert!(make_primitive(&tiny, &mut next).is_err(), "64 sides in half a unit");
    let fewer = PrimitiveParams { sides: 4, ..tiny.clone() };
    let solids = make_primitive(&fewer, &mut next).expect("4 sides fit");
    assert_valid(&solids[0], "4 sides");
    let sliver = PrimitiveParams {
        kind: Primitive::SpiralStairs,
        bounds: Aabb::new(DVec3::ZERO, DVec3::new(128.0, 96.0, 64.0)),
        stair_steps: 1,
        spiral_angle: 179.9,
        spiral_inner: 45.0,
        stair_open: true,
        ..Default::default()
    };
    assert!(make_primitive(&sliver, &mut next).is_err(), "one step turning 179.9 degrees is a sliver");
    let flat = PrimitiveParams {
        kind: Primitive::Cylinder,
        sides: 7,
        up_axis: 0,
        bounds: Aabb::new(DVec3::ZERO, DVec3::new(64.0, 64.0, 0.02)),
        ..Default::default()
    };
    if let Ok(solids) = make_primitive(&flat, &mut next) {
        for s in &solids {
            assert!(flat.bounds.expand(0.5).contains_aabb(&build_solid(s).aabb));
        }
    }
}

#[test]
fn carve_many_matches_one_by_one_carving() {
    let mut next = ids();
    let mut rng = Rng(0xc4);
    let targets: Vec<Vec<Side>> = (0..40)
        .map(|i| {
            let min = DVec3::new(f64::from(i % 8) * 64.0, f64::from(i / 8) * 64.0, 0.0);
            block(min, min + DVec3::splat(64.0), &mut next)
        })
        .collect();
    let carvers: Vec<Vec<Side>> = (0..6)
        .map(|_| {
            let mut c = block(DVec3::splat(-20.0), DVec3::splat(20.0), &mut next);
            let m = DMat4::from_rotation_translation(
                rng.quat(),
                DVec3::new(rng.range(0.0, 512.0), rng.range(0.0, 320.0), 32.0),
            );
            transform_sides(&mut c, m, false, false, None).expect("transform");
            c
        })
        .collect();
    let target_refs: Vec<(&[Side], Aabb)> = targets.iter().map(|s| (s.as_slice(), build_solid(s).aabb)).collect();
    let carver_refs: Vec<(&[Side], Aabb)> = carvers.iter().map(|s| (s.as_slice(), build_solid(s).aabb)).collect();
    let many = hp_geom::carve_many(&target_refs, &carver_refs, &mut next);
    assert_eq!(many.len(), targets.len());
    let mut changed = 0;
    for (target, result) in targets.iter().zip(&many) {
        let mut serial = vec![target.clone()];
        for c in &carvers {
            let mut out = Vec::new();
            for p in serial {
                out.extend(carve(&p, c, &mut next).expect("carve"));
            }
            serial = out;
        }
        match result {
            None => assert_eq!(serial, vec![target.clone()], "untouched targets are reported as None"),
            Some(pieces) => {
                changed += 1;
                let a: f64 = pieces.iter().map(|p| assert_valid(&p.sides, "many")).sum();
                let b: f64 = serial.iter().map(|p| assert_valid(p, "serial")).sum();
                assert!((a - b).abs() < 1e-6, "{a} != {b}");
                assert_eq!(pieces.len(), serial.len());
                for p in pieces {
                    assert_eq!(p.sides.len(), p.origins.len());
                    for (s, o) in p.sides.iter().zip(&p.origins) {
                        if let Some(o) = o {
                            assert_eq!(s.material, target[*o].material);
                        }
                    }
                }
            }
        }
    }
    assert!(changed > 0);
    let all: Vec<Vec<Side>> = many.iter().flatten().flatten().map(|p| p.sides.clone()).collect();
    let kept: Vec<u32> = targets.iter().flatten().map(|s| s.id).collect();
    let mut seen: Vec<u32> = all.iter().flatten().map(|s| s.id).collect();
    let n = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), n, "unique ids over every target");
    assert!(all.iter().flatten().filter(|s| !kept.contains(&s.id)).all(|s| s.id > 100_000));
}
