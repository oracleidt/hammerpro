use std::f64::consts::{PI, TAU};

use glam::DVec2;

use super::error::RoadsideError;
use super::spine::{Spine, perp_left};

pub const MIN_JOIN_ANGLE: f64 = 20.0 * PI / 180.0;
const THROUGH_TOL: f64 = 3.0 * PI / 180.0;
const JOIN_SLACK: f64 = 2.0;
const END_MARGIN: f64 = 1.0;
const ARC_STEP: f64 = 12.0 * PI / 180.0;
const MIN_ARC_STEPS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sides {
    Left,
    Right,
    Both,
}

impl Sides {
    pub fn name(self) -> &'static str {
        match self {
            Sides::Left => "left",
            Sides::Right => "right",
            Sides::Both => "both",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" => Some(Sides::Left),
            "right" => Some(Sides::Right),
            "both" => Some(Sides::Both),
            _ => None,
        }
    }

    pub fn has(self, side: f64) -> bool {
        match self {
            Sides::Both => true,
            Sides::Left => side > 0.0,
            Sides::Right => side < 0.0,
        }
    }
}

pub fn side_index(side: f64) -> usize {
    if side > 0.0 { 0 } else { 1 }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Corner {
    pub raw: Vec<DVec2>,
    pub center: DVec2,
    pub radius: f64,
    pub apex: DVec2,
    pub bis: DVec2,
    pub inv_sin_half: f64,
    pub ta: DVec2,
    pub tb: DVec2,
    pub na: DVec2,
    pub nb: DVec2,
    pub corr_a: DVec2,
    pub corr_b: DVec2,
    pub z_a: f64,
    pub z_b: f64,
}

impl Corner {
    pub fn steps(&self) -> usize {
        self.raw.len() - 1
    }

    pub fn fraction(&self, k: usize) -> f64 {
        k as f64 / self.steps() as f64
    }

    pub fn z_ref(&self, k: usize) -> f64 {
        self.z_a + (self.z_b - self.z_a) * self.fraction(k)
    }

    pub fn point(&self, k: usize, o: f64) -> DVec2 {
        let n = self.steps();
        let k = k.min(n);
        let corr = self.corr_a.lerp(self.corr_b, self.fraction(k));
        if o <= 1e-9 {
            return self.raw[k] + corr;
        }
        if self.radius > 1e-9 && o <= self.radius {
            return self.center + (self.raw[k] - self.center) * ((self.radius - o) / self.radius) + corr;
        }
        if k == 0 {
            self.ta + self.corr_a + self.na * o
        } else if k == n {
            self.tb + self.corr_b + self.nb * o
        } else {
            self.apex + self.bis * (o * self.inv_sin_half) + self.corr_a.lerp(self.corr_b, 0.5)
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub removed: Vec<[Vec<(f64, f64)>; 2]>,
    pub corners: Vec<Corner>,
}

#[derive(Clone, Copy, Debug)]
struct Arm {
    spine: usize,
    flip: bool,
    s_node: f64,
    node: DVec2,
    dir: DVec2,
}

impl Arm {
    fn angle(&self) -> f64 {
        self.dir.y.atan2(self.dir.x)
    }

    fn spine_side(&self, left_of_arm: bool) -> f64 {
        let left = left_of_arm != self.flip;
        if left { 1.0 } else { -1.0 }
    }

    fn station(&self, d: f64) -> f64 {
        if self.flip { self.s_node - d } else { self.s_node + d }
    }
}

#[derive(Clone, Copy, Debug)]
struct Hit {
    host: usize,
    s: f64,
    guest: Arm,
}

fn find_hits(spines: &[Spine]) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (b, sb) in spines.iter().enumerate() {
        for at_end in [false, true] {
            let e = if at_end { sb.end() } else { sb.start() };
            let (s_node, dir, flip) = if at_end {
                (sb.length(), -sb.chord_dir(sb.segments() - 1), true)
            } else {
                (0.0, sb.chord_dir(0), false)
            };
            let mut best: Option<(f64, usize, f64)> = None;
            for (a, sa) in spines.iter().enumerate() {
                if a == b {
                    continue;
                }
                let near = sa.nearest(e);
                let inside = near.dist <= sa.half_width + JOIN_SLACK;
                let interior = near.s > END_MARGIN && near.s < sa.length() - END_MARGIN;
                if inside && interior && best.is_none_or(|x| near.dist < x.0) {
                    best = Some((near.dist, a, near.s));
                }
            }
            if let Some((_, host, s)) = best {
                hits.push(Hit { host, s, guest: Arm { spine: b, flip, s_node, node: e, dir } });
            }
        }
    }
    hits
}

struct Node {
    host: usize,
    s: f64,
    guests: Vec<Arm>,
}

fn cluster(spines: &[Spine], mut hits: Vec<Hit>) -> Vec<Node> {
    hits.sort_by(|x, y| x.host.cmp(&y.host).then(x.s.total_cmp(&y.s)));
    let mut nodes: Vec<(Node, f64)> = Vec::new();
    for h in hits {
        let tol = (spines[h.host].half_width * 0.5).max(4.0);
        match nodes.last_mut() {
            Some((n, sum)) if n.host == h.host && (h.s - *sum / (n.guests.len() as f64)).abs() <= tol => {
                n.guests.push(h.guest);
                *sum += h.s;
                n.s = *sum / n.guests.len() as f64;
            }
            _ => nodes.push((Node { host: h.host, s: h.s, guests: vec![h.guest] }, h.s)),
        }
    }
    nodes.into_iter().map(|(n, _)| n).collect()
}

struct Request<'a> {
    spines: &'a [Spine],
    sides: Sides,
    gap: f64,
    radius: f64,
}

pub fn plan(spines: &[Spine], sides: Sides, gap: f64, radius: f64) -> Result<Plan, RoadsideError> {
    let mut out = Plan { removed: spines.iter().map(|_| [Vec::new(), Vec::new()]).collect(), corners: Vec::new() };
    let req = Request { spines, sides, gap, radius: radius.max(0.0) };
    let mut spans: Vec<Vec<(f64, f64, usize)>> = vec![Vec::new(); spines.len() * 2];
    for (id, node) in cluster(spines, find_hits(spines)).iter().enumerate() {
        process_node(&req, node, id, &mut out, &mut spans)?;
    }
    for (slot, list) in spans.iter_mut().enumerate() {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f64, f64, usize)> = Vec::new();
        for item in list.iter() {
            match merged.last_mut() {
                Some(last) if last.2 == item.2 && item.0 <= last.1 + 1e-6 => last.1 = last.1.max(item.1),
                _ => merged.push(*item),
            }
        }
        if merged.windows(2).any(|w| w[1].0 < w[0].1 - 1e-6) {
            return Err(RoadsideError::JunctionsClose);
        }
        out.removed[slot / 2][slot % 2] = merged.iter().map(|(lo, hi, _)| (*lo, *hi)).collect();
    }
    Ok(out)
}

fn process_node(
    req: &Request<'_>,
    node: &Node,
    id: usize,
    out: &mut Plan,
    spans: &mut [Vec<(f64, f64, usize)>],
) -> Result<(), RoadsideError> {
    let host = &req.spines[node.host];
    let frame = host.frame_at(node.s, true);
    let mut arms = vec![
        Arm { spine: node.host, flip: false, s_node: node.s, node: frame.point, dir: frame.dir },
        Arm { spine: node.host, flip: true, s_node: node.s, node: frame.point, dir: -frame.dir },
    ];
    arms.extend(node.guests.iter().copied());
    arms.sort_by(|a, b| a.angle().total_cmp(&b.angle()));
    let m = arms.len();
    for i in 0..m {
        let (ai, aj) = (arms[i], arms[(i + 1) % m]);
        let mut g = aj.angle() - ai.angle();
        if g <= 0.0 {
            g += TAU;
        }
        let straight = (g - PI).abs() < THROUGH_TOL;
        if straight {
            continue;
        }
        if !(MIN_JOIN_ANGLE..PI).contains(&g) {
            return Err(RoadsideError::JunctionAngle);
        }
        corner(req, &ai, &aj, g, id, out, spans)?;
    }
    Ok(())
}

fn cross(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn corner(
    req: &Request<'_>,
    ai: &Arm,
    aj: &Arm,
    g: f64,
    id: usize,
    out: &mut Plan,
    spans: &mut [Vec<(f64, f64, usize)>],
) -> Result<(), RoadsideError> {
    let (si, sj) = (&req.spines[ai.spine], &req.spines[aj.spine]);
    let side_i = ai.spine_side(true);
    let side_j = aj.spine_side(false);
    let has_i = req.sides.has(side_i);
    let has_j = req.sides.has(side_j);
    if !has_i && !has_j {
        return Ok(());
    }
    let base_i = si.half_width + req.gap;
    let base_j = sj.half_width + req.gap;
    let (ui, uj) = (ai.dir, aj.dir);
    let qi = ai.node + perp_left(ui) * base_i;
    let qj = aj.node - perp_left(uj) * base_j;
    let w = qj - qi;
    let det = cross(ui, uj);
    if det.abs() < 1e-9 {
        return Err(RoadsideError::JunctionAngle);
    }
    let s_i = cross(w, uj) / det;
    let s_j = -cross(ui, w) / det;
    let apex = qi + ui * s_i;
    let r = if has_i && has_j { req.radius } else { 0.0 };
    let half = g * 0.5;
    let t = if r > 0.0 { r / half.tan() } else { 0.0 };
    let (d_i, d_j) = (s_i + t, s_j + t);
    if d_i < 0.0 || d_j < 0.0 {
        return Err(RoadsideError::JunctionAngle);
    }
    let (cut_i, cut_j) = (ai.station(d_i), aj.station(d_j));
    for (cut, len) in [(cut_i, si.length()), (cut_j, sj.length())] {
        if cut < END_MARGIN || cut > len - END_MARGIN {
            return Err(RoadsideError::JunctionEnd);
        }
    }
    for (arm, side, d, has) in [(ai, side_i, d_i, has_i), (aj, side_j, d_j, has_j)] {
        if !has {
            continue;
        }
        let (a, b) = (arm.s_node, arm.station(d));
        spans[arm.spine * 2 + side_index(side)].push((a.min(b), a.max(b), id));
    }
    if !(has_i && has_j) {
        return Ok(());
    }
    let bis = (ui + uj).normalize_or_zero();
    let inv_sin_half = 1.0 / half.sin();
    let center = apex + bis * (r * inv_sin_half);
    let ta = apex + ui * t;
    let tb = apex + uj * t;
    let raw = if r > 0.0 {
        let a0 = (ta - center).to_array();
        let a1 = (tb - center).to_array();
        let (a0, a1) = (a0[1].atan2(a0[0]), a1[1].atan2(a1[0]));
        let mut delta = a1 - a0;
        while delta > PI {
            delta -= TAU;
        }
        while delta < -PI {
            delta += TAU;
        }
        let n = ((delta.abs() / ARC_STEP).ceil() as usize).max(MIN_ARC_STEPS);
        (0..=n)
            .map(|k| {
                let a = a0 + delta * k as f64 / n as f64;
                center + DVec2::new(a.cos(), a.sin()) * r
            })
            .collect()
    } else {
        vec![apex; MIN_ARC_STEPS + 1]
    };
    let fi = si.frame_at(cut_i, !ai.flip);
    let fj = sj.frame_at(cut_j, !aj.flip);
    let true_a = fi.point + fi.left * (side_i * base_i);
    let true_b = fj.point + fj.left * (side_j * base_j);
    out.corners.push(Corner {
        raw,
        center,
        radius: r,
        apex,
        bis,
        inv_sin_half,
        ta,
        tb,
        na: fi.left * side_i,
        nb: fj.left * side_j,
        corr_a: true_a - ta,
        corr_b: true_b - tb,
        z_a: si.edge_z(&fi, side_i),
        z_b: sj.edge_z(&fj, side_j),
    });
    Ok(())
}

pub fn keep_ranges(length: f64, removed: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut cuts: Vec<(f64, f64)> = removed.iter().map(|(a, b)| (a.max(0.0), b.min(length))).collect();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut at = 0.0;
    for (lo, hi) in cuts {
        if hi <= lo {
            continue;
        }
        if lo > at {
            out.push((at, lo));
        }
        at = at.max(hi);
    }
    if length > at {
        out.push((at, length));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64), half: f64) -> Spine {
        let pts = vec![DVec2::new(a.0, a.1), DVec2::new(b.0, b.1)];
        Spine::new(pts, vec![0.0; 2], vec![0.0; 2], half, 0.0).unwrap()
    }

    #[test]
    fn keep_ranges_cut_holes() {
        assert_eq!(keep_ranges(100.0, &[(40.0, 60.0)]), vec![(0.0, 40.0), (60.0, 100.0)]);
        assert_eq!(keep_ranges(100.0, &[(-5.0, 10.0), (90.0, 120.0)]), vec![(10.0, 90.0)]);
        assert_eq!(keep_ranges(100.0, &[]), vec![(0.0, 100.0)]);
        assert!(keep_ranges(100.0, &[(0.0, 100.0)]).is_empty());
    }

    #[test]
    fn perpendicular_t_junction_cuts_the_main_road_and_trims_the_side_road() {
        let main = line((-200.0, 0.0), (200.0, 0.0), 32.0);
        let side = line((0.0, 300.0), (0.0, 0.0), 32.0);
        let plan = plan(&[main, side], Sides::Both, 0.0, 16.0).unwrap();
        assert_eq!(plan.corners.len(), 2);
        let main_left = &plan.removed[0][0];
        assert_eq!(main_left.len(), 1);
        let (lo, hi) = main_left[0];
        assert!((lo - (200.0 - 32.0 - 16.0)).abs() < 1e-6, "{lo}");
        assert!((hi - (200.0 + 32.0 + 16.0)).abs() < 1e-6, "{hi}");
        assert!(plan.removed[0][1].is_empty());
        for side_cut in &plan.removed[1] {
            assert_eq!(side_cut.len(), 1);
            let (lo, hi) = side_cut[0];
            assert!((hi - 300.0).abs() < 1e-6);
            assert!((lo - (300.0 - 32.0 - 16.0)).abs() < 1e-6);
        }
    }

    #[test]
    fn corner_arc_is_tangent_and_has_the_radius() {
        let main = line((-200.0, 0.0), (200.0, 0.0), 32.0);
        let side = line((0.0, 300.0), (0.0, 0.0), 32.0);
        let plan = plan(&[main, side], Sides::Both, 0.0, 16.0).unwrap();
        for c in &plan.corners {
            assert!((c.radius - 16.0).abs() < 1e-9);
            for p in &c.raw {
                assert!((p.distance(c.center) - 16.0).abs() < 1e-6);
            }
            assert!(c.raw[0].distance(c.ta) < 1e-6);
            assert!(c.raw[c.raw.len() - 1].distance(c.tb) < 1e-6);
            let inner = c.point(1, 0.0);
            let outer = c.point(1, 40.0);
            assert!(inner.distance(outer) > 1.0);
        }
    }

    #[test]
    fn crossing_roads_make_four_corners() {
        let main = line((-300.0, 0.0), (300.0, 0.0), 32.0);
        let up = line((0.0, 300.0), (0.0, 0.0), 32.0);
        let down = line((0.0, -300.0), (0.0, 0.0), 32.0);
        let plan = plan(&[main, up, down], Sides::Both, 0.0, 24.0).unwrap();
        assert_eq!(plan.corners.len(), 4);
        assert_eq!(plan.removed[0][0].len(), 1);
        assert_eq!(plan.removed[0][1].len(), 1);
    }

    #[test]
    fn sharp_angle_and_end_junctions_are_refused() {
        let main = line((-200.0, 0.0), (200.0, 0.0), 32.0);
        let shallow = line((300.0, 40.0), (0.0, 1.0), 32.0);
        assert_eq!(plan(&[main.clone(), shallow], Sides::Both, 0.0, 16.0), Err(RoadsideError::JunctionAngle));
        let near_end = line((-170.0, 300.0), (-170.0, 0.0), 32.0);
        assert_eq!(plan(&[main, near_end], Sides::Both, 0.0, 64.0), Err(RoadsideError::JunctionEnd));
    }

    #[test]
    fn one_sided_sidewalk_has_no_corner_patch_on_the_far_side() {
        let main = line((-200.0, 0.0), (200.0, 0.0), 32.0);
        let side = line((0.0, 300.0), (0.0, 0.0), 32.0);
        let plan = plan(&[main, side], Sides::Right, 0.0, 16.0).unwrap();
        assert!(plan.corners.is_empty());
        assert!(plan.removed[0][0].is_empty());
    }
}
