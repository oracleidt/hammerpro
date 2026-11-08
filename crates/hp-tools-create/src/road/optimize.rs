use std::collections::HashMap;

use super::build::{PieceRole, RoadPiece};

fn same_uv(a: &RoadPiece, b: &RoadPiece) -> bool {
    a.uv.u_axis.distance(b.uv.u_axis) <= 1e-9
        && a.uv.v_axis.distance(b.uv.v_axis) <= 1e-9
        && (a.uv.u_shift - b.uv.u_shift).abs() <= 1e-6
        && (a.uv.v_shift - b.uv.v_shift).abs() <= 1e-6
}

fn lateral_extend(a: &mut RoadPiece, b: &RoadPiece) -> bool {
    if a.segment != b.segment
        || a.material != b.material
        || a.lateral.1 != b.lateral.0
        || a.corners.len() != 8
        || b.corners.len() != 8
        || !same_uv(a, b)
    {
        return false;
    }
    for (left, right) in [(1, 0), (3, 2), (5, 4), (7, 6)] {
        if a.corners[left].distance(b.corners[right]) > 1e-8 {
            return false;
        }
    }
    let Some(normal) = (a.corners[6] - a.corners[4]).cross(a.corners[5] - a.corners[4]).try_normalize() else {
        return false;
    };
    if a.corners[4..].iter().chain(&b.corners[4..]).any(|p| (*p - a.corners[4]).dot(normal).abs() > 1e-7) {
        return false;
    }
    for end in [1, 3, 5, 7] {
        a.corners[end] = b.corners[end];
    }
    a.lateral.1 = b.lateral.1;
    if matches!(b.role, PieceRole::Lane(_)) {
        a.role = b.role;
    }
    true
}

fn extend(a: &mut RoadPiece, b: &RoadPiece) -> bool {
    if a.corners.len() != 8 || b.corners.len() != 8 || !same_uv(a, b) {
        return false;
    }
    for (start, end) in [(0, 2), (1, 3), (4, 6), (5, 7)] {
        let joint = a.corners[end];
        if joint.distance(b.corners[start]) > 1e-8 {
            return false;
        }
        let delta = b.corners[end] - a.corners[start];
        let length2 = delta.length_squared();
        if length2 <= 1e-12 {
            return false;
        }
        let t = (joint - a.corners[start]).dot(delta) / length2;
        if t <= 0.0 || t >= 1.0 || (a.corners[start] + delta * t).distance(joint) > 1e-7 {
            return false;
        }
    }
    for end in [2, 3, 6, 7] {
        a.corners[end] = b.corners[end];
    }
    true
}

fn merge_width(pieces: Vec<RoadPiece>) -> Vec<RoadPiece> {
    let mut lateral: Vec<RoadPiece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        if let Some(previous) = lateral.last_mut()
            && lateral_extend(previous, &piece)
        {
            continue;
        }
        lateral.push(piece);
    }
    lateral
}

fn merge_length(pieces: Vec<RoadPiece>) -> Vec<RoadPiece> {
    let mut out: Vec<RoadPiece> = Vec::with_capacity(pieces.len());
    let mut last: HashMap<(PieceRole, u64, u64, String), usize> = HashMap::new();
    for piece in pieces {
        let key = (piece.role, piece.lateral.0.to_bits(), piece.lateral.1.to_bits(), piece.material.clone());
        if let Some(index) = last.get(&key).copied()
            && extend(&mut out[index], &piece)
        {
            continue;
        }
        last.insert(key, out.len());
        out.push(piece);
    }
    out
}

pub(super) fn coalesce(pieces: &mut Vec<RoadPiece>) {
    let original = std::mem::take(pieces);
    let length = merge_length(original.clone());
    let width = merge_length(merge_width(original));
    *pieces = if width.len() < length.len() { width } else { length };
}
