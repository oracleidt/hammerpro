use glam::DVec3;
use hp_app::grid::GridFrame;
use hp_geom::Aabb;

use super::model::{
    MAX_COLLAR_EVERY, MAX_LENGTH, MAX_MOVES, MAX_PIECES, MAX_RADIUS, MAX_SEGMENTS, MAX_SIDES, MAX_SLICES, MIN_SIDES,
    PieceKind, PipeError, PipeMove, PipePiece, PipePlan, PipeSpec, Shape, Turn,
};

const EPS: f64 = 1e-6;
const GRID_FLOOR: f64 = 0.125;

struct Steps<'a> {
    frame: &'a GridFrame,
    snap: bool,
}

impl Steps<'_> {
    fn along(&self, d: DVec3) -> f64 {
        let m = self.frame.axes;
        let cols = [m.x_axis, m.y_axis, m.z_axis];
        let mut best = 0;
        for j in 1..3 {
            if cols[j].dot(d).abs() > cols[best].dot(d).abs() {
                best = j;
            }
        }
        self.frame.step[best].max(GRID_FLOOR)
    }

    fn nominal(&self) -> f64 {
        self.frame.nominal().max(GRID_FLOOR)
    }

    fn length(&self, x: f64, d: DVec3) -> f64 {
        if !self.snap {
            return x;
        }
        let s = self.along(d);
        crate::common::clean((x / s).round() * s)
    }

    fn size(&self, x: f64) -> f64 {
        if !self.snap {
            return x;
        }
        let s = self.nominal();
        crate::common::clean(((x / s).round() * s).max(s))
    }

    fn fine(&self, x: f64) -> f64 {
        if !self.snap {
            return x;
        }
        let s = (self.nominal() / 2.0).max(GRID_FLOOR);
        crate::common::clean(((x / s).round() * s).max(s))
    }
}

pub fn nearest_axis(frame: &GridFrame, v: DVec3) -> DVec3 {
    let m = frame.axes;
    let cols = [m.x_axis, m.y_axis, m.z_axis];
    let mut best = (m.x_axis, f64::NEG_INFINITY);
    for c in cols {
        for s in [c, -c] {
            let d = s.dot(v);
            if d > best.1 {
                best = (s, d);
            }
        }
    }
    best.0
}

pub fn heading_frame(dir: DVec3, frame: &GridFrame) -> (DVec3, DVec3) {
    let z = frame.axes.z_axis;
    let up = z - dir * dir.dot(z);
    if up.length_squared() > 1e-9 {
        return (dir, up.normalize());
    }
    let x = frame.axes.x_axis;
    (dir, (x - dir * dir.dot(x)).normalize_or_zero())
}

fn rotate(v: DVec3, axis: DVec3, degrees: f64) -> DVec3 {
    let (s, c) = if degrees == 90.0 {
        (1.0, 0.0)
    } else if degrees == 0.0 {
        (0.0, 1.0)
    } else {
        degrees.to_radians().sin_cos()
    };
    v * c + axis.cross(v) * s + axis * axis.dot(v) * (1.0 - c)
}

fn turn_direction(turn: Turn, dir: DVec3, up: DVec3) -> DVec3 {
    let right = dir.cross(up);
    match turn {
        Turn::Left => -right,
        Turn::Right => right,
        Turn::Up => up,
        Turn::Down => -up,
    }
}

pub fn turn_between(dir: DVec3, up: DVec3, next: DVec3) -> Option<Turn> {
    Turn::ALL.into_iter().find(|t| turn_direction(*t, dir, up).dot(next) > 0.999)
}

pub fn moves_from_points(points: &[DVec3], frame: &GridFrame) -> Result<(DVec3, Vec<PipeMove>), PipeError> {
    let mut runs: Vec<(DVec3, f64)> = Vec::new();
    for w in points.windows(2) {
        let d = w[1] - w[0];
        let len = d.length();
        if len < EPS {
            continue;
        }
        let dir = d / len;
        match runs.last_mut() {
            Some((rd, rl)) if rd.dot(dir) > 0.999_999 => *rl += len,
            _ => runs.push((dir, len)),
        }
    }
    let Some(&(first, _)) = runs.first() else {
        return Err(PipeError::Invalid("the route needs at least two different points".to_owned()));
    };
    let (mut dir, mut up) = heading_frame(first, frame);
    let mut moves = Vec::new();
    for (i, (d, len)) in runs.iter().enumerate() {
        if i > 0 {
            let turn = turn_between(dir, up, *d).ok_or_else(|| {
                PipeError::Invalid("a pipe turns by 90 degrees only: no U-turns or slanted turns".to_owned())
            })?;
            let next = turn_direction(turn, dir, up);
            let axis = dir.cross(next).normalize_or_zero();
            up = rotate(up, axis, 90.0);
            dir = next;
            moves.push(PipeMove::Turn(turn));
        }
        moves.push(PipeMove::Forward(*len));
    }
    Ok((first, moves))
}

struct Leg {
    start: DVec3,
    dir: DVec3,
    up: DVec3,
    len: f64,
}

struct Ring {
    outer: Vec<DVec3>,
    inner: Vec<DVec3>,
}

fn sides_of(spec: &PipeSpec) -> usize {
    match spec.shape {
        Shape::Square => 4,
        Shape::Round => spec.sides as usize,
    }
}

fn ring(center: DVec3, dir: DVec3, up: DVec3, n: usize, outer: f64, inner: Option<f64>) -> Ring {
    let right = dir.cross(up);
    let half = std::f64::consts::PI / n as f64;
    let scale = 1.0 / half.cos();
    let corner = |apothem: f64, k: usize| {
        let (x, y) = if n == 4 {
            let signs = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
            (signs[k].0 * apothem, signs[k].1 * apothem)
        } else {
            let a = half + 2.0 * half * k as f64;
            (apothem * scale * a.cos(), apothem * scale * a.sin())
        };
        center + right * x + up * y
    };
    Ring {
        outer: (0..n).map(|k| corner(outer, k)).collect(),
        inner: inner.map_or_else(Vec::new, |r| (0..n).map(|k| corner(r, k)).collect()),
    }
}

struct Out {
    pieces: Vec<PipePiece>,
    wire: Vec<[DVec3; 2]>,
}

impl Out {
    fn tube(&mut self, a: &Ring, b: &Ring, kind: PieceKind) {
        let n = a.outer.len();
        if a.inner.is_empty() {
            let mut points = a.outer.clone();
            points.extend_from_slice(&b.outer);
            self.pieces.push(PipePiece { kind, points });
        } else {
            for i in 0..n {
                let j = (i + 1) % n;
                self.pieces.push(PipePiece {
                    kind,
                    points: vec![
                        a.outer[i], a.outer[j], a.inner[i], a.inner[j], b.outer[i], b.outer[j], b.inner[i], b.inner[j],
                    ],
                });
            }
        }
        for i in 0..n {
            let j = (i + 1) % n;
            self.wire.push([a.outer[i], a.outer[j]]);
            self.wire.push([b.outer[i], b.outer[j]]);
            self.wire.push([a.outer[i], b.outer[i]]);
            if !a.inner.is_empty() {
                self.wire.push([a.inner[i], a.inner[j]]);
                self.wire.push([b.inner[i], b.inner[j]]);
                self.wire.push([a.inner[i], b.inner[i]]);
            }
        }
    }
}

fn normalized(spec: &PipeSpec, steps: &Steps<'_>) -> Result<PipeSpec, PipeError> {
    let mut s = spec.clone();
    let bad = |msg: &str| Err(PipeError::Invalid(msg.to_owned()));
    if !s.start.is_finite() || !s.direction.is_finite() || s.direction.length_squared() < 1e-12 {
        return bad("the start and the direction must be finite numbers");
    }
    if s.moves.is_empty() {
        return bad("the route is empty: give at least one length");
    }
    if s.moves.len() > MAX_MOVES {
        return Err(PipeError::Invalid(format!("a route holds at most {MAX_MOVES} moves")));
    }
    for m in &s.moves {
        if let PipeMove::Forward(l) = m
            && !(l.is_finite() && *l > 0.0 && *l <= MAX_LENGTH)
        {
            return Err(PipeError::Invalid(format!("a length must be a number from 0 to {MAX_LENGTH}")));
        }
    }
    let sizes = [s.radius, s.wall, s.bend_radius, s.collar_size, s.collar_length, s.segment_length];
    if sizes.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return bad("sizes must be positive numbers");
    }
    if !(1.0..=MAX_RADIUS).contains(&s.radius) {
        return Err(PipeError::Invalid(format!("the radius must be from 1 to {MAX_RADIUS}")));
    }
    s.sides = s.sides.clamp(MIN_SIDES, MAX_SIDES);
    s.segments = s.segments.clamp(1, MAX_SEGMENTS);
    s.bend_slices = s.bend_slices.clamp(1, MAX_SLICES);
    s.collar_every = s.collar_every.min(MAX_COLLAR_EVERY);
    if s.snap {
        s.start = steps.frame.snap_point(s.start);
        s.direction = nearest_axis(steps.frame, s.direction);
    } else {
        s.direction = s.direction.normalize();
    }
    s.radius = steps.size(s.radius);
    if s.hollow {
        s.wall = steps.fine(s.wall.max(0.01));
        if s.wall >= s.radius - EPS {
            return bad("the wall must be thinner than the radius (the radius is half the width)");
        }
    }
    s.bend_radius = steps.size(s.bend_radius.max(s.radius));
    if s.bend_radius <= s.radius + EPS {
        s.bend_radius = s.radius + steps.nominal();
    }
    if s.collar_size > 0.0 {
        s.collar_size = steps.fine(s.collar_size);
        s.collar_length = steps.fine(s.collar_length.max(0.01));
    }
    if s.segment_length > 0.0 {
        s.segment_length = steps.size(s.segment_length);
    }
    Ok(s)
}

type Route = (Vec<Leg>, DVec3, DVec3);

fn legs_of(spec: &PipeSpec, steps: &Steps<'_>, frame: &GridFrame) -> Result<Route, PipeError> {
    let (mut dir, mut up) = heading_frame(spec.direction, frame);
    let mut pos = spec.start;
    let mut moves = spec.moves.clone();
    while matches!(moves.last(), Some(PipeMove::Turn(_))) {
        moves.pop();
    }
    let mut legs: Vec<Leg> = Vec::new();
    let mut open = false;
    let mut previous_turn = false;
    for m in moves {
        match m {
            PipeMove::Forward(l) => {
                let l = steps.length(l, dir);
                if l <= EPS {
                    return Err(PipeError::Invalid("a length is smaller than the grid step".to_owned()));
                }
                match legs.last_mut() {
                    Some(leg) if open => leg.len += l,
                    _ => legs.push(Leg { start: pos, dir, up, len: l }),
                }
                pos += dir * l;
                open = true;
                previous_turn = false;
            }
            PipeMove::Turn(t) => {
                if previous_turn && !legs.is_empty() {
                    return Err(PipeError::Invalid("two turns in a row: put a length between them".to_owned()));
                }
                let next = turn_direction(t, dir, up);
                let axis = dir.cross(next).normalize_or_zero();
                up = rotate(up, axis, 90.0);
                dir = next;
                open = false;
                previous_turn = true;
            }
        }
    }
    if legs.is_empty() {
        return Err(PipeError::Invalid("the route has no length".to_owned()));
    }
    Ok((legs, pos, dir))
}

fn estimate(spec: &PipeSpec, legs: usize) -> usize {
    let n = sides_of(spec);
    let per = if spec.hollow { n } else { 1 };
    let segs = if spec.segment_length > 0.0 { MAX_SEGMENTS as usize } else { spec.segments as usize };
    let collars = if spec.collar_size > 0.0 { (segs + 2) * n * legs } else { 0 };
    legs * segs * per + legs.saturating_sub(1) * spec.bend_slices as usize * per + collars
}

pub fn plan(spec: &PipeSpec, frame: &GridFrame) -> Result<PipePlan, PipeError> {
    let steps = Steps { frame, snap: spec.snap };
    let spec = normalized(spec, &steps)?;
    let (legs, end, end_dir) = legs_of(&spec, &steps, frame)?;
    let count = legs.len();
    let guess = estimate(&spec, count);
    if guess > MAX_PIECES * 8 {
        return Err(PipeError::TooMany(guess));
    }
    let n = sides_of(&spec);
    let r = spec.radius;
    let inner = spec.hollow.then_some(r - spec.wall);
    let big_r = spec.bend_radius;
    let mut out = Out { pieces: Vec::new(), wire: Vec::new() };
    let mut centerline = vec![spec.start];
    let mut length = 0.0;
    let mut bends = 0usize;
    let mut collars = 0usize;

    for (i, leg) in legs.iter().enumerate() {
        let lead = if i > 0 { big_r } else { 0.0 };
        let trail = if i + 1 < count { big_r } else { 0.0 };
        let span = leg.len - lead - trail;
        if span < -EPS {
            return Err(PipeError::TooShort { leg: i + 1, length: leg.len, needed: lead + trail });
        }
        let span = span.max(0.0);
        let s0 = leg.start + leg.dir * lead;
        length += span;
        if span > EPS {
            let wanted = if spec.segment_length > 0.0 {
                ((span / spec.segment_length).ceil() as u32).clamp(1, MAX_SEGMENTS)
            } else {
                spec.segments
            };
            let step = steps.along(leg.dir);
            let mut cuts = vec![0.0];
            for k in 1..wanted {
                let mut t = span * f64::from(k) / f64::from(wanted);
                if spec.snap {
                    t = crate::common::clean((t / step).round() * step);
                }
                if t > cuts.last().copied().unwrap_or(0.0) + EPS && t < span - EPS {
                    cuts.push(t);
                }
            }
            cuts.push(span);
            let at = |t: f64| ring(s0 + leg.dir * t, leg.dir, leg.up, n, r, inner);
            for w in cuts.windows(2) {
                out.tube(&at(w[0]), &at(w[1]), PieceKind::Straight);
            }
            if spec.collar_size > 0.0 {
                let cl = spec.collar_length;
                let outer = r + spec.collar_size;
                let collar = |out: &mut Out, from: f64, to: f64| {
                    let a = ring(s0 + leg.dir * from, leg.dir, leg.up, n, outer, Some(r));
                    let b = ring(s0 + leg.dir * to, leg.dir, leg.up, n, outer, Some(r));
                    out.tube(&a, &b, PieceKind::Collar);
                };
                if spec.collar_every > 0 {
                    for (k, t) in cuts.iter().enumerate().skip(1).take(cuts.len().saturating_sub(2)) {
                        if (k as u32).is_multiple_of(spec.collar_every)
                            && *t - cl / 2.0 > EPS
                            && *t + cl / 2.0 < span - EPS
                        {
                            collar(&mut out, *t - cl / 2.0, *t + cl / 2.0);
                            collars += 1;
                        }
                    }
                }
                let at_start = (i == 0 && spec.collar_ends) || (i > 0 && spec.collar_bends);
                let at_end = (i + 1 == count && spec.collar_ends) || (i + 1 < count && spec.collar_bends);
                let fits_start = at_start && span >= cl - EPS;
                if fits_start {
                    collar(&mut out, 0.0, cl);
                    collars += 1;
                }
                if at_end && span >= cl - EPS && !(fits_start && span < cl * 2.0 - EPS) {
                    collar(&mut out, span - cl, span);
                    collars += 1;
                }
            }
        }
        centerline.push(s0 + leg.dir * span);
        if i + 1 < count {
            let next = &legs[i + 1];
            let corner = leg.start + leg.dir * leg.len;
            let t0 = corner - leg.dir * big_r;
            let center = t0 + next.dir * big_r;
            let axis = leg.dir.cross(next.dir).normalize_or_zero();
            let slices = spec.bend_slices;
            let ring_at = |k: u32| {
                let deg = 90.0 * f64::from(k) / f64::from(slices);
                let d = rotate(leg.dir, axis, deg);
                let u = rotate(leg.up, axis, deg);
                let c = if k == 0 {
                    t0
                } else if k == slices {
                    corner + next.dir * big_r
                } else {
                    let (s, c) = deg.to_radians().sin_cos();
                    center + (-next.dir * c + leg.dir * s) * big_r
                };
                (c, d, u)
            };
            for k in 0..slices {
                let (c0, d0, u0) = ring_at(k);
                let (c1, d1, u1) = ring_at(k + 1);
                out.tube(&ring(c0, d0, u0, n, r, inner), &ring(c1, d1, u1, n, r, inner), PieceKind::Bend);
                centerline.push(c1);
            }
            length += big_r * std::f64::consts::FRAC_PI_2;
            bends += 1;
        }
    }
    if out.pieces.len() > MAX_PIECES {
        return Err(PipeError::TooMany(out.pieces.len()));
    }
    if out.pieces.is_empty() {
        return Err(PipeError::Invalid("the route is too short for the bends".to_owned()));
    }
    let bounds = Aabb::from_points(out.pieces.iter().flat_map(|p| p.points.iter().copied()));
    Ok(PipePlan {
        spec,
        pieces: out.pieces,
        wire: out.wire,
        centerline,
        end,
        end_direction: end_dir,
        length,
        runs: count,
        bends,
        collars,
        bounds,
    })
}
