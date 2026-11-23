use std::collections::HashMap;

use glam::{DVec2, DVec3};

use super::*;
use crate::Triangle;

fn sphere(radius: f64, subdivisions: usize) -> Vec<Triangle> {
    let mut tris: Vec<[DVec3; 3]> = Vec::new();
    let (x, y, z) = (DVec3::X, DVec3::Y, DVec3::Z);
    for (a, b) in [(x, y), (y, -x), (-x, -y), (-y, x)] {
        tris.push([a, b, z]);
        tris.push([b, a, -z]);
    }
    for _ in 0..subdivisions {
        let mut next = Vec::with_capacity(tris.len() * 4);
        for [a, b, c] in tris {
            let (ab, bc, ca) = ((a + b).normalize(), (b + c).normalize(), (c + a).normalize());
            next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        tris = next;
    }
    tris.into_iter()
        .map(|t| Triangle { pos: t.map(|p| p * radius), normal: t, uv: [DVec2::ZERO; 3], material: 0 })
        .collect()
}

fn grid(n: usize, size: f64) -> Vec<Triangle> {
    let p = |i: usize, j: usize| {
        DVec3::new(-size + 2.0 * size * i as f64 / n as f64, -size + 2.0 * size * j as f64 / n as f64, 0.0)
    };
    let uv = |p: DVec3| DVec2::new(p.x / 64.0, -p.y / 64.0);
    let mut out = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let material = usize::from(i >= n / 2);
            let q = [p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1)];
            for t in [[0, 1, 2], [0, 2, 3]] {
                let pos = t.map(|k| q[k]);
                out.push(Triangle { pos, normal: [DVec3::Z; 3], uv: pos.map(uv), material });
            }
        }
    }
    out
}

fn key(p: DVec3) -> [i64; 3] {
    [(p.x * 1e3).round() as i64, (p.y * 1e3).round() as i64, (p.z * 1e3).round() as i64]
}

fn edge_uses(tris: &[Triangle]) -> HashMap<([i64; 3], [i64; 3]), usize> {
    let mut edges = HashMap::new();
    for t in tris {
        for k in 0..3 {
            let (a, b) = (key(t.pos[k]), key(t.pos[(k + 1) % 3]));
            *edges.entry(if a < b { (a, b) } else { (b, a) }).or_insert(0) += 1;
        }
    }
    edges
}

fn levels(fractions: &[f64]) -> Vec<LodLevel> {
    fractions.iter().map(|&fraction| LodLevel { fraction, switch: 10.0 }).collect()
}

#[test]
fn plans_three_levels_at_least() {
    let o = LodOptions::default();
    assert_eq!(o.levels, 4);
    let p = plan(&o, 5000, 64.0);
    assert_eq!(p.len(), 4);
    assert!((p[0].fraction - 0.5).abs() < 1e-9 && (p[2].fraction - 0.125).abs() < 1e-9);
    assert!(p.windows(2).all(|w| w[1].switch > w[0].switch && w[1].fraction < w[0].fraction));
    let small = plan(&LodOptions { levels: 1, ..o }, 40, 4.0);
    assert_eq!(small.len(), MIN_LEVELS);
    assert!(small[0].fraction >= 0.75 - 1e-9);
    assert_eq!(plan(&o, 50_000, 300.0).len(), 5);
    assert_eq!(plan(&o, 200_000, 300.0).len(), MAX_LEVELS);
    assert_eq!(plan(&LodOptions { levels: 9, ..o }, 200_000, 300.0).len(), MAX_LEVELS);
    assert!(plan(&LodOptions { enabled: false, ..o }, 5000, 64.0).is_empty());
    assert!(plan(&o, 5000, 512.0)[0].switch > plan(&o, 5000, 64.0)[0].switch);
}

#[test]
fn custom_lod_distances_keep_fractions_and_automatic_levels() {
    let defaults = LodOptions::default();
    let automatic = plan(&defaults, 5000, 64.0);
    let custom = LodOptions { distances: [Some(37.5), None, Some(700.0), Some(2000.0), None, None], ..defaults };
    let levels = plan(&custom, 5000, 64.0);
    assert_eq!(
        levels.iter().map(|level| level.fraction).collect::<Vec<_>>(),
        automatic.iter().map(|level| level.fraction).collect::<Vec<_>>()
    );
    assert_eq!(levels.iter().map(|level| level.switch).collect::<Vec<_>>(), [37.5, automatic[1].switch, 700.0, 2000.0]);
    let large = plan(
        &LodOptions {
            distances: [Some(20.0), Some(10.0), Some(f64::NAN), None, Some(5000.0), Some(9000.0)],
            ..defaults
        },
        100_000,
        64.0,
    );
    assert_eq!(large.len(), 6);
    assert!(large.windows(2).all(|pair| pair[0].switch < pair[1].switch));
    assert_eq!(large[5].switch, 9000.0);
}

#[test]
fn a_closed_sphere_stays_closed() {
    let s = sphere(64.0, 4);
    assert_eq!(s.len(), 2048);
    let out = simplify_triangles(&s, &levels(&[0.5, 0.25, 0.125]));
    assert_eq!(out.len(), 3);
    for (level, target) in out.iter().zip([1024usize, 512, 256]) {
        assert!(level.len() <= target + 2 && level.len() + 64 >= target, "{} for {target}", level.len());
        for (edge, n) in edge_uses(level) {
            assert_eq!(n, 2, "edge {edge:?} has {n} triangles: a hole or a fold");
        }
        for t in level {
            assert!(t.pos.iter().all(|p| (p.length() - 64.0).abs() < 1e-6));
            let n = (t.pos[1] - t.pos[0]).cross(t.pos[2] - t.pos[0]);
            assert!(n.dot(t.pos[0] + t.pos[1] + t.pos[2]) > 0.0, "a flipped triangle");
        }
    }
}

#[test]
fn borders_materials_and_uvs_are_kept() {
    let g = grid(16, 128.0);
    let out = simplify_triangles(&g, &levels(&[0.5, 0.25, 0.1]));
    let last = out.last().expect("levels");
    assert!(last.len() < g.len() / 4, "a flat grid simplifies a lot: {}", last.len());
    for level in &out {
        let mut area = [0.0f64; 2];
        for t in level {
            area[t.material] += (t.pos[1] - t.pos[0]).cross(t.pos[2] - t.pos[0]).z * 0.5;
            for k in 0..3 {
                assert!((t.uv[k] - DVec2::new(t.pos[k].x / 64.0, -t.pos[k].y / 64.0)).length() < 1e-9);
            }
            let side = if t.material == 0 { -1.0 } else { 1.0 };
            assert!(t.pos.iter().all(|p| p.x * side >= -1e-9), "a triangle crosses the material border");
        }
        assert!((area[0] - 128.0 * 256.0).abs() < 1e-6 && (area[1] - 128.0 * 256.0).abs() < 1e-6, "{area:?}");
    }
}

#[test]
fn hard_edged_boxes_stay_whole() {
    let mut s = Vec::new();
    let quads = [
        ([0, 2, 6, 4], -DVec3::X),
        ([1, 5, 7, 3], DVec3::X),
        ([0, 4, 5, 1], -DVec3::Y),
        ([2, 3, 7, 6], DVec3::Y),
        ([0, 1, 3, 2], -DVec3::Z),
        ([4, 6, 7, 5], DVec3::Z),
    ];
    let c = |i: usize| DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64) * 32.0;
    for (q, n) in quads {
        for k in [[0, 2, 1], [0, 3, 2]] {
            s.push(Triangle { pos: k.map(|i| c(q[i])), normal: [n; 3], uv: [DVec2::ZERO; 3], material: 0 });
        }
    }
    let out = simplify_triangles(&s, &levels(&[0.5, 0.25, 0.125]));
    assert!(out.iter().all(|l| l.len() == 12));
}

fn faceted_sphere(radius: f64, rings: usize, segments: usize) -> Vec<Triangle> {
    let point = |ring: usize, segment: usize| {
        let theta = std::f64::consts::PI * ring as f64 / rings as f64;
        let phi = std::f64::consts::TAU * (segment % segments) as f64 / segments as f64;
        DVec3::new(theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos()) * radius
    };
    let mut out = Vec::new();
    for ring in 0..rings {
        for segment in 0..segments {
            let quad = [
                point(ring, segment),
                point(ring + 1, segment),
                point(ring + 1, segment + 1),
                point(ring, segment + 1),
            ];
            let mut tris = Vec::new();
            if ring > 0 {
                tris.push([quad[0], quad[1], quad[3]]);
            }
            if ring + 1 < rings {
                tris.push([quad[1], quad[2], quad[3]]);
            }
            if ring == 0 {
                tris = vec![[quad[0], quad[1], quad[2]]];
            }
            if ring + 1 == rings {
                tris = vec![[quad[0], quad[1], quad[3]]];
            }
            let n = (quad[1] - quad[0]).cross(quad[3] - quad[1]).normalize_or_zero();
            let n = if n == DVec3::ZERO { (quad[2] - quad[1]).cross(quad[3] - quad[1]).normalize() } else { n };
            let n = if n.dot(quad[1]) < 0.0 { -n } else { n };
            let axis = if n.z.abs() >= n.x.abs().max(n.y.abs()) {
                2
            } else if n.x.abs() >= n.y.abs() {
                0
            } else {
                1
            };
            let uv = |p: DVec3| match axis {
                0 => DVec2::new(p.y, -p.z) / 128.0,
                1 => DVec2::new(p.x, -p.z) / 128.0,
                _ => DVec2::new(p.x, -p.y) / 128.0,
            };
            for t in tris {
                let t = if (t[1] - t[0]).cross(t[2] - t[0]).dot(n) < 0.0 { [t[0], t[2], t[1]] } else { t };
                out.push(Triangle { pos: t, normal: [n; 3], uv: t.map(uv), material: 0 });
            }
        }
    }
    out
}

#[test]
fn faceted_brush_spheres_reach_the_requested_part() {
    let s = faceted_sphere(64.0, 12, 24);
    let full = s.len();
    let out = simplify_triangles(&s, &levels(&[0.5, 0.25, 0.125]));
    let parts: Vec<f64> = out.iter().map(|l| l.len() as f64 / full as f64).collect();
    assert!((parts[0] - 0.5).abs() < 0.05, "LOD 1 keeps {:.3} of {full}", parts[0]);
    assert!((parts[1] - 0.25).abs() < 0.05, "LOD 2 keeps {:.3}", parts[1]);
    assert!(parts[2] < 0.25, "LOD 3 keeps {:.3}", parts[2]);
    for level in &out {
        for (edge, n) in edge_uses(level) {
            assert_eq!(n, 2, "edge {edge:?} has {n} triangles: a hole or a fold");
        }
        for t in level {
            let n = (t.pos[1] - t.pos[0]).cross(t.pos[2] - t.pos[0]);
            assert!(n.dot(t.pos[0] + t.pos[1] + t.pos[2]) > 0.0, "a flipped triangle");
            assert!(t.pos.iter().all(|p| (p.length() - 64.0).abs() < 1e-6));
            assert!(t.normal.iter().all(|m| (m.length() - 1.0).abs() < 1e-6));
        }
    }
}
