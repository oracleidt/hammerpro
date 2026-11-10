use egui::{Align2, CursorIcon, Pos2, Ui};
use glam::{DVec2, DVec3};
use hp_app::state::AppState;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, t, tf};
use hp_editor::{Id, ObjRef};
use hp_render::Overlay;

use crate::common::{
    COLOR_GHOST, Mouse, Ov, Snap, SurfaceHit, active_material, clean_vec, create_entity, fgd_defaults, fresh,
    material_chip, material_size, paint_label, pick_surface, rgba, set_kv,
};

pub const DECAL_SURFACE_OFFSET: f64 = 1.0;

pub fn decal_basis(normal: DVec3) -> (DVec3, DVec3) {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    let (s, t) = if n.z.abs() > std::f64::consts::FRAC_1_SQRT_2 {
        let t = DVec3::X.cross(n);
        (n.cross(t), t)
    } else {
        let s = n.cross(DVec3::NEG_Z);
        (s, s.cross(n))
    };
    (s.normalize_or_zero(), t.normalize_or_zero())
}

pub fn decal_origin(point: DVec3, normal: DVec3) -> DVec3 {
    clean_vec(point + normal.normalize_or_zero() * DECAL_SURFACE_OFFSET)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DecalPreview {
    view: usize,
    frame: u64,
    point: DVec3,
    normal: DVec3,
    s: DVec3,
    t: DVec3,
    half: DVec2,
}

impl DecalPreview {
    fn quad(&self) -> [DVec3; 4] {
        let c = self.point + self.normal * 0.5;
        let (s, t) = (self.s * self.half.x, self.t * self.half.y);
        [c - s - t, c + s - t, c + s + t, c - s + t]
    }
}

pub struct DecalTool {
    pub snap_to_grid: bool,
    preview: Option<DecalPreview>,
}

impl Default for DecalTool {
    fn default() -> Self {
        Self::new()
    }
}

impl DecalTool {
    pub fn new() -> Self {
        Self { snap_to_grid: false, preview: None }
    }

    fn surface_point(&self, state: &AppState, view: &ViewInfo, hit: &SurfaceHit, m: &Mouse) -> DVec3 {
        let surface = Some(ObjRef::Solid(hit.solid));
        let snap = if self.snap_to_grid { Snap::on_object(state, view, m.modifiers, surface) } else { Snap::OFF };
        snap.on_plane(hit.point, hit.normal)
    }

    pub fn place(&mut self, state: &mut AppState, point: DVec3, normal: DVec3, keep_selection: bool) -> Option<Id> {
        let material = active_material(state);
        let mut kv = fgd_defaults(state, "infodecal");
        set_kv(&mut kv, "texture", material);
        match create_entity(state, "infodecal", decal_origin(point, normal), &kv, keep_selection) {
            Ok(id) => Some(id),
            Err(e) => {
                state.notify(Severity::Error, tf!("create.decal.failed", e));
                None
            }
        }
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        if view.is_2d() {
            if m.activated() {
                state.toasts.push(Severity::Info, t!("create.need_3d"));
                return ToolFlow::Handled;
            }
            return if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored };
        }
        let hit = pick_surface(state, view, pos);
        if m.hovering() {
            self.preview = match hit {
                Some(h) => {
                    let material = active_material(state);
                    let size = material_size(state, &material);
                    let (s, t) = decal_basis(h.normal);
                    Some(DecalPreview {
                        view: view.index,
                        frame: state.frame.index,
                        point: self.surface_point(state, view, &h, m),
                        normal: h.normal,
                        s,
                        t,
                        half: size * 0.5,
                    })
                }
                None => None,
            };
            return ToolFlow::Ignored;
        }
        if m.activated()
            && let Some(h) = hit
        {
            let p = self.surface_point(state, view, &h, m);
            self.place(state, p, h.normal, m.modifiers.shift);
        }
        if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored }
    }
}

impl Tool for DecalTool {
    fn id(&self) -> ToolId {
        ToolId::Decal
    }

    fn deactivate(&mut self, _state: &mut AppState) {
        self.preview = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            self.preview = None;
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let Some(p) = self.preview.filter(|p| fresh(state, p.frame)) else { return };
        let mut ov = Ov::new(out, view);
        let quad = p.quad();
        ov.polygon_fill(&quad, rgba(COLOR_GHOST, 0.22));
        ov.polygon_outline(&quad, rgba(COLOR_GHOST, 1.0), true);
        ov.line(p.point, p.point + p.normal * 16.0, rgba(COLOR_GHOST, 0.9));
        ov.point(p.point, rgba(COLOR_GHOST, 1.0), 5.0);
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(p) = self.preview.filter(|p| fresh(state, p.frame) && p.view == view.index) else { return };
        if let Some(s) = view.world_to_screen(p.point)
            && view.rect.contains(s)
        {
            let text = format!("{}  {} x {}", active_material(state), p.half.x * 2.0, p.half.y * 2.0);
            paint_label(painter, s + egui::vec2(12.0, -12.0), Align2::LEFT_BOTTOM, &text, COLOR_GHOST);
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            material_chip(ui, state);
            let material = active_material(state);
            let size = material_size(state, &material);
            ui.label(tf!("create.decal.size", size.x, size.y)).on_hover_text(t!("create.decal.size_tip"));
            ui.separator();
            ui.checkbox(&mut self.snap_to_grid, t!("action.toggle_snap"));
        });
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        Some(if view.is_2d() { CursorIcon::NotAllowed } else { CursorIcon::Crosshair })
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        Some(t!("create.decal.hint").to_owned())
    }

    fn set_option(&mut self, _state: &mut AppState, key: &str, value: &str) -> bool {
        match key {
            "snap" => crate::common::option_flag(value).map(|b| self.snap_to_grid = b).is_some(),
            _ => false,
        }
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        vec![hp_app::tool::ToolOption::new(
            "snap",
            crate::common::on_off(self.snap_to_grid),
            "on: the decal snaps to the grid of the face, off: it follows the cursor freely",
        )]
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        self.preview = None;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{fgd_state, info_2d, info_3d, settle};
    use hp_app::ViewKind;

    #[test]
    fn engine_basis() {
        let (s, t) = decal_basis(DVec3::Z);
        assert!((s - DVec3::X).length() < 1e-12 && (t - DVec3::NEG_Y).length() < 1e-12);
        let (s, t) = decal_basis(DVec3::X);
        assert!((s - DVec3::Y).length() < 1e-12 && (t - DVec3::NEG_Z).length() < 1e-12);
        let (s, t) = decal_basis(DVec3::NEG_Y);
        assert!((s - DVec3::X).length() < 1e-12 && (t - DVec3::NEG_Z).length() < 1e-12);
        for n in [DVec3::new(1.0, 1.0, 0.3).normalize(), DVec3::new(0.2, -0.1, -1.0).normalize()] {
            let (s, t) = decal_basis(n);
            assert!(s.dot(n).abs() < 1e-9 && t.dot(n).abs() < 1e-9 && s.dot(t).abs() < 1e-9);
        }
    }

    #[test]
    fn places_infodecal_on_the_wall() {
        let mut state = fgd_state();
        state.current_material = "decals/decalgraffiti001a".to_owned();
        let mut tool = DecalTool::new();
        let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0);
        let p = view.world_to_screen(DVec3::new(20.0, 256.0, 70.0)).expect("screen");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(p));
        let preview = tool.preview.expect("preview");
        assert_eq!(preview.half, DVec2::splat(256.0), "mock materials are 512 x 512");
        let (undo, _) = state.doc.history_len();
        tool.handle_mouse(&mut state, &view, &Mouse::click(p));
        assert_eq!(state.doc.history_len().0, undo + 1);
        let sel = state.doc.selected_entity_ids();
        let e = &state.doc.scene().entity(sel[0]).expect("decal").entity;
        assert_eq!(e.classname, "infodecal");
        assert_eq!(e.get("texture"), Some("decals/decalgraffiti001a"));
        assert_eq!(e.get("LowPriority"), Some("0"), "FGD default");
        let o = e.origin().expect("origin");
        assert!((o.y - (256.0 - DECAL_SURFACE_OFFSET)).abs() < 1e-6, "{o:?}");
        assert!((o.x - 20.0).abs() < 0.5 && (o.z - 70.0).abs() < 0.5, "{o:?}");
    }

    #[test]
    fn snapping_and_2d_views() {
        let mut state = settle();
        let mut tool = DecalTool::new();
        tool.snap_to_grid = true;
        let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0);
        let p = view.world_to_screen(DVec3::new(20.0, 256.0, 70.0)).expect("screen");
        let id = {
            tool.handle_mouse(&mut state, &view, &Mouse::click(p));
            state.doc.selected_entity_ids()[0]
        };
        let o = state.doc.scene().entity(id).expect("decal").entity.origin().expect("origin");
        assert_eq!(o, DVec3::new(16.0, 255.0, 64.0));
        let top = info_2d(ViewKind::Top, 1.0);
        let entities = state.doc.stats().entities;
        assert_eq!(tool.handle_mouse(&mut state, &top, &Mouse::click(top.rect.center())), ToolFlow::Handled);
        assert_eq!(state.doc.stats().entities, entities);
    }
}
