use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Hierarchy {
    start: i32,
    end: i32,
    highest: bool,
    growth: f32,
    maximum: f32,
    bulge: f32,
    bulge_control: i32,
    midpoint: f32,
    bias: V3,
    absolute: V3,
}

impl Hierarchy {
    pub(super) fn compile(op: &Operator) -> Self {
        Self {
            start: i(op, "start control point number", 0).clamp(0, 63),
            end: i(op, "end control point number", 1).clamp(0, 63),
            highest: b(op, "use highest supplied end point", false),
            growth: f(op, "growth time", 0.0),
            maximum: f(op, "maximum distance", 0.0),
            bulge: f(op, "bulge", 0.0),
            bulge_control: i(op, "bulge control 0=random 1=orientation of start pnt 2=orientation of end point", 0),
            midpoint: f(op, "mid point position", 0.5),
            bias: v(op, "distance_bias", [1.0; 3]),
            absolute: v(op, "distance_bias_absolute_value", [0.0; 3]),
        }
    }
}

pub(super) fn middle(
    points: &Points,
    start: i32,
    end: i32,
    midpoint: f32,
    bulge: f32,
    orientation: i32,
    random: V3,
) -> V3 {
    let a = points.get(start);
    let z = points.get(end);
    let middle = lerp3(a, z, midpoint);
    if orientation == 0 {
        return add(middle, scale(random, bulge));
    }
    let direction = points.orientation(if orientation == 2 { end } else { start })[0];
    let distance = length(sub(z, a));
    let direction_length = length(direction);
    if direction_length <= f32::EPSILON || distance <= f32::EPSILON {
        return middle;
    }
    let perpendicular = 1.0 - dot(scale(sub(z, a), distance.recip()), direction).abs();
    add(middle, scale(direction, distance * bulge * perpendicular / direction_length))
}

pub(super) fn hierarchy(op: &Hierarchy, points: &Points, time: f32, born: f32, random: V3, rng: &mut Rng) -> V3 {
    let end = if op.highest { points.highest().min(63) as i32 } else { op.end };
    let minimum = (op.start + 1).min(end);
    let mut maximum = end as f32;
    let mut amount = 1.0;
    if time <= op.growth && end > 0 {
        let growth =
            if op.growth == 0.0 { if born >= 0.0 { 1.0 } else { 0.0 } } else { (born / op.growth).clamp(0.0, 1.0) };
        maximum = lerp(minimum as f32, maximum, growth);
        amount = maximum / end as f32 * growth;
    }
    let minimum = minimum.min(maximum as i32);
    let child = minimum + (rng.next() * (maximum as i32 - minimum + 1) as f32) as i32;
    let parent = points.parents.get(child as usize).copied().unwrap_or(0).clamp(0, 63);
    let start = points.get(parent);
    let middle = middle(points, parent, child, op.midpoint, op.bulge, op.bulge_control, random);
    let finish = scale(points.get(child), amount);
    let t = rng.next();
    let p = lerp3(lerp3(start, middle, t), lerp3(middle, finish, t), t);
    let mut offset = rng.range3([-op.maximum; 3], [op.maximum; 3]);
    for (axis, value) in offset.iter_mut().enumerate() {
        if op.absolute[axis] != 0.0 {
            *value = value.abs();
        }
        *value *= op.bias[axis];
    }
    add(p, offset)
}
