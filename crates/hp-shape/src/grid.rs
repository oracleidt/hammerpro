use glam::DVec2;

pub(crate) struct Grid2 {
    origin: DVec2,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl Grid2 {
    pub(crate) fn new(min: DVec2, max: DVec2, items: usize) -> Self {
        let size = (max - min).max(DVec2::splat(1e-6));
        let target = (items as f64).sqrt().clamp(1.0, 256.0);
        let cell = (size.x.max(size.y) / target).max(1e-3);
        let nx = ((size.x / cell).ceil() as usize).clamp(1, 512);
        let ny = ((size.y / cell).ceil() as usize).clamp(1, 512);
        let cell = (size.x / nx as f64).max(size.y / ny as f64).max(1e-3);
        Self { origin: min, cell, nx, ny, cells: vec![Vec::new(); nx * ny] }
    }

    fn range(&self, lo: DVec2, hi: DVec2) -> Option<(usize, usize, usize, usize)> {
        let a = (lo - self.origin) / self.cell;
        let b = (hi - self.origin) / self.cell;
        if b.x < 0.0 || b.y < 0.0 || a.x >= self.nx as f64 || a.y >= self.ny as f64 {
            return None;
        }
        let x0 = (a.x.floor().max(0.0) as usize).min(self.nx - 1);
        let y0 = (a.y.floor().max(0.0) as usize).min(self.ny - 1);
        let x1 = (b.x.floor().max(0.0) as usize).min(self.nx - 1);
        let y1 = (b.y.floor().max(0.0) as usize).min(self.ny - 1);
        Some((x0, y0, x1, y1))
    }

    pub(crate) fn insert(&mut self, id: u32, lo: DVec2, hi: DVec2) {
        let Some((x0, y0, x1, y1)) = self.range(lo, hi) else { return };
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.cells[y * self.nx + x].push(id);
            }
        }
    }

    pub(crate) fn at(&self, p: DVec2) -> &[u32] {
        match self.range(p, p) {
            Some((x, y, _, _)) => &self.cells[y * self.nx + x],
            None => &[],
        }
    }

    pub(crate) fn visit(&self, lo: DVec2, hi: DVec2, stamp: &mut Stamp, mut f: impl FnMut(u32)) {
        let Some((x0, y0, x1, y1)) = self.range(lo, hi) else { return };
        stamp.next();
        for y in y0..=y1 {
            for x in x0..=x1 {
                for &id in &self.cells[y * self.nx + x] {
                    if stamp.mark(id) {
                        f(id);
                    }
                }
            }
        }
    }
}

pub(crate) struct Stamp {
    marks: Vec<u32>,
    current: u32,
}

impl Stamp {
    pub(crate) fn new(n: usize) -> Self {
        Self { marks: vec![0; n], current: 0 }
    }

    fn next(&mut self) {
        self.current = self.current.wrapping_add(1);
        if self.current == 0 {
            self.marks.iter_mut().for_each(|m| *m = 0);
            self.current = 1;
        }
    }

    fn mark(&mut self, id: u32) -> bool {
        let Some(m) = self.marks.get_mut(id as usize) else { return false };
        if *m == self.current {
            return false;
        }
        *m = self.current;
        true
    }
}

pub(crate) fn bary(p: DVec2, a: DVec2, b: DVec2, c: DVec2) -> Option<(f64, f64, f64)> {
    let v0 = b - a;
    let v1 = c - a;
    let den = v0.perp_dot(v1);
    if den.abs() < 1e-12 {
        return None;
    }
    let v2 = p - a;
    let v = v2.perp_dot(v1) / den;
    let w = v0.perp_dot(v2) / den;
    let u = 1.0 - v - w;
    const E: f64 = -1e-9;
    (u >= E && v >= E && w >= E).then_some((u, v, w))
}

pub(crate) fn seg_cross(a0: DVec2, a1: DVec2, b0: DVec2, b1: DVec2) -> Option<(f64, f64)> {
    let r = a1 - a0;
    let s = b1 - b0;
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let q = b0 - a0;
    let t = q.perp_dot(s) / den;
    let u = q.perp_dot(r) / den;
    const E: f64 = 1e-9;
    ((-E..=1.0 + E).contains(&t) && (-E..=1.0 + E).contains(&u)).then_some((t.clamp(0.0, 1.0), u.clamp(0.0, 1.0)))
}
