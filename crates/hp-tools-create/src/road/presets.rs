use std::f64::consts::{FRAC_PI_2, PI, TAU};

use glam::{DVec2, DVec3};

use super::build::{LateralSide, LineRole, MAX_PIECES, PieceRole, Road, RoadError, RoadPiece, RoadSample, UvFrame};
use super::model::{LineKind, MAX_COORD, MAX_TOTAL_WIDTH, RoadLayout, RoadMeta};

struct Node {
    p: DVec2,
    open: bool,
}

fn direction(angle: f64) -> DVec2 {
    DVec2::new(angle.cos(), angle.sin())
}

fn outline(meta: &RoadMeta, length: f64, offset: f64) -> Vec<Node> {
    let half = meta.lane_area() * 0.5 + offset;
    if meta.layout == RoadLayout::CulDeSac {
        let radius = meta.radius + offset;
        let angle = (half / radius).clamp(-1.0, 1.0).asin();
        let count = ((TAU * meta.radius / meta.step).ceil() as usize).clamp(12, 128);
        let mut out = vec![Node { p: DVec2::new(0.0, -half), open: false }];
        for i in 0..=count {
            let a = PI + angle + (TAU - 2.0 * angle) * i as f64 / count as f64;
            out.push(Node { p: DVec2::new(length, 0.0) + direction(a) * radius, open: false });
        }
        out.push(Node { p: DVec2::new(0.0, half), open: true });
        return out;
    }
    let angles: &[f64] = match meta.layout {
        RoadLayout::Tee => &[-FRAC_PI_2, 0.0, FRAC_PI_2],
        RoadLayout::Cross => &[0.0, FRAC_PI_2, PI, PI + FRAC_PI_2],
        RoadLayout::Wye => &[0.0, TAU / 3.0, TAU * 2.0 / 3.0],
        _ => &[],
    };
    let radius = (meta.radius - offset).max(0.0);
    let mut out = Vec::new();
    for (i, angle) in angles.iter().copied().enumerate() {
        let dir = direction(angle);
        let left = DVec2::new(-dir.y, dir.x);
        out.push(Node { p: dir * length - left * half, open: true });
        out.push(Node { p: dir * length + left * half, open: false });
        let next = angles.get(i + 1).copied().unwrap_or(angles[0] + TAU);
        let gap = next - angle;
        if gap >= PI - 1e-8 {
            continue;
        }
        let bisector = direction(angle + gap * 0.5);
        let center = bisector * ((half + radius) / (gap * 0.5).sin());
        let count = ((meta.radius * (PI - gap) / meta.step).ceil() as usize).clamp(1, 32);
        for k in 0..=count {
            let a = angle - FRAC_PI_2 - (PI - gap) * k as f64 / count as f64;
            out.push(Node { p: center + direction(a) * radius, open: false });
        }
    }
    out
}

struct Builder<'a> {
    meta: &'a RoadMeta,
    origin: DVec3,
    dir: DVec2,
    left: DVec2,
    slope: f64,
    pieces: Vec<RoadPiece>,
}

impl Builder<'_> {
    fn world(&self, point: DVec2) -> DVec3 {
        let xy = self.origin.truncate() + self.dir * point.x + self.left * point.y;
        DVec3::new(xy.x, xy.y, self.origin.z + point.x * self.slope)
    }

    fn polygon(&mut self, points: &[DVec2], material: &str, role: PieceRole) -> Result<(), RoadError> {
        let mut polygon = Vec::new();
        for p in points {
            if polygon.last().is_none_or(|last: &DVec2| last.distance_squared(*p) > 1e-12) {
                polygon.push(*p);
            }
        }
        if polygon.len() > 1 && polygon[0].distance_squared(polygon[polygon.len() - 1]) < 1e-12 {
            polygon.pop();
        }
        if polygon.len() < 3 {
            return Ok(());
        }
        let area: f64 =
            polygon.iter().zip(polygon.iter().cycle().skip(1)).take(polygon.len()).map(|(a, b)| a.perp_dot(*b)).sum();
        if area.abs() < 1e-7 {
            return Ok(());
        }
        if area < 0.0 {
            polygon.reverse();
        }
        let parts = hp_geom::convex_decompose(&polygon).ok_or(RoadError::LayoutSize)?;
        for part in parts {
            let mut corners: Vec<DVec3> = part.iter().map(|i| self.world(polygon[*i])).collect();
            corners.extend((0..corners.len()).map(|i| corners[i] - DVec3::Z * self.meta.thickness).collect::<Vec<_>>());
            if corners.iter().any(|p| !p.is_finite() || p.abs().max_element() > MAX_COORD) {
                return Err(RoadError::OutOfRange);
            }
            self.pieces.push(RoadPiece {
                segment: self.pieces.len(),
                role,
                lateral: (0.0, 0.0),
                corners,
                material: material.to_owned(),
                uv: UvFrame {
                    u_axis: self.dir.extend(self.slope).normalize(),
                    v_axis: self.left.extend(0.0),
                    u_shift: 0.0,
                    v_shift: 0.0,
                },
            });
            if self.pieces.len() > MAX_PIECES {
                return Err(RoadError::TooManyPieces(self.pieces.len()));
            }
        }
        Ok(())
    }

    fn band(
        &mut self,
        length: f64,
        inner: f64,
        outer: f64,
        material: &str,
        role: PieceRole,
        dashed: bool,
    ) -> Result<(), RoadError> {
        let a = outline(self.meta, length, inner);
        let b = outline(self.meta, length, outer);
        let mut distance = 0.0;
        for i in 0..a.len() {
            let j = (i + 1) % a.len();
            if a[i].open {
                continue;
            }
            let span = (a[j].p.distance(a[i].p) + b[j].p.distance(b[i].p)) * 0.5;
            let mut at = 0.0;
            while at < span - 1e-8 {
                let phase = (distance + at).rem_euclid(self.meta.dash + self.meta.gap);
                let line = !dashed || phase < self.meta.dash;
                let end = if !dashed {
                    span
                } else {
                    (at + if line { self.meta.dash - phase } else { self.meta.dash + self.meta.gap - phase }).min(span)
                };
                let end = end.max((at + 1e-6).min(span));
                let (f0, f1) = (at / span, end / span);
                self.polygon(
                    &[
                        a[i].p.lerp(a[j].p, f0),
                        a[i].p.lerp(a[j].p, f1),
                        b[i].p.lerp(b[j].p, f1),
                        b[i].p.lerp(b[j].p, f0),
                    ],
                    if line { material } else { &self.meta.material },
                    if line { role } else { PieceRole::Lane(0) },
                )?;
                at = end;
            }
            distance += span;
        }
        Ok(())
    }
}

pub(super) fn build(meta: &RoadMeta) -> Result<Road, RoadError> {
    if meta.points.len() != 2 {
        return Err(RoadError::LayoutPoints);
    }
    if meta.follow_terrain
        || meta.bank.roll_start.abs() > 1e-9
        || meta.bank.roll_end.abs() > 1e-9
        || meta.bank.auto_gain.abs() > 1e-9
        || meta.junction.is_some()
    {
        return Err(RoadError::LayoutOptions);
    }
    if meta.total_width() > MAX_TOTAL_WIDTH {
        return Err(RoadError::TooWide);
    }
    if meta.shoulder > 0.0 && meta.shoulder < 1.0 {
        return Err(RoadError::ShoulderTooSmall);
    }
    if meta.points.iter().any(|p| !p.is_valid()) {
        return Err(RoadError::OutOfRange);
    }
    let origin = meta.points[0].xyz();
    let delta = meta.points[1].xyz() - origin;
    let length = delta.truncate().length();
    let half = meta.lane_area() * 0.5;
    let line_depth = if meta.edge == LineKind::None {
        0.0
    } else {
        meta.line_width * if meta.edge == LineKind::Double { 3.0 } else { 1.0 }
    };
    if half - line_depth < 1.0 {
        return Err(RoadError::LinesTooWide);
    }
    if length <= half + meta.radius + meta.shoulder + 1.0
        || (meta.layout == RoadLayout::CulDeSac && meta.radius <= half + 1.0)
    {
        return Err(RoadError::LayoutSize);
    }
    let dir = delta.truncate() / length;
    let left = DVec2::new(-dir.y, dir.x);
    let mut builder = Builder { meta, origin, dir, left, slope: delta.z / length, pieces: Vec::new() };
    let interior: Vec<DVec2> = outline(meta, length, -line_depth).into_iter().map(|n| n.p).collect();
    builder.polygon(&interior, &meta.material, PieceRole::Lane(0))?;
    if meta.edge != LineKind::None {
        let role = PieceRole::Line(LineRole::Edge(LateralSide::Left));
        builder.band(
            length,
            -line_depth,
            -line_depth + meta.line_width,
            &meta.line_material,
            role,
            meta.edge == LineKind::Dashed,
        )?;
        if meta.edge == LineKind::Double {
            builder.band(
                length,
                -2.0 * meta.line_width,
                -meta.line_width,
                &meta.material,
                PieceRole::Lane(0),
                false,
            )?;
            builder.band(length, -meta.line_width, 0.0, &meta.line_material, role, false)?;
        }
    }
    if meta.shoulder > 0.0 {
        builder.band(
            length,
            0.0,
            meta.shoulder,
            meta.shoulder_material_name(),
            PieceRole::Shoulder(LateralSide::Left),
            false,
        )?;
    }
    let tangent = delta.normalize();
    let samples = meta
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let center = p.xyz();
            RoadSample {
                s: meta.u0 + i as f64 * length,
                center,
                tangent,
                left,
                roll: 0.0,
                left_edge: center + left.extend(0.0) * (half + meta.shoulder),
                right_edge: center - left.extend(0.0) * (half + meta.shoulder),
                left_lane_edge: center + left.extend(0.0) * half,
                right_lane_edge: center - left.extend(0.0) * half,
            }
        })
        .collect();
    Ok(Road {
        meta: meta.clone(),
        samples,
        pieces: builder.pieces,
        length,
        half_width: half + meta.shoulder,
        lane_half_width: half,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::road::{RoadPoint, apply::road_solids};

    #[test]
    fn preset_bands_cover_the_footprint_without_missing_area() {
        for layout in [RoadLayout::Tee, RoadLayout::Cross, RoadLayout::Wye, RoadLayout::CulDeSac] {
            for radius in [0.0, 8.0, 256.0] {
                if layout == RoadLayout::CulDeSac && radius < 256.0 {
                    continue;
                }
                for edge in LineKind::ALL {
                    let meta = RoadMeta {
                        layout,
                        radius,
                        edge,
                        points: vec![RoadPoint::new(0.0, 0.0, 0.0), RoadPoint::new(1024.0, 0.0, 0.0)],
                        ..RoadMeta::default()
                    };
                    let road = build(&meta).unwrap();
                    let expected = hp_geom::signed_area(
                        &outline(&meta, 1024.0, meta.shoulder).into_iter().map(|n| n.p).collect::<Vec<_>>(),
                    );
                    let actual: f64 = road_solids(&road, 16, 100)
                        .unwrap()
                        .iter()
                        .map(|s| hp_geom::validate_solid(s).unwrap().volume() / meta.thickness)
                        .sum();
                    assert!(
                        (actual - expected).abs() < expected * 1e-6,
                        "{layout:?}/{edge:?}/{radius}: {actual} vs {expected}"
                    );
                }
            }
        }
    }
}
