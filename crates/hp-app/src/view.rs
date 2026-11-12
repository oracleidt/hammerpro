use egui::{Pos2, Rect};
use glam::{DMat4, DVec2, DVec3, DVec4};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewKind {
    Perspective,
    Top,
    Front,
    Side,
}

impl ViewKind {
    pub const ALL: [ViewKind; 4] = [ViewKind::Perspective, ViewKind::Top, ViewKind::Front, ViewKind::Side];

    pub fn is_2d(self) -> bool {
        !matches!(self, ViewKind::Perspective)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            ViewKind::Perspective => "view.3d",
            ViewKind::Top => "view.top",
            ViewKind::Front => "view.front",
            ViewKind::Side => "view.side",
        }
    }

    pub fn from_index(i: u8) -> ViewKind {
        match i {
            1 => ViewKind::Top,
            2 => ViewKind::Front,
            3 => ViewKind::Side,
            _ => ViewKind::Perspective,
        }
    }

    pub fn axes_2d(self) -> Option<(usize, usize, usize)> {
        match self {
            ViewKind::Top => Some((0, 1, 2)),
            ViewKind::Front => Some((1, 2, 0)),
            ViewKind::Side => Some((0, 2, 1)),
            ViewKind::Perspective => None,
        }
    }

    pub fn to_2d(self, p: DVec3) -> DVec2 {
        match self.axes_2d() {
            Some((r, u, _)) => DVec2::new(p[r], p[u]),
            None => DVec2::new(p.x, p.y),
        }
    }

    pub fn from_2d(self, p: DVec2, depth: f64) -> DVec3 {
        let mut out = DVec3::ZERO;
        if let Some((r, u, d)) = self.axes_2d() {
            out[r] = p.x;
            out[u] = p.y;
            out[d] = depth;
        }
        out
    }

    pub fn toward_viewer(self) -> DVec3 {
        match self {
            ViewKind::Top => DVec3::Z,
            ViewKind::Front => DVec3::X,
            ViewKind::Side => DVec3::NEG_Y,
            ViewKind::Perspective => DVec3::Z,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuadrantView {
    View(ViewKind),
    Logical,
}

impl QuadrantView {
    pub const LOGICAL_INDEX: u8 = 4;

    pub const ALL: [QuadrantView; 5] = [
        QuadrantView::View(ViewKind::Perspective),
        QuadrantView::View(ViewKind::Top),
        QuadrantView::View(ViewKind::Front),
        QuadrantView::View(ViewKind::Side),
        QuadrantView::Logical,
    ];

    pub fn from_index(i: u8) -> QuadrantView {
        if i == Self::LOGICAL_INDEX { QuadrantView::Logical } else { QuadrantView::View(ViewKind::from_index(i)) }
    }

    pub fn index(self) -> u8 {
        match self {
            QuadrantView::View(ViewKind::Perspective) => 0,
            QuadrantView::View(ViewKind::Top) => 1,
            QuadrantView::View(ViewKind::Front) => 2,
            QuadrantView::View(ViewKind::Side) => 3,
            QuadrantView::Logical => Self::LOGICAL_INDEX,
        }
    }

    pub fn kind(self) -> Option<ViewKind> {
        match self {
            QuadrantView::View(kind) => Some(kind),
            QuadrantView::Logical => None,
        }
    }

    pub fn is_logical(self) -> bool {
        self == QuadrantView::Logical
    }

    pub fn label_key(self) -> &'static str {
        match self {
            QuadrantView::View(kind) => kind.label_key(),
            QuadrantView::Logical => "view.logical",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera3d {
    pub pos: DVec3,
    pub yaw_deg: f64,
    pub pitch_deg: f64,
    pub fov_y_deg: f64,
    pub near: f64,
    pub far: f64,
}

impl Default for Camera3d {
    fn default() -> Self {
        Self {
            pos: DVec3::new(-512.0, -512.0, 384.0),
            yaw_deg: 45.0,
            pitch_deg: -25.0,
            fov_y_deg: 75.0,
            near: 1.0,
            far: 32768.0,
        }
    }
}

impl Camera3d {
    pub fn forward(&self) -> DVec3 {
        let (sy, cy) = self.yaw_deg.to_radians().sin_cos();
        let (sp, cp) = self.pitch_deg.to_radians().sin_cos();
        DVec3::new(cp * cy, cp * sy, sp)
    }

    pub fn right(&self) -> DVec3 {
        let f = self.forward();
        let r = f.cross(DVec3::Z);
        if r.length_squared() < 1e-12 { DVec3::X } else { r.normalize() }
    }

    pub fn up(&self) -> DVec3 {
        self.right().cross(self.forward()).normalize()
    }

    pub fn look_at(&mut self, target: DVec3) {
        let d = target - self.pos;
        if d.length_squared() < 1e-9 {
            return;
        }
        let d = d.normalize();
        self.yaw_deg = d.y.atan2(d.x).to_degrees();
        self.pitch_deg = d.z.asin().to_degrees().clamp(-89.0, 89.0);
    }

    pub fn frame_box(&mut self, min: DVec3, max: DVec3) {
        let center = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(16.0);
        let dist = radius / (self.fov_y_deg.to_radians() * 0.5).tan().max(0.05) * 1.1;
        self.pos = center - self.forward() * dist;
    }

    pub fn view_matrix(&self) -> DMat4 {
        glam::dcamera::rh::view::look_to_mat4(self.pos, self.forward(), DVec3::Z)
    }

    pub fn proj_matrix(&self, aspect: f64) -> DMat4 {
        glam::dcamera::rh::proj::directx::perspective(
            self.fov_y_deg.to_radians(),
            aspect.max(1e-3),
            self.near,
            self.far,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera2d {
    pub center: DVec2,
    pub zoom: f64,
}

impl Default for Camera2d {
    fn default() -> Self {
        Self { center: DVec2::ZERO, zoom: 0.25 }
    }
}

impl Camera2d {
    pub const MIN_ZOOM: f64 = 0.002;
    pub const MAX_ZOOM: f64 = 64.0;

    pub fn zoom_at(&mut self, factor: f64, anchor: DVec2) {
        let new_zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let k = self.zoom / new_zoom;
        self.center = anchor + (self.center - anchor) * k;
        self.zoom = new_zoom;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ViewInfo {
    pub index: usize,
    pub kind: ViewKind,
    pub rect: Rect,
    pub cam3d: Camera3d,
    pub cam2d: Camera2d,
    pub pixels_per_point: f32,
    pub grid: f64,
}

impl ViewInfo {
    pub fn is_2d(&self) -> bool {
        self.kind.is_2d()
    }

    pub fn units_per_point(&self) -> f64 {
        if self.is_2d() {
            1.0 / self.cam2d.zoom
        } else {
            let h = self.rect.height().max(1.0) as f64;
            2.0 * (self.cam3d.fov_y_deg.to_radians() * 0.5).tan() * 512.0 / h
        }
    }

    pub fn aspect(&self) -> f64 {
        (self.rect.width().max(1.0) / self.rect.height().max(1.0)) as f64
    }

    pub fn view_proj(&self) -> DMat4 {
        if self.is_2d() {
            let w = self.rect.width().max(1.0) as f64 / self.cam2d.zoom;
            let h = self.rect.height().max(1.0) as f64 / self.cam2d.zoom;
            let toward = self.kind.toward_viewer();
            let center3 = self.kind.from_2d(self.cam2d.center, 0.0);
            let up = match self.kind {
                ViewKind::Top => DVec3::Y,
                _ => DVec3::Z,
            };
            let eye = center3 + toward * 65536.0;
            let view = glam::dcamera::rh::view::look_to_mat4(eye, -toward, up);
            let proj =
                glam::dcamera::rh::proj::directx::orthographic(-w * 0.5, w * 0.5, -h * 0.5, h * 0.5, 1.0, 131072.0);
            proj * view
        } else {
            self.cam3d.proj_matrix(self.aspect()) * self.cam3d.view_matrix()
        }
    }

    pub fn world_to_screen(&self, p: DVec3) -> Option<Pos2> {
        if self.is_2d() {
            return Some(self.screen_of_2d(self.kind.to_2d(p)));
        }
        let clip = self.view_proj() * DVec4::new(p.x, p.y, p.z, 1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        let x = self.rect.left() as f64 + (ndc.x * 0.5 + 0.5) * self.rect.width() as f64;
        let y = self.rect.top() as f64 + (1.0 - (ndc.y * 0.5 + 0.5)) * self.rect.height() as f64;
        Some(Pos2::new(x as f32, y as f32))
    }

    pub fn screen_to_2d(&self, pos: Pos2) -> DVec2 {
        let c = self.rect.center();
        DVec2::new(
            self.cam2d.center.x + (pos.x - c.x) as f64 / self.cam2d.zoom,
            self.cam2d.center.y - (pos.y - c.y) as f64 / self.cam2d.zoom,
        )
    }

    pub fn screen_of_2d(&self, p: DVec2) -> Pos2 {
        let c = self.rect.center();
        let x = c.x as f64 + (p.x - self.cam2d.center.x) * self.cam2d.zoom;
        let y = c.y as f64 - (p.y - self.cam2d.center.y) * self.cam2d.zoom;
        Pos2::new(x as f32, y as f32)
    }

    pub fn follow_2d(&self, before: &ViewInfo, pos: Pos2) -> Pos2 {
        if !self.is_2d() || before.kind != self.kind {
            return pos;
        }
        self.screen_of_2d(before.screen_to_2d(pos))
    }

    pub fn screen_to_world_2d(&self, pos: Pos2, depth: f64) -> DVec3 {
        self.kind.from_2d(self.screen_to_2d(pos), depth)
    }

    pub fn ray(&self, pos: Pos2) -> (DVec3, DVec3) {
        if self.is_2d() {
            let toward = self.kind.toward_viewer();
            let origin = self.screen_to_world_2d(pos, 0.0) + toward * 65536.0;
            return (origin, -toward);
        }
        let nx = ((pos.x - self.rect.left()) / self.rect.width().max(1.0)) as f64 * 2.0 - 1.0;
        let ny = 1.0 - ((pos.y - self.rect.top()) / self.rect.height().max(1.0)) as f64 * 2.0;
        let inv = self.view_proj().inverse();
        let near = inv * DVec4::new(nx, ny, 0.0, 1.0);
        let far = inv * DVec4::new(nx, ny, 1.0, 1.0);
        let near = near.truncate() / near.w;
        let far = far.truncate() / far.w;
        let dir = (far - near).normalize_or_zero();
        (self.cam3d.pos, if dir == DVec3::ZERO { self.cam3d.forward() } else { dir })
    }

    pub fn snap(&self, v: f64) -> f64 {
        if self.grid <= 0.0 { v } else { (v / self.grid).round() * self.grid }
    }

    pub fn snap_2d(&self, p: DVec2) -> DVec2 {
        DVec2::new(self.snap(p.x), self.snap(p.y))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CameraMotion {
    last: Option<[f64; 5]>,
    moved: bool,
}

impl CameraMotion {
    pub fn gliding(&mut self, cam: [f64; 5]) -> bool {
        let moved = self.last.is_some_and(|c| c != cam);
        let was = std::mem::replace(&mut self.moved, moved);
        self.last = Some(cam);
        moved && was
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(Pos2::new(100.0, 50.0), egui::vec2(800.0, 600.0)),
            cam3d: Camera3d::default(),
            cam2d: Camera2d { center: DVec2::new(10.0, 20.0), zoom: 2.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn screen_round_trip_2d() {
        for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
            let v = info(kind);
            let p = DVec3::new(37.0, -12.0, 99.0);
            let s = v.world_to_screen(p).unwrap();
            let back = v.screen_to_2d(s);
            let p2 = kind.to_2d(p);
            assert!((back - p2).length() < 1e-3, "{kind:?}");
        }
    }

    #[test]
    fn follow_2d_keeps_the_world_point_after_a_pan() {
        let before = info(ViewKind::Front);
        let mut after = before;
        after.cam2d.center += DVec2::new(40.0, -12.0);
        let press = Pos2::new(300.0, 200.0);
        let moved = after.follow_2d(&before, press);
        assert!((after.screen_to_2d(moved) - before.screen_to_2d(press)).length() < 1e-4);
        assert_eq!(moved, Pos2::new(press.x - 80.0, press.y - 24.0));
        let persp = info(ViewKind::Perspective);
        assert_eq!(persp.follow_2d(&before, press), press);
        let top = info(ViewKind::Top);
        assert_eq!(top.follow_2d(&before, press), press);
    }

    #[test]
    fn top_view_orientation() {
        let v = info(ViewKind::Top);
        let a = v.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).unwrap();
        let b = v.world_to_screen(DVec3::new(10.0, 10.0, 0.0)).unwrap();
        assert!(b.x > a.x, "+X goes right");
        assert!(b.y < a.y, "+Y goes up");
    }

    #[test]
    fn perspective_projection_center() {
        let mut v = info(ViewKind::Perspective);
        v.cam3d.pos = DVec3::ZERO;
        v.cam3d.yaw_deg = 0.0;
        v.cam3d.pitch_deg = 0.0;
        let s = v.world_to_screen(DVec3::new(100.0, 0.0, 0.0)).unwrap();
        assert!((s - v.rect.center()).length() < 0.5);
        let (o, d) = v.ray(v.rect.center());
        assert!((o - DVec3::ZERO).length() < 1e-9);
        assert!((d - DVec3::X).length() < 1e-6);
    }

    #[test]
    fn quadrant_views_round_trip_and_logical_has_no_projection() {
        assert_eq!(QuadrantView::ALL.len(), 5);
        assert!(QuadrantView::ALL.contains(&QuadrantView::Logical), "the view type menu offers the logical view");
        for (i, v) in QuadrantView::ALL.iter().enumerate() {
            assert_eq!(v.index(), i as u8);
            assert_eq!(QuadrantView::from_index(v.index()), *v);
        }
        for kind in ViewKind::ALL {
            assert_eq!(QuadrantView::View(kind).kind(), Some(kind));
            assert_eq!(ViewKind::from_index(QuadrantView::View(kind).index()), kind);
        }
        let logical = QuadrantView::from_index(QuadrantView::LOGICAL_INDEX);
        assert!(logical.is_logical());
        assert_eq!(logical.kind(), None, "no view kind, so no projection, camera or picking");
        assert_eq!(logical.label_key(), "view.logical");
        assert_eq!(ViewKind::from_index(QuadrantView::LOGICAL_INDEX), ViewKind::Perspective);
        assert_eq!(QuadrantView::from_index(200), QuadrantView::View(ViewKind::Perspective));
    }

    #[test]
    fn logical_quadrant_survives_the_settings_file() {
        use crate::settings::{SavedLayout, Settings};
        let mut s = Settings::default();
        s.ui.quadrants = [0, QuadrantView::LOGICAL_INDEX, 2, 3];
        let layout = SavedLayout { quadrants: [QuadrantView::LOGICAL_INDEX, 1, 2, 0], ..SavedLayout::default() };
        s.workflow.layouts.insert("Logic".to_owned(), layout.clone());
        let back: Settings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(back.ui.quadrants, s.ui.quadrants);
        assert_eq!(QuadrantView::from_index(back.ui.quadrants[1]), QuadrantView::Logical);
        assert_eq!(back.workflow.layouts["Logic"], layout);
        let old: Settings = toml::from_str("[ui]\nquadrants = [1, 0, 3, 2]\n").expect("old settings load");
        assert_eq!(old.ui.quadrants, [1, 0, 3, 2]);
        let views: Vec<QuadrantView> = old.ui.quadrants.iter().map(|q| QuadrantView::from_index(*q)).collect();
        assert!(!views.contains(&QuadrantView::Logical));
    }

    #[test]
    fn zoom_keeps_anchor() {
        let mut c = Camera2d { center: DVec2::new(0.0, 0.0), zoom: 1.0 };
        let anchor = DVec2::new(100.0, 50.0);
        c.zoom_at(2.0, anchor);
        assert!((c.center - DVec2::new(50.0, 25.0)).length() < 1e-9);
    }

    #[test]
    fn camera_motion_glides_only_after_two_moving_frames() {
        let mut m = CameraMotion::default();
        assert!(!m.gliding([0.0; 5]));
        assert!(!m.gliding([1.0, 0.0, 0.0, 0.0, 0.0]), "a single jump is not a glide");
        assert!(m.gliding([2.0, 0.0, 0.0, 0.0, 0.0]));
        assert!(!m.gliding([2.0, 0.0, 0.0, 0.0, 0.0]), "a still frame ends it");
    }
}
