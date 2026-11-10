use std::time::Instant;

use glam::DVec3;
use hp_geom::{Aabb, Primitive, PrimitiveParams, build_solid, carve, carve_many, hollow, make_primitive, union_pieces};
use hp_vmf::Side;

fn ids() -> impl FnMut() -> u32 {
    let mut n = 10_000;
    move || {
        n += 1;
        n
    }
}

fn block(min: DVec3, max: DVec3, next: &mut dyn FnMut() -> u32) -> Vec<Side> {
    let p = PrimitiveParams { kind: Primitive::Block, bounds: Aabb::new(min, max), ..Default::default() };
    make_primitive(&p, next).expect("block").remove(0)
}

fn grid(nx: usize, ny: usize, size: f64, step: f64, next: &mut dyn FnMut() -> u32) -> Vec<Vec<Side>> {
    let mut out = Vec::new();
    for x in 0..nx {
        for y in 0..ny {
            let min = DVec3::new(x as f64 * step, y as f64 * step, 0.0);
            out.push(block(min, min + DVec3::new(size, size, 64.0), next));
        }
    }
    out
}

fn best<T>(runs: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut out = None;
    let mut min = f64::INFINITY;
    for _ in 0..runs {
        let t = Instant::now();
        let r = f();
        min = min.min(t.elapsed().as_secs_f64() * 1000.0);
        out = Some(r);
    }
    (min, out.expect("ran"))
}

fn serial_carve(targets: &[Vec<Side>], carvers: &[Vec<Side>], next: &mut dyn FnMut() -> u32) -> usize {
    let carver_boxes: Vec<Aabb> = carvers.iter().map(|c| build_solid(c).aabb).collect();
    let mut pieces = 0;
    for target in targets {
        let tb = build_solid(target).aabb;
        let mut current = vec![target.clone()];
        for (c, cb) in carvers.iter().zip(&carver_boxes) {
            if !tb.intersects(cb) {
                continue;
            }
            let mut out = Vec::new();
            for p in current {
                out.extend(carve(&p, c, next).expect("carve"));
            }
            current = out;
        }
        pieces += current.len();
    }
    pieces
}

#[test]
#[ignore]
fn bench_csg_on_many_brushes() {
    let runs: usize = std::env::var("HP_BENCH_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    let mut next = ids();
    for (nx, ny) in [(10, 10), (20, 10), (20, 20)] {
        let touching = grid(nx, ny, 64.0, 64.0, &mut next);
        let (ms, parts) = best(runs, || union_pieces(&touching, &mut next).expect("union").len());
        eprintln!("union of {} touching boxes: {ms:.1} ms -> {parts} parts", touching.len());
        let overlapping = grid(nx, ny, 96.0, 64.0, &mut next);
        let (ms, parts) = best(runs, || union_pieces(&overlapping, &mut next).expect("union").len());
        eprintln!("union of {} overlapping boxes: {ms:.1} ms -> {parts} parts", overlapping.len());
        let staggered: Vec<Vec<Side>> = (0..nx * ny)
            .map(|i| {
                let min = DVec3::new((i % nx) as f64 * 48.0, (i / nx) as f64 * 40.0, (i % 3) as f64 * 16.0);
                block(min, min + DVec3::new(64.0, 56.0, 64.0), &mut next)
            })
            .collect();
        let (ms, parts) = best(runs, || union_pieces(&staggered, &mut next).expect("union").len());
        eprintln!("union of {} staggered boxes: {ms:.1} ms -> {parts} parts", staggered.len());
    }
    let targets = grid(30, 30, 64.0, 64.0, &mut next);
    let carvers: Vec<Vec<Side>> = (0..30)
        .map(|i| {
            let x = i as f64 * 64.0 + 16.0;
            block(DVec3::new(x, -64.0, -16.0), DVec3::new(x + 32.0, 30.0 * 64.0 + 64.0, 32.0), &mut next)
        })
        .collect();
    let (ms, pieces) = best(runs, || serial_carve(&targets, &carvers, &mut next));
    eprintln!("serial carve of {} targets by {} bars: {ms:.1} ms -> {pieces} pieces", targets.len(), carvers.len());
    let target_refs: Vec<(&[Side], Aabb)> = targets.iter().map(|s| (s.as_slice(), build_solid(s).aabb)).collect();
    let carver_refs: Vec<(&[Side], Aabb)> = carvers.iter().map(|s| (s.as_slice(), build_solid(s).aabb)).collect();
    let (ms, pieces) = best(runs, || {
        carve_many(&target_refs, &carver_refs, &mut next).iter().map(|p| p.as_ref().map_or(1, Vec::len)).sum::<usize>()
    });
    eprintln!("carve_many of {} targets by {} bars: {ms:.1} ms -> {pieces} pieces", targets.len(), carvers.len());
    let (ms, walls) =
        best(runs, || targets.iter().map(|t| hollow(t, 8.0, &mut next).expect("hollow").len()).sum::<usize>());
    eprintln!("hollow of {} boxes: {ms:.1} ms -> {walls} walls", targets.len());
}
