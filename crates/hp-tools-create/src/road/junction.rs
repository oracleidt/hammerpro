use glam::{DVec2, DVec3};

use super::build::{JunctionProblem, LateralSide, PieceRole, Road, RoadError, RoadPiece, UvFrame};
use super::model::{JunctionEnd, RoadPoint};

const MIN_CUT_LENGTH: f64 = 2.0;
const MIN_SIN_ANGLE: f64 = 0.27;
const MAX_REACH: f64 = 2048.0;
const FLARE_CURVE_STEPS: usize = 6;
const MIN_FLARE_AREA: f64 = 0.25;
const MAX_FLARE_COS: f64 = 0.94;
const MIN_FLARE_COS: f64 = -0.985;

#[derive(Clone, Debug)]
pub struct CutPlan {
    pub end: JunctionEnd,
    pub facing: LateralSide,
    pub edge: Vec<DVec3>,
}

struct Hit {
    t: f64,
    point: DVec2,
    seg: usize,
    frac: f64,
}

fn line_hits(origin: DVec2, dir: DVec2, poly: &[DVec3]) -> Vec<Hit> {
    let mut out = Vec::new();
    for (i, w) in poly.windows(2).enumerate() {
        let (a, b) = (w[0].truncate(), w[1].truncate());
        let e = b - a;
        let denom = dir.perp_dot(e);
        if denom.abs() < 1e-9 {
            continue;
        }
        let ao = a - origin;
        let t = ao.perp_dot(e) / denom;
        let f = ao.perp_dot(dir) / denom;
        if (-1e-9..=1.0 + 1e-9).contains(&f) {
            out.push(Hit { t, point: origin + dir * t, seg: i, frac: f.clamp(0.0, 1.0) });
        }
    }
    out
}

fn edge_point(poly: &[DVec3], seg: usize, frac: f64) -> DVec3 {
    poly[seg].lerp(poly[seg + 1], frac)
}

pub fn move_endpoint(pts: &mut [RoadPoint], end: JunctionEnd, target: &Road, half: f64) -> Result<CutPlan, RoadError> {
    let n = pts.len();
    let (ei, ni) = match end {
        JunctionEnd::End => (n - 1, n - 2),
        JunctionEnd::Start => (0, 1),
    };
    let (e, nb) = (pts[ei].xy(), pts[ni].xy());
    let len = nb.distance(e);
    let dir = (e - nb).try_normalize().ok_or(RoadError::TooFewPoints)?;
    let mut best: Option<(f64, LateralSide, Hit)> = None;
    for side in [LateralSide::Right, LateralSide::Left] {
        let poly = target.edge(side, false);
        for h in line_hits(nb, dir, &poly) {
            let miss = (h.t - len).abs();
            if best.as_ref().is_none_or(|b| miss < b.0) {
                best = Some((miss, side, h));
            }
        }
    }
    let Some((miss, side, hit)) = best else { return Err(RoadError::Junction(JunctionProblem::Parallel)) };
    if miss > MAX_REACH {
        return Err(RoadError::Junction(JunctionProblem::TooFar));
    }
    let poly = target.edge(side, false);
    let along = (poly[hit.seg + 1].truncate() - poly[hit.seg].truncate()).try_normalize().unwrap_or(DVec2::X);
    let sin = dir.perp_dot(along).abs();
    if sin < MIN_SIN_ANGLE {
        return Err(RoadError::Junction(JunctionProblem::Shallow));
    }
    let retreat = half * dir.dot(along).abs() / sin;
    if hit.t - retreat < MIN_CUT_LENGTH * 8.0 {
        return Err(RoadError::Junction(JunctionProblem::TooClose));
    }
    let z = edge_point(&poly, hit.seg, hit.frac).z;
    let at = hit.point - dir * retreat;
    pts[ei] = RoadPoint::new(at.x, at.y, z);
    Ok(CutPlan { end, facing: side, edge: poly })
}

impl CutPlan {
    fn nearest_hit(&self, origin: DVec2, dir: DVec2, near: DVec2) -> Option<DVec2> {
        line_hits(origin, dir, &self.edge)
            .into_iter()
            .min_by(|a, b| a.point.distance(near).total_cmp(&b.point.distance(near)))
            .map(|h| h.point)
    }

    pub fn cut_quads(&self, quads: &mut [[DVec2; 4]], limit: f64) -> Result<(), RoadError> {
        let shallow = RoadError::Junction(JunctionProblem::Shallow);
        let (index, right_edge, left_edge) = match self.end {
            JunctionEnd::End => (quads.len() - 1, (0, 1), (3, 2)),
            JunctionEnd::Start => (0, (1, 0), (2, 3)),
        };
        let q = &mut quads[index];
        let mid_dir = {
            let a = (q[0] + q[3]) * 0.5;
            let b = (q[1] + q[2]) * 0.5;
            (b - a).try_normalize().ok_or_else(|| shallow.clone())?
        };
        for (from, to) in [right_edge, left_edge] {
            let d = (q[to] - q[from]).try_normalize().ok_or_else(|| shallow.clone())?;
            let hit = self.nearest_hit(q[from], d, q[to]).ok_or_else(|| shallow.clone())?;
            q[to] = hit;
        }
        let (ra, la) = (q[1] - q[0], q[2] - q[3]);
        let ahead = |v: DVec2| v.dot(mid_dir);
        if ahead(ra) < MIN_CUT_LENGTH || ahead(la) < MIN_CUT_LENGTH || ra.length().max(la.length()) > limit {
            return Err(shallow);
        }
        Ok(())
    }
}

fn walk(poly: &[DVec3], start_seg: usize, start: DVec3, forward: bool, reach: f64) -> Vec<DVec3> {
    let mut out = vec![start];
    let mut total = 0.0;
    let mut cur = start;
    let mut i = if forward { start_seg + 1 } else { start_seg };
    while let Some(next) = poly.get(i).copied() {
        let seg = cur.truncate().distance(next.truncate());
        if total + seg >= reach {
            let f = if seg > 1e-9 { (reach - total) / seg } else { 0.0 };
            out.push(cur.lerp(next, f));
            break;
        }
        total += seg;
        out.push(next);
        cur = next;
        if forward {
            i += 1;
        } else if i == 0 {
            break;
        } else {
            i -= 1;
        }
    }
    out
}

fn walk_inward(poly: &[DVec3], reach: f64) -> Vec<DVec3> {
    let mut out = vec![poly[0]];
    let mut total = 0.0;
    for w in poly.windows(2) {
        let seg = w[0].truncate().distance(w[1].truncate());
        if total + seg >= reach {
            let f = if seg > 1e-9 { (reach - total) / seg } else { 0.0 };
            out.push(w[0].lerp(w[1], f));
            return out;
        }
        total += seg;
        out.push(w[1]);
    }
    out
}

fn area(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    (b - a).perp_dot(c - a) * 0.5
}

pub fn flare_pieces(road: &Road, plan: &CutPlan, frames: &[UvFrame]) -> Vec<RoadPiece> {
    let Some(flare) = road.meta.junction.map(|j| j.flare).filter(|f| *f > 0.0) else { return Vec::new() };
    let samples = &road.samples;
    let ordered: Vec<usize> = match plan.end {
        JunctionEnd::End => (0..samples.len()).rev().collect(),
        JunctionEnd::Start => (0..samples.len()).collect(),
    };
    let right_poly: Vec<DVec3> = ordered.iter().map(|i| samples[*i].right_edge).collect();
    let left_poly: Vec<DVec3> = ordered.iter().map(|i| samples[*i].left_edge).collect();
    let frame = match plan.end {
        JunctionEnd::End => frames.last().copied(),
        JunctionEnd::Start => frames.first().copied(),
    };
    let Some(frame) = frame else { return Vec::new() };
    let segment = match plan.end {
        JunctionEnd::End => frames.len() - 1,
        JunctionEnd::Start => 0,
    };
    let mut out = Vec::new();
    for (side, poly, other) in
        [(LateralSide::Right, &right_poly, &left_poly), (LateralSide::Left, &left_poly, &right_poly)]
    {
        let v = poly[0];
        let outward = (v.truncate() - other[0].truncate()).try_normalize().unwrap_or(DVec2::X);
        let mut best: Option<(f64, usize, f64)> = None;
        for (i, w) in plan.edge.windows(2).enumerate() {
            let (a, b) = (w[0].truncate(), w[1].truncate());
            let d = b - a;
            let len2 = d.length_squared();
            let f = if len2 > 1e-18 { ((v.truncate() - a).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let dist = (a + d * f).distance(v.truncate());
            if best.is_none_or(|bst| dist < bst.0) {
                best = Some((dist, i, f));
            }
        }
        let Some((_, seg, frac)) = best else { continue };
        let tangent = (plan.edge[seg + 1].truncate() - plan.edge[seg].truncate()).try_normalize().unwrap_or(DVec2::X);
        let forward = tangent.dot(outward) >= 0.0;
        let start = edge_point(&plan.edge, seg, frac);
        let path_a = walk(&plan.edge, seg, start, forward, flare);
        let path_b = walk_inward(poly, flare);
        if path_a.len() < 2 || path_b.len() < 2 {
            continue;
        }
        let dir_a = (path_a[1].truncate() - path_a[0].truncate()).try_normalize();
        let dir_b = (path_b[1].truncate() - path_b[0].truncate()).try_normalize();
        let (Some(da), Some(db)) = (dir_a, dir_b) else { continue };
        let cos = da.dot(db);
        if !(MIN_FLARE_COS..=MAX_FLARE_COS).contains(&cos) {
            continue;
        }
        let ta = path_a[path_a.len() - 1];
        let tb = path_b[path_b.len() - 1];
        let mut ring: Vec<DVec3> = path_a[1..].to_vec();
        for k in 1..FLARE_CURVE_STEPS {
            let t = k as f64 / FLARE_CURVE_STEPS as f64;
            let p = v.truncate() * (2.0 * t * (1.0 - t))
                + ta.truncate() * ((1.0 - t) * (1.0 - t))
                + tb.truncate() * (t * t);
            ring.push(DVec3::new(p.x, p.y, ta.z + (tb.z - ta.z) * t));
        }
        ring.extend(path_b[1..].iter().rev().copied());
        let mut orientation = 0.0f64;
        for w in ring.windows(2) {
            let a = area(v.truncate(), w[0].truncate(), w[1].truncate());
            if a.abs() >= MIN_FLARE_AREA {
                orientation = a.signum();
                break;
            }
        }
        if orientation == 0.0 {
            continue;
        }
        for w in ring.windows(2) {
            let a = area(v.truncate(), w[0].truncate(), w[1].truncate());
            if a * orientation < MIN_FLARE_AREA {
                continue;
            }
            let top = [v, w[0], w[1]];
            let mut corners: Vec<DVec3> = top.iter().map(|p| DVec3::new(p.x, p.y, p.z - road.meta.thickness)).collect();
            corners.extend(top);
            out.push(RoadPiece {
                segment,
                role: PieceRole::Flare(side),
                lateral: (0.0, 0.0),
                corners,
                material: road.meta.material.clone(),
                uv: frame,
            });
        }
    }
    out
}
