use egui::Pos2;
use glam::DVec3;
use hp_app::state::AppState;
use hp_app::{Severity, ViewInfo, tf};
use hp_geom::Ray;

use super::stamps::StampShape;
use super::{TerraOp, grid};
use crate::common::{line_on_top, point, rgba};
use crate::disp::DisplacementTool;

const PREVIEW_LINES: usize = 18;
const PREVIEW_STEPS: usize = 36;

impl DisplacementTool {
    pub(crate) fn set_clone_source(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2) -> bool {
        let (o, d) = view.ray(pos);
        let Some(hit) = crate::disp::paint::pick_disp(&state.doc, &Ray::new(o, d)) else { return false };
        self.terraform.clone_source = Some(hit.point);
        let text = tf!("tf.clone.source_set", format!("{:.0} {:.0} {:.0}", hit.point.x, hit.point.y, hit.point.z));
        state.notify(Severity::Info, text);
        self.save_settings(state);
        true
    }

    pub(crate) fn terraform_overlay(&self, state: &AppState, out: &mut hp_render::Overlay) {
        let cursor = self.cursor_fresh(state);
        match self.terraform.op {
            TerraOp::Stamp => {
                let Some(c) = cursor.filter(|c| !c.blocked) else { return };
                let invert = c.mods.shift;
                let mut color = rgba(super::ui::op_color(TerraOp::Stamp, invert));
                color[3] = 0.8;
                let up = c.normal.try_normalize().unwrap_or(DVec3::Z);
                let size = self.brush.radius * self.terraform.stamp_scale;
                let height = if invert { -self.terraform.stamp_height } else { self.terraform.stamp_height };
                let (t1, t2) = grid::basis(up);
                let at = |i: f64, j: f64| -> Option<DVec3> {
                    let (du, dv) = (i * size, j * size);
                    let (s, inside) =
                        super::apply::stamp_height(self.terraform.stamp_shape, &self.terraform, du, dv, size);
                    (inside > 0.0).then(|| c.center + t1 * du + t2 * dv + up * (height * s * inside))
                };
                for line in 0..=PREVIEW_LINES {
                    let a = line as f64 / PREVIEW_LINES as f64 * 2.0 - 1.0;
                    for axis in 0..2 {
                        let mut last: Option<DVec3> = None;
                        for step in 0..=PREVIEW_STEPS {
                            let b = step as f64 / PREVIEW_STEPS as f64 * 2.0 - 1.0;
                            let p = if axis == 0 { at(b, a) } else { at(a, b) };
                            if let (Some(p0), Some(p1)) = (last, p) {
                                line_on_top(out, p0, p1, color);
                            }
                            last = p;
                        }
                    }
                }
                let mut rim = color;
                rim[3] = 1.0;
                let segments = 64;
                let square = self.terraform.stamp_shape == StampShape::Image;
                let rot = self.terraform.stamp_rotation.to_radians();
                let outline = |k: usize| -> DVec3 {
                    let a = k as f64 / segments as f64 * std::f64::consts::TAU;
                    let (x, y) = if square {
                        let (cx, cy) = (a.cos(), a.sin());
                        let m = cx.abs().max(cy.abs()).max(1e-9);
                        (cx / m, cy / m)
                    } else {
                        (a.cos(), a.sin())
                    };
                    let (s, co) = rot.sin_cos();
                    let (rx, ry) = (x * co - y * s, x * s + y * co);
                    c.center + (t1 * rx + t2 * ry) * size
                };
                for k in 0..segments {
                    line_on_top(out, outline(k), outline(k + 1), rim);
                }
                let (s, co) = rot.sin_cos();
                let tip = c.center + (t1 * co + t2 * s) * size * 1.1;
                line_on_top(out, c.center, tip, rim);
                point(out, tip, rim, 6.0);
            }
            TerraOp::Clone => {
                let Some(source) = self.terraform.clone_source else { return };
                let mut color = rgba(super::ui::op_color(TerraOp::Clone, false));
                let up = cursor.map_or(DVec3::Z, |c| c.normal.try_normalize().unwrap_or(DVec3::Z));
                let src = match (cursor, self.stroke.as_ref().and_then(|s| s.terra.origin)) {
                    (Some(c), Some(origin)) => {
                        super::apply::clone_point(source, origin, c.center, up, self.terraform.clone_rotation)
                    }
                    _ => source,
                };
                let (t1, t2) = grid::basis(up);
                let r = self.brush.radius;
                let segments = 48;
                for k in 0..segments {
                    let a0 = k as f64 / segments as f64 * std::f64::consts::TAU;
                    let a1 = (k + 1) as f64 / segments as f64 * std::f64::consts::TAU;
                    let p0 = src + (t1 * a0.cos() + t2 * a0.sin()) * r;
                    let p1 = src + (t1 * a1.cos() + t2 * a1.sin()) * r;
                    line_on_top(out, p0, p1, color);
                }
                let arm = r * 0.25;
                line_on_top(out, src - t1 * arm, src + t1 * arm, color);
                line_on_top(out, src - t2 * arm, src + t2 * arm, color);
                let (s, co) = self.terraform.clone_rotation.to_radians().sin_cos();
                line_on_top(out, src, src + (t1 * co + t2 * s) * r, color);
                point(out, src, color, 7.0);
                if let Some(c) = cursor {
                    color[3] = 0.45;
                    let steps = 24;
                    for k in (0..steps).step_by(2) {
                        let a = c.center.lerp(src, k as f64 / steps as f64);
                        let b = c.center.lerp(src, (k + 1) as f64 / steps as f64);
                        line_on_top(out, a, b, color);
                    }
                }
            }
            _ => {}
        }
    }
}
