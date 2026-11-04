use glam::{DMat3, DVec2, DVec3};
use hp_editor::ObjRef;

use crate::grid::{GridFrame, GridSource};
use crate::settings::ViewGrid;
use crate::state::AppState;
use crate::view::ViewKind;

pub const ALIGN_QUICK_ANGLE: f64 = 45.0;

fn tidy_angle(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    r + 0.0
}

fn sin_cos(degrees: f64) -> (f64, f64) {
    match degrees.rem_euclid(360.0) {
        0.0 => (0.0, 1.0),
        90.0 => (1.0, 0.0),
        180.0 => (0.0, -1.0),
        270.0 => (-1.0, 0.0),
        d => d.to_radians().sin_cos(),
    }
}

pub fn view_rotation(kind: ViewKind, degrees: f64) -> DMat3 {
    let (r, u, d) = kind.axes_2d().unwrap_or((0, 1, 2));
    let (s, c) = if degrees.is_finite() { sin_cos(degrees) } else { (0.0, 1.0) };
    let mut cols = [DVec3::ZERO; 3];
    cols[r][r] = c;
    cols[r][u] = s;
    cols[u][r] = -s;
    cols[u][u] = c;
    cols[d][d] = 1.0;
    DMat3::from_cols(cols[0], cols[1], cols[2])
}

impl GridFrame {
    pub fn rotated_in_view(mut self, kind: ViewKind, degrees: f64) -> GridFrame {
        if degrees != 0.0 && degrees.is_finite() {
            self.axes = view_rotation(kind, degrees) * self.axes;
        }
        self
    }

    pub fn angle_in_view(&self, kind: ViewKind) -> f64 {
        let a = self.lattice_2d(kind).a;
        if a.length() > 0.0 { tidy_angle(a.y.atan2(a.x).to_degrees()) } else { 0.0 }
    }
}

fn longest_edge_2d(kind: ViewKind, polygons: impl Iterator<Item = Vec<DVec3>>) -> Option<DVec2> {
    let mut best: Option<(f64, DVec2)> = None;
    for v in polygons {
        for i in 0..v.len() {
            let d = kind.to_2d(v[(i + 1) % v.len()] - v[i]);
            let len = d.length_squared();
            if len > 1e-12 && best.is_none_or(|(l, _)| len > l) {
                best = Some((len, d));
            }
        }
    }
    best.map(|(_, d)| d)
}

impl AppState {
    pub fn grid_angle(&self, kind: ViewKind) -> f64 {
        let target = self.active_grid(kind);
        match target.source {
            GridSource::Global | GridSource::View(_) => self.settings.grid.per_view.get(kind).sanitized().angle,
            _ => target.frame.angle_in_view(kind),
        }
    }

    pub fn set_grid_angle(&mut self, kind: ViewKind, degrees: f64) -> Result<(), String> {
        if !degrees.is_finite() {
            return Err("the grid angle must be a number of degrees".to_owned());
        }
        match self.active_grid(kind).source {
            GridSource::Global | GridSource::View(_) => {
                let g = self.settings.grid.per_view.get_mut(kind);
                let angle = ViewGrid { angle: degrees, ..g.clone() }.sanitized().angle;
                if g.angle != angle {
                    g.angle = angle;
                    self.settings_dirty = true;
                }
            }
            GridSource::Map(name) | GridSource::Preset(name) => {
                let Some((mut spec, source)) = self.find_grid(&name) else { return Ok(()) };
                let frame = GridFrame::from_spec(&spec, self.settings.edit.grid_size);
                let delta = degrees - frame.angle_in_view(kind);
                if delta.rem_euclid(360.0).min(360.0 - delta.rem_euclid(360.0)) < 1e-9 {
                    return Ok(());
                }
                let axes = view_rotation(kind, delta) * frame.axes;
                let angles = hp_geom::clean_angles(hp_geom::matrix_angles(&axes));
                spec.angles = angles.to_array().map(tidy_angle);
                self.put_custom_grid(&name, spec, matches!(source, GridSource::Map(_)), true)?;
            }
        }
        Ok(())
    }

    pub fn rotate_grid(&mut self, kind: ViewKind, delta: f64) -> Result<(), String> {
        let now = self.grid_angle(kind);
        self.set_grid_angle(kind, tidy_angle(now + delta))
    }

    pub fn selection_edge_angle(&self, kind: ViewKind) -> Option<f64> {
        let scene = self.doc.scene();
        let polygons =
            |solid| scene.solid(solid).map(|n| n.geom.faces.iter().map(|f| f.vertices.clone()).collect::<Vec<_>>());
        let dir = if let Some(face) = self.doc.selected_faces().into_iter().next() {
            let node = scene.solid(face.solid)?;
            let verts = node.geom.faces.iter().find(|f| f.side_index == face.side)?.vertices.clone();
            longest_edge_2d(kind, std::iter::once(verts))
        } else {
            let objs = self.doc.selected_objects();
            match scene.expand_solids(&objs).into_iter().find_map(polygons) {
                Some(faces) => longest_edge_2d(kind, faces.into_iter()),
                None => {
                    let id = objs.iter().find_map(|o| match o {
                        ObjRef::Entity(id) => Some(*id),
                        ObjRef::Solid(_) => None,
                    })?;
                    let angles = scene.entity(id)?.entity.angles().filter(|a| a.is_finite())?;
                    Some(kind.to_2d(hp_geom::angle_matrix(angles).x_axis)).filter(|d| d.length_squared() > 1e-12)
                }
            }
        }?;
        Some(tidy_angle(dir.y.atan2(dir.x).to_degrees()))
    }

    pub fn align_grid_to_selection(&mut self, kind: ViewKind) -> Result<f64, String> {
        let angle = self.selection_edge_angle(kind).ok_or_else(|| "select a face, a brush or an entity".to_owned())?;
        self.set_grid_angle(kind, angle)?;
        Ok(self.grid_angle(kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::GridSpec;

    const TOP: ViewKind = ViewKind::Top;

    fn near(a: DVec2, b: DVec2) -> bool {
        (a - b).length() < 1e-6
    }

    #[test]
    fn rotated_lattice_snaps_on_its_own_nodes() {
        let f = GridFrame::uniform(16.0).rotated_in_view(TOP, 45.0);
        assert!(f.is_rotated());
        let step = 16.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert!(near(f.snap_2d(TOP, DVec2::new(step + 1.0, step - 1.0)), DVec2::new(step, step)));
        assert!(near(f.snap_2d(TOP, DVec2::new(-0.5, 0.6)), DVec2::ZERO));
        assert!(near(f.snap_2d(TOP, DVec2::new(0.3, 2.0 * step - 0.2)), DVec2::new(0.0, 2.0 * step)));
    }

    #[test]
    fn negative_angles_and_full_turns() {
        let a = GridFrame::uniform(8.0).rotated_in_view(TOP, -30.0);
        let b = GridFrame::uniform(8.0).rotated_in_view(TOP, 330.0);
        let p = DVec2::new(37.3, -12.9);
        assert!(near(a.snap_2d(TOP, p), b.snap_2d(TOP, p)));
        let (s, c) = (-30f64).to_radians().sin_cos();
        let node = DVec2::new(8.0 * c, 8.0 * s);
        assert!(near(a.snap_2d(TOP, node + DVec2::new(0.4, -0.3)), node));
        let quarter = GridFrame::uniform(16.0).rotated_in_view(TOP, 90.0);
        assert!(!quarter.is_rotated());
        assert_eq!(quarter.snap_2d(TOP, DVec2::new(17.0, 9.0)), DVec2::new(16.0, 16.0));
    }

    #[test]
    fn origin_offset_is_the_pivot() {
        let mut f = GridFrame::uniform(10.0).rotated_in_view(TOP, 45.0);
        f.origin = DVec3::new(100.0, 50.0, 0.0);
        assert!(near(f.snap_2d(TOP, DVec2::new(100.2, 49.9)), DVec2::new(100.0, 50.0)));
        let r = 10.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert!(near(f.snap_2d(TOP, DVec2::new(100.0 + r + 0.2, 50.0 + r - 0.3)), DVec2::new(100.0 + r, 50.0 + r)));
        let d = f.snap_delta_2d(TOP, DVec2::new(100.0, 50.0), DVec2::new(r + 0.2, r - 0.2));
        assert!(near(d, DVec2::new(r, r)));
        assert!((f.angle_in_view(TOP) - 45.0).abs() < 1e-9);
    }

    #[test]
    fn front_and_side_views_rotate_around_their_own_axis() {
        for kind in [ViewKind::Front, ViewKind::Side] {
            let f = GridFrame::uniform(16.0).rotated_in_view(kind, 45.0);
            assert!((f.angle_in_view(kind) - 45.0).abs() < 1e-9);
            let (_, _, d) = kind.axes_2d().unwrap();
            let depth = [f.axes.x_axis, f.axes.y_axis, f.axes.z_axis][d];
            assert!((depth[d] - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn global_per_view_and_custom_grids_rotate_and_undo() {
        let mut s = crate::testing::test_state();
        assert_eq!(s.grid_angle(TOP), 0.0);
        s.set_grid_angle(TOP, 45.0).unwrap();
        assert_eq!(s.grid_angle(TOP), 45.0);
        assert!(s.grid_frame(TOP).is_rotated());
        assert!(!s.grid_frame(ViewKind::Front).is_rotated());
        s.rotate_grid(TOP, 45.0).unwrap();
        assert_eq!(s.grid_angle(TOP), 90.0);
        s.set_grid_angle(TOP, 0.0).unwrap();
        assert!(!s.grid_frame(TOP).is_rotated());
        s.set_grid_angle(TOP, 20.0).unwrap();
        s.set_grid_mode(crate::settings::GridMode::PerView);
        assert_eq!(s.grid_angle(TOP), 20.0);
        assert_eq!(s.settings.grid.per_view.top.size, s.settings.edit.grid_size);
        s.set_grid_mode(crate::settings::GridMode::Global);
        s.put_custom_grid("", GridSpec::uniform("R", 16.0), true, false).unwrap();
        s.use_custom_grid("R").unwrap();
        s.set_grid_angle(TOP, 30.0).unwrap();
        assert!((s.grid_angle(TOP) - 30.0).abs() < 1e-6);
        assert_eq!(s.find_grid("R").unwrap().0.angles[1], 30.0);
        s.set_grid_angle(TOP, 0.0).unwrap();
        assert_eq!(s.find_grid("R").unwrap().0.angles, [0.0; 3]);
        assert!(s.set_grid_angle(TOP, f64::NAN).is_err());
    }

    #[test]
    fn settings_keep_the_angle() {
        let mut s = crate::settings::GridSettings::default();
        s.per_view.top.angle = -37.5;
        let back: crate::settings::GridSettings = toml::from_str(&toml::to_string_pretty(&s).unwrap()).unwrap();
        assert_eq!(back.per_view.top.angle, -37.5);
        let old: crate::settings::GridSettings = toml::from_str("[per_view.top]\nsize = 8.0\n").unwrap();
        assert_eq!(old.per_view.top.angle, 0.0);
    }
}
