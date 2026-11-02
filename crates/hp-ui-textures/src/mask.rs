use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::DVec3;
use hp_app::{AppState, ViewKind};
use hp_editor::FaceRef;
use hp_geom::{DispMesh, SolidGeom};
use hp_render::{Overlay, OverlayLine, OverlayTri};

const QUANT: f64 = 256.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MaskStyle {
    pub fill: [f32; 4],
    pub line: [f32; 4],
}

fn f32x3(p: DVec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}

type QPoint = [i64; 3];

fn quantize(p: DVec3) -> QPoint {
    [(p.x * QUANT).round() as i64, (p.y * QUANT).round() as i64, (p.z * QUANT).round() as i64]
}

fn edge_key(a: QPoint, b: QPoint) -> (QPoint, QPoint) {
    if a <= b { (a, b) } else { (b, a) }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MaskGeom {
    tris: Vec<OverlayTri>,
    lines: Vec<OverlayLine>,
    on_top: bool,
}

impl MaskGeom {
    pub(crate) fn emit(&self, out: &mut Overlay) {
        out.triangles.extend_from_slice(&self.tris);
        let lines = if self.on_top { &mut out.lines_on_top } else { &mut out.lines };
        lines.extend_from_slice(&self.lines);
    }
}

enum FaceShape<'a> {
    Polygon { vertices: &'a [DVec3], normal: DVec3 },
    Disp { mesh: &'a DispMesh, normal: DVec3 },
}

fn disp_border(row: usize) -> Vec<usize> {
    if row < 2 {
        return Vec::new();
    }
    (0..row)
        .chain((1..row).map(|i| i * row + row - 1))
        .chain((0..row - 1).rev().map(|i| (row - 1) * row + i))
        .chain((1..row - 1).rev().map(|i| i * row))
        .collect()
}

fn shape_of(state: &AppState, face: FaceRef) -> Option<FaceShape<'_>> {
    let node = state.doc.scene().solid(face.solid)?;
    let geom = node.geom.face_for_side(face.side)?;
    let normal = geom.normal();
    match node.disp_mesh(face.side) {
        Some(mesh) => Some(FaceShape::Disp { mesh, normal }),
        None if geom.vertices.len() >= 3 => Some(FaceShape::Polygon { vertices: &geom.vertices, normal }),
        None => None,
    }
}

fn for_each_shape(state: &AppState, faces: &[FaceRef], mut f: impl FnMut(FaceShape<'_>)) {
    for face in faces {
        if let Some(shape) = shape_of(state, *face) {
            f(shape);
        }
    }
}

fn for_each_solid_run(state: &AppState, faces: &[FaceRef], mut f: impl FnMut(&[FaceShape<'_>])) {
    let mut shapes: Vec<FaceShape<'_>> = Vec::new();
    for run in faces.chunk_by(|a, b| a.solid == b.solid) {
        shapes.clear();
        shapes.extend(run.iter().filter_map(|face| shape_of(state, *face)));
        if !shapes.is_empty() {
            f(&shapes);
        }
    }
}

fn outline(shape: &FaceShape<'_>, mut f: impl FnMut(DVec3, DVec3, DVec3)) {
    match shape {
        FaceShape::Polygon { vertices, normal } => {
            for (i, a) in vertices.iter().enumerate() {
                let b = vertices[(i + 1) % vertices.len()];
                f(*a, b, *normal);
            }
        }
        FaceShape::Disp { mesh, normal } => {
            if mesh.positions.len() < mesh.row_len * mesh.row_len {
                return;
            }
            let border = disp_border(mesh.row_len);
            for (w, &a) in border.iter().enumerate() {
                let b = border[(w + 1) % border.len()];
                if let (Some(pa), Some(pb)) = (mesh.positions.get(a), mesh.positions.get(b)) {
                    f(*pa, *pb, *normal);
                }
            }
        }
    }
}

fn build_3d(state: &AppState, faces: &[FaceRef], style: MaskStyle) -> MaskGeom {
    let mut out = MaskGeom::default();
    let mut edges: Vec<((QPoint, QPoint), DVec3, DVec3, DVec3)> = Vec::new();
    let fill = Some(style.fill).filter(|c| c[3] > 0.0);
    for_each_solid_run(state, faces, |shapes| {
        edges.clear();
        for shape in shapes {
            if let Some(fill) = fill {
                match shape {
                    FaceShape::Polygon { vertices, .. } => {
                        let a = f32x3(vertices[0]);
                        out.tris.extend(
                            vertices
                                .windows(2)
                                .skip(1)
                                .map(|w| OverlayTri { p: [a, f32x3(w[0]), f32x3(w[1])], color: fill }),
                        );
                    }
                    FaceShape::Disp { mesh, .. } => {
                        let lifted = |i: u32| mesh.positions.get(i as usize).map(|q| f32x3(*q));
                        out.tris.extend(mesh.indices.chunks_exact(3).filter_map(|tri| {
                            Some(OverlayTri { p: [lifted(tri[0])?, lifted(tri[1])?, lifted(tri[2])?], color: fill })
                        }));
                    }
                }
            }
            if let FaceShape::Disp { mesh, .. } = shape {
                let grid = [style.line[0], style.line[1], style.line[2], style.line[3] * 0.5];
                let lifted = |i: usize| mesh.positions.get(i).map(|q| f32x3(*q));
                let mut seen: ahash::AHashSet<(u32, u32)> = ahash::AHashSet::new();
                for tri in mesh.indices.chunks_exact(3) {
                    for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                        if seen.insert((a.min(b), a.max(b)))
                            && let (Some(pa), Some(pb)) = (lifted(a as usize), lifted(b as usize))
                        {
                            out.lines.push(OverlayLine { a: pa, b: pb, color: grid });
                        }
                    }
                }
            }
            outline(shape, |a, b, n| {
                let key = edge_key(quantize(a), quantize(b));
                match edges.iter_mut().find(|e| e.0 == key) {
                    Some(e) => e.3 += n,
                    None => edges.push((key, a, b, n)),
                }
            });
        }
        out.lines.extend(edges.iter().map(|(_, a, b, _)| OverlayLine {
            a: f32x3(*a),
            b: f32x3(*b),
            color: style.line,
        }));
    });
    out
}

fn build_2d(state: &AppState, faces: &[FaceRef], kind: ViewKind, style: MaskStyle) -> MaskGeom {
    let mut out = MaskGeom { on_top: true, ..MaskGeom::default() };
    let Some((r, u, _)) = kind.axes_2d() else { return out };
    let mut seen: ahash::AHashSet<([i64; 2], [i64; 2])> = ahash::AHashSet::new();
    let mut local: Vec<([i64; 2], [i64; 2])> = Vec::new();
    let q2 = |p: DVec3| [(p[r] * QUANT).round() as i64, (p[u] * QUANT).round() as i64];
    for_each_solid_run(state, faces, |shapes| {
        local.clear();
        for shape in shapes {
            outline(shape, |a, b, _| {
                let (qa, qb) = (q2(a), q2(b));
                if qa == qb {
                    return;
                }
                let key = if qa <= qb { (qa, qb) } else { (qb, qa) };
                if !local.contains(&key) {
                    local.push(key);
                    if seen.insert(key) {
                        out.lines.push(OverlayLine { a: f32x3(a), b: f32x3(b), color: style.line });
                    }
                }
            });
        }
    });
    out
}

#[derive(Default)]
struct Source {
    faces: Vec<FaceRef>,
    parts: Vec<Option<FacePart>>,
}

type FacePart = (Arc<SolidGeom>, Option<Arc<DispMesh>>);

impl Source {
    fn part(state: &AppState, f: FaceRef) -> Option<(&Arc<SolidGeom>, Option<&Arc<DispMesh>>)> {
        let node = state.doc.scene().solid(f.solid)?;
        Some((&node.geom, node.disp_mesh(f.side)))
    }

    fn matches(&self, state: &AppState) -> bool {
        let sel = &state.doc.selection().faces;
        if sel.len() != self.faces.len() {
            return false;
        }
        sel.iter().zip(&self.faces).zip(&self.parts).all(|((f, g), part)| {
            f == g
                && match (Self::part(state, *f), part) {
                    (None, None) => true,
                    (Some((geom, disp)), Some((g2, d2))) => {
                        Arc::ptr_eq(geom, g2)
                            && match (disp, d2) {
                                (None, None) => true,
                                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                                _ => false,
                            }
                    }
                    _ => false,
                }
        })
    }

    fn capture(state: &AppState) -> Self {
        let faces: Vec<FaceRef> = state.doc.selection().faces.iter().copied().collect();
        let parts = faces.iter().map(|f| Self::part(state, *f).map(|(g, d)| (Arc::clone(g), d.cloned()))).collect();
        Self { faces, parts }
    }
}

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub(crate) struct MaskCache {
    inner: RefCell<CacheState>,
}

#[derive(Default)]
struct CacheState {
    revision: Option<u64>,
    source: Source,
    style: Option<MaskStyle>,
    generation: u64,
    views: [Option<Arc<MaskGeom>>; 4],
    builds: u64,
}

impl CacheState {
    fn refresh(&mut self, state: &AppState, style: MaskStyle) {
        let revision = state.doc.revision();
        if self.generation != 0 && self.revision == Some(revision) && self.style == Some(style) {
            return;
        }
        if self.generation == 0 || self.style != Some(style) || !self.source.matches(state) {
            self.source = Source::capture(state);
            self.views = Default::default();
            self.generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        self.revision = Some(revision);
        self.style = Some(style);
    }
}

pub(crate) fn slot(kind: ViewKind) -> usize {
    match kind {
        ViewKind::Perspective => 0,
        ViewKind::Top => 1,
        ViewKind::Front => 2,
        ViewKind::Side => 3,
    }
}

impl MaskCache {
    pub(crate) fn generation(&self, state: &AppState, style: MaskStyle) -> u64 {
        let mut c = self.inner.borrow_mut();
        c.refresh(state, style);
        c.generation
    }

    pub(crate) fn get(&self, state: &AppState, kind: ViewKind, style: MaskStyle) -> Arc<MaskGeom> {
        let mut c = self.inner.borrow_mut();
        c.refresh(state, style);
        let i = slot(kind);
        if let Some(g) = &c.views[i] {
            return Arc::clone(g);
        }
        let faces = &c.source.faces;
        let geom =
            Arc::new(if kind.is_2d() { build_2d(state, faces, kind, style) } else { build_3d(state, faces, style) });
        c.views[i] = Some(Arc::clone(&geom));
        c.builds += 1;
        geom
    }

    #[cfg(test)]
    pub(crate) fn builds(&self) -> u64 {
        self.inner.borrow().builds
    }

    pub(crate) fn clear(&self) {
        *self.inner.borrow_mut() = CacheState::default();
    }
}

pub(crate) fn face_outline(state: &AppState, face: FaceRef, color: [f32; 4], on_top: bool, out: &mut Overlay) {
    for_each_shape(state, &[face], |shape| {
        outline(&shape, |a, b, _| {
            let l = OverlayLine { a: f32x3(a), b: f32x3(b), color };
            if on_top { out.lines_on_top.push(l) } else { out.lines.push(l) }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_displacements_show_their_grid() {
        let mut state = hp_app::testing::test_state();
        let top = FaceRef::new(2, 0);
        assert_eq!(state.doc.create_displacements(&[top], 2).ok(), Some(1));
        state.doc.select_faces(&[top], hp_editor::SelectOp::Replace);
        let style = MaskStyle { fill: [1.0, 0.0, 0.0, 0.3], line: [1.0, 0.0, 0.0, 1.0] };
        let g = build_3d(&state, &[top], style);
        assert_eq!(g.tris.len(), 32);
        assert_eq!(g.lines.len(), 16 + 56);
        assert_eq!(g.lines.iter().filter(|l| l.color[3] < 1.0).count(), 56);
    }

    #[test]
    fn border_of_a_grid() {
        assert_eq!(disp_border(3), [0, 1, 2, 5, 8, 7, 6, 3]);
        assert!(disp_border(1).is_empty());
    }
}
