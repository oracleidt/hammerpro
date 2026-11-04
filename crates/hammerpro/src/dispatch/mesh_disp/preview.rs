use glam::DVec3;
use hp_editor::mesh_disp::{Frame, TilePlan};
use hp_render::OverlayLine;

pub(crate) const OUTLINE: [f32; 4] = [0.2, 0.9, 1.0, 1.0];
pub(crate) const SURFACE: [f32; 4] = [1.0, 0.8, 0.2, 0.9];
pub(crate) const SKIRT: [f32; 4] = [0.5, 1.0, 0.4, 0.9];
const MAX_LINES: usize = 60_000;

fn line(frame: &Frame, a: DVec3, b: DVec3, color: [f32; 4]) -> OverlayLine {
    let (a, b) = (frame.world(a), frame.world(b));
    OverlayLine { a: [a.x as f32, a.y as f32, a.z as f32], b: [b.x as f32, b.y as f32, b.z as f32], color }
}

pub(crate) fn plan_lines(plan: &TilePlan, skirt: bool, bottom: bool) -> Vec<OverlayLine> {
    let frame = plan.frame;
    let cells = plan.cells();
    let tiles = plan.tile_count().max(1);
    let per_tile = MAX_LINES / tiles;
    let stride = [1, 2, 4, 8, 16]
        .into_iter()
        .filter(|s| cells.is_multiple_of(*s))
        .find(|s| 2 * (cells / s + 1) * cells <= per_tile)
        .unwrap_or(cells);
    let mut out = Vec::new();
    for j in 0..plan.ny {
        for i in 0..plan.nx {
            if !plan.kept[j * plan.nx + i] {
                continue;
            }
            let (u0, u1, v0, v1) = plan.tile_rect(i, j);
            let z = plan.plane;
            let corners = [DVec3::new(u0, v0, z), DVec3::new(u1, v0, z), DVec3::new(u1, v1, z), DVec3::new(u0, v1, z)];
            for k in 0..4 {
                out.push(line(&frame, corners[k], corners[(k + 1) % 4], OUTLINE));
            }
            let at = |gi: usize, gj: usize| {
                DVec3::new(
                    plan.lo_u + gi as f64 * plan.spacing,
                    plan.lo_v + gj as f64 * plan.spacing,
                    plan.node(gi, gj),
                )
            };
            for r in (0..=cells).step_by(stride) {
                for c in 0..cells {
                    let (gi, gj) = (i * cells + c, j * cells + r);
                    out.push(line(&frame, at(gi, gj), at(gi + 1, gj), SURFACE));
                    let (gi, gj) = (i * cells + r, j * cells + c);
                    out.push(line(&frame, at(gi, gj), at(gi, gj + 1), SURFACE));
                }
            }
            if skirt || bottom {
                let (ii, jj) = (i as isize, j as isize);
                for (di, dj, a, b) in [(-1, 0, 3, 0), (1, 0, 1, 2), (0, -1, 0, 1), (0, 1, 2, 3)] {
                    if plan.tile_kept(ii + di, jj + dj) {
                        continue;
                    }
                    let (ca, cb) = (corners[a], corners[b]);
                    let (ga, gb) = (
                        (
                            ((ca.x - plan.lo_u) / plan.spacing).round() as usize,
                            ((ca.y - plan.lo_v) / plan.spacing).round() as usize,
                        ),
                        (
                            ((cb.x - plan.lo_u) / plan.spacing).round() as usize,
                            ((cb.y - plan.lo_v) / plan.spacing).round() as usize,
                        ),
                    );
                    let (ta, tb) = (at(ga.0, ga.1), at(gb.0, gb.1));
                    let (ba, bb) = (DVec3::new(ca.x, ca.y, plan.base), DVec3::new(cb.x, cb.y, plan.base));
                    if skirt {
                        out.push(line(&frame, ta, ba, SKIRT));
                        out.push(line(&frame, tb, bb, SKIRT));
                    }
                    out.push(line(&frame, ba, bb, SKIRT));
                }
            }
        }
    }
    out
}

pub(crate) fn box_lines(lo: DVec3, hi: DVec3) -> Vec<OverlayLine> {
    let frame = Frame::new(2, false);
    let c = |x: bool, y: bool, z: bool| {
        DVec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z })
    };
    let mut out = Vec::new();
    for z in [false, true] {
        let ring = [c(false, false, z), c(true, false, z), c(true, true, z), c(false, true, z)];
        for k in 0..4 {
            out.push(line(&frame, ring[k], ring[(k + 1) % 4], OUTLINE));
        }
    }
    for (x, y) in [(false, false), (true, false), (true, true), (false, true)] {
        out.push(line(&frame, c(x, y, false), c(x, y, true), OUTLINE));
    }
    out
}
