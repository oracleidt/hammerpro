use egui::{Align2, CursorIcon, Pos2, RichText, Ui};
use glam::{DQuat, DVec3};
use hp_app::state::{AppState, ModelInfo, ModelState};
use hp_app::{Action, Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, t, tf};
use hp_editor::{Id, ObjRef, PickOptions};
use hp_fgd::ResolvedClass;
use hp_geom::{Aabb, Ray, clean_angles};
use hp_render::Overlay;

use crate::common::{
    COLOR_GHOST, Mouse, Ov, Snap, box_screen_rect, clean_vec, create_entity, depth_axis, fgd_defaults, fmt_vec, fresh,
    ground_point, paint_label, pick_surface, rgba, round_to, set_kv,
};
use crate::model_bvh::{ModelHit, ModelSurfaces, Surface, ray_placed};
use crate::picker::{ClassPicker, PickerSource, is_valid_class_name};
use crate::prefab_place::PrefabPlacer;

const MAX_RECENT: usize = 12;
const DEFAULT_HALF: f64 = 8.0;

pub fn is_prop_class(class: &str, def: Option<&ResolvedClass>) -> bool {
    if def.is_some_and(|c| {
        c.studio().is_some_and(|s| s.model.is_some())
            || c.property("model").and_then(|p| p.default_value()).is_some_and(|m| !m.trim().is_empty())
    }) {
        return false;
    }
    class.trim().to_ascii_lowercase().starts_with("prop_")
        || def.and_then(|c| c.studio()).is_some_and(|s| s.is_prop && s.model.is_none())
}

pub(crate) fn model_box(info: &ModelInfo) -> Option<Aabb> {
    let usable = |b: &Aabb| b.min.is_finite() && b.max.is_finite() && b.size().max_element() > 0.0;
    let hull = info.hull_min.zip(info.hull_max).map(|(a, b)| Aabb::new(a, b)).filter(usable);
    hull.or_else(|| Some(Aabb::new(info.bounds_min, info.bounds_max)).filter(usable))
}

const FREE_REST_STEP: f64 = 0.01;
const ORIGIN_STEP: f64 = 0.001;

pub fn rest_on_surface(point: DVec3, normal: DVec3, local: &Aabb, rotation: DQuat, grid: f64) -> DVec3 {
    rest_on_surface_snapped(point, normal, local, rotation, &Snap::grid(grid))
}

pub(crate) fn rest_on_surface_snapped(
    point: DVec3,
    normal: DVec3,
    local: &Aabb,
    rotation: DQuat,
    snap: &Snap,
) -> DVec3 {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    let p = snap.on_plane(point, n);
    let offset = local.corners().iter().map(|c| (rotation * *c).dot(-n)).fold(f64::NEG_INFINITY, f64::max);
    let offset = if offset.is_finite() { offset } else { 0.0 };
    let step = if snap.is_on() { snap.step().min(1.0) } else { FREE_REST_STEP };
    let offset = round_to(offset, step);
    let o = p + n * offset;
    clean_vec(DVec3::new(round_to(o.x, ORIGIN_STEP), round_to(o.y, ORIGIN_STEP), round_to(o.z, ORIGIN_STEP)))
}

pub fn align_to_normal(normal: DVec3) -> (DQuat, DVec3) {
    hp_geom::align_to_normal(normal)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ghost {
    view: usize,
    frame: u64,
    in_3d: bool,
    origin: DVec3,
    local: Aabb,
    rotation: DQuat,
    angles: Option<DVec3>,
}

impl Ghost {
    fn corners(&self) -> [DVec3; 8] {
        self.local.corners().map(|c| self.origin + self.rotation * c)
    }

    fn world_bounds(&self) -> Aabb {
        Aabb::from_points(self.corners())
    }
}

pub struct EntityTool {
    pub align_to_normal: bool,
    recent: Vec<String>,
    picker: ClassPicker,
    ghost: Option<Ghost>,
    last_3d: Option<DVec3>,
    sel_center: Option<(u64, Option<DVec3>)>,
    prefab: PrefabPlacer,
    surfaces: ModelSurfaces,
    prop_hit: Option<([u64; 7], Option<ModelHit>)>,
    repaint: Option<egui::Context>,
}

impl Default for EntityTool {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityTool {
    pub fn new() -> Self {
        Self {
            align_to_normal: false,
            recent: Vec::new(),
            picker: ClassPicker::default(),
            ghost: None,
            last_3d: None,
            sel_center: None,
            prefab: PrefabPlacer::default(),
            surfaces: ModelSurfaces::default(),
            prop_hit: None,
            repaint: None,
        }
    }

    pub fn recent_classes(&self) -> &[String] {
        &self.recent
    }

    pub fn push_recent(&mut self, class: &str) {
        let class = class.trim();
        if class.is_empty() {
            return;
        }
        self.recent.retain(|c| !c.eq_ignore_ascii_case(class));
        self.recent.insert(0, class.to_owned());
        self.recent.truncate(MAX_RECENT);
    }

    fn class(state: &AppState) -> String {
        let c = state.current_entity_class.trim();
        if c.is_empty() { state.settings.edit.default_point_entity.trim().to_owned() } else { c.to_owned() }
    }

    fn is_prop(state: &AppState, class: &str) -> bool {
        is_prop_class(class, state.assets.fgd().and_then(|f| f.class(class)))
    }

    pub(crate) fn local_bounds(state: &mut AppState, class: &str) -> Aabb {
        let fallback = Aabb::new(DVec3::splat(-DEFAULT_HALF), DVec3::splat(DEFAULT_HALF));
        let def = state.assets.fgd().and_then(|f| f.class(class));
        let prop = is_prop_class(class, def);
        let size =
            def.and_then(ResolvedClass::size).map(|(a, b)| Aabb::new(DVec3::from_array(a), DVec3::from_array(b)));
        let fixed_model = def.and_then(|c| c.studio()).and_then(|s| s.model).or_else(|| {
            def.and_then(|c| c.property("model")).and_then(|p| p.default_value()).filter(|m| !m.trim().is_empty())
        });
        let model = if prop {
            state.current_model.clone()
        } else if size.is_none() {
            fixed_model
        } else {
            None
        };
        if let Some(path) = model.filter(|p| !p.trim().is_empty())
            && let ModelState::Ready(info) = state.assets.model_info(&path)
            && let Some(b) = model_box(&info)
        {
            return b;
        }
        size.filter(|b| b.min.is_finite() && b.max.is_finite()).unwrap_or(fallback)
    }

    fn depth_2d(&mut self, state: &AppState, axis: usize) -> f64 {
        if let Some(p) = self.last_3d {
            return p[axis];
        }
        let rev = state.doc.revision();
        let center = match self.sel_center {
            Some((r, c)) if r == rev => c,
            _ => {
                let c = state.doc.selection_center(&|e| state.entity_bounds(e)).filter(|c| c.is_finite());
                self.sel_center = Some((rev, c));
                c
            }
        };
        center.map_or(0.0, |c| crate::common::clean(c[axis]))
    }

    fn prop_surface(&mut self, state: &AppState, view: &ViewInfo, pos: Pos2, limit: f64) -> Option<ModelHit> {
        self.surfaces.poll(state.assets.generation());
        let c = &view.cam3d;
        let key = [
            u64::from(pos.x.to_bits()) << 32 | u64::from(pos.y.to_bits()),
            c.pos.x.to_bits() ^ c.pos.y.to_bits().rotate_left(21) ^ c.pos.z.to_bits().rotate_left(42),
            c.yaw_deg.to_bits() ^ c.pitch_deg.to_bits().rotate_left(32),
            state.doc.revision(),
            self.surfaces.version(),
            limit.to_bits(),
            view.index as u64,
        ];
        if let Some((k, hit)) = &self.prop_hit
            && *k == key
        {
            return *hit;
        }
        let (o, d) = view.ray(pos);
        let mut found = None;
        if o.is_finite() && d.is_finite() {
            let opts = PickOptions { include_solids: false, max_distance: limit, ..PickOptions::default() };
            let hits = state.doc.pick_ray_all(&Ray::new(o, d), &opts, &|e| state.entity_bounds(e));
            let fs = state.assets.fs();
            for hit in hits {
                let ObjRef::Entity(id) = hit.obj else { continue };
                let Some(e) = state.doc.scene().entity(id).map(|n| &n.entity) else { continue };
                let Some(model) = e.get("model").filter(|m| m.trim().to_ascii_lowercase().ends_with(".mdl")) else {
                    continue;
                };
                let Some(origin) = e.origin().filter(|o| o.is_finite()) else { continue };
                let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
                if let Surface::Ready(bvh) = self.surfaces.get(model, fs, self.repaint.as_ref())
                    && let Some(h) = ray_placed(&bvh, origin, angles, o, d).filter(|h| h.t <= limit)
                {
                    found = Some(h);
                    break;
                }
            }
        }
        self.prop_hit = Some((key, found));
        found
    }

    fn placement_3d(
        &mut self,
        state: &mut AppState,
        view: &ViewInfo,
        pos: Pos2,
        snap: &Snap,
        class: &str,
    ) -> Option<Ghost> {
        let solid = pick_surface(state, view, pos);
        let surface_snap = solid
            .filter(|_| snap.is_on())
            .map(|hit| Snap::on_object(state, view, egui::Modifiers::NONE, Some(hp_editor::ObjRef::Solid(hit.solid))));
        let limit = solid.map_or(f64::INFINITY, |hit| (hit.point - view.ray(pos).0).length());
        let prop = self.prop_surface(state, view, pos, limit);
        let (point, normal) = match (prop, solid) {
            (Some(p), _) => (p.point, p.normal),
            (None, Some(hit)) => (hit.point, hit.normal),
            (None, None) => (ground_point(view, pos)?, DVec3::Z),
        };
        let local = Self::local_bounds(state, class);
        let (rotation, angles) = if self.align_to_normal && Self::is_prop(state, class) {
            let (q, a) = align_to_normal(normal);
            (q, Some(a))
        } else {
            (DQuat::IDENTITY, None)
        };
        let snap = match (prop, surface_snap) {
            (None, Some(s)) => s,
            _ => *snap,
        };
        let origin = rest_on_surface_snapped(point, normal, &local, rotation, &snap);
        Some(Ghost { view: view.index, frame: state.frame.index, in_3d: true, origin, local, rotation, angles })
    }

    pub fn place(
        &mut self,
        state: &mut AppState,
        origin: DVec3,
        angles: Option<DVec3>,
        keep_selection: bool,
    ) -> Option<Id> {
        let class = Self::class(state);
        if class.is_empty() || !is_valid_class_name(&class) {
            state.notify(Severity::Warning, t!("create.entity.no_class"));
            return None;
        }
        let mut kv = fgd_defaults(state, &class);
        if Self::is_prop(state, &class) {
            match state.current_model.clone().filter(|m| !m.trim().is_empty()) {
                Some(model) => {
                    set_kv(&mut kv, "model", model);
                    if state.current_model_skin > 0 {
                        set_kv(&mut kv, "skin", state.current_model_skin.to_string());
                    }
                }
                None => state.log(Severity::Warning, t!("create.entity.no_model_warning")),
            }
        }
        if let Some(a) = angles {
            set_kv(&mut kv, "angles", fmt_vec(clean_angles(a)));
        }
        match create_entity(state, &class, origin, &kv, keep_selection) {
            Ok(id) => {
                self.push_recent(&class);
                Some(id)
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("create.entity.failed", e));
                None
            }
        }
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        if PrefabPlacer::is_armed(state) {
            self.ghost = None;
            let depth = if view.is_2d() { self.depth_2d(state, depth_axis(view.kind)) } else { 0.0 };
            return self.prefab.handle_mouse(state, view, m, depth, None);
        }
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let snap = Snap::new(state, view, m.modifiers);
        let class = Self::class(state);
        let ghost = if view.is_2d() {
            let p2 = snap.v2(view.screen_to_2d(pos));
            let depth = self.depth_2d(state, depth_axis(view.kind));
            let local = Self::local_bounds(state, &class);
            Some(Ghost {
                view: view.index,
                frame: state.frame.index,
                in_3d: false,
                origin: clean_vec(view.kind.from_2d(p2, depth)),
                local,
                rotation: DQuat::IDENTITY,
                angles: None,
            })
        } else {
            self.placement_3d(state, view, pos, &snap, &class)
        };
        if m.hovering() {
            self.ghost = ghost;
            return ToolFlow::Ignored;
        }
        if m.activated() {
            if let Some(g) = ghost
                && self.place(state, g.origin, g.angles, m.modifiers.shift).is_some()
                && g.in_3d
            {
                self.last_3d = Some(g.origin);
            }
            self.ghost = ghost;
        }
        if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored }
    }

    fn class_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let class = Self::class(state);
        let generation = state.assets.generation();
        let mut picked = None;
        match state.assets.fgd().filter(|f| !f.point_classes().is_empty()) {
            Some(fgd) => {
                let src = PickerSource {
                    classes: fgd.point_classes(),
                    recent: &self.recent,
                    fixed: &[],
                    fgd: Some(fgd),
                    generation,
                };
                let tip = fgd
                    .class(&class)
                    .map(|c| c.description.trim())
                    .filter(|d| !d.is_empty())
                    .unwrap_or(t!("create.entity.class_tip"));
                picked = self.picker.show(ui, "entity_class", &class, tip, &src);
            }
            None => {
                let mut text = class.clone();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(170.0)
                        .hint_text(t!("create.entity.class_hint")),
                );
                let resp = resp.on_hover_text(t!("create.entity.no_fgd"));
                if resp.changed() && (text.is_empty() || is_valid_class_name(&text)) {
                    state.current_entity_class = text;
                }
            }
        }
        if let Some(p) = picked {
            state.current_entity_class = p;
        }
    }
}

impl Tool for EntityTool {
    fn id(&self) -> ToolId {
        ToolId::Entity
    }

    fn activate(&mut self, state: &mut AppState) {
        if state.current_entity_class.trim().is_empty() {
            state.current_entity_class = state.settings.edit.default_point_entity.clone();
        }
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.ghost = None;
        self.prefab.disarm(state);
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            self.ghost = None;
            self.prefab.clear();
            return ToolFlow::Ignored;
        }
        if self.repaint.is_none() {
            self.repaint = Some(input.response.ctx.clone());
        }
        if let Some(path) = state.runtime.placement.armed_prefab.clone() {
            let _ = state.runtime.placement.prefabs.get(&path, Some(&input.response.ctx));
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if PrefabPlacer::is_armed(state) {
            self.prefab.overlay(state, view, out);
            return;
        }
        let Some(g) = self.ghost.filter(|g| fresh(state, g.frame)) else { return };
        if !g.in_3d && g.view != view.index {
            return;
        }
        let mut ov = Ov::new(out, view);
        let c = g.corners();
        ov.hull_edges(&c, rgba(COLOR_GHOST, 0.95), true);
        ov.hull_faces(&c, rgba(COLOR_GHOST, 0.12));
        ov.point(g.origin, rgba(COLOR_GHOST, 1.0), 5.0);
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if PrefabPlacer::is_armed(state) {
            self.prefab.paint(state, view, painter);
            return;
        }
        let Some(g) = self.ghost.filter(|g| fresh(state, g.frame)) else { return };
        if !g.in_3d && g.view != view.index {
            return;
        }
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let class = Self::class(state);
        if view.is_2d() {
            if g.view == view.index
                && let Some(r) = box_screen_rect(view, &g.world_bounds())
            {
                paint_label(
                    &painter,
                    r.center_top() - egui::vec2(0.0, 4.0),
                    Align2::CENTER_BOTTOM,
                    &class,
                    COLOR_GHOST,
                );
            }
        } else if g.view == view.index
            && let Some(s) = view.world_to_screen(g.origin)
            && view.rect.contains(s)
        {
            paint_label(&painter, s + egui::vec2(10.0, -10.0), Align2::LEFT_BOTTOM, &class, COLOR_GHOST);
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        if PrefabPlacer::is_armed(state) {
            self.prefab.options_ui(ui, state);
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("create.entity.class"));
            self.class_ui(ui, state);
            let class = Self::class(state);
            if Self::is_prop(state, &class) {
                ui.separator();
                ui.label(t!("create.entity.model"));
                let model = state.current_model.clone();
                let text = match &model {
                    Some(m) if !m.trim().is_empty() => RichText::new(m.as_str()),
                    _ => RichText::new(t!("create.entity.no_model")).weak().italics(),
                };
                ui.add(egui::Label::new(text).truncate())
                    .on_hover_text(model.as_deref().unwrap_or(t!("create.entity.no_model")));
                if state.current_model_skin > 0 && model.as_deref().is_some_and(|m| !m.trim().is_empty()) {
                    ui.label(tf!("create.entity.skin", state.current_model_skin))
                        .on_hover_text(t!("create.entity.skin_tip"));
                }
                if ui.button(t!("create.entity.pick_model")).on_hover_text(t!("create.entity.pick_model_tip")).clicked()
                {
                    state.request(Action::ShowModelBrowser);
                }
                ui.separator();
                ui.checkbox(&mut self.align_to_normal, t!("create.entity.align"))
                    .on_hover_text(t!("create.entity.align_tip"));
            }
        });
    }

    fn cursor(&self, _state: &AppState, _view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        if PrefabPlacer::is_armed(state) {
            return self.prefab.status_hint(state);
        }
        Some(tf!("create.entity.hint", Self::class(state)))
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        self.ghost = None;
        self.prefab.cancel(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{
        fgd_state, fgd_state_with_model_infos, fgd_state_with_models, info_2d, info_3d, info_3d_looking_down, settle,
    };
    use egui::Modifiers;
    use hp_app::ViewKind;

    fn player_start_model() -> ModelInfo {
        ModelInfo {
            path: "models/editor/playerstart.mdl".to_owned(),
            bounds_min: DVec3::new(-13.0, -10.0, 0.113_388_687_372_207_64),
            bounds_max: DVec3::new(13.0, 10.0, 73.0),
            hull_min: Some(DVec3::new(-16.0, -16.0, 0.0)),
            hull_max: Some(DVec3::new(16.0, 16.0, 72.0)),
            ..Default::default()
        }
    }

    fn options_frame(
        ctx: &egui::Context,
        tool: &mut EntityTool,
        state: &mut AppState,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 200.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| tool.options_ui(ui, state));
        out.textures_delta.clear();
        let mut nodes = Vec::new();
        if let Some(update) = out.platform_output.accesskit_update {
            for (_, node) in update.nodes {
                let text = [node.label(), node.value()].into_iter().flatten().collect::<Vec<_>>().join(" ");
                if let Some(b) = node.bounds() {
                    let min = egui::pos2(b.x0 as f32, b.y0 as f32);
                    let max = egui::pos2(b.x1 as f32, b.y1 as f32);
                    nodes.push((text, egui::Rect::from_min_max(min, max)));
                }
            }
        }
        nodes
    }

    fn player_box() -> Aabb {
        Aabb::new(DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 72.0))
    }

    #[test]
    fn predefined_prop_models_ignore_browser_selection_during_creation() {
        let mut state = crate::tests_support::fgd_state_from(
            r#"
            @PointClass studio("models/editor/fixed.mdl") = prop_fixed : "Fixed" []
            @PointClass studioprop() = prop_default : "Default" [ model(studio) : "Model" : "models/default.mdl" ]
            @PointClass studioprop() = prop_generic : "Generic" [ model(studio) : "Model" ]
        "#,
        );
        state.current_model = Some("models/browser.mdl".to_owned());
        state.current_model_skin = 3;
        let mut tool = EntityTool::new();
        for (class, expected) in [
            ("prop_fixed", None),
            ("prop_default", Some("models/default.mdl")),
            ("prop_generic", Some("models/browser.mdl")),
        ] {
            state.current_entity_class = class.to_owned();
            let id = tool.place(&mut state, DVec3::ZERO, None, false).unwrap();
            let entity = &state.doc.scene().entity(id).unwrap().entity;
            assert_eq!(entity.get("model"), expected);
            assert_eq!(entity.get("skin"), if class == "prop_generic" { Some("3") } else { None });
        }
    }

    #[test]
    fn resting_offsets_on_floor_wall_and_ceiling() {
        let b = player_box();
        let id = DQuat::IDENTITY;
        assert_eq!(rest_on_surface(DVec3::new(3.0, 5.0, 0.0), DVec3::Z, &b, id, 16.0), DVec3::new(0.0, 0.0, 0.0));
        assert_eq!(
            rest_on_surface(DVec3::new(20.0, 0.0, 128.0), DVec3::NEG_Z, &b, id, 16.0),
            DVec3::new(16.0, 0.0, 56.0)
        );
        assert_eq!(rest_on_surface(DVec3::new(64.0, 7.0, 40.0), DVec3::X, &b, id, 16.0), DVec3::new(80.0, 0.0, 48.0));
        assert_eq!(
            rest_on_surface(DVec3::new(3.0, 256.0, 5.0), DVec3::NEG_Y, &b, id, 0.0),
            DVec3::new(3.0, 240.0, 5.0)
        );
        let cube = Aabb::new(DVec3::splat(-8.0), DVec3::splat(8.0));
        assert_eq!(rest_on_surface(DVec3::ZERO, DVec3::Z, &cube, id, 16.0).z, 8.0);
        let (q, angles) = align_to_normal(DVec3::X);
        assert!((q * DVec3::Z - DVec3::X).length() < 1e-9);
        assert_eq!(angles.x.abs(), 90.0);
        let o = rest_on_surface(DVec3::new(64.0, 0.0, 32.0), DVec3::X, &b, q, 16.0);
        assert_eq!(o, DVec3::new(64.0, 0.0, 32.0), "mins.z = 0 along the normal");
    }

    #[test]
    fn resting_offsets_are_rounded() {
        let id = DQuat::IDENTITY;
        let player = Aabb::new(DVec3::new(-13.0, -10.0, 0.113_388_687_372_207_64), DVec3::new(13.0, 10.0, 73.0));
        let crate_box = Aabb::new(DVec3::splat(-20.010_789_871_215_82), DVec3::splat(20.010_789_871_215_82));
        assert_eq!(
            rest_on_surface(DVec3::new(3.0, -290.0, 0.0), DVec3::Z, &player, id, 16.0),
            DVec3::new(0.0, -288.0, 0.0)
        );
        assert_eq!(
            rest_on_surface(DVec3::new(-220.0, 81.0, 0.0), DVec3::Z, &crate_box, id, 16.0),
            DVec3::new(-224.0, 80.0, 20.0)
        );
        assert_eq!(rest_on_surface(DVec3::ZERO, DVec3::Z, &crate_box, id, 0.25).z, 20.0);
        let half = Aabb::new(DVec3::splat(-0.4), DVec3::splat(0.4));
        assert_eq!(rest_on_surface(DVec3::ZERO, DVec3::Z, &half, id, 0.5).z, 0.5);
        let o = rest_on_surface(DVec3::new(1.234_567_891, 2.0, 0.0), DVec3::Z, &crate_box, id, 0.0);
        assert_eq!(o, DVec3::new(1.235, 2.0, 20.01));
        let n = DVec3::new(0.0, 1.0, 1.0).normalize();
        let o = rest_on_surface(DVec3::new(5.0, 3.0, -3.0), n, &crate_box, id, 16.0);
        for c in o.to_array() {
            assert_eq!(c, (c * 1000.0).round() / 1000.0, "{o:?}");
        }
    }

    #[test]
    fn studio_entities_rest_on_the_floor_with_clean_origins() {
        let mut state = fgd_state_with_models(vec![
            (
                "models/editor/playerstart.mdl".to_owned(),
                DVec3::new(-13.0, -10.0, 0.113_388_687_372_207_64),
                DVec3::new(13.0, 10.0, 73.0),
            ),
            (
                "models/props_junk/wood_crate001a.mdl".to_owned(),
                DVec3::splat(-20.010_789_871_215_82),
                DVec3::splat(20.010_789_871_215_82),
            ),
        ]);
        let mut tool = EntityTool::new();
        let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 512.0));
        let p = view.world_to_screen(DVec3::new(37.0, -41.0, 0.0)).expect("screen");
        state.current_entity_class = "info_player_model".to_owned();
        tool.handle_mouse(&mut state, &view, &Mouse::click(p));
        let sel = state.doc.selected_entity_ids();
        let e = &state.doc.scene().entity(sel[0]).expect("entity").entity;
        assert_eq!(e.get("origin"), Some("32 -48 0"));
        state.current_entity_class = "prop_physics".to_owned();
        state.current_model = Some("models/props_junk/wood_crate001a.mdl".to_owned());
        tool.handle_mouse(&mut state, &view, &Mouse::click(p));
        let sel = state.doc.selected_entity_ids();
        let e = &state.doc.scene().entity(sel[0]).expect("entity").entity;
        assert_eq!(e.get("origin"), Some("32 -48 20"));
    }

    #[test]
    fn places_on_the_floor_in_3d_with_fgd_defaults() {
        let mut state = fgd_state();
        state.current_entity_class = "info_player_start".to_owned();
        let mut tool = EntityTool::new();
        let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 512.0));
        let p = view.world_to_screen(DVec3::new(37.0, -41.0, 0.0)).expect("screen");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(p));
        let ghost = tool.ghost.expect("ghost");
        assert_eq!(ghost.origin, DVec3::new(32.0, -48.0, 0.0));
        let (undo, _) = state.doc.history_len();
        tool.handle_mouse(&mut state, &view, &Mouse::click(p));
        assert_eq!(state.doc.history_len().0, undo + 1);
        let sel = state.doc.selected_entity_ids();
        assert_eq!(sel.len(), 1);
        let e = &state.doc.scene().entity(sel[0]).expect("entity").entity;
        assert_eq!(e.classname, "info_player_start");
        assert_eq!(e.origin(), Some(DVec3::new(32.0, -48.0, 0.0)));
        assert_eq!(e.get("angles"), Some("0 0 0"));
        assert_eq!(tool.recent_classes(), ["info_player_start"]);
        let top = info_2d(ViewKind::Front, 1.0);
        let q = top.world_to_screen(DVec3::new(0.0, 100.0, 70.0)).expect("q");
        tool.handle_mouse(&mut state, &top, &Mouse::click(q));
        let sel = state.doc.selected_entity_ids();
        let e = &state.doc.scene().entity(sel[0]).expect("entity").entity;
        assert_eq!(e.origin(), Some(DVec3::new(32.0, 96.0, 64.0)));
    }

    #[test]
    fn defaults_spawnflags_and_props() {
        let mut state = fgd_state();
        state.current_entity_class = "prop_physics".to_owned();
        state.current_model = Some("models/props_c17/oildrum001.mdl".to_owned());
        let mut tool = EntityTool::new();
        let id = tool.place(&mut state, DVec3::new(0.0, 0.0, 16.0), None, false).expect("placed");
        let e = &state.doc.scene().entity(id).expect("entity").entity;
        assert_eq!(e.get("model"), Some("models/props_c17/oildrum001.mdl"));
        assert_eq!(e.get("spawnflags"), Some("1"));
        assert_eq!(e.get("skin"), Some("0"));
        let local = EntityTool::local_bounds(&mut state, "prop_physics");
        assert_eq!(local.min, DVec3::splat(-16.0));
        state.current_entity_class = "light".to_owned();
        let id = tool.place(&mut state, DVec3::new(0.0, 0.0, 64.0), None, false).expect("light");
        let e = &state.doc.scene().entity(id).expect("entity").entity;
        assert_eq!(e.get("_light"), Some("255 255 255 200"));
        assert_eq!(e.get("spawnflags"), Some("0"));
        assert_eq!(tool.recent_classes(), ["light", "prop_physics"]);
    }

    #[test]
    fn props_get_the_skin_chosen_in_the_model_browser() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = fgd_state();
        state.current_entity_class = "prop_physics".to_owned();
        state.current_model = Some("models/props_c17/oildrum001.mdl".to_owned());
        state.current_model_skin = 3;
        let mut tool = EntityTool::new();
        let id = tool.place(&mut state, DVec3::ZERO, None, false).expect("placed");
        let e = &state.doc.scene().entity(id).expect("entity").entity;
        assert_eq!(e.get("model"), Some("models/props_c17/oildrum001.mdl"));
        assert_eq!(e.get("skin"), Some("3"));
        state.current_model_skin = 0;
        let id = tool.place(&mut state, DVec3::ZERO, None, false).expect("placed");
        assert_eq!(state.doc.scene().entity(id).expect("entity").entity.get("skin"), Some("0"));
        state.current_model_skin = 2;
        state.current_model = None;
        let id = tool.place(&mut state, DVec3::ZERO, None, false).expect("placed");
        assert_eq!(state.doc.scene().entity(id).expect("entity").entity.get("skin"), Some("0"));
        state.current_model = Some("models/props_c17/oildrum001.mdl".to_owned());
        state.current_entity_class = "light".to_owned();
        let id = tool.place(&mut state, DVec3::ZERO, None, false).expect("light");
        assert_eq!(state.doc.scene().entity(id).expect("entity").entity.get("skin"), None);
        state.current_entity_class = "prop_physics".to_owned();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let nodes = options_frame(&ctx, &mut tool, &mut state, Vec::new());
        let skin = tf!("create.entity.skin", 2);
        assert!(nodes.iter().any(|(t, _)| *t == skin), "{nodes:?}");
        state.current_model_skin = 0;
        let nodes = options_frame(&ctx, &mut tool, &mut state, Vec::new());
        assert!(!nodes.iter().any(|(t, _)| t.starts_with("Skin")), "{nodes:?}");
    }

    #[test]
    fn models_button_shows_the_model_browser() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = fgd_state();
        state.current_entity_class = "prop_physics".to_owned();
        let mut tool = EntityTool::new();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let nodes = options_frame(&ctx, &mut tool, &mut state, Vec::new());
        let labels = hp_app::i18n::Lang::ALL.map(|l| hp_app::i18n::tr_in("create.entity.pick_model", l));
        let rect = nodes
            .iter()
            .find(|(t, _)| labels.contains(&t.as_str()))
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{nodes:?}"));
        let pos = rect.center();
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        options_frame(&ctx, &mut tool, &mut state, vec![egui::Event::PointerMoved(pos)]);
        assert!(state.requested_actions.is_empty());
        options_frame(&ctx, &mut tool, &mut state, vec![press(true)]);
        options_frame(&ctx, &mut tool, &mut state, vec![press(false)]);
        assert_eq!(state.requested_actions, vec![Action::ShowModelBrowser]);
    }

    #[test]
    fn model_hulls_are_preferred_to_mesh_bounds() {
        let hull = Aabb::new(DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 72.0));
        let start = player_start_model();
        assert_eq!(model_box(&start), Some(hull));
        let mesh = ModelInfo { hull_min: None, hull_max: None, ..start.clone() };
        assert_eq!(model_box(&mesh), Some(Aabb::new(start.bounds_min, start.bounds_max)));
        let flat = ModelInfo { hull_min: Some(DVec3::ZERO), hull_max: Some(DVec3::ZERO), ..start.clone() };
        assert_eq!(model_box(&flat), model_box(&mesh));
        let nan = ModelInfo { hull_min: Some(DVec3::NAN), ..start.clone() };
        assert_eq!(model_box(&nan), model_box(&mesh));
        assert_eq!(model_box(&ModelInfo::default()), None);

        let crate_model = ModelInfo {
            path: "models/props_junk/wood_crate001a.mdl".to_owned(),
            bounds_min: DVec3::splat(-20.010_789_871_215_82),
            bounds_max: DVec3::splat(20.010_789_871_215_82),
            hull_min: Some(DVec3::splat(-24.0)),
            hull_max: Some(DVec3::splat(24.0)),
            ..Default::default()
        };
        let mut state = fgd_state_with_model_infos(vec![start, crate_model]);
        assert_eq!(EntityTool::local_bounds(&mut state, "info_player_model"), hull);
        assert_eq!(EntityTool::local_bounds(&mut state, "info_player_start"), hull);
        state.current_model = Some("models/props_junk/wood_crate001a.mdl".to_owned());
        let crate_hull = Aabb::new(DVec3::splat(-24.0), DVec3::splat(24.0));
        assert_eq!(EntityTool::local_bounds(&mut state, "prop_physics"), crate_hull);
        state.current_model = None;
        assert_eq!(EntityTool::local_bounds(&mut state, "prop_physics").size(), DVec3::splat(16.0));

        let mut tool = EntityTool::new();
        state.current_entity_class = "info_player_model".to_owned();
        let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0);
        tool.handle_mouse(&mut state, &view, &Mouse::hover(view.rect.center()));
        let ghost = tool.ghost.expect("ghost");
        assert_eq!(ghost.local, hull);
        assert_eq!(ghost.origin.y, 240.0, "{:?}", ghost.origin);
        assert_eq!(ghost.world_bounds().max.y, 256.0);
        let down = info_3d_looking_down(DVec3::new(0.0, 0.0, 512.0));
        let p = down.world_to_screen(DVec3::new(37.0, -41.0, 0.0)).expect("screen");
        tool.handle_mouse(&mut state, &down, &Mouse::click(p));
        let sel = state.doc.selected_entity_ids();
        let e = &state.doc.scene().entity(sel[0]).expect("entity").entity;
        assert_eq!(e.get("origin"), Some("32 -48 0"));
        let top = info_2d(ViewKind::Top, 1.0);
        let q = top.world_to_screen(DVec3::new(100.0, 100.0, 0.0)).expect("q");
        tool.handle_mouse(&mut state, &top, &Mouse::hover(q));
        assert_eq!(tool.ghost.expect("2d ghost").world_bounds().size(), DVec3::new(32.0, 32.0, 72.0));
    }

    #[test]
    fn align_option_sets_prop_angles_on_walls() {
        let mut state = fgd_state();
        state.current_entity_class = "prop_physics".to_owned();
        state.current_model = Some("models/props_c17/oildrum001.mdl".to_owned());
        let mut tool = EntityTool::new();
        tool.align_to_normal = true;
        let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0);
        let p = view.rect.center();
        tool.handle_mouse(&mut state, &view, &Mouse::click(p));
        let sel = state.doc.selected_entity_ids();
        let e = &state.doc.scene().entity(sel[0]).expect("entity").entity;
        let o = e.origin().expect("origin");
        assert!((o.y - 240.0).abs() < 1e-6, "{o:?}");
        let a = e.angles().expect("angles");
        let up = hp_geom::angles_to_vectors(a).2;
        assert!((up - DVec3::NEG_Y).length() < 1e-6, "{a:?} {up:?}");
    }

    #[test]
    fn entities_rest_on_model_surfaces() {
        let mut state = fgd_state();
        let mut tool = EntityTool::new();
        let plate = vec![
            [glam::Vec3::new(-32.0, -32.0, 6.0), glam::Vec3::new(0.0, -32.0, 6.0), glam::Vec3::new(0.0, 32.0, 6.0)],
            [glam::Vec3::new(-32.0, -32.0, 6.0), glam::Vec3::new(0.0, 32.0, 6.0), glam::Vec3::new(-32.0, 32.0, 6.0)],
        ];
        tool.surfaces.insert("models/test/table.mdl", crate::model_bvh::TriBvh::build(plate));
        let kv = vec![("model".to_owned(), "models/test/table.mdl".to_owned())];
        state.doc.create_point_entity("prop_static", DVec3::new(-100.0, -100.0, 0.0), &kv).expect("table");
        state.current_entity_class = "info_player_start".to_owned();
        let view = info_3d_looking_down(DVec3::new(-100.0, -100.0, 512.0));
        let p = view.world_to_screen(DVec3::new(-104.0, -100.0, 0.0)).expect("screen");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(p));
        let g = tool.ghost.expect("ghost");
        assert_eq!(g.origin.z, 6.0, "{:?}", g.origin);
        let q = view.world_to_screen(DVec3::new(-96.0, -100.0, 0.0)).expect("screen");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(q));
        assert_eq!(tool.ghost.expect("ghost").origin.z, 0.0);
        tool.handle_mouse(&mut state, &view, &Mouse::hover(p));
        assert_eq!(tool.ghost.expect("ghost").origin.z, 6.0);
    }

    #[test]
    fn shift_keeps_the_selection() {
        let mut state = settle();
        state.doc.set_selection(&[hp_editor::ObjRef::Entity(5)]);
        state.current_entity_class = "info_target".to_owned();
        let mut tool = EntityTool::new();
        let view = info_2d(ViewKind::Top, 1.0);
        let p = view.world_to_screen(DVec3::new(64.0, 64.0, 0.0)).expect("p");
        let (undo, _) = state.doc.history_len();
        let entities = state.doc.stats().entities;
        tool.handle_mouse(&mut state, &view, &Mouse::click(p).with_modifiers(Modifiers::SHIFT));
        assert_eq!(state.doc.stats().entities, entities + 1);
        assert_eq!(state.doc.selected_objects(), vec![hp_editor::ObjRef::Entity(5)]);
        assert_eq!(state.doc.history_len().0, undo + 1);
        let placed =
            state.doc.scene().entities().values().find(|n| n.entity.classname == "info_target").expect("placed");
        assert_eq!(placed.entity.origin(), Some(DVec3::new(64.0, 64.0, 96.0)));
        assert!(state.doc.undo());
        assert_eq!(state.doc.stats().entities, entities);
    }

    #[test]
    fn a_shaky_click_still_places() {
        let mut state = settle();
        state.current_entity_class = "info_target".to_owned();
        let mut tool = EntityTool::new();
        let view = info_2d(ViewKind::Top, 1.0);
        let p = view.world_to_screen(DVec3::new(32.0, 32.0, 0.0)).expect("p");
        let entities = state.doc.stats().entities;
        let moved = p + egui::vec2(9.0, 0.0);
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(p, moved));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(p, moved));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(p, moved));
        assert_eq!(state.doc.stats().entities, entities + 1, "placed once");
    }

    #[test]
    fn rejects_bad_class_names() {
        let mut state = settle();
        state.current_entity_class = "bad class".to_owned();
        let mut tool = EntityTool::new();
        assert!(tool.place(&mut state, DVec3::ZERO, None, false).is_none());
        assert!(!state.console.is_empty());
    }

    #[test]
    fn class_picker_popup_and_fallback_field() {
        let mut state = fgd_state();
        let mut tool = EntityTool::new();
        tool.push_recent("light");
        tool.picker.open = true;
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
        assert!(tool.picker.open, "the popup stays open without clicks");
        assert_eq!(tool.picker.filtered_len(), 6, "every point class");
        *tool.picker.search_mut() = "info_".to_owned();
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
        assert_eq!(tool.picker.filtered_len(), 3, "info_player_start, info_player_model and info_overlay");
        state.current_entity_class = "prop_physics".to_owned();
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
        let mut plain = settle();
        hp_app::testing::run_ui(&mut plain, |ui, state| tool.options_ui(ui, state));
        tool.activate(&mut plain);
        assert!(!plain.current_entity_class.is_empty());
    }
}
