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
