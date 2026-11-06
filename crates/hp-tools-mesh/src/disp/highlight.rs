use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use glam::DVec3;
use hp_app::settings::Disp2dSetting;
use hp_app::state::AppState;
use hp_app::{ViewInfo, ViewKind};
use hp_editor::FaceRef;
use hp_geom::DispMesh;
use hp_render::{Overlay, OverlayLine, OverlayTri};

use super::tags::is_removed;
use crate::common::{DocKey, f3};

const LIFT: f64 = 0.2;
const LINE_ALPHA: f32 = 0.9;
pub(crate) const REMOVED: [f32; 4] = [0.55, 0.2, 0.85, 0.6];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct HighlightOpts {
    pub grid: bool,
    pub tint: bool,
    pub removed: bool,
}

fn selection(state: &AppState, alpha: f32) -> [f32; 4] {
    let [r, g, b] = state.settings.view.selection_color;
    [f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, alpha]
}

fn grid_in_2d(state: &AppState, view: &ViewInfo) -> bool {
    state.settings.render.displacements_2d_of(view.index) != Disp2dSetting::Outline
}

fn selected(state: &AppState) -> Vec<FaceRef> {
    super::paint::selected_disp_faces(&state.doc)
}

pub(crate) fn version(state: &AppState, view: &ViewInfo, opts: HighlightOpts) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    0xD159_u64.hash(&mut h);
    DocKey::of(&state.doc).hash(&mut h);
    for f in &state.doc.selection().faces {
        f.hash(&mut h);
    }
    opts.hash(&mut h);
    state.settings.mesh.displacement_selection_opacity().to_bits().hash(&mut h);
    state.settings.view.selection_color.hash(&mut h);
    view.kind.hash(&mut h);
    if view.is_2d() {
        grid_in_2d(state, view).hash(&mut h);
    }
    h.finish()
}

fn triangle_edges(mesh: &DispMesh) -> Vec<(usize, usize)> {
    let mut seen: HashSet<(u32, u32)> = HashSet::with_capacity(mesh.indices.len());
    let mut out = Vec::with_capacity(mesh.indices.len());
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let key = if a < b { (a, b) } else { (b, a) };
            if seen.insert(key) {
                out.push((key.0 as usize, key.1 as usize));
            }
        }
    }
    out
}

fn lifted(mesh: &DispMesh, k: usize) -> Option<DVec3> {
    let n = mesh.normals.get(k).copied().unwrap_or(mesh.face_normal);
    mesh.positions.get(k).map(|p| *p + n * LIFT)
}

pub(crate) fn build(state: &AppState, view: &ViewInfo, opts: HighlightOpts, out: &mut Overlay) {
    let doc = &state.doc;
    let fill = selection(state, state.settings.mesh.displacement_selection_opacity());
    let line = selection(state, LINE_ALPHA);
    let in_3d = view.kind == ViewKind::Perspective;
    let grid = opts.grid && (in_3d || grid_in_2d(state, view));
    for f in selected(state) {
        let Some(mesh) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) else { continue };
        if in_3d && opts.tint && fill[3] > 0.0 {
            out.triangles.extend(mesh.indices.chunks_exact(3).filter_map(|t| {
                let p = [lifted(mesh, t[0] as usize)?, lifted(mesh, t[1] as usize)?, lifted(mesh, t[2] as usize)?];
                Some(OverlayTri { p: p.map(f3), color: fill })
            }));
        }
        if grid {
            for (a, b) in triangle_edges(mesh) {
                if in_3d {
                    if let (Some(pa), Some(pb)) = (lifted(mesh, a), lifted(mesh, b)) {
                        out.lines.push(OverlayLine { a: f3(pa), b: f3(pb), color: line });
                    }
                } else if let (Some(pa), Some(pb)) = (mesh.positions.get(a), mesh.positions.get(b)) {
                    out.lines_on_top.push(OverlayLine { a: f3(*pa), b: f3(*pb), color: line });
                }
            }
        }
    }
    if in_3d && opts.removed {
        for f in doc.disp_faces() {
            if !doc.is_solid_visible(f.solid) {
                continue;
            }
            let Some(node) = doc.scene().solid(f.solid) else { continue };
            let (Some(mesh), Some(disp)) =
                (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
            else {
                continue;
            };
            if !disp.triangle_tags.iter().any(|t| is_removed(*t)) {
                continue;
            }
            for (i, t) in mesh.indices.chunks_exact(3).enumerate() {
                if !disp.triangle_tags.get(i).is_some_and(|t| is_removed(*t)) {
                    continue;
                }
                let corners = [lifted(mesh, t[0] as usize), lifted(mesh, t[1] as usize), lifted(mesh, t[2] as usize)];
                if let [Some(a), Some(b), Some(c)] = corners {
                    out.triangles.push(OverlayTri { p: [f3(a), f3(b), f3(c)], color: REMOVED });
                }
            }
        }
    }
}
