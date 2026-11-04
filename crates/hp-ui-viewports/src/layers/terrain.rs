use glam::DVec3;
use hp_app::AppState;
use hp_app::i18n::TrTable;
use hp_app::runtime::TerrainPreview;
use hp_app::view::ViewInfo;
use hp_render::{Overlay, OverlayLine, OverlayTri};

use super::OverlayLayer;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(TerrainPreviewLayer)]
}

pub(crate) const TR: TrTable = &[];

const OUTLINE: [f32; 4] = [1.0, 0.8, 0.1, 1.0];
const GRID: [f32; 4] = [1.0, 1.0, 1.0, 0.18];
const SUN: DVec3 = DVec3::new(0.35, -0.45, 0.82);

struct TerrainPreviewLayer;

fn ramp(t: f32) -> [f32; 3] {
    let stops: [[f32; 3]; 4] = [[0.18, 0.38, 0.2], [0.45, 0.5, 0.25], [0.5, 0.4, 0.3], [0.95, 0.95, 0.97]];
    let x = t.clamp(0.0, 1.0) * 3.0;
    let i = (x.floor() as usize).min(2);
    let f = x - i as f32;
    let (a, b) = (stops[i], stops[i + 1]);
    [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
}

fn vertex(p: &TerrainPreview, i: usize, j: usize) -> Option<DVec3> {
    let z = f64::from(*p.heights.get(j * p.nx + i)?);
    let x = p.min[0] + p.size[0] * i as f64 / (p.nx - 1) as f64;
    let y = p.min[1] + p.size[1] * j as f64 / (p.ny - 1) as f64;
    Some(DVec3::new(x, y, z))
}

fn f3(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

impl OverlayLayer for TerrainPreviewLayer {
    fn name(&self) -> &'static str {
        "terrain_preview"
    }

    fn retained_version(&self, state: &AppState, _view: &ViewInfo) -> Option<u64> {
        state.runtime.mesh.terrain_preview.as_ref().map(|p| p.version)
    }

    fn retained(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let Some(p) = state.runtime.mesh.terrain_preview.as_ref() else { return };
        if p.nx < 2 || p.ny < 2 || p.heights.len() != p.nx * p.ny {
            return;
        }
        let lo = p.heights.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = p.heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let span = (hi - lo).max(1e-3);
        let corners = [(0, 0), (p.nx - 1, 0), (p.nx - 1, p.ny - 1), (0, p.ny - 1)];
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            if let (Some(va), Some(vb)) = (vertex(p, a.0, a.1), vertex(p, b.0, b.1)) {
                out.lines_on_top.push(OverlayLine { a: f3(va), b: f3(vb), color: OUTLINE });
            }
        }
        if view.is_2d() {
            return;
        }
        let sun = SUN.normalize();
        for j in 0..p.ny - 1 {
            for i in 0..p.nx - 1 {
                let (Some(v00), Some(v10), Some(v11), Some(v01)) =
                    (vertex(p, i, j), vertex(p, i + 1, j), vertex(p, i + 1, j + 1), vertex(p, i, j + 1))
                else {
                    continue;
                };
                for tri in [[v00, v10, v11], [v00, v11, v01]] {
                    let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]).try_normalize().unwrap_or(DVec3::Z);
                    let shade = (0.45 + 0.55 * n.dot(sun).max(0.0)) as f32;
                    let mid = (tri[0].z + tri[1].z + tri[2].z) as f32 / 3.0;
                    let c = ramp((mid - lo) / span);
                    out.triangles.push(OverlayTri {
                        p: [f3(tri[0]), f3(tri[1]), f3(tri[2])],
                        color: [c[0] * shade, c[1] * shade, c[2] * shade, 0.92],
                    });
                }
                if i % 4 == 0 {
                    out.lines.push(OverlayLine { a: f3(v00), b: f3(v01), color: GRID });
                }
                if j % 4 == 0 {
                    out.lines.push(OverlayLine { a: f3(v00), b: f3(v10), color: GRID });
                }
            }
        }
    }
}
