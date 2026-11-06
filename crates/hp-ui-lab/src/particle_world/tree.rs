use super::*;

#[derive(Clone, Debug)]
enum Node {
    Leaf { bounds: Aabb, start: usize, end: usize },
    Branch { bounds: Aabb, left: usize, right: usize },
}

impl Node {
    fn bounds(&self) -> Aabb {
        match self {
            Self::Leaf { bounds, .. } | Self::Branch { bounds, .. } => *bounds,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct SurfaceTree {
    surfaces: Vec<Surface>,
    nodes: Vec<Node>,
}

impl SurfaceTree {
    /// Named particle environments preserve the triangle normal on either side.
    pub(super) fn ray(&self, start: Vector, direction: Vector, maximum: f32) -> Option<hp_pcf::environment::RayHit> {
        if !start.iter().chain(&direction).all(|v| v.is_finite()) || maximum.is_nan() || maximum < 0.0 {
            return None;
        }
        let (start, direction) = (vec(start), vec(direction));
        if direction.length_squared() == 0.0 || self.nodes.is_empty() {
            return None;
        }
        let mut best = None;
        self.visit_ray(0, start, direction, f64::from(maximum), &mut best);
        best.map(|(distance, normal)| hp_pcf::environment::RayHit {
            distance: distance as f32,
            normal: normal.as_vec3().to_array(),
        })
    }

    fn visit_ray(&self, index: usize, origin: DVec3, direction: DVec3, maximum: f64, best: &mut Option<(f64, DVec3)>) {
        let node = &self.nodes[index];
        let limit = best.map_or(maximum, |(distance, _)| distance.min(maximum));
        if !ray_bounds(origin, direction, node.bounds(), limit) {
            return;
        }
        match *node {
            Node::Branch { left, right, .. } => {
                self.visit_ray(left, origin, direction, maximum, best);
                self.visit_ray(right, origin, direction, maximum, best);
            }
            Node::Leaf { start, end, .. } => {
                for surface in &self.surfaces[start..end] {
                    let [a, b, c] = surface.points;
                    let (ab, ac) = (b - a, c - a);
                    let cross = direction.cross(ac);
                    let determinant = ab.dot(cross);
                    if determinant.abs() < 1e-12 {
                        continue;
                    }
                    let inverse = determinant.recip();
                    let offset = origin - a;
                    let u = offset.dot(cross) * inverse;
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let q = offset.cross(ab);
                    let v = direction.dot(q) * inverse;
                    if v < 0.0 || u + v > 1.0 {
                        continue;
                    }
                    let distance = ac.dot(q) * inverse;
                    if distance >= 0.0 && distance <= maximum && best.is_none_or(|(old, _)| distance < old) {
                        *best = Some((distance, ab.cross(ac).normalize()));
                    }
                }
            }
        }
    }

    pub(super) fn new(surfaces: Vec<Surface>) -> Self {
        let mut tree = Self { surfaces, nodes: Vec::new() };
        if !tree.surfaces.is_empty() {
            tree.build(0, tree.surfaces.len());
        }
        tree
    }

    fn build(&mut self, start: usize, end: usize) -> usize {
        let bounds = self.surfaces[start..end].iter().fold(Aabb::empty(), |b, s| b.union(&s.bounds));
        let node = self.nodes.len();
        self.nodes.push(Node::Leaf { bounds, start, end });
        if end - start > 8 {
            let size = bounds.max - bounds.min;
            let axis = if size.x >= size.y && size.x >= size.z {
                0
            } else if size.y >= size.z {
                1
            } else {
                2
            };
            let mid = (start + end) / 2;
            self.surfaces[start..end].select_nth_unstable_by(mid - start, |a, b| {
                a.bounds.center()[axis].total_cmp(&b.bounds.center()[axis])
            });
            let left = self.build(start, mid);
            let right = self.build(mid, end);
            self.nodes[node] = Node::Branch { bounds, left, right };
        }
        node
    }

    pub(super) fn trace(&self, start: DVec3, end: DVec3, half: DVec3, bounds: &Aabb) -> Option<SweepHit> {
        let mut best = None;
        if !self.nodes.is_empty() {
            self.visit(0, bounds, &mut |surface| {
                if let Some(hit) = hp_geom::sweep_box_triangle(start, end, half, surface.points)
                    && best.is_none_or(|old: SweepHit| hit.fraction < old.fraction)
                {
                    best = Some(hit);
                }
            });
        }
        best
    }

    fn visit(&self, index: usize, bounds: &Aabb, visitor: &mut impl FnMut(&Surface)) {
        let node = &self.nodes[index];
        if !bounds.intersects(&node.bounds()) {
            return;
        }
        match *node {
            Node::Leaf { start, end, .. } => {
                for surface in &self.surfaces[start..end] {
                    if bounds.intersects(&surface.bounds) {
                        visitor(surface);
                    }
                }
            }
            Node::Branch { left, right, .. } => {
                self.visit(left, bounds, visitor);
                self.visit(right, bounds, visitor);
            }
        }
    }
}

fn ray_bounds(origin: DVec3, direction: DVec3, bounds: Aabb, maximum: f64) -> bool {
    let (mut near, mut far) = (0.0f64, maximum);
    for axis in 0..3 {
        if direction[axis] == 0.0 {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return false;
            }
        } else {
            let a = (bounds.min[axis] - origin[axis]) / direction[axis];
            let b = (bounds.max[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_traces_prune_remote_surfaces_and_match_brute_force() {
        let surfaces = (0..10000)
            .map(|i| {
                let x = f64::from(i) * 100.0;
                let points = [DVec3::new(x, 0.0, 0.0), DVec3::new(x + 20.0, 0.0, 0.0), DVec3::new(x, 20.0, 0.0)];
                Surface { bounds: Aabb::from_points(points), points }
            })
            .collect();
        let tree = SurfaceTree::new(surfaces);
        let start = DVec3::new(500005.0, 5.0, 100.0);
        let end = start - DVec3::Z * 200.0;
        let bounds = Aabb::new(start, end).expand(2.0);
        let mut tested = 0;
        tree.visit(0, &bounds, &mut |_| tested += 1);
        assert_eq!(tested, 1);
        let actual = tree.trace(start, end, DVec3::splat(2.0), &bounds);
        let expected = tree
            .surfaces
            .iter()
            .filter_map(|surface| hp_geom::sweep_box_triangle(start, end, DVec3::splat(2.0), surface.points))
            .min_by(|a, b| a.fraction.total_cmp(&b.fraction));
        assert_eq!(actual, expected);
        assert_eq!(actual.unwrap().fraction, 0.49);
    }
}
