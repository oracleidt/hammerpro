use glam::{DVec3, Vec2, Vec3};

pub(crate) const MAX_SKY_LAYERS: u32 = 3;
pub(crate) const MAX_SUN_LAYERS: u32 = 8;
pub(crate) const LAYER_GAP: f32 = 24.0;
const GRID: usize = 256;
const MIN_SPACING: f32 = 16.0;
const MAX_TESTS: u64 = 4_000_000;
pub(crate) const REGION_MARGIN: f32 = 64.0;

pub(crate) fn march_steps_with(texel: f32, growth: f32) -> [[f32; 4]; 18] {
    let texel = texel.max(1.0);
    let mut radius = 10.0f32;
    std::array::from_fn(|_| {
        let level = (0.2 * radius / texel).max(1.0).log2().round_ties_even().clamp(0.0, 7.0);
        let step = [radius, level, 1.0, 0.0];
        radius += texel * level.exp2() * growth;
        step
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Basis {
    pub r: Vec3,
    pub u: Vec3,
    pub f: Vec3,
}

impl Basis {
    pub fn of(dir: Vec3) -> Basis {
        let f = dir.normalize_or(Vec3::NEG_Z).as_dvec3();
        let hint = if f.z.abs() < 0.99 { DVec3::Z } else { DVec3::Y };
        let r = f.cross(hint).normalize();
        let u = r.cross(f);
        Basis { r: r.as_vec3(), u: u.as_vec3(), f: f.as_vec3() }
    }

    pub fn project(&self, p: Vec3) -> Vec3 {
        Vec3::new(p.dot(self.r), p.dot(self.u), p.dot(self.f))
    }

    pub fn box_rect(&self, lo: Vec3, hi: Vec3) -> (Rect, f32, f32) {
        let mut out = Rect::EMPTY;
        let mut near = f32::INFINITY;
        let mut far = f32::NEG_INFINITY;
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 != 0 { hi.x } else { lo.x },
                if i & 2 != 0 { hi.y } else { lo.y },
                if i & 4 != 0 { hi.z } else { lo.z },
            );
            let p = self.project(c);
            out.add(Vec2::new(p.x, p.y));
            near = near.min(p.z);
            far = far.max(p.z);
        }
        (out, near, far)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub lo: Vec2,
    pub hi: Vec2,
}

impl Rect {
    pub const EMPTY: Rect = Rect { lo: Vec2::splat(f32::INFINITY), hi: Vec2::splat(f32::NEG_INFINITY) };

    fn add(&mut self, p: Vec2) {
        self.lo = self.lo.min(p);
        self.hi = self.hi.max(p);
    }

    fn union(&mut self, o: Rect) {
        self.lo = self.lo.min(o.lo);
        self.hi = self.hi.max(o.hi);
    }

    pub fn is_empty(&self) -> bool {
        !(self.lo.cmple(self.hi).all())
    }

    pub fn overlaps(&self, o: &Rect) -> bool {
        !self.is_empty() && !o.is_empty() && self.lo.cmple(o.hi).all() && o.lo.cmple(self.hi).all()
    }

    fn grown(&self, by: f32) -> Rect {
        Rect { lo: self.lo - Vec2::splat(by), hi: self.hi + Vec2::splat(by) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayerRegion {
    pub rect: Rect,
    pub depth: f32,
    pub below: f32,
}

impl LayerRegion {
    pub fn keeps(&self, basis: &Basis, lo: Vec3, hi: Vec3) -> bool {
        let (rect, near, far) = basis.box_rect(lo, hi);
        far >= self.depth && near <= self.below && rect.overlaps(&self.rect)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SkyLayers {
    pub count: u32,
    pub max: u32,
    pub regions: Vec<LayerRegion>,
}

impl SkyLayers {
    pub fn layers(&self) -> u32 {
        self.count.clamp(1, self.max.max(1))
    }

    pub fn region(&self, layer: u32) -> Option<&LayerRegion> {
        layer.checked_sub(1).and_then(|i| self.regions.get(i as usize))
    }
}

pub(crate) fn facing(normal: Vec3, travel: Vec3) -> bool {
    normal.dot(travel) > 0.0
}

fn edge(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

pub(crate) fn analyze(basis: &Basis, tris: &[[Vec3; 3]], from_sun: bool) -> SkyLayers {
    let proj: Vec<[Vec3; 3]> = tris
        .iter()
        .map(|t| t.map(|p| basis.project(p)))
        .filter(|t| t.iter().all(|p| p.is_finite()))
        .filter(|t| edge(t[0].truncate(), t[1].truncate(), t[2].truncate()).abs() > 1e-3)
        .collect();
    let mut bounds = Rect::EMPTY;
    for t in &proj {
        for p in t {
            bounds.add(p.truncate());
        }
    }
    let max = if from_sun { MAX_SUN_LAYERS } else { MAX_SKY_LAYERS };
    if bounds.is_empty() {
        return SkyLayers { max, ..SkyLayers::default() };
    }
    let extent = (bounds.hi - bounds.lo).max_element().max(1.0);
    let mut spacing = (extent / GRID as f32).max(MIN_SPACING);
    let tests = |s: f32| -> u64 {
        proj.iter()
            .map(|t| {
                let lo = t.iter().fold(Vec2::splat(f32::INFINITY), |m, p| m.min(p.truncate()));
                let hi = t.iter().fold(Vec2::splat(f32::NEG_INFINITY), |m, p| m.max(p.truncate()));
                let n = ((hi - lo) / s).ceil() + Vec2::ONE;
                (n.x as u64).saturating_mul(n.y as u64)
            })
            .sum()
    };
    while tests(spacing) > MAX_TESTS {
        spacing *= 1.5;
    }
    let nx = (((bounds.hi.x - bounds.lo.x) / spacing).floor() as usize + 1).max(1);
    let ny = (((bounds.hi.y - bounds.lo.y) / spacing).floor() as usize + 1).max(1);
    let center = |ix: usize, iy: usize| bounds.lo + Vec2::new(ix as f32 + 0.5, iy as f32 + 0.5) * spacing;
    let mut cells: Vec<Vec<f32>> = vec![Vec::new(); nx * ny];
    for t in &proj {
        let (a, b, c) = (t[0].truncate(), t[1].truncate(), t[2].truncate());
        let area = edge(a, b, c);
        let lo = a.min(b).min(c);
        let hi = a.max(b).max(c);
        let cell_of = |v: f32, o: f32, n: usize| (((v - o) / spacing - 0.5).floor().max(0.0) as usize).min(n - 1);
        let cell_hi = |v: f32, o: f32, n: usize| (((v - o) / spacing - 0.5).ceil().max(0.0) as usize).min(n - 1);
        let (x0, x1) = (cell_of(lo.x, bounds.lo.x, nx), cell_hi(hi.x, bounds.lo.x, nx));
        let (y0, y1) = (cell_of(lo.y, bounds.lo.y, ny), cell_hi(hi.y, bounds.lo.y, ny));
        for iy in y0..=y1 {
            for ix in x0..=x1 {
                let p = center(ix, iy);
                let (w0, w1, w2) = (edge(b, c, p) / area, edge(c, a, p) / area, edge(a, b, p) / area);
                if w0 < -1e-5 || w1 < -1e-5 || w2 < -1e-5 {
                    continue;
                }
                let d = w0 * t[0].z + w1 * t[1].z + w2 * t[2].z;
                if d.is_finite()
                    && let Some(cell) = cells.get_mut(iy * nx + ix)
                {
                    cell.push(d);
                }
            }
        }
    }
    let mut count = 0u32;
    let mut regions: Vec<LayerRegion> = Vec::new();
    let mut starts: Vec<f32> = Vec::new();
    for iy in 0..ny {
        for ix in 0..nx {
            let Some(cell) = cells.get_mut(iy * nx + ix) else { continue };
            if cell.is_empty() {
                continue;
            }
            cell.sort_by(f32::total_cmp);
            starts.clear();
            starts.push(cell[0]);
            for w in cell.windows(2) {
                if w[1] - w[0] > LAYER_GAP {
                    starts.push(w[1]);
                }
            }
            let clusters = starts.len() as u32;
            count = count.max(clusters);
            let p = center(ix, iy);
            let here = Rect { lo: p, hi: p }.grown(spacing);
            for k in 1..clusters.min(max) {
                let i = (k - 1) as usize;
                if regions.len() <= i {
                    let empty = LayerRegion { rect: Rect::EMPTY, depth: f32::INFINITY, below: f32::NEG_INFINITY };
                    regions.resize(i + 1, empty);
                }
                regions[i].rect.union(here);
                let (start, below) = if from_sun {
                    (starts[k as usize], starts.get(k as usize + 1).map_or(f32::INFINITY, |next| next + LAYER_GAP))
                } else {
                    (starts[(clusters - 1 - k) as usize], starts[(clusters - k) as usize] + LAYER_GAP)
                };
                regions[i].depth = regions[i].depth.min(start - LAYER_GAP);
                regions[i].below = regions[i].below.max(below);
            }
        }
    }
    let regions = regions.into_iter().map(|r| LayerRegion { rect: r.rect.grown(REGION_MARGIN), ..r }).collect();
    SkyLayers { count, max, regions }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(lo: [f32; 2], hi: [f32; 2], z: f32) -> [[Vec3; 3]; 2] {
        let p = |x: f32, y: f32| Vec3::new(x, y, z);
        [[p(lo[0], lo[1]), p(hi[0], lo[1]), p(hi[0], hi[1])], [p(lo[0], lo[1]), p(hi[0], hi[1]), p(lo[0], hi[1])]]
    }

    #[test]
    fn one_sky_is_one_layer() {
        let b = Basis::of(Vec3::NEG_Z);
        let tris = quad([-4096.0, -4096.0], [4096.0, 4096.0], 2048.0);
        let l = analyze(&b, &tris, false);
        assert_eq!((l.count, l.layers()), (1, 1));
        assert!(l.regions.is_empty());
        assert!(l.region(1).is_none());
        let none = analyze(&b, &[], true);
        assert_eq!((none.count, none.layers()), (0, 1));
    }

    #[test]
    fn a_sealed_room_above_a_sky_adds_a_layer_over_its_own_footprint() {
        let b = Basis::of(Vec3::new(0.3, 0.2, -1.0));
        let mut tris: Vec<[Vec3; 3]> = quad([-4096.0, -4096.0], [4096.0, 4096.0], 2048.0).to_vec();
        tris.extend(quad([-256.0, -256.0], [256.0, 256.0], 3000.0));
        tris.extend(quad([-4096.0, -4096.0], [4096.0, 4096.0], 2040.0));
        let under = Vec3::new(875.0, 575.0, 0.0);
        let room = Vec3::new(130.0, 80.0, 2500.0);
        let above = Vec3::new(-175.0, -125.0, 3450.0);
        let aside = Vec3::new(3500.0, 3500.0, 0.0);
        for from_sun in [false, true] {
            let l = analyze(&b, &tris, from_sun);
            assert_eq!(l.count, 2, "faces closer than the gap are one layer");
            let region = l.region(1).expect("region");
            let r = region.rect;
            let inside = b.project(Vec3::new(0.0, 0.0, 2048.0)).truncate();
            assert!(r.lo.cmple(inside).all() && inside.cmple(r.hi).all(), "{r:?}");
            let far = b.project(Vec3::new(3500.0, 3500.0, 2048.0)).truncate();
            assert!(!(r.lo.cmple(far).all() && far.cmple(r.hi).all()), "{r:?} {far:?}");
            let wide = (r.hi - r.lo).max_element();
            assert!(wide < 2.0 * REGION_MARGIN + 1024.0 + 256.0, "{wide}");
            assert_eq!(region.keeps(&b, room, room + Vec3::splat(50.0)), !from_sun, "the room under its own sky");
            assert!(!region.keeps(&b, above, above + Vec3::splat(50.0)), "what lies above both skies is skipped");
            assert_eq!(region.keeps(&b, under - Vec3::Z * 3000.0, under), from_sun, "the map under the lower sky");
            assert!(!region.keeps(&b, aside, aside + Vec3::splat(50.0)), "what lies far from the room is skipped");
        }
    }

    #[test]
    fn hostile_input_is_ignored() {
        let b = Basis::of(Vec3::NAN);
        let bad = [[Vec3::NAN, Vec3::ZERO, Vec3::X], [Vec3::ZERO, Vec3::X, Vec3::X * 2.0]];
        assert_eq!(analyze(&b, &bad, true).count, 0);
        let huge: Vec<[Vec3; 3]> =
            (0..200).flat_map(|i| quad([-30000.0, -30000.0], [30000.0, 30000.0], i as f32 * 100.0)).collect();
        let l = analyze(&b, &huge, true);
        assert_eq!(l.layers(), MAX_SUN_LAYERS);
        assert_eq!(analyze(&b, &huge, false).layers(), MAX_SKY_LAYERS);
        assert!(facing(Vec3::NEG_Z, Vec3::NEG_Z) && !facing(Vec3::Z, Vec3::NEG_Z));
        let r = Rect { lo: Vec2::ZERO, hi: Vec2::ONE };
        assert!(r.overlaps(&Rect { lo: Vec2::splat(0.5), hi: Vec2::splat(4.0) }));
        assert!(!r.overlaps(&Rect::EMPTY) && !r.overlaps(&Rect { lo: Vec2::splat(2.0), hi: Vec2::splat(4.0) }));
        assert_eq!(b.box_rect(Vec3::ZERO, Vec3::ZERO), (Rect { lo: Vec2::ZERO, hi: Vec2::ZERO }, 0.0, 0.0));
    }
}
