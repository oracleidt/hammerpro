use glam::{DVec2, DVec3};

use super::junction::{self, CutPlan};
use super::model::{JunctionEnd, LineKind, MAX_COORD, MAX_TOTAL_WIDTH, RoadMeta, RoadPoint};
use super::spline::{Dense, Elevation, dense};
use crate::curve::geom::{CurveError, layout_centers, resample_dense};
use crate::curve::section::{bank_shift, station_rolls};

pub const MAX_SEGMENTS: usize = 2048;
pub const MAX_PIECES: usize = 8000;
pub const MIN_BAND: f64 = 1.0;
const SMOOTH_PASSES: usize = 2;
const MIN_POINT_GAP: f64 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LateralSide {
    Right,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineRole {
    Edge(LateralSide),
    Divider(u32),
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceRole {
    Lane(u32),
    Shoulder(LateralSide),
    Line(LineRole),
    Flare(LateralSide),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvFrame {
    pub u_axis: DVec3,
    pub v_axis: DVec3,
    pub u_shift: f64,
    pub v_shift: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadPiece {
    pub segment: usize,
    pub role: PieceRole,
    pub lateral: (f64, f64),
    pub corners: Vec<DVec3>,
    pub material: String,
    pub uv: UvFrame,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoadSample {
    pub s: f64,
    pub center: DVec3,
    pub tangent: DVec3,
    pub left: DVec2,
    pub roll: f64,
    pub left_edge: DVec3,
    pub right_edge: DVec3,
    pub left_lane_edge: DVec3,
    pub right_lane_edge: DVec3,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JunctionProblem {
    MissingTarget,
    SelfTarget,
    Parallel,
    TooFar,
    TooClose,
    Shallow,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RoadError {
    TooFewPoints,
    OutOfRange,
    TooShort(f64),
    Folded(usize),
    LinesTooWide,
    ShoulderTooSmall,
    TooWide,
    TooManyPieces(usize),
    Junction(JunctionProblem),
}

impl std::fmt::Display for RoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoadError::TooFewPoints => write!(f, "the road needs at least 2 different points"),
            RoadError::OutOfRange => write!(f, "the road goes outside the map (coordinates up to {MAX_COORD})"),
            RoadError::TooShort(l) => {
                write!(f, "the road is too short ({l:.2} units): add length or lower the segment length")
            }
            RoadError::Folded(n) => write!(
                f,
                "{n} segment(s) fold on the inside of a tight bend: lower the lane width or the shoulder, or open the bend"
            ),
            RoadError::LinesTooWide => write!(f, "the lines are too wide for the lane width"),
            RoadError::ShoulderTooSmall => write!(f, "the shoulder must be 0 or at least {MIN_BAND} unit"),
            RoadError::TooWide => write!(f, "the road is wider than {MAX_TOTAL_WIDTH} units"),
            RoadError::TooManyPieces(n) => {
                write!(f, "the road would need {n} brushes (limit {MAX_PIECES}): raise the segment length")
            }
            RoadError::Junction(p) => match p {
                JunctionProblem::MissingTarget => write!(f, "the road of the junction was not found"),
                JunctionProblem::SelfTarget => write!(f, "a road cannot join itself"),
                JunctionProblem::Parallel => {
                    write!(f, "the road is too close to parallel with the other road to join its side")
                }
                JunctionProblem::TooFar => write!(f, "the end of the road is too far from the side of the other road"),
                JunctionProblem::TooClose => write!(f, "the junction would leave the road shorter than 2 segments"),
                JunctionProblem::Shallow => write!(f, "the road reaches the other road at too shallow an angle"),
            },
        }
    }
}

pub type GroundFn<'a> = dyn Fn(DVec2, f64) -> Option<f64> + 'a;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Notch {
    pub side: LateralSide,
    pub from: f64,
    pub to: f64,
}

fn clip_piece(corners: &[DVec3], f0: f64, f1: f64) -> Vec<DVec3> {
    let mut out = corners.to_vec();
    for (a, b) in [(0, 2), (1, 3), (4, 6), (5, 7)] {
        out[a] = corners[a].lerp(corners[b], f0);
        out[b] = corners[a].lerp(corners[b], f1);
    }
    out
}

#[derive(Default)]
pub struct RoadEnv<'a> {
    pub ground: Option<&'a GroundFn<'a>>,
    pub target: Option<&'a Road>,
    pub notches: &'a [Notch],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Road {
    pub meta: RoadMeta,
    pub samples: Vec<RoadSample>,
    pub pieces: Vec<RoadPiece>,
    pub length: f64,
    pub half_width: f64,
    pub lane_half_width: f64,
}

pub(crate) struct Station {
    pub right: DVec2,
    pub left: DVec2,
    pub z: f64,
    pub tan: f64,
}

impl Station {
    pub fn top(&self, w: f64, half: f64) -> DVec3 {
        let f = if half > 0.0 { w / half } else { 0.0 };
        let p = self.right.lerp(self.left, (f + 1.0) * 0.5);
        DVec3::new(p.x, p.y, self.z + w * self.tan)
    }
}

#[derive(Clone, Debug)]
struct Band {
    w0: f64,
    w1: f64,
    role: PieceRole,
    dashed: bool,
    material: BandMaterial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BandMaterial {
    Road,
    Shoulder,
    Line,
    Center,
}

fn bands(meta: &RoadMeta) -> Result<Vec<Band>, RoadError> {
    let lh = meta.lane_area() * 0.5;
    let half = lh + meta.shoulder;
    if meta.shoulder > 0.0 && meta.shoulder < MIN_BAND {
        return Err(RoadError::ShoulderTooSmall);
    }
    if half * 2.0 > MAX_TOTAL_WIDTH {
        return Err(RoadError::TooWide);
    }
    let lw = meta.line_width;
    let mut lines: Vec<(f64, f64, LineRole, bool, BandMaterial)> = Vec::new();
    let mut add = |kind: LineKind, centre: f64, role: LineRole, material: BandMaterial, outer: Option<f64>| {
        let dashed = kind == LineKind::Dashed;
        match kind {
            LineKind::None => {}
            LineKind::Solid | LineKind::Dashed => match outer {
                Some(dir) => {
                    lines.push((centre.min(centre + dir * lw), centre.max(centre + dir * lw), role, dashed, material))
                }
                None => lines.push((centre - lw * 0.5, centre + lw * 0.5, role, dashed, material)),
            },
            LineKind::Double => match outer {
                Some(dir) => {
                    let a = (centre, centre + dir * lw);
                    let b = (centre + dir * 2.0 * lw, centre + dir * 3.0 * lw);
                    for (x, y) in [a, b] {
                        lines.push((x.min(y), x.max(y), role, false, material));
                    }
                }
                None => {
                    lines.push((centre - lw * 1.5, centre - lw * 0.5, role, false, material));
                    lines.push((centre + lw * 0.5, centre + lw * 1.5, role, false, material));
                }
            },
        }
    };
    add(meta.edge, -lh, LineRole::Edge(LateralSide::Right), BandMaterial::Line, Some(1.0));
    add(meta.edge, lh, LineRole::Edge(LateralSide::Left), BandMaterial::Line, Some(-1.0));
    for k in 1..meta.lanes {
        let x = -lh + f64::from(k) * meta.lane_width;
        if meta.lanes.is_multiple_of(2) && k == meta.lanes / 2 {
            add(meta.center, x, LineRole::Center, BandMaterial::Center, None);
        } else {
            add(meta.divider, x, LineRole::Divider(k), BandMaterial::Line, None);
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<Band> = Vec::new();
    if meta.shoulder >= MIN_BAND {
        out.push(Band {
            w0: -half,
            w1: -lh,
            role: PieceRole::Shoulder(LateralSide::Right),
            dashed: false,
            material: BandMaterial::Shoulder,
        });
    }
    let lane_at =
        |a: f64, b: f64| ((((a + b) * 0.5 + lh) / meta.lane_width).floor().max(0.0) as u32).min(meta.lanes - 1);
    let mut cursor = -lh;
    for (a, b, role, dashed, material) in &lines {
        if *a < -lh - 1e-9 || *b > lh + 1e-9 || *a < cursor - 1e-9 {
            return Err(RoadError::LinesTooWide);
        }
        let gap = *a - cursor;
        if gap > 1e-6 {
            if gap < MIN_BAND {
                return Err(RoadError::LinesTooWide);
            }
            out.push(Band {
                w0: cursor,
                w1: *a,
                role: PieceRole::Lane(lane_at(cursor, *a)),
                dashed: false,
                material: BandMaterial::Road,
            });
        }
        out.push(Band { w0: *a, w1: *b, role: PieceRole::Line(*role), dashed: *dashed, material: *material });
        cursor = *b;
    }
    let tail = lh - cursor;
    if tail > 1e-6 {
        if tail < MIN_BAND {
            return Err(RoadError::LinesTooWide);
        }
        out.push(Band {
            w0: cursor,
            w1: lh,
            role: PieceRole::Lane(lane_at(cursor, lh)),
            dashed: false,
            material: BandMaterial::Road,
        });
    }
    if meta.shoulder >= MIN_BAND {
        out.push(Band {
            w0: lh,
            w1: half,
            role: PieceRole::Shoulder(LateralSide::Left),
            dashed: false,
            material: BandMaterial::Shoulder,
        });
    }
    Ok(out)
}

fn clean_points(points: &[RoadPoint]) -> Result<Vec<RoadPoint>, RoadError> {
    let mut out: Vec<RoadPoint> = Vec::with_capacity(points.len());
    for p in points {
        if !p.is_valid() {
            return Err(RoadError::OutOfRange);
        }
        if out.last().is_none_or(|q| q.xy().distance(p.xy()) > MIN_POINT_GAP) {
            out.push(*p);
        }
    }
    if out.len() < 2 { Err(RoadError::TooFewPoints) } else { Ok(out) }
}

fn smooth(values: &mut [f64], fixed_ends: bool) {
    let n = values.len();
    if n < 3 {
        return;
    }
    for _ in 0..SMOOTH_PASSES {
        let prev = values.to_vec();
        for i in 1..n - 1 {
            values[i] = (prev[i - 1] + 2.0 * prev[i] + prev[i + 1]) * 0.25;
        }
        if !fixed_ends {
            values[0] = (prev[0] * 3.0 + prev[1]) * 0.25;
            values[n - 1] = (prev[n - 1] * 3.0 + prev[n - 2]) * 0.25;
        }
    }
}

pub struct Elevations {
    pub dense: Dense,
    pub elevation: Elevation,
}

pub fn elevations(meta: &RoadMeta) -> Result<Elevations, RoadError> {
    let pts = clean_points(&meta.points)?;
    let xy: Vec<DVec2> = pts.iter().map(|p| p.xy()).collect();
    let d = dense(&xy, meta.start_dir, meta.end_dir);
    let knots: Vec<f64> = (0..pts.len()).map(|k| d.control_arc(k)).collect();
    let zs: Vec<f64> = pts.iter().map(|p| p.z).collect();
    let elevation = Elevation::new(&knots, &zs, meta.start_slope, meta.end_slope);
    Ok(Elevations { dense: d, elevation })
}

pub fn build(meta: &RoadMeta, env: &RoadEnv<'_>) -> Result<Road, RoadError> {
    let meta = meta.clone().clamped();
    let band_list = bands(&meta)?;
    let mut pts = clean_points(&meta.points)?;
    let half = meta.total_width() * 0.5;
    let lh = meta.lane_area() * 0.5;
    let mut cut: Option<CutPlan> = None;
    if let Some(j) = meta.junction {
        let target = env.target.ok_or(RoadError::Junction(JunctionProblem::MissingTarget))?;
        if target.meta.uid == meta.uid {
            return Err(RoadError::Junction(JunctionProblem::SelfTarget));
        }
        cut = Some(junction::move_endpoint(&mut pts, j.end, target, half)?);
    }
    let xy: Vec<DVec2> = pts.iter().map(|p| p.xy()).collect();
    let d = dense(&xy, meta.start_dir, meta.end_dir);
    let len2 = d.length();
    if len2 < 1.0 {
        return Err(RoadError::TooShort(len2));
    }
    let n = ((len2 / meta.step).ceil() as usize).clamp(1, MAX_SEGMENTS);
    let centers = resample_dense(&d.points, n as u32);
    let knots: Vec<f64> = (0..pts.len()).map(|k| d.control_arc(k)).collect();
    let zs: Vec<f64> = pts.iter().map(|p| p.z).collect();
    let elevation = Elevation::new(&knots, &zs, meta.start_slope, meta.end_slope);
    let mut layout = layout_centers(centers, half * 2.0).map_err(|e| match e {
        CurveError::TooShort { segment } => RoadError::TooShort(segment),
        CurveError::OutOfRange => RoadError::OutOfRange,
        CurveError::Folded(l) => RoadError::Folded(l.len()),
        CurveError::TooFewPoints => RoadError::TooFewPoints,
    })?;
    if !layout.folded.is_empty() {
        return Err(RoadError::Folded(layout.folded.len()));
    }
    let count = layout.quads.len();
    let mut z: Vec<f64> = (0..=count).map(|i| elevation.at(len2 * i as f64 / count as f64)).collect();
    if meta.follow_terrain
        && let Some(ground) = env.ground
    {
        let mut sampled: Vec<f64> =
            z.iter().enumerate().map(|(i, zi)| ground(layout.centers[i], *zi).map_or(*zi, |g| g + meta.lift)).collect();
        smooth(&mut sampled, cut.is_some());
        if let Some(plan) = &cut {
            match plan.end {
                JunctionEnd::Start => sampled[0] = z[0],
                JunctionEnd::End => sampled[count] = z[count],
            }
        }
        z = sampled;
    }
    let cut_start = cut.as_ref().is_some_and(|p| p.end == JunctionEnd::Start);
    let cut_end = cut.as_ref().is_some_and(|p| p.end == JunctionEnd::End);
    if let (Some(d), false) = (meta.start_dir, cut_start) {
        let off = DVec2::new(-d.y, d.x) * half;
        let c = layout.centers[0];
        layout.quads[0][0] = c - off;
        layout.quads[0][3] = c + off;
    }
    if let (Some(d), false) = (meta.end_dir, cut_end) {
        let off = DVec2::new(-d.y, d.x) * half;
        let c = layout.centers[count];
        layout.quads[count - 1][1] = c - off;
        layout.quads[count - 1][2] = c + off;
    }
    if let Some(plan) = &cut {
        plan.cut_quads(&mut layout.quads, meta.step * 4.0 + meta.total_width() * 3.0)?;
    }
    let rolls = station_rolls(&layout.centers, &meta.bank);
    let stations: Vec<Station> = (0..=count)
        .map(|i| {
            let (right, left) = if i == 0 {
                (layout.quads[0][0], layout.quads[0][3])
            } else {
                (layout.quads[i - 1][1], layout.quads[i - 1][2])
            };
            let roll = rolls.get(i).copied().unwrap_or(0.0);
            Station { right, left, z: z[i], tan: bank_shift(1.0, 1.0, roll) }
        })
        .collect();
    let c3: Vec<DVec3> = (0..=count).map(|i| DVec3::new(layout.centers[i].x, layout.centers[i].y, z[i])).collect();
    let mut arc = Vec::with_capacity(count + 1);
    let mut s = meta.u0;
    arc.push(s);
    for w in c3.windows(2) {
        s += w[0].distance(w[1]);
        arc.push(s);
    }
    let length = arc[count] - meta.u0;
    let frame_of = |i: usize| -> UvFrame {
        let ua = (c3[i + 1] - c3[i]).try_normalize().unwrap_or(DVec3::X);
        let r = (stations[i].top(-half, half) + stations[i + 1].top(-half, half)) * 0.5;
        let l = (stations[i].top(half, half) + stations[i + 1].top(half, half)) * 0.5;
        let normal = ua.cross(l - r).try_normalize().unwrap_or(DVec3::Z);
        let va = ua.cross(normal);
        UvFrame {
            u_axis: ua,
            v_axis: va,
            u_shift: (arc[i] - c3[i].dot(ua)) / meta.tex_scale,
            v_shift: -c3[i].dot(va) / meta.tex_scale,
        }
    };
    let center_name = if meta.center_material.is_empty() { &meta.line_material } else { &meta.center_material };
    let seg_len = length / count as f64;
    let on = ((meta.dash / seg_len).round() as i64).max(1);
    let off = ((meta.gap / seg_len).round() as i64).max(1);
    let phase = (meta.u0 / seg_len).round() as i64;
    let total = band_list.len() * count;
    if total > MAX_PIECES {
        return Err(RoadError::TooManyPieces(total));
    }
    let mut pieces = Vec::with_capacity(total + 16);
    let mut frames = Vec::with_capacity(count);
    for i in 0..count {
        let frame = frame_of(i);
        frames.push(frame);
        let slot = (i as i64 + phase).rem_euclid(on + off);
        let dash_on = slot < on;
        for b in &band_list {
            let name = match b.material {
                BandMaterial::Road => &meta.material,
                BandMaterial::Shoulder => meta.shoulder_material_name(),
                BandMaterial::Line => {
                    if b.dashed && !dash_on {
                        &meta.material
                    } else {
                        &meta.line_material
                    }
                }
                BandMaterial::Center => {
                    if b.dashed && !dash_on {
                        &meta.material
                    } else {
                        center_name
                    }
                }
            };
            let (a, c) = (&stations[i], &stations[i + 1]);
            let top = |st: &Station, w: f64| st.top(w, half);
            let low = |p: DVec3| DVec3::new(p.x, p.y, p.z - meta.thickness);
            let t = [top(a, b.w0), top(a, b.w1), top(c, b.w0), top(c, b.w1)];
            let mut corners = Vec::with_capacity(8);
            corners.extend(t.iter().map(|p| low(*p)));
            corners.extend(t);
            let mut spans = vec![(0.0, 1.0)];
            if let PieceRole::Shoulder(side) = b.role {
                let (s0, s1) = (arc[i] - meta.u0, arc[i + 1] - meta.u0);
                for n in env.notches.iter().filter(|n| n.side == side) {
                    let (f0, f1) =
                        (((n.from - s0) / (s1 - s0)).clamp(0.0, 1.0), ((n.to - s0) / (s1 - s0)).clamp(0.0, 1.0));
                    let mut next = Vec::new();
                    for (a, c) in spans {
                        if f1 <= a || f0 >= c {
                            next.push((a, c));
                            continue;
                        }
                        if f0 - a > 0.01 {
                            next.push((a, f0));
                        }
                        if c - f1 > 0.01 {
                            next.push((f1, c));
                        }
                    }
                    spans = next;
                }
            }
            for (f0, f1) in spans {
                let pc = if f0 == 0.0 && f1 == 1.0 { corners.clone() } else { clip_piece(&corners, f0, f1) };
                pieces.push(RoadPiece {
                    segment: i,
                    role: b.role,
                    lateral: (b.w0, b.w1),
                    corners: pc,
                    material: name.to_string(),
                    uv: frame,
                });
            }
        }
    }
    if pieces.iter().flat_map(|p| p.corners.iter()).any(|c| !c.is_finite() || c.abs().max_element() > MAX_COORD) {
        return Err(RoadError::OutOfRange);
    }
    let samples: Vec<RoadSample> = (0..=count)
        .map(|i| {
            let st = &stations[i];
            let before = c3[i.saturating_sub(1)];
            let after = c3[(i + 1).min(count)];
            let tangent = (after - before).try_normalize().unwrap_or(DVec3::X);
            let left = (st.left - st.right).try_normalize().unwrap_or(DVec2::Y);
            RoadSample {
                s: arc[i] - meta.u0,
                center: c3[i],
                tangent,
                left,
                roll: rolls.get(i).copied().unwrap_or(0.0),
                left_edge: st.top(half, half),
                right_edge: st.top(-half, half),
                left_lane_edge: st.top(lh, half),
                right_lane_edge: st.top(-lh, half),
            }
        })
        .collect();
    let mut road = Road { meta, samples, pieces, length, half_width: half, lane_half_width: lh };
    if let (Some(plan), Some(j)) = (&cut, road.meta.junction)
        && j.flare > 0.0
    {
        let extra = junction::flare_pieces(&road, plan, &frames);
        road.pieces.extend(extra);
    }
    Ok(road)
}

impl Road {
    pub fn edge(&self, side: LateralSide, lanes_only: bool) -> Vec<DVec3> {
        self.samples
            .iter()
            .map(|s| match (side, lanes_only) {
                (LateralSide::Left, false) => s.left_edge,
                (LateralSide::Right, false) => s.right_edge,
                (LateralSide::Left, true) => s.left_lane_edge,
                (LateralSide::Right, true) => s.right_lane_edge,
            })
            .collect()
    }

    pub fn sample_at(&self, distance: f64) -> Option<RoadSample> {
        let first = self.samples.first()?;
        let last = self.samples.last()?;
        let d = distance.clamp(first.s, last.s);
        let i = self.samples.partition_point(|s| s.s <= d).saturating_sub(1).min(self.samples.len().saturating_sub(2));
        let (a, b) = (self.samples.get(i)?, self.samples.get(i + 1)?);
        let f = if b.s > a.s { ((d - a.s) / (b.s - a.s)).clamp(0.0, 1.0) } else { 0.0 };
        Some(RoadSample {
            s: d,
            center: a.center.lerp(b.center, f),
            tangent: a.tangent.lerp(b.tangent, f).try_normalize().unwrap_or(a.tangent),
            left: a.left.lerp(b.left, f).try_normalize().unwrap_or(a.left),
            roll: a.roll + (b.roll - a.roll) * f,
            left_edge: a.left_edge.lerp(b.left_edge, f),
            right_edge: a.right_edge.lerp(b.right_edge, f),
            left_lane_edge: a.left_lane_edge.lerp(b.left_lane_edge, f),
            right_lane_edge: a.right_lane_edge.lerp(b.right_lane_edge, f),
        })
    }

    pub fn pieces_with(&self, f: impl Fn(&PieceRole) -> bool) -> Vec<&RoadPiece> {
        self.pieces.iter().filter(|p| f(&p.role)).collect()
    }

    pub fn bounds(&self) -> Option<hp_geom::Aabb> {
        let pts: Vec<DVec3> = self.pieces.iter().flat_map(|p| p.corners.iter().copied()).collect();
        (!pts.is_empty()).then(|| hp_geom::Aabb::from_points(pts))
    }
}
