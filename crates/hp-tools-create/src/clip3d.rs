use egui::{Modifiers, Pos2};
use glam::DVec3;
use hp_app::ViewInfo;
use hp_app::state::AppState;
use hp_editor::ObjRef;
use hp_geom::Plane;

use crate::common::{Snap, dominant_axis, pick_surface, pick_surface_with};

pub(crate) const MAGNET_PX: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Vertex,
    Midpoint,
    Grid,
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CutAxis {
    #[default]
    Auto,
    X,
    Y,
    Z,
}

impl CutAxis {
    pub const ALL: [CutAxis; 4] = [CutAxis::Auto, CutAxis::X, CutAxis::Y, CutAxis::Z];

    pub fn id(self) -> &'static str {
        match self {
            CutAxis::Auto => "auto",
            CutAxis::X => "x",
            CutAxis::Y => "y",
            CutAxis::Z => "z",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id().eq_ignore_ascii_case(s.trim()))
    }

    pub fn next(self) -> Self {
        match self {
            CutAxis::Auto => CutAxis::X,
            CutAxis::X => CutAxis::Y,
            CutAxis::Y => CutAxis::Z,
            CutAxis::Z => CutAxis::Auto,
        }
    }

    pub fn resolve(self, face_normal: DVec3, forward: DVec3) -> usize {
        match self {
            CutAxis::X => 0,
            CutAxis::Y => 1,
            CutAxis::Z => 2,
            CutAxis::Auto => auto_axis(face_normal, forward),
        }
    }
}

pub fn auto_axis(face_normal: DVec3, forward: DVec3) -> usize {
    let skip = dominant_axis(face_normal);
    let mut best = (2usize, f64::NEG_INFINITY);
    for axis in [2usize, 0, 1] {
        if axis == skip {
            continue;
        }
        let w = forward[axis].abs();
        if w > best.1 + 1e-9 {
            best = (axis, w);
        }
    }
    best.0
}

pub fn face_cut_plane(a: DVec3, b: DVec3, face_normal: DVec3) -> Option<Plane> {
    let d = b - a;
    let n = face_normal.try_normalize()?;
    let len = d.length();
    if !(len.is_finite() && len > 1e-6) {
        return None;
    }
    let normal = d.cross(n);
    if normal.length() < len * 1e-6 {
        return None;
    }
    Plane::from_normal_point(normal, a)
}

pub fn axis_plane(axis: usize, p: DVec3) -> Option<Plane> {
    let n = [DVec3::X, DVec3::Y, DVec3::Z].get(axis).copied()?;
    Plane::from_normal_point(n, p)
}

pub fn facing(plane: Plane, eye: DVec3) -> Plane {
    if plane.distance(eye) < 0.0 { plane.flip() } else { plane }
}

pub fn keep_side(plane: Plane, old: Option<Plane>) -> Plane {
    match old {
        Some(o) if o.normal.dot(plane.normal) < 0.0 => plane.flip(),
        _ => plane,
    }
}

pub fn snap_targets(face: &[DVec3]) -> Vec<(DVec3, SnapKind)> {
    let mut out: Vec<(DVec3, SnapKind)> = face.iter().map(|v| (*v, SnapKind::Vertex)).collect();
    for (i, a) in face.iter().enumerate() {
        if let Some(b) = face.get((i + 1) % face.len()).filter(|_| face.len() > 1) {
            out.push(((*a + *b) * 0.5, SnapKind::Midpoint));
        }
    }
    out
}

pub fn magnet(view: &ViewInfo, pos: Pos2, targets: &[(DVec3, SnapKind)], radius: f32) -> Option<(DVec3, SnapKind)> {
    let mut best: Option<(f32, DVec3, SnapKind)> = None;
    for (p, kind) in targets {
        let Some(s) = view.world_to_screen(*p) else { continue };
        let d = s.distance(pos);
        let bias = if *kind == SnapKind::Vertex { 0.0 } else { 0.5 };
        if d <= radius && best.is_none_or(|(bd, _, _)| d + bias < bd) {
            best = Some((d + bias, *p, *kind));
        }
    }
    best.map(|(_, p, k)| (p, k))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pick3d {
    pub point: DVec3,
    pub kind: SnapKind,
    pub face_plane: Plane,
}

pub(crate) fn pick_3d(state: &AppState, view: &ViewInfo, pos: Pos2, modifiers: Modifiers) -> Option<Pick3d> {
    let selected = state.doc.selected_solid_ids();
    let only_selected = |o: ObjRef| matches!(o, ObjRef::Solid(id) if selected.contains(&id));
    let hit = pick_surface_with(state, view, pos, Some(&only_selected)).or_else(|| pick_surface(state, view, pos))?;
    let face: Vec<DVec3> = state
        .doc
        .scene()
        .solid(hit.solid)
        .and_then(|n| n.geom.face_for_side(hit.side_index))
        .map(|f| f.vertices.clone())
        .unwrap_or_default();
    let snap = Snap::for_selection(state, view, modifiers);
    let magnetic = if modifiers.alt { None } else { magnet(view, pos, &snap_targets(&face), MAGNET_PX) };
    let (point, kind) = match magnetic {
        Some(found) => found,
        None if snap.is_on() => (snap.on_plane(hit.point, hit.face_plane.normal), SnapKind::Grid),
        None => (hit.point, SnapKind::Free),
    };
    point.is_finite().then_some(Pick3d { point, kind, face_plane: hit.face_plane })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{info_3d, settle};

    #[test]
    fn face_line_gives_a_plane_perpendicular_to_the_face() {
        let p = face_cut_plane(DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 64.0, 0.0), DVec3::Z).expect("plane");
        assert!((p.normal.abs() - DVec3::X).length() < 1e-12);
        assert!(p.normal.dot(DVec3::Z).abs() < 1e-12);
        let tilted = face_cut_plane(DVec3::ZERO, DVec3::new(64.0, 64.0, 0.0), DVec3::Z).expect("plane");
        assert!(tilted.distance(DVec3::new(32.0, 32.0, 500.0)).abs() < 1e-9, "contains the face normal");
        assert!(face_cut_plane(DVec3::ZERO, DVec3::ZERO, DVec3::Z).is_none());
        assert!(face_cut_plane(DVec3::ZERO, DVec3::new(0.0, 0.0, 32.0), DVec3::Z).is_none(), "line along the normal");
        assert!(face_cut_plane(DVec3::ZERO, DVec3::X, DVec3::ZERO).is_none());
    }

    #[test]
    fn axis_planes_and_auto_axis() {
        let p = axis_plane(1, DVec3::new(5.0, 40.0, -3.0)).expect("plane");
        assert_eq!((p.normal, p.dist), (DVec3::Y, 40.0));
        assert!(axis_plane(3, DVec3::ZERO).is_none());
        let along_x = DVec3::new(0.7, 0.0, -0.7);
        assert_eq!(auto_axis(DVec3::Z, along_x), 0, "floor seen along X: cut across the view");
        assert_eq!(auto_axis(DVec3::NEG_Y, DVec3::new(0.05, 0.99, -0.1)), 2, "wall seen from the front");
        assert_eq!(auto_axis(DVec3::NEG_Y, DVec3::Y), 2, "ties go to the horizontal cut");
        assert_eq!(CutAxis::Auto.resolve(DVec3::Z, along_x), 0);
        assert_eq!(CutAxis::Z.resolve(DVec3::Z, along_x), 2);
        let mut a = CutAxis::Auto;
        for expected in [CutAxis::X, CutAxis::Y, CutAxis::Z, CutAxis::Auto] {
            a = a.next();
            assert_eq!(a, expected);
            assert_eq!(CutAxis::from_id(a.id()), Some(a));
        }
        assert_eq!(CutAxis::from_id(" Y "), Some(CutAxis::Y));
        assert!(CutAxis::from_id("w").is_none());
    }

    #[test]
    fn orientation_helpers() {
        let p = Plane::new(DVec3::X, 0.0).expect("plane");
        assert_eq!(facing(p, DVec3::new(-10.0, 0.0, 0.0)).normal, DVec3::NEG_X);
        assert_eq!(facing(p, DVec3::new(10.0, 0.0, 0.0)).normal, DVec3::X);
        assert_eq!(keep_side(p, Some(p.flip())).normal, DVec3::NEG_X);
        assert_eq!(keep_side(p, None).normal, DVec3::X);
    }

    #[test]
    fn magnet_prefers_vertices_and_midpoints_near_the_cursor() {
        let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0);
        let face = [
            DVec3::new(-256.0, 256.0, 0.0),
            DVec3::new(256.0, 256.0, 0.0),
            DVec3::new(256.0, 256.0, 128.0),
            DVec3::new(-256.0, 256.0, 128.0),
        ];
        let targets = snap_targets(&face);
        assert_eq!(targets.len(), 8);
        let near_mid = view.world_to_screen(DVec3::new(2.0, 256.0, 1.0)).expect("screen");
        assert_eq!(
            magnet(&view, near_mid, &targets, MAGNET_PX),
            Some((DVec3::new(0.0, 256.0, 0.0), SnapKind::Midpoint))
        );
        let near_corner = view.world_to_screen(DVec3::new(-254.0, 256.0, 2.0)).expect("screen");
        assert_eq!(magnet(&view, near_corner, &targets, MAGNET_PX).map(|t| t.1), Some(SnapKind::Vertex));
        let far = view.world_to_screen(DVec3::new(-100.0, 256.0, 60.0)).expect("screen");
        assert!(magnet(&view, far, &targets, MAGNET_PX).is_none());
    }

    #[test]
    fn picking_snaps_to_the_grid_on_the_face_and_prefers_selected_brushes() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0);
        let s = view.world_to_screen(DVec3::new(-61.0, 256.0, 37.0)).expect("screen");
        let p = pick_3d(&state, &view, s, Modifiers::NONE).expect("pick");
        assert_eq!(p.kind, SnapKind::Grid);
        assert!((p.point - DVec3::new(-64.0, 256.0, 32.0)).length() < 1e-9, "{:?}", p.point);
        assert!((p.face_plane.normal - DVec3::NEG_Y).length() < 1e-9);
        let raw = pick_3d(&state, &view, s, Modifiers::ALT).expect("pick");
        assert_eq!(raw.kind, SnapKind::Free);
        assert!((raw.point.y - 256.0).abs() < 1e-6 && (raw.point.x + 61.0).abs() < 0.5);
        let corner = view.world_to_screen(DVec3::new(-255.0, 256.0, 1.0)).expect("screen");
        let v = pick_3d(&state, &view, corner, Modifiers::NONE).expect("pick");
        assert_eq!((v.point, v.kind), (DVec3::new(-256.0, 256.0, 0.0), SnapKind::Vertex));
        let below = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, -60.0);
        let floor = below.world_to_screen(DVec3::new(0.0, 100.0, 0.0)).expect("screen");
        let f = pick_3d(&state, &below, floor, Modifiers::NONE).expect("pick");
        assert!((f.face_plane.normal - DVec3::Z).length() < 1e-9, "unselected surfaces still answer");
        let behind = info_3d(DVec3::new(96.0, -200.0, 32.0), 90.0, 0.0);
        let mid = behind.world_to_screen(DVec3::new(96.0, 0.0, 32.0)).expect("screen");
        let first = pick_3d(&state, &behind, mid, Modifiers::NONE).expect("pick");
        assert!((first.point.y - 256.0).abs() < 1e-6, "the selected wall behind the trigger: {:?}", first.point);
        state.doc.clear_selection();
        let any = pick_3d(&state, &behind, mid, Modifiers::NONE).expect("pick");
        assert!(any.point.y.abs() < 1e-6, "nothing selected: the closest surface");
    }
}
